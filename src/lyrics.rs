//! Lyrics from LRCLIB (https://lrclib.net), a free, open lyrics database.

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Lyrics {
    /// `(time_ms, text)`; times are `None` for unsynced lyrics.
    pub lines: Vec<(Option<u64>, String)>,
    pub synced: bool,
    pub instrumental: bool,
}

impl Lyrics {
    /// Index of the line being sung at `pos_ms`.
    pub fn current(&self, pos_ms: u64) -> Option<usize> {
        if !self.synced {
            return None;
        }
        let mut cur = None;
        for (i, (t, _)) in self.lines.iter().enumerate() {
            match t {
                Some(t) if *t <= pos_ms => cur = Some(i),
                Some(_) => break,
                None => {}
            }
        }
        cur
    }

    /// Progress (0..1) through line `i` at `pos_ms`.
    pub fn line_progress(&self, i: usize, pos_ms: u64) -> f32 {
        let start = self.lines.get(i).and_then(|l| l.0).unwrap_or(0);
        let end = self
            .lines
            .get(i + 1)
            .and_then(|l| l.0)
            .unwrap_or(start + 4000)
            .min(start + 8000);
        if end <= start {
            return 1.0;
        }
        ((pos_ms.saturating_sub(start)) as f32 / (end - start) as f32).clamp(0.0, 1.0)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcLibEntry {
    #[serde(default)]
    synced_lyrics: Option<String>,
    #[serde(default)]
    plain_lyrics: Option<String>,
    #[serde(default)]
    instrumental: bool,
}

pub fn parse_lrc(text: &str) -> Vec<(Option<u64>, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut rest = line.trim();
        let mut stamps = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            let tag = &stripped[..end];
            rest = &stripped[end + 1..];
            if let Some(ms) = parse_stamp(tag) {
                stamps.push(ms);
            }
        }
        let text = rest.trim().to_string();
        for s in stamps {
            out.push((Some(s), text.clone()));
        }
    }
    out.sort_by_key(|(t, _)| *t);
    out
}

fn parse_stamp(tag: &str) -> Option<u64> {
    let (m, s) = tag.split_once(':')?;
    let m: u64 = m.trim().parse().ok()?;
    let s: f64 = s.trim().parse().ok()?;
    Some(m * 60_000 + (s * 1000.0) as u64)
}

pub async fn fetch(http: &reqwest::Client, track: &str, artist: &str, album: &str, duration_ms: u64) -> Result<Option<Lyrics>> {
    let ua = concat!("Spotr/", env!("CARGO_PKG_VERSION"), " (terminal Spotify client)");
    let resp = http
        .get("https://lrclib.net/api/get")
        .header("User-Agent", ua)
        .query(&[
            ("track_name", track.to_string()),
            ("artist_name", artist.to_string()),
            ("album_name", album.to_string()),
            ("duration", (duration_ms / 1000).to_string()),
        ])
        .send()
        .await?;
    let entry: Option<LrcLibEntry> = if resp.status().is_success() {
        resp.json().await.ok()
    } else {
        // Fall back to a fuzzy search.
        let list: Vec<LrcLibEntry> = http
            .get("https://lrclib.net/api/search")
            .header("User-Agent", ua)
            .query(&[("track_name", track), ("artist_name", artist)])
            .send()
            .await?
            .json()
            .await
            .unwrap_or_default();
        let mut list = list;
        let idx = list
            .iter()
            .position(|e| e.synced_lyrics.is_some())
            .or(if list.is_empty() { None } else { Some(0) });
        idx.map(|i| list.swap_remove(i))
    };
    let Some(e) = entry else { return Ok(None) };
    if e.instrumental {
        return Ok(Some(Lyrics { lines: vec![], synced: false, instrumental: true }));
    }
    if let Some(s) = e.synced_lyrics.filter(|s| !s.trim().is_empty()) {
        let lines = parse_lrc(&s);
        if !lines.is_empty() {
            return Ok(Some(Lyrics { lines, synced: true, instrumental: false }));
        }
    }
    if let Some(p) = e.plain_lyrics.filter(|s| !s.trim().is_empty()) {
        return Ok(Some(Lyrics {
            lines: p.lines().map(|l| (None, l.to_string())).collect(),
            synced: false,
            instrumental: false,
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc() {
        let l = parse_lrc("[00:01.50]Hello\n[01:02.00][00:03.00]World\n[ar:Someone]");
        assert_eq!(l.len(), 3);
        assert_eq!(l[0], (Some(1500), "Hello".into()));
        assert_eq!(l[1], (Some(3000), "World".into()));
        assert_eq!(l[2].0, Some(62_000));
    }
}
