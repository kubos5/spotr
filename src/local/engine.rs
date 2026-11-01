//! Audio engine for local files: Symphonia decodes, cpal plays.
//!
//! A decoder thread keeps a short ring of stereo frames (at the file's sample
//! rate) filled; the output callback resamples it to the device rate, applies
//! speed and volume, and feeds the visualizer tap — the same `AudioShared` the
//! built-in Spotify player uses, so speed and the real visualizer just work.

use std::path::PathBuf;
use std::sync::Arc;

use crate::audio::AudioShared;

#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "local-files"), allow(dead_code))]
pub enum EngineEvent {
    /// A file opened and is ready (or playing). `duration_ms` is 0 when unknown.
    Loaded { load_id: u64, duration_ms: u64 },
    /// The current file played to its end.
    Ended { load_id: u64 },
    Error { load_id: u64, msg: String },
}

#[cfg_attr(not(feature = "local-files"), allow(dead_code))]
enum Cmd {
    Load { path: PathBuf, start_ms: u64, load_id: u64 },
    Seek(u64),
    Stop,
    Wake,
    Shutdown,
}

#[cfg(feature = "local-files")]
pub use imp::*;

#[cfg(not(feature = "local-files"))]
pub use stub::*;

#[cfg(not(feature = "local-files"))]
mod stub {
    use super::*;

    pub const AVAILABLE: bool = false;

    pub struct Engine;

    impl Engine {
        pub fn start(_audio: Arc<AudioShared>, _events: impl Fn(EngineEvent) + Send + 'static) -> anyhow::Result<Engine> {
            anyhow::bail!("Spotr was built without the `local-files` feature")
        }
        pub fn load(&self, _path: PathBuf, _start_ms: u64, _play: bool) -> u64 {
            0
        }
        pub fn play(&self) {}
        pub fn pause(&self) {}
        pub fn seek(&self, _ms: u64) {}
        pub fn stop(&self) {}
        pub fn set_volume(&self, _percent: u32) {}
        pub fn position_ms(&self) -> u64 {
            0
        }
        pub fn shutdown(&self) {}
    }

    pub fn output_devices() -> Vec<String> {
        Vec::new()
    }
}

#[cfg(feature = "local-files")]
mod imp {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

    use anyhow::{Result, anyhow};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use symphonia::core::audio::{SampleBuffer, SignalSpec};
    use symphonia::core::codecs::{CODEC_TYPE_NULL, Decoder, DecoderOptions};
    use symphonia::core::errors::Error as SymError;
    use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    use symphonia::core::units::{Time, TimeBase};

    use super::*;

    pub const AVAILABLE: bool = true;

    /// About a quarter second of audio waits in the ring: pausing and seeking
    /// feel instant, and there's still slack for a busy disk.
    const RING_SECONDS: f64 = 0.25;

    struct Shared {
        /// Interleaved stereo frames at `rate`.
        ring: Mutex<VecDeque<f32>>,
        cv: Condvar,
        rate: AtomicU32,
        /// Source frames played since `base_ms` (updated under the ring lock).
        played: AtomicU64,
        base_ms: AtomicU64,
        playing: AtomicBool,
        gain_bits: AtomicU32,
        load_id: AtomicU64,
        audio: Arc<AudioShared>,
    }

    impl Shared {
        fn position_ms(&self) -> u64 {
            let rate = self.rate.load(Ordering::Relaxed).max(1) as u64;
            self.base_ms.load(Ordering::Relaxed) + self.played.load(Ordering::Relaxed) * 1000 / rate
        }

        /// Empty the ring and restart the clock at `ms`.
        fn reset(&self, ms: u64) {
            let mut q = self.ring.lock().unwrap_or_else(|e| e.into_inner());
            q.clear();
            self.base_ms.store(ms, Ordering::Relaxed);
            self.played.store(0, Ordering::Relaxed);
            drop(q);
            self.cv.notify_all();
        }
    }

    pub struct Engine {
        tx: Sender<Cmd>,
        shared: Arc<Shared>,
    }

    impl Engine {
        pub fn start(audio: Arc<AudioShared>, events: impl Fn(EngineEvent) + Send + 'static) -> Result<Engine> {
            let shared = Arc::new(Shared {
                ring: Mutex::new(VecDeque::new()),
                cv: Condvar::new(),
                rate: AtomicU32::new(44_100),
                played: AtomicU64::new(0),
                base_ms: AtomicU64::new(0),
                playing: AtomicBool::new(false),
                gain_bits: AtomicU32::new(1.0f32.to_bits()),
                load_id: AtomicU64::new(0),
                audio,
            });
            let (tx, rx) = mpsc::channel();
            let s = shared.clone();
            std::thread::Builder::new()
                .name("spotr-local-audio".into())
                .spawn(move || Worker::new(s, rx, Box::new(events)).run())
                .map_err(|e| anyhow!("couldn't start the audio thread: {e}"))?;
            Ok(Engine { tx, shared })
        }

        /// Open `path` at `start_ms`. Returns the load's generation, which tags its events.
        pub fn load(&self, path: PathBuf, start_ms: u64, play: bool) -> u64 {
            let load_id = self.shared.load_id.fetch_add(1, Ordering::Relaxed) + 1;
            self.shared.playing.store(play, Ordering::Relaxed);
            self.shared.audio.playing.store(play, Ordering::Relaxed);
            self.shared.reset(start_ms);
            let _ = self.tx.send(Cmd::Load { path, start_ms, load_id });
            load_id
        }

        pub fn play(&self) {
            self.shared.playing.store(true, Ordering::Relaxed);
            self.shared.audio.playing.store(true, Ordering::Relaxed);
            let _ = self.tx.send(Cmd::Wake);
        }

        pub fn pause(&self) {
            self.shared.playing.store(false, Ordering::Relaxed);
            self.shared.audio.playing.store(false, Ordering::Relaxed);
        }

        pub fn seek(&self, ms: u64) {
            self.shared.reset(ms);
            let _ = self.tx.send(Cmd::Seek(ms));
        }

        pub fn stop(&self) {
            self.pause();
            self.shared.reset(0);
            let _ = self.tx.send(Cmd::Stop);
        }

        /// 0–100, on a curve that sounds even across the range.
        pub fn set_volume(&self, percent: u32) {
            let v = percent.min(100) as f32 / 100.0;
            self.shared.gain_bits.store((v * v).to_bits(), Ordering::Relaxed);
        }

        pub fn position_ms(&self) -> u64 {
            self.shared.position_ms()
        }

        pub fn shutdown(&self) {
            self.pause();
            let _ = self.tx.send(Cmd::Shutdown);
        }
    }

    impl Drop for Engine {
        fn drop(&mut self) {
            self.shutdown();
        }
    }

    pub fn output_devices() -> Vec<String> {
        cpal::default_host()
            .output_devices()
            .map(|d| d.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default()
    }

    // ---------------------------------------------------------------- decoder

    struct Open {
        format: Box<dyn FormatReader>,
        decoder: Box<dyn Decoder>,
        track_id: u32,
        time_base: Option<TimeBase>,
        rate: u32,
        load_id: u64,
        /// Frames to drop after an accurate seek lands slightly early.
        skip: u64,
        eof: bool,
        ended_sent: bool,
        buf: Option<SampleBuffer<f32>>,
    }

    struct Worker {
        shared: Arc<Shared>,
        rx: Receiver<Cmd>,
        events: Box<dyn Fn(EngineEvent) + Send>,
        open: Option<Open>,
        stream: Option<cpal::Stream>,
    }

    impl Worker {
        fn new(shared: Arc<Shared>, rx: Receiver<Cmd>, events: Box<dyn Fn(EngineEvent) + Send>) -> Self {
            Self { shared, rx, events, open: None, stream: None }
        }

        fn run(mut self) {
            loop {
                let busy = self.open.as_ref().is_some_and(|o| !o.eof) && self.shared.playing.load(Ordering::Relaxed);
                let cmd = if busy {
                    match self.rx.try_recv() {
                        Ok(c) => Some(c),
                        Err(mpsc::TryRecvError::Empty) => None,
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                } else {
                    match self.rx.recv_timeout(Duration::from_millis(40)) {
                        Ok(c) => Some(c),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                };
                match cmd {
                    Some(Cmd::Shutdown) => return,
                    Some(Cmd::Load { path, start_ms, load_id }) => self.load(path, start_ms, load_id),
                    Some(Cmd::Seek(ms)) => self.seek(ms),
                    Some(Cmd::Stop) => self.open = None,
                    Some(Cmd::Wake) | None => {}
                }
                if self.shared.audio.reopen.swap(false, Ordering::Relaxed) {
                    self.stream = None;
                }
                if self.open.is_some() && self.stream.is_none() {
                    match open_stream(self.shared.clone()) {
                        Ok(s) => self.stream = Some(s),
                        Err(e) => {
                            let load_id = self.open.as_ref().map(|o| o.load_id).unwrap_or(0);
                            self.open = None;
                            (self.events)(EngineEvent::Error { load_id, msg: format!("audio output: {e}") });
                            continue;
                        }
                    }
                }
                self.fill();
            }
        }

        fn load(&mut self, path: PathBuf, start_ms: u64, load_id: u64) {
            // A newer load already replaced this one.
            if load_id != self.shared.load_id.load(Ordering::Relaxed) {
                return;
            }
            self.open = None;
            // Drop anything the previous file pushed after the UI asked for this one.
            self.shared.reset(start_ms);
            match open_file(&path) {
                Ok((format, decoder, track_id, time_base, rate, duration_ms)) => {
                    self.shared.rate.store(rate, Ordering::Relaxed);
                    self.open = Some(Open { format, decoder, track_id, time_base, rate, load_id, skip: 0, eof: false, ended_sent: false, buf: None });
                    if start_ms > 0 {
                        self.seek(start_ms);
                    }
                    (self.events)(EngineEvent::Loaded { load_id, duration_ms });
                }
                Err(e) => (self.events)(EngineEvent::Error { load_id, msg: e.to_string() }),
            }
        }

        fn seek(&mut self, ms: u64) {
            let Some(o) = self.open.as_mut() else { return };
            self.shared.reset(ms);
            let to = SeekTo::Time { time: Time::from(ms as f64 / 1000.0), track_id: Some(o.track_id) };
            match o.format.seek(SeekMode::Accurate, to) {
                Ok(seeked) => {
                    o.decoder.reset();
                    o.eof = false;
                    o.ended_sent = false;
                    let early = seeked.required_ts.saturating_sub(seeked.actual_ts);
                    o.skip = match o.time_base {
                        Some(tb) => {
                            let t = tb.calc_time(early);
                            ((t.seconds as f64 + t.frac) * o.rate as f64) as u64
                        }
                        None => early,
                    };
                }
                Err(e) => {
                    // Past the end (or unseekable): treat as finished.
                    log::debug!("seek failed: {e}");
                    o.eof = true;
                }
            }
        }

        /// Decode until the ring holds enough audio, then wait for the output to drain it.
        fn fill(&mut self) {
            let shared = self.shared.clone();
            let Some(o) = self.open.as_mut() else { return };
            if o.eof {
                // Ends once the output has played everything (not while paused on the last bit).
                let left = shared.ring.lock().map(|q| q.len()).unwrap_or(0);
                if !o.ended_sent && left <= 4 && shared.playing.load(Ordering::Relaxed) {
                    o.ended_sent = true;
                    (self.events)(EngineEvent::Ended { load_id: o.load_id });
                }
                return;
            }
            if !shared.playing.load(Ordering::Relaxed) {
                return;
            }
            let cap = (o.rate as f64 * 2.0 * RING_SECONDS) as usize;
            {
                let q = shared.ring.lock().unwrap_or_else(|e| e.into_inner());
                if q.len() >= cap {
                    let _ = shared.cv.wait_timeout(q, Duration::from_millis(30));
                    return;
                }
            }
            let packet = match o.format.next_packet() {
                Ok(p) => p,
                Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    o.eof = true;
                    return;
                }
                Err(SymError::ResetRequired) => {
                    o.decoder.reset();
                    return;
                }
                Err(e) => {
                    log::warn!("local playback stopped reading: {e}");
                    o.eof = true;
                    return;
                }
            };
            if packet.track_id() != o.track_id {
                return;
            }
            let decoded = match o.decoder.decode(&packet) {
                Ok(d) => d,
                // A damaged frame: skip it, like other players do.
                Err(SymError::DecodeError(e)) => {
                    log::debug!("skipping a bad frame: {e}");
                    return;
                }
                Err(e) => {
                    log::warn!("local playback decode error: {e}");
                    o.eof = true;
                    return;
                }
            };
            let spec: SignalSpec = *decoded.spec();
            if spec.rate != o.rate && spec.rate > 0 {
                o.rate = spec.rate;
                shared.rate.store(spec.rate, Ordering::Relaxed);
            }
            let frames = decoded.capacity() as u64;
            let buf = match &mut o.buf {
                Some(b) if b.capacity() as u64 >= frames * spec.channels.count() as u64 => b,
                slot => slot.insert(SampleBuffer::<f32>::new(frames, spec)),
            };
            buf.copy_interleaved_ref(decoded);
            let ch = spec.channels.count().max(1);
            let samples = buf.samples();
            let mut start = 0usize;
            if o.skip > 0 {
                let n = (samples.len() / ch) as u64;
                let drop_n = o.skip.min(n);
                o.skip -= drop_n;
                start = drop_n as usize * ch;
            }
            let mut q = shared.ring.lock().unwrap_or_else(|e| e.into_inner());
            for f in samples[start..].chunks(ch) {
                let (l, r) = match f {
                    [m] => (*m, *m),
                    [l, r, ..] => (*l, *r),
                    _ => (0.0, 0.0),
                };
                q.push_back(l);
                q.push_back(r);
            }
        }
    }

    type Opened = (Box<dyn FormatReader>, Box<dyn Decoder>, u32, Option<TimeBase>, u32, u64);

    fn open_file(path: &std::path::Path) -> Result<Opened> {
        let file = std::fs::File::open(path).map_err(|e| anyhow!("couldn't open {}: {e}", path.display()))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
            .map_err(|e| anyhow!("unsupported file ({e})"))?;
        let format = probed.format;
        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| anyhow!("no audio in this file"))?;
        let track_id = track.id;
        let params = track.codec_params.clone();
        let decoder = symphonia::default::get_codecs()
            .make(&params, &DecoderOptions::default())
            .map_err(|e| anyhow!("can't decode this file ({e})"))?;
        let rate = params.sample_rate.unwrap_or(44_100);
        let duration_ms = match (params.n_frames, params.time_base) {
            (Some(n), Some(tb)) => {
                let t = tb.calc_time(n);
                ((t.seconds as f64 + t.frac) * 1000.0) as u64
            }
            (Some(n), None) => n * 1000 / rate.max(1) as u64,
            _ => 0,
        };
        Ok((format, decoder, track_id, params.time_base, rate, duration_ms))
    }

    // ----------------------------------------------------------------- output

    fn open_stream(shared: Arc<Shared>) -> Result<cpal::Stream> {
        let host = cpal::default_host();
        let wanted = shared.audio.device.lock().map(|d| d.clone()).unwrap_or_default();
        let device = if wanted.is_empty() {
            None
        } else {
            host.output_devices().ok().and_then(|mut it| it.find(|d| d.name().map(|n| n == wanted).unwrap_or(false)))
        };
        let device = device.or_else(|| host.default_output_device()).ok_or_else(|| anyhow!("no audio output device"))?;
        let supported = device.default_output_config().map_err(|e| anyhow!("{e}"))?;
        let config: cpal::StreamConfig = supported.config();
        shared.audio.sample_rate.store(config.sample_rate.0, Ordering::Relaxed);
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => build::<f32>(&device, &config, shared),
            cpal::SampleFormat::I16 => build::<i16>(&device, &config, shared),
            cpal::SampleFormat::U16 => build::<u16>(&device, &config, shared),
            cpal::SampleFormat::I32 => build::<i32>(&device, &config, shared),
            other => return Err(anyhow!("unsupported sample format {other:?}")),
        }?;
        stream.play().map_err(|e| anyhow!("{e}"))?;
        Ok(stream)
    }

    fn build<T>(device: &cpal::Device, config: &cpal::StreamConfig, shared: Arc<Shared>) -> Result<cpal::Stream>
    where
        T: cpal::SizedSample + cpal::FromSample<f32>,
    {
        let channels = config.channels as usize;
        let out_rate = config.sample_rate.0 as f64;
        let mut pos = 0.0f64;
        let mut mono: Vec<f32> = Vec::with_capacity(4096);
        device
            .build_output_stream(
                config,
                move |data: &mut [T], _| {
                    let silence = |data: &mut [T]| data.iter_mut().for_each(|s| *s = T::from_sample(0.0f32));
                    if !shared.playing.load(Ordering::Relaxed) {
                        silence(data);
                        return;
                    }
                    let gain = f32::from_bits(shared.gain_bits.load(Ordering::Relaxed));
                    let step = shared.rate.load(Ordering::Relaxed) as f64 / out_rate * shared.audio.speed() as f64;
                    let Ok(mut q) = shared.ring.lock() else {
                        silence(data);
                        return;
                    };
                    mono.clear();
                    let avail = q.len() / 2;
                    for frame in data.chunks_mut(channels) {
                        let i = pos.floor() as usize;
                        let (l, r) = if i + 1 < avail {
                            let f = (pos - i as f64) as f32;
                            let l = q[i * 2] * (1.0 - f) + q[i * 2 + 2] * f;
                            let r = q[i * 2 + 1] * (1.0 - f) + q[i * 2 + 3] * f;
                            pos += step;
                            (l * gain, r * gain)
                        } else {
                            (0.0, 0.0)
                        };
                        mono.push((l + r) * 0.5);
                        match channels {
                            1 => frame[0] = T::from_sample((l + r) * 0.5),
                            _ => {
                                frame[0] = T::from_sample(l);
                                frame[1] = T::from_sample(r);
                                for s in frame.iter_mut().skip(2) {
                                    *s = T::from_sample(0.0f32);
                                }
                            }
                        }
                    }
                    let consumed = (pos.floor() as usize).min(avail);
                    q.drain(..consumed * 2);
                    pos -= consumed as f64;
                    if avail == 0 {
                        pos = 0.0;
                    }
                    shared.played.fetch_add(consumed as u64, Ordering::Relaxed);
                    drop(q);
                    shared.cv.notify_all();
                    shared.audio.push_tap(&mono);
                },
                |e| log::warn!("local audio stream error: {e}"),
                None,
            )
            .map_err(|e| anyhow!("{e}"))
    }
}
