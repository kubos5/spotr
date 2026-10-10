//! Transient effects: toasts, on-screen badges, particles, splash and goodbye.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Clear, Widget};

use super::widgets::*;
use crate::anim;
use crate::app::{App, Click, ToastKind};
use crate::theme::Icon;

pub fn toasts(app: &mut App, buf: &mut Buffer, area: Rect) {
    let th = app.theme.clone();
    let mut y = area.y + 3;
    let max_w = (area.width / 2).clamp(24, 60);
    for i in (0..app.toasts.len()).rev() {
        let t = &app.toasts[i];
        let age = t.born.elapsed();
        let ttl = t.ttl;
        let (icon, color) = match t.kind {
            ToastKind::Info => (th.icon(Icon::Info), th.accent),
            ToastKind::Success => (th.icon(Icon::Check), Color::Green),
            ToastKind::Warn => ("!", Color::Yellow),
            ToastKind::Error => (th.icon(Icon::Cross), Color::Red),
        };
        // Wrap the message into at most 3 lines.
        let msg_w = max_w.saturating_sub(6) as usize;
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for word in t.msg.split_whitespace() {
            if width(&cur) as usize + word.len() + 1 > msg_w && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        lines.truncate(3);
        let h = lines.len() as u16 + 2;
        let w = (lines.iter().map(|l| width(l)).max().unwrap_or(0) + 6).min(max_w);
        // Slide in from the right; fade at the end.
        let tin = app.motion.t(t.born, 260);
        let slide = ((1.0 - anim::ease_out_cubic(tin)) * (w + 2) as f32) as u16;
        let leaving = app.motion.enabled && ttl.saturating_sub(age).as_millis() < 300;
        let x = area.right().saturating_sub(w + 1) + slide;
        if y + h > area.bottom() {
            break;
        }
        let rect = Rect { x, y, width: w.min(area.right().saturating_sub(x)), height: h };
        if rect.width < 4 {
            y += h;
            continue;
        }
        Clear.render(rect, buf);
        let mut border = Style::new().fg(color);
        if leaving {
            border = border.add_modifier(Modifier::DIM);
        }
        let block = ratatui::widgets::Block::bordered().border_type(th.border_type).border_style(border);
        let inner = block.inner(rect);
        block.render(rect, buf);
        put(buf, inner.x + 1, inner.y, icon, Style::new().fg(color).add_modifier(Modifier::BOLD), 2);
        let text_style = if leaving { th.dim() } else { Style::new() };
        let tw_t = app.motion.t(t.born, 350);
        for (li, l) in lines.iter().enumerate() {
            let s = if app.motion.text_effects && li == 0 { anim::typewriter(l, tw_t) } else { l.clone() };
            put(buf, inner.x + 3, inner.y + li as u16, &s, text_style, inner.width.saturating_sub(3));
        }
        app.clicks.push((rect, Click::Toast(i)));
        y += h;
    }
}

/// Big centered badge for quick actions.
pub fn osd(app: &mut App, buf: &mut Buffer, area: Rect) {
    let Some(o) = &app.osd else { return };
    let th = app.theme.clone();
    let t = app.motion.t(o.born, 1000);
    let appear = anim::ease_out_back((t * 4.0).min(1.0));
    let fading = t > 0.75;
    let text = format!("{}  {}", o.icon, o.text);
    let base_w = (width(&text) + 6).max(if o.value.is_some() { 30 } else { 0 });
    let w = ((base_w as f32) * appear.clamp(0.3, 1.1)) as u16;
    let h = if o.value.is_some() { 4 } else { 3 };
    let rect = Rect { x: area.x + area.width.saturating_sub(w) / 2, y: area.y + area.height / 4, width: w.min(area.width), height: h };
    Clear.render(rect, buf);
    let mut border = th.border(true);
    if fading {
        border = border.add_modifier(Modifier::DIM);
    }
    let block = ratatui::widgets::Block::bordered().border_type(th.border_type).border_style(border);
    let inner = block.inner(rect);
    block.render(rect, buf);
    let style = if fading { th.accent().add_modifier(Modifier::DIM) } else { th.accent_bold() };
    let s = ellipsize(&text, inner.width as usize);
    put(buf, inner.x + inner.width.saturating_sub(width(&s)) / 2, inner.y, &s, style, inner.width);
    if let Some(v) = o.value {
        let bw = inner.width.saturating_sub(4);
        let filled = (v.clamp(0.0, 1.0) * bw as f32).round() as u16;
        for i in 0..bw {
            let (ch, st) = if i < filled {
                (if th.unicode { '█' } else { '#' }, th.accent())
            } else {
                (if th.unicode { '░' } else { '.' }, th.muted())
            };
            if let Some(c) = buf.cell_mut((inner.x + 2 + i, inner.y + 1)) {
                c.set_char(ch).set_style(st);
            }
        }
    }
}

pub fn particles(app: &mut App, buf: &mut Buffer, area: Rect) {
    for p in &app.particles {
        let x = p.x.round() as i32;
        let y = p.y.round() as i32;
        if x < area.x as i32 || y < area.y as i32 || x >= area.right() as i32 || y >= area.bottom() as i32 {
            continue;
        }
        let age = p.born.elapsed().as_secs_f32() / p.life;
        let mut style = Style::new().fg(if p.ch == '♥' || p.ch == '♡' { Color::Red } else { app.theme.accent_bright });
        if age > 0.6 {
            style = style.add_modifier(Modifier::DIM);
        }
        let ch = if app.theme.unicode { p.ch } else { '*' };
        if let Some(c) = buf.cell_mut((x as u16, y as u16)) {
            c.set_char(ch).set_style(style);
        }
    }
}

pub fn splash(app: &mut App, buf: &mut Buffer, area: Rect) {
    let Some(start) = app.splash else { return };
    let t = (start.elapsed().as_secs_f32() / 1.2).min(1.0);
    let h = crate::font::lines_for(app.title_font()).max(1);
    let y = area.y + area.height.saturating_sub(h + 4) / 2;
    let y = y + brand_title(app, buf, Rect { x: area.x, y, width: area.width, height: h }, "SPOTR", start);
    let tag = "a better Spotify client, for the terminal";
    let shown = anim::typewriter(tag, ((t - 0.3) / 0.6).clamp(0.0, 1.0));
    put(buf, area.x + area.width.saturating_sub(width(tag)) / 2, y + 1, &shown, app.theme.dim(), area.width);
    // A little wave of palette colors underneath.
    let pal = app.theme.spectrum();
    let wave_w = 24u16.min(area.width);
    let x0 = area.x + (area.width - wave_w) / 2;
    for i in 0..wave_w {
        let phase = anim::clock() * 6.0 + i as f32 * 0.5;
        let lvl = ((phase.sin() * 0.5 + 0.5) * 7.0) as usize;
        let ch = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'][lvl.min(7)];
        if let Some(c) = buf.cell_mut((x0 + i, y + 3)) {
            c.set_char(if app.theme.unicode { ch } else { '=' }).set_style(Style::new().fg(pal[i as usize % pal.len()]));
        }
    }
}

pub fn quit(app: &mut App, buf: &mut Buffer, area: Rect) {
    let Some(at) = app.quit_at else { return };
    let t = (at.elapsed().as_secs_f32() / 0.45).min(1.0);
    // Wipe the screen from the edges toward the middle.
    let cols = ((area.width as f32 / 2.0) * anim::ease_in_out(t)) as u16;
    for y in area.top()..area.bottom() {
        for dx in 0..cols {
            for x in [area.x + dx, area.right().saturating_sub(1 + dx)] {
                if let Some(c) = buf.cell_mut((x, y)) {
                    c.set_char(' ').set_style(Style::reset());
                }
            }
        }
    }
    let msg = if app.theme.unicode { "♪  see you soon  ♪" } else { "see you soon" };
    put(buf, area.x + area.width.saturating_sub(width(msg)) / 2, area.y + area.height / 2, msg, app.theme.accent_bold(), area.width);
}
