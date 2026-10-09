//! Tiny bitmap fonts rendered with half-block characters.
//!
//! * `Slim`  — 4 pixel rows → 2 terminal lines
//! * `Block` — 5 pixel rows → 3 terminal lines
//!
//! Glyphs are stored as rows of `#` (on) and `.` (off); widths may vary.

use std::path::Path;
use std::sync::OnceLock;

use crate::figlet::Figlet;

/// A title font, identified by a short id (`"block"`, `"ansi-compact"`, …).
pub type FontId = &'static str;

pub const BLOCK: FontId = "block";
pub const SLIM: FontId = "slim";
pub const PLAIN: FontId = "plain";

/// FIGlet / TOIlet fonts compiled into Spotr (see assets/fonts/README.md).
pub const BUNDLED: &[(&str, &str)] = &[
    ("ansi-compact", include_str!("../assets/fonts/ansi-compact.flf")),
    ("future", include_str!("../assets/fonts/future.tlf")),
    ("pagga", include_str!("../assets/fonts/pagga.tlf")),
    ("emboss", include_str!("../assets/fonts/emboss.tlf")),
    ("small-block", include_str!("../assets/fonts/small-block.tlf")),
    ("small-braille", include_str!("../assets/fonts/small-braille.tlf")),
    ("mini", include_str!("../assets/fonts/mini.flf")),
];

enum Kind {
    Block,
    Slim,
    Plain,
    Figlet(Box<Figlet>),
}

struct Entry {
    id: FontId,
    name: String,
    kind: Kind,
    /// Tallest a rendered line can be.
    lines: u16,
    user: bool,
}

static REGISTRY: OnceLock<Vec<Entry>> = OnceLock::new();

fn pretty(id: &str) -> String {
    id.split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn figlet_entry(id: FontId, name: String, f: Figlet, user: bool) -> Entry {
    let lines = f.render("AQgjpqy|").len().max(1) as u16;
    Entry { id, name, kind: Kind::Figlet(Box::new(f)), lines, user }
}

/// Load the built-in fonts plus any `.flf` / `.tlf` files in `user_dir`.
pub fn init(user_dir: Option<&Path>) {
    let _ = REGISTRY.set(build(user_dir));
}

fn build(user_dir: Option<&Path>) -> Vec<Entry> {
    let mut v = vec![
        Entry { id: BLOCK, name: "Block".into(), kind: Kind::Block, lines: 3, user: false },
        Entry { id: SLIM, name: "Slim".into(), kind: Kind::Slim, lines: 2, user: false },
    ];
    for (id, src) in BUNDLED {
        match Figlet::parse(src) {
            Ok(f) => v.push(figlet_entry(id, pretty(id).replace("Ansi", "ANSI"), f, false)),
            Err(e) => log::warn!("bundled font {id}: {e}"),
        }
    }
    if let Some(dir) = user_dir {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .map(|d| d.filter_map(|e| e.ok()).map(|e| e.path()).collect())
            .unwrap_or_default();
        files.sort();
        for path in files {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
            if ext != "flf" && ext != "tlf" {
                continue;
            }
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("font").to_string();
            let id_s = stem.to_lowercase().replace([' ', '_'], "-");
            if v.iter().any(|e| e.id == id_s) {
                continue;
            }
            let bytes = match std::fs::read(&path) {
                Ok(b) => b,
                Err(e) => {
                    log::warn!("font {}: {e}", path.display());
                    continue;
                }
            };
            // FIGlet fonts may be Latin-1; TOIlet fonts are UTF-8.
            let text = String::from_utf8(bytes.clone()).unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect());
            match Figlet::parse(&text) {
                Ok(f) => {
                    let id: FontId = Box::leak(id_s.into_boxed_str());
                    v.push(figlet_entry(id, pretty(&stem), f, true));
                }
                Err(e) => log::warn!("font {}: {e}", path.display()),
            }
        }
    }
    v.push(Entry { id: PLAIN, name: "Plain text".into(), kind: Kind::Plain, lines: 1, user: false });
    v
}

fn registry() -> &'static [Entry] {
    REGISTRY.get_or_init(|| build(None))
}

fn entry(id: &str) -> &'static Entry {
    let r = registry();
    r.iter().find(|e| e.id == id).unwrap_or(&r[0])
}

/// All fonts, in display order: `(id, name, user-provided)`.
pub fn all() -> impl Iterator<Item = (FontId, &'static str, bool)> {
    registry().iter().map(|e| (e.id, e.name.as_str(), e.user))
}

/// Find a font by id or name (case-insensitive, prefix allowed). Unknown names give `None`.
pub fn find(name: &str) -> Option<FontId> {
    let q = name.trim().to_lowercase().replace([' ', '_'], "-");
    if q.is_empty() {
        return None;
    }
    let r = registry();
    r.iter()
        .find(|e| e.id == q || e.name.to_lowercase().replace(' ', "-") == q)
        .or_else(|| r.iter().find(|e| e.id.starts_with(&q)))
        .map(|e| e.id)
}

/// The configured font, falling back to Block if it no longer exists.
pub fn resolve(name: &str) -> FontId {
    find(name).unwrap_or(BLOCK)
}

pub fn label(id: &str) -> &'static str {
    entry(id).name.as_str()
}

pub fn cycle(id: &str, dir: i32) -> FontId {
    let r = registry();
    let i = r.iter().position(|e| e.id == id).unwrap_or(0) as i32;
    r[(i + dir).rem_euclid(r.len() as i32) as usize].id
}

/// Fonts to try, in order, when a title doesn't fit.
pub fn fallbacks(id: FontId) -> Vec<FontId> {
    match id {
        PLAIN => vec![],
        SLIM => vec![SLIM],
        _ if lines_for(id) > 2 => vec![id, SLIM],
        _ => vec![id],
    }
}

type Glyph = &'static [&'static str];

fn slim(c: char) -> Option<Glyph> {
    Some(match c {
        'A' => &[".#.", "#.#", "###", "#.#"],
        'B' => &["##.", "###", "#.#", "###"],
        'C' => &[".##", "#..", "#..", ".##"],
        'D' => &["##.", "#.#", "#.#", "##."],
        'E' => &["###", "#..", "##.", "###"],
        'F' => &["###", "##.", "#..", "#.."],
        'G' => &[".##", "#..", "#.#", ".##"],
        'H' => &["#.#", "###", "#.#", "#.#"],
        'I' => &["###", ".#.", ".#.", "###"],
        'J' => &["..#", "..#", "#.#", ".#."],
        'K' => &["#.#", "##.", "#.#", "#.#"],
        'L' => &["#..", "#..", "#..", "###"],
        'M' => &["#...#", "##.##", "#.#.#", "#...#"],
        'N' => &["#..#", "##.#", "#.##", "#..#"],
        'O' => &[".#.", "#.#", "#.#", ".#."],
        'P' => &["##.", "#.#", "##.", "#.."],
        'Q' => &[".#.", "#.#", "#.#", ".##"],
        'R' => &["##.", "#.#", "##.", "#.#"],
        'S' => &[".##", "#..", "..#", "##."],
        'T' => &["###", ".#.", ".#.", ".#."],
        'U' => &["#.#", "#.#", "#.#", "###"],
        'V' => &["#.#", "#.#", "#.#", ".#."],
        'W' => &["#...#", "#...#", "#.#.#", ".#.#."],
        'X' => &["#.#", ".#.", ".#.", "#.#"],
        'Y' => &["#.#", ".#.", ".#.", ".#."],
        'Z' => &["###", "..#", "#..", "###"],
        '0' => &["###", "#.#", "#.#", "###"],
        '1' => &[".#.", "##.", ".#.", "###"],
        '2' => &["##.", "..#", ".#.", "###"],
        '3' => &["###", ".##", "..#", "###"],
        '4' => &["#.#", "#.#", "###", "..#"],
        '5' => &["###", "#..", "..#", "##."],
        '6' => &["#..", "###", "#.#", "###"],
        '7' => &["###", "..#", ".#.", ".#."],
        '8' => &["###", "###", "#.#", "###"],
        '9' => &["###", "#.#", "###", "..#"],
        ' ' => &["..", "..", "..", ".."],
        '.' => &[".", ".", ".", "#"],
        ',' => &[".", ".", "#", "#"],
        ':' => &[".", "#", ".", "#"],
        ';' => &[".", "#", ".", "#"],
        '!' => &["#", "#", ".", "#"],
        '?' => &["##.", "..#", ".#.", "..."],
        '\'' => &["#", "#", ".", "."],
        '"' => &["#.#", "#.#", "...", "..."],
        '-' => &["...", "###", "...", "..."],
        '_' => &["...", "...", "...", "###"],
        '+' => &["...", ".#.", "###", ".#."],
        '=' => &["...", "###", "...", "###"],
        '/' => &["..#", ".#.", ".#.", "#.."],
        '\\' => &["#..", ".#.", ".#.", "..#"],
        '(' => &[".#", "#.", "#.", ".#"],
        ')' => &["#.", ".#", ".#", "#."],
        '[' => &["##", "#.", "#.", "##"],
        ']' => &["##", ".#", ".#", "##"],
        '&' => &[".#.", "#.#", ".#.", "#.#"],
        '#' => &["#.#", "###", "#.#", "###"],
        '$' => &[".##", "##.", ".##", "##."],
        '%' => &["#.#", "..#", "#..", "#.#"],
        '*' => &["#.#", ".#.", "#.#", "..."],
        '@' => &["###", "#.#", "#..", "###"],
        '<' => &["..#", ".#.", "#..", ".#."],
        '>' => &["#..", ".#.", "..#", ".#."],
        '|' => &["#", "#", "#", "#"],
        '~' => &["....", ".#.#", "#.#.", "...."],
        _ => return None,
    })
}

fn block(c: char) -> Option<Glyph> {
    Some(match c {
        'A' => &[".#.", "#.#", "###", "#.#", "#.#"],
        'B' => &["##.", "#.#", "##.", "#.#", "##."],
        'C' => &[".##", "#..", "#..", "#..", ".##"],
        'D' => &["##.", "#.#", "#.#", "#.#", "##."],
        'E' => &["###", "#..", "##.", "#..", "###"],
        'F' => &["###", "#..", "##.", "#..", "#.."],
        'G' => &[".##", "#..", "#.#", "#.#", ".##"],
        'H' => &["#.#", "#.#", "###", "#.#", "#.#"],
        'I' => &["###", ".#.", ".#.", ".#.", "###"],
        'J' => &["..#", "..#", "..#", "#.#", ".#."],
        'K' => &["#.#", "#.#", "##.", "#.#", "#.#"],
        'L' => &["#..", "#..", "#..", "#..", "###"],
        'M' => &["#...#", "##.##", "#.#.#", "#...#", "#...#"],
        'N' => &["#..#", "##.#", "#.##", "#..#", "#..#"],
        'O' => &[".#.", "#.#", "#.#", "#.#", ".#."],
        'P' => &["##.", "#.#", "##.", "#..", "#.."],
        'Q' => &[".#.", "#.#", "#.#", "##.", ".##"],
        'R' => &["##.", "#.#", "##.", "#.#", "#.#"],
        'S' => &[".##", "#..", ".#.", "..#", "##."],
        'T' => &["###", ".#.", ".#.", ".#.", ".#."],
        'U' => &["#.#", "#.#", "#.#", "#.#", "###"],
        'V' => &["#.#", "#.#", "#.#", "#.#", ".#."],
        'W' => &["#...#", "#...#", "#.#.#", "#.#.#", ".#.#."],
        'X' => &["#.#", "#.#", ".#.", "#.#", "#.#"],
        'Y' => &["#.#", "#.#", ".#.", ".#.", ".#."],
        'Z' => &["###", "..#", ".#.", "#..", "###"],
        '0' => &["###", "#.#", "#.#", "#.#", "###"],
        '1' => &[".#.", "##.", ".#.", ".#.", "###"],
        '2' => &["##.", "..#", ".#.", "#..", "###"],
        '3' => &["##.", "..#", ".#.", "..#", "##."],
        '4' => &["#.#", "#.#", "###", "..#", "..#"],
        '5' => &["###", "#..", "##.", "..#", "##."],
        '6' => &[".##", "#..", "###", "#.#", "###"],
        '7' => &["###", "..#", ".#.", ".#.", ".#."],
        '8' => &["###", "#.#", "###", "#.#", "###"],
        '9' => &["###", "#.#", "###", "..#", "##."],
        ' ' => &["..", "..", "..", "..", ".."],
        '.' => &[".", ".", ".", ".", "#"],
        ',' => &[".", ".", ".", "#", "#"],
        ':' => &[".", "#", ".", "#", "."],
        ';' => &[".", "#", ".", "#", "#"],
        '!' => &["#", "#", "#", ".", "#"],
        '?' => &["##.", "..#", ".#.", "...", ".#."],
        '\'' => &["#", "#", ".", ".", "."],
        '"' => &["#.#", "#.#", "...", "...", "..."],
        '-' => &["...", "...", "###", "...", "..."],
        '_' => &["...", "...", "...", "...", "###"],
        '+' => &["...", ".#.", "###", ".#.", "..."],
        '=' => &["...", "###", "...", "###", "..."],
        '/' => &["..#", "..#", ".#.", "#..", "#.."],
        '\\' => &["#..", "#..", ".#.", "..#", "..#"],
        '(' => &[".#", "#.", "#.", "#.", ".#"],
        ')' => &["#.", ".#", ".#", ".#", "#."],
        '[' => &["##", "#.", "#.", "#.", "##"],
        ']' => &["##", ".#", ".#", ".#", "##"],
        '&' => &[".#.", "#.#", ".#.", "#.#", ".##"],
        '#' => &["#.#", "###", "#.#", "###", "#.#"],
        '$' => &[".##", "##.", ".#.", ".##", "##."],
        '%' => &["#.#", "..#", ".#.", "#..", "#.#"],
        '*' => &["...", "#.#", ".#.", "#.#", "..."],
        '@' => &["###", "#.#", "#.#", "#..", "###"],
        '<' => &["..#", ".#.", "#..", ".#.", "..#"],
        '>' => &["#..", ".#.", "..#", ".#.", "#.."],
        '|' => &["#", "#", "#", "#", "#"],
        '~' => &["....", ".#.#", "#.#.", "....", "...."],
        _ => return None,
    })
}

/// Rendered big text: `lines[row]` is a vector of cells.
pub struct BigText {
    pub lines: Vec<Vec<char>>,
}

impl BigText {
    pub fn width(&self) -> usize {
        self.lines.first().map(|l| l.len()).unwrap_or(0)
    }
    pub fn height(&self) -> usize {
        self.lines.len()
    }
}

/// The most rows a line of this font can take.
pub fn lines_for(font: &str) -> u16 {
    entry(font).lines
}

/// Prepare a string for the bitmap fonts: transliterate to ASCII and upper-case.
fn normalize(text: &str) -> String {
    deunicode::deunicode(text).to_uppercase()
}

/// Render `text` with a font. Returns `None` for the plain font.
pub fn render(text: &str, font: &str, spacing: usize) -> Option<BigText> {
    let (lookup, rows): (fn(char) -> Option<Glyph>, usize) = match &entry(font).kind {
        Kind::Slim => (slim, 4),
        Kind::Block => (block, 5),
        Kind::Plain => return None,
        Kind::Figlet(f) => {
            let lines = f.render(text);
            return if lines.is_empty() { None } else { Some(BigText { lines }) };
        }
    };
    // Build the pixel rows.
    let mut pixels: Vec<Vec<bool>> = vec![Vec::new(); rows];
    let text = normalize(text);
    let mut first = true;
    for ch in text.chars() {
        let glyph = match lookup(ch) {
            Some(g) => g,
            None => continue,
        };
        if !first {
            for row in pixels.iter_mut() {
                row.extend(std::iter::repeat_n(false, spacing));
            }
        }
        first = false;
        for (r, row) in pixels.iter_mut().enumerate() {
            row.extend(glyph[r].chars().map(|c| c == '#'));
        }
    }
    let width = pixels[0].len();
    let lines_out = rows.div_ceil(2);
    let mut lines = vec![Vec::with_capacity(width); lines_out];
    for (l, line) in lines.iter_mut().enumerate() {
        for x in 0..width {
            let top = pixels[l * 2][x];
            let bottom = pixels.get(l * 2 + 1).map(|r| r[x]).unwrap_or(false);
            line.push(match (top, bottom) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
    }
    Some(BigText { lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_rows_consistent() {
        for c in (32u8..127).map(|b| b as char) {
            for (font, rows) in [(slim as fn(char) -> Option<Glyph>, 4), (block, 5)] {
                if let Some(g) = font(c) {
                    assert_eq!(g.len(), rows, "{c:?} row count");
                    let w = g[0].len();
                    assert!(g.iter().all(|r| r.len() == w), "{c:?} ragged");
                }
            }
        }
    }

    #[test]
    fn renders_heights() {
        assert_eq!(render("Hello", SLIM, 1).unwrap().height(), 2);
        assert_eq!(render("Hello", BLOCK, 1).unwrap().height(), 3);
        assert_eq!(render("Ünïcødé", BLOCK, 1).unwrap().height(), 3);
        assert_eq!(render("Night Drive", "ansi-compact", 1).unwrap().height(), 3);
        assert!(render("x", PLAIN, 1).is_none());
    }

    #[test]
    fn registry_lookup() {
        assert_eq!(find("ANSI Compact"), Some("ansi-compact"));
        assert_eq!(find("ansi"), Some("ansi-compact"));
        assert_eq!(resolve("nope"), BLOCK);
        assert_eq!(cycle(BLOCK, -1), PLAIN);
        assert_eq!(label("ansi-compact"), "ANSI Compact");
    }
}
