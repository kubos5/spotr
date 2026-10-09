//! `--demo`: a fictional catalog and a simulated player, so Spotr can be
//! explored (and screenshotted) without a Spotify account.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Instant;

use super::models::*;

const ARTISTS: &[(&str, &[&str])] = &[
    ("Neon Harbor", &["synthwave", "retrowave"]),
    ("The Velvet Static", &["indie rock", "shoegaze"]),
    ("Lumen Drift", &["ambient", "downtempo"]),
    ("Kites Over Kyoto", &["city pop", "funk"]),
];

/// (artist index, album name, year, tracks)
const ALBUMS: &[(usize, &str, &str, &[&str])] = &[
    (0, "Midnight Arcade", "2024", &["Night Drive", "Chrome Sunset", "Arcade Hearts", "Pulse Runner", "Last Train to Neo City", "Afterimage"]),
    (0, "Afterglow FM", "2021", &["Afterglow", "Radio Silence", "Palm Shadows", "VHS Summer", "Outrun the Morning"]),
    (1, "Paper Satellites", "2023", &["Paper Satellites", "Static Bloom", "Velvet Underpass", "Slow Motion Fireworks", "Echo Chamber", "Hollow Bones", "Satellite Reprise"]),
    (1, "Low Tide Radio", "2019", &["Low Tide", "Grey Weather", "Signal Fade", "Postcards from Nowhere"]),
    (2, "Glass Gardens", "2025", &["Glass Gardens", "Moss & Light", "Slow Rivers", "Lantern Field", "Weightless", "Morning Fog"]),
    (2, "Northbound", "2022", &["Northbound", "Aurora Hum", "Snowfield", "Driftwood"]),
    (3, "Tokyo Weather Report", "2024", &["Umbrella Funk", "Shinkansen Love", "Neon Rain", "Vending Machine Moon", "Coffee at 3 AM"]),
    (3, "Kites", "2020", &["Kites Over Kyoto", "Paper Lanterns", "Seaside Line", "Golden Week"]),
];

/// (album index, track index) pairs.
type Picks = &'static [(usize, usize)];

const PLAYLISTS: &[(&str, &str, Picks)] = &[
    ("Late Night Coding", "Focus beats for the small hours", &[(0, 0), (2, 0), (2, 4), (0, 3), (4, 1), (4, 4), (5, 1), (0, 5)]),
    ("Sunday Morning", "Slow coffee, soft light", &[(4, 5), (6, 4), (7, 1), (4, 2), (3, 0), (5, 3)]),
    ("Road Trip", "Windows down", &[(0, 4), (6, 1), (1, 4), (2, 0), (7, 2), (0, 1), (6, 0)]),
];

const LYRICS: &[&str] = &[
    "City lights are humming low",
    "Every signal says to go",
    "We were running out of night",
    "Chasing static, chasing light",
    "",
    "Hold the wheel and close your eyes",
    "Paint the dark in neon skies",
    "Every mile a little song",
    "Every song a little long",
    "",
    "Oh, the radio is calling",
    "Oh, the stars are slowly falling",
    "Turn it up and let it play",
    "Night will turn to gold someday",
    "",
    "Static, static, in the air",
    "Echoes drifting everywhere",
    "If the morning finds us here",
    "Sing it once and disappear",
];

struct State {
    track: usize,
    progress_ms: u64,
    at: Instant,
    playing: bool,
    volume: u32,
    shuffle: bool,
    repeat: String,
    saved: HashSet<String>,
    queue: Vec<usize>,
    context: Option<String>,
    context_tracks: Vec<usize>,
    recent: Vec<usize>,
}

pub struct Demo {
    tracks: Vec<Track>,
    albums: Vec<Album>,
    artists: Vec<Artist>,
    state: Mutex<State>,
}

fn img(seed: &str) -> Vec<Image> {
    vec![Image { url: format!("demo://{seed}"), width: Some(300), height: Some(300) }]
}

impl Demo {
    pub fn new() -> Self {
        let artists: Vec<Artist> = ARTISTS
            .iter()
            .enumerate()
            .map(|(i, (name, genres))| Artist {
                id: Some(format!("artist{i}")),
                name: name.to_string(),
                uri: format!("spotify:artist:artist{i}"),
                images: img(&format!("artist{i}")),
                genres: genres.iter().map(|g| g.to_string()).collect(),
                followers: Some(Followers { total: 120_000 + i as u64 * 87_431 }),
                popularity: Some(70 - i as u32 * 5),
            })
            .collect();
        let mut albums = Vec::new();
        let mut tracks = Vec::new();
        for (ai, (artist, name, year, names)) in ALBUMS.iter().enumerate() {
            let a = &artists[*artist];
            let album = Album {
                id: Some(format!("album{ai}")),
                name: name.to_string(),
                uri: format!("spotify:album:album{ai}"),
                images: img(&format!("album{ai}")),
                artists: vec![SimpleArtist { id: a.id.clone(), name: a.name.clone(), uri: a.uri.clone() }],
                release_date: Some(format!("{year}-05-01")),
                total_tracks: Some(names.len() as u32),
                album_type: Some(if names.len() < 5 { "single".into() } else { "album".into() }),
                label: None,
            };
            for (ti, tn) in names.iter().enumerate() {
                let dur = 150_000 + (crate::anim::hash((ai * 31 + ti) as u64) % 120_000);
                tracks.push(Track {
                    id: Some(format!("t{ai}x{ti}")),
                    name: tn.to_string(),
                    uri: format!("spotify:track:t{ai}x{ti}"),
                    duration_ms: dur,
                    artists: album.artists.clone(),
                    album: Some(album.clone()),
                    explicit: ti == 2 && ai % 3 == 0,
                    popularity: Some(80 - (ti as u32 * 7) % 40),
                    track_number: Some(ti as u32 + 1),
                    is_local: false,
                    is_episode: false,
                });
            }
            albums.push(album);
        }
        let saved: HashSet<String> = tracks.iter().step_by(3).map(|t| t.uri.clone()).collect();
        let context_tracks: Vec<usize> = (0..6).collect();
        Self {
            tracks,
            albums,
            artists,
            state: Mutex::new(State {
                track: 0,
                progress_ms: 42_000,
                at: Instant::now(),
                playing: true,
                volume: 65,
                shuffle: false,
                repeat: "off".into(),
                saved,
                queue: Vec::new(),
                context: Some("spotify:album:album0".into()),
                context_tracks,
                recent: vec![3, 9, 14, 20],
            }),
        }
    }

    fn idx(&self, uri: &str) -> Option<usize> {
        self.tracks.iter().position(|t| t.uri == uri)
    }

    fn album_tracks(&self, ai: usize) -> Vec<usize> {
        let id = format!("album{ai}");
        (0..self.tracks.len())
            .filter(|&i| self.tracks[i].album.as_ref().and_then(|a| a.id.as_deref()) == Some(id.as_str()))
            .collect()
    }

    fn playlist(&self, pi: usize) -> Playlist {
        let (name, desc, items) = PLAYLISTS[pi];
        Playlist {
            id: format!("pl{pi}"),
            name: name.into(),
            uri: format!("spotify:playlist:pl{pi}"),
            images: img(&format!("pl{pi}")),
            owner: Owner { id: "demo".into(), display_name: Some("Demo Listener".into()) },
            description: Some(desc.into()),
            tracks: Some(TracksRef { total: items.len() as u32 }),
            items: None,
            followers: Some(Followers { total: 1_234 * (pi as u64 + 1) }),
        }
    }

    fn playlist_tracks(&self, pi: usize) -> Vec<usize> {
        PLAYLISTS[pi]
            .2
            .iter()
            .filter_map(|(a, t)| self.album_tracks(*a).get(*t).copied())
            .collect()
    }

    fn context_list(&self, uri: &str) -> Vec<usize> {
        let id = uri_id(uri);
        if let Some(n) = id.strip_prefix("album").and_then(|n| n.parse::<usize>().ok()) {
            return self.album_tracks(n);
        }
        if let Some(n) = id.strip_prefix("pl").and_then(|n| n.parse::<usize>().ok()) {
            return self.playlist_tracks(n);
        }
        if let Some(n) = id.strip_prefix("artist").and_then(|n| n.parse::<usize>().ok()) {
            return self.top(n);
        }
        if uri.ends_with(":collection") {
            let s = self.state.lock().unwrap();
            return (0..self.tracks.len()).filter(|i| s.saved.contains(&self.tracks[*i].uri)).collect();
        }
        Vec::new()
    }

    fn top(&self, artist: usize) -> Vec<usize> {
        let id = format!("artist{artist}");
        let mut v: Vec<usize> = (0..self.tracks.len())
            .filter(|&i| self.tracks[i].artists.first().and_then(|a| a.id.as_deref()) == Some(id.as_str()))
            .collect();
        v.sort_by_key(|&i| std::cmp::Reverse(self.tracks[i].popularity.unwrap_or(0)));
        v.truncate(8);
        v
    }

    /// Advance the simulated clock (track ends → next).
    fn tick(&self) {
        let mut s = self.state.lock().unwrap();
        if !s.playing {
            return;
        }
        let elapsed = s.at.elapsed().as_millis() as u64;
        s.progress_ms += elapsed;
        s.at = Instant::now();
        while s.progress_ms >= self.tracks[s.track].duration_ms {
            s.progress_ms -= self.tracks[s.track].duration_ms;
            self.advance(&mut s, 1);
        }
    }

    fn advance(&self, s: &mut State, dir: i32) {
        let prev = s.track;
        if dir > 0 && !s.queue.is_empty() {
            s.track = s.queue.remove(0);
        } else if s.repeat == "track" && dir > 0 {
        } else {
            let list = if s.context_tracks.is_empty() { (0..self.tracks.len()).collect() } else { s.context_tracks.clone() };
            let pos = list.iter().position(|&i| i == s.track).unwrap_or(0) as i32;
            let n = list.len() as i32;
            let next = if s.shuffle && dir > 0 {
                (crate::anim::hash(s.track as u64 + s.at.elapsed().as_nanos() as u64) % n as u64) as i32
            } else {
                (pos + dir).rem_euclid(n)
            };
            s.track = list[next as usize];
        }
        if prev != s.track {
            s.recent.retain(|&r| r != prev);
            s.recent.insert(0, prev);
        }
    }

    // ------------------------------------------------------------ API mirror

    pub fn me(&self) -> User {
        User { id: "demo".into(), display_name: Some("Demo Listener".into()), product: Some("premium".into()), country: Some("US".into()) }
    }

    pub fn playback(&self) -> Option<Playback> {
        self.tick();
        let s = self.state.lock().unwrap();
        Some(Playback {
            device: Some(self.devices()[0].clone()).map(|mut d| {
                d.volume_percent = Some(s.volume);
                d
            }),
            repeat_state: s.repeat.clone(),
            shuffle_state: s.shuffle,
            progress_ms: s.progress_ms,
            is_playing: s.playing,
            item: Some(self.tracks[s.track].clone()),
            context: s.context.clone().map(|uri| Context { uri, kind: "album".into() }),
        })
    }

    pub fn play(&self, ctx: Option<String>, uris: Option<Vec<String>>, offset: Option<String>, offset_pos: Option<u32>) {
        self.tick();
        let mut s = self.state.lock().unwrap();
        let list: Vec<usize> = match (&ctx, &uris) {
            (Some(c), _) => self.context_list(c),
            (None, Some(u)) => u.iter().filter_map(|u| self.idx(u)).collect(),
            _ => Vec::new(),
        };
        if !list.is_empty() {
            let start = offset
                .as_deref()
                .and_then(|o| self.idx(o))
                .or_else(|| offset_pos.and_then(|p| list.get(p as usize).copied()))
                .unwrap_or(list[0]);
            if start != s.track {
                let prev = s.track;
                s.recent.insert(0, prev);
            }
            s.track = start;
            s.progress_ms = 0;
            s.context_tracks = list;
            s.context = ctx;
        }
        s.playing = true;
        s.at = Instant::now();
    }

    pub fn pause(&self) {
        self.tick();
        self.state.lock().unwrap().playing = false;
    }

    pub fn skip(&self, dir: i32) {
        self.tick();
        let mut s = self.state.lock().unwrap();
        if dir < 0 && s.progress_ms > 3000 {
            s.progress_ms = 0;
            return;
        }
        self.advance(&mut s, dir);
        s.progress_ms = 0;
        s.at = Instant::now();
    }

    pub fn seek(&self, ms: u64) {
        let mut s = self.state.lock().unwrap();
        s.progress_ms = ms;
        s.at = Instant::now();
    }

    pub fn volume(&self, v: u32) {
        self.state.lock().unwrap().volume = v;
    }
    pub fn shuffle(&self, on: bool) {
        self.state.lock().unwrap().shuffle = on;
    }
    pub fn repeat(&self, r: &str) {
        self.state.lock().unwrap().repeat = r.to_string();
    }

    pub fn devices(&self) -> Vec<Device> {
        vec![
            Device { id: Some("demo-device".into()), name: "Spotr (demo)".into(), kind: "Computer".into(), volume_percent: Some(65), is_active: true, is_restricted: false, supports_volume: true },
            Device { id: Some("demo-speaker".into()), name: "Living Room".into(), kind: "Speaker".into(), volume_percent: Some(40), is_active: false, is_restricted: false, supports_volume: true },
            Device { id: Some("demo-phone".into()), name: "Phone".into(), kind: "Smartphone".into(), volume_percent: Some(100), is_active: false, is_restricted: false, supports_volume: false },
        ]
    }

    pub fn queue(&self) -> SpotifyQueue {
        let s = self.state.lock().unwrap();
        let mut q: Vec<Track> = s.queue.iter().map(|&i| self.tracks[i].clone()).collect();
        let pos = s.context_tracks.iter().position(|&i| i == s.track).unwrap_or(0);
        q.extend(s.context_tracks.iter().skip(pos + 1).take(6).map(|&i| self.tracks[i].clone()));
        SpotifyQueue { queue: q }
    }

    pub fn add_to_queue(&self, uri: &str) {
        if let Some(i) = self.idx(uri) {
            self.state.lock().unwrap().queue.push(i);
        }
    }

    pub fn recently_played(&self) -> Vec<Track> {
        self.state.lock().unwrap().recent.iter().map(|&i| self.tracks[i].clone()).collect()
    }

    pub fn search(&self, q: &str) -> SearchResults {
        let q = q.to_lowercase();
        let words: Vec<&str> = q.split_whitespace().collect();
        let hit = |s: &str| {
            let s = s.to_lowercase();
            words.iter().all(|w| s.contains(w))
        };
        let tracks: Vec<Track> = self
            .tracks
            .iter()
            .filter(|t| hit(&format!("{} {} {}", t.name, t.artist_names(), t.album_name())))
            .cloned()
            .collect();
        let artists: Vec<Artist> = self.artists.iter().filter(|a| hit(&format!("{} {}", a.name, a.genres.join(" ")))).cloned().collect();
        let albums: Vec<Album> = self
            .albums
            .iter()
            .filter(|a| hit(&format!("{} {}", a.name, a.artists[0].name)))
            .cloned()
            .collect();
        let playlists: Vec<Playlist> = (0..PLAYLISTS.len())
            .map(|i| self.playlist(i))
            .filter(|p| hit(&format!("{} {}", p.name, p.description.clone().unwrap_or_default())))
            .collect();
        let totals = [tracks.len() as u32, artists.len() as u32, albums.len() as u32, playlists.len() as u32];
        SearchResults { tracks, artists, albums, playlists, totals }
    }

    pub fn album(&self, id: &str) -> Option<(Album, Vec<Track>)> {
        let n: usize = id.strip_prefix("album")?.parse().ok()?;
        Some((self.albums.get(n)?.clone(), self.album_tracks(n).into_iter().map(|i| self.tracks[i].clone()).collect()))
    }

    pub fn artist(&self, id: &str) -> Option<Artist> {
        let n: usize = id.strip_prefix("artist")?.parse().ok()?;
        self.artists.get(n).cloned()
    }

    pub fn artist_top(&self, id: &str) -> Vec<Track> {
        let n: usize = id.strip_prefix("artist").and_then(|n| n.parse().ok()).unwrap_or(0);
        self.top(n).into_iter().map(|i| self.tracks[i].clone()).collect()
    }

    pub fn artist_albums(&self, id: &str) -> Vec<Album> {
        self.albums.iter().filter(|a| a.artists[0].id.as_deref() == Some(id)).cloned().collect()
    }

    pub fn playlist_by_id(&self, id: &str) -> Option<(Playlist, Vec<Track>)> {
        let n: usize = id.strip_prefix("pl")?.parse().ok()?;
        if n >= PLAYLISTS.len() {
            return None;
        }
        Some((self.playlist(n), self.playlist_tracks(n).into_iter().map(|i| self.tracks[i].clone()).collect()))
    }

    pub fn my_playlists(&self) -> Vec<Playlist> {
        (0..PLAYLISTS.len()).map(|i| self.playlist(i)).collect()
    }

    pub fn saved_tracks(&self) -> Vec<Track> {
        let s = self.state.lock().unwrap();
        self.tracks.iter().filter(|t| s.saved.contains(&t.uri)).cloned().collect()
    }

    pub fn saved_albums(&self) -> Vec<Album> {
        vec![self.albums[0].clone(), self.albums[4].clone()]
    }

    pub fn followed_artists(&self) -> Vec<Artist> {
        self.artists.clone()
    }

    pub fn top_tracks(&self) -> Vec<Track> {
        let mut v = self.tracks.clone();
        v.sort_by_key(|t| std::cmp::Reverse(t.popularity.unwrap_or(0)));
        v.truncate(15);
        v
    }

    pub fn is_saved(&self, id: &str) -> bool {
        self.state.lock().unwrap().saved.contains(&format!("spotify:track:{id}"))
    }

    pub fn set_saved(&self, id: &str, saved: bool) {
        let uri = format!("spotify:track:{id}");
        let mut s = self.state.lock().unwrap();
        if saved {
            s.saved.insert(uri);
        } else {
            s.saved.remove(&uri);
        }
    }
}

/// Synced lyrics for demo songs (original text).
pub fn lyrics() -> crate::lyrics::Lyrics {
    let mut lines = Vec::new();
    let mut t = 6_000u64;
    for l in LYRICS {
        lines.push((Some(t), l.to_string()));
        t += if l.is_empty() { 3_000 } else { 4_200 };
    }
    crate::lyrics::Lyrics { lines, synced: true, instrumental: false }
}

/// Procedural cover art for `demo://` image URLs.
pub fn image(seed: &str) -> image::DynamicImage {
    let h = crate::anim::hash(seed.bytes().fold(7u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64)));
    let pick = |shift: u32| ((h >> shift) & 0xff) as f32;
    let (r1, g1, b1) = (pick(0), pick(8), pick(16));
    let (r2, g2, b2) = (pick(24), pick(32), pick(40));
    let style = (h >> 48) % 4;
    let size = 160u32;
    let img = image::RgbImage::from_fn(size, size, |x, y| {
        let fx = x as f32 / size as f32;
        let fy = y as f32 / size as f32;
        let t = match style {
            0 => fy,
            1 => (fx + fy) / 2.0,
            2 => ((fx - 0.5).powi(2) + (fy - 0.5).powi(2)).sqrt() * 1.4,
            _ => (((fx * 6.0).sin() + (fy * 5.0).cos()) * 0.25 + 0.5).clamp(0.0, 1.0),
        };
        let mut r = r1 + (r2 - r1) * t;
        let mut g = g1 + (g2 - g1) * t;
        let mut b = b1 + (b2 - b1) * t;
        // A bright disc / ring for some character.
        let d = ((fx - 0.62).powi(2) + (fy - 0.38).powi(2)).sqrt();
        if (style == 0 && d < 0.18) || (style == 2 && (0.2..0.24).contains(&d)) || (style == 1 && d < 0.1) {
            r = 255.0 - r * 0.2;
            g = 230.0 - g * 0.3;
            b = 180.0 - b * 0.3;
        }
        if style == 3 && ((fy * 20.0) as u32).is_multiple_of(5) {
            r *= 0.6;
            g *= 0.6;
            b *= 0.6;
        }
        image::Rgb([r.clamp(0.0, 255.0) as u8, g.clamp(0.0, 255.0) as u8, b.clamp(0.0, 255.0) as u8])
    });
    image::DynamicImage::ImageRgb8(img)
}
