//! The music files in a folder: scanning, tags, and the albums and artists
//! built from them. Local items reuse Spotr's Spotify models (with `local:`
//! URIs), so every list, page and the now-playing view work unchanged.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::spotify::models::*;

/// Extensions Spotr can play (Opus isn't decodable yet, so it isn't listed).
pub const AUDIO_EXTS: &[&str] = &["mp3", "flac", "ogg", "oga", "wav", "wave", "aif", "aiff", "aifc", "m4a", "m4b", "mp4", "aac", "caf", "mka"];

/// Images used as album art when a file has none embedded.
const FOLDER_ART: &[&str] = &["cover", "folder", "front", "album", "albumart", "artwork"];
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp"];

pub const TRACK_PREFIX: &str = "local:track:";
pub const ALBUM_PREFIX: &str = "local:album:";
pub const ARTIST_PREFIX: &str = "local:artist:";
pub const PLAYLIST_PREFIX: &str = "local:playlist:";
/// Cover embedded in an audio file (absolute path follows).
pub const ART_SCHEME: &str = "local-art:";
/// An image file next to the music (absolute path follows).
pub const FILE_SCHEME: &str = "local-file:";

pub fn rel_of(uri: &str) -> Option<&str> {
    uri.strip_prefix(TRACK_PREFIX)
}

/// What a file says about itself (or what its name and folders suggest).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Tags {
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub year: Option<String>,
    pub genre: Option<String>,
    pub duration_ms: u64,
    pub has_art: bool,
    pub has_lyrics: bool,
}

/// Scan cache (`.spotr/index.json`): unchanged files aren't read again.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Index {
    pub version: u32,
    pub files: HashMap<String, IndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexEntry {
    pub size: u64,
    pub mtime: u64,
    pub tags: Tags,
}

pub const INDEX_VERSION: u32 = 1;

pub struct Entry {
    pub rel: String,
    pub track: Track,
    pub genre: Option<String>,
    pub disc: u32,
    /// File modification time, used for "recently added".
    pub added: u64,
    pub album: Option<usize>,
    pub has_lyrics: bool,
    /// Title, artists and album, normalized for searching.
    pub hay: String,
}

pub struct AlbumInfo {
    pub album: Album,
    pub tracks: Vec<usize>,
    pub genre: Option<String>,
    pub duration_ms: u64,
}

pub struct ArtistInfo {
    pub artist: Artist,
    pub tracks: Vec<usize>,
    pub albums: Vec<usize>,
}

#[derive(Default)]
pub struct Library {
    pub root: PathBuf,
    pub entries: Vec<Entry>,
    pub albums: Vec<AlbumInfo>,
    pub artists: Vec<ArtistInfo>,
    by_rel: HashMap<String, usize>,
    by_album: HashMap<String, usize>,
    by_artist: HashMap<String, usize>,
    pub total_ms: u64,
}

/// Lower-case, accent-free text for matching and grouping.
pub fn norm(s: &str) -> String {
    deunicode::deunicode(s).to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Library {
    /// A library with nothing in it yet (before the first scan finishes).
    pub fn empty(root: PathBuf) -> Library {
        Library { root, ..Default::default() }
    }

    pub fn path_of(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub fn entry(&self, rel: &str) -> Option<&Entry> {
        self.by_rel.get(rel).map(|&i| &self.entries[i])
    }

    pub fn track(&self, rel: &str) -> Option<&Track> {
        self.entry(rel).map(|e| &e.track)
    }

    pub fn album(&self, id: &str) -> Option<&AlbumInfo> {
        self.by_album.get(id).map(|&i| &self.albums[i])
    }

    pub fn artist(&self, id: &str) -> Option<&ArtistInfo> {
        self.by_artist.get(id).map(|&i| &self.artists[i])
    }

    pub fn tracks_of(&self, idx: &[usize]) -> Vec<Track> {
        idx.iter().map(|&i| self.entries[i].track.clone()).collect()
    }

    /// Group scanned files into tracks, albums and artists.
    pub fn build(root: PathBuf, files: Vec<(String, IndexEntry)>) -> Library {
        let mut files = files;
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let folder_art = find_folder_art(&root, files.iter().map(|(r, _)| r.as_str()));
        let mut lib = Library { root, ..Default::default() };
        // Details shared by an album's songs, filled in as they're found.
        struct AlbumMeta {
            name: String,
            shown: String,
            year: Option<String>,
            art: Option<String>,
            genre: Option<String>,
        }
        let mut album_meta: Vec<AlbumMeta> = Vec::new();
        for (rel, ie) in files {
            let t = &ie.tags;
            let abs = lib.root.join(&rel);
            let dir = Path::new(&rel).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
            let art = if t.has_art {
                Some(format!("{ART_SCHEME}{}", abs.display()))
            } else {
                folder_art.get(&dir).map(|p| format!("{FILE_SCHEME}{}", lib.root.join(p).display()))
            };
            let artists: Vec<String> = if t.artists.is_empty() { vec!["Unknown artist".into()] } else { t.artists.clone() };
            let simple: Vec<SimpleArtist> = artists
                .iter()
                .map(|a| {
                    let key = norm(a);
                    SimpleArtist { id: Some(key.clone()), name: a.clone(), uri: format!("{ARTIST_PREFIX}{key}") }
                })
                .collect();
            let album_name = t.album.clone();
            let album_idx = album_name.as_ref().map(|name| {
                // With an album artist, an album spans folders; without one, files
                // in one folder with the same album name belong together.
                let key = match &t.album_artist {
                    Some(aa) => format!("{}\u{1f}{}", norm(aa), norm(name)),
                    None => format!("{}\u{1f}{}", norm(name), norm(&dir)),
                };
                match lib.by_album.get(&key) {
                    Some(&i) => i,
                    None => {
                        let i = lib.albums.len();
                        lib.by_album.insert(key.clone(), i);
                        let shown = t.album_artist.clone().unwrap_or_else(|| artists[0].clone());
                        album_meta.push(AlbumMeta { name: name.clone(), shown, year: t.year.clone(), art: art.clone(), genre: t.genre.clone() });
                        lib.albums.push(AlbumInfo {
                            album: Album { id: Some(key.clone()), name: name.clone(), uri: format!("{ALBUM_PREFIX}{key}"), ..Default::default() },
                            tracks: Vec::new(),
                            genre: None,
                            duration_ms: 0,
                        });
                        i
                    }
                }
            });
            let title = t.title.clone().unwrap_or_else(|| rel.clone());
            let mut track = Track {
                id: Some(rel.clone()),
                name: title,
                uri: format!("{TRACK_PREFIX}{rel}"),
                duration_ms: t.duration_ms,
                artists: simple,
                track_number: t.track,
                is_local: true,
                ..Default::default()
            };
            if let Some(ai) = album_idx {
                let meta = &mut album_meta[ai];
                if meta.art.is_none() {
                    meta.art = art.clone();
                }
                if meta.year.is_none() {
                    meta.year = t.year.clone();
                }
                // Prefer this track's own art; fall back to the album's.
                let img = art.clone().or_else(|| meta.art.clone());
                track.album = Some(Album {
                    id: lib.albums[ai].album.id.clone(),
                    name: meta.name.clone(),
                    uri: lib.albums[ai].album.uri.clone(),
                    images: img.map(|url| vec![Image { url, width: Some(640), height: Some(640) }]).unwrap_or_default(),
                    release_date: t.year.clone(),
                    ..Default::default()
                });
            } else if let Some(url) = art.clone() {
                // No album: still carry the art for the player.
                track.album = Some(Album { images: vec![Image { url, width: Some(640), height: Some(640) }], ..Default::default() });
            }
            let i = lib.entries.len();
            lib.total_ms += t.duration_ms;
            lib.by_rel.insert(rel.clone(), i);
            if let Some(ai) = album_idx {
                lib.albums[ai].tracks.push(i);
                lib.albums[ai].duration_ms += t.duration_ms;
            }
            let hay = norm(&format!("{} {} {}", track.name, track.artist_names(), track.album_name()));
            lib.entries.push(Entry {
                hay,
                rel,
                track,
                genre: t.genre.clone(),
                disc: t.disc.unwrap_or(1),
                added: ie.mtime,
                album: album_idx,
                has_lyrics: t.has_lyrics,
            });
        }
        // Finish albums: order their songs, fill in shared details.
        for (ai, a) in lib.albums.iter_mut().enumerate() {
            let AlbumMeta { name, shown, year, art, genre } = std::mem::replace(
                &mut album_meta[ai],
                AlbumMeta { name: String::new(), shown: String::new(), year: None, art: None, genre: None },
            );
            a.tracks.sort_by_key(|&i| (lib.entries[i].disc, lib.entries[i].track.track_number.unwrap_or(u32::MAX), lib.entries[i].track.name.to_lowercase()));
            let distinct: std::collections::HashSet<String> = a.tracks.iter().map(|&i| norm(&lib.entries[i].track.artists[0].name)).collect();
            let shown = if distinct.len() > 3 && shown == lib.entries[a.tracks[0]].track.artists[0].name { "Various Artists".to_string() } else { shown };
            let key = norm(&shown);
            a.album.name = name;
            a.album.artists = vec![SimpleArtist { id: Some(key.clone()), name: shown, uri: format!("{ARTIST_PREFIX}{key}") }];
            a.album.release_date = year;
            a.album.total_tracks = Some(a.tracks.len() as u32);
            // Tags don't say; a lone song named like its "album" is a single.
            let single = a.tracks.len() == 1 && norm(&lib.entries[a.tracks[0]].track.name) == norm(&a.album.name);
            a.album.album_type = Some(if single { "single".into() } else { "album".into() });
            a.album.images = art.map(|url| vec![Image { url, width: Some(640), height: Some(640) }]).unwrap_or_default();
            a.genre = genre;
            for &i in &a.tracks {
                if let Some(al) = lib.entries[i].track.album.as_mut() {
                    al.artists = a.album.artists.clone();
                    al.total_tracks = a.album.total_tracks;
                    al.album_type = a.album.album_type.clone();
                }
            }
        }
        // Artists: everyone credited on a song, plus album artists.
        for i in 0..lib.entries.len() {
            let names: Vec<(String, String)> = lib.entries[i].track.artists.iter().map(|a| (a.id.clone().unwrap_or_default(), a.name.clone())).collect();
            for (key, name) in names {
                let ai = lib.artist_slot(&key, &name);
                lib.artists[ai].tracks.push(i);
            }
        }
        for al in 0..lib.albums.len() {
            let a = lib.albums[al].album.artists[0].clone();
            let ai = lib.artist_slot(a.id.as_deref().unwrap_or(""), &a.name);
            if !lib.artists[ai].albums.contains(&al) {
                lib.artists[ai].albums.push(al);
            }
            for &t in &lib.albums[al].tracks.clone() {
                for ta in lib.entries[t].track.artists.clone() {
                    if let Some(&x) = lib.by_artist.get(ta.id.as_deref().unwrap_or(""))
                        && !lib.artists[x].albums.contains(&al)
                    {
                        lib.artists[x].albums.push(al);
                    }
                }
            }
        }
        for a in lib.artists.iter_mut() {
            a.albums.sort_by_key(|&al| std::cmp::Reverse(lib.albums[al].album.release_date.clone()));
            a.artist.images = a.albums.iter().find_map(|&al| lib.albums[al].album.images.first().cloned()).into_iter().collect();
            let mut genres: Vec<String> = a.tracks.iter().filter_map(|&t| lib.entries[t].genre.clone()).collect();
            genres.sort();
            genres.dedup();
            genres.truncate(3);
            a.artist.genres = genres;
        }
        lib
    }

    fn artist_slot(&mut self, key: &str, name: &str) -> usize {
        if let Some(&i) = self.by_artist.get(key) {
            return i;
        }
        let i = self.artists.len();
        self.by_artist.insert(key.to_string(), i);
        self.artists.push(ArtistInfo {
            artist: Artist { id: Some(key.to_string()), name: name.to_string(), uri: format!("{ARTIST_PREFIX}{key}"), ..Default::default() },
            tracks: Vec::new(),
            albums: Vec::new(),
        });
        i
    }
}

/// `cover.jpg` & co. per folder (relative paths).
fn find_folder_art<'a>(root: &Path, rels: impl Iterator<Item = &'a str>) -> HashMap<String, String> {
    let mut dirs: Vec<String> = rels.map(|r| Path::new(r).parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()).collect();
    dirs.dedup();
    let mut out = HashMap::new();
    for d in dirs {
        if out.contains_key(&d) {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(root.join(&d)) else { continue };
        let mut best: Option<(usize, String)> = None;
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let p = Path::new(&name);
            let ext = p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
            if !IMAGE_EXTS.contains(&ext.as_str()) {
                continue;
            }
            let stem = p.file_stem().and_then(|s| s.to_str()).map(|s| s.to_lowercase()).unwrap_or_default();
            // Named covers win; any other image in the folder is a last resort.
            let rank = FOLDER_ART.iter().position(|n| stem == *n || stem.starts_with(n)).unwrap_or(FOLDER_ART.len());
            if best.as_ref().is_none_or(|(r, _)| rank < *r) {
                let rel = if d.is_empty() { name.clone() } else { format!("{d}/{name}") };
                best = Some((rank, rel));
            }
        }
        if let Some((_, rel)) = best {
            out.insert(d, rel);
        }
    }
    out
}

// ------------------------------------------------------------------- scanning

/// Every playable file under `root` as (relative path, size, mtime), skipping
/// hidden files and folders (which includes `.spotr`).
pub fn walk(root: &Path) -> Vec<(String, u64, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let path = e.path();
            // Don't follow symlinked folders (loops); symlinked files are fine.
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if depth < 24 {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            let ext = path.extension().and_then(|x| x.to_str()).map(|x| x.to_lowercase()).unwrap_or_default();
            if !AUDIO_EXTS.contains(&ext.as_str()) {
                continue;
            }
            let Ok(meta) = std::fs::metadata(&path) else { continue };
            if !meta.is_file() {
                continue;
            }
            let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
            let Ok(rel) = path.strip_prefix(root) else { continue };
            let rel = rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/");
            out.push((rel, meta.len(), mtime));
        }
    }
    out
}

pub struct ScanResult {
    pub library: Library,
    pub index: Index,
    pub index_changed: bool,
}

/// Scan `root`, reading tags only for new or changed files.
pub fn scan(root: &Path, mut index: Index, progress: &(dyn Fn(usize, usize) + Sync)) -> ScanResult {
    if index.version != INDEX_VERSION {
        index = Index { version: INDEX_VERSION, ..Default::default() };
    }
    let found = walk(root);
    let mut keep: Vec<(String, IndexEntry)> = Vec::with_capacity(found.len());
    let mut todo: Vec<(String, u64, u64)> = Vec::new();
    for (rel, size, mtime) in found {
        match index.files.get(&rel) {
            Some(e) if e.size == size && e.mtime == mtime => keep.push((rel, e.clone())),
            _ => todo.push((rel, size, mtime)),
        }
    }
    let total = todo.len();
    let done = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    let chunk = total.div_ceil(threads).max(1);
    let fresh: Vec<(String, IndexEntry)> = std::thread::scope(|s| {
        let handles: Vec<_> = todo
            .chunks(chunk)
            .map(|part| {
                let done = &done;
                s.spawn(move || {
                    part.iter()
                        .map(|(rel, size, mtime)| {
                            let tags = read_tags(&root.join(rel), rel);
                            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                            if n.is_multiple_of(16) || n == total {
                                progress(n, total);
                            }
                            (rel.clone(), IndexEntry { size: *size, mtime: *mtime, tags })
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    let removed = index.files.len() != keep.len();
    let read = fresh.len();
    let mut files = keep;
    files.extend(fresh);
    let new_index = Index { version: INDEX_VERSION, files: files.iter().cloned().collect() };
    let library = Library::build(root.to_path_buf(), files);
    ScanResult { library, index: new_index, index_changed: read > 0 || removed }
}

/// Tags from the file, completed with what its name and folders suggest.
pub fn read_tags(path: &Path, rel: &str) -> Tags {
    let mut tags = embedded_tags(path);
    fill_from_path(&mut tags, rel);
    tags
}

/// `Artist/Album/01 - Title.mp3`, `Artist - Title.mp3`, `03. Title.flac`…
pub fn fill_from_path(t: &mut Tags, rel: &str) {
    let p = Path::new(rel);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(rel).trim().to_string();
    let (num, rest) = split_track_number(&stem);
    if t.track.is_none() {
        t.track = num;
    }
    let (artist, title) = match rest.split_once(" - ") {
        Some((a, b)) if !a.trim().is_empty() && !b.trim().is_empty() => (Some(a.trim().to_string()), b.trim().to_string()),
        _ => (None, rest.to_string()),
    };
    if t.title.is_none() {
        t.title = Some(if title.is_empty() { stem.clone() } else { title });
    }
    let parents: Vec<String> = p.parent().map(|d| d.components().map(|c| c.as_os_str().to_string_lossy().to_string()).collect()).unwrap_or_default();
    if t.artists.is_empty() {
        if let Some(a) = artist {
            t.artists.push(a);
        } else if parents.len() >= 2 {
            t.artists.push(parents[parents.len() - 2].clone());
        }
    }
    if t.album.is_none()
        && let Some(dir) = parents.last()
    {
        t.album = Some(dir.clone());
    }
}

/// "01 - Title" → (1, "Title"); "1-03 Title" (disc-track) → (3, "Title").
fn split_track_number(stem: &str) -> (Option<u32>, &str) {
    let digits = stem.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 3 {
        return (None, stem);
    }
    let mut rest = &stem[digits..];
    let mut num: u32 = stem[..digits].parse().unwrap_or(0);
    // Disc-track form.
    if let Some(r) = rest.strip_prefix('-') {
        let d2 = r.chars().take_while(|c| c.is_ascii_digit()).count();
        if (1..=3).contains(&d2) && r[d2..].starts_with([' ', '.', '_', '-']) {
            num = r[..d2].parse().unwrap_or(num);
            rest = &r[d2..];
        }
    }
    let Some(first) = rest.chars().next() else { return (None, stem) };
    if !matches!(first, ' ' | '.' | '_' | '-' | ')') {
        return (None, stem);
    }
    let rest = rest.trim_start_matches([' ', '.', '_', '-', ')']);
    if rest.is_empty() { (None, stem) } else { (Some(num), rest) }
}

#[cfg_attr(not(feature = "local-files"), allow(dead_code))]
fn parse_num(s: &str) -> Option<u32> {
    let d: String = s.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
    d.parse().ok().filter(|n| *n > 0)
}

#[cfg(feature = "local-files")]
fn probe(path: &Path) -> Option<symphonia::core::probe::ProbeResult> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    let file = std::fs::File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    symphonia::default::get_probe().format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default()).ok()
}

#[cfg(feature = "local-files")]
fn embedded_tags(path: &Path) -> Tags {
    use symphonia::core::codecs::CODEC_TYPE_NULL;
    let mut t = Tags::default();
    let Some(mut probed) = probe(path) else { return t };
    if let Some(m) = probed.metadata.get()
        && let Some(rev) = m.current()
    {
        apply_revision(&mut t, rev);
    }
    if let Some(rev) = probed.format.metadata().current() {
        apply_revision(&mut t, rev);
    }
    let Some(track) = probed.format.tracks().iter().find(|t| t.codec_params.codec != CODEC_TYPE_NULL).cloned() else { return t };
    let p = &track.codec_params;
    let to_ms = |n: u64| -> u64 {
        match p.time_base {
            Some(tb) => {
                let x = tb.calc_time(n);
                ((x.seconds as f64 + x.frac) * 1000.0) as u64
            }
            None => n * 1000 / p.sample_rate.unwrap_or(44_100).max(1) as u64,
        }
    };
    t.duration_ms = match p.n_frames {
        Some(n) => to_ms(n),
        None => {
            // No length in the header (some VBR MP3s): add up the packets.
            let mut ts = 0u64;
            while let Ok(pkt) = probed.format.next_packet() {
                if pkt.track_id() == track.id {
                    ts = ts.max(pkt.ts() + pkt.dur());
                }
            }
            to_ms(ts)
        }
    };
    t
}

#[cfg(feature = "local-files")]
fn apply_revision(t: &mut Tags, rev: &symphonia::core::meta::MetadataRevision) {
    use symphonia::core::meta::StandardTagKey as K;
    for tag in rev.tags() {
        let v = tag.value.to_string();
        let v = v.trim().trim_matches('\0').trim();
        if v.is_empty() {
            continue;
        }
        let set = |slot: &mut Option<String>| {
            if slot.is_none() {
                *slot = Some(v.to_string());
            }
        };
        match tag.std_key {
            Some(K::TrackTitle) => set(&mut t.title),
            Some(K::Album) => set(&mut t.album),
            Some(K::AlbumArtist) => set(&mut t.album_artist),
            Some(K::Genre) => {
                // ID3v1-style "(17)" codes aren't worth showing.
                if !v.starts_with('(') {
                    set(&mut t.genre)
                }
            }
            Some(K::Artist) => {
                for a in v.split(['\0', ';']).map(str::trim).filter(|a| !a.is_empty()) {
                    if !t.artists.iter().any(|x| x.eq_ignore_ascii_case(a)) {
                        t.artists.push(a.to_string());
                    }
                }
            }
            Some(K::TrackNumber) if t.track.is_none() => t.track = parse_num(v),
            Some(K::DiscNumber) if t.disc.is_none() => t.disc = parse_num(v),
            Some(K::Date | K::ReleaseDate | K::OriginalDate) if t.year.is_none() => {
                let y: String = v.chars().filter(|c| c.is_ascii_digit()).take(4).collect();
                if y.len() == 4 {
                    t.year = Some(y);
                }
            }
            Some(K::Lyrics) => t.has_lyrics = true,
            _ => {}
        }
    }
    if !rev.visuals().is_empty() {
        t.has_art = true;
    }
}

#[cfg(not(feature = "local-files"))]
fn embedded_tags(_path: &Path) -> Tags {
    Tags::default()
}

/// The front cover embedded in an audio file.
#[cfg(feature = "local-files")]
pub fn embedded_art(path: &Path) -> Option<Vec<u8>> {
    use symphonia::core::meta::StandardVisualKey;
    let mut probed = probe(path)?;
    let mut visuals: Vec<symphonia::core::meta::Visual> = Vec::new();
    if let Some(m) = probed.metadata.get()
        && let Some(rev) = m.current()
    {
        visuals.extend(rev.visuals().iter().cloned());
    }
    if let Some(rev) = probed.format.metadata().current() {
        visuals.extend(rev.visuals().iter().cloned());
    }
    let front = visuals.iter().position(|v| v.usage == Some(StandardVisualKey::FrontCover)).unwrap_or(0);
    visuals.into_iter().nth(front).map(|v| v.data.into_vec())
}

#[cfg(not(feature = "local-files"))]
pub fn embedded_art(_path: &Path) -> Option<Vec<u8>> {
    None
}

/// Lyrics stored in the file's tags.
#[cfg(feature = "local-files")]
pub fn embedded_lyrics(path: &Path) -> Option<String> {
    use symphonia::core::meta::StandardTagKey;
    let mut probed = probe(path)?;
    let find = |rev: &symphonia::core::meta::MetadataRevision| {
        rev.tags().iter().find(|t| t.std_key == Some(StandardTagKey::Lyrics)).map(|t| t.value.to_string())
    };
    let from_id3 = probed.metadata.get().and_then(|m| m.current().and_then(find));
    from_id3.or_else(|| probed.format.metadata().current().and_then(find)).filter(|s| !s.trim().is_empty())
}

#[cfg(not(feature = "local-files"))]
pub fn embedded_lyrics(_path: &Path) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags_for(rel: &str) -> Tags {
        let mut t = Tags::default();
        fill_from_path(&mut t, rel);
        t
    }

    #[test]
    fn names_and_folders() {
        let t = tags_for("Daft Punk/Discovery/03 - Digital Love.mp3");
        assert_eq!((t.title.as_deref(), t.track, t.album.as_deref()), (Some("Digital Love"), Some(3), Some("Discovery")));
        assert_eq!(t.artists, ["Daft Punk"]);
        let t = tags_for("Queen - Bohemian Rhapsody.flac");
        assert_eq!((t.title.as_deref(), t.artists.as_slice(), t.album), (Some("Bohemian Rhapsody"), &["Queen".to_string()][..], None));
        let t = tags_for("mix/1-07 Intro.ogg");
        assert_eq!((t.title.as_deref(), t.track), (Some("Intro"), Some(7)));
        // A title that starts with a number isn't a track number.
        let t = tags_for("1999.mp3");
        assert_eq!((t.title.as_deref(), t.track), (Some("1999"), None));
        let t = tags_for("22 Acacia Avenue.mp3");
        assert_eq!((t.title.as_deref(), t.track), (Some("Acacia Avenue"), Some(22)));
    }

    #[test]
    fn groups_albums_and_artists() {
        let entry = |title: &str, artist: &str, album: Option<&str>, track: u32| IndexEntry {
            size: 1,
            mtime: 1,
            tags: Tags {
                title: Some(title.into()),
                artists: vec![artist.into()],
                album: album.map(String::from),
                track: Some(track),
                duration_ms: 1000,
                ..Default::default()
            },
        };
        let lib = Library::build(
            PathBuf::from("/music"),
            vec![
                ("a/2.mp3".into(), entry("Two", "Ann", Some("First"), 2)),
                ("a/1.mp3".into(), entry("One", "Ann", Some("First"), 1)),
                ("b/1.mp3".into(), entry("Other", "Bob", Some("First"), 1)),
                ("loose.mp3".into(), entry("Loose", "Ann", None, 1)),
            ],
        );
        assert_eq!(lib.entries.len(), 4);
        // Same album name in different folders (no album artist) stays apart.
        assert_eq!(lib.albums.len(), 2);
        let first = &lib.albums[0];
        assert_eq!(lib.tracks_of(&first.tracks).iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["One", "Two"]);
        let ann = lib.artist("ann").unwrap();
        assert_eq!(ann.tracks.len(), 3);
        assert_eq!(ann.albums.len(), 1);
        assert_eq!(lib.total_ms, 4000);
        let t = lib.track("a/1.mp3").unwrap();
        assert_eq!(t.uri, "local:track:a/1.mp3");
        assert_eq!(rel_of(&t.uri), Some("a/1.mp3"));
        assert!(lib.track("loose.mp3").unwrap().album.is_none());
    }
}
