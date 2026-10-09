//! OS media integration: "Now Playing" metadata, playback state and media keys.
//!
//! * macOS: Now Playing / Control Center and the media keys
//!   (needs an AppKit event loop on the main thread, see `run_main_loop`)
//! * Linux: MPRIS over D-Bus (GNOME, KDE, playerctl…)
//! * Windows: not supported (system media controls need a real window)
//!
//! All calls go through a dedicated thread that owns `MediaControls`.

use std::sync::mpsc;
use std::time::Duration;

use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig, SeekDirection};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::AppEvent;

/// What the OS asked us to do.
#[derive(Debug, Clone)]
pub enum MediaKey {
    Play,
    Pause,
    Toggle,
    Next,
    Previous,
    Stop,
    /// Relative seek in milliseconds (negative = backwards).
    SeekBy(i64),
    SetPosition(u64),
    SetVolume(f64),
    OpenUri(String),
    Raise,
    Quit,
}

#[derive(Clone, PartialEq)]
pub struct Metadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// The original, unmodified cover image (http(s) or file:// URL).
    pub cover_url: Option<String>,
    pub duration_ms: u64,
}

enum Cmd {
    Metadata(Metadata),
    Playback { playing: bool, position_ms: u64 },
    Stopped,
}

pub struct Media {
    tx: mpsc::Sender<Cmd>,
}

impl Media {
    pub fn set_metadata(&self, m: Metadata) {
        let _ = self.tx.send(Cmd::Metadata(m));
    }
    pub fn set_playback(&self, playing: bool, position_ms: u64) {
        let _ = self.tx.send(Cmd::Playback { playing, position_ms });
    }
    pub fn set_stopped(&self) {
        let _ = self.tx.send(Cmd::Stopped);
    }
}

fn map_event(e: MediaControlEvent) -> MediaKey {
    let ms = |d: Duration| d.as_millis() as i64;
    match e {
        MediaControlEvent::Play => MediaKey::Play,
        MediaControlEvent::Pause => MediaKey::Pause,
        MediaControlEvent::Toggle => MediaKey::Toggle,
        MediaControlEvent::Next => MediaKey::Next,
        MediaControlEvent::Previous => MediaKey::Previous,
        MediaControlEvent::Stop => MediaKey::Stop,
        MediaControlEvent::Seek(SeekDirection::Forward) => MediaKey::SeekBy(10_000),
        MediaControlEvent::Seek(SeekDirection::Backward) => MediaKey::SeekBy(-10_000),
        MediaControlEvent::SeekBy(SeekDirection::Forward, d) => MediaKey::SeekBy(ms(d)),
        MediaControlEvent::SeekBy(SeekDirection::Backward, d) => MediaKey::SeekBy(-ms(d)),
        MediaControlEvent::SetPosition(MediaPosition(d)) => MediaKey::SetPosition(d.as_millis() as u64),
        MediaControlEvent::SetVolume(v) => MediaKey::SetVolume(v),
        MediaControlEvent::OpenUri(u) => MediaKey::OpenUri(u),
        MediaControlEvent::Raise => MediaKey::Raise,
        MediaControlEvent::Quit => MediaKey::Quit,
    }
}

/// Start the media integration. Returns `None` where it isn't available.
pub fn start(app_tx: UnboundedSender<AppEvent>) -> Option<Media> {
    if cfg!(windows) {
        return None;
    }
    let (tx, rx) = mpsc::channel::<Cmd>();
    let (ready_tx, ready_rx) = mpsc::channel::<bool>();
    std::thread::Builder::new()
        .name("spotr-media".into())
        .spawn(move || {
            let config = PlatformConfig { display_name: "Spotr", dbus_name: "spotr", hwnd: None };
            let mut controls = match MediaControls::new(config) {
                Ok(c) => c,
                Err(e) => {
                    log::warn!("media controls unavailable: {e:?}");
                    let _ = ready_tx.send(false);
                    return;
                }
            };
            let events = app_tx.clone();
            if let Err(e) = controls.attach(move |e| {
                let _ = events.send(AppEvent::Media(map_event(e)));
            }) {
                log::warn!("media controls: attach failed: {e:?}");
                let _ = ready_tx.send(false);
                return;
            }
            let _ = ready_tx.send(true);
            while let Ok(cmd) = rx.recv() {
                let r = match cmd {
                    Cmd::Metadata(m) => controls.set_metadata(MediaMetadata {
                        title: Some(&m.title),
                        artist: Some(&m.artist),
                        album: Some(&m.album),
                        cover_url: m.cover_url.as_deref(),
                        duration: Some(Duration::from_millis(m.duration_ms)),
                    }),
                    Cmd::Playback { playing, position_ms } => {
                        let progress = Some(MediaPosition(Duration::from_millis(position_ms)));
                        controls.set_playback(if playing {
                            MediaPlayback::Playing { progress }
                        } else {
                            MediaPlayback::Paused { progress }
                        })
                    }
                    Cmd::Stopped => controls.set_playback(MediaPlayback::Stopped),
                };
                if let Err(e) = r {
                    log::debug!("media controls update failed: {e:?}");
                }
                #[cfg(target_os = "macos")]
                log::debug!("now playing: {}", macos_now_playing_summary());
            }
        })
        .ok()?;
    match ready_rx.recv_timeout(Duration::from_secs(3)) {
        Ok(true) => Some(Media { tx }),
        _ => None,
    }
}

/// What MPNowPlayingInfoCenter currently holds (for the debug log).
#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)] // from the `objc` macros
fn macos_now_playing_summary() -> String {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    // SAFETY: read-only queries on the shared info center.
    unsafe {
        let center: *mut Object = msg_send![class!(MPNowPlayingInfoCenter), defaultCenter];
        let info: *mut Object = msg_send![center, nowPlayingInfo];
        let state: i64 = msg_send![center, playbackState];
        let count: usize = if info.is_null() { 0 } else { msg_send![info, count] };
        format!("{count} fields, playbackState={state}")
    }
}

/// macOS only delivers media keys / Now Playing to a process running an AppKit
/// event loop on its main thread. This runs one (no Dock icon, no windows) and
/// never returns; the UI runs on another thread and exits the process.
#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)] // from the `objc` macros
pub fn run_main_loop() -> ! {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {}

    // SAFETY: standard NSApplication setup, on the main thread.
    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        // NSApplicationActivationPolicyAccessory: no Dock icon, no menu bar.
        let _: bool = msg_send![app, setActivationPolicy: 1i64];
        let _: () = msg_send![app, run];
    }
    // `run` only returns if someone stops the app.
    loop {
        std::thread::park();
    }
}
