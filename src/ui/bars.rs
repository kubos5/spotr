//! Search bar, command bar (+ completions), hint line and visualizer strip.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, Widget};

use super::widgets::*;
use crate::anim;
use crate::app::{App, Click, Focus, LocalStatus};
use crate::keys::Action;
use crate::theme::Icon;

pub fn search_bar(app: &mut App, buf: &mut Buffer, area: Rect, cursor: &mut Option<(u16, u16)>) {
    let focused = app.focus == Focus::SearchInput;
    let inner = panel(app, buf, area, None, focused || app.search_open);
    app.clicks.push((area, Click::SearchBar));
    let icon = app.theme.icon(Icon::Search);
    let icon_style = if focused { app.theme.accent_bold() } else { app.theme.dim() };
    put(buf, inner.x + 1, inner.y, icon, icon_style, 2);
    let tx = inner.x + 3;

    // Right side: account / device status (main mode) or result count.
    let right = if app.search_open {
        if app.search.loading {
            format!("{} searching", app.theme.spinner(app.frame))
        } else if !app.search.query.is_empty() && app.search.stack.is_empty() && app.search.error.is_none() {
            let n = app.search.items.iter().filter(|i| i.is_selectable()).count();
            format!("{n} results")
        } else {
            String::new()
        }
    } else {
        status_text(app)
    };
    let rw = width(&right);
    let field_w = inner.width.saturating_sub(4 + rw + 2);
    put(buf, inner.right().saturating_sub(rw + 1), inner.y, &right, app.theme.dim(), rw);

    if app.search.input.is_empty() && !focused {
        let hint = format!("Search songs, artists, albums, playlists…  ({})", app.keymap.hint(Action::FocusSearch, app.theme.unicode));
        put(buf, tx, inner.y, &ellipsize(&hint, field_w as usize), app.theme.muted(), field_w);
    } else {
        let (text, col) = app.search.input.view(field_w as usize);
        let style = if focused && app.search.select_all {
            app.theme.selected()
        } else if focused {
            app.theme.bold()
        } else {
            app.theme.dim()
        };
        put(buf, tx, inner.y, &text, style, field_w);
        if focused {
            *cursor = Some((tx + col as u16, inner.y));
        }
    }
}

fn status_text(app: &App) -> String {
    let mut parts = Vec::new();
    if let Some(u) = &app.user {
        parts.push(u.name().to_string());
    }
    match (&app.pb.device, &app.local_status) {
        (Some(d), _) => parts.push(format!("{} {}", app.theme.icon(Icon::Device), d.name)),
        (None, LocalStatus::Connecting) => parts.push(format!("{} starting player", app.theme.spinner(app.frame))),
        (None, _) => parts.push("no device".into()),
    }
    if let Some(at) = app.sleep_at {
        let mins = at.saturating_duration_since(std::time::Instant::now()).as_secs().div_ceil(60);
        parts.push(format!("☾ {mins}m"));
    }
    parts.join("  ·  ")
}

pub fn command_bar(app: &mut App, buf: &mut Buffer, area: Rect, cursor: &mut Option<(u16, u16)>) {
    let focused = app.focus == Focus::Command;
    // Flash / shake feedback after running a command.
    let mut area = area;
    let mut border = app.theme.border(focused);
    if let Some((at, ok)) = app.command.flash {
        let t = app.motion.t(at, 600);
        if t < 1.0 {
            border = if ok { Style::new().fg(Color::Green) } else { Style::new().fg(Color::Red) };
            if !ok {
                let dx = anim::shake(t);
                area.x = (area.x as i32 + dx as i32).max(0) as u16;
            }
        } else {
            app.command.flash = None;
        }
    }
    let block = Block::default().borders(Borders::ALL).border_type(app.theme.border_type).border_style(border);
    let inner = block.inner(area);
    block.render(area, buf);
    app.clicks.push((area, Click::CommandBar));

    let prompt = app.theme.icon(Icon::Command);
    let pstyle = if focused { app.theme.accent_bold() } else { app.theme.dim() };
    put(buf, inner.x + 1, inner.y, prompt, pstyle, 2);
    let tx = inner.x + 3;
    let field_w = inner.width.saturating_sub(4);

    if focused {
        let (text, col) = app.command.input.view(field_w as usize);
        put(buf, tx, inner.y, &text, app.theme.bold(), field_w);
        *cursor = Some((tx + col as u16, inner.y));
        // Ghost completion.
        if app.command.base.is_none() && app.command.input.cursor == app.command.input.len() {
            let sugg = crate::commands::suggest(&app.command.input.value, &app.device_names(), &app.playlist_names());
            if let Some(s) = sugg.first()
                && let Some(rest) = s.replace.strip_prefix(app.command.input.value.as_str()) {
                    let used = width(&text);
                    put(buf, tx + used, inner.y, rest, app.theme.muted(), field_w.saturating_sub(used));
                }
        }
        return;
    }

    // Unfocused: last result, or a hint.
    let recent = app.command.message.as_ref().filter(|(_, _, at)| at.elapsed().as_secs() < 6).cloned();
    match recent {
        Some((msg, ok, at)) => {
            let icon = if ok { app.theme.icon(Icon::Check) } else { app.theme.icon(Icon::Cross) };
            let style = if ok { app.theme.success() } else { app.theme.error() };
            let t = app.motion.t(at, 400);
            let text = if app.motion.text_effects { anim::typewriter(&msg, t) } else { msg };
            put(buf, tx, inner.y, icon, style, 2);
            put(buf, tx + 2, inner.y, &ellipsize(&text, field_w.saturating_sub(2) as usize), Style::new(), field_w.saturating_sub(2));
        }
        None => {
            let hint = "Type a command — play, next, skip 10, volume 60, lyrics, queue, help…";
            put(buf, tx, inner.y, &ellipsize(hint, field_w as usize), app.theme.muted(), field_w);
        }
    }
}

/// Completion popup above the command bar.
pub fn suggestions(app: &mut App, buf: &mut Buffer, screen: Rect) {
    let base = app.command.base.clone().unwrap_or_else(|| app.command.input.value.clone());
    if base.trim().is_empty() && app.command.base.is_none() {
        return;
    }
    let sugg = crate::commands::suggest(&base, &app.device_names(), &app.playlist_names());
    if sugg.is_empty() {
        return;
    }
    // Find where the command bar is (its click rect).
    let Some(cmd) = app.clicks.iter().find(|(_, c)| *c == Click::CommandBar).map(|(r, _)| *r) else { return };
    let n = sugg.len().min(8) as u16;
    let h = n + 2;
    let w = (screen.width - 4).min(72);
    let y = cmd.y.saturating_sub(h);
    let area = Rect { x: cmd.x + 1, y, width: w, height: h };
    Clear.render(area, buf);
    let inner = panel(app, buf, area, None, true);
    let sel = app.command.suggestion;
    let offset = sel.map(|s| s.saturating_sub(n as usize - 1)).unwrap_or(0);
    for (i, s) in sugg.iter().enumerate().skip(offset).take(n as usize) {
        let y = inner.y + (i - offset) as u16;
        let row = Rect { x: inner.x, y, width: inner.width, height: 1 };
        let selected = sel == Some(i);
        if selected {
            fill(buf, row, app.theme.selected());
        }
        let style = if selected { app.theme.selected() } else { app.theme.accent() };
        let lw = put(buf, inner.x + 1, y, &s.label, style, inner.width / 2 + 6);
        if !s.desc.is_empty() {
            let dstyle = if selected { app.theme.selected() } else { app.theme.dim() };
            let dx = (inner.x + 1 + lw + 2).max(inner.x + inner.width / 2 - 4);
            put(buf, dx, y, &ellipsize(&s.desc, inner.right().saturating_sub(dx + 1) as usize), dstyle, inner.right().saturating_sub(dx));
        }
        app.clicks.push((row, Click::Suggestion(i)));
    }
}

/// The quick-keybind line under the command bar.
pub fn hints(app: &mut App, buf: &mut Buffer, area: Rect) {
    let u = app.theme.unicode;
    let arrow_label = app.pair_label(app.cfg.playback.arrow_mode);
    let bracket_label = app.pair_label(app.cfg.playback.bracket_mode);
    let left = app.keymap.hint(Action::ArrowLeft, u);
    let right = app.keymap.hint(Action::ArrowRight, u);
    let items: Vec<(String, String, Action)> = vec![
        (app.keymap.hint(Action::TogglePlay, u), if app.pb.is_playing { "pause".into() } else { "play".into() }, Action::TogglePlay),
        (format!("{left}{right}"), arrow_label, Action::CycleArrowMode),
        (format!("{} {}", app.keymap.hint(Action::BracketLeft, u), app.keymap.hint(Action::BracketRight, u)), bracket_label, Action::CycleBracketMode),
        (app.keymap.hint(Action::FocusSearch, u), "search".into(), Action::FocusSearch),
        (app.keymap.hint(Action::Help, u), "help".into(), Action::Help),
        (app.keymap.hint(Action::Settings, u), "settings".into(), Action::Settings),
        (app.keymap.hint(Action::CycleArrowMode, u), "arrow mode".into(), Action::CycleArrowMode),
        (app.keymap.hint(Action::Lyrics, u), "lyrics".into(), Action::Lyrics),
        (app.keymap.hint(Action::Queue, u), "queue".into(), Action::Queue),
        (app.keymap.hint(Action::Devices, u), "devices".into(), Action::Devices),
    ];
    let mut x = area.x + 1;
    for (key, label, action) in items {
        let w = width(&key) + 1 + width(&label);
        if x + w + 2 > area.right() {
            break;
        }
        let kw = put(buf, x, area.y, &key, app.theme.key(), w);
        put(buf, x + kw + 1, area.y, &label, app.theme.dim(), w);
        app.clicks.push((Rect { x, y: area.y, width: w, height: 1 }, Click::Action(action)));
        x += w + 3;
    }
}

pub fn visualizer(app: &mut App, buf: &mut Buffer, area: Rect) {
    if area.height == 0 {
        return;
    }
    let inner = Rect { x: area.x + 2, y: area.y, width: area.width.saturating_sub(4), height: area.height };
    let vis = &app.cfg.visualizer;
    if app.vis.idle() && !app.pb.is_playing {
        // Resting baseline.
        let ch = if app.theme.unicode { "▁" } else { "_" };
        let line: String = ch.repeat(inner.width as usize);
        put(buf, inner.x, inner.bottom() - 1, &line, app.theme.muted(), inner.width);
    } else {
        app.vis.render(inner, buf, vis.style, vis.colors, &app.theme, vis.peaks);
    }
    if !app.vis.real && app.pb.is_playing && vis.source == crate::config::VisSource::Auto {
        let label = if app.theme.unicode { "◌ simulated" } else { "simulated" };
        let lw = width(label);
        put(buf, area.right().saturating_sub(lw + 1), area.y, label, Style::new().fg(Color::DarkGray).add_modifier(Modifier::DIM), lw);
    }
    app.clicks.push((area, Click::Action(Action::CycleVisualizer)));
}

