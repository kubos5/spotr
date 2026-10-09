//! The now-playing view and the compact player used while searching.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::widgets::*;
use crate::anim;
use crate::app::{App, Click, Ctl, LocalStatus};
use crate::config::Layout;
use crate::font::FontId;
use crate::keys::Action;
use crate::theme::Icon;

const INFO_MIN_W: u16 = 34;

pub fn draw(app: &mut App, buf: &mut Buffer, area: Rect) {
    let area = Rect { x: area.x + 2, y: area.y + 1, width: area.width.saturating_sub(4), height: area.height.saturating_sub(1) };
    if area.height < 4 {
        return;
    }
    let Some(track) = app.pb.track.clone() else {
        idle(app, buf, area);
        return;
    };

    let font = app.title_font();
    // Rough info width, to know whether the title will wrap.
    let guess_w = area.width.saturating_sub(((area.height.min(22)) as f32 * app.images.cell_aspect()) as u16 + 3).min(72);
    let title_h = title_rows(app, &track.name, guess_w.max(INFO_MIN_W), crate::font::lines_for(font) * 2, font);
    // title, gap, artist, album, gap, progress, gap, then either
    //   big:     3-row controls, gap, volume, status   (= +12)
    //   compact: 1-row controls, status                (= +8)
    let big = app.cfg.appearance.big_controls && app.theme.unicode && area.height >= title_h + 12;
    let info_h = title_h + if big { 12 } else { 8 };
    let show_cover = app.cfg.appearance.show_cover;
    let aspect = app.images.cell_aspect();

    let side_cover_h = area.height.min(22);
    let side_cover_w = (side_cover_h as f32 * aspect).round() as u16;
    let side_ok = area.width >= side_cover_w + 3 + INFO_MIN_W;
    let stacked_cover_h = area.height.saturating_sub(info_h + 1);
    let layout = match app.cfg.appearance.layout {
        Layout::Auto => {
            if !show_cover || side_ok && (area.width as f32 / area.height as f32) > 2.2 || stacked_cover_h < 6 {
                Layout::Side
            } else {
                Layout::Stacked
            }
        }
        l => l,
    };
    let born = app.track_changed_at;
    let url = track.image(300);

    let info_area = if !show_cover {
        let w = area.width.min(90);
        Rect { x: area.x + (area.width - w) / 2, y: area.y + area.height.saturating_sub(info_h) / 2, width: w, height: info_h.min(area.height) }
    } else if layout == Layout::Side && side_ok {
        // Cover on the left, info on the right; the pair is centered.
        let info_w = (area.width - side_cover_w - 3).min(72);
        let total = side_cover_w + 3 + info_w;
        let x0 = area.x + (area.width - total) / 2;
        let cover_area = Rect { x: x0, y: area.y + (area.height - side_cover_h) / 2, width: side_cover_w, height: side_cover_h };
        cover(app, buf, cover_area, url.as_deref(), born, true);
        let ih = info_h.min(area.height);
        Rect { x: x0 + side_cover_w + 3, y: area.y + (area.height - ih) / 2, width: info_w, height: ih }
    } else {
        let ch = stacked_cover_h.max(3);
        let cover_area = Rect { x: area.x, y: area.y, width: area.width, height: ch };
        cover(app, buf, cover_area, url.as_deref(), born, true);
        let w = area.width.min(80);
        Rect { x: area.x + (area.width - w) / 2, y: area.y + ch + 1, width: w, height: area.bottom().saturating_sub(area.y + ch + 1) }
    };
    let centered = layout != Layout::Side || !show_cover;
    info(app, buf, info_area, &track, font, centered, title_h, big);
}

#[allow(clippy::too_many_arguments)]
fn info(app: &mut App, buf: &mut Buffer, area: Rect, track: &crate::spotify::models::Track, font: FontId, center: bool, title_h: u16, big: bool) {
    let born = app.track_changed_at;
    let mut y = area.y;
    let title_area = Rect { x: area.x, y, width: area.width, height: title_h.max(1) };
    let used = big_title(app, buf, title_area, &track.name, born, center, font);
    y += used.max(1) + 1;

    let line = |app: &App, buf: &mut Buffer, y: u16, s: &str, style: Style| {
        let s = ellipsize(s, area.width as usize);
        let x = if center { area.x + (area.width - width(&s)) / 2 } else { area.x };
        put(buf, x, y, &s, style, area.width);
        let _ = app;
    };

    // Artist line, decoded in.
    let t = app.motion.t(born, 900);
    let mut artists = track.artist_names();
    if track.explicit {
        artists = format!("{artists}  {}", app.theme.icon(Icon::Explicit));
    }
    let artists = if app.motion.text_effects { anim::scramble(&artists, t, app.frame, app.theme.unicode) } else { artists };
    line(app, buf, y, &artists, app.theme.bold());
    y += 1;
    let album = match track.album.as_ref() {
        Some(a) if !a.name.is_empty() => match a.year() {
            Some(yr) => format!("{} · {}", a.name, yr),
            None => a.name.clone(),
        },
        _ => String::new(),
    };
    let album = if app.motion.text_effects { anim::scramble(&album, (t - 0.15).max(0.0) / 0.85, app.frame + 7, app.theme.unicode) } else { album };
    line(app, buf, y, &album, app.theme.dim());
    y += 2;
    if y >= area.bottom() {
        return;
    }
    progress(app, buf, Rect { x: area.x, y, width: area.width, height: 1 }, true);
    // Breathing room between the progress bar and the controls.
    y += 2;
    if y >= area.bottom() {
        return;
    }
    if big && area.width >= BIG_CONTROLS_W && y + 3 < area.bottom() {
        // 4 rows: the icons plus the gap below, which the press dip can use.
        big_controls(app, buf, Rect { x: area.x, y, width: area.width, height: 4 }, center);
        y += 4;
        if y >= area.bottom() {
            return;
        }
        volume_row(app, buf, Rect { x: area.x, y, width: area.width, height: 1 }, center);
    } else {
        controls(app, buf, Rect { x: area.x, y, width: area.width, height: 1 }, center);
    }
    y += 1;
    if y >= area.bottom() {
        return;
    }
    let mut status = Vec::new();
    if let Some(d) = &app.pb.device {
        status.push(format!("{} {}", app.theme.icon(Icon::Device), d.name));
    }
    if let Some(ctx) = &app.pb.context_uri {
        let kind = ctx.split(':').nth(1).unwrap_or("");
        let label = match kind {
            "album" => "from album",
            "playlist" => "from playlist",
            "artist" => "from artist",
            "collection" | "user" => "from Liked Songs",
            _ => "",
        };
        if !label.is_empty() {
            status.push(label.to_string());
        }
    }
    if !app.queue.items.is_empty() {
        status.push(format!("{} {} queued", app.theme.icon(Icon::Queue), app.queue.items.len()));
    }
    if let Some(at) = app.sleep_at {
        let mins = at.saturating_duration_since(std::time::Instant::now()).as_secs().div_ceil(60);
        status.push(format!("☾ sleep in {mins}m"));
    }
    line(app, buf, y, &status.join("   "), app.theme.muted());
}

/// Width the big controls need.
const BIG_CONTROLS_W: u16 = 54;

type Bitmap = [&'static str; 6];

/// Turn a 6-row pixel bitmap into 4 rows of half blocks: three for the icon,
/// plus one so `dip` can push the picture down one pixel (half a cell) for the
/// "pressed" effect without cutting off its bottom edge.
fn halfblocks(rows: Bitmap, dip: bool) -> [String; 4] {
    let px = |r: usize, x: usize| {
        let r = if dip { r.checked_sub(1) } else { Some(r) };
        r.and_then(|r| rows.get(r)).and_then(|row| row.as_bytes().get(x)) == Some(&b'#')
    };
    let w = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    std::array::from_fn(|line| {
        (0..w)
            .map(|x| match (px(line * 2, x), px(line * 2 + 1, x)) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            })
            .collect()
    })
}

fn bitmap(c: Ctl, app: &App) -> Bitmap {
    // Play and pause share a width so nothing shifts when toggling.
    match c {
        Ctl::Prev => ["#...#", "#..##", "#.###", "#.###", "#..##", "#...#"],
        Ctl::Next => ["#...#", "##..#", "###.#", "###.#", "##..#", "#...#"],
        Ctl::Play if app.pb.is_playing => ["##..##", "##..##", "##..##", "##..##", "##..##", "##..##"],
        Ctl::Play => ["##....", "####..", "######", "######", "####..", "##...."],
        Ctl::Like if app.saved_current == Some(true) => [".##.##.", "#######", "#######", ".#####.", "..###..", "...#..."],
        Ctl::Like => [".##.##.", "#..#..#", "#.....#", ".#...#.", "..#.#..", "...#..."],
        Ctl::Shuffle => ["##...###", "..#.#..#", "...#....", "...#....", "..#.#..#", "##...###"],
        // An open circle with an arrowhead (↻); repeat-one adds a "1" as text.
        Ctl::Repeat => ["..####..", ".#....#.", "#....###", "#.....#.", ".#......", "..####.."],
    }
}

/// Which controls to show, in order, with the gap after each.
fn control_set(app: &App) -> Vec<(Ctl, u16)> {
    let a = &app.cfg.appearance;
    let mut v = Vec::new();
    if a.show_shuffle {
        v.push((Ctl::Shuffle, 4));
    }
    v.extend([(Ctl::Prev, 4), (Ctl::Play, 4), (Ctl::Next, if a.show_repeat { 4 } else { 7 })]);
    if a.show_repeat {
        v.push((Ctl::Repeat, 7));
    }
    v.push((Ctl::Like, 0));
    v
}

/// Large transport controls, three rows tall.
fn big_controls(app: &mut App, buf: &mut Buffer, area: Rect, center: bool) {
    let th = app.theme.clone();
    let off = th.muted();
    let set = control_set(app);
    let style_of = |c: Ctl| match c {
        Ctl::Shuffle if app.pb.shuffle => th.accent_bold(),
        Ctl::Repeat if app.pb.repeat != "off" => th.accent_bold(),
        Ctl::Shuffle | Ctl::Repeat => off,
        Ctl::Prev | Ctl::Next => th.text(),
        Ctl::Play => Style::new().fg(th.accent_bright).add_modifier(Modifier::BOLD),
        Ctl::Like if app.saved_current == Some(true) => Style::new().fg(ratatui::style::Color::Red).add_modifier(Modifier::BOLD),
        Ctl::Like => off,
    };
    let total: u16 = set.iter().map(|(c, gap)| bitmap(*c, app)[0].len() as u16 + gap).sum();
    let mut x = if center { area.x + area.width.saturating_sub(total) / 2 } else { area.x };
    for (ctl, gap) in set {
        let bm = bitmap(ctl, app);
        let w = bm[0].len() as u16;
        // Pressed: the button dips half a cell and glows, then springs back.
        let t = app.pressed.filter(|(c, _)| *c == ctl).map(|(_, at)| app.motion.t(at, 320));
        let pressed = t.is_some_and(|t| t < 1.0);
        let dip = t.is_some_and(|t| t < 0.45);
        let mut style = style_of(ctl);
        if pressed {
            let glow = if ctl == Ctl::Like && app.saved_current == Some(true) { ratatui::style::Color::LightRed } else { th.accent_bright };
            style = Style::new().fg(glow).add_modifier(Modifier::BOLD);
        }
        let mut rows = halfblocks(bm, dip);
        if ctl == Ctl::Repeat && app.pb.repeat == "track" {
            // Repeat-one: a "1" in the middle of the loop. Text can't move half a
            // cell, so while dipped it becomes "₁", which sits lower in its cell.
            let mut mid: Vec<char> = rows[1].chars().collect();
            if let Some(c) = mid.get_mut(3) {
                *c = if dip { '₁' } else { '1' };
            }
            rows[1] = mid.into_iter().collect();
        }
        for (row, line) in rows.iter().enumerate() {
            // Blank cells are skipped so the spare 4th row never paints over anything.
            for (i, ch) in line.chars().enumerate() {
                if ch != ' '
                    && let Some(cell) = buf.cell_mut((x + i as u16, area.y + row as u16))
                {
                    cell.set_char(ch).set_style(style);
                }
            }
        }
        let click = match ctl {
            Ctl::Shuffle => Click::Shuffle,
            Ctl::Prev => Click::Action(Action::PrevTrack),
            Ctl::Play => Click::PlayPause,
            Ctl::Next => Click::Action(Action::NextTrack),
            Ctl::Repeat => Click::Repeat,
            Ctl::Like => Click::Like,
        };
        if ctl == Ctl::Like {
            app.like_pos = Some((x + w / 2, area.y + 1));
        }
        app.clicks.push((Rect { x, y: area.y, width: w, height: 3 }, click));
        x += w + gap;
    }
}

/// `♪ ▮▮▮▮▮▮▯▯▯▯▯▯  60%   » 1.25×`
fn volume_row(app: &mut App, buf: &mut Buffer, area: Rect, center: bool) {
    let th = app.theme.clone();
    let vol = app.pb.volume.unwrap_or(0);
    let icon = format!("{} ", th.icon(if vol == 0 { Icon::Mute } else { Icon::Volume }));
    let bar = meter(vol as f32 / 100.0, 16, th.unicode);
    let pct = format!(" {vol:>3}%");
    let speed = if (app.speed - 1.0).abs() > f32::EPSILON { format!("    {} {}×", th.icon(Icon::Speed), app.speed) } else { String::new() };
    let total = width(&icon) + width(&bar) + width(&pct) + width(&speed);
    let mut x = if center { area.x + area.width.saturating_sub(total) / 2 } else { area.x };
    x += put(buf, x, area.y, &icon, th.dim(), area.width);
    let bw = put(buf, x, area.y, &bar, th.accent(), area.right().saturating_sub(x));
    app.clicks.push((Rect { x, y: area.y, width: bw, height: 1 }, Click::Volume { x, width: bw }));
    x += bw;
    x += put(buf, x, area.y, &pct, th.dim(), area.right().saturating_sub(x));
    put(buf, x, area.y, &speed, th.accent_bold(), area.right().saturating_sub(x));
}

/// `⤮  ⏮  ▶  ⏭  ↻    ♥    ♪ ▮▮▮▮▯▯ 60%   » 1.0×`
fn controls(app: &mut App, buf: &mut Buffer, area: Rect, center: bool) {
    let th = app.theme.clone();
    let on = th.accent_bold();
    let off = th.muted();
    // Pad to the same width so the row doesn't shift when toggling.
    let (p, q) = (th.icon(Icon::Pause), th.icon(Icon::Play));
    let pw = width(p).max(width(q)) as usize;
    let play_icon = format!("{:<pw$}", if app.pb.is_playing { p } else { q });
    let repeat_icon = if app.pb.repeat == "track" { th.icon(Icon::RepeatOne) } else { th.icon(Icon::Repeat) };
    let liked = app.saved_current.unwrap_or(false);
    let heart = if liked { th.icon(Icon::Heart) } else { th.icon(Icon::HeartEmpty) };
    let vol = app.pb.volume.unwrap_or(0);
    let vol_meter = meter(vol as f32 / 100.0, 8, th.unicode);
    let speed = app.speed;

    let mut items: Vec<(String, Style, Option<Click>)> = Vec::new();
    if app.cfg.appearance.show_shuffle {
        items.push((th.icon(Icon::Shuffle).into(), if app.pb.shuffle { on } else { off }, Some(Click::Shuffle)));
        items.push(("  ".into(), Style::new(), None));
    }
    items.extend([
        (th.icon(Icon::Prev).into(), th.text(), Some(Click::Action(Action::PrevTrack))),
        ("  ".into(), Style::new(), None),
        (play_icon, on, Some(Click::PlayPause)),
        ("  ".into(), Style::new(), None),
        (th.icon(Icon::Next).into(), th.text(), Some(Click::Action(Action::NextTrack))),
    ]);
    if app.cfg.appearance.show_repeat {
        items.push(("  ".into(), Style::new(), None));
        items.push((repeat_icon.into(), if app.pb.repeat != "off" { on } else { off }, Some(Click::Repeat)));
    }
    items.extend([
        ("    ".into(), Style::new(), None),
        (heart.into(), if liked { Style::new().fg(ratatui::style::Color::Red).add_modifier(Modifier::BOLD) } else { off }, Some(Click::Like)),
        ("    ".into(), Style::new(), None),
        (format!("{} ", th.icon(if vol == 0 { Icon::Mute } else { Icon::Volume })), th.dim(), None),
        (vol_meter, th.accent(), Some(Click::Volume { x: 0, width: 0 })),
        (format!(" {vol:>3}%"), th.dim(), None),
    ]);
    if (speed - 1.0).abs() > f32::EPSILON {
        items.push(("   ".into(), Style::new(), None));
        items.push((format!("{} {speed}×", th.icon(Icon::Speed)), on, None));
    }
    let total: u16 = items.iter().map(|(s, _, _)| width(s)).sum();
    let mut x = if center { area.x + area.width.saturating_sub(total) / 2 } else { area.x };
    for (s, style, click) in items {
        let w = width(&s);
        if x + w > area.right() {
            break;
        }
        put(buf, x, area.y, &s, style, w);
        if let Some(c) = click {
            let rect = Rect { x, y: area.y, width: w, height: 1 };
            let c = match c {
                Click::Volume { .. } => Click::Volume { x, width: w },
                other => other,
            };
            if c == Click::Like {
                app.like_pos = Some((x, area.y));
            }
            app.clicks.push((rect, c));
        }
        x += w;
    }
}

/// Nothing is playing.
fn idle(app: &mut App, buf: &mut Buffer, area: Rect) {
    let font = app.title_font();
    let th = crate::font::lines_for(font);
    let h = th + 6;
    let y0 = area.y + area.height.saturating_sub(h) / 2;
    big_title(app, buf, Rect { x: area.x, y: y0, width: area.width, height: th }, "spotr", app.started, true, font);
    let lines: Vec<(String, Style)> = vec![
        (
            match (&app.pb.error, app.pb.loaded) {
                (Some(e), false) => format!("{} Can't reach Spotify: {e}", app.theme.icon(Icon::Cross)),
                (_, true) => "Nothing is playing".into(),
                (None, false) => format!("{} connecting to Spotify", app.theme.spinner(app.frame)),
            },
            if app.pb.error.is_some() && !app.pb.loaded { app.theme.error() } else { app.theme.bold() },
        ),
        (String::new(), Style::new()),
        (
            format!(
                "{} search   ·   {} library   ·   {} devices   ·   {} help",
                app.keymap.hint(Action::FocusSearch, app.theme.unicode),
                app.keymap.hint(Action::Library, app.theme.unicode),
                app.keymap.hint(Action::Devices, app.theme.unicode),
                app.keymap.hint(Action::Help, app.theme.unicode)
            ),
            app.theme.dim(),
        ),
        (
            match &app.local_status {
                LocalStatus::Ready => format!("“{}” is ready as a Spotify Connect device", app.cfg.general.device_name),
                LocalStatus::Connecting => format!("{} starting the built-in player…", app.theme.spinner(app.frame)),
                LocalStatus::NeedsLogin => "The built-in player needs a one-time sign-in — run `player login`".into(),
                LocalStatus::Failed(e) => format!("Built-in player unavailable: {e}"),
                LocalStatus::Off => "Play something on any Spotify device and it will show up here".into(),
            },
            app.theme.muted(),
        ),
    ];
    for (i, (s, style)) in lines.iter().enumerate() {
        let s = ellipsize(s, area.width as usize);
        let y = y0 + th + 1 + i as u16;
        if y >= area.bottom() {
            break;
        }
        put(buf, area.x + (area.width - width(&s)) / 2, y, &s, *style, area.width);
    }
}

/// Compact player shown at the bottom while searching.
pub fn mini(app: &mut App, buf: &mut Buffer, area: Rect) {
    // Slide up into place when search opens.
    let t = app.motion.t(app.search.shown_at, 350);
    let off = ((1.0 - anim::ease_out_cubic(t)) * area.height as f32) as u16;
    let area = Rect { y: area.y + off, height: area.height - off, ..area };
    if area.height < 3 {
        return;
    }
    let inner = panel(app, buf, area, None, false);
    let Some(track) = app.pb.track.clone() else {
        put(buf, inner.x + 2, inner.y + 1, "Nothing playing", app.theme.dim(), inner.width);
        return;
    };
    let cover_h = inner.height;
    let cover_w = ((cover_h as f32) * app.images.cell_aspect()).round() as u16;
    let url = track.image(64);
    let crect = cover(app, buf, Rect { x: inner.x + 1, y: inner.y, width: cover_w, height: cover_h }, url.as_deref(), app.track_changed_at, true);
    let x = crect.right() + 2;
    let w = inner.right().saturating_sub(x + 1);
    let play = if app.pb.is_playing { app.theme.icon(Icon::Play) } else { app.theme.icon(Icon::Pause) };
    let liked = if app.saved_current == Some(true) { format!(" {}", app.theme.icon(Icon::Heart)) } else { String::new() };
    put(buf, x, inner.y, &format!("{play} "), app.theme.accent_bold(), 2);
    put(buf, x + 2, inner.y, &ellipsize(&format!("{}{liked}", track.name), w.saturating_sub(2) as usize), app.theme.accent_bold(), w.saturating_sub(2));
    if inner.height > 1 {
        let sub = format!("{} · {}", track.artist_names(), track.album_name());
        put(buf, x + 2, inner.y + 1, &ellipsize(&sub, w.saturating_sub(2) as usize), app.theme.dim(), w.saturating_sub(2));
    }
    if inner.height > 2 {
        progress(app, buf, Rect { x: x + 2, y: inner.bottom() - 1, width: w.saturating_sub(2).min(70), height: 1 }, true);
    }
    app.clicks.push((Rect { x, y: inner.y, width: 2, height: 1 }, Click::PlayPause));
}
