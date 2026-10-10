//! Persistent user configuration (`~/.config/spotr/config.toml`).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::keys::Action;

/// Where Spotr keeps its files.
pub struct Paths {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl Paths {
    pub fn new() -> Self {
        let config_dir = std::env::var_os("SPOTR_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| dirs::config_dir().map(|d| d.join("spotr")))
            .unwrap_or_else(|| PathBuf::from(".spotr"));
        let cache_dir = std::env::var_os("SPOTR_CACHE_DIR")
            .map(PathBuf::from)
            .or_else(|| dirs::cache_dir().map(|d| d.join("spotr")))
            .unwrap_or_else(|| config_dir.join("cache"));
        Self { config_dir, cache_dir }
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }
    pub fn token_file(&self) -> PathBuf {
        self.config_dir.join("token.json")
    }
    pub fn librespot_dir(&self) -> PathBuf {
        self.cache_dir.join("librespot")
    }
    pub fn log_file(&self) -> PathBuf {
        self.cache_dir.join("spotr.log")
    }
    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(&self.config_dir).context("creating config directory")?;
        fs::create_dir_all(&self.cache_dir).context("creating cache directory")?;
        Ok(())
    }
}

macro_rules! cycle_enum {
    ($name:ident { $($variant:ident => $label:expr),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
        #[serde(rename_all = "kebab-case")]
        pub enum $name {
            #[default]
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn label(self) -> &'static str {
                match self { $($name::$variant => $label),+ }
            }

            /// Lower-case identifier used by commands (`visualizer bars`).
            #[allow(dead_code)]
            pub fn id(self) -> String {
                self.label().to_lowercase().replace(' ', "-")
            }

            #[allow(dead_code)]
            pub fn from_id(s: &str) -> Option<Self> {
                let s = s.to_lowercase();
                if s.is_empty() {
                    return None;
                }
                Self::ALL.iter().copied().find(|v| v.id() == s)
                    .or_else(|| Self::ALL.iter().copied().find(|v| v.id().starts_with(&s)))
            }

            pub fn cycle(self, dir: i32) -> Self {
                let all = Self::ALL;
                let i = all.iter().position(|v| *v == self).unwrap_or(0) as i32;
                let n = all.len() as i32;
                all[((i + dir) % n + n) as usize % all.len()]
            }
        }
    };
}

// The first variant listed is the default.
cycle_enum!(CoverMode { Image => "Image", Text => "Text" });
cycle_enum!(TextArt { Blocks => "Blocks", Ascii => "ASCII", AsciiMono => "ASCII mono", Braille => "Braille" });
cycle_enum!(LongTitle { Shrink => "Shrink", Marquee => "Marquee", Plain => "Plain" });
cycle_enum!(Layout { Auto => "Auto", Stacked => "Stacked", Side => "Side by side" });
cycle_enum!(Icons { Unicode => "Unicode", Ascii => "ASCII" });
cycle_enum!(BorderStyle { Rounded => "Rounded", Plain => "Plain", Double => "Double", Thick => "Thick" });
cycle_enum!(AccentColor {
    Green => "Green", Cyan => "Cyan", Blue => "Blue", Magenta => "Magenta",
    Red => "Red", Yellow => "Yellow", White => "White"
});
cycle_enum!(VisStyle { Bars => "Bars", Mirror => "Mirror", Wave => "Wave", Blocks => "Blocks", Dots => "Dots" });
cycle_enum!(VisColors { Spectrum => "Spectrum", Accent => "Accent", Mono => "Mono" });
cycle_enum!(VisSource { Auto => "Auto", Simulated => "Simulated" });
// What a key pair (← / → or [ / ]) does.
cycle_enum!(ArrowMode { Seek => "Seek", Track => "Track", Speed => "Speed", Volume => "Volume" });
cycle_enum!(SearchContext { Album => "Album", Single => "Single" });
cycle_enum!(SearchFilter { All => "All", Tracks => "Tracks", Artists => "Artists", Albums => "Albums", Playlists => "Playlists" });

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub appearance: Appearance,
    pub animations: Animations,
    pub visualizer: Visualizer,
    pub playback: Playback,
    pub search: Search,
    pub lyrics: Lyrics,
    /// Action name -> list of key strings (`"space"`, `"ctrl+l"`, `"?"`).
    pub keys: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub onboarded: bool,
    /// Client ID of the user's own Spotify developer application.
    pub client_id: String,
    pub redirect_port: u16,
    /// Run the built-in librespot player so Spotr shows up as a Connect device.
    pub local_player: bool,
    pub device_name: String,
    /// Make the built-in player the active device on start if nothing else is playing.
    pub auto_activate: bool,
    /// Preferred local audio output (built-in player). Empty = system default.
    pub audio_device: String,
    pub bitrate: u16,
    pub normalisation: bool,
    pub poll_interval_ms: u64,
    pub mouse: bool,
    pub splash: bool,
    /// Play the closing animation on quit.
    pub goodbye: bool,
    /// Report playback to the OS (Now Playing, MPRIS) and accept media keys.
    pub media_controls: bool,
    pub device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub accent: AccentColor,
    pub cover_mode: CoverMode,
    pub text_art: TextArt,
    /// Pixel grid used for text cover art; 0 = as detailed as the terminal allows.
    pub pixelation: u32,
    pub show_cover: bool,
    /// Font id (see `font::all()`), e.g. "block", "ansi-compact", or a user font's file name.
    pub title_font: String,
    pub long_title: LongTitle,
    pub title_shimmer: bool,
    /// Three-row transport controls on the now-playing screen.
    pub big_controls: bool,
    pub show_shuffle: bool,
    pub show_repeat: bool,
    pub layout: Layout,
    pub icons: Icons,
    pub borders: BorderStyle,
    pub high_contrast: bool,
    pub show_hints: bool,
    pub show_osd: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Animations {
    pub enabled: bool,
    /// Multiplier on every animation duration (higher = slower).
    pub duration: f32,
    pub text_effects: bool,
    pub fps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Visualizer {
    pub enabled: bool,
    pub style: VisStyle,
    pub colors: VisColors,
    pub source: VisSource,
    pub height: u16,
    pub sensitivity: f32,
    pub peaks: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Playback {
    pub seek_step: u32,
    pub volume_step: u32,
    pub speed_step: f32,
    pub arrow_mode: ArrowMode,
    /// What `[` / `]` do.
    pub bracket_mode: ArrowMode,
    pub search_context: SearchContext,
    pub digit_seek: bool,
    /// Volume of the built-in player, remembered across sessions.
    pub volume: u32,
    /// Seconds before the end of a track at which the next Spotr-queue item is handed to Spotify.
    pub queue_handoff: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Search {
    pub live: bool,
    pub debounce_ms: u64,
    pub results_per_type: u32,
    pub default_filter: SearchFilter,
    pub remember_position: bool,
    pub close_on_play: bool,
    pub show_top_result: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Lyrics {
    pub karaoke: bool,
    pub center: bool,
    /// Draw lyrics with an ASCII-art font.
    pub ascii: bool,
    /// Font for ASCII lyrics; empty = same as the title font.
    pub font: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            onboarded: false,
            client_id: String::new(),
            redirect_port: 8888,
            local_player: cfg!(feature = "local-player"),
            device_name: "Spotr".into(),
            auto_activate: true,
            audio_device: String::new(),
            bitrate: 320,
            normalisation: false,
            poll_interval_ms: 1000,
            mouse: true,
            splash: true,
            goodbye: true,
            media_controls: true,
            device_id: String::new(),
        }
    }
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            accent: AccentColor::Green,
            cover_mode: CoverMode::Image,
            text_art: TextArt::Blocks,
            pixelation: 0,
            show_cover: true,
            title_font: "block".into(),
            long_title: LongTitle::Shrink,
            title_shimmer: true,
            big_controls: true,
            show_shuffle: true,
            show_repeat: true,
            layout: Layout::Auto,
            icons: Icons::Unicode,
            borders: BorderStyle::Rounded,
            high_contrast: false,
            show_hints: true,
            show_osd: true,
        }
    }
}

impl Default for Animations {
    fn default() -> Self {
        Self { enabled: true, duration: 1.0, text_effects: true, fps: 60 }
    }
}

impl Default for Visualizer {
    fn default() -> Self {
        Self {
            enabled: true,
            style: VisStyle::Bars,
            colors: VisColors::Spectrum,
            source: VisSource::Auto,
            height: 7,
            sensitivity: 1.0,
            peaks: true,
        }
    }
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            seek_step: 10,
            volume_step: 5,
            speed_step: 0.25,
            arrow_mode: ArrowMode::Seek,
            bracket_mode: ArrowMode::Track,
            search_context: SearchContext::Album,
            digit_seek: true,
            queue_handoff: 3,
            volume: 70,
        }
    }
}

impl Default for Search {
    fn default() -> Self {
        Self {
            live: true,
            debounce_ms: 300,
            results_per_type: 10,
            default_filter: SearchFilter::All,
            remember_position: true,
            close_on_play: true,
            show_top_result: true,
        }
    }
}

impl Default for Lyrics {
    fn default() -> Self {
        Self { karaoke: true, center: true, ascii: false, font: String::new() }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: General::default(),
            appearance: Appearance::default(),
            animations: Animations::default(),
            visualizer: Visualizer::default(),
            playback: Playback::default(),
            search: Search::default(),
            lyrics: Lyrics::default(),
            keys: default_keys(),
        }
    }
}

pub fn default_keys() -> BTreeMap<String, Vec<String>> {
    Action::ALL
        .iter()
        .map(|a| (a.id().to_string(), a.default_keys().iter().map(|s| s.to_string()).collect()))
        .collect()
}

impl Config {
    pub fn load(paths: &Paths) -> Result<Self> {
        let file = paths.config_file();
        if !file.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(&file).context("reading config")?;
        let mut cfg: Config = toml::from_str(&text)
            .with_context(|| format!("parsing {}", file.display()))?;
        // [ / ] used to be bound straight to previous / next; they're now their
        // own configurable pair. Move old default bindings over.
        if !cfg.keys.contains_key("bracket_left") {
            if cfg.keys.get("previous").map(|v| v == &["["]).unwrap_or(false) {
                cfg.keys.remove("previous");
            }
            if cfg.keys.get("next").map(|v| v == &["]"]).unwrap_or(false) {
                cfg.keys.remove("next");
            }
        }
        // Make sure newly added actions get their default bindings.
        for (k, v) in default_keys() {
            cfg.keys.entry(k).or_insert(v);
        }
        cfg.sanitize();
        Ok(cfg)
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        paths.ensure()?;
        let text = toml::to_string_pretty(self).context("serializing config")?;
        let tmp = paths.config_file().with_extension("toml.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, paths.config_file())?;
        Ok(())
    }

    pub fn sanitize(&mut self) {
        self.animations.duration = self.animations.duration.clamp(0.25, 3.0);
        self.animations.fps = self.animations.fps.clamp(10, 120);
        self.visualizer.height = self.visualizer.height.clamp(3, 20);
        self.visualizer.sensitivity = self.visualizer.sensitivity.clamp(0.2, 4.0);
        self.playback.seek_step = self.playback.seek_step.clamp(1, 600);
        self.playback.volume_step = self.playback.volume_step.clamp(1, 50);
        self.playback.volume = self.playback.volume.min(100);
        self.playback.speed_step = self.playback.speed_step.clamp(0.05, 1.0);
        self.playback.queue_handoff = self.playback.queue_handoff.clamp(1, 15);
        // Spotify caps search at 10 results per type (Feb 2026).
        self.search.results_per_type = self.search.results_per_type.clamp(3, 10);
        self.search.debounce_ms = self.search.debounce_ms.clamp(0, 2000);
        self.general.poll_interval_ms = self.general.poll_interval_ms.clamp(250, 10_000);
        if ![96, 160, 320].contains(&self.general.bitrate) {
            self.general.bitrate = 320;
        }
        if self.general.device_name.trim().is_empty() {
            self.general.device_name = "Spotr".into();
        }
        if self.general.device_id.len() != 40 {
            self.general.device_id = (0..40)
                .map(|_| char::from_digit(rand::random_range(0..16u32), 16).unwrap())
                .collect();
        }
    }

    pub fn keys_for(&self, action: Action) -> Vec<String> {
        self.keys.get(action.id()).cloned().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_old_bracket_bindings() {
        let dir = std::env::temp_dir().join(format!("spotr-cfg-test-{}", std::process::id()));
        let paths = Paths { config_dir: dir.clone(), cache_dir: dir.join("cache") };
        paths.ensure().unwrap();
        std::fs::write(paths.config_file(), "[keys]\nnext = [\"]\"]\nprevious = [\"[\"]\n").unwrap();
        let cfg = Config::load(&paths).unwrap();
        assert_eq!(cfg.keys["bracket_right"], vec!["]"]);
        assert_eq!(cfg.keys["next"], vec!["f8"]);
        assert_eq!(cfg.keys["previous"], vec!["f7"]);
        assert_eq!(cfg.playback.bracket_mode, ArrowMode::Track);
        let _ = std::fs::remove_dir_all(dir);
    }
}
