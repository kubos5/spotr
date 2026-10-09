//! Built-in Spotify Connect player powered by librespot.
//!
//! When enabled, Spotr registers itself as a Connect device. Audio is played
//! through our own cpal sink, which lets us (1) feed the real-time visualizer
//! and (2) change playback speed by resampling.

use std::sync::Arc;

use crate::audio::AudioShared;

#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "local-player"), allow(dead_code))]
pub enum LocalEvent {
    Connected { device_name: String },
    /// No stored credentials and the Web API token was refused.
    NeedsLogin,
    Playing { uri: String, position_ms: u32 },
    Paused { uri: String, position_ms: u32 },
    Position { uri: String, position_ms: u32 },
    Stopped,
    TrackEnded,
    TrackChanged,
}

#[cfg_attr(not(feature = "local-player"), allow(dead_code))]
pub struct LocalOptions {
    pub device_name: String,
    pub device_id: String,
    pub bitrate: u16,
    pub normalisation: bool,
    pub cache_dir: std::path::PathBuf,
    pub audio: Arc<AudioShared>,
}

/// Client ID + redirect used by librespot's own OAuth login.
pub const LIBRESPOT_CLIENT_ID: &str = "65b708073fc0480ea92a077233ca87bd";
pub const LIBRESPOT_REDIRECT: &str = "http://127.0.0.1:8898/login";
pub const LIBRESPOT_SCOPES: &[&str] = &["streaming", "user-read-playback-state", "user-modify-playback-state", "user-read-private"];

#[cfg(feature = "local-player")]
pub use imp::*;

#[cfg(not(feature = "local-player"))]
pub use stub::*;

#[cfg(not(feature = "local-player"))]
mod stub {
    use super::*;

    pub struct LocalPlayer;

    pub const AVAILABLE: bool = false;

    impl LocalPlayer {
        pub fn shutdown(&self) {}
        pub fn activate(&self) {}
    }

    pub async fn start(
        _o: LocalOptions,
        _token: Option<String>,
        _tx: tokio::sync::mpsc::UnboundedSender<crate::app::AppEvent>,
    ) -> anyhow::Result<LocalPlayer> {
        anyhow::bail!("Spotr was built without the `local-player` feature")
    }

    pub fn output_devices() -> Vec<String> {
        Vec::new()
    }

    pub fn has_cached_credentials(_dir: &std::path::Path) -> bool {
        false
    }
}

#[cfg(feature = "local-player")]
mod imp {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::Ordering;
    use std::sync::{Condvar, Mutex};
    use std::time::Duration;

    use anyhow::{Result, anyhow};
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use librespot_connect::{ConnectConfig, Spirc};
    use librespot_core::authentication::Credentials;
    use librespot_core::cache::Cache;
    use librespot_core::config::{DeviceType, SessionConfig};
    use librespot_core::session::Session;
    use librespot_playback::audio_backend::{Sink, SinkError, SinkResult};
    use librespot_playback::config::{Bitrate, PlayerConfig};
    use librespot_playback::convert::Converter;
    use librespot_playback::decoder::AudioPacket;
    use librespot_playback::mixer::{self, MixerConfig};
    use librespot_playback::player::{Player, PlayerEvent};
    use tokio::sync::mpsc::UnboundedSender;

    use crate::app::AppEvent;

    pub const AVAILABLE: bool = true;

    pub struct LocalPlayer {
        spirc: Spirc,
        _player: Arc<Player>,
    }

    impl LocalPlayer {
        pub fn shutdown(&self) {
            let _ = self.spirc.shutdown();
        }
        pub fn activate(&self) {
            let _ = self.spirc.activate();
        }
    }

    pub fn has_cached_credentials(dir: &std::path::Path) -> bool {
        Cache::new(Some(dir), None, None, None)
            .ok()
            .and_then(|c| c.credentials())
            .is_some()
    }

    pub fn output_devices() -> Vec<String> {
        let host = cpal::default_host();
        host.output_devices()
            .map(|d| d.filter_map(|d| d.name().ok()).collect())
            .unwrap_or_default()
    }

    /// Connect to Spotify and register the Connect device.
    pub async fn start(opts: LocalOptions, token: Option<String>, tx: UnboundedSender<AppEvent>) -> Result<LocalPlayer> {
        let cache = Cache::new(Some(opts.cache_dir.as_path()), Some(opts.cache_dir.as_path()), None, None)
            .map_err(|e| anyhow!("librespot cache: {e}"))?;
        let mut attempts: Vec<Credentials> = Vec::new();
        if let Some(c) = cache.credentials() {
            attempts.push(c);
        }
        if let Some(t) = token {
            attempts.push(Credentials::with_access_token(t));
        }
        if attempts.is_empty() {
            let _ = tx.send(AppEvent::Local(LocalEvent::NeedsLogin));
            return Err(anyhow!("built-in player needs a one-time sign-in"));
        }

        let mut last_err = anyhow!("could not start the built-in player");
        for creds in attempts {
            let session_config = SessionConfig { device_id: opts.device_id.clone(), ..SessionConfig::default() };
            let session = Session::new(session_config, Some(cache.clone()));

            let mixer_fn = mixer::find(None).ok_or_else(|| anyhow!("no mixer"))?;
            let mixer = mixer_fn(MixerConfig::default()).map_err(|e| anyhow!("mixer: {e}"))?;

            let player_config = PlayerConfig {
                bitrate: match opts.bitrate {
                    96 => Bitrate::Bitrate96,
                    160 => Bitrate::Bitrate160,
                    _ => Bitrate::Bitrate320,
                },
                normalisation: opts.normalisation,
                position_update_interval: Some(Duration::from_millis(1000)),
                ..PlayerConfig::default()
            };
            let audio = opts.audio.clone();
            let player = Player::new(player_config, session.clone(), mixer.get_soft_volume(), move || {
                Box::new(CpalSink::new(audio)) as Box<dyn Sink>
            });

            let connect_config = ConnectConfig {
                name: opts.device_name.clone(),
                device_type: DeviceType::Computer,
                initial_volume: (u16::MAX as f32 * 0.7) as u16,
                ..ConnectConfig::default()
            };

            match Spirc::new(connect_config, session.clone(), creds, player.clone(), mixer).await {
                Ok((spirc, task)) => {
                    tokio::spawn(task);
                    spawn_event_forwarder(player.clone(), tx.clone());
                    let _ = tx.send(AppEvent::Local(LocalEvent::Connected { device_name: opts.device_name.clone() }));
                    return Ok(LocalPlayer { spirc, _player: player });
                }
                Err(e) => {
                    log::warn!("librespot connect failed: {e}");
                    last_err = anyhow!("{e}");
                }
            }
        }
        let _ = tx.send(AppEvent::Local(LocalEvent::NeedsLogin));
        Err(last_err)
    }

    fn spawn_event_forwarder(player: Arc<Player>, tx: UnboundedSender<AppEvent>) {
        let mut rx = player.get_player_event_channel();
        tokio::spawn(async move {
            while let Some(ev) = rx.recv().await {
                let uri = |id: &librespot_core::SpotifyUri| id.to_uri().unwrap_or_default();
                let mapped = match ev {
                    PlayerEvent::Playing { track_id, position_ms, .. } => {
                        Some(LocalEvent::Playing { uri: uri(&track_id), position_ms })
                    }
                    PlayerEvent::Paused { track_id, position_ms, .. } => {
                        Some(LocalEvent::Paused { uri: uri(&track_id), position_ms })
                    }
                    PlayerEvent::Seeked { track_id, position_ms, .. }
                    | PlayerEvent::PositionCorrection { track_id, position_ms, .. }
                    | PlayerEvent::PositionChanged { track_id, position_ms, .. } => {
                        Some(LocalEvent::Position { uri: uri(&track_id), position_ms })
                    }
                    PlayerEvent::Stopped { .. } => Some(LocalEvent::Stopped),
                    PlayerEvent::EndOfTrack { .. } => Some(LocalEvent::TrackEnded),
                    PlayerEvent::TrackChanged { .. } => Some(LocalEvent::TrackChanged),
                    _ => None,
                };
                if let Some(m) = mapped
                    && tx.send(AppEvent::Local(m)).is_err() {
                        break;
                    }
            }
        });
    }

    // ------------------------------------------------------------------ sink

    /// Interleaved stereo f32 frames at 44.1 kHz waiting to be played.
    struct Ring {
        q: Mutex<VecDeque<f32>>,
        cv: Condvar,
    }

    const SOURCE_RATE: f64 = 44_100.0;
    /// ~150 ms of buffered audio keeps latency (and pause/seek lag) low.
    const MAX_BUFFERED: usize = (44_100.0 * 2.0 * 0.15) as usize;

    struct Output {
        _stream: cpal::Stream,
    }

    pub struct CpalSink {
        shared: Arc<AudioShared>,
        ring: Arc<Ring>,
        out: Option<Output>,
    }

    impl CpalSink {
        fn new(shared: Arc<AudioShared>) -> Self {
            Self {
                shared,
                ring: Arc::new(Ring { q: Mutex::new(VecDeque::with_capacity(MAX_BUFFERED * 2)), cv: Condvar::new() }),
                out: None,
            }
        }

        fn open(&mut self) -> SinkResult<()> {
            let host = cpal::default_host();
            let wanted = self.shared.device.lock().map(|d| d.clone()).unwrap_or_default();
            let device = if wanted.is_empty() {
                None
            } else {
                host.output_devices()
                    .ok()
                    .and_then(|mut it| it.find(|d| d.name().map(|n| n == wanted).unwrap_or(false)))
            };
            let device = device
                .or_else(|| host.default_output_device())
                .ok_or_else(|| SinkError::NotConnected("no audio output device".into()))?;
            let supported = device
                .default_output_config()
                .map_err(|e| SinkError::NotConnected(e.to_string()))?;
            let config: cpal::StreamConfig = supported.config();
            self.shared.sample_rate.store(config.sample_rate.0, Ordering::Relaxed);
            let stream = match supported.sample_format() {
                cpal::SampleFormat::F32 => build::<f32>(&device, &config, self.ring.clone(), self.shared.clone()),
                cpal::SampleFormat::I16 => build::<i16>(&device, &config, self.ring.clone(), self.shared.clone()),
                cpal::SampleFormat::U16 => build::<u16>(&device, &config, self.ring.clone(), self.shared.clone()),
                cpal::SampleFormat::I32 => build::<i32>(&device, &config, self.ring.clone(), self.shared.clone()),
                other => return Err(SinkError::InvalidParams(format!("unsupported sample format {other:?}"))),
            }?;
            stream.play().map_err(|e| SinkError::StateChange(e.to_string()))?;
            self.out = Some(Output { _stream: stream });
            Ok(())
        }
    }

    fn build<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        ring: Arc<Ring>,
        shared: Arc<AudioShared>,
    ) -> SinkResult<cpal::Stream>
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
                    let step = SOURCE_RATE / out_rate * shared.speed() as f64;
                    mono.clear();
                    let Ok(mut q) = ring.q.lock() else {
                        data.iter_mut().for_each(|s| *s = T::from_sample(0.0f32));
                        return;
                    };
                    let avail = q.len() / 2;
                    for frame in data.chunks_mut(channels) {
                        let i = pos.floor() as usize;
                        let (l, r) = if i + 1 < avail {
                            let f = (pos - i as f64) as f32;
                            let l = q[i * 2] * (1.0 - f) + q[i * 2 + 2] * f;
                            let r = q[i * 2 + 1] * (1.0 - f) + q[i * 2 + 3] * f;
                            pos += step;
                            (l, r)
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
                    drop(q);
                    ring.cv.notify_all();
                    shared.push_tap(&mono);
                },
                |e| log::warn!("audio stream error: {e}"),
                None,
            )
            .map_err(|e| SinkError::NotConnected(e.to_string()))
    }

    pub fn self_test() -> Result<String> {
        use std::time::Instant;
        let shared = Arc::new(AudioShared::default());
        let mut sink = CpalSink::new(shared.clone());
        let mut conv = Converter::new(None);
        let t0 = Instant::now();
        sink.start().map_err(|e| anyhow!("start: {e}"))?;
        let opened = t0.elapsed();
        let mut phase = 0.0f64;
        let mut tone = |secs: f64, sink: &mut CpalSink| -> Result<()> {
            let frames = (44_100.0 * secs) as usize;
            let mut done = 0;
            while done < frames {
                let n = 2048.min(frames - done);
                let mut buf = Vec::with_capacity(n * 2);
                for _ in 0..n {
                    let v = (phase * std::f64::consts::TAU).sin() * 0.15;
                    phase += 440.0 / 44_100.0;
                    buf.push(v);
                    buf.push(v);
                }
                sink.write(AudioPacket::Samples(buf), &mut conv).map_err(|e| anyhow!("write: {e}"))?;
                done += n;
            }
            Ok(())
        };
        let t1 = Instant::now();
        tone(1.0, &mut sink)?;
        let first = t1.elapsed();
        sink.stop().map_err(|e| anyhow!("stop: {e}"))?;
        sink.start().map_err(|e| anyhow!("restart: {e}"))?;
        shared.set_speed(2.0);
        let t2 = Instant::now();
        tone(1.0, &mut sink)?;
        let fast = t2.elapsed();
        sink.stop().map_err(|e| anyhow!("stop: {e}"))?;
        let tap = shared.snapshot(1024).iter().fold(0.0f32, |m, s| m.max(s.abs()));
        Ok(format!(
            "device opened in {opened:?} at {} Hz\n1 s of audio at 1x took {first:?}\n1 s of audio at 2x took {fast:?}\nvisualizer tap peak: {tap:.3}",
            shared.sample_rate.load(Ordering::Relaxed)
        ))
    }

    impl Sink for CpalSink {
        fn start(&mut self) -> SinkResult<()> {
            if self.out.is_none() {
                self.open()?;
            }
            self.shared.playing.store(true, Ordering::Relaxed);
            Ok(())
        }

        fn stop(&mut self) -> SinkResult<()> {
            self.out = None;
            self.ring.q.lock().unwrap_or_else(|e| e.into_inner()).clear();
            self.ring.cv.notify_all();
            self.shared.playing.store(false, Ordering::Relaxed);
            Ok(())
        }

        fn write(&mut self, packet: AudioPacket, _converter: &mut Converter) -> SinkResult<()> {
            if self.shared.reopen.swap(false, Ordering::Relaxed) {
                self.out = None;
                self.ring.q.lock().unwrap_or_else(|e| e.into_inner()).clear();
            }
            if self.out.is_none() {
                self.open()?;
            }
            let samples = match packet.samples() {
                Ok(s) => s,
                Err(_) => return Ok(()),
            };
            let mut q = self.ring.q.lock().unwrap_or_else(|e| e.into_inner());
            q.extend(samples.iter().map(|&s| s as f32));
            // Back-pressure: block the decoder until the device catches up. If the
            // device stops pulling audio (unplugged, sleeping…), don't wait forever:
            // reopen it once, then report an error so librespot pauses.
            let mut last_len = q.len();
            let mut stalled = std::time::Instant::now();
            let mut reopened = false;
            while q.len() > MAX_BUFFERED {
                let (guard, _) = self.ring.cv.wait_timeout(q, Duration::from_millis(100)).unwrap_or_else(|e| e.into_inner());
                q = guard;
                if q.len() < last_len {
                    last_len = q.len();
                    stalled = std::time::Instant::now();
                } else if stalled.elapsed() > Duration::from_secs(2) {
                    drop(q);
                    self.out = None;
                    if reopened {
                        self.ring.q.lock().unwrap_or_else(|e| e.into_inner()).clear();
                        return Err(SinkError::OnWrite("audio device stopped responding".into()));
                    }
                    log::warn!("audio output stalled; reopening the device");
                    self.open()?;
                    reopened = true;
                    q = self.ring.q.lock().unwrap_or_else(|e| e.into_inner());
                    last_len = q.len();
                    stalled = std::time::Instant::now();
                }
            }
            Ok(())
        }
    }
}

/// `spotr --test-audio`: play a short tone through the built-in player's sink.
#[cfg(feature = "local-player")]
pub fn self_test() -> anyhow::Result<String> {
    imp::self_test()
}

/// Run librespot's (keymaster) OAuth login in the browser and return an access token.
pub async fn browser_login(http: &reqwest::Client) -> anyhow::Result<String> {
    let pkce = crate::spotify::auth::begin(LIBRESPOT_CLIENT_ID, LIBRESPOT_REDIRECT, LIBRESPOT_SCOPES);
    let token = crate::spotify::auth::login(http, &pkce, true).await?;
    Ok(token.access_token)
}
