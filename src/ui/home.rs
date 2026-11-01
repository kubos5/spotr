//! A local library's home: every song (or album, artist…) in the folder,
//! shown instead of "Nothing is playing".

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::browse::list;
use super::widgets::*;
use crate::app::{App, Click, HomeTab};
use crate::font;
use crate::keys::Action;
use crate::theme::Icon;

pub fn draw(app: &mut App, buf: &mut Buffer, area: Rect) {
    let area = Rect { x: area.x + 2, y: area.y + 1, width: area.width.saturating_sub(4), height: area.height.saturating_sub(1) };
    if area.height < 6 || app.offline.is_none() {
        return;
    }
    let th = app.theme.clone();
    let (name, root, songs, albums, artists, total_ms, scanning, scanned, tab, shown) = {
        let o = app.offline.as_ref().unwrap();
        (
            o.name.clone(),
            o.root.display().to_string(),
            o.lib.entries.len(),
            o.lib.albums.len(),
            o.lib.artists.len(),
            o.lib.total_ms,
            o.scanning,
            o.scanned,
            o.home.tab,
            o.home.shown,
        )
    };

    // Header: what this library is.
    let mut y = area.y;
    let label = format!("{} LOCAL LIBRARY", th.icon(Icon::Playlist));
    put(buf, area.x, y, &label, th.accent(), area.width);
    let path = ellipsize_start(&root, area.width.saturating_sub(width(&label) + 4) as usize);
    put(buf, area.right().saturating_sub(width(&path)), y, &path, th.muted(), area.width);
    y += 1;
    let font = app.title_font();
    let font = if area.height >= 18 && font::rows(&name, font) <= 3 { font } else { font::PLAIN };
    let title_h = font::rows(&name, font);
    let born = app.track_changed_at;
    let used = big_title(app, buf, Rect { x: area.x, y, width: area.width, height: title_h }, &name, born, false, font);
    y += used.max(1);
    let meta = match scanning {
        Some((done, total)) if total > 0 => format!("{} reading new files… {done} / {total}", th.spinner(app.frame)),
        Some(_) if !scanned => format!("{} looking for music…", th.spinner(app.frame)),
        _ => {
            let n = |n: usize, w: &str| if n == 1 { format!("1 {w}") } else { format!("{} {w}s", thousands(n)) };
            format!("{} · {} · {} · {}", n(songs, "song"), n(albums, "album"), n(artists, "artist"), long_duration(total_ms))
        }
    };
    put(buf, area.x, y, &ellipsize(&meta, area.width as usize), th.dim(), area.width);
    y += 2;
    if y + 3 >= area.bottom() {
        return;
    }

    // Tabs.
    let mut x = area.x;
    for (i, t) in HomeTab::ALL.iter().enumerate() {
        let label = format!(" {} ", t.label());
        let w = width(&label);
        if x + w >= area.right() {
            break;
        }
        let style = if *t == tab { th.selected() } else { th.dim() };
        put(buf, x, y, &label, style, w);
        app.clicks.push((Rect { x, y, width: w, height: 1 }, Click::HomeTab(i)));
        x += w + 1;
    }
    let hint = if th.unicode { "Tab ⇄" } else { "Tab" };
    if x + width(hint) + 2 < area.right() {
        put(buf, area.right().saturating_sub(width(hint)), y, hint, th.muted(), width(hint));
    }
    y += 2;

    // The list, with a key reminder underneath.
    let list_area = Rect { x: area.x.saturating_sub(1), y, width: area.width + 1, height: area.bottom().saturating_sub(y + 1) };
    let items = app.offline.as_mut().map(|o| std::mem::take(&mut o.home.items)).unwrap_or_default();
    if items.is_empty() {
        let (msg, sub) = empty_message(app, tab, songs, scanning.is_some() && !scanned);
        let my = list_area.y + list_area.height / 3;
        let msg = ellipsize(&msg, area.width as usize);
        put(buf, area.x + area.width.saturating_sub(width(&msg)) / 2, my, &msg, th.bold(), area.width);
        let sub = ellipsize(&sub, area.width as usize);
        put(buf, area.x + area.width.saturating_sub(width(&sub)) / 2, my + 2, &sub, th.dim(), area.width);
    } else {
        let mut nav = app.offline.as_ref().map(|o| o.home.nav).unwrap_or_default();
        // Liked songs in red, except in the Liked Songs tab where they all are.
        let highlight = app.cfg.search.highlight_liked && tab != HomeTab::Liked;
        list(app, buf, list_area, &items, &mut nav, false, None, shown, true, highlight, Click::HomeRow);
        if let Some(o) = app.offline.as_mut() {
            o.home.nav = nav;
        }
    }
    if let Some(o) = app.offline.as_mut() {
        o.home.items = items;
    }
    let u = th.unicode;
    let keys = format!(
        "{} play · {} like · {} queue · {} search · {} lyrics",
        if u { "⏎" } else { "Enter" },
        app.keymap.hint(Action::Like, u),
        app.keymap.hint(Action::AddToQueue, u),
        app.keymap.hint(Action::FocusSearch, u),
        app.keymap.hint(Action::Lyrics, u),
    );
    put(buf, area.x, area.bottom().saturating_sub(1), &ellipsize(&keys, area.width as usize), th.muted(), area.width);
}

fn empty_message(app: &App, tab: HomeTab, songs: usize, scanning: bool) -> (String, String) {
    if scanning {
        return (format!("{} Looking for music in this folder…", app.theme.spinner(app.frame)), "MP3, FLAC, Ogg, WAV, AIFF and M4A files, in every subfolder".into());
    }
    if songs == 0 {
        return ("No music here yet".into(), "Add MP3, FLAC, Ogg, WAV, AIFF or M4A files to this folder, then run `rescan`".into());
    }
    match tab {
        HomeTab::Liked => ("No liked songs yet".into(), format!("Press {} on a song to like it", app.keymap.hint(Action::Like, app.theme.unicode))),
        HomeTab::Playlists => ("No playlists yet".into(), "`playlist new <name>` creates one, `addto <name>` adds the selected song".into()),
        HomeTab::Recent => ("Nothing played yet".into(), "Songs you play show up here".into()),
        _ => ("Nothing to show".into(), String::new()),
    }
}

/// "3 h 12 min", "2 days 4 h".
fn long_duration(ms: u64) -> String {
    let m = ms / 60_000;
    match m {
        0..=59 => format!("{m} min"),
        60..=1439 => format!("{} h {} min", m / 60, m % 60),
        _ => format!("{} days {} h", m / 1440, (m % 1440) / 60),
    }
}

/// 1234 → "1,234".
fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn formats() {
        assert_eq!(super::thousands(1234567), "1,234,567");
        assert_eq!(super::thousands(999), "999");
        assert_eq!(super::long_duration(3 * 3_600_000 + 12 * 60_000), "3 h 12 min");
        assert_eq!(super::long_duration(50 * 3_600_000), "2 days 2 h");
    }
}
