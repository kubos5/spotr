//! Application state and the event/update loop.

mod browse;
mod exec;
mod input;
pub mod onboarding;
mod playback;
pub mod settings;

use std::collections::VecDeque;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::Event;
use image::DynamicImage;
use ratatui::layout::Rect;
use tokio::sync::mpsc::UnboundedSender;

use crate::anim::Motion;
use crate::audio::AudioShared;
use crate::config::{Config, Paths, SearchFilter};
use crate::images::ImageStore;
use crate::keys::{Action, KeyMap};
use crate::lyrics::Lyrics;
use crate::player::{LocalEvent, LocalPlayer};
use crate::spotify::Spotify;
use crate::spotify::models::*;
use crate::textinput::TextInput;
use crate::theme::Theme;
use crate::visualizer::Visualizer;

pub use browse::{Page, PageKind};
pub use onboarding::Onboarding;
pub use settings::SettingsState;

/// Everything that can wake the main loop.
pub enum AppEvent {
    Input(Event),
    Playback { result: Result<Option<Playback>, String> },
    Search { seq: u64, append: bool, result: Result<SearchResults, String> },
    Page { id: u64, result: Result<browse::PageData, String> },
    PageMore { id: u64, result: Result<(Vec<Item>, Option<String>), String> },
    Devices(Result<Vec<Device>, String>),
    SpotifyQueue(Result<SpotifyQueue, String>),
    Saved { track_id: String, saved: bool },
    Image { url: String, result: Result<DynamicImage, String> },
    Lyrics { uri: String, result: Result<Option<Lyrics>, String> },
    Auth(Result<User, String>),
    User(User),
    Playlists(Vec<Playlist>),
    #[cfg_attr(not(feature = "local-player"), allow(dead_code))]
    Local(LocalEvent),
    LocalReady(Result<LocalPlayer, String>),
    /// A background command finished. `ok` is shown as a toast/flash.
    Done { ok: Option<String>, err: Option<String>, refresh: bool },
    Toast(ToastKind, String),
    Enqueue(Track),
    /// A media key / OS media control.
    Media(crate::media::MediaKey),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Main,
    Command,
    SearchInput,
    SearchList,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MainView {
    NowPlaying,
    Lyrics,
    Queue,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Overlay {
    None,
    Help,
    Settings,
    About,
    Devices,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    Info,
    Success,
    Warn,
    Error,
}

pub struct Toast {
    pub msg: String,
    pub kind: ToastKind,
    pub born: Instant,
    pub ttl: Duration,
}

/// On-screen display: a short-lived badge for an action (volume, seek…).
pub struct Osd {
    pub icon: String,
    pub text: String,
    pub value: Option<f32>,
    pub born: Instant,
}

pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub born: Instant,
    pub life: f32,
    pub ch: char,
}

/// Transport controls, for the "pressed" flash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ctl {
    Shuffle,
    Prev,
    Play,
    Next,
    Repeat,
    Like,
}

/// Mouse targets registered while drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Click {
    SearchBar,
    CommandBar,
    Action(Action),
    Progress { x: u16, width: u16 },
    Volume { x: u16, width: u16 },
    Cover,
    Like,
    PlayPause,
    Shuffle,
    Repeat,
    Row(usize),
    Filter(SearchFilter),
    Back,
    QueueRow(usize),
    LyricsLine(usize),
    DeviceRow(usize),
    SettingsRow(usize),
    SettingsTab(usize),
    CloseOverlay,
    Suggestion(usize),
    Onboard(onboarding::OnboardClick),
    Toast(usize),
    /// Run a command line.
    Run(&'static str),
}

#[derive(Default)]
pub struct CommandBar {
    pub input: TextInput,
    pub history: Vec<String>,
    pub hist_idx: Option<usize>,
    pub draft: String,
    pub suggestion: Option<usize>,
    /// Text the completions are computed from while cycling with Tab.
    pub base: Option<String>,
    /// (when, ok) of the last submit, for flash / shake effects.
    pub flash: Option<(Instant, bool)>,
    pub message: Option<(String, bool, Instant)>,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct ListNav {
    pub selected: usize,
    pub offset: usize,
    pub moved_at: Option<Instant>,
}

impl ListNav {
    /// Move the selection by `delta`, skipping non-selectable rows.
    pub fn step(&mut self, delta: i32, items: &[Item]) {
        if items.is_empty() {
            return;
        }
        let n = items.len() as i32;
        let mut i = self.selected as i32;
        let dir = delta.signum();
        let mut remaining = delta.abs();
        while remaining > 0 {
            let mut j = i + dir;
            while j >= 0 && j < n && !items[j as usize].is_selectable() {
                j += dir;
            }
            if j < 0 || j >= n {
                break;
            }
            i = j;
            remaining -= 1;
        }
        if i as usize != self.selected {
            self.selected = i as usize;
            self.moved_at = Some(Instant::now());
        }
    }

    pub fn first(&mut self, items: &[Item]) {
        self.selected = items.iter().position(|i| i.is_selectable()).unwrap_or(0);
        self.moved_at = Some(Instant::now());
    }

    pub fn last(&mut self, items: &[Item]) {
        self.selected = items.iter().rposition(|i| i.is_selectable()).unwrap_or(0);
        self.moved_at = Some(Instant::now());
    }

    pub fn fix(&mut self, items: &[Item]) {
        if self.selected >= items.len() || items.get(self.selected).map(|i| !i.is_selectable()).unwrap_or(true) {
            self.first(items);
        }
    }

    pub fn simple_step(&mut self, delta: i32, len: usize) {
        if len == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as i32 + delta).clamp(0, len as i32 - 1) as usize;
        self.moved_at = Some(Instant::now());
    }
}

pub struct SearchState {
    pub input: TextInput,
    pub filter: SearchFilter,
    pub query: String,
    pub results: SearchResults,
    pub items: Vec<Item>,
    pub list: ListNav,
    pub loading: bool,
    pub loading_more: bool,
    pub error: Option<String>,
    pub seq: u64,
    pub stack: Vec<Page>,
    pub shown_at: Instant,
    pub pending: Option<Instant>,
    pub has_more: bool,
    /// The restored query is "selected": typing replaces it.
    pub select_all: bool,
}

#[derive(Default)]
pub struct QueueState {
    pub items: Vec<Track>,
    pub list: ListNav,
    /// A Spotr-queue track already handed to Spotify's queue.
    pub committed: Option<Track>,
    pub spotify: Vec<Track>,
    pub fetched: Option<Instant>,
}

#[derive(Default)]
pub struct LyricsState {
    pub uri: String,
    pub data: Option<Lyrics>,
    pub loading: bool,
    pub error: Option<String>,
    /// Manual scroll (line, when) — auto-follow resumes after a few seconds.
    pub manual: Option<(usize, Instant)>,
}

#[derive(Default)]
pub struct DevicesState {
    pub list: Vec<Device>,
    pub outputs: Vec<String>,
    pub nav: ListNav,
    pub loading: bool,
    pub fetched: Option<Instant>,
}

#[derive(Default)]
pub struct HelpState {
    pub scroll: usize,
    pub filter: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LocalStatus {
    Off,
    Connecting,
    Ready,
    NeedsLogin,
    Failed(String),
}

/// Current playback, as last reported by Spotify plus local interpolation.
pub struct PlaybackState {
    pub track: Option<Track>,
    pub is_playing: bool,
    pub progress_ms: u64,
    pub at: Instant,
    pub shuffle: bool,
    pub repeat: String,
    pub device: Option<Device>,
    pub context_uri: Option<String>,
    pub volume: Option<u32>,
    /// Ignore polls until this instant (after optimistic updates).
    pub hold_until: Instant,
    pub loaded: bool,
    /// Last error from polling, shown when nothing is playing.
    pub error: Option<String>,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            track: None,
            is_playing: false,
            progress_ms: 0,
            at: Instant::now(),
            shuffle: false,
            repeat: "off".into(),
            device: None,
            context_uri: None,
            volume: None,
            hold_until: Instant::now(),
            loaded: false,
            error: None,
        }
    }
}

impl PlaybackState {
    pub fn position(&self, speed: f32) -> u64 {
        let mut p = self.progress_ms;
        if self.is_playing {
            p += (self.at.elapsed().as_millis() as f32 * speed) as u64;
        }
        match &self.track {
            Some(t) if t.duration_ms > 0 => p.min(t.duration_ms),
            _ => p,
        }
    }

    pub fn set_position(&mut self, ms: u64) {
        self.progress_ms = ms;
        self.at = Instant::now();
    }

    pub fn duration(&self) -> u64 {
        self.track.as_ref().map(|t| t.duration_ms).unwrap_or(0)
    }

    pub fn hold(&mut self, ms: u64) {
        self.hold_until = Instant::now() + Duration::from_millis(ms);
    }
}

pub struct App {
    pub cfg: Config,
    pub paths: Paths,
    pub keymap: KeyMap,
    pub theme: Theme,
    pub motion: Motion,
    pub tx: UnboundedSender<AppEvent>,
    pub spotify: Arc<Spotify>,
    pub http: reqwest::Client,
    pub user: Option<User>,
    pub playlists: Vec<Playlist>,
    pub running: bool,

    pub pb: PlaybackState,
    pub saved_current: Option<bool>,
    pub muted_volume: Option<u32>,
    pub speed: f32,

    pub view: MainView,
    pub search_open: bool,
    pub focus: Focus,
    /// Focus to return to when the command bar closes.
    pub prev_focus: Focus,
    pub overlay: Overlay,
    pub command: CommandBar,
    pub search: SearchState,
    pub queue: QueueState,
    pub lyrics: LyricsState,
    pub devices: DevicesState,
    pub settings: SettingsState,
    pub help: HelpState,
    pub onboarding: Option<Onboarding>,

    pub images: ImageStore,
    pub vis: Visualizer,
    pub audio: Arc<AudioShared>,
    pub local: Option<LocalPlayer>,
    pub local_status: LocalStatus,

    pub toasts: Vec<Toast>,
    pub osd: Option<Osd>,
    pub particles: Vec<Particle>,
    pub clicks: Vec<(Rect, Click)>,
    pub mouse: (u16, u16),
    pub frame: u64,
    pub needs_clear: bool,
    pub track_changed_at: Instant,
    pub view_changed_at: Instant,
    pub like_pos: Option<(u16, u16)>,
    /// Last transport control used, for the pressed effect.
    pub pressed: Option<(Ctl, Instant)>,
    pub sleep_at: Option<Instant>,
    pub quit_at: Option<Instant>,
    pub splash: Option<Instant>,
    pub started: Instant,
    next_poll: Instant,
    page_seq: u64,
    pub last_input: Instant,
    pub recent_errors: VecDeque<String>,
    /// Don't run the built-in player this session (`--no-player`, `--demo`).
    pub no_player: bool,
    /// OS "Now Playing" integration, when available.
    pub media: Option<crate::media::Media>,
    media_sync: MediaSync,
}

/// What we last told the OS, to only send changes.
#[derive(Default)]
struct MediaSync {
    uri: Option<String>,
    playing: bool,
    position_ms: u64,
    at: Option<Instant>,
}

impl App {
    pub fn new(
        cfg: Config,
        paths: Paths,
        tx: UnboundedSender<AppEvent>,
        http: reqwest::Client,
        images: ImageStore,
        demo: bool,
    ) -> Self {
        let spotify = Arc::new(if demo {
            Spotify::new_demo(http.clone(), paths.token_file())
        } else {
            Spotify::new(http.clone(), paths.token_file())
        });
        let keymap = KeyMap::from_config(&cfg);
        let theme = Theme::from_config(&cfg);
        let motion = Motion {
            enabled: cfg.animations.enabled,
            scale: cfg.animations.duration,
            text_effects: cfg.animations.text_effects,
        };
        let filter = cfg.search.default_filter;
        let splash = if cfg.general.splash && cfg.general.onboarded && cfg.animations.enabled {
            Some(Instant::now())
        } else {
            None
        };
        let now = Instant::now();
        Self {
            keymap,
            theme,
            motion,
            tx,
            spotify,
            http,
            user: None,
            playlists: Vec::new(),
            running: true,
            pb: PlaybackState::default(),
            saved_current: None,
            muted_volume: None,
            speed: 1.0,
            view: MainView::NowPlaying,
            search_open: false,
            focus: Focus::Main,
            prev_focus: Focus::Main,
            overlay: Overlay::None,
            command: CommandBar::default(),
            search: SearchState {
                input: TextInput::default(),
                filter,
                query: String::new(),
                results: SearchResults::default(),
                items: Vec::new(),
                list: ListNav::default(),
                loading: false,
                loading_more: false,
                error: None,
                seq: 0,
                stack: Vec::new(),
                shown_at: now,
                pending: None,
                has_more: false,
                select_all: false,
            },
            queue: QueueState::default(),
            lyrics: LyricsState::default(),
            devices: DevicesState::default(),
            settings: SettingsState::default(),
            help: HelpState::default(),
            onboarding: None,
            images,
            vis: Visualizer::default(),
            audio: Arc::new(AudioShared::default()),
            local: None,
            local_status: LocalStatus::Off,
            toasts: Vec::new(),
            osd: None,
            particles: Vec::new(),
            clicks: Vec::new(),
            mouse: (0, 0),
            frame: 0,
            needs_clear: false,
            track_changed_at: now,
            view_changed_at: now,
            like_pos: None,
            pressed: None,
            sleep_at: None,
            quit_at: None,
            splash,
            started: now,
            next_poll: now,
            page_seq: 0,
            last_input: now,
            recent_errors: VecDeque::new(),
            no_player: demo,
            media: None,
            media_sync: MediaSync::default(),
            cfg,
            paths,
        }
    }

    /// Called once after construction.
    pub fn start(&mut self) {
        *self.audio.device.lock().unwrap() = self.cfg.general.audio_device.clone();
        if self.spotify.is_demo() {
            self.after_login();
            self.toast(ToastKind::Info, "Demo mode — a fictional catalog, nothing is streamed. Run without --demo to use Spotify.");
            return;
        }
        let have_token = auth_token_exists(&self.paths);
        if !self.cfg.general.onboarded || self.cfg.general.client_id.is_empty() || !have_token {
            self.onboarding = Some(Onboarding::new(&self.cfg, have_token));
            return;
        }
        self.after_login();
    }

    /// Things to do once we have a working token.
    pub fn after_login(&mut self) {
        let sp = self.spotify.clone();
        self.spawn(async move {
            match sp.me().await {
                Ok(u) => AppEvent::User(u),
                Err(e) => AppEvent::Toast(ToastKind::Error, format!("Couldn't load your profile: {e}")),
            }
        });
        let sp = self.spotify.clone();
        self.spawn(async move { AppEvent::Playlists(sp.my_playlists().await.unwrap_or_default()) });
        self.next_poll = Instant::now();
        if self.cfg.general.local_player && crate::player::AVAILABLE && !self.no_player {
            self.start_local_player(false);
        }
    }

    pub fn spawn<F>(&self, fut: F)
    where
        F: Future<Output = AppEvent> + Send + 'static,
    {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let ev = fut.await;
            let _ = tx.send(ev);
        });
    }

    /// Like `spawn`, but if the task panics, `on_panic` turns the message into an event.
    pub fn spawn_or<F, P>(&self, on_panic: P, fut: F)
    where
        F: Future<Output = AppEvent> + Send + 'static,
        P: FnOnce(String) -> AppEvent + Send + 'static,
    {
        let tx = self.tx.clone();
        let handle = tokio::spawn(fut);
        tokio::spawn(async move {
            let ev = match handle.await {
                Ok(ev) => ev,
                Err(e) => {
                    let msg = e
                        .try_into_panic()
                        .ok()
                        .and_then(|p| p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()))
                        .unwrap_or_else(|| "task failed".into());
                    on_panic(msg.lines().next().unwrap_or("").to_string())
                }
            };
            let _ = tx.send(ev);
        });
    }

    /// Run a Spotify call in the background; report success / failure.
    pub fn run<F, Fut>(&self, ok: Option<String>, f: F)
    where
        F: FnOnce(Arc<Spotify>) -> Fut + Send + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let sp = self.spotify.clone();
        self.spawn(async move {
            match f(sp).await {
                Ok(()) => AppEvent::Done { ok, err: None, refresh: true },
                Err(e) => AppEvent::Done { ok: None, err: Some(e.to_string()), refresh: true },
            }
        });
    }

    pub fn save_config(&mut self) {
        self.cfg.sanitize();
        if let Err(e) = self.cfg.save(&self.paths) {
            self.toast(ToastKind::Error, format!("Couldn't save settings: {e}"));
        }
        self.apply_config();
    }

    /// Re-derive cached things after a config change.
    pub fn apply_config(&mut self) {
        self.keymap = KeyMap::from_config(&self.cfg);
        self.theme = Theme::from_config(&self.cfg);
        self.motion = Motion {
            enabled: self.cfg.animations.enabled,
            scale: self.cfg.animations.duration,
            text_effects: self.cfg.animations.text_effects,
        };
        self.needs_clear = true;
        let _ = set_mouse(self.cfg.general.mouse);
    }

    pub fn toast(&mut self, kind: ToastKind, msg: impl Into<String>) {
        let msg = msg.into();
        if kind == ToastKind::Error {
            log::warn!("{msg}");
            self.recent_errors.push_back(msg.clone());
            if self.recent_errors.len() > 20 {
                self.recent_errors.pop_front();
            }
        }
        // Don't stack identical toasts.
        if let Some(t) = self.toasts.iter_mut().find(|t| t.msg == msg) {
            t.born = Instant::now();
            return;
        }
        let ttl = match kind {
            ToastKind::Error => Duration::from_secs(6),
            ToastKind::Warn => Duration::from_secs(5),
            _ => Duration::from_millis(3200),
        };
        self.toasts.push(Toast { msg, kind, born: Instant::now(), ttl });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    pub fn osd(&mut self, icon: &str, text: impl Into<String>, value: Option<f32>) {
        if !self.cfg.appearance.show_osd {
            return;
        }
        self.osd = Some(Osd { icon: icon.to_string(), text: text.into(), value, born: Instant::now() });
    }

    pub fn press(&mut self, c: Ctl) {
        self.pressed = Some((c, Instant::now()));
    }

    pub fn burst(&mut self, x: u16, y: u16, chars: &[char]) {
        if !self.motion.enabled {
            return;
        }
        for i in 0..14 {
            let a = (i as f32 / 14.0) * std::f32::consts::TAU + rand::random_range(0.0..0.4);
            let sp = rand::random_range(6.0..16.0f32);
            self.particles.push(Particle {
                x: x as f32,
                y: y as f32,
                vx: a.cos() * sp * 1.8,
                vy: a.sin() * sp - 6.0,
                born: Instant::now(),
                life: rand::random_range(0.5..0.9),
                ch: chars[i % chars.len()],
            });
        }
    }

    /// The configured title font (falls back to Block if it's missing).
    pub fn title_font(&self) -> crate::font::FontId {
        crate::font::resolve(&self.cfg.appearance.title_font)
    }

    pub fn is_local_active(&self) -> bool {
        self.local_status == LocalStatus::Ready
            && self
                .pb
                .device
                .as_ref()
                .map(|d| d.id.as_deref() == Some(self.cfg.general.device_id.as_str()) || d.name == self.cfg.general.device_name)
                .unwrap_or(false)
    }

    /// The speed factor that applies to the progress clock.
    pub fn effective_speed(&self) -> f32 {
        if self.is_local_active() { self.speed } else { 1.0 }
    }

    pub fn position(&self) -> u64 {
        self.pb.position(self.effective_speed())
    }

    pub fn wants_animation(&self) -> bool {
        let m = &self.motion;
        if !m.enabled {
            return self.pb.is_playing;
        }
        self.pb.is_playing
            || !self.toasts.is_empty()
            || self.osd.is_some()
            || !self.particles.is_empty()
            || self.splash.is_some()
            || self.quit_at.is_some()
            || self.onboarding.is_some()
            || m.active(self.track_changed_at, 1200)
            || m.active(self.view_changed_at, 800)
            || m.active(self.search.shown_at, 900)
            || self.command.flash.map(|(t, _)| m.active(t, 600)).unwrap_or(false)
            || self.pressed.map(|(_, t)| m.active(t, 300)).unwrap_or(false)
            || self.search.loading
            || !self.vis.idle()
    }

    /// How long the main loop may sleep before the next frame.
    pub fn frame_interval(&self) -> Duration {
        if self.wants_animation() {
            Duration::from_millis(1000 / self.cfg.animations.fps.max(10) as u64)
        } else {
            Duration::from_millis(250)
        }
    }

    // ---------------------------------------------------------------- events

    pub fn handle_event(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Input(e) => {
                self.last_input = Instant::now();
                self.handle_input(e);
            }
            AppEvent::Playback { result } => match result {
                Ok(p) => {
                    self.pb.error = None;
                    self.apply_playback(p);
                }
                Err(e) => {
                    log::debug!("poll failed: {e}");
                    if self.pb.error.is_none() && (e.contains("Not signed in") || e.contains("invalid_grant")) {
                        self.toast(ToastKind::Error, "Your Spotify login expired — run `login` to sign in again");
                    }
                    self.pb.error = Some(e);
                }
            },
            AppEvent::Search { seq, append, result } => self.on_search_results(seq, append, result),
            AppEvent::Page { id, result } => self.on_page(id, result),
            AppEvent::PageMore { id, result } => self.on_page_more(id, result),
            AppEvent::Devices(r) => {
                self.devices.loading = false;
                match r {
                    Ok(list) => {
                        self.devices.list = list;
                        let n = self.devices.list.len() + self.devices.outputs.len();
                        if self.devices.nav.selected >= n {
                            self.devices.nav.selected = 0;
                        }
                    }
                    Err(e) => self.toast(ToastKind::Error, e),
                }
            }
            AppEvent::SpotifyQueue(r) => {
                if let Ok(q) = r {
                    self.queue.spotify = q.queue;
                }
            }
            AppEvent::Saved { track_id, saved } => {
                if self.pb.track.as_ref().and_then(|t| t.id.as_deref()) == Some(track_id.as_str()) {
                    self.saved_current = Some(saved);
                }
            }
            AppEvent::Image { url, result } => match result {
                Ok(img) => self.images.insert(url, img),
                Err(e) => {
                    log::debug!("image {url}: {e}");
                    self.images.fail(url);
                }
            },
            AppEvent::Lyrics { uri, result } => {
                if uri == self.lyrics.uri {
                    self.lyrics.loading = false;
                    match result {
                        Ok(l) => {
                            self.lyrics.data = l;
                            self.lyrics.error = None;
                        }
                        Err(e) => self.lyrics.error = Some(e),
                    }
                }
            }
            AppEvent::Auth(r) => self.on_auth(r),
            AppEvent::User(u) => {
                self.user = Some(u.clone());
                if u.product.as_deref().is_some_and(|p| p != "premium") {
                    self.toast(
                        ToastKind::Warn,
                        "Your account isn't Premium — Spotify only allows playback control for Premium accounts.",
                    );
                }
            }
            AppEvent::Playlists(p) => self.playlists = p,
            AppEvent::Local(e) => self.on_local_event(e),
            AppEvent::LocalReady(r) => match r {
                Ok(p) => {
                    self.local = Some(p);
                    self.local_status = LocalStatus::Ready;
                    if let Some(o) = self.onboarding.as_mut() {
                        o.player_ready();
                    }
                }
                Err(e) => {
                    if self.local_status != LocalStatus::NeedsLogin {
                        self.local_status = LocalStatus::Failed(e.clone());
                        self.toast(ToastKind::Error, format!("Built-in player: {e}"));
                    }
                    if let Some(o) = self.onboarding.as_mut() {
                        o.player_failed(e);
                    }
                }
            },
            AppEvent::Done { ok, err, refresh } => {
                if let Some(e) = err {
                    self.command.flash = Some((Instant::now(), false));
                    self.toast(ToastKind::Error, e);
                    // Undo optimistic state.
                    self.pb.hold_until = Instant::now();
                    self.poll_now();
                } else if let Some(m) = ok {
                    self.toast(ToastKind::Success, m);
                }
                if refresh {
                    self.poll_soon(350);
                }
            }
            AppEvent::Toast(kind, msg) => self.toast(kind, msg),
            AppEvent::Enqueue(t) => self.enqueue(t),
            AppEvent::Media(key) => self.on_media_key(key),
        }
    }

    /// Per-frame bookkeeping.
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        let now = Instant::now();

        // A background thread (e.g. librespot's) panicked: tell the user instead of dying.
        for msg in crate::term::take_background_panics() {
            if self.local_status == LocalStatus::Connecting {
                self.local_status = LocalStatus::Failed(msg.clone());
            }
            self.toast(ToastKind::Error, format!("Background error: {msg} (details in the log)"));
        }

        // Toasts / OSD / particles lifetimes.
        self.toasts.retain(|t| now.duration_since(t.born) < t.ttl);
        if let Some(o) = &self.osd
            && now.duration_since(o.born) > self.motion.dur(1100).max(Duration::from_millis(900)) {
                self.osd = None;
            }
        let dt = 1.0 / self.cfg.animations.fps.max(10) as f32;
        self.particles.retain_mut(|p| {
            let age = p.born.elapsed().as_secs_f32();
            p.x += p.vx * dt;
            p.y += p.vy * dt * 0.5;
            p.vy += 30.0 * dt;
            p.vx *= 0.96;
            age < p.life
        });
        if let Some(s) = self.splash
            && s.elapsed() > Duration::from_millis(1400) {
                self.splash = None;
                self.needs_clear = true;
            }
        if let Some(q) = self.quit_at
            && q.elapsed() > Duration::from_millis(450) {
                self.running = false;
            }

        if self.onboarding.is_some() {
            return;
        }

        // Polling.
        if now >= self.next_poll {
            self.poll_now();
        }

        // Debounced live search.
        if let Some(t) = self.search.pending
            && t.elapsed() >= Duration::from_millis(self.cfg.search.debounce_ms) {
                self.search.pending = None;
                self.run_search(false);
            }

        // Sleep timer.
        if let Some(at) = self.sleep_at
            && now >= at {
                self.sleep_at = None;
                self.pause();
                self.toast(ToastKind::Info, "Sleep timer: playback paused. Good night ☾");
            }

        // Periodic refreshes for open panels.
        if self.overlay == Overlay::Devices
            && self.devices.fetched.map(|t| t.elapsed() > Duration::from_secs(4)).unwrap_or(true)
        {
            self.fetch_devices();
        }
        if self.view == MainView::Queue
            && self.queue.fetched.map(|t| t.elapsed() > Duration::from_secs(6)).unwrap_or(true)
        {
            self.fetch_spotify_queue();
        }

        self.queue_handoff();
        self.sync_media();

        // Visualizer.
        if self.cfg.visualizer.enabled && !self.search_open {
            let width = self.vis_width();
            let n = Visualizer::bands_for(width, self.cfg.visualizer.style);
            let use_real = self.cfg.visualizer.source != crate::config::VisSource::Simulated && self.is_local_active();
            let audio = if use_real { Some(self.audio.as_ref()) } else { None };
            self.vis.update(n, audio, self.pb.is_playing, self.cfg.visualizer.sensitivity, self.effective_speed());
        }
    }

    fn vis_width(&self) -> u16 {
        crossterm::terminal::size().map(|(w, _)| w.saturating_sub(4)).unwrap_or(80)
    }

    pub fn poll_now(&mut self) {
        self.next_poll = Instant::now() + Duration::from_millis(self.cfg.general.poll_interval_ms);
        if !self.spotify_ready() {
            return;
        }
        let sp = self.spotify.clone();
        self.spawn(async move { AppEvent::Playback { result: sp.playback().await.map_err(|e| e.to_string()) } });
    }

    pub fn poll_soon(&mut self, ms: u64) {
        let at = Instant::now() + Duration::from_millis(ms);
        if at < self.next_poll {
            self.next_poll = at;
        }
    }

    pub fn spotify_ready(&self) -> bool {
        self.onboarding.is_none()
    }

    /// Fetch an image if needed.
    pub fn want_image(&mut self, url: &str) {
        if self.images.want(url) {
            let http = self.http.clone();
            let disk = self.images.disk_path(url);
            let url = url.to_string();
            self.spawn(async move {
                let result = crate::images::fetch(http, url.clone(), disk).await.map_err(|e| e.to_string());
                AppEvent::Image { url, result }
            });
        }
    }

    pub fn set_overlay(&mut self, o: Overlay) {
        if self.overlay != o {
            self.overlay = o;
            self.view_changed_at = Instant::now();
            self.needs_clear = true;
        }
        if o == Overlay::Devices {
            self.devices.outputs = if self.local.is_some() { crate::player::output_devices() } else { Vec::new() };
            self.fetch_devices();
        }
        if self.focus == Focus::Command {
            self.focus = Focus::Main;
        }
    }

    pub fn set_view(&mut self, v: MainView) {
        if self.search_open {
            self.close_search();
        }
        if self.overlay != Overlay::None {
            self.overlay = Overlay::None;
        }
        if self.view != v {
            self.view = v;
            self.view_changed_at = Instant::now();
            self.needs_clear = true;
        }
        match v {
            MainView::Queue => self.fetch_spotify_queue(),
            MainView::Lyrics => self.ensure_lyrics(),
            MainView::NowPlaying => {}
        }
    }

    pub fn fetch_devices(&mut self) {
        self.devices.fetched = Some(Instant::now());
        self.devices.loading = true;
        let sp = self.spotify.clone();
        self.spawn(async move { AppEvent::Devices(sp.devices().await.map_err(|e| e.to_string())) });
    }

    pub fn fetch_spotify_queue(&mut self) {
        self.queue.fetched = Some(Instant::now());
        let sp = self.spotify.clone();
        self.spawn(async move { AppEvent::SpotifyQueue(sp.queue().await.map_err(|e| e.to_string())) });
    }

    fn on_media_key(&mut self, key: crate::media::MediaKey) {
        use crate::media::MediaKey::*;
        log::debug!("media key: {key:?}");
        match key {
            Play => self.resume(),
            Pause | Stop => {
                if self.pb.is_playing {
                    self.pause();
                }
            }
            Toggle => self.toggle_play(),
            Next => self.next(),
            Previous => self.previous(),
            SeekBy(ms) => self.seek_by(ms),
            SetPosition(ms) => self.seek_to(ms),
            SetVolume(v) => self.set_volume((v.clamp(0.0, 1.0) * 100.0).round() as u32),
            OpenUri(u) => self.execute(&format!("play {u}")),
            Raise => {}
            Quit => self.quit(),
        }
    }

    /// Cover for the OS: always the original artwork, never our rendering.
    fn media_cover_url(&self, track: &Track) -> Option<String> {
        let url = track.image(640)?;
        let Some(seed) = url.strip_prefix("demo://") else { return Some(url) };
        // Demo covers are generated; save one as a PNG the OS can load.
        let dir = self.paths.cache_dir.join("media");
        let path = dir.join(format!("{}.png", seed.replace(|c: char| !c.is_ascii_alphanumeric(), "_")));
        if !path.exists() {
            std::fs::create_dir_all(&dir).ok()?;
            crate::spotify::demo::image(seed).save(&path).ok()?;
        }
        Some(format!("file://{}", path.display()))
    }

    /// Tell the OS what's playing, when it changes.
    fn sync_media(&mut self) {
        if self.media.is_none() {
            return;
        }
        let Some(track) = self.pb.track.clone() else {
            if self.media_sync.uri.take().is_some()
                && let Some(m) = &self.media
            {
                m.set_stopped();
            }
            return;
        };
        let pos = self.position();
        let speed = self.effective_speed();
        let new_track = self.media_sync.uri.as_deref() != Some(track.uri.as_str());
        if new_track {
            let meta = crate::media::Metadata {
                title: track.name.clone(),
                artist: track.artist_names(),
                album: track.album_name().to_string(),
                cover_url: self.media_cover_url(&track),
                duration_ms: track.duration_ms,
            };
            if let Some(m) = &self.media {
                m.set_metadata(meta);
            }
            self.media_sync.uri = Some(track.uri.clone());
        }
        // Resend the position when it drifts from what the OS extrapolates.
        let s = &self.media_sync;
        let expected = match s.at {
            Some(at) if s.playing => s.position_ms + (at.elapsed().as_millis() as f32 * speed) as u64,
            _ => s.position_ms,
        };
        let drift = (expected as i64 - pos as i64).abs();
        let stale = s.at.is_none_or(|at| at.elapsed() > Duration::from_secs(15));
        if new_track || s.playing != self.pb.is_playing || drift > 1500 || stale {
            if let Some(m) = &self.media {
                m.set_playback(self.pb.is_playing, pos);
            }
            self.media_sync.playing = self.pb.is_playing;
            self.media_sync.position_ms = pos;
            self.media_sync.at = Some(Instant::now());
        }
    }

    pub fn quit(&mut self) {
        if let Some(p) = &self.local {
            p.shutdown();
        }
        if self.motion.enabled {
            if self.quit_at.is_none() {
                self.quit_at = Some(Instant::now());
            }
        } else {
            self.running = false;
        }
    }

    pub fn next_page_id(&mut self) -> u64 {
        self.page_seq += 1;
        self.page_seq
    }

    pub fn device_names(&self) -> Vec<String> {
        self.devices.list.iter().map(|d| d.name.clone()).collect()
    }

    pub fn playlist_names(&self) -> Vec<String> {
        let me = self.user.as_ref().map(|u| u.id.as_str()).unwrap_or("");
        self.playlists
            .iter()
            .filter(|p| p.owner.id == me)
            .map(|p| p.name.clone())
            .collect()
    }
}

fn auth_token_exists(paths: &Paths) -> bool {
    crate::spotify::auth::load_token(&paths.token_file()).is_some()
}

pub fn set_mouse(on: bool) -> std::io::Result<()> {
    use crossterm::event::{DisableMouseCapture, EnableMouseCapture};
    if on {
        crossterm::execute!(crate::term::out(), EnableMouseCapture)
    } else {
        crossterm::execute!(crate::term::out(), DisableMouseCapture)
    }
}
