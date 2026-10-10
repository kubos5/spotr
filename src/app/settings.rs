//! The settings panel: a declarative list of options per tab.

use crossterm::event::{KeyCode, KeyEvent};

use super::*;
use crate::config::*;
use crate::keys::Chord;
use crate::textinput::TextInput;

pub const TABS: &[&str] = &["Look", "Motion", "Visualizer", "Playback", "Search", "Player", "Keys", "Account"];

#[derive(Default)]
pub struct SettingsState {
    pub tab: usize,
    pub selected: usize,
    pub scroll: usize,
    /// Waiting for a key press to bind to this action.
    pub capture: Option<Action>,
    pub editing: Option<TextInput>,
    pub changed_at: Option<(usize, Instant)>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Id {
    Accent,
    CoverMode,
    TextArt,
    Pixelation,
    ShowCover,
    TitleFont,
    LongTitle,
    Shimmer,
    BigControls,
    ShowShuffle,
    ShowRepeat,
    Layout,
    Icons,
    Borders,
    HighContrast,
    Hints,
    Osd,
    Mouse,
    AnimEnabled,
    AnimDuration,
    TextEffects,
    Fps,
    Splash,
    Goodbye,
    VisEnabled,
    VisStyle,
    VisColors,
    VisSource,
    VisHeight,
    VisSensitivity,
    VisPeaks,
    SeekStep,
    VolumeStep,
    SpeedStep,
    ArrowMode,
    BracketMode,
    DigitSeek,
    SearchContext,
    QueueHandoff,
    Karaoke,
    LyricsAscii,
    LyricsFont,
    LiveSearch,
    Debounce,
    ResultsPerType,
    DefaultFilter,
    TopResult,
    RememberPos,
    CloseOnPlay,
    LocalPlayer,
    DeviceName,
    AutoActivate,
    Bitrate,
    Normalisation,
    PlayerLogin,
    PollInterval,
    MediaControls,
    Key(Action),
    ResetKeys,
    SpotifyApp,
    Relogin,
    Logout,
    RerunOnboarding,
    ResetAll,
}

pub enum Kind {
    Toggle,
    Choice,
    Number,
    Text,
    Key,
    Button,
}

pub struct Row {
    pub id: Id,
    pub label: String,
    pub desc: &'static str,
    pub kind: Kind,
}

fn row(id: Id, label: &str, desc: &'static str, kind: Kind) -> Row {
    Row { id, label: label.to_string(), desc, kind }
}

pub fn rows(tab: usize) -> Vec<Row> {
    use Id::*;
    use Kind::*;
    match tab {
        0 => vec![
            row(Accent, "Accent color", "Taken from your terminal's palette, so it matches your theme.", Choice),
            row(CoverMode, "Cover art", "Image uses your terminal's graphics protocol; Text draws it with characters.", Choice),
            row(TextArt, "Text art style", "How cover art is drawn in Text mode.", Choice),
            row(Pixelation, "Pixelation", "Coarse pixel grid for text art (0 = as sharp as possible).", Number),
            row(ShowCover, "Show cover art", "Hide the cover to give the title more room.", Toggle),
            row(TitleFont, "Title font", "ASCII-art font for titles. Add your own .flf / .tlf fonts to the fonts folder next to the config.", Choice),
            row(LongTitle, "Long titles", "What to do when a title doesn't fit.", Choice),
            row(Shimmer, "Title shimmer", "A soft highlight that sweeps across the title.", Toggle),
            row(BigControls, "Big controls", "Three-row play / pause / next buttons (needs Unicode icons and some height).", Toggle),
            row(ShowShuffle, "Shuffle button", "Show the shuffle button next to the controls (Ctrl+S still works).", Toggle),
            row(ShowRepeat, "Loop button", "Show the loop / repeat button next to the controls (Ctrl+R still works).", Toggle),
            row(Layout, "Now playing layout", "Cover beside or above the song info.", Choice),
            row(Icons, "Icons", "Use ASCII if your font lacks some symbols.", Choice),
            row(Borders, "Borders", "Border style for bars and panels.", Choice),
            row(HighContrast, "High contrast", "No dim text; selections are reversed.", Toggle),
            row(Hints, "Keybind hints", "The cheat-sheet line under the command bar.", Toggle),
            row(Osd, "On-screen badges", "Brief badges for volume, seeking, speed…", Toggle),
            row(Mouse, "Mouse support", "Click, hover and scroll. Disable to select text in the terminal.", Toggle),
        ],
        1 => vec![
            row(AnimEnabled, "Animations", "Turn off for reduced motion.", Toggle),
            row(AnimDuration, "Animation length", "Multiplier on every animation's duration.", Number),
            row(TextEffects, "Text effects", "Decoding / typewriter effects on titles and messages.", Toggle),
            row(Fps, "Frame rate", "Frames per second while animating.", Number),
            row(Splash, "Startup animation", "Show the logo briefly on launch.", Toggle),
            row(Goodbye, "Closing animation", "Wipe the screen with a goodbye when quitting.", Toggle),
        ],
        2 => vec![
            row(VisEnabled, "Visualizer", "Show the audio visualizer above the command bar.", Toggle),
            row(VisStyle, "Style", "Bars, mirrored bars, waveform, LED blocks or dots.", Choice),
            row(VisColors, "Colors", "Spectrum uses several palette colors; Accent and Mono stay calm.", Choice),
            row(VisSource, "Source", "Auto uses real audio from the built-in player and simulates otherwise.", Choice),
            row(VisHeight, "Height", "Rows used by the visualizer.", Number),
            row(VisSensitivity, "Sensitivity", "Gain applied to the spectrum.", Number),
            row(VisPeaks, "Falling peaks", "Little caps that fall slowly.", Toggle),
        ],
        3 => vec![
            row(ArrowMode, "← / → keys", "Skip seconds, change song, speed or volume. Tab switches.", Choice),
            row(BracketMode, "[ / ] keys", "Same choices for the bracket keys. Shift+Tab switches.", Choice),
            row(SeekStep, "Skip amount", "Seconds skipped by the arrows and `skip`.", Number),
            row(VolumeStep, "Volume step", "Percent per ↑ / ↓.", Number),
            row(SpeedStep, "Speed step", "Speed change per press.", Number),
            row(DigitSeek, "Digits seek", "0–9 jump to 0%–90% of the song.", Toggle),
            row(SearchContext, "Songs from search", "Album: keep playing the album afterwards. Single: just that song.", Choice),
            row(QueueHandoff, "Queue handoff", "Seconds before a song ends when the next queued song is sent to Spotify.", Number),
            row(Karaoke, "Karaoke lyrics", "Fill the current lyric line as it's sung.", Toggle),
            row(LyricsAscii, "Lyrics style", "Plain text, or ASCII art in the lyrics font.", Choice),
            row(LyricsFont, "Lyrics font", "Font for ASCII-art lyrics. Compact fonts (Slim, Small Braille, ANSI Compact) fit more lines.", Choice),
        ],
        4 => vec![
            row(LiveSearch, "Search as you type", "Otherwise search when you press Enter.", Toggle),
            row(Debounce, "Typing delay", "Milliseconds to wait after typing before searching.", Number),
            row(ResultsPerType, "Results per type", "How many of each kind to fetch at a time (Spotify allows at most 10).", Number),
            row(DefaultFilter, "Default filter", "Which tab search opens on.", Choice),
            row(TopResult, "Top result card", "Show the best match as a big card.", Toggle),
            row(RememberPos, "Remember position", "Reopen search exactly where you left it.", Toggle),
            row(CloseOnPlay, "Close after playing", "Return to the player when you start a song.", Toggle),
        ],
        5 => vec![
            row(LocalPlayer, "Built-in player", "Run Spotr as a Spotify Connect device (real visualizer + speed).", Toggle),
            row(DeviceName, "Device name", "How the built-in player appears in Spotify.", Text),
            row(AutoActivate, "Auto-activate", "Make the built-in player active on start if nothing is playing.", Toggle),
            row(Bitrate, "Quality", "Streaming bitrate in kbps.", Choice),
            row(Normalisation, "Normalize volume", "Even out loudness between songs.", Toggle),
            row(PlayerLogin, "Sign in built-in player", "One-time browser sign-in, if it asks for one.", Button),
            row(PollInterval, "Sync interval", "Milliseconds between playback state refreshes.", Number),
            row(MediaControls, "OS media controls", "Show what's playing in Now Playing / MPRIS and accept media keys (applies after a restart).", Toggle),
        ],
        6 => {
            let mut v: Vec<Row> = Action::ALL
                .iter()
                .map(|a| Row { id: Id::Key(*a), label: a.label().to_string(), desc: "Enter: rebind · Del: reset to default", kind: Kind::Key })
                .collect();
            v.push(row(ResetKeys, "Reset all shortcuts", "Restore every default binding.", Button));
            v
        }
        _ => vec![
            row(SpotifyApp, "Spotify Client ID", "From developer.spotify.com — your own app.", Text),
            row(Relogin, "Sign in again", "Re-authorize Spotr with Spotify.", Button),
            row(Logout, "Sign out", "Forget the saved token.", Button),
            row(RerunOnboarding, "Run setup again", "The first-launch walkthrough.", Button),
            row(ResetAll, "Reset all settings", "Back to defaults (keeps your login).", Button),
        ],
    }
}

fn on_off(b: bool) -> String {
    if b { "on".into() } else { "off".into() }
}

impl App {
    pub fn setting_value(&self, id: Id) -> String {
        let c = &self.cfg;
        use Id::*;
        match id {
            Accent => c.appearance.accent.label().into(),
            CoverMode => {
                if c.appearance.cover_mode == crate::config::CoverMode::Image && !self.images.graphics {
                    "Image (unsupported → Text)".into()
                } else {
                    c.appearance.cover_mode.label().into()
                }
            }
            TextArt => c.appearance.text_art.label().into(),
            Pixelation => if c.appearance.pixelation == 0 { "off".into() } else { format!("{0}×{0}", c.appearance.pixelation) },
            ShowCover => on_off(c.appearance.show_cover),
            TitleFont => crate::font::label(self.title_font()).into(),
            LongTitle => c.appearance.long_title.label().into(),
            Shimmer => on_off(c.appearance.title_shimmer),
            BigControls => on_off(c.appearance.big_controls),
            ShowShuffle => on_off(c.appearance.show_shuffle),
            ShowRepeat => on_off(c.appearance.show_repeat),
            Layout => c.appearance.layout.label().into(),
            Icons => c.appearance.icons.label().into(),
            Borders => c.appearance.borders.label().into(),
            HighContrast => on_off(c.appearance.high_contrast),
            Hints => on_off(c.appearance.show_hints),
            Osd => on_off(c.appearance.show_osd),
            Mouse => on_off(c.general.mouse),
            AnimEnabled => on_off(c.animations.enabled),
            AnimDuration => format!("{:.2}×", c.animations.duration),
            TextEffects => on_off(c.animations.text_effects),
            Fps => format!("{} fps", c.animations.fps),
            Splash => on_off(c.general.splash),
            Goodbye => on_off(c.general.goodbye),
            VisEnabled => on_off(c.visualizer.enabled),
            VisStyle => c.visualizer.style.label().into(),
            VisColors => c.visualizer.colors.label().into(),
            VisSource => c.visualizer.source.label().into(),
            VisHeight => format!("{} rows", c.visualizer.height),
            VisSensitivity => format!("{:.1}×", c.visualizer.sensitivity),
            VisPeaks => on_off(c.visualizer.peaks),
            SeekStep => format!("{} s", c.playback.seek_step),
            VolumeStep => format!("{}%", c.playback.volume_step),
            SpeedStep => format!("{:.2}×", c.playback.speed_step),
            ArrowMode => c.playback.arrow_mode.label().into(),
            BracketMode => c.playback.bracket_mode.label().into(),
            DigitSeek => on_off(c.playback.digit_seek),
            SearchContext => c.playback.search_context.label().into(),
            QueueHandoff => format!("{} s", c.playback.queue_handoff),
            Karaoke => on_off(c.lyrics.karaoke),
            LyricsAscii => (if c.lyrics.ascii { "ASCII art" } else { "Text" }).into(),
            LyricsFont => {
                if c.lyrics.font.is_empty() {
                    "Same as title".into()
                } else {
                    crate::font::label(crate::font::resolve(&c.lyrics.font)).into()
                }
            }
            LiveSearch => on_off(c.search.live),
            Debounce => format!("{} ms", c.search.debounce_ms),
            ResultsPerType => c.search.results_per_type.to_string(),
            DefaultFilter => c.search.default_filter.label().into(),
            TopResult => on_off(c.search.show_top_result),
            RememberPos => on_off(c.search.remember_position),
            CloseOnPlay => on_off(c.search.close_on_play),
            LocalPlayer => {
                if !crate::player::AVAILABLE {
                    "not compiled in".into()
                } else {
                    let status = match &self.local_status {
                        LocalStatus::Off => "",
                        LocalStatus::Connecting => " · connecting…",
                        LocalStatus::Ready => " · ready",
                        LocalStatus::NeedsLogin => " · needs sign-in",
                        LocalStatus::Failed(_) => " · failed",
                    };
                    format!("{}{status}", on_off(c.general.local_player))
                }
            }
            DeviceName => c.general.device_name.clone(),
            AutoActivate => on_off(c.general.auto_activate),
            Bitrate => format!("{} kbps", c.general.bitrate),
            Normalisation => on_off(c.general.normalisation),
            PlayerLogin => "Enter ⏎".into(),
            PollInterval => format!("{} ms", c.general.poll_interval_ms),
            MediaControls => on_off(c.general.media_controls),
            Key(a) => self.keymap.hints(a, self.theme.unicode),
            ResetKeys | Relogin | Logout | RerunOnboarding | ResetAll => "Enter ⏎".into(),
            SpotifyApp => {
                let id = &c.general.client_id;
                if id.len() > 8 { format!("{}…{}", &id[..4], &id[id.len() - 4..]) } else if id.is_empty() { "not set".into() } else { id.clone() }
            }
        }
    }

    /// Left / right on a row.
    pub fn setting_adjust(&mut self, id: Id, dir: i32) {
        let c = &mut self.cfg;
        let d = dir as f32;
        use Id::*;
        match id {
            Accent => c.appearance.accent = c.appearance.accent.cycle(dir),
            CoverMode => c.appearance.cover_mode = c.appearance.cover_mode.cycle(dir),
            TextArt => c.appearance.text_art = c.appearance.text_art.cycle(dir),
            Pixelation => {
                const STEPS: [u32; 8] = [0, 8, 12, 16, 24, 32, 48, 64];
                let i = STEPS.iter().position(|s| *s >= c.appearance.pixelation).unwrap_or(0) as i32;
                c.appearance.pixelation = STEPS[(i + dir).clamp(0, STEPS.len() as i32 - 1) as usize];
            }
            ShowCover => c.appearance.show_cover = !c.appearance.show_cover,
            TitleFont => {
                let cur = crate::font::resolve(&c.appearance.title_font);
                c.appearance.title_font = crate::font::cycle(cur, dir).to_string();
            }
            LongTitle => c.appearance.long_title = c.appearance.long_title.cycle(dir),
            Shimmer => c.appearance.title_shimmer = !c.appearance.title_shimmer,
            BigControls => c.appearance.big_controls = !c.appearance.big_controls,
            ShowShuffle => c.appearance.show_shuffle = !c.appearance.show_shuffle,
            ShowRepeat => c.appearance.show_repeat = !c.appearance.show_repeat,
            Layout => c.appearance.layout = c.appearance.layout.cycle(dir),
            Icons => c.appearance.icons = c.appearance.icons.cycle(dir),
            Borders => c.appearance.borders = c.appearance.borders.cycle(dir),
            HighContrast => c.appearance.high_contrast = !c.appearance.high_contrast,
            Hints => c.appearance.show_hints = !c.appearance.show_hints,
            Osd => c.appearance.show_osd = !c.appearance.show_osd,
            Mouse => c.general.mouse = !c.general.mouse,
            AnimEnabled => c.animations.enabled = !c.animations.enabled,
            AnimDuration => c.animations.duration = (c.animations.duration + 0.25 * d).clamp(0.25, 3.0),
            TextEffects => c.animations.text_effects = !c.animations.text_effects,
            Fps => c.animations.fps = (c.animations.fps as i32 + 10 * dir).clamp(10, 120) as u32,
            Splash => c.general.splash = !c.general.splash,
            Goodbye => c.general.goodbye = !c.general.goodbye,
            VisEnabled => c.visualizer.enabled = !c.visualizer.enabled,
            VisStyle => c.visualizer.style = c.visualizer.style.cycle(dir),
            VisColors => c.visualizer.colors = c.visualizer.colors.cycle(dir),
            VisSource => c.visualizer.source = c.visualizer.source.cycle(dir),
            VisHeight => c.visualizer.height = (c.visualizer.height as i32 + dir).clamp(3, 20) as u16,
            VisSensitivity => c.visualizer.sensitivity = ((c.visualizer.sensitivity + 0.1 * d) * 10.0).round() / 10.0,
            VisPeaks => c.visualizer.peaks = !c.visualizer.peaks,
            SeekStep => {
                const STEPS: [u32; 9] = [1, 2, 5, 10, 15, 20, 30, 60, 120];
                let i = STEPS.iter().position(|s| *s >= c.playback.seek_step).unwrap_or(3) as i32;
                c.playback.seek_step = STEPS[(i + dir).clamp(0, STEPS.len() as i32 - 1) as usize];
            }
            VolumeStep => c.playback.volume_step = (c.playback.volume_step as i32 + dir).clamp(1, 50) as u32,
            SpeedStep => c.playback.speed_step = ((c.playback.speed_step + 0.05 * d) * 100.0).round() / 100.0,
            ArrowMode => c.playback.arrow_mode = c.playback.arrow_mode.cycle(dir),
            BracketMode => c.playback.bracket_mode = c.playback.bracket_mode.cycle(dir),
            DigitSeek => c.playback.digit_seek = !c.playback.digit_seek,
            SearchContext => c.playback.search_context = c.playback.search_context.cycle(dir),
            QueueHandoff => c.playback.queue_handoff = (c.playback.queue_handoff as i32 + dir).clamp(1, 15) as u32,
            Karaoke => c.lyrics.karaoke = !c.lyrics.karaoke,
            LyricsAscii => c.lyrics.ascii = !c.lyrics.ascii,
            LyricsFont => {
                // "Same as title" sits before the first font in the cycle.
                let ids: Vec<&str> = crate::font::all().map(|(id, _, _)| id).filter(|id| *id != crate::font::PLAIN).collect();
                let cur = if c.lyrics.font.is_empty() { None } else { ids.iter().position(|i| *i == crate::font::resolve(&c.lyrics.font)) };
                let n = ids.len() as i32 + 1;
                let pos = cur.map(|p| p as i32 + 1).unwrap_or(0);
                let next = (pos + dir).rem_euclid(n);
                c.lyrics.font = if next == 0 { String::new() } else { ids[(next - 1) as usize].to_string() };
            }
            LiveSearch => c.search.live = !c.search.live,
            Debounce => c.search.debounce_ms = (c.search.debounce_ms as i64 + 50 * dir as i64).clamp(0, 2000) as u64,
            ResultsPerType => c.search.results_per_type = (c.search.results_per_type as i32 + dir).clamp(3, 10) as u32,
            DefaultFilter => c.search.default_filter = c.search.default_filter.cycle(dir),
            TopResult => c.search.show_top_result = !c.search.show_top_result,
            RememberPos => c.search.remember_position = !c.search.remember_position,
            CloseOnPlay => c.search.close_on_play = !c.search.close_on_play,
            LocalPlayer => {
                c.general.local_player = !c.general.local_player;
                let on = c.general.local_player;
                if on {
                    self.local_status = LocalStatus::Off;
                    self.start_local_player(false);
                } else {
                    self.stop_local_player();
                }
            }
            AutoActivate => c.general.auto_activate = !c.general.auto_activate,
            Bitrate => {
                c.general.bitrate = match (c.general.bitrate, dir > 0) {
                    (96, true) => 160,
                    (160, true) => 320,
                    (320, true) => 96,
                    (96, false) => 320,
                    (160, false) => 96,
                    _ => 160,
                };
                self.toast(ToastKind::Info, "Quality applies after `player restart`");
            }
            Normalisation => {
                c.general.normalisation = !c.general.normalisation;
                self.toast(ToastKind::Info, "Applies after `player restart`");
            }
            MediaControls => c.general.media_controls = !c.general.media_controls,
            PollInterval => c.general.poll_interval_ms = (c.general.poll_interval_ms as i64 + 250 * dir as i64).clamp(250, 10_000) as u64,
            _ => return,
        }
        let row = self.settings.selected;
        self.settings.changed_at = Some((row, Instant::now()));
        self.save_config();
    }

    /// Enter on a row.
    pub fn setting_activate(&mut self, id: Id) {
        use Id::*;
        match id {
            Key(a) => self.settings.capture = Some(a),
            DeviceName => self.settings.editing = Some(TextInput::with(&self.cfg.general.device_name)),
            SpotifyApp => self.settings.editing = Some(TextInput::with(&self.cfg.general.client_id)),
            ResetKeys => {
                self.cfg.keys = default_keys();
                self.save_config();
                self.toast(ToastKind::Success, "Shortcuts reset to defaults");
            }
            PlayerLogin => self.execute("player login"),
            Relogin => {
                self.set_overlay(Overlay::None);
                self.execute("login");
            }
            Logout => self.execute("logout"),
            RerunOnboarding => {
                self.set_overlay(Overlay::None);
                self.execute("onboarding");
            }
            ResetAll => {
                let general = self.cfg.general.clone();
                self.cfg = Config { general, ..Config::default() };
                self.save_config();
                self.toast(ToastKind::Success, "Settings reset to defaults");
            }
            other => self.setting_adjust(other, 1),
        }
    }

    fn commit_text_setting(&mut self, id: Id) {
        let Some(input) = self.settings.editing.take() else { return };
        let v = input.value.trim().to_string();
        match id {
            Id::DeviceName if !v.is_empty() => {
                self.cfg.general.device_name = v;
                self.save_config();
                self.toast(ToastKind::Info, "Device name applies after `player restart`");
            }
            Id::SpotifyApp => {
                if crate::spotify::auth::valid_client_id(&v) {
                    self.cfg.general.client_id = v;
                    self.save_config();
                    self.toast(ToastKind::Info, "Client ID saved — run `login` to use it");
                } else {
                    self.toast(ToastKind::Error, "A client ID is 32 hexadecimal characters");
                }
            }
            _ => {}
        }
    }

    pub fn current_setting_rows(&self) -> Vec<Row> {
        rows(self.settings.tab)
    }

    pub(super) fn settings_key(&mut self, k: KeyEvent) -> bool {
        let rows = self.current_setting_rows();
        let n = rows.len();
        let sel = self.settings.selected.min(n.saturating_sub(1));
        // Capturing a new shortcut.
        if let Some(action) = self.settings.capture {
            if k.code == KeyCode::Esc {
                self.settings.capture = None;
                return true;
            }
            let chord = Chord::from_event(&k);
            if chord.is_typing_key() {
                self.toast(ToastKind::Warn, "Letters are reserved for typing commands — try a symbol, F-key or Ctrl+letter");
                return true;
            }
            let s = chord.to_config_string();
            // Steal the chord from any other action.
            for (other, keys) in self.cfg.keys.iter_mut() {
                if other != action.id() && keys.iter().any(|k| Chord::parse(k) == Some(chord)) {
                    keys.retain(|k| Chord::parse(k) != Some(chord));
                    let label = Action::from_id(other).map(|a| a.label()).unwrap_or(other);
                    self.toasts.push(Toast {
                        msg: format!("{} was also on “{label}” — moved it", chord.display(self.theme.unicode)),
                        kind: ToastKind::Warn,
                        born: Instant::now(),
                        ttl: std::time::Duration::from_secs(4),
                    });
                }
            }
            self.cfg.keys.insert(action.id().to_string(), vec![s]);
            self.settings.capture = None;
            self.settings.changed_at = Some((sel, Instant::now()));
            self.save_config();
            return true;
        }
        // Editing a text value.
        if self.settings.editing.is_some() {
            let id = rows[sel].id;
            match k.code {
                KeyCode::Esc => self.settings.editing = None,
                KeyCode::Enter => self.commit_text_setting(id),
                _ => {
                    if let Some(e) = self.settings.editing.as_mut() {
                        e.handle(&k);
                    }
                }
            }
            return true;
        }
        match k.code {
            KeyCode::Esc => self.set_overlay(Overlay::None),
            KeyCode::Up => self.settings.selected = sel.saturating_sub(1),
            KeyCode::Down => self.settings.selected = (sel + 1).min(n.saturating_sub(1)),
            KeyCode::PageUp | KeyCode::Home => self.settings.selected = 0,
            KeyCode::PageDown | KeyCode::End => self.settings.selected = n.saturating_sub(1),
            KeyCode::Tab | KeyCode::BackTab => {
                let dir: i32 = if k.code == KeyCode::BackTab { -1 } else { 1 };
                self.settings.tab = ((self.settings.tab as i32 + dir).rem_euclid(TABS.len() as i32)) as usize;
                self.settings.selected = 0;
                self.settings.scroll = 0;
            }
            KeyCode::Left => self.setting_adjust(rows[sel].id, -1),
            KeyCode::Right => self.setting_adjust(rows[sel].id, 1),
            KeyCode::Enter | KeyCode::Char(' ') => self.setting_activate(rows[sel].id),
            KeyCode::Delete | KeyCode::Backspace => {
                if let Id::Key(a) = rows[sel].id {
                    self.cfg.keys.insert(a.id().into(), a.default_keys().iter().map(|s| s.to_string()).collect());
                    self.save_config();
                }
            }
            KeyCode::F(n) if (1..=8).contains(&n) && n as usize <= TABS.len()
                && n == 2 => {
                    self.set_overlay(Overlay::None);
                }
            _ => return self.overlay_fallthrough_settings(k),
        }
        true
    }

    fn overlay_fallthrough_settings(&mut self, k: KeyEvent) -> bool {
        let chord = Chord::from_event(&k);
        if chord.is_typing_key() {
            return false;
        }
        if let Some(a @ (Action::Settings | Action::Help | Action::TogglePlay | Action::Quit | Action::CommandBar | Action::Back)) =
            self.keymap.action_for(&k)
        {
            self.do_action(a);
        }
        true
    }

    pub(super) fn settings_scroll(&mut self, dir: i32) {
        let n = self.current_setting_rows().len();
        self.settings.selected = (self.settings.selected as i32 + dir).clamp(0, n as i32 - 1) as usize;
    }

    pub(super) fn settings_click(&mut self, i: usize, rect: Rect, col: u16) {
        if self.settings.capture.is_some() || self.settings.editing.is_some() {
            return;
        }
        let rows = self.current_setting_rows();
        let Some(r) = rows.get(i) else { return };
        let id = r.id;
        let already = self.settings.selected == i;
        self.settings.selected = i;
        // Clicking the left/right half of the value cycles backward/forward.
        match r.kind {
            Kind::Toggle => self.setting_adjust(id, 1),
            Kind::Choice | Kind::Number => {
                let mid = rect.x + rect.width * 3 / 4;
                self.setting_adjust(id, if col < mid { -1 } else { 1 });
            }
            _ if already => self.setting_activate(id),
            _ => {}
        }
    }
}
