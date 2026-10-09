//! Colors and glyphs. Everything uses the terminal's own palette (ANSI colors
//! and the default foreground/background), so Spotr follows the terminal theme.

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::BorderType;

use crate::config::{AccentColor, BorderStyle, Config, Icons};

#[derive(Clone)]
pub struct Theme {
    pub accent: Color,
    pub accent_bright: Color,
    pub high_contrast: bool,
    pub unicode: bool,
    pub border_type: BorderType,
}

impl Theme {
    pub fn from_config(cfg: &Config) -> Self {
        let (accent, accent_bright) = match cfg.appearance.accent {
            AccentColor::Green => (Color::Green, Color::LightGreen),
            AccentColor::Cyan => (Color::Cyan, Color::LightCyan),
            AccentColor::Blue => (Color::Blue, Color::LightBlue),
            AccentColor::Magenta => (Color::Magenta, Color::LightMagenta),
            AccentColor::Red => (Color::Red, Color::LightRed),
            AccentColor::Yellow => (Color::Yellow, Color::LightYellow),
            AccentColor::White => (Color::Gray, Color::White),
        };
        let border_type = match cfg.appearance.borders {
            BorderStyle::Rounded => BorderType::Rounded,
            BorderStyle::Plain => BorderType::Plain,
            BorderStyle::Double => BorderType::Double,
            BorderStyle::Thick => BorderType::Thick,
        };
        Self {
            accent,
            accent_bright,
            high_contrast: cfg.appearance.high_contrast,
            unicode: cfg.appearance.icons == Icons::Unicode,
            border_type,
        }
    }

    pub fn text(&self) -> Style {
        Style::new()
    }
    pub fn dim(&self) -> Style {
        if self.high_contrast {
            Style::new()
        } else {
            Style::new().add_modifier(Modifier::DIM)
        }
    }
    pub fn muted(&self) -> Style {
        if self.high_contrast {
            Style::new()
        } else {
            Style::new().fg(Color::DarkGray)
        }
    }
    pub fn accent(&self) -> Style {
        Style::new().fg(self.accent)
    }
    pub fn accent_bold(&self) -> Style {
        Style::new().fg(self.accent).add_modifier(Modifier::BOLD)
    }
    pub fn bold(&self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }
    pub fn selected(&self) -> Style {
        if self.high_contrast {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new().bg(self.accent).fg(Color::Black).add_modifier(Modifier::BOLD)
        }
    }
    pub fn border(&self, focused: bool) -> Style {
        if focused {
            Style::new().fg(self.accent)
        } else if self.high_contrast {
            Style::new()
        } else {
            Style::new().fg(Color::DarkGray)
        }
    }
    pub fn error(&self) -> Style {
        Style::new().fg(Color::Red).add_modifier(Modifier::BOLD)
    }
    pub fn success(&self) -> Style {
        Style::new().fg(Color::Green)
    }
    pub fn warn(&self) -> Style {
        Style::new().fg(Color::Yellow)
    }
    pub fn key(&self) -> Style {
        Style::new().fg(self.accent).add_modifier(Modifier::BOLD)
    }

    /// Terminal-palette colors used for gradients (visualizer, logo waves).
    pub fn spectrum(&self) -> &'static [Color] {
        &[
            Color::Blue,
            Color::Cyan,
            Color::Green,
            Color::LightGreen,
            Color::Yellow,
            Color::LightRed,
            Color::Red,
            Color::Magenta,
        ]
    }

    pub fn icon(&self, i: Icon) -> &'static str {
        if self.unicode {
            match i {
                Icon::Play => "▶",
                Icon::Pause => "❚❚",
                Icon::Next => "⏭",
                Icon::Prev => "⏮",
                Icon::Shuffle => "⤮",
                Icon::Repeat => "↻",
                Icon::RepeatOne => "↻¹",
                Icon::Heart => "♥",
                Icon::HeartEmpty => "♡",
                Icon::Volume => "♪",
                Icon::Mute => "∅",
                Icon::Track => "♫",
                Icon::Artist => "☺",
                Icon::Album => "◉",
                Icon::Playlist => "≡",
                Icon::Device => "◈",
                Icon::Search => "⌕",
                Icon::Command => "❯",
                Icon::Pointer => "▸",
                Icon::Explicit => "ᴱ",
                Icon::Speed => "»",
                Icon::Check => "✓",
                Icon::Cross => "✗",
                Icon::Info => "ℹ",
                Icon::Clock => "◷",
                Icon::Lyrics => "❝",
                Icon::Queue => "☰",
            }
        } else {
            match i {
                Icon::Play => ">",
                Icon::Pause => "||",
                Icon::Next => ">|",
                Icon::Prev => "|<",
                Icon::Shuffle => "S",
                Icon::Repeat => "R",
                Icon::RepeatOne => "R1",
                Icon::Heart => "<3",
                Icon::HeartEmpty => "</3",
                Icon::Volume => "vol",
                Icon::Mute => "mute",
                Icon::Track => "#",
                Icon::Artist => "@",
                Icon::Album => "o",
                Icon::Playlist => "=",
                Icon::Device => "*",
                Icon::Search => "?",
                Icon::Command => ">",
                Icon::Pointer => ">",
                Icon::Explicit => "E",
                Icon::Speed => "x",
                Icon::Check => "ok",
                Icon::Cross => "x",
                Icon::Info => "i",
                Icon::Clock => "t",
                Icon::Lyrics => "\"",
                Icon::Queue => "=",
            }
        }
    }

    pub fn spinner(&self, frame: u64) -> &'static str {
        if self.unicode {
            const F: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            F[(frame / 4) as usize % F.len()]
        } else {
            const F: [&str; 4] = ["|", "/", "-", "\\"];
            F[(frame / 6) as usize % F.len()]
        }
    }
}

#[derive(Clone, Copy)]
pub enum Icon {
    Play,
    Pause,
    Next,
    Prev,
    Shuffle,
    Repeat,
    RepeatOne,
    Heart,
    HeartEmpty,
    Volume,
    Mute,
    Track,
    Artist,
    Album,
    Playlist,
    Device,
    Search,
    Command,
    Pointer,
    Explicit,
    Speed,
    Check,
    Cross,
    Info,
    Clock,
    Lyrics,
    Queue,
}
