//! Reusable drawing helpers: big titles, cover art, progress bars, boxes.

use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Widget};
use unicode_width::UnicodeWidthStr;

use crate::anim;
use crate::app::{App, Click};
use crate::config::{CoverMode, LongTitle};
use crate::font::FontId;
use crate::font;
use crate::spotify::models::fmt_duration;

/// Write a string clipped to `max` cells. Returns the width written.
pub fn put(buf: &mut Buffer, x: u16, y: u16, s: &str, style: Style, max: u16) -> u16 {
    if max == 0 || y >= buf.area.bottom() || x >= buf.area.right() {
        return 0;
    }
    let max = max.min(buf.area.right() - x);
    let (end, _) = buf.set_stringn(x, y, s, max as usize, style);
    end - x
}

pub fn width(s: &str) -> u16 {
    UnicodeWidthStr::width(s) as u16
}

/// Truncate with an ellipsis to fit `max` cells.
pub fn ellipsize(s: &str, max: usize) -> String {
    if UnicodeWidthStr::width(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw + 1 > max {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

pub fn fill(buf: &mut Buffer, area: Rect, style: Style) {
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(c) = buf.cell_mut((x, y)) {
                c.set_char(' ').set_style(style);
            }
        }
    }
}

pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

/// A bordered panel in the app's style; returns the inner rect.
pub fn panel(app: &App, buf: &mut Buffer, area: Rect, title: Option<&str>, focused: bool) -> Rect {
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(app.theme.border_type)
        .border_style(app.theme.border(focused));
    if let Some(t) = title {
        block = block.title(Line::from(vec![Span::raw(" "), Span::styled(t.to_string(), app.theme.accent_bold()), Span::raw(" ")]));
    }
    let inner = block.inner(area);
    block.render(area, buf);
    inner
}

/// A modal popup with a cleared background.
pub fn popup(app: &mut App, buf: &mut Buffer, area: Rect, title: &str) -> Rect {
    Clear.render(area, buf);
    let inner = panel(app, buf, area, Some(title), true);
    // Close button.
    let close = if app.theme.unicode { "✕" } else { "x" };
    let cx = area.right().saturating_sub(4);
    put(buf, cx, area.y, &format!(" {close} "), app.theme.dim(), 3);
    app.clicks.push((Rect { x: cx, y: area.y, width: 3, height: 1 }, Click::CloseOverlay));
    inner
}

/// How a title will be laid out.
pub enum TitlePlan {
    /// One or more lines of big text (wrapped at word boundaries).
    Big(Vec<font::BigText>),
    /// One line of big text scrolling horizontally.
    Marquee(font::BigText),
    Plain,
}

impl TitlePlan {
    pub fn rows(&self) -> u16 {
        match self {
            TitlePlan::Big(lines) => lines.iter().map(|l| l.height() as u16).sum(),
            TitlePlan::Marquee(b) => b.height() as u16,
            TitlePlan::Plain => 1,
        }
    }
}

/// Greedy word wrap of big text into at most `max_lines` lines that fit `width`.
pub fn wrap_big(text: &str, f: FontId, width: u16, max_lines: usize) -> Option<Vec<font::BigText>> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let fits = |s: &str| font::render(s, f, 1).map(|b| b.width() as u16 <= width).unwrap_or(false);
    for word in text.split_whitespace() {
        let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if fits(&candidate) {
            cur = candidate;
        } else {
            if cur.is_empty() || !fits(word) {
                return None;
            }
            lines.push(std::mem::replace(&mut cur, word.to_string()));
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() || lines.len() > max_lines {
        return None;
    }
    lines.iter().map(|l| font::render(l, f, 1)).collect()
}

/// Decide how to show `text` in at most `width` x `height` cells.
pub fn title_plan(text: &str, width: u16, height: u16, pref: FontId, long: LongTitle) -> TitlePlan {
    let fonts = font::fallbacks(pref);
    let mut first_overflow = None;
    for &f in &fonts {
        let Some(b) = font::render(text, f, 1).filter(|b| b.width() > 0) else { continue };
        let h = b.height() as u16;
        if h > height {
            continue;
        }
        if b.width() as u16 <= width {
            return TitlePlan::Big(vec![b]);
        }
        if long == LongTitle::Shrink {
            // Wrap onto a second line before giving up on this font.
            if let Some(lines) = wrap_big(text, f, width, 2)
                && lines.iter().map(|l| l.height() as u16).sum::<u16>() <= height
            {
                return TitlePlan::Big(lines);
            }
        }
        if first_overflow.is_none() {
            first_overflow = Some(b);
        }
        if long != LongTitle::Shrink {
            break;
        }
    }
    match (long, first_overflow) {
        (LongTitle::Marquee, Some(b)) => TitlePlan::Marquee(b),
        _ => TitlePlan::Plain,
    }
}

/// Rows a title will need (for layout).
pub fn title_rows(app: &App, text: &str, width: u16, max_height: u16, pref: FontId) -> u16 {
    title_plan(text, width, max_height, pref, app.cfg.appearance.long_title).rows()
}

/// Draw a title with the configured ASCII font. Returns rows used.
/// `text` in the user's title font, centered. Returns the rows used.
pub fn brand_title(app: &App, buf: &mut Buffer, area: Rect, text: &str, born: Instant) -> u16 {
    let font = app.title_font();
    let h = font::lines_for(font).max(1).min(buf.area.bottom().saturating_sub(area.y));
    big_title(app, buf, Rect { height: h, ..area }, text, born, true, font)
}

pub fn big_title(app: &App, buf: &mut Buffer, area: Rect, text: &str, born: Instant, center: bool, font_pref: FontId) -> u16 {
    if area.width == 0 || area.height == 0 || text.is_empty() {
        return 0;
    }
    let t = app.motion.t(born, 650);
    let plan = title_plan(text, area.width, area.height, font_pref, app.cfg.appearance.long_title);
    let (lines, marquee) = match plan {
        TitlePlan::Big(lines) => (lines, false),
        TitlePlan::Marquee(b) => (vec![b], true),
        TitlePlan::Plain => {
            let s = if app.motion.text_effects && t < 1.0 { anim::scramble(text, t, app.frame, app.theme.unicode) } else { text.to_string() };
            let s = ellipsize(&s, area.width as usize);
            let x = if center { area.x + (area.width - width(&s)) / 2 } else { area.x };
            put(buf, x, area.y, &s, app.theme.accent_bold(), area.width);
            return 1;
        }
    };
    let mut y = area.y;
    let total_w = lines.iter().map(|b| b.width()).max().unwrap_or(0) as u16;
    for big in &lines {
        draw_big(app, buf, Rect { y, height: area.bottom().saturating_sub(y), ..area }, big, t, center, marquee, total_w);
        y += big.height() as u16;
    }
    y - area.y
}

#[allow(clippy::too_many_arguments)]
fn draw_big(app: &App, buf: &mut Buffer, area: Rect, big: &font::BigText, t: f32, center: bool, marquee: bool, total_w: u16) {
    let w = big.width() as u16;
    let overflow = marquee && w > area.width;
    // Marquee: scroll the text with a gap, pausing at the start.
    let period = big.width() + 8;
    let shift = if overflow { (((anim::clock() - 1.5).max(0.0) * 12.0) as usize) % period } else { 0 };
    let x0 = if center && !overflow { area.x + area.width.saturating_sub(w) / 2 } else { area.x };
    let band = if app.cfg.appearance.title_shimmer && app.motion.enabled {
        let cycle = 4.5;
        let p = (anim::clock() % cycle) / cycle;
        Some(p * (total_w as f32 + 30.0) - 15.0)
    } else {
        None
    };
    let reveal_cols = if t < 1.0 { (anim::ease_out_cubic(t) * total_w as f32) as usize } else { usize::MAX };
    let unicode = app.theme.unicode;
    let cols = if overflow { area.width } else { w.min(area.width) };
    for (row, line) in big.lines.iter().enumerate() {
        let y = area.y + row as u16;
        if y >= area.bottom() {
            break;
        }
        for cx in 0..cols {
            let src = if overflow {
                let i = (cx as usize + shift) % period;
                if i >= big.width() { ' ' } else { line[i] }
            } else {
                line[cx as usize]
            };
            let x = x0 + cx;
            if x >= area.right() {
                break;
            }
            let col = cx as usize;
            if col > reveal_cols {
                continue;
            }
            let mut ch = src;
            if col == reveal_cols && t < 1.0 {
                ch = if unicode { '▌' } else { '|' };
            }
            if ch == ' ' {
                continue;
            }
            if !unicode {
                ch = match ch {
                    '█' => '#',
                    '▀' => '"',
                    '▄' => '_',
                    '▌' => '|',
                    c => c,
                };
            }
            let mut style = Style::new().fg(app.theme.accent);
            if let Some(b) = band {
                let d = (cx as f32 + row as f32 * 1.5 - b).abs();
                if d < 3.0 {
                    style = Style::new().fg(app.theme.accent_bright).add_modifier(Modifier::BOLD);
                }
            }
            if let Some(c) = buf.cell_mut((x, y)) {
                c.set_char(ch).set_style(style);
            }
        }
    }
}

/// Cover art (or a placeholder) inside `avail`; returns the rect used.
pub fn cover(app: &mut App, buf: &mut Buffer, avail: Rect, url: Option<&str>, born: Instant, clickable: bool) -> Rect {
    let rect = app.images.cover_rect(avail);
    if rect.width == 0 || rect.height == 0 {
        return rect;
    }
    if clickable {
        app.clicks.push((rect, Click::Cover));
    }
    let Some(url) = url else {
        crate::images::render_placeholder(rect, buf, app.theme.unicode, anim::clock() * 0.6);
        return rect;
    };
    app.want_image(url);
    if app.images.get(url).is_none() {
        crate::images::render_placeholder(rect, buf, app.theme.unicode, anim::clock() * 1.5);
        return rect;
    }
    let modal = app.overlay != crate::app::Overlay::None || app.splash.is_some() || app.quit_at.is_some();
    let use_graphics = app.cfg.appearance.cover_mode == CoverMode::Image && app.images.graphics && !modal;
    if use_graphics {
        app.images.render_graphics(url, rect, buf);
    } else {
        let art = app.cfg.appearance.text_art;
        let pix = app.cfg.appearance.pixelation;
        app.images.render_text(url, rect, buf, art, pix, modal);
        // Dissolve-in when the art changes.
        let t = app.motion.t(born, 700);
        if t < 1.0 {
            for y in rect.top()..rect.bottom() {
                for x in rect.left()..rect.right() {
                    let h = anim::hash(((x as u64) << 16) ^ y as u64) % 1000;
                    if (h as f32 / 1000.0) > anim::ease_in_out(t)
                        && let Some(c) = buf.cell_mut((x, y)) {
                            c.set_char(' ').set_style(Style::reset());
                        }
                }
            }
        }
    }
    rect
}

/// `1:23 ━━━━━━●────── 3:45` — registers a seek target.
pub fn progress(app: &mut App, buf: &mut Buffer, area: Rect, show_times: bool) {
    if area.width < 8 || area.height == 0 {
        return;
    }
    let pos = app.position();
    let dur = app.pb.duration();
    let (l, r) = (fmt_duration(pos), fmt_duration(dur));
    let (bar_x, bar_w) = if show_times {
        let lw = width(&l) + 1;
        let rw = width(&r) + 1;
        put(buf, area.x, area.y, &l, app.theme.dim(), lw);
        put(buf, area.right() - rw + 1, area.y, &r, app.theme.dim(), rw);
        (area.x + lw, area.width.saturating_sub(lw + rw))
    } else {
        (area.x, area.width)
    };
    if bar_w < 2 {
        return;
    }
    let frac = if dur > 0 { (pos as f64 / dur as f64).clamp(0.0, 1.0) } else { 0.0 };
    let filled = frac * bar_w as f64;
    let knob = (filled.floor() as u16).min(bar_w - 1);
    let unicode = app.theme.unicode;
    let playing = app.pb.is_playing;
    for i in 0..bar_w {
        let x = bar_x + i;
        let (ch, style) = if i < knob {
            (if unicode { '━' } else { '=' }, app.theme.accent())
        } else if i == knob {
            let pulse = if playing && app.motion.enabled { anim::pulse(1600) } else { 1.0 };
            let ch = if unicode { if pulse > 0.5 { '●' } else { '◉' } } else { 'o' };
            (ch, Style::new().fg(app.theme.accent_bright).add_modifier(Modifier::BOLD))
        } else {
            (if unicode { '─' } else { '-' }, app.theme.muted())
        };
        if let Some(c) = buf.cell_mut((x, area.y)) {
            c.set_char(ch).set_style(style);
        }
    }
    app.clicks.push((Rect { x: bar_x, y: area.y, width: bar_w, height: 1 }, Click::Progress { x: bar_x, width: bar_w }));
}

/// A tiny meter like `▮▮▮▮▯▯▯▯`.
pub fn meter(value: f32, cells: usize, unicode: bool) -> String {
    let n = (value.clamp(0.0, 1.0) * cells as f32).round() as usize;
    let (on, off) = if unicode { ('▮', '▯') } else { ('#', '-') };
    std::iter::repeat_n(on, n).chain(std::iter::repeat_n(off, cells - n)).collect()
}

/// Animated "now playing" equalizer glyph for list rows.
pub fn eq_glyph(app: &App) -> String {
    if !app.theme.unicode {
        return ">".into();
    }
    if !app.pb.is_playing || !app.motion.enabled {
        return "▮".into();
    }
    const BARS: [char; 5] = ['▁', '▃', '▅', '▇', '▅'];
    let t = (anim::clock() * 8.0) as usize;
    (0..3).map(|i| BARS[(t + i * 2) % BARS.len()]).collect()
}

