//! Keyboard shortcuts: actions, parsing and display of key strings.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::Config;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    TogglePlay,
    FocusSearch,
    Help,
    Settings,
    CommandBar,
    ArrowLeft,
    ArrowRight,
    CycleArrowMode,
    BracketLeft,
    BracketRight,
    CycleBracketMode,
    VolumeUp,
    VolumeDown,
    Mute,
    NextTrack,
    PrevTrack,
    SeekForward,
    SeekBackward,
    SpeedUp,
    SpeedDown,
    SpeedReset,
    Shuffle,
    Repeat,
    Like,
    Lyrics,
    Queue,
    Devices,
    Library,
    ToggleCover,
    CycleVisualizer,
    AddToQueue,
    Back,
    Quit,
}

impl Action {
    pub const ALL: &'static [Action] = &[
        Action::TogglePlay,
        Action::FocusSearch,
        Action::CommandBar,
        Action::Help,
        Action::Settings,
        Action::ArrowLeft,
        Action::ArrowRight,
        Action::CycleArrowMode,
        Action::BracketLeft,
        Action::BracketRight,
        Action::CycleBracketMode,
        Action::SeekBackward,
        Action::SeekForward,
        Action::PrevTrack,
        Action::NextTrack,
        Action::SpeedDown,
        Action::SpeedUp,
        Action::SpeedReset,
        Action::VolumeUp,
        Action::VolumeDown,
        Action::Mute,
        Action::Shuffle,
        Action::Repeat,
        Action::Like,
        Action::AddToQueue,
        Action::Lyrics,
        Action::Queue,
        Action::Devices,
        Action::Library,
        Action::ToggleCover,
        Action::CycleVisualizer,
        Action::Back,
        Action::Quit,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Action::TogglePlay => "play_pause",
            Action::FocusSearch => "search",
            Action::Help => "help",
            Action::Settings => "settings",
            Action::CommandBar => "command_bar",
            Action::ArrowLeft => "arrow_left",
            Action::ArrowRight => "arrow_right",
            Action::CycleArrowMode => "arrow_mode",
            Action::BracketLeft => "bracket_left",
            Action::BracketRight => "bracket_right",
            Action::CycleBracketMode => "bracket_mode",
            Action::VolumeUp => "volume_up",
            Action::VolumeDown => "volume_down",
            Action::Mute => "mute",
            Action::NextTrack => "next",
            Action::PrevTrack => "previous",
            Action::SeekForward => "seek_forward",
            Action::SeekBackward => "seek_backward",
            Action::SpeedUp => "speed_up",
            Action::SpeedDown => "speed_down",
            Action::SpeedReset => "speed_reset",
            Action::Shuffle => "shuffle",
            Action::Repeat => "repeat",
            Action::Like => "like",
            Action::Lyrics => "lyrics",
            Action::Queue => "queue",
            Action::Devices => "devices",
            Action::Library => "library",
            Action::ToggleCover => "toggle_cover",
            Action::CycleVisualizer => "cycle_visualizer",
            Action::AddToQueue => "add_to_queue",
            Action::Back => "back",
            Action::Quit => "quit",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Action::TogglePlay => "Play / pause",
            Action::FocusSearch => "Focus the search bar",
            Action::Help => "Show this help",
            Action::Settings => "Open settings",
            Action::CommandBar => "Focus the command bar",
            Action::ArrowLeft => "← action (depends on its mode)",
            Action::ArrowRight => "→ action (depends on its mode)",
            Action::CycleArrowMode => "Switch what ← / → do (seek / track / speed / volume)",
            Action::BracketLeft => "[ action (depends on its mode)",
            Action::BracketRight => "] action (depends on its mode)",
            Action::CycleBracketMode => "Switch what [ / ] do (seek / track / speed / volume)",
            Action::VolumeUp => "Volume up",
            Action::VolumeDown => "Volume down",
            Action::Mute => "Mute / unmute",
            Action::NextTrack => "Next track",
            Action::PrevTrack => "Previous track",
            Action::SeekForward => "Skip forward",
            Action::SeekBackward => "Rewind",
            Action::SpeedUp => "Increase speed",
            Action::SpeedDown => "Decrease speed",
            Action::SpeedReset => "Reset speed to 1×",
            Action::Shuffle => "Toggle shuffle",
            Action::Repeat => "Cycle repeat (off / context / track)",
            Action::Like => "Like / unlike (current or selected track)",
            Action::Lyrics => "Lyrics",
            Action::Queue => "Queue",
            Action::Devices => "Output devices",
            Action::Library => "Your library",
            Action::ToggleCover => "Toggle cover art: image / text",
            Action::CycleVisualizer => "Cycle visualizer style",
            Action::AddToQueue => "Add selected item to the queue",
            Action::Back => "Back / close",
            Action::Quit => "Quit Spotr",
        }
    }

    pub fn default_keys(self) -> &'static [&'static str] {
        match self {
            Action::TogglePlay => &["space"],
            Action::FocusSearch => &["/"],
            Action::Help => &["?", "f1"],
            Action::Settings => &[",", "f2"],
            Action::CommandBar => &[":"],
            Action::ArrowLeft => &["left"],
            Action::ArrowRight => &["right"],
            Action::CycleArrowMode => &["tab"],
            Action::BracketLeft => &["["],
            Action::BracketRight => &["]"],
            Action::CycleBracketMode => &["shift+tab"],
            Action::VolumeUp => &["up", "+"],
            Action::VolumeDown => &["down", "-"],
            Action::Mute => &["ctrl+u"],
            Action::NextTrack => &["f8"],
            Action::PrevTrack => &["f7"],
            Action::SeekForward => &["shift+right", "}"],
            Action::SeekBackward => &["shift+left", "{"],
            Action::SpeedUp => &[">"],
            Action::SpeedDown => &["<"],
            Action::SpeedReset => &["="],
            Action::Shuffle => &["ctrl+s"],
            Action::Repeat => &["ctrl+r"],
            Action::Like => &["*"],
            Action::Lyrics => &["ctrl+l", "f3"],
            Action::Queue => &["ctrl+e", "f4"],
            Action::Devices => &["ctrl+d", "f5"],
            Action::Library => &["ctrl+b", "f6"],
            Action::ToggleCover => &["ctrl+t"],
            Action::CycleVisualizer => &["ctrl+v"],
            Action::AddToQueue => &["ctrl+a"],
            Action::Back => &["esc"],
            Action::Quit => &["ctrl+c", "ctrl+q"],
        }
    }

    pub fn from_id(id: &str) -> Option<Action> {
        Action::ALL.iter().copied().find(|a| a.id() == id)
    }
}

/// A normalized key chord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl Chord {
    pub fn from_event(ev: &KeyEvent) -> Self {
        let mut mods = ev.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        let mut code = ev.code;
        // Shift is already folded into printable characters ("?" vs "/").
        if let KeyCode::Char(c) = code {
            if !c.is_ascii_alphabetic() || !mods.contains(KeyModifiers::CONTROL) {
                mods.remove(KeyModifiers::SHIFT);
            }
            if mods.contains(KeyModifiers::CONTROL) || mods.contains(KeyModifiers::ALT) {
                code = KeyCode::Char(c.to_ascii_lowercase());
            }
        }
        if code == KeyCode::BackTab {
            code = KeyCode::Tab;
            mods.insert(KeyModifiers::SHIFT);
        }
        Self { code, mods }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        // A lone "+" or "-" (or a chord ending in them) is a key, not a separator.
        let (prefix, key) = if s.len() > 1 && (s.ends_with("++") || s.ends_with("+-")) {
            (&s[..s.len() - 2], &s[s.len() - 1..])
        } else if s == "+" || s == "-" {
            ("", s)
        } else {
            match s.rfind('+') {
                Some(i) => (&s[..i], &s[i + 1..]),
                None => ("", s),
            }
        };
        let mut mods = KeyModifiers::NONE;
        for m in prefix.split('+').filter(|p| !p.is_empty()) {
            match m.to_lowercase().as_str() {
                "ctrl" | "control" | "c" => mods |= KeyModifiers::CONTROL,
                "alt" | "meta" | "opt" | "option" | "m" => mods |= KeyModifiers::ALT,
                "shift" | "s" => mods |= KeyModifiers::SHIFT,
                _ => return None,
            }
        }
        let lower = key.to_lowercase();
        let code = match lower.as_str() {
            "space" | "spc" => KeyCode::Char(' '),
            "enter" | "return" | "ret" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "tab" => KeyCode::Tab,
            "backtab" => {
                mods |= KeyModifiers::SHIFT;
                KeyCode::Tab
            }
            "backspace" | "bs" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "insert" | "ins" => KeyCode::Insert,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" => KeyCode::PageDown,
            "plus" => KeyCode::Char('+'),
            "minus" => KeyCode::Char('-'),
            "comma" => KeyCode::Char(','),
            "slash" => KeyCode::Char('/'),
            f if f.starts_with('f') && f.len() > 1 && f[1..].chars().all(|c| c.is_ascii_digit()) => {
                KeyCode::F(f[1..].parse().ok()?)
            }
            _ => {
                let mut chars = key.chars();
                let c = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                if mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
                    KeyCode::Char(c.to_ascii_lowercase())
                } else {
                    KeyCode::Char(c)
                }
            }
        };
        if let KeyCode::Char(c) = code
            && !(c.is_ascii_alphabetic() && mods.contains(KeyModifiers::CONTROL)) {
                mods.remove(KeyModifiers::SHIFT);
            }
        Some(Self { code, mods })
    }

    /// Canonical string form, as stored in the config.
    pub fn to_config_string(self) -> String {
        let mut out = String::new();
        if self.mods.contains(KeyModifiers::CONTROL) {
            out.push_str("ctrl+");
        }
        if self.mods.contains(KeyModifiers::ALT) {
            out.push_str("alt+");
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            out.push_str("shift+");
        }
        let key = match self.code {
            KeyCode::Char(' ') => "space".to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "enter".into(),
            KeyCode::Esc => "esc".into(),
            KeyCode::Tab => "tab".into(),
            KeyCode::Backspace => "backspace".into(),
            KeyCode::Delete => "delete".into(),
            KeyCode::Insert => "insert".into(),
            KeyCode::Left => "left".into(),
            KeyCode::Right => "right".into(),
            KeyCode::Up => "up".into(),
            KeyCode::Down => "down".into(),
            KeyCode::Home => "home".into(),
            KeyCode::End => "end".into(),
            KeyCode::PageUp => "pageup".into(),
            KeyCode::PageDown => "pagedown".into(),
            KeyCode::F(n) => format!("f{n}"),
            other => format!("{other:?}").to_lowercase(),
        };
        out.push_str(&key);
        out
    }

    /// Short, pretty form for hints ("␣", "Ctrl+L", "←").
    pub fn display(self, unicode: bool) -> String {
        let mut out = String::new();
        if self.mods.contains(KeyModifiers::CONTROL) {
            out.push_str(if unicode { "^" } else { "C-" });
        }
        if self.mods.contains(KeyModifiers::ALT) {
            out.push_str(if unicode { "⌥" } else { "M-" });
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            out.push_str(if unicode { "⇧" } else { "S-" });
        }
        let key = match self.code {
            KeyCode::Char(' ') => (if unicode { "␣" } else { "Space" }).to_string(),
            KeyCode::Char(c) if self.mods.contains(KeyModifiers::CONTROL) => c.to_ascii_uppercase().to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Left => (if unicode { "←" } else { "Left" }).into(),
            KeyCode::Right => (if unicode { "→" } else { "Right" }).into(),
            KeyCode::Up => (if unicode { "↑" } else { "Up" }).into(),
            KeyCode::Down => (if unicode { "↓" } else { "Down" }).into(),
            KeyCode::Enter => (if unicode { "⏎" } else { "Enter" }).into(),
            KeyCode::Esc => "Esc".into(),
            KeyCode::Tab => "Tab".into(),
            KeyCode::Backspace => (if unicode { "⌫" } else { "Bksp" }).into(),
            KeyCode::Delete => "Del".into(),
            KeyCode::F(n) => format!("F{n}"),
            KeyCode::PageUp => "PgUp".into(),
            KeyCode::PageDown => "PgDn".into(),
            KeyCode::Home => "Home".into(),
            KeyCode::End => "End".into(),
            other => format!("{other:?}"),
        };
        out.push_str(&key);
        out
    }

    /// Plain letters always go to the command bar, so they can't be shortcuts.
    pub fn is_typing_key(self) -> bool {
        matches!(self.code, KeyCode::Char(c) if c.is_alphabetic())
            && !self.mods.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
    }
}

/// Resolved key map built from the config.
pub struct KeyMap {
    bindings: Vec<(Chord, Action)>,
}

impl KeyMap {
    pub fn from_config(cfg: &Config) -> Self {
        let mut bindings = Vec::new();
        for action in Action::ALL {
            for k in cfg.keys_for(*action) {
                if let Some(ch) = Chord::parse(&k) {
                    bindings.push((ch, *action));
                }
            }
        }
        Self { bindings }
    }

    pub fn action_for(&self, ev: &KeyEvent) -> Option<Action> {
        let chord = Chord::from_event(ev);
        self.bindings.iter().find(|(c, _)| *c == chord).map(|(_, a)| *a)
    }

    pub fn chords(&self, action: Action) -> impl Iterator<Item = Chord> + '_ {
        self.bindings.iter().filter(move |(_, a)| *a == action).map(|(c, _)| *c)
    }

    /// First binding of an action, prettified (or "—" if unbound).
    pub fn hint(&self, action: Action, unicode: bool) -> String {
        self.chords(action)
            .next()
            .map(|c| c.display(unicode))
            .unwrap_or_else(|| "—".into())
    }

    pub fn hints(&self, action: Action, unicode: bool) -> String {
        let v: Vec<String> = self.chords(action).map(|c| c.display(unicode)).collect();
        if v.is_empty() { "unbound".into() } else { v.join("  ") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_roundtrip() {
        for s in ["space", "ctrl+l", "?", "+", "-", "ctrl++", "shift+left", "f1", "esc", "["] {
            let c = Chord::parse(s).unwrap_or_else(|| panic!("{s}"));
            let back = c.to_config_string();
            assert_eq!(Chord::parse(&back), Some(c), "{s} -> {back}");
        }
    }

    #[test]
    fn letters_are_typing() {
        assert!(Chord::parse("a").unwrap().is_typing_key());
        assert!(!Chord::parse("ctrl+a").unwrap().is_typing_key());
        assert!(!Chord::parse("?").unwrap().is_typing_key());
    }
}
