//! Lyrics and queue views (shown in the main area).

use std::time::Duration;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::widgets::*;
use crate::app::{App, Click};
use crate::spotify::models::fmt_duration;
use crate::theme::Icon;

/// One-line header with the current song and a slim progress bar.
fn header(app: &mut App, buf: &mut Buffer, area: Rect, icon: Icon, title: &str) -> u16 {
    let th = app.theme.clone();
    put(buf, area.x + 2, area.y, &format!("{} {title}", th.icon(icon)), th.accent_bold(), area.width);
    if let Some(t) = app.pb.track.clone() {
        let s = format!("{} — {}", t.name, t.artist_names());
        let w = area.width.saturating_sub(width(title) + 8);
        let s = ellipsize(&s, w as usize);
        put(buf, area.right().saturating_sub(width(&s) + 2), area.y, &s, th.dim(), w);
        progress(app, buf, Rect { x: area.x + 2, y: area.y + 1, width: area.width.saturating_sub(4), height: 1 }, true);
    }
    2
}

pub fn lyrics(app: &mut App, buf: &mut Buffer, area: Rect) {
    let area = Rect { x: area.x + 1, y: area.y + 1, width: area.width.saturating_sub(2), height: area.height.saturating_sub(1) };
    let used = header(app, buf, area, Icon::Lyrics, "Lyrics");
    // Leave the last row for the attribution line.
    let body = Rect { y: area.y + used + 1, height: area.height.saturating_sub(used + 2), ..area };
    if body.height < 2 {
        return;
    }
    let th = app.theme.clone();
    let center_msg = |buf: &mut Buffer, msg: &str, style: Style| {
        let msg = ellipsize(msg, body.width as usize);
        put(buf, body.x + body.width.saturating_sub(width(&msg)) / 2, body.y + body.height / 3, &msg, style, body.width);
    };
    if app.pb.track.is_none() {
        center_msg(buf, "Nothing playing", th.dim());
        return;
    }
    if app.lyrics.loading {
        center_msg(buf, &format!("{} looking for lyrics…", th.spinner(app.frame)), th.dim());
        return;
    }
    let Some(lyr) = app.lyrics.data.clone() else {
        let msg = app.lyrics.error.clone().map(|e| format!("Couldn't load lyrics: {e}")).unwrap_or_else(|| "No lyrics found for this song".into());
        center_msg(buf, &msg, th.dim());
        return;
    };
    if lyr.instrumental {
        center_msg(buf, "♪  Instrumental  ♪", th.accent());
        return;
    }
    // Manual scroll expires after a few seconds of inactivity.
    if let Some((_, at)) = app.lyrics.manual
        && at.elapsed() > Duration::from_secs(5) && lyr.synced {
            app.lyrics.manual = None;
        }
    let pos = app.position();
    let current = lyr.current(pos);
    let focus = app.lyrics.manual.map(|(l, _)| l).or(current).unwrap_or(0);
    if app.cfg.lyrics.ascii {
        ascii_lyrics(app, buf, body, &lyr, current, focus, pos);
        let note = "lyrics: LRCLIB";
        put(buf, body.right().saturating_sub(width(note) + 1), area.y + area.height.saturating_sub(1), note, th.muted(), body.width);
        return;
    }
    let mid = body.height / 2;
    let start = focus as i32 - mid as i32;
    let center = app.cfg.lyrics.center;
    for row in 0..body.height {
        let i = start + row as i32;
        if i < 0 || i as usize >= lyr.lines.len() {
            continue;
        }
        let i = i as usize;
        let (time, text) = &lyr.lines[i];
        let text = if text.is_empty() { if th.unicode { "♪".to_string() } else { "~".into() } } else { text.clone() };
        let text = ellipsize(&text, body.width.saturating_sub(4) as usize);
        let tw = width(&text);
        let x = if center { body.x + body.width.saturating_sub(tw) / 2 } else { body.x + 4 };
        let y = body.y + row;
        let dist = (i as i32 - current.map(|c| c as i32).unwrap_or(-100)).unsigned_abs();
        let is_cur = current == Some(i);
        if is_cur && app.cfg.lyrics.karaoke {
            // Karaoke fill: the sung part is bright.
            let p = lyr.line_progress(i, pos);
            let split = (text.chars().count() as f32 * p).round() as usize;
            let sung: String = text.chars().take(split).collect();
            let rest: String = text.chars().skip(split).collect();
            let sw = put(buf, x, y, &sung, Style::new().fg(th.accent_bright).add_modifier(Modifier::BOLD), body.width);
            put(buf, x + sw, y, &rest, th.accent_bold(), body.width.saturating_sub(sw));
        } else {
            let style = if is_cur {
                th.accent_bold()
            } else if !lyr.synced || dist <= 2 {
                Style::new()
            } else if dist <= 5 {
                th.dim()
            } else {
                th.muted()
            };
            let style = if app.lyrics.manual.map(|(l, _)| l) == Some(i) { style.add_modifier(Modifier::UNDERLINED) } else { style };
            put(buf, x, y, &text, style, body.width);
        }
        if time.is_some() {
            app.clicks.push((Rect { x: body.x, y, width: body.width, height: 1 }, Click::LyricsLine(i)));
        }
    }
    let note = if lyr.synced { "lyrics: LRCLIB · ⏎ / click a line to jump" } else { "lyrics: LRCLIB · not synced" };
    put(buf, body.right().saturating_sub(width(note) + 1), area.y + area.height.saturating_sub(1), note, th.muted(), body.width);
}

/// One lyric line laid out in big text (possibly wrapped over several lines).
struct Block {
    lines: Vec<crate::font::BigText>,
    plain: Option<String>,
}

impl Block {
    fn height(&self) -> u16 {
        if self.plain.is_some() { 1 } else { self.lines.iter().map(|l| l.height() as u16).sum() }
    }
}

fn layout_line(text: &str, font: crate::font::FontId, width: u16, unicode: bool) -> Block {
    if text.trim().is_empty() {
        return Block { lines: Vec::new(), plain: Some(if unicode { "♪".into() } else { "~".into() }) };
    }
    for f in crate::font::fallbacks(font) {
        if let Some(lines) = wrap_big(text, f, width, 4) {
            return Block { lines, plain: None };
        }
    }
    Block { lines: Vec::new(), plain: Some(ellipsize(text, width as usize)) }
}

/// Lyrics drawn in an ASCII-art font: the current line is centered, with
/// a karaoke fill sweeping across it.
fn ascii_lyrics(
    app: &mut App,
    buf: &mut Buffer,
    body: Rect,
    lyr: &crate::lyrics::Lyrics,
    current: Option<usize>,
    focus: usize,
    pos: u64,
) {
    let th = app.theme.clone();
    let font = if app.cfg.lyrics.font.is_empty() { app.title_font() } else { crate::font::resolve(&app.cfg.lyrics.font) };
    let wrap_w = body.width.saturating_sub(4);
    let n = lyr.lines.len();
    if n == 0 {
        return;
    }
    let focus = focus.min(n - 1);
    let block = |i: usize| layout_line(&lyr.lines[i].1, font, wrap_w, th.unicode);
    let gap = 1u16;
    // Lay out around the focused line until the body is full.
    let mut placed: Vec<(usize, Block)> = vec![(focus, block(focus))];
    let mut used = placed[0].1.height();
    let (mut up, mut down) = (focus, focus);
    loop {
        let mut grew = false;
        if down + 1 < n {
            let b = block(down + 1);
            if used + gap + b.height() <= body.height {
                used += gap + b.height();
                down += 1;
                placed.push((down, b));
                grew = true;
            }
        }
        if up > 0 {
            let b = block(up - 1);
            if used + gap + b.height() <= body.height {
                used += gap + b.height();
                up -= 1;
                placed.insert(0, (up, b));
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    // Center the focused block vertically where possible.
    let above: u16 = placed.iter().take_while(|(i, _)| *i < focus).map(|(_, b)| b.height() + gap).sum();
    let focus_h = placed.iter().find(|(i, _)| *i == focus).map(|(_, b)| b.height()).unwrap_or(1);
    let ideal = (body.height.saturating_sub(focus_h)) / 2;
    let mut y = body.y + ideal.saturating_sub(above).min(body.height.saturating_sub(used));
    let manual = app.lyrics.manual.map(|(l, _)| l);
    for (i, b) in &placed {
        let i = *i;
        let h = b.height();
        if y + h > body.bottom() {
            break;
        }
        let is_cur = current == Some(i);
        let dist = (i as i32 - current.map(|c| c as i32).unwrap_or(-100)).unsigned_abs();
        let base = if is_cur {
            th.accent_bold()
        } else if !lyr.synced || dist <= 1 {
            Style::new()
        } else if dist <= 3 {
            th.dim()
        } else {
            th.muted()
        };
        let base = if manual == Some(i) && !is_cur { base.add_modifier(Modifier::UNDERLINED) } else { base };
        let progress = if is_cur && app.cfg.lyrics.karaoke { Some(lyr.line_progress(i, pos)) } else { None };
        let sung = Style::new().fg(th.accent_bright).add_modifier(Modifier::BOLD);
        if let Some(p) = &b.plain {
            let x = body.x + body.width.saturating_sub(width(p)) / 2;
            put(buf, x, y, p, base, body.width);
        } else {
            // The karaoke sweep runs across all wrapped lines of the block in order.
            let total_w: usize = b.lines.iter().map(|l| l.width()).sum();
            let mut done_w = 0usize;
            let mut yy = y;
            for line in &b.lines {
                let w = line.width() as u16;
                let x0 = body.x + body.width.saturating_sub(w) / 2;
                for (r, row) in line.lines.iter().enumerate() {
                    for (cx, ch) in row.iter().enumerate() {
                        if *ch == ' ' {
                            continue;
                        }
                        let filled = progress.is_some_and(|p| ((done_w + cx) as f32) < p * total_w as f32);
                        let style = if filled { sung } else { base };
                        if let Some(c) = buf.cell_mut((x0 + cx as u16, yy + r as u16)) {
                            c.set_char(*ch).set_style(style);
                        }
                    }
                }
                done_w += line.width();
                yy += line.height() as u16;
            }
        }
        if lyr.lines[i].0.is_some() {
            app.clicks.push((Rect { x: body.x, y, width: body.width, height: h }, Click::LyricsLine(i)));
        }
        y += h + gap;
    }
}

pub fn queue(app: &mut App, buf: &mut Buffer, area: Rect) {
    let area = Rect { x: area.x + 1, y: area.y + 1, width: area.width.saturating_sub(2), height: area.height.saturating_sub(1) };
    let used = header(app, buf, area, Icon::Queue, "Queue");
    let mut y = area.y + used + 1;
    let th = app.theme.clone();
    let bottom = area.bottom().saturating_sub(1);
    let x = area.x + 2;
    let w = area.width.saturating_sub(4);

    let section = |buf: &mut Buffer, y: u16, label: &str| {
        put(buf, x, y, label, th.bold().add_modifier(Modifier::UNDERLINED), w);
    };

    if let Some(t) = app.pb.track.clone() {
        section(buf, y, "Now playing");
        y += 1;
        let line = format!("{} {} — {}", eq_glyph(app), t.name, t.artist_names());
        put(buf, x + 1, y, &ellipsize(&line, w as usize - 1), th.accent_bold(), w);
        y += 2;
    }

    if let Some(c) = app.queue.committed.clone() {
        put(buf, x + 1, y, &ellipsize(&format!("→ {} — {}   (up next)", c.name, c.artist_names()), w as usize - 1), th.accent(), w);
        y += 1;
    }
    section(buf, y, &format!("Next in your queue ({})", app.queue.items.len()));
    y += 1;
    if app.queue.items.is_empty() {
        put(buf, x + 1, y, "Empty — add songs with Ctrl+A in search, or `queue add <song>`", th.dim(), w);
        y += 2;
    } else {
        let n = app.queue.items.len();
        let room = (bottom.saturating_sub(y + 4)).max(3) as usize;
        let sel = app.queue.list.selected.min(n - 1);
        let mut off = app.queue.list.offset;
        if sel < off {
            off = sel;
        }
        if sel >= off + room {
            off = sel + 1 - room;
        }
        app.queue.list.offset = off;
        for (i, t) in app.queue.items.clone().iter().enumerate().skip(off).take(room) {
            if y >= bottom {
                break;
            }
            let selected = i == sel;
            let rect = Rect { x, y, width: w, height: 1 };
            let style = if selected { th.selected() } else { Style::new() };
            if selected {
                fill(buf, rect, style);
            }
            let just_moved = selected && app.queue.list.moved_at.map(|m| app.motion.active(m, 250)).unwrap_or(false);
            let num = format!("{:>3}", i + 1);
            put(buf, x, y, &num, if selected { style } else { th.muted() }, 4);
            let dur = fmt_duration(t.duration_ms);
            let text = format!("{} — {}", t.name, t.artist_names());
            let tx = x + 5 + if just_moved { 1 } else { 0 };
            put(buf, tx, y, &ellipsize(&text, w.saturating_sub(12) as usize), if selected { style } else { th.bold() }, w.saturating_sub(12));
            put(buf, (x + w).saturating_sub(width(&dur) + 1), y, &dur, if selected { style } else { th.dim() }, 6);
            app.clicks.push((rect, Click::QueueRow(i)));
            y += 1;
        }
        y += 1;
    }

    if !app.queue.spotify.is_empty() && y + 2 < bottom {
        section(buf, y, "Then, from Spotify");
        y += 1;
        for t in app.queue.spotify.iter().take((bottom - y) as usize) {
            put(buf, x + 1, y, &ellipsize(&format!("{} — {}", t.name, t.artist_names()), w as usize - 1), th.dim(), w);
            y += 1;
        }
    }
    let help = if th.unicode {
        "↑↓ select · ⇧↑↓ move · Del remove · ⏎ play now · `queue clear`"
    } else {
        "Up/Down select - Shift+Up/Down move - Del remove - Enter play now"
    };
    put(buf, x, area.bottom().saturating_sub(1), &ellipsize(help, w as usize), th.muted(), w);
}
