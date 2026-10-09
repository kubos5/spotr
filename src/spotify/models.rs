//! Spotify Web API data types (only the fields Spotr uses).

use serde::{Deserialize, Deserializer, Serialize};

fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Image {
    pub url: String,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

/// Pick the image closest to (but not below) a target size.
pub fn best_image(images: &[Image], target: u32) -> Option<String> {
    if images.is_empty() {
        return None;
    }
    let mut sorted: Vec<&Image> = images.iter().collect();
    sorted.sort_by_key(|i| i.width.unwrap_or(640));
    sorted
        .iter()
        .find(|i| i.width.unwrap_or(640) >= target)
        .or(sorted.last())
        .map(|i| i.url.clone())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimpleArtist {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub uri: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Album {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub uri: String,
    #[serde(default, deserialize_with = "null_default")]
    pub images: Vec<Image>,
    #[serde(default, deserialize_with = "null_default")]
    pub artists: Vec<SimpleArtist>,
    #[serde(default)]
    pub release_date: Option<String>,
    #[serde(default)]
    pub total_tracks: Option<u32>,
    #[serde(default)]
    pub album_type: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
}

impl Album {
    pub fn year(&self) -> Option<&str> {
        self.release_date.as_deref().and_then(|d| d.get(..4))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Track {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub uri: String,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default, deserialize_with = "null_default")]
    pub artists: Vec<SimpleArtist>,
    #[serde(default)]
    pub album: Option<Album>,
    #[serde(default)]
    pub explicit: bool,
    #[serde(default)]
    pub popularity: Option<u32>,
    #[serde(default)]
    pub track_number: Option<u32>,
    #[serde(default)]
    pub is_local: bool,
    /// Set for podcast episodes, which Spotr shows like tracks.
    #[serde(skip)]
    pub is_episode: bool,
}

impl Track {
    pub fn artist_names(&self) -> String {
        self.artists.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")
    }
    pub fn first_artist(&self) -> Option<&SimpleArtist> {
        self.artists.first()
    }
    pub fn image(&self, target: u32) -> Option<String> {
        self.album.as_ref().and_then(|a| best_image(&a.images, target))
    }
    pub fn album_name(&self) -> &str {
        self.album.as_ref().map(|a| a.name.as_str()).unwrap_or("")
    }

    /// Normalize a "track or episode" JSON object.
    pub fn from_playable(v: serde_json::Value) -> Option<Track> {
        if v.is_null() {
            return None;
        }
        let kind = v.get("type").and_then(|t| t.as_str()).unwrap_or("track");
        if kind == "episode" {
            let show = v.get("show");
            let show_name = show
                .and_then(|s| s.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("Podcast")
                .to_string();
            let images: Vec<Image> = v
                .get("images")
                .or_else(|| show.and_then(|s| s.get("images")))
                .and_then(|i| serde_json::from_value(i.clone()).ok())
                .unwrap_or_default();
            Some(Track {
                id: v.get("id").and_then(|i| i.as_str()).map(String::from),
                name: v.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                uri: v.get("uri").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                duration_ms: v.get("duration_ms").and_then(|d| d.as_u64()).unwrap_or(0),
                artists: vec![SimpleArtist { id: None, name: show_name.clone(), uri: String::new() }],
                album: Some(Album { name: show_name, images, ..Default::default() }),
                explicit: v.get("explicit").and_then(|e| e.as_bool()).unwrap_or(false),
                is_episode: true,
                ..Default::default()
            })
        } else {
            serde_json::from_value(v).ok()
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Followers {
    #[serde(default)]
    pub total: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Artist {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub uri: String,
    #[serde(default, deserialize_with = "null_default")]
    pub images: Vec<Image>,
    #[serde(default, deserialize_with = "null_default")]
    pub genres: Vec<String>,
    #[serde(default)]
    pub followers: Option<Followers>,
    #[serde(default)]
    pub popularity: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Owner {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TracksRef {
    #[serde(default)]
    pub total: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Playlist {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub uri: String,
    #[serde(default, deserialize_with = "null_default")]
    pub images: Vec<Image>,
    #[serde(default)]
    pub owner: Owner,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tracks: Option<TracksRef>,
    #[serde(default)]
    pub items: Option<TracksRef>,
    #[serde(default)]
    pub followers: Option<Followers>,
}

impl Playlist {
    pub fn total(&self) -> u32 {
        self.items.as_ref().or(self.tracks.as_ref()).map(|t| t.total).unwrap_or(0)
    }
    pub fn owner_name(&self) -> &str {
        self.owner.display_name.as_deref().unwrap_or(&self.owner.id)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Paging<T> {
    #[serde(default = "Vec::new")]
    pub items: Vec<T>,
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub next: Option<String>,
    #[serde(default)]
    pub offset: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Device {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub volume_percent: Option<u32>,
    #[serde(default)]
    pub is_active: bool,
    #[serde(default)]
    pub is_restricted: bool,
    #[serde(default)]
    pub supports_volume: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Context {
    #[serde(default)]
    pub uri: String,
    #[serde(rename = "type", default)]
    pub kind: String,
}

#[derive(Debug, Clone, Default)]
pub struct Playback {
    pub device: Option<Device>,
    pub repeat_state: String,
    pub shuffle_state: bool,
    pub progress_ms: u64,
    pub is_playing: bool,
    pub item: Option<Track>,
    pub context: Option<Context>,
}

impl Playback {
    pub fn from_json(v: serde_json::Value) -> Playback {
        Playback {
            device: v.get("device").and_then(|d| serde_json::from_value(d.clone()).ok()),
            repeat_state: v
                .get("repeat_state")
                .and_then(|s| s.as_str())
                .unwrap_or("off")
                .to_string(),
            shuffle_state: v.get("shuffle_state").and_then(|s| s.as_bool()).unwrap_or(false),
            progress_ms: v.get("progress_ms").and_then(|p| p.as_u64()).unwrap_or(0),
            is_playing: v.get("is_playing").and_then(|p| p.as_bool()).unwrap_or(false),
            item: v.get("item").cloned().and_then(Track::from_playable),
            context: v.get("context").and_then(|c| serde_json::from_value(c.clone()).ok()),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub product: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
}

impl User {
    pub fn name(&self) -> &str {
        self.display_name.as_deref().filter(|s| !s.is_empty()).unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    pub tracks: Vec<Track>,
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub playlists: Vec<Playlist>,
    pub totals: [u32; 4],
}

/// Anything that can appear in a result list.
#[derive(Debug, Clone)]
pub enum Item {
    Track(Track),
    Artist(Artist),
    Album(Album),
    Playlist(Playlist),
    /// A virtual entry such as "Liked Songs" in the library.
    Link { title: String, subtitle: String, target: LinkTarget },
    Header(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    LikedSongs,
    Recent,
    TopTracks,
    PlayAll,
    ShuffleAll,
}

impl Item {
    pub fn name(&self) -> &str {
        match self {
            Item::Track(t) => &t.name,
            Item::Artist(a) => &a.name,
            Item::Album(a) => &a.name,
            Item::Playlist(p) => &p.name,
            Item::Link { title, .. } => title,
            Item::Header(h) => h,
        }
    }
    pub fn uri(&self) -> Option<&str> {
        match self {
            Item::Track(t) => Some(&t.uri),
            Item::Artist(a) => Some(&a.uri),
            Item::Album(a) => Some(&a.uri),
            Item::Playlist(p) => Some(&p.uri),
            _ => None,
        }
    }
    pub fn image(&self, target: u32) -> Option<String> {
        match self {
            Item::Track(t) => t.image(target),
            Item::Artist(a) => best_image(&a.images, target),
            Item::Album(a) => best_image(&a.images, target),
            Item::Playlist(p) => best_image(&p.images, target),
            _ => None,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Item::Track(t) if t.is_episode => "Episode",
            Item::Track(_) => "Song",
            Item::Artist(_) => "Artist",
            Item::Album(a) => match a.album_type.as_deref() {
                Some("single") => "Single",
                Some("compilation") => "Compilation",
                _ => "Album",
            },
            Item::Playlist(_) => "Playlist",
            Item::Link { target: LinkTarget::PlayAll | LinkTarget::ShuffleAll, .. } => "",
            Item::Link { .. } => "Collection",
            Item::Header(_) => "",
        }
    }
    pub fn is_selectable(&self) -> bool {
        !matches!(self, Item::Header(_))
    }
}

#[derive(Debug, Clone, Default)]
pub struct SpotifyQueue {
    pub queue: Vec<Track>,
}

/// Extract the id from `spotify:track:ID` or an open.spotify.com URL.
pub fn uri_id(uri: &str) -> &str {
    let s = uri.rsplit([':', '/']).next().unwrap_or(uri);
    s.split('?').next().unwrap_or(s)
}

/// Convert open.spotify.com links into `spotify:` URIs.
pub fn normalize_uri(input: &str) -> Option<String> {
    let input = input.trim();
    if input.starts_with("spotify:") {
        return Some(input.to_string());
    }
    let rest = input
        .strip_prefix("https://open.spotify.com/")
        .or_else(|| input.strip_prefix("http://open.spotify.com/"))
        .or_else(|| input.strip_prefix("open.spotify.com/"))?;
    let mut parts = rest.split('/').filter(|p| !p.starts_with("intl-"));
    let kind = parts.next()?;
    let id = parts.next()?.split('?').next()?;
    Some(format!("spotify:{kind}:{id}"))
}

pub fn share_url(uri: &str) -> Option<String> {
    let mut parts = uri.split(':');
    if parts.next()? != "spotify" {
        return None;
    }
    let kind = parts.next()?;
    let id = parts.next()?;
    Some(format!("https://open.spotify.com/{kind}/{id}"))
}

pub fn fmt_duration(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris() {
        assert_eq!(
            normalize_uri("https://open.spotify.com/intl-de/track/abc?si=x").as_deref(),
            Some("spotify:track:abc")
        );
        assert_eq!(uri_id("spotify:album:xyz"), "xyz");
        assert_eq!(share_url("spotify:track:abc").as_deref(), Some("https://open.spotify.com/track/abc"));
        assert_eq!(fmt_duration(3_725_000), "1:02:05");
        assert_eq!(fmt_duration(65_000), "1:05");
    }
}
