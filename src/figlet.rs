//! A small FIGlet (`.flf`) / TOIlet (`.tlf`) font loader and renderer.
//!
//! Supports the standard file layout (header, comments, ASCII 32–126, the
//! seven Deutsch characters and code-tagged extras), hardblanks and end
//! marks. Layout is "fitting" (characters slide together until they touch)
//! or full width, as the font asks; smushing rules are approximated by fitting.

use std::collections::HashMap;

use anyhow::{Result, anyhow, bail};

pub struct Figlet {
    pub height: usize,
    hardblank: Option<char>,
    full_width: bool,
    glyphs: HashMap<char, Vec<Vec<char>>>,
}

const DEUTSCH: [char; 7] = ['Ä', 'Ö', 'Ü', 'ä', 'ö', 'ü', 'ß'];

impl Figlet {
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        let header = lines.next().ok_or_else(|| anyhow!("empty font file"))?;
        if !(header.starts_with("flf2a") || header.starts_with("tlf2a")) {
            bail!("not a FIGlet/TOIlet font (bad signature)");
        }
        let hardblank = header.chars().nth(5).filter(|c| !c.is_whitespace());
        let mut fields = header.split_whitespace().skip(1).map(|f| f.parse::<i64>().ok());
        let height = fields.next().flatten().ok_or_else(|| anyhow!("missing height"))? as usize;
        let _baseline = fields.next();
        let _max_len = fields.next();
        let old_layout = fields.next().flatten().unwrap_or(0);
        let comments = fields.next().flatten().unwrap_or(0) as usize;
        if height == 0 || height > 32 {
            bail!("unreasonable font height {height}");
        }
        let lines: Vec<&str> = lines.skip(comments).collect();

        let read_glyph = |at: usize| -> Option<Vec<Vec<char>>> {
            let rows = lines.get(at..at + height)?;
            let rows: Vec<Vec<char>> = rows
                .iter()
                .map(|r| {
                    let r = r.trim_end_matches(['\r', '\n']);
                    // The end mark is the last character; it may be doubled on the last row.
                    let mark = r.chars().last();
                    let mut chars: Vec<char> = r.chars().collect();
                    if let Some(m) = mark {
                        while chars.last() == Some(&m) {
                            chars.pop();
                        }
                    }
                    chars
                })
                .collect();
            let w = rows.iter().map(|r| r.len()).max().unwrap_or(0);
            Some(rows.into_iter().map(|mut r| {
                r.resize(w, ' ');
                r
            }).collect())
        };

        let mut glyphs = HashMap::new();
        let mut at = 0;
        for code in 32u32..127 {
            let g = read_glyph(at).ok_or_else(|| anyhow!("font ends early (char {code})"))?;
            glyphs.insert(char::from_u32(code).unwrap(), g);
            at += height;
        }
        for c in DEUTSCH {
            match read_glyph(at) {
                Some(g) => {
                    glyphs.insert(c, g);
                    at += height;
                }
                None => break,
            }
        }
        // Code-tagged characters: a line with the code, then `height` rows.
        while at < lines.len() {
            let tag = lines[at].split_whitespace().next().unwrap_or("");
            let code = if let Some(h) = tag.strip_prefix("0x").or_else(|| tag.strip_prefix("0X")) {
                i64::from_str_radix(h, 16).ok()
            } else if tag.len() > 1 && tag.starts_with('0') {
                i64::from_str_radix(&tag[1..], 8).ok()
            } else {
                tag.parse::<i64>().ok()
            };
            let Some(code) = code else { break };
            let Some(g) = read_glyph(at + 1) else { break };
            if let Some(c) = u32::try_from(code).ok().and_then(char::from_u32) {
                glyphs.insert(c, g);
            }
            at += 1 + height;
        }
        Ok(Self { height, hardblank, full_width: old_layout == -1, glyphs })
    }

    fn glyph(&self, c: char) -> Option<&Vec<Vec<char>>> {
        self.glyphs.get(&c).filter(|g| g.iter().any(|r| !r.is_empty()))
    }

    /// Render `text` into rows of cells, with blank rows above and below trimmed.
    pub fn render(&self, text: &str) -> Vec<Vec<char>> {
        let mut out: Vec<Vec<char>> = vec![Vec::new(); self.height];
        let blank = |c: char| c == ' ';
        for ch in text.chars() {
            let g = match self.glyph(ch) {
                Some(g) => g,
                None => {
                    // Fall back to an ASCII transliteration, letter by letter.
                    let t = deunicode::deunicode_char(ch).unwrap_or("");
                    for c in t.chars() {
                        if let Some(g) = self.glyph(c) {
                            self.append(&mut out, g, blank);
                        }
                    }
                    continue;
                }
            };
            self.append(&mut out, g, blank);
        }
        let hb = self.hardblank;
        let mut rows: Vec<Vec<char>> = out
            .into_iter()
            .map(|r| r.into_iter().map(|c| if Some(c) == hb { ' ' } else { c }).collect())
            .collect();
        while rows.first().is_some_and(|r: &Vec<char>| r.iter().all(|c| *c == ' ')) {
            rows.remove(0);
        }
        while rows.last().is_some_and(|r| r.iter().all(|c| *c == ' ')) {
            rows.pop();
        }
        // Trim columns that are blank in every row at both ends.
        let w = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        for r in rows.iter_mut() {
            r.resize(w, ' ');
        }
        let lead = (0..w).take_while(|&x| rows.iter().all(|r| r[x] == ' ')).count();
        let trail = (0..w).rev().take_while(|&x| x >= lead && rows.iter().all(|r| r[x] == ' ')).count();
        for r in rows.iter_mut() {
            r.truncate(w - trail);
            r.drain(..lead.min(r.len()));
        }
        rows
    }

    fn append(&self, out: &mut [Vec<char>], g: &[Vec<char>], blank: impl Fn(char) -> bool) {
        let cur_w = out.iter().map(|r| r.len()).max().unwrap_or(0);
        for r in out.iter_mut() {
            r.resize(cur_w, ' ');
        }
        // How far can the glyph slide left before touching what's there?
        let overlap = if self.full_width || cur_w == 0 {
            0
        } else {
            out.iter()
                .zip(g)
                .map(|(row, grow)| {
                    let trailing = row.iter().rev().take_while(|c| blank(**c)).count();
                    let leading = grow.iter().take_while(|c| blank(**c)).count();
                    trailing + leading
                })
                .min()
                .unwrap_or(0)
                .min(cur_w)
        };
        for (row, grow) in out.iter_mut().zip(g) {
            let start = row.len() - overlap;
            for (i, &c) in grow.iter().enumerate() {
                let x = start + i;
                if x < row.len() {
                    if !blank(c) {
                        row[x] = c;
                    }
                } else {
                    row.push(c);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY: &str = "flf2a$ 2 2 4 0 1\ncomment\n";

    fn tiny_font() -> String {
        let mut s = TINY.to_string();
        for code in 32u8..127 {
            let c = code as char;
            if c == 'A' {
                s.push_str("/\\@\n/\\@@\n");
            } else if c == ' ' {
                s.push_str("$$@\n$$@@\n");
            } else {
                s.push_str("#@\n#@@\n");
            }
        }
        s
    }

    #[test]
    fn parses_and_renders() {
        let f = Figlet::parse(&tiny_font()).unwrap();
        assert_eq!(f.height, 2);
        let r = f.render("AA");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].iter().collect::<String>(), "/\\/\\");
        // Hardblanks become spaces but keep their width.
        let r = f.render("A A");
        assert_eq!(r[0].iter().collect::<String>(), "/\\  /\\");
    }

    #[test]
    fn bundled_fonts_parse() {
        for (name, src) in crate::font::BUNDLED {
            let f = Figlet::parse(src).unwrap_or_else(|e| panic!("{name}: {e}"));
            let r = f.render("Night Drive");
            assert!(!r.is_empty() && r.len() <= 4, "{name}: {} rows", r.len());
            assert!(r.iter().all(|row| row.len() == r[0].len()), "{name} ragged");
        }
    }
}
