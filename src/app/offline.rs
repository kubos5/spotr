//! Local files mode inside the app: switching modes, the "set up this folder?"
//! prompt, playback through the local engine, library pages and the home list.
//!
//! Local tracks use Spotr's Spotify models with `local:` URIs, so the regular
//! playback methods only need to branch here instead of calling Spotify.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::browse::{PageData, cap_first, fmt_long, play_buttons};
use super::*;
use crate::config::{SearchContext, SongSort};
use crate::local::engine::{Engine, EngineEvent};
use crate::local::library::{self, Library, ScanResult, rel_of};
use crate::local::store::{self, Session, Store};

pub enum OfflineEvent {
    Progress { root: PathBuf, done: usize, total: usize },
    Scanned { root: PathBuf, result: Box<ScanResult> },
    Engine(EngineEvent),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HomeTab {
    Songs,
    Albums,
    Artists,
    Playlists,
    Liked,
    Recent,
}

impl HomeTab {
    pub const ALL: [HomeTab; 6] = [HomeTab::Songs, HomeTab::Albums, HomeTab::Artists, HomeTab::Playlists, HomeTab::Liked, HomeTab::Recent];

    pub fn label(self) -> &'static str {
        match self {
            HomeTab::Songs => "Songs",
            HomeTab::Albums => "Albums",
            HomeTab::Artists => "Artists",
            HomeTab::Playlists => "Playlists",
            HomeTab::Liked => "Liked Songs",
            HomeTab::Recent => "Recent",
        }
    }

    pub fn cycle(self, dir: i32) -> HomeTab {
        let n = Self::ALL.len() as i32;
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0) as i32;
        Self::ALL[(i + dir).rem_euclid(n) as usize]
    }
}

/// The list shown instead of "Nothing is playing".
pub struct Home {
    pub tab: HomeTab,
    pub items: Vec<Item>,
    pub nav: ListNav,
    pub shown: Instant,
}

pub struct Offline {
    pub root: PathBuf,
    /// The folder's name, shown as the library's title.
    pub name: String,
    pub lib: Library,
    pub store: Store,
    pub engine: Option<Engine>,
    pub engine_error: Option<String>,
    /// (done, total) while reading new files.
    pub scanning: Option<(usize, usize)>,
    pub scanned: bool,
    /// What's playing from (an album, the song list, a playlist…) and in which order.
    pub ctx: Vec<Track>,
    pub order: Vec<usize>,
    pub pos: usize,
    pub ctx_label: String,
    pub ctx_uri: Option<String>,
    /// Generation of the engine's current load; older events are ignored.
    pub load_id: u64,
    /// The current song already counts as played.
    pub counted: bool,
    /// Songs that failed to play in a row (stops skipping through a broken folder).
    pub failures: u32,
    /// Where you left off last time.
    pub resume: Option<Session>,
    pub home: Home,
}

/// "Set up this folder as a library?"
pub struct InitPrompt {
    pub dir: PathBuf,
    pub warning: Option<String>,
    /// 0 = set up, 1 = cancel.
    pub choice: usize,
    /// Opened at launch: cancelling continues with Spotify.
    pub at_startup: bool,
}

/// "1 song", "3 songs".
pub fn plural(n: usize, word: &str) -> String {
    if n == 1 { format!("1 {word}") } else { format!("{n} {word}s") }
}

fn folder_name(dir: &Path) -> String {
    dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| dir.display().to_string())
}

/// Shuffled play order that starts with `first`.
fn shuffled(n: usize, first: usize) -> Vec<usize> {
    use rand::seq::SliceRandom;
    let mut rest: Vec<usize> = (0..n).filter(|&i| i != first).collect();
    rest.shuffle(&mut rand::rng());
    std::iter::once(first).filter(|&f| f < n).chain(rest).collect()
}

impl App {
    /// The local home list replaces the idle now-playing screen.
    pub fn home_active(&self) -> bool {
        self.offline.is_some() && self.view == MainView::NowPlaying && self.pb.track.is_none() && !self.search_open
    }

    // ------------------------------------------------------------ modes

    /// Open a folder as a local library (asking to set it up first if needed).
    pub fn open_local(&mut self, dir: PathBuf, at_startup: bool) {
        if !crate::local::engine::AVAILABLE {
            self.toast(ToastKind::Error, "Spotr was built without local files support (the `local-files` feature)");
            if at_startup {
                self.local_launch_failed();
            }
            return;
        }
        let dir = dir.canonicalize().unwrap_or(dir);
        if !dir.is_dir() {
            self.toast(ToastKind::Error, format!("{} isn't a folder", dir.display()));
            if at_startup {
                self.local_launch_failed();
            }
            return;
        }
        if store::is_initialized(&dir) {
            self.enter_offline(dir);
            return;
        }
        let warning = store::init_warning(&dir);
        // A risky folder starts on "Cancel", so Enter alone doesn't set it up.
        let choice = if warning.is_some() { 1 } else { 0 };
        self.init_prompt = Some(InitPrompt { warning, dir, choice, at_startup });
        self.set_overlay(Overlay::Init);
    }

    pub fn confirm_init(&mut self) {
        let Some(p) = self.init_prompt.take() else { return };
        self.set_overlay(Overlay::None);
        match Store::init(&p.dir) {
            Ok(_) => {
                self.toast(ToastKind::Success, format!("“{}” is now a Spotr library — likes, playlists and history live in its .spotr folder", folder_name(&p.dir)));
                self.enter_offline(p.dir);
            }
            Err(e) => {
                self.toast(ToastKind::Error, format!("Couldn't set up the folder: {e:#}"));
                if p.at_startup {
                    self.local_launch_failed();
                }
            }
        }
    }

    pub fn cancel_init(&mut self) {
        let Some(p) = self.init_prompt.take() else { return };
        self.set_overlay(Overlay::None);
        if p.at_startup {
            if !self.cfg.general.local_only {
                self.toast(ToastKind::Info, "Nothing was set up — `local` opens a music folder any time");
            }
            self.local_launch_failed();
        }
    }

    /// The folder Spotr was launched with didn't open: fall back to Spotify,
    /// unless local-only mode keeps it away.
    fn local_launch_failed(&mut self) {
        if self.cfg.general.local_only && !self.spotify.is_demo() {
            self.toast(ToastKind::Info, "Local-only mode — `local <folder>` opens a music folder, `online` uses Spotify");
            return;
        }
        self.start_online();
    }

    fn enter_offline(&mut self, root: PathBuf) {
        let store = match Store::open(&root) {
            Ok(s) => s,
            Err(e) => {
                self.toast(ToastKind::Error, format!("{e:#}"));
                return;
            }
        };
        if self.offline.is_some() {
            self.close_offline();
        } else if self.online {
            // Let go of Spotify: stop what plays here, stop polling, free the Connect device.
            if self.pb.is_playing && self.is_local_active() {
                self.pause();
            }
            self.stop_local_player();
            self.local_retry = None;
            self.online_queue = std::mem::take(&mut self.queue.items);
            self.online = false;
        }
        let name = folder_name(&root);
        let resume = if self.cfg.local.resume { store.load_session() } else { None };
        let tx = self.tx.clone();
        let (engine, engine_error) = match Engine::start(self.audio.clone(), move |e| {
            let _ = tx.send(AppEvent::Offline(OfflineEvent::Engine(e)));
        }) {
            Ok(e) => (Some(e), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let volume = resume.as_ref().and_then(|s| s.volume).unwrap_or(self.cfg.playback.volume);
        if let Some(e) = &engine {
            e.set_volume(volume);
        }
        self.offline = Some(Offline {
            lib: Library::empty(root.clone()),
            root: root.clone(),
            name: name.clone(),
            store,
            engine,
            engine_error: engine_error.clone(),
            scanning: Some((0, 0)),
            scanned: false,
            ctx: Vec::new(),
            order: Vec::new(),
            pos: 0,
            ctx_label: String::new(),
            ctx_uri: None,
            load_id: 0,
            counted: false,
            failures: 0,
            resume,
            home: Home { tab: HomeTab::Songs, items: Vec::new(), nav: ListNav::default(), shown: Instant::now() },
        });
        self.reset_for_mode();
        self.pb.volume = Some(volume);
        self.pb.device = Some(self.local_device());
        if let Some(s) = self.offline.as_ref().and_then(|o| o.resume.clone()) {
            self.pb.shuffle = s.shuffle;
            self.pb.repeat = if s.repeat.is_empty() { "off".into() } else { s.repeat.clone() };
        }
        self.pb.loaded = true;
        if let Some(e) = engine_error {
            self.toast(ToastKind::Error, format!("Local playback is unavailable: {e}"));
        }
        self.toast(ToastKind::Info, format!("Local library “{name}” — `online` switches to Spotify"));
        self.rescan();
    }

    /// Save and shut the local library (when leaving it or quitting).
    pub fn close_offline(&mut self) {
        self.save_local_session();
        if let Some(mut off) = self.offline.take() {
            if let Some(e) = off.store.flush(true) {
                log::warn!("couldn't save the library: {e}");
            }
            if let Some(e) = off.engine.take() {
                e.shutdown();
            }
        }
    }

    /// Back to Spotify.
    pub fn go_online(&mut self) {
        if self.offline.is_none() {
            return;
        }
        let name = self.offline.as_ref().map(|o| o.name.clone()).unwrap_or_default();
        let root = self.offline.as_ref().map(|o| o.root.clone());
        self.close_offline();
        self.reset_for_mode();
        self.queue.items = std::mem::take(&mut self.online_queue);
        self.toast(ToastKind::Info, format!("Left “{name}” — back on Spotify"));
        self.start_online();
        if self.onboarding.is_some() {
            self.return_to_local = root;
        }
    }

    /// Clear everything that belongs to the other mode.
    fn reset_for_mode(&mut self) {
        self.pb = PlaybackState { volume: Some(self.cfg.playback.volume), ..Default::default() };
        self.saved_current = None;
        self.radio = None;
        self.queue = QueueState::default();
        self.lyrics = LyricsState::default();
        self.devices = DevicesState::default();
        self.search.stack.clear();
        self.search.items.clear();
        self.search.results = SearchResults::default();
        self.search.query.clear();
        self.search.input.clear();
        self.search.list = ListNav::default();
        self.search.error = None;
        self.search.loading = false;
        self.search_open = false;
        self.focus = Focus::Main;
        self.view = MainView::NowPlaying;
        self.overlay = Overlay::None;
        self.view_changed_at = Instant::now();
        self.track_changed_at = Instant::now();
        self.needs_clear = true;
    }

    pub fn local_device(&self) -> Device {
        let name = if self.cfg.general.audio_device.is_empty() { "This computer".to_string() } else { self.cfg.general.audio_device.clone() };
        Device { id: None, name, kind: "Computer".into(), volume_percent: self.pb.volume, is_active: true, is_restricted: false, supports_volume: true }
    }

    fn save_local_session(&mut self) {
        let Some(off) = self.offline.as_ref() else { return };
        let Some(t) = self.pb.track.as_ref() else {
            // Nothing playing: keep the previous resume point.
            return;
        };
        let Some(rel) = rel_of(&t.uri) else { return };
        let pos = self.position();
        let s = Session {
            track: Some(rel.to_string()),
            // Near the end counts as finished: resume at the start next time.
            position_ms: if t.duration_ms > 0 && pos + 5000 > t.duration_ms { 0 } else { pos },
            queue: self.queue.items.iter().filter_map(|t| rel_of(&t.uri).map(String::from)).collect(),
            context: off.order.iter().filter_map(|&i| off.ctx.get(i)).filter_map(|t| rel_of(&t.uri).map(String::from)).collect(),
            context_label: off.ctx_label.clone(),
            context_uri: off.ctx_uri.clone(),
            shuffle: self.pb.shuffle,
            repeat: self.pb.repeat.clone(),
            volume: self.pb.volume,
        };
        if let Err(e) = off.store.save_session(&s) {
            log::warn!("couldn't save where you left off: {e:#}");
        }
    }

    // ----------------------------------------------------------- scanning

    pub fn rescan(&mut self) {
        let Some(off) = self.offline.as_mut() else { return };
        off.scanning = Some((0, 0));
        let root = off.root.clone();
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || {
            let index = Store::load_index(&root);
            let progress = |done: usize, total: usize| {
                let _ = tx.send(AppEvent::Offline(OfflineEvent::Progress { root: root.clone(), done, total }));
            };
            let result = library::scan(&root, index, &progress);
            if result.index_changed
                && let Err(e) = Store::save_index(&root, &result.index)
            {
                log::warn!("couldn't save the scan cache: {e:#}");
            }
            let _ = tx.send(AppEvent::Offline(OfflineEvent::Scanned { root, result: Box::new(result) }));
        });
    }

    pub(super) fn on_offline_event(&mut self, ev: OfflineEvent) {
        match ev {
            OfflineEvent::Progress { root, done, total } => {
                if let Some(off) = self.offline.as_mut().filter(|o| o.root == root) {
                    off.scanning = Some((done, total));
                }
            }
            OfflineEvent::Scanned { root, result } => {
                let Some(off) = self.offline.as_mut().filter(|o| o.root == root) else { return };
                let first = !off.scanned;
                let before = off.lib.entries.len();
                off.lib = result.library;
                off.scanning = None;
                off.scanned = true;
                let n = off.lib.entries.len();
                // Keep the resume point only if its song is still here.
                if off.resume.as_ref().and_then(|s| s.track.as_deref()).is_some_and(|r| off.lib.entry(r).is_none()) {
                    off.resume = None;
                }
                if first {
                    if let Some(s) = off.resume.clone() {
                        let q: Vec<Track> = s.queue.iter().filter_map(|r| off.lib.track(r).cloned()).collect();
                        self.queue.items = q;
                    }
                    if n == 0 {
                        self.toast(ToastKind::Warn, "No music found in this folder yet — add some and run `rescan`");
                    }
                } else {
                    let diff = n as i64 - before as i64;
                    let msg = match diff {
                        0 => format!("Library up to date — {n} songs"),
                        d if d > 0 => format!("Found {d} new song{} — {n} in total", if d == 1 { "" } else { "s" }),
                        d => format!("{} song{} gone — {n} in total", -d, if d == -1 { "" } else { "s" }),
                    };
                    self.toast(ToastKind::Success, msg);
                }
                self.rebuild_home(!first);
                if self.search_open {
                    self.search.query.clear();
                    self.run_search(false);
                }
            }
            OfflineEvent::Engine(e) => self.on_engine_event(e),
        }
    }

    fn on_engine_event(&mut self, e: EngineEvent) {
        let Some(off) = self.offline.as_mut() else { return };
        match e {
            EngineEvent::Loaded { load_id, duration_ms } if load_id == off.load_id => {
                off.failures = 0;
                if let Some(t) = self.pb.track.as_mut()
                    && t.duration_ms == 0
                {
                    t.duration_ms = duration_ms;
                }
            }
            EngineEvent::Ended { load_id } if load_id == off.load_id => {
                if self.pb.repeat == "track" {
                    if let Some(t) = self.pb.track.clone() {
                        self.off_load(t, 0, true);
                    }
                } else {
                    self.off_next(false);
                }
            }
            EngineEvent::Error { load_id, msg } if load_id == off.load_id => {
                off.failures += 1;
                let failures = off.failures;
                let name = self.pb.track.as_ref().map(|t| t.name.clone()).unwrap_or_default();
                self.toast(ToastKind::Error, format!("Can't play “{name}”: {msg}"));
                if failures < 5 && self.pb.is_playing {
                    self.off_next(false);
                } else {
                    self.off_stop();
                }
            }
            _ => {}
        }
    }

    /// Per-frame bookkeeping in local mode.
    pub(super) fn offline_tick(&mut self) {
        let Some(off) = self.offline.as_mut() else { return };
        if let (Some(e), Some(t)) = (&off.engine, &self.pb.track) {
            // The engine's clock is exact; follow it.
            let pos = if Instant::now() >= self.pb.hold_until { e.position_ms() } else { self.pb.progress_ms };
            // A play counts after half the song or 30 seconds, like scrobbling.
            let need = if t.duration_ms > 0 { (t.duration_ms / 2).min(30_000) } else { 30_000 };
            if !off.counted && pos >= need {
                off.counted = true;
                if let Some(rel) = rel_of(&t.uri) {
                    off.store.record_play(rel);
                }
            }
            if Instant::now() >= self.pb.hold_until {
                self.pb.set_position(pos);
            }
        }
        if let Some(err) = off.store.flush(false) {
            self.toast(ToastKind::Error, format!("Couldn't save the library: {err}"));
        }
    }

    // ----------------------------------------------------------- playback

    fn engine_ready(&mut self) -> bool {
        match self.offline.as_ref() {
            Some(o) if o.engine.is_some() => true,
            Some(o) => {
                let e = o.engine_error.clone().unwrap_or_default();
                self.toast(ToastKind::Error, format!("Local playback is unavailable: {e}"));
                false
            }
            None => false,
        }
    }

    /// Play `tracks` starting at `start`, as one context (album, list, playlist…).
    pub fn off_play_list(&mut self, tracks: Vec<Track>, start: usize, label: String, uri: Option<String>) {
        if tracks.is_empty() || !self.engine_ready() {
            return;
        }
        let shuffle = self.pb.shuffle;
        let Some(off) = self.offline.as_mut() else { return };
        let start = start.min(tracks.len() - 1);
        off.order = if shuffle { shuffled(tracks.len(), start) } else { (0..tracks.len()).collect() };
        off.pos = off.order.iter().position(|&i| i == start).unwrap_or(0);
        off.ctx = tracks;
        off.ctx_label = label;
        off.ctx_uri = uri;
        let t = off.ctx[start].clone();
        self.osd(self.theme.icon(crate::theme::Icon::Play), t.name.clone(), None);
        self.off_load(t, 0, true);
        if self.cfg.search.close_on_play && self.search_open {
            self.close_search();
        }
    }

    /// Load a song into the engine and make it current.
    pub fn off_load(&mut self, t: Track, start_ms: u64, play: bool) {
        let Some(off) = self.offline.as_mut() else { return };
        let Some(engine) = off.engine.as_ref() else { return };
        let Some(rel) = rel_of(&t.uri) else { return };
        off.load_id = engine.load(off.lib.path_of(rel), start_ms, play);
        off.counted = false;
        off.resume = None;
        let liked = off.store.is_liked(rel);
        let ctx_uri = off.ctx_uri.clone();
        self.pb.track = Some(t);
        self.pb.is_playing = play;
        self.pb.set_position(start_ms);
        self.pb.hold(150);
        self.pb.context_uri = ctx_uri;
        self.on_track_change();
        self.saved_current = Some(liked);
        self.refresh_upcoming();
    }

    /// Play `t` right away, keeping the current context for afterwards.
    pub fn off_play_now(&mut self, t: Track) {
        if !self.engine_ready() {
            return;
        }
        if self.offline.as_ref().is_some_and(|o| o.ctx.is_empty()) {
            self.off_play_list(vec![t], 0, "Queue".into(), None);
            return;
        }
        self.off_load(t, 0, true);
    }

    fn off_stop(&mut self) {
        if let Some(e) = self.offline.as_ref().and_then(|o| o.engine.as_ref()) {
            e.stop();
        }
        self.pb.track = None;
        self.pb.is_playing = false;
        self.pb.set_position(0);
        self.saved_current = None;
        self.track_changed_at = Instant::now();
        self.needs_clear = true;
        self.queue.spotify.clear();
        self.rebuild_home(false);
    }

    pub(super) fn off_resume(&mut self) {
        if self.pb.track.is_some() {
            if let Some(e) = self.offline.as_ref().and_then(|o| o.engine.as_ref()) {
                e.play();
            }
            return;
        }
        // Nothing loaded: continue where you left off, or play the list on screen.
        self.pb.is_playing = false;
        if self.resume_session() {
            return;
        }
        let items = self.offline.as_ref().map(|o| o.home.items.clone()).unwrap_or_default();
        let tracks: Vec<Track> = items.into_iter().filter_map(|i| if let Item::Track(t) = i { Some(t) } else { None }).collect();
        if tracks.is_empty() {
            self.toast(ToastKind::Info, "Pick a song to play");
            return;
        }
        let label = self.offline.as_ref().map(|o| o.name.clone()).unwrap_or_default();
        self.off_play_list(tracks, 0, label, None);
    }

    pub(super) fn off_pause(&mut self) {
        if let Some(e) = self.offline.as_ref().and_then(|o| o.engine.as_ref()) {
            e.pause();
        }
    }

    /// Continue the saved session. Returns false when there's none.
    pub fn resume_session(&mut self) -> bool {
        let Some(off) = self.offline.as_mut() else { return false };
        let Some(s) = off.resume.take() else { return false };
        let Some(rel) = s.track.clone() else { return false };
        let Some(track) = off.lib.track(&rel).cloned() else { return false };
        let mut ctx: Vec<Track> = s.context.iter().filter_map(|r| off.lib.track(r).cloned()).collect();
        if ctx.is_empty() {
            ctx.push(track.clone());
        }
        let pos = ctx.iter().position(|t| t.uri == track.uri).unwrap_or(0);
        // The saved context is already in play order.
        off.order = (0..ctx.len()).collect();
        off.pos = pos;
        off.ctx = ctx;
        off.ctx_label = s.context_label.clone();
        off.ctx_uri = s.context_uri.clone();
        if !self.engine_ready() {
            return true;
        }
        self.osd(self.theme.icon(crate::theme::Icon::Play), format!("Resuming “{}”", track.name), None);
        self.off_load(track, s.position_ms, true);
        true
    }

    /// Next song: the queue first, then the context. `user`: pressed next (vs. the song ended).
    pub(super) fn off_next(&mut self, user: bool) {
        if !self.queue.items.is_empty() {
            let t = self.queue.items.remove(0);
            self.off_load(t, 0, true);
            return;
        }
        let repeat_all = self.pb.repeat == "context";
        let shuffle = self.pb.shuffle;
        let Some(off) = self.offline.as_mut() else { return };
        if off.pos + 1 < off.order.len() {
            off.pos += 1;
        } else if repeat_all && !off.order.is_empty() {
            if shuffle {
                let first = off.order[0];
                off.order = shuffled(off.ctx.len(), first);
            }
            off.pos = 0;
        } else {
            let label = off.ctx_label.clone();
            self.off_stop();
            if user || !label.is_empty() {
                self.toast(ToastKind::Info, if label.is_empty() { "That was the last song".to_string() } else { format!("Finished “{label}”") });
            }
            return;
        }
        let t = off.ctx[off.order[off.pos]].clone();
        self.off_load(t, 0, true);
    }

    pub(super) fn off_previous(&mut self) {
        // Like every player: a few seconds in, "previous" restarts the song.
        if self.position() > 3000 || self.pb.track.is_none() {
            self.seek_to(0);
            return;
        }
        let repeat_all = self.pb.repeat == "context";
        let Some(off) = self.offline.as_mut() else { return };
        if off.pos > 0 {
            off.pos -= 1;
        } else if repeat_all && !off.order.is_empty() {
            off.pos = off.order.len() - 1;
        } else {
            self.seek_to(0);
            return;
        }
        let t = off.ctx[off.order[off.pos]].clone();
        self.off_load(t, 0, true);
    }

    pub(super) fn off_seek(&mut self, ms: u64) {
        if let Some(e) = self.offline.as_ref().and_then(|o| o.engine.as_ref()) {
            e.seek(ms);
        }
    }

    pub(super) fn off_volume(&mut self, v: u32) {
        if let Some(e) = self.offline.as_ref().and_then(|o| o.engine.as_ref()) {
            e.set_volume(v);
        }
        if let Some(d) = self.pb.device.as_mut() {
            d.volume_percent = Some(v);
        }
    }

    pub(super) fn off_shuffle(&mut self, on: bool) {
        let Some(off) = self.offline.as_mut() else { return };
        if off.ctx.is_empty() {
            return;
        }
        let cur = off.order.get(off.pos).copied().unwrap_or(0);
        off.order = if on { shuffled(off.ctx.len(), cur) } else { (0..off.ctx.len()).collect() };
        off.pos = off.order.iter().position(|&i| i == cur).unwrap_or(0);
        self.refresh_upcoming();
    }

    /// The context's next songs, shown under the queue ("Then, from …").
    pub(super) fn refresh_upcoming(&mut self) {
        let Some(off) = self.offline.as_ref() else { return };
        self.queue.spotify = off.order.iter().skip(off.pos + 1).take(50).filter_map(|&i| off.ctx.get(i).cloned()).collect();
        self.queue.fetched = Some(Instant::now());
    }

    pub(super) fn off_set_liked(&mut self, track: Track, like: bool) {
        let Some(rel) = rel_of(&track.uri).map(String::from) else {
            self.toast(ToastKind::Warn, "This item can't be liked");
            return;
        };
        let Some(off) = self.offline.as_mut() else { return };
        off.store.set_liked(&rel, like);
        let is_current = self.pb.track.as_ref().is_some_and(|t| t.uri == track.uri);
        if is_current {
            self.press(Ctl::Like);
            self.saved_current = Some(like);
            if like && let Some((x, y)) = self.like_pos {
                self.burst(x, y, &['♥', '•', '♡', '·']);
            }
        }
        self.osd(
            self.theme.icon(if like { crate::theme::Icon::Heart } else { crate::theme::Icon::HeartEmpty }),
            if like { "Liked" } else { "Unliked" },
            None,
        );
        self.toast(ToastKind::Success, if like { format!("Added “{}” to Liked Songs", track.name) } else { format!("Removed “{}” from Liked Songs", track.name) });
        if self.offline.as_ref().is_some_and(|o| o.home.tab == HomeTab::Liked) {
            self.rebuild_home(false);
        }
        self.liked_changed(&track, like);
        if self.search.stack.last().is_some_and(|p| p.kind == PageKind::Liked) {
            self.search.stack.pop();
            self.off_open_liked();
        }
    }

    /// Whether a local track is liked (for list rows).
    pub fn local_liked(&self, uri: &str) -> bool {
        match (self.offline.as_ref(), rel_of(uri)) {
            (Some(o), Some(rel)) => o.store.is_liked(rel),
            _ => false,
        }
    }

    /// A song from search: its album, a local "radio" or just the song, as configured.
    pub(super) fn off_play_searched(&mut self, t: &Track) {
        let Some(off) = self.offline.as_ref() else { return };
        match self.cfg.playback.search_context {
            SearchContext::Album => {
                let album = t.album.as_ref().and_then(|a| a.id.as_deref()).and_then(|id| off.lib.album(id));
                match album {
                    Some(a) => {
                        let tracks = off.lib.tracks_of(&a.tracks);
                        let start = tracks.iter().position(|x| x.uri == t.uri).unwrap_or(0);
                        let (label, uri) = (a.album.name.clone(), Some(a.album.uri.clone()));
                        self.off_play_list(tracks, start, label, uri);
                    }
                    None => self.off_play_list(vec![t.clone()], 0, t.name.clone(), None),
                }
            }
            SearchContext::Radio => self.off_play_radio(t),
            SearchContext::Single => self.off_play_list(vec![t.clone()], 0, t.name.clone(), None),
        }
    }

    /// A local "radio": the song, then the same artist and genre mixed with
    /// your most played and liked songs.
    pub(super) fn off_play_radio(&mut self, seed: &Track) {
        use rand::seq::SliceRandom;
        let Some(off) = self.offline.as_ref() else { return };
        let lib = &off.lib;
        let seed_rel = rel_of(&seed.uri).unwrap_or("");
        let seed_artists: Vec<String> = seed.artists.iter().filter_map(|a| a.id.clone()).collect();
        let seed_genre = lib.entry(seed_rel).and_then(|e| e.genre.clone()).map(|g| g.to_lowercase());
        let mut rng = rand::rng();
        let mut scored: Vec<(f32, usize)> = lib
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.rel != seed_rel)
            .map(|(i, e)| {
                let mut s: f32 = rand::random_range(0.0..1.0);
                if e.track.artists.iter().any(|a| a.id.as_ref().is_some_and(|id| seed_artists.contains(id))) {
                    s += 2.0;
                }
                if seed_genre.is_some() && e.genre.as_ref().map(|g| g.to_lowercase()) == seed_genre {
                    s += 1.5;
                }
                if off.store.is_liked(&e.rel) {
                    s += 0.6;
                }
                s += (off.store.plays(&e.rel) as f32).sqrt().min(3.0) * 0.3;
                (s, i)
            })
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut picks: Vec<usize> = scored.into_iter().take(49).map(|(_, i)| i).collect();
        // Don't play one artist's songs back to back for too long.
        picks.shuffle(&mut rng);
        let mut tracks = vec![seed.clone()];
        tracks.extend(lib.tracks_of(&picks));
        let label = format!("{} radio", seed.name);
        self.off_play_list(tracks, 0, label, None);
    }

    // ------------------------------------------------------------- lyrics

    /// `.lrc` next to the file, then lyrics in its tags, then LRCLIB (if allowed).
    pub(super) fn off_lyrics(&mut self, t: Track) {
        let Some(off) = self.offline.as_ref() else { return };
        let Some(rel) = rel_of(&t.uri) else { return };
        let path = off.lib.path_of(rel);
        let tagged = off.lib.entry(rel).is_some_and(|e| e.has_lyrics);
        let online = self.cfg.local.online_lyrics;
        let http = self.http.clone();
        self.spawn(async move {
            let local = tokio::task::spawn_blocking(move || {
                let lrc = path.with_extension("lrc");
                if let Some(l) = std::fs::read_to_string(&lrc).ok().and_then(|s| crate::lyrics::Lyrics::from_text(&s)) {
                    return Some(l);
                }
                if tagged {
                    return library::embedded_lyrics(&path).and_then(|s| crate::lyrics::Lyrics::from_text(&s));
                }
                None
            })
            .await
            .ok()
            .flatten();
            let result = match local {
                Some(l) => Ok(Some(l)),
                None if online => crate::lyrics::fetch(&http, &t.name, &t.artist_names(), t.album_name(), t.duration_ms).await.map_err(|e| e.to_string()),
                None => Ok(None),
            };
            AppEvent::Lyrics { uri: t.uri, result }
        });
    }

    // -------------------------------------------------------------- pages

    fn off_page(&mut self, kind: PageKind, data: PageData) {
        let id = self.push_page(kind, data.title.clone(), data.image.clone());
        self.on_page(id, Ok(data));
    }

    pub(super) fn off_open_album(&mut self, album_id: &str) {
        let Some(off) = self.offline.as_ref() else { return };
        let Some(a) = off.lib.album(album_id) else {
            self.toast(ToastKind::Warn, "That album isn't in the library anymore");
            return;
        };
        let tracks = off.lib.tracks_of(&a.tracks);
        let album = a.album.clone();
        let mut meta = format!(
            "{} · {}{} · {}",
            cap_first(album.album_type.as_deref().unwrap_or("album")),
            album.year().map(|y| format!("{y} · ")).unwrap_or_default(),
            plural(tracks.len(), "song"),
            fmt_long(a.duration_ms)
        );
        if let Some(g) = &a.genre {
            meta.push_str(&format!(" · {g}"));
        }
        let mut items: Vec<Item> = tracks.into_iter().map(Item::Track).collect();
        play_buttons(&mut items, true);
        let data = PageData {
            title: album.name.clone(),
            subtitle: album.artists.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", "),
            meta,
            image: best_image(&album.images, 300),
            context_uri: Some(album.uri.clone()),
            items,
            next: None,
            album: Some(album),
        };
        self.off_page(PageKind::Album, data);
    }

    pub(super) fn off_open_artist(&mut self, artist_id: &str) {
        let Some(off) = self.offline.as_ref() else { return };
        let Some(a) = off.lib.artist(artist_id) else {
            self.toast(ToastKind::Warn, "That artist isn't in the library anymore");
            return;
        };
        let mut items = Vec::new();
        // Your most played first, like an artist's "Popular" songs.
        let mut top: Vec<usize> = a.tracks.iter().copied().filter(|&i| off.store.plays(&off.lib.entries[i].rel) > 0).collect();
        top.sort_by_key(|&i| std::cmp::Reverse(off.store.plays(&off.lib.entries[i].rel)));
        top.truncate(5);
        if !top.is_empty() && a.tracks.len() > 8 {
            items.push(Item::Header("Most played".into()));
            items.extend(off.lib.tracks_of(&top).into_iter().map(Item::Track));
        }
        if !a.albums.is_empty() {
            items.push(Item::Header("Albums".into()));
            items.extend(a.albums.iter().map(|&al| Item::Album(off.lib.albums[al].album.clone())));
        }
        let mut songs = a.tracks.clone();
        songs.sort_by_key(|&i| {
            let e = &off.lib.entries[i];
            let year = e.album.and_then(|al| off.lib.albums[al].album.release_date.clone()).unwrap_or_default();
            (year, e.album, e.disc, e.track.track_number.unwrap_or(u32::MAX), e.track.name.to_lowercase())
        });
        items.push(Item::Header(format!("Songs ({})", songs.len())));
        items.extend(off.lib.tracks_of(&songs).into_iter().map(Item::Track));
        play_buttons(&mut items, true);
        let ms: u64 = songs.iter().map(|&i| off.lib.entries[i].track.duration_ms).sum();
        let artist = a.artist.clone();
        let data = PageData {
            title: artist.name.clone(),
            subtitle: if artist.genres.is_empty() { "Artist".into() } else { artist.genres.join(" · ") },
            meta: format!("{} · {} · {}", plural(songs.len(), "song"), plural(a.albums.len(), "album"), fmt_long(ms)),
            image: best_image(&artist.images, 300),
            context_uri: Some(artist.uri.clone()),
            items,
            next: None,
            album: None,
        };
        self.off_page(PageKind::Artist, data);
    }

    pub(super) fn off_open_playlist(&mut self, id: &str) {
        let Some(off) = self.offline.as_ref() else { return };
        let Some(p) = off.store.playlist(id) else {
            self.toast(ToastKind::Warn, "That playlist doesn't exist anymore");
            return;
        };
        let tracks: Vec<Track> = p.tracks.iter().filter_map(|r| off.lib.track(r).cloned()).collect();
        let missing = p.tracks.len() - tracks.len();
        let ms: u64 = tracks.iter().map(|t| t.duration_ms).sum();
        let mut items: Vec<Item> = tracks.into_iter().map(Item::Track).collect();
        let has = !items.is_empty();
        play_buttons(&mut items, has);
        if !has {
            items.push(Item::Header("Empty — add songs with `addto <playlist>` (the selected or current song)".into()));
        }
        if missing > 0 {
            items.push(Item::Header(format!("{missing} song{} not found in the folder", if missing == 1 { "" } else { "s" })));
        }
        let model = crate::local::playlist_model(p);
        let data = PageData {
            title: p.name.clone(),
            subtitle: "Local playlist · Del removes a song".into(),
            meta: format!("Playlist · {} · {}", plural(p.tracks.len() - missing, "song"), fmt_long(ms)),
            image: None,
            context_uri: Some(model.uri),
            items,
            next: None,
            album: None,
        };
        self.off_page(PageKind::Playlist, data);
    }

    pub(super) fn liked_tracks(&self) -> Vec<Track> {
        let Some(off) = self.offline.as_ref() else { return Vec::new() };
        let mut liked: Vec<(&String, &u64)> = off.store.data.liked.iter().collect();
        liked.sort_by_key(|(_, at)| std::cmp::Reverse(**at));
        liked.into_iter().filter_map(|(r, _)| off.lib.track(r).cloned()).collect()
    }

    fn played_tracks(&self, by_count: bool) -> Vec<Track> {
        let Some(off) = self.offline.as_ref() else { return Vec::new() };
        let mut plays: Vec<(&String, &store::Plays)> = off.store.data.plays.iter().collect();
        if by_count {
            plays.sort_by_key(|(_, p)| (std::cmp::Reverse(p.count), std::cmp::Reverse(p.last)));
        } else {
            plays.sort_by_key(|(_, p)| std::cmp::Reverse(p.last));
        }
        plays.into_iter().filter_map(|(r, _)| off.lib.track(r).cloned()).take(100).collect()
    }

    fn collection_page(&mut self, kind: PageKind, title: &str, subtitle: &str, tracks: Vec<Track>, empty: &str) {
        let n = tracks.len();
        let ms: u64 = tracks.iter().map(|t| t.duration_ms).sum();
        let mut items: Vec<Item> = tracks.into_iter().map(Item::Track).collect();
        play_buttons(&mut items, n > 0);
        if n == 0 {
            items.push(Item::Header(empty.into()));
        }
        let data = PageData {
            title: title.into(),
            subtitle: subtitle.into(),
            meta: if n == 0 { String::new() } else { format!("{} · {}", plural(n, "song"), fmt_long(ms)) },
            image: None,
            context_uri: None,
            items,
            next: None,
            album: None,
        };
        self.off_page(kind, data);
    }

    pub(super) fn off_open_liked(&mut self) {
        let t = self.liked_tracks();
        self.collection_page(PageKind::Liked, "Liked Songs", "Your favorites in this library", t, "No liked songs yet — press * on a song");
    }

    pub(super) fn off_open_recent(&mut self) {
        let t = self.played_tracks(false);
        self.collection_page(PageKind::Recent, "Recently Played", "Pick up where you left off", t, "Nothing played yet");
    }

    pub(super) fn off_open_top(&mut self) {
        let t = self.played_tracks(true);
        self.collection_page(PageKind::Top, "Most Played", "Your favorites by the numbers", t, "Nothing played yet");
    }

    pub(super) fn off_open_songs(&mut self) {
        let t = self.sorted_songs();
        let label = format!("All songs · sorted by {}", self.cfg.local.sort.label().to_lowercase());
        self.collection_page(PageKind::Songs, "All Songs", &label, t, "No music found in this folder");
    }

    pub(super) fn off_open_library(&mut self, only_playlists: bool) {
        let Some(off) = self.offline.as_ref() else { return };
        let mut items = Vec::new();
        if !only_playlists {
            items.push(Item::Header("Collections".into()));
            items.push(Item::Link { title: "All Songs".into(), subtitle: plural(off.lib.entries.len(), "song"), target: LinkTarget::AllSongs });
            items.push(Item::Link { title: "Liked Songs".into(), subtitle: plural(off.store.data.liked.len(), "song"), target: LinkTarget::LikedSongs });
            items.push(Item::Link { title: "Recently Played".into(), subtitle: "History".into(), target: LinkTarget::Recent });
            items.push(Item::Link { title: "Most Played".into(), subtitle: "Your top songs".into(), target: LinkTarget::TopTracks });
        }
        let playlists: Vec<Item> = off.store.data.playlists.iter().map(|p| Item::Playlist(crate::local::playlist_model(p))).collect();
        items.push(Item::Header("Playlists".into()));
        if playlists.is_empty() {
            items.push(Item::Header("None yet — `playlist new <name>` creates one".into()));
        }
        items.extend(playlists);
        if !only_playlists {
            if !off.lib.albums.is_empty() {
                items.push(Item::Header("Albums".into()));
                items.extend(self.sorted_albums().into_iter().map(Item::Album));
            }
            if !off.lib.artists.is_empty() {
                items.push(Item::Header("Artists".into()));
                items.extend(self.sorted_artists().into_iter().map(Item::Artist));
            }
        }
        let Some(off) = self.offline.as_ref() else { return };
        let title = if only_playlists { "Playlists" } else { "Your Library" };
        let data = PageData {
            title: title.into(),
            subtitle: off.root.display().to_string(),
            meta: format!(
                "{} · {} · {} · {}",
                plural(off.lib.entries.len(), "song"),
                plural(off.lib.albums.len(), "album"),
                plural(off.lib.artists.len(), "artist"),
                fmt_long(off.lib.total_ms)
            ),
            image: None,
            context_uri: None,
            items,
            next: None,
            album: None,
        };
        self.off_page(PageKind::Library, data);
    }

    // --------------------------------------------------------------- lists

    pub fn sorted_songs(&self) -> Vec<Track> {
        let Some(off) = self.offline.as_ref() else { return Vec::new() };
        let lib = &off.lib;
        let mut idx: Vec<usize> = (0..lib.entries.len()).collect();
        let lower = |s: &str| library::norm(s);
        let album_key = |i: usize| {
            let e = &lib.entries[i];
            (e.album, e.disc, e.track.track_number.unwrap_or(u32::MAX))
        };
        match self.cfg.local.sort {
            SongSort::Artist => idx.sort_by_cached_key(|&i| {
                let e = &lib.entries[i];
                let artist = e.track.album.as_ref().and_then(|a| a.artists.first()).map(|a| a.name.as_str()).unwrap_or_else(|| e.track.artists[0].name.as_str());
                let year = e.album.and_then(|al| lib.albums[al].album.release_date.clone()).unwrap_or_default();
                (lower(artist), year, album_key(i), lower(&e.track.name))
            }),
            SongSort::Title => idx.sort_by_cached_key(|&i| lower(&lib.entries[i].track.name)),
            SongSort::Album => idx.sort_by_cached_key(|&i| (lower(lib.entries[i].track.album_name()), album_key(i))),
            SongSort::Added => idx.sort_by_cached_key(|&i| (std::cmp::Reverse(lib.entries[i].added), album_key(i))),
            SongSort::Plays => idx.sort_by_cached_key(|&i| (std::cmp::Reverse(off.store.plays(&lib.entries[i].rel)), lower(&lib.entries[i].track.name))),
            SongSort::Duration => idx.sort_by_cached_key(|&i| std::cmp::Reverse(lib.entries[i].track.duration_ms)),
        }
        lib.tracks_of(&idx)
    }

    fn sorted_albums(&self) -> Vec<Album> {
        let Some(off) = self.offline.as_ref() else { return Vec::new() };
        let mut v: Vec<&library::AlbumInfo> = off.lib.albums.iter().collect();
        v.sort_by_cached_key(|a| (library::norm(a.album.artists.first().map(|x| x.name.as_str()).unwrap_or("")), a.album.release_date.clone(), library::norm(&a.album.name)));
        v.into_iter().map(|a| a.album.clone()).collect()
    }

    fn sorted_artists(&self) -> Vec<Artist> {
        let Some(off) = self.offline.as_ref() else { return Vec::new() };
        let mut v: Vec<&library::ArtistInfo> = off.lib.artists.iter().collect();
        v.sort_by_cached_key(|a| library::norm(&a.artist.name));
        v.into_iter().map(|a| a.artist.clone()).collect()
    }

    /// Rebuild the home list for its tab. `keep`: try to keep the selection.
    pub fn rebuild_home(&mut self, keep: bool) {
        let Some(off) = self.offline.as_ref() else { return };
        let tab = off.home.tab;
        let selected_uri = off.home.items.get(off.home.nav.selected).and_then(|i| i.uri().map(String::from));
        let mut items: Vec<Item> = Vec::new();
        match tab {
            HomeTab::Songs => {
                if let Some(s) = &off.resume
                    && let Some(t) = s.track.as_deref().and_then(|r| off.lib.track(r))
                {
                    let at = if s.position_ms > 0 { format!(" at {}", fmt_duration(s.position_ms)) } else { String::new() };
                    items.push(Item::Link { title: format!("Continue “{}”", t.name), subtitle: format!("{}{at}", t.artist_names()), target: LinkTarget::Resume });
                }
                let songs = self.sorted_songs();
                if !songs.is_empty() {
                    items.push(Item::Link { title: "Play all".into(), subtitle: String::new(), target: LinkTarget::PlayAll });
                    items.push(Item::Link { title: "Shuffle all".into(), subtitle: String::new(), target: LinkTarget::ShuffleAll });
                }
                items.extend(songs.into_iter().map(Item::Track));
            }
            HomeTab::Albums => items.extend(self.sorted_albums().into_iter().map(Item::Album)),
            HomeTab::Artists => items.extend(self.sorted_artists().into_iter().map(Item::Artist)),
            HomeTab::Liked => {
                let liked = self.liked_tracks();
                if !liked.is_empty() {
                    items.push(Item::Link { title: "Play all".into(), subtitle: String::new(), target: LinkTarget::PlayAll });
                    items.push(Item::Link { title: "Shuffle all".into(), subtitle: String::new(), target: LinkTarget::ShuffleAll });
                }
                items.extend(liked.into_iter().map(Item::Track));
            }
            HomeTab::Playlists => items.extend(off.store.data.playlists.iter().map(|p| Item::Playlist(crate::local::playlist_model(p)))),
            HomeTab::Recent => items.extend(self.played_tracks(false).into_iter().map(Item::Track)),
        }
        let Some(off) = self.offline.as_mut() else { return };
        let mut nav = if keep { off.home.nav } else { ListNav::default() };
        if let Some(uri) = selected_uri.filter(|_| keep)
            && let Some(i) = items.iter().position(|it| it.uri() == Some(uri.as_str()))
        {
            nav.selected = i;
        }
        nav.fix(&items);
        if !keep {
            // Land on the first song rather than the play buttons.
            if let Some(i) = items.iter().position(|i| matches!(i, Item::Track(_))).filter(|_| matches!(items.first(), Some(Item::Link { target: LinkTarget::PlayAll, .. }))) {
                nav.selected = i;
            }
            off.home.shown = Instant::now();
        }
        off.home.items = items;
        off.home.nav = nav;
    }

    pub fn set_home_tab(&mut self, tab: HomeTab) {
        let Some(off) = self.offline.as_mut() else { return };
        if off.home.tab == tab {
            return;
        }
        off.home.tab = tab;
        self.rebuild_home(false);
    }

    pub fn home_selected(&self) -> Option<&Item> {
        let off = self.offline.as_ref()?;
        off.home.items.get(off.home.nav.selected)
    }

    /// Enter on the home list.
    pub fn activate_home(&mut self, i: usize) {
        let Some(off) = self.offline.as_ref() else { return };
        let Some(item) = off.home.items.get(i).cloned() else { return };
        let tracks: Vec<Track> = off.home.items.iter().filter_map(|i| if let Item::Track(t) = i { Some(t.clone()) } else { None }).collect();
        let label = match off.home.tab {
            HomeTab::Liked => "Liked Songs".to_string(),
            HomeTab::Recent => "Recently Played".to_string(),
            _ => off.name.clone(),
        };
        match item {
            Item::Track(t) => {
                let start = tracks.iter().position(|x| x.uri == t.uri).unwrap_or(0);
                self.off_play_list(tracks, start, label, None);
            }
            Item::Link { target: LinkTarget::Resume, .. } => {
                self.resume_session();
            }
            Item::Link { target: target @ (LinkTarget::PlayAll | LinkTarget::ShuffleAll), .. } => {
                let shuffle = target == LinkTarget::ShuffleAll;
                self.pb.shuffle = shuffle;
                let start = if shuffle { rand::random_range(0..tracks.len().max(1)) } else { 0 };
                self.off_play_list(tracks, start, label, None);
            }
            other => self.activate_item(other),
        }
    }

    pub fn home_step(&mut self, delta: i32) {
        if let Some(off) = self.offline.as_mut() {
            let items = std::mem::take(&mut off.home.items);
            off.home.nav.step(delta, &items);
            off.home.items = items;
        }
    }

    pub fn home_jump(&mut self, last: bool) {
        if let Some(off) = self.offline.as_mut() {
            let items = std::mem::take(&mut off.home.items);
            if last {
                off.home.nav.last(&items);
            } else {
                off.home.nav.first(&items);
            }
            off.home.items = items;
        }
    }

    // ------------------------------------------------------------- search

    /// Everything in the library (empty query) or what matches the query.
    pub(super) fn off_search(&mut self, q: &str) -> SearchResults {
        let Some(off) = self.offline.as_ref() else { return SearchResults::default() };
        if q.trim().is_empty() {
            let tracks = self.sorted_songs();
            let artists = self.sorted_artists();
            let albums = self.sorted_albums();
            let playlists: Vec<Playlist> = off.store.data.playlists.iter().map(crate::local::playlist_model).collect();
            let totals = [tracks.len() as u32, artists.len() as u32, albums.len() as u32, playlists.len() as u32];
            return SearchResults { tracks, artists, albums, playlists, totals, top: None };
        }
        crate::local::search(&off.lib, &off.store, q)
    }

    pub(super) fn offline_play_query(&mut self, q: &str) -> Result<Option<String>, String> {
        let r = self.off_search(q);
        let t = match r.top {
            Some(Item::Track(t)) => t,
            _ => r.tracks.into_iter().next().ok_or_else(|| format!("No song matching “{q}” in this library"))?,
        };
        let msg = format!("Playing “{}” by {}", t.name, t.artist_names());
        self.off_play_searched(&t);
        Ok(Some(msg))
    }

    pub(super) fn offline_queue_query(&mut self, q: &str) -> Result<Option<String>, String> {
        let r = self.off_search(q);
        let t = r.tracks.into_iter().next().ok_or_else(|| format!("No song matching “{q}” in this library"))?;
        self.enqueue(t);
        Ok(None)
    }

    // ----------------------------------------------------------- commands

    /// The song a command applies to: the selected row, else the current song.
    pub fn target_track(&self) -> Option<Track> {
        let selected = if self.search_open {
            self.selected_item()
        } else if self.home_active() {
            self.home_selected()
        } else {
            None
        };
        match selected {
            Some(Item::Track(t)) => Some(t.clone()),
            _ => self.pb.track.clone(),
        }
    }

    pub(super) fn offline_command(&mut self, name: &str, arg: &str) -> Option<Result<Option<String>, String>> {
        let r = match name {
            "rescan" | "refresh" => {
                self.rescan();
                Ok(Some("Looking for new or changed files…".into()))
            }
            "sort" => {
                self.cfg.local.sort = if arg.is_empty() {
                    self.cfg.local.sort.cycle(1)
                } else {
                    match arg {
                        "length" | "time" | "duration" => SongSort::Duration,
                        "added" | "recent" | "new" | "date" => SongSort::Added,
                        "plays" | "played" | "popular" => SongSort::Plays,
                        "name" => SongSort::Title,
                        other => match SongSort::from_id(other) {
                            Some(s) => s,
                            None => return Some(Err("Usage: sort <artist | title | album | added | plays | length>".into())),
                        },
                    }
                };
                self.save_config();
                self.local_resort();
                Ok(Some(format!("Songs sorted by {}", self.cfg.local.sort.label().to_lowercase())))
            }
            "playlist" => self.playlist_command(arg),
            "addto" => self.local_addto(arg),
            "share" => match self.pb.track.clone().and_then(|t| self.local_path(&t)) {
                Some(path) => {
                    super::exec::osc52_copy(&path.display().to_string());
                    Ok(Some(format!("Copied {}", path.display())))
                }
                None => Err("Nothing playing".into()),
            },
            "open" => match self.target_track().and_then(|t| self.local_path(&t)) {
                Some(path) => reveal(&path).map(|_| Some("Shown in your file manager".into())).map_err(|e| e.to_string()),
                None => Err("Nothing selected".into()),
            },
            "follow" | "unfollow" | "savealbum" | "login" | "logout" | "whoami" | "player" | "onboarding" => {
                Err(format!("`{name}` needs Spotify — `online` switches back"))
            }
            _ => return None,
        };
        Some(r)
    }

    fn local_path(&self, t: &Track) -> Option<PathBuf> {
        let off = self.offline.as_ref()?;
        rel_of(&t.uri).map(|r| off.lib.path_of(r))
    }

    /// Re-sort the lists after the sort order changed.
    pub fn local_resort(&mut self) {
        if self.offline.is_none() {
            return;
        }
        self.rebuild_home(true);
        if self.search_open && self.search.query.is_empty() {
            self.run_search(false);
        }
    }

    fn playlist_command(&mut self, arg: &str) -> Result<Option<String>, String> {
        let (sub, rest) = crate::commands::split(arg);
        let rest = rest.trim();
        let Some(off) = self.offline.as_mut() else { return Err("Playlists here are for local libraries".into()) };
        let r = match sub {
            "new" | "create" | "add" => {
                if rest.is_empty() {
                    return Err("Usage: playlist new <name>".into());
                }
                off.store.create_playlist(rest);
                Ok(Some(format!("Created playlist “{rest}” — `addto {rest}` adds the selected song")))
            }
            "delete" | "remove" | "rm" => {
                let id = off.store.find_playlist(rest).map(|p| p.id.clone()).ok_or_else(|| format!("No playlist matching “{rest}”"))?;
                let p = off.store.delete_playlist(&id).ok_or("Playlist not found")?;
                Ok(Some(format!("Deleted “{}”", p.name)))
            }
            "rename" => {
                let (old, new) = rest.split_once(" to ").or_else(|| rest.split_once(" > ")).ok_or("Usage: playlist rename <name> to <new name>")?;
                let id = off.store.find_playlist(old).map(|p| p.id.clone()).ok_or_else(|| format!("No playlist matching “{old}”"))?;
                if let Some(p) = off.store.playlist_mut(&id) {
                    p.name = new.trim().to_string();
                }
                off.store.touch();
                Ok(Some(format!("Renamed to “{}”", new.trim())))
            }
            "" | "list" => {
                let names: Vec<String> = off.store.data.playlists.iter().map(|p| p.name.clone()).collect();
                if names.is_empty() { Ok(Some("No playlists yet — `playlist new <name>`".into())) } else { Ok(Some(names.join(" · "))) }
            }
            _ => Err("Usage: playlist <new | delete | rename> <name>".into()),
        };
        if self.offline.as_ref().is_some_and(|o| o.home.tab == HomeTab::Playlists) {
            self.rebuild_home(true);
        }
        r
    }

    fn local_addto(&mut self, arg: &str) -> Result<Option<String>, String> {
        if arg.is_empty() {
            return Err("Usage: addto <playlist>".into());
        }
        let t = self.target_track().ok_or("Nothing selected")?;
        let rel = rel_of(&t.uri).ok_or("Only songs from this folder can be added")?.to_string();
        let off = self.offline.as_mut().ok_or("Not in a local library")?;
        let id = off.store.find_playlist(arg).map(|p| p.id.clone()).ok_or_else(|| format!("No playlist “{arg}” — `playlist new {arg}` creates it"))?;
        let p = off.store.playlist_mut(&id).ok_or("Playlist not found")?;
        if p.tracks.contains(&rel) {
            return Ok(Some(format!("“{}” is already in {}", t.name, p.name)));
        }
        p.tracks.push(rel);
        let name = p.name.clone();
        off.store.touch();
        Ok(Some(format!("Added “{}” to {name}", t.name)))
    }

    /// Del on a local playlist page removes the selected song from it.
    pub fn remove_from_local_playlist(&mut self) -> bool {
        let Some(page) = self.search.stack.last() else { return false };
        let Some(id) = page.context_uri.as_deref().and_then(|u| u.strip_prefix(library::PLAYLIST_PREFIX)).map(String::from) else { return false };
        let Some(Item::Track(t)) = page.items.get(page.list.selected).cloned() else { return true };
        let Some(off) = self.offline.as_mut() else { return false };
        let Some(rel) = rel_of(&t.uri) else { return true };
        if let Some(p) = off.store.playlist_mut(&id) {
            p.tracks.retain(|r| r != rel);
        }
        off.store.touch();
        let sel = page.list.selected;
        self.search.stack.pop();
        self.off_open_playlist(&id);
        if let Some(p) = self.search.stack.last_mut() {
            p.list.selected = sel.min(p.items.len().saturating_sub(1));
            p.list.fix(&p.items);
        }
        self.toast(ToastKind::Info, format!("Removed “{}” from the playlist", t.name));
        true
    }

    /// A cover the OS can show for a local song.
    pub fn local_media_cover(&self, url: &str) -> Option<String> {
        if let Some(path) = url.strip_prefix(library::FILE_SCHEME) {
            return Some(format!("file://{path}"));
        }
        let path = url.strip_prefix(library::ART_SCHEME)?;
        let dir = self.paths.cache_dir.join("media");
        let key = {
            use sha2::Digest;
            let h = sha2::Sha256::digest(path.as_bytes());
            h.iter().take(8).map(|b| format!("{b:02x}")).collect::<String>()
        };
        let out = dir.join(format!("local-{key}.img"));
        if !out.exists() {
            std::fs::create_dir_all(&dir).ok()?;
            let bytes = library::embedded_art(Path::new(path))?;
            std::fs::write(&out, bytes).ok()?;
        }
        Some(format!("file://{}", out.display()))
    }
}

/// Show a file in Finder / the file manager.
fn reveal(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg("-R").arg(path).spawn().map(|_| ())
    }
    #[cfg(not(target_os = "macos"))]
    {
        open::that_detached(path.parent().unwrap_or(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shuffle_starts_with_the_chosen_song() {
        for first in 0..5 {
            let o = shuffled(5, first);
            assert_eq!(o[0], first);
            let mut s = o.clone();
            s.sort();
            assert_eq!(s, vec![0, 1, 2, 3, 4]);
        }
        assert!(shuffled(0, 0).is_empty());
    }

    #[test]
    fn home_tabs_cycle() {
        assert_eq!(HomeTab::Songs.cycle(-1), HomeTab::Recent);
        assert_eq!(HomeTab::Recent.cycle(1), HomeTab::Songs);
        // Liked Songs sits right before Recent.
        assert_eq!(HomeTab::Recent.cycle(-1), HomeTab::Liked);
    }
}
