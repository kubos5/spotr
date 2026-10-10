//! Modal panels: help, settings, about, devices.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::widgets::*;
use crate::anim;
use crate::app::settings::{Kind, TABS, rows};
use crate::app::{App, Click, LocalStatus};
use crate::commands::COMMANDS;
use crate::keys::Action;
use crate::theme::Icon;

fn modal_area(area: Rect, w: u16, h: u16, app: &App) -> Rect {
    // Grow in from the center.
    let t = app.motion.t(app.view_changed_at, 220);
    let e = anim::ease_out_back(t).max(0.2);
    let w = ((w.min(area.width.saturating_sub(4))) as f32 * e.min(1.0)) as u16;
    let h = ((h.min(area.height.saturating_sub(2))) as f32 * e.min(1.0)).max(3.0) as u16;
    centered(area, w, h)
}

pub fn help(app: &mut App, buf: &mut Buffer, area: Rect) {
    let r = modal_area(area, 100, 40, app);
    let inner = popup(app, buf, r, "Help");
    let th = app.theme.clone();
    let u = th.unicode;
    let filter = app.help.filter.clone();
    // Build all lines first, then scroll.
    let mut lines: Vec<(String, String, bool)> = Vec::new(); // (left, right, is_header)
    let matches = |a: &str, b: &str| filter.is_empty() || a.to_lowercase().contains(&filter) || b.to_lowercase().contains(&filter);
    lines.push(("Keyboard shortcuts".into(), String::new(), true));
    for a in Action::ALL {
        let keys = app.keymap.hints(*a, u);
        if matches(&keys, a.label()) {
            lines.push((keys, a.label().into(), false));
        }
    }
    let nav: &[(&str, &str)] = &[
        ("letters", "Start typing a command (focuses the command bar)"),
        ("0 … 9", "Jump to 0% … 90% of the song"),
        ("↑ ↓  PgUp PgDn  Home End", "Move in lists"),
        ("⏎", "Play song / open album, artist, playlist"),
        ("⌫ / Esc", "Back to the previous page / close"),
        ("Tab", "Search: next filter · Settings: next tab · Command bar: complete"),
        ("⇧↑ ⇧↓", "Queue: move the selected song"),
        ("Del", "Queue: remove the selected song"),
        ("Ctrl+W  Ctrl+U  Alt+←→", "Text fields: delete word / line, jump words"),
    ];
    lines.push((String::new(), String::new(), false));
    lines.push(("Lists & text fields".into(), String::new(), true));
    for (k, d) in nav {
        if matches(k, d) {
            lines.push((k.to_string(), d.to_string(), false));
        }
    }
    let mut group = "";
    for c in COMMANDS {
        if !matches(c.usage, c.desc) && !c.aliases.iter().any(|a| matches(a, "")) {
            continue;
        }
        if c.group != group {
            group = c.group;
            lines.push((String::new(), String::new(), false));
            lines.push((format!("Commands · {group}"), String::new(), true));
        }
        let aliases = if c.aliases.is_empty() { String::new() } else { format!("  (also: {})", c.aliases.join(", ")) };
        lines.push((c.usage.to_string(), format!("{}{aliases}", c.desc), false));
    }
    lines.push((String::new(), String::new(), false));
    lines.push(("Mouse".into(), String::new(), true));
    lines.push(("click".into(), "Focus bars, play results, seek on the progress bar, toggle controls".into(), false));
    lines.push(("hover / scroll".into(), "Select rows · scroll lists · scroll elsewhere for volume".into(), false));

    let header = if filter.is_empty() {
        "Everything is reachable from the keyboard. Type `help <word>` to filter.".to_string()
    } else {
        format!("Filter: “{filter}” — ⌫ to clear")
    };
    put(buf, inner.x + 2, inner.y, &header, th.dim(), inner.width.saturating_sub(4));
    let body = Rect { y: inner.y + 2, height: inner.height.saturating_sub(2), ..inner };
    let max_scroll = lines.len().saturating_sub(body.height as usize);
    app.help.scroll = app.help.scroll.min(max_scroll);
    let key_w = 30.min(body.width / 3);
    for (i, (l, rtext, hdr)) in lines.iter().skip(app.help.scroll).take(body.height as usize).enumerate() {
        let y = body.y + i as u16;
        if *hdr {
            put(buf, body.x + 2, y, l, th.accent_bold().add_modifier(Modifier::UNDERLINED), body.width);
        } else {
            put(buf, body.x + 4, y, &ellipsize(l, key_w as usize), th.key(), key_w);
            put(buf, body.x + 6 + key_w, y, &ellipsize(rtext, body.width.saturating_sub(key_w + 8) as usize), Style::new(), body.width.saturating_sub(key_w + 8));
        }
    }
    if max_scroll > 0 {
        let pct = app.help.scroll * 100 / max_scroll.max(1);
        let s = format!("↕ {pct}%");
        put(buf, r.right().saturating_sub(width(&s) + 3), r.bottom() - 1, &s, th.dim(), 10);
    }
}

pub fn settings(app: &mut App, buf: &mut Buffer, area: Rect, cursor: &mut Option<(u16, u16)>) {
    let r = modal_area(area, 96, 34, app);
    let inner = popup(app, buf, r, "Settings");
    let th = app.theme.clone();
    // Tabs.
    let mut x = inner.x + 1;
    for (i, t) in TABS.iter().enumerate() {
        let w = width(t) + 2;
        if x + w > inner.right() {
            break;
        }
        let style = if i == app.settings.tab { th.selected() } else { th.dim() };
        put(buf, x, inner.y, &format!(" {t} "), style, w);
        app.clicks.push((Rect { x, y: inner.y, width: w, height: 1 }, Click::SettingsTab(i)));
        x += w + 1;
    }
    let rows = rows(app.settings.tab);
    let list = Rect { x: inner.x + 1, y: inner.y + 2, width: inner.width.saturating_sub(2), height: inner.height.saturating_sub(6) };
    let n = rows.len();
    let sel = app.settings.selected.min(n.saturating_sub(1));
    app.settings.selected = sel;
    // Keep the selection visible, but don't save positions computed while the
    // panel is still growing open (its height is tiny then).
    let h = (list.height as usize).max(1);
    let mut scroll = app.settings.scroll.min(n.saturating_sub(h));
    if sel < scroll {
        scroll = sel;
    }
    if sel >= scroll + h {
        scroll = sel + 1 - h;
    }
    if !app.motion.active(app.view_changed_at, 220) {
        app.settings.scroll = scroll;
    }
    let label_w = (list.width / 2).min(40);
    for (i, row) in rows.iter().enumerate().skip(scroll).take(list.height as usize) {
        let y = list.y + (i - scroll) as u16;
        let rect = Rect { x: list.x, y, width: list.width, height: 1 };
        let selected = i == sel;
        if selected {
            fill(buf, rect, th.selected());
        }
        let base = if selected { th.selected() } else { Style::new() };
        put(buf, list.x + 1, y, if selected { th.icon(Icon::Pointer) } else { " " }, base, 1);
        put(buf, list.x + 3, y, &ellipsize(&row.label, label_w as usize), base, label_w);
        let vx = list.x + 4 + label_w;
        let vw = list.right().saturating_sub(vx + 1);
        // Pulse the value briefly after a change.
        let changed = app.settings.changed_at.map(|(r, at)| r == i && app.motion.active(at, 350)).unwrap_or(false);
        let value = app.setting_value(row.id);
        let capture = selected && matches!(row.kind, Kind::Key) && app.settings.capture.is_some();
        let editing = selected && app.settings.editing.is_some();
        if capture {
            let blink = if anim::pulse(900) > 0.5 { "▌" } else { " " };
            put(buf, vx, y, &format!("press a key… {blink}  (Esc cancels)"), base.add_modifier(Modifier::BOLD), vw);
        } else if editing {
            let e = app.settings.editing.as_ref().unwrap();
            let (text, col) = e.view(vw.saturating_sub(4) as usize);
            put(buf, vx, y, &format!("[ {text}"), base, vw);
            put(buf, vx + vw.saturating_sub(1), y, "]", base, 1);
            *cursor = Some((vx + 2 + col as u16, y));
        } else {
            let display = match row.kind {
                Kind::Toggle => {
                    let on = value.starts_with("on");
                    let sw = if th.unicode { if on { "━━●" } else { "●━━" } } else if on { "[ x]" } else { "[x ]" };
                    let style = if selected { base } else if on { th.accent_bold() } else { th.muted() };
                    let w = put(buf, vx, y, sw, style, vw);
                    put(buf, vx + w + 1, y, &value, if selected { base } else { th.dim() }, vw.saturating_sub(w + 1));
                    None
                }
                Kind::Choice | Kind::Number => Some(format!("‹ {value} ›")),
                Kind::Text => Some(format!("{value}  ✎")),
                Kind::Key => Some(value),
                Kind::Button => Some(value),
            };
            if let Some(d) = display {
                let style = if changed && !selected {
                    Style::new().fg(th.accent_bright).add_modifier(Modifier::BOLD)
                } else if changed {
                    base.add_modifier(Modifier::BOLD | Modifier::ITALIC)
                } else if selected {
                    base
                } else {
                    th.accent()
                };
                let d = if th.unicode { d } else { d.replace('‹', "<").replace('›', ">").replace('✎', "*") };
                put(buf, vx, y, &ellipsize(&d, vw as usize), style, vw);
            }
        }
        app.clicks.push((rect, Click::SettingsRow(i)));
    }
    // Description + help footer.
    if let Some(row) = rows.get(sel) {
        let dy = inner.bottom().saturating_sub(3);
        let line = if th.unicode { "─" } else { "-" };
        put(buf, inner.x + 1, dy, &line.repeat(inner.width.saturating_sub(2) as usize), th.muted(), inner.width);
        put(buf, inner.x + 2, dy + 1, &ellipsize(row.desc, inner.width.saturating_sub(4) as usize), th.dim(), inner.width);
    }
    let foot = if th.unicode {
        "↑↓ select · ← → change · ⏎ toggle/edit · Tab next tab · Esc close · saved automatically"
    } else {
        "Up/Down select - Left/Right change - Enter edit - Tab next tab - Esc close"
    };
    put(buf, inner.x + 2, inner.bottom() - 1, &ellipsize(foot, inner.width.saturating_sub(4) as usize), th.muted(), inner.width);
}

pub fn about(app: &mut App, buf: &mut Buffer, area: Rect) {
    let r = modal_area(area, 76, 24, app);
    let inner = popup(app, buf, r, "About");
    let th = app.theme.clone();
    let mut y = inner.y + 1;
    y += brand_title(app, buf, Rect { x: inner.x, y, width: inner.width, height: 3 }, "SPOTR", app.view_changed_at) + 1;
    let player = match &app.local_status {
        LocalStatus::Ready => format!("built-in player ready (“{}”)", app.cfg.general.device_name),
        LocalStatus::Connecting => "built-in player starting…".into(),
        LocalStatus::NeedsLogin => "built-in player needs sign-in".into(),
        LocalStatus::Failed(e) => format!("built-in player failed: {e}"),
        LocalStatus::Off if crate::player::AVAILABLE => "built-in player off".into(),
        LocalStatus::Off => "built-in player not compiled in".into(),
    };
    let lines: Vec<(String, Style)> = vec![
        ("A better Spotify client, for the terminal".into(), th.bold()),
        (format!("version {}", env!("CARGO_PKG_VERSION")), th.dim()),
        (String::new(), Style::new()),
        (format!("account      {}", app.user.as_ref().map(|u| format!("{} ({})", u.name(), u.product.as_deref().unwrap_or("?"))).unwrap_or_else(|| "not signed in".into())), Style::new()),
        (format!("graphics     {}", app.images.protocol_name()), Style::new()),
        (format!("player       {player}"), Style::new()),
        (format!("config       {}", app.paths.config_file().display()), Style::new()),
        (String::new(), Style::new()),
        ("Free software under the GNU GPL v3 (or later).".into(), th.dim()),
        ("Built with ratatui, librespot and the Spotify Web API. Lyrics by LRCLIB.".into(), th.dim()),
        ("Not affiliated with or endorsed by Spotify AB.".into(), th.muted()),
    ];
    for (l, st) in lines {
        if y >= inner.bottom() {
            break;
        }
        let l = ellipsize(&l, inner.width.saturating_sub(4) as usize);
        put(buf, inner.x + inner.width.saturating_sub(width(&l)) / 2, y, &l, st, inner.width);
        y += 1;
    }
}

pub fn devices(app: &mut App, buf: &mut Buffer, area: Rect) {
    let outputs = app.devices.outputs.len();
    let h = app.devices.list.len().max(1) + if outputs > 0 { outputs + 3 } else { 0 } + 6;
    let r = modal_area(area, 70, (h as u16).clamp(10, 26), app);
    let inner = popup(app, buf, r, "Devices");
    let th = app.theme.clone();
    let mut y = inner.y + 1;
    let x = inner.x + 2;
    let w = inner.width.saturating_sub(4);
    put(buf, x, y, "Play on", th.bold().add_modifier(Modifier::UNDERLINED), w);
    if app.devices.loading && app.devices.list.is_empty() {
        put(buf, x + 10, y, &format!("{} looking for devices", th.spinner(app.frame)), th.dim(), w);
    }
    y += 1;
    if app.devices.list.is_empty() && !app.devices.loading {
        put(buf, x + 1, y, "No devices — open Spotify somewhere, or enable the built-in player", th.dim(), w);
        y += 1;
    }
    let sel = app.devices.nav.selected;
    let list = app.devices.list.clone();
    for (i, d) in list.iter().enumerate() {
        if y >= inner.bottom() - 2 {
            break;
        }
        let rect = Rect { x, y, width: w, height: 1 };
        let selected = i == sel;
        if selected {
            fill(buf, rect, th.selected());
        }
        let base = if selected { th.selected() } else { Style::new() };
        let kind = match d.kind.to_lowercase().as_str() {
            "computer" => if th.unicode { "▭" } else { "pc" },
            "smartphone" => if th.unicode { "▯" } else { "ph" },
            "speaker" => if th.unicode { "◍" } else { "sp" },
            "tv" => if th.unicode { "▬" } else { "tv" },
            _ => th.icon(Icon::Device),
        };
        let mine = d.id.as_deref() == Some(app.cfg.general.device_id.as_str()) || d.name == app.cfg.general.device_name && app.local.is_some();
        let active = if d.is_active { format!(" {} active", eq_glyph(app)) } else { String::new() };
        let name = format!("{kind}  {}{}{active}", d.name, if mine { "  (Spotr)" } else { "" });
        put(buf, x + 1, y, &ellipsize(&name, w.saturating_sub(12) as usize), if d.is_active && !selected { th.accent_bold() } else { base }, w);
        if let Some(v) = d.volume_percent {
            let s = format!("{} {v:>3}%", meter(v as f32 / 100.0, 5, th.unicode));
            put(buf, x + w - width(&s) - 1, y, &s, if selected { base } else { th.dim() }, 12);
        }
        app.clicks.push((rect, Click::DeviceRow(i)));
        y += 1;
    }
    if !app.devices.outputs.is_empty() && y + 2 < inner.bottom() {
        y += 1;
        put(buf, x, y, "Built-in player output", th.bold().add_modifier(Modifier::UNDERLINED), w);
        y += 1;
        let current = app.cfg.general.audio_device.clone();
        for (j, name) in app.devices.outputs.clone().iter().enumerate() {
            if y >= inner.bottom() - 1 {
                break;
            }
            let i = list.len() + j;
            let rect = Rect { x, y, width: w, height: 1 };
            let selected = i == sel;
            if selected {
                fill(buf, rect, th.selected());
            }
            let is_cur = *name == current || (current.is_empty() && j == 0);
            let mark = if is_cur { th.icon(Icon::Check) } else { " " };
            put(buf, x + 1, y, &format!("{mark}  {}", ellipsize(name, w as usize - 5)), if selected { th.selected() } else { Style::new() }, w);
            app.clicks.push((rect, Click::DeviceRow(i)));
            y += 1;
        }
    }
    let foot = "↑↓ select · ⏎ switch · Esc close";
    put(buf, x, inner.bottom() - 1, foot, th.muted(), w);
}
