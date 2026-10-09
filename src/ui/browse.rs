//! Search results and album / artist / playlist pages.

use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::widgets::*;
use crate::anim;
use crate::app::{App, Click, Focus, ListNav, PageKind};
use crate::config::SearchFilter;
use crate::font;
use crate::spotify::models::*;
use crate::theme::Icon;

const TOP_H: u16 = 8;

pub fn tabs(app: &mut App, buf: &mut Buffer, area: Rect) {
    let mut x = area.x + 2;
    if let Some(page) = app.search.stack.last() {
        // Breadcrumb.
        let back = if app.theme.unicode { "‹ Back" } else { "< Back" };
        put(buf, x, area.y, back, app.theme.key(), 6);
        app.clicks.push((Rect { x, y: area.y, width: 6, height: 1 }, Click::Back));
        x += 9;
        let mut crumbs: Vec<String> = Vec::new();
        if !app.search.query.is_empty() {
            crumbs.push(format!("“{}”", app.search.query));
        }
        crumbs.extend(app.search.stack.iter().map(|p| p.title.clone()));
        let sep = if app.theme.unicode { " › " } else { " > " };
        let text = crumbs.join(sep);
        let w = area.right().saturating_sub(x + 2);
        let text = if width(&text) > w {
            // Keep the end of the trail.
            let chars: Vec<char> = text.chars().collect();
            let keep = w.saturating_sub(1) as usize;
            format!("…{}", chars[chars.len().saturating_sub(keep)..].iter().collect::<String>())
        } else {
            text
        };
        put(buf, x, area.y, &text, app.theme.dim(), w);
        let _ = page;
        return;
    }
    for f in SearchFilter::ALL {
        let label = match f {
            SearchFilter::Tracks => "Songs",
            other => other.label(),
        };
        let w = width(label) + 2;
        let selected = *f == app.search.filter;
        let style = if selected { app.theme.selected() } else { app.theme.dim() };
        put(buf, x, area.y, &format!(" {label} "), style, w);
        app.clicks.push((Rect { x, y: area.y, width: w, height: 1 }, Click::Filter(*f)));
        x += w + 1;
    }
    let hint = "Tab ⇄";
    let hw = width(hint);
    if x + hw + 4 < area.right() {
        put(buf, area.right() - hw - 2, area.y, hint, app.theme.muted(), hw);
    }
}

pub fn draw(app: &mut App, buf: &mut Buffer, area: Rect) {
    let area = Rect { x: area.x + 1, y: area.y + 1, width: area.width.saturating_sub(2), height: area.height.saturating_sub(1) };
    if area.height < 2 {
        return;
    }
    if !app.search.stack.is_empty() {
        page(app, buf, area);
        return;
    }
    let s = &app.search;
    if s.items.is_empty() {
        let (msg, style) = if s.loading {
            (format!("{} Searching for “{}”…", app.theme.spinner(app.frame), s.input.value.trim()), app.theme.dim())
        } else if let Some(e) = &s.error {
            (format!("{} {e}", app.theme.icon(Icon::Cross)), app.theme.error())
        } else if s.query.is_empty() {
            empty_state(app, buf, area);
            return;
        } else {
            (format!("No results for “{}”", s.query), app.theme.dim())
        };
        let msg = ellipsize(&msg, area.width as usize);
        put(buf, area.x + (area.width - width(&msg)) / 2, area.y + area.height / 3, &msg, style, area.width);
        return;
    }
    let big_first = app.cfg.search.show_top_result;
    let items = app.search.items.clone();
    let mut nav = app.search.list;
    let shown = app.search.shown_at;
    list(app, buf, area, &items, &mut nav, big_first, None, shown);
    app.search.list = nav;
    if app.search.loading_more {
        let msg = format!("{} loading more", app.theme.spinner(app.frame));
        put(buf, area.right().saturating_sub(width(&msg) + 1), area.bottom() - 1, &msg, app.theme.dim(), area.width);
    }
}

fn empty_state(app: &mut App, buf: &mut Buffer, area: Rect) {
    let u = app.theme.unicode;
    let lines = [
        ("Find anything", app.theme.bold()),
        ("", Style::new()),
        ("Type to search songs, artists, albums and playlists.", app.theme.dim()),
        ("Enter or ↓ to browse results · Tab to switch filters · Esc to close", app.theme.dim()),
        ("", Style::new()),
        ("Paste a Spotify link to play it straight away.", app.theme.muted()),
    ];
    let y0 = area.y + area.height.saturating_sub(lines.len() as u16 + 4) / 3;
    for (i, (l, st)) in lines.iter().enumerate() {
        let l = if u { l.to_string() } else { l.replace('↓', "Down").replace('·', "-") };
        put(buf, area.x + area.width.saturating_sub(width(&l)) / 2, y0 + i as u16, &l, *st, area.width);
    }
    // Quick links.
    let links = [("Liked Songs", "liked"), ("Recently Played", "recent"), ("Your Library", "library")];
    let y = y0 + lines.len() as u16 + 1;
    let total: u16 = links.iter().map(|(l, _)| width(l) + 4).sum::<u16>() + 2 * (links.len() as u16 - 1);
    let mut x = area.x + area.width.saturating_sub(total) / 2;
    for (label, cmd) in links {
        let w = width(label) + 4;
        put(buf, x, y, &format!("[ {label} ]"), app.theme.accent(), w);
        app.clicks.push((Rect { x, y, width: w, height: 1 }, Click::Run(cmd)));
        x += w + 2;
    }
}

fn row_height(i: usize, big_first: bool) -> u16 {
    if i == 0 && big_first { TOP_H + 1 } else { 1 }
}

/// Draw a scrolling list. `numbers`: show track numbers (album pages).
#[allow(clippy::too_many_arguments)]
fn list(app: &mut App, buf: &mut Buffer, area: Rect, items: &[Item], nav: &mut ListNav, big_first: bool, numbers: Option<PageKind>, shown: Instant) {
    let focused = app.focus == Focus::SearchList;
    let sel = nav.selected.min(items.len().saturating_sub(1));
    // Keep the selection visible (rows have different heights).
    if sel < nav.offset {
        nav.offset = sel;
    }
    loop {
        let h: u16 = (nav.offset..=sel).map(|i| row_height(i, big_first)).sum();
        if h <= area.height || nav.offset >= sel {
            break;
        }
        nav.offset += 1;
    }
    // Show the section header above the first visible row when possible.
    if nav.offset > 0 && nav.offset == sel && matches!(items.get(sel.wrapping_sub(1)), Some(Item::Header(_))) {
        nav.offset -= 1;
    }

    let playing_uri = app.pb.track.as_ref().map(|t| t.uri.clone());
    let mut y = area.y;
    let mut track_no = 0usize;
    for (i, item) in items.iter().enumerate() {
        if let Item::Track(_) = item {
            track_no += 1;
        }
        if i < nav.offset {
            continue;
        }
        let h = row_height(i, big_first);
        if y + h > area.bottom() && !(i == nav.offset) {
            break;
        }
        let visible = (i - nav.offset) as u64;
        // Staggered entrance.
        let delay = visible * 22;
        let el = shown.elapsed().as_millis() as u64;
        if app.motion.enabled && el < delay {
            y += h;
            continue;
        }
        let t = if app.motion.enabled { ((el - delay) as f32 / (180.0 * app.motion.scale)).min(1.0) } else { 1.0 };
        let slide = ((1.0 - anim::ease_out_cubic(t)) * 6.0) as u16;
        let selected = i == sel && item.is_selectable();
        let rect = Rect { x: area.x, y, width: area.width, height: h.min(area.bottom() - y) };
        if i == 0 && big_first {
            top_card(app, buf, rect, item, selected && focused, shown);
        } else {
            row(app, buf, rect, item, selected, focused, slide, t < 1.0, playing_uri.as_deref(), numbers.map(|_| track_no));
        }
        if item.is_selectable() {
            app.clicks.push((rect, Click::Row(i)));
        }
        y += h;
        if y >= area.bottom() {
            break;
        }
    }
    // Scrollbar.
    let total: u16 = (0..items.len()).map(|i| row_height(i, big_first)).sum();
    if total > area.height && area.height > 2 {
        let track_h = area.height;
        let before: u16 = (0..nav.offset).map(|i| row_height(i, big_first)).sum();
        let thumb_h = ((area.height as f32 / total as f32) * track_h as f32).max(1.0) as u16;
        let thumb_y = ((before as f32 / total as f32) * track_h as f32) as u16;
        for dy in 0..track_h {
            let on = dy >= thumb_y && dy < thumb_y + thumb_h;
            let (ch, st) = if on { (if app.theme.unicode { '┃' } else { '#' }, app.theme.accent()) } else { (if app.theme.unicode { '│' } else { '|' }, app.theme.muted()) };
            if let Some(c) = buf.cell_mut((area.right() - 1, area.y + dy)) {
                c.set_char(ch).set_style(st);
            }
        }
    }
}

fn item_icon(app: &App, item: &Item) -> &'static str {
    match item {
        Item::Track(_) => app.theme.icon(Icon::Track),
        Item::Artist(_) => app.theme.icon(Icon::Artist),
        Item::Album(_) => app.theme.icon(Icon::Album),
        Item::Playlist(_) => app.theme.icon(Icon::Playlist),
        Item::Link { target: LinkTarget::PlayAll, .. } => app.theme.icon(Icon::Play),
        Item::Link { target: LinkTarget::ShuffleAll, .. } => app.theme.icon(Icon::Shuffle),
        Item::Link { target: LinkTarget::LikedSongs, .. } => app.theme.icon(Icon::Heart),
        Item::Link { target: LinkTarget::Recent, .. } => app.theme.icon(Icon::Clock),
        Item::Link { .. } => app.theme.icon(Icon::Pointer),
        Item::Header(_) => "",
    }
}

fn subtitle(item: &Item) -> String {
    match item {
        Item::Track(t) => {
            if t.album_name().is_empty() { t.artist_names() } else { format!("{} · {}", t.artist_names(), t.album_name()) }
        }
        Item::Artist(a) => {
            if a.genres.is_empty() {
                "Artist".into()
            } else {
                a.genres.iter().take(2).cloned().collect::<Vec<_>>().join(", ")
            }
        }
        Item::Album(a) => {
            let artists = a.artists.iter().map(|x| x.name.as_str()).collect::<Vec<_>>().join(", ");
            match a.year() {
                Some(y) => format!("{artists} · {y}"),
                None => artists,
            }
        }
        Item::Playlist(p) => format!("by {} · {} songs", p.owner_name(), p.total()),
        Item::Link { subtitle, .. } => subtitle.clone(),
        Item::Header(_) => String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn row(app: &mut App, buf: &mut Buffer, rect: Rect, item: &Item, selected: bool, focused: bool, slide: u16, entering: bool, playing: Option<&str>, number: Option<usize>) {
    let th = app.theme.clone();
    if let Item::Header(h) = item {
        let label = format!(" {h} ");
        let lw = width(&label);
        put(buf, rect.x + 1, rect.y, &label, th.bold().add_modifier(Modifier::UNDERLINED), lw);
        let line_ch = if th.unicode { "─" } else { "-" };
        let rest = rect.width.saturating_sub(lw + 4);
        put(buf, rect.x + 2 + lw, rect.y, &line_ch.repeat(rest as usize), th.muted(), rest);
        return;
    }
    let x = rect.x + slide;
    let w = rect.width.saturating_sub(slide + 1);
    let is_playing = playing.is_some_and(|p| item.uri() == Some(p));
    let base = if selected && focused {
        fill(buf, Rect { x: rect.x, y: rect.y, width: rect.width.saturating_sub(1), height: 1 }, th.selected());
        th.selected()
    } else if selected {
        th.bold()
    } else if entering {
        th.dim()
    } else {
        Style::new()
    };
    let marker = if selected { th.icon(Icon::Pointer) } else { " " };
    put(buf, x, rect.y, marker, if selected && focused { base } else { th.accent() }, 1);

    // Leading column: track number, now-playing equalizer, or icon.
    let lead = if is_playing {
        eq_glyph(app)
    } else if let (Some(n), Item::Track(_)) = (number, item) {
        format!("{n:>2}")
    } else {
        item_icon(app, item).to_string()
    };
    let lead_style = if selected && focused { base } else if is_playing { th.accent_bold() } else { th.accent() };
    put(buf, x + 2, rect.y, &lead, lead_style, 3);

    let right = match item {
        Item::Track(t) => fmt_duration(t.duration_ms),
        other => other.kind().to_string(),
    };
    let rw = width(&right);
    let name_x = x + 6;
    let avail = w.saturating_sub(6 + rw + 2);
    let name_w = (avail as f32 * 0.55) as u16;
    let mut name = item.name().to_string();
    if let Item::Track(t) = item
        && t.explicit {
            name = format!("{name} {}", th.icon(Icon::Explicit));
        }
    let name_style = if selected && focused { base } else if is_playing { th.accent_bold() } else { base.add_modifier(Modifier::BOLD) };
    let nw = put(buf, name_x, rect.y, &ellipsize(&name, name_w as usize), name_style, name_w);
    let sub = match (item, number.is_some()) {
        // Album pages: the album is already in the header.
        (Item::Track(t), true) if app.search.stack.last().map(|p| p.kind) == Some(PageKind::Album) => t.artist_names(),
        _ => subtitle(item),
    };
    let _ = nw;
    let sub_x = name_x + name_w + 2;
    let sub_w = (x + w).saturating_sub(sub_x + rw + 2);
    let sub_style = if selected && focused { base } else { th.dim() };
    put(buf, sub_x, rect.y, &ellipsize(&sub, sub_w as usize), sub_style, sub_w);
    let rstyle = if selected && focused { base } else { th.muted() };
    put(buf, (x + w).saturating_sub(rw + 1), rect.y, &right, rstyle, rw);
}

/// The big first result: cover + name in the ASCII font.
fn top_card(app: &mut App, buf: &mut Buffer, rect: Rect, item: &Item, selected: bool, shown: Instant) {
    let card = Rect { height: TOP_H.min(rect.height), ..rect };
    let card = Rect { width: card.width.saturating_sub(1), ..card };
    let label = if app.search.filter == SearchFilter::All { "Top result" } else { "Best match" };
    let inner = panel(app, buf, card, Some(label), selected);
    let url = item.image(300);
    let cover_rect = Rect { x: inner.x + 1, y: inner.y, width: (inner.height as f32 * app.images.cell_aspect()) as u16, height: inner.height };
    let c = cover(app, buf, cover_rect, url.as_deref(), shown, false);
    let x = c.right() + 3;
    let w = inner.right().saturating_sub(x + 1);
    // The card fits a title of up to three lines.
    let font = app.title_font();
    let font = if font::lines_for(font) > 3 { font::SLIM } else { font };
    let title_h = crate::font::lines_for(font);
    let mut y = inner.y + inner.height.saturating_sub(title_h + 3) / 2;
    big_title(app, buf, Rect { x, y, width: w, height: title_h }, item.name(), shown, false, font);
    y += title_h + 1;
    let kind = item.kind().to_uppercase();
    let kw = width(&kind) + 2;
    put(buf, x, y, &format!(" {kind} "), app.theme.selected(), kw);
    put(buf, x + kw + 1, y, &ellipsize(&subtitle(item), w.saturating_sub(kw + 1) as usize), app.theme.dim(), w.saturating_sub(kw + 1));
    y += 1;
    if y < inner.bottom() {
        let action = match item {
            Item::Track(_) => "⏎ play",
            _ => "⏎ open",
        };
        put(buf, x, y, action, if selected { app.theme.key() } else { app.theme.muted() }, w);
    }
}

fn page(app: &mut App, buf: &mut Buffer, area: Rect) {
    let Some(p) = app.search.stack.last() else { return };
    let (kind, title, subtitle, meta, image, opened, loading, error) = (
        p.kind,
        p.title.clone(),
        p.subtitle.clone(),
        p.meta.clone(),
        p.image.clone(),
        p.opened,
        p.loading,
        p.error.clone(),
    );
    let header_h = 9.min(area.height / 2).max(5);
    let header = Rect { height: header_h, ..area };
    // Artwork (or a big glyph for collections without one).
    let cover_w = (header_h as f32 * app.images.cell_aspect()) as u16;
    let cover_rect = Rect { x: header.x + 1, y: header.y, width: cover_w, height: header_h };
    let crect = if image.is_some() {
        cover(app, buf, cover_rect, image.as_deref(), opened, false)
    } else {
        let glyph = match kind {
            PageKind::Liked => app.theme.icon(Icon::Heart),
            PageKind::Recent => app.theme.icon(Icon::Clock),
            PageKind::Library => app.theme.icon(Icon::Playlist),
            _ => app.theme.icon(Icon::Track),
        };
        let inner = panel(app, buf, Rect { width: cover_w.max(8), ..cover_rect }, None, true);
        put(buf, inner.x + inner.width / 2, inner.y + inner.height / 2, glyph, app.theme.accent_bold(), 2);
        Rect { width: cover_w.max(8), ..cover_rect }
    };
    let x = crect.right() + 3;
    let w = header.right().saturating_sub(x + 1);
    let kind_label = match kind {
        PageKind::Album => "ALBUM",
        PageKind::Artist => "ARTIST",
        PageKind::Playlist => "PLAYLIST",
        PageKind::Liked => "COLLECTION",
        PageKind::Recent => "HISTORY",
        PageKind::Top => "FOR YOU",
        PageKind::Library => "LIBRARY",
    };
    let font = app.title_font();
    let title_h = crate::font::lines_for(font).min(header_h.saturating_sub(4));
    let content_h = title_h + 4;
    let mut y = header.y + header_h.saturating_sub(content_h) / 2;
    put(buf, x, y, kind_label, app.theme.accent(), w);
    y += 1;
    let font = if title_h < crate::font::lines_for(font) { font::SLIM } else { font };
    let used = big_title(app, buf, Rect { x, y, width: w, height: title_h.max(1) }, &title, opened, false, font);
    y += used + 1;
    put(buf, x, y, &ellipsize(&subtitle, w as usize), app.theme.bold(), w);
    y += 1;
    put(buf, x, y, &ellipsize(&meta, w as usize), app.theme.dim(), w);

    let list_area = Rect { x: area.x, y: area.y + header_h + 1, width: area.width, height: area.height.saturating_sub(header_h + 1) };
    if loading {
        put(buf, list_area.x + 2, list_area.y, &format!("{} loading…", app.theme.spinner(app.frame)), app.theme.dim(), list_area.width);
        return;
    }
    if let Some(e) = error {
        put(buf, list_area.x + 2, list_area.y, &format!("{} {e}", app.theme.icon(Icon::Cross)), app.theme.error(), list_area.width);
        return;
    }
    let p = app.search.stack.last().unwrap();
    let items = p.items.clone();
    let mut nav = p.list;
    let numbers = if kind == PageKind::Album || kind == PageKind::Playlist || kind == PageKind::Liked { Some(kind) } else { None };
    list(app, buf, list_area, &items, &mut nav, false, numbers, opened);
    if let Some(p) = app.search.stack.last_mut() {
        p.list = nav;
    }
}
