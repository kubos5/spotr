//! Spotify's own search and song radio, through the built-in player's session.
//!
//! The Web API's `/search` ranks results differently from the Spotify apps (and
//! isn't personalized), and the recommendations endpoints are gone for new apps.
//! The built-in player is signed in like a real Spotify client, so it can ask
//! the same services the apps use: the `searchDesktop` query behind the search
//! box, and "song radio" mixes for Spotify's own idea of what to play next.
//! Every caller falls back to the Web API when this isn't available.

// Without librespot only the stub remains; the parsing is still tested.
#![cfg_attr(not(feature = "local-player"), allow(dead_code))]

use serde_json::Value;

use super::models::*;

/// Persisted GraphQL queries for `searchDesktop`, newest first. Spotify keeps
/// old ones alive for a long time; the parser understands both schemas.
const SEARCH_HASHES: &[&str] = &[
    "d9f785900f0710b31c07818d617f4f7600c1e21217e80f5b043d1e78d74e6026",
    "21969b655b795601fb2d2204a4243188e75fdc6d3520e7b9cd3f4db2aff9591e",
];

#[cfg(feature = "local-player")]
pub use imp::Internal;

#[cfg(not(feature = "local-player"))]
pub use stub::Internal;

#[cfg(not(feature = "local-player"))]
mod stub {
    use super::*;

    /// Without librespot there is no session to borrow.
    pub struct Internal;

    impl Internal {
        pub async fn search(&self, _q: &str, _limit: u32, _offset: u32) -> anyhow::Result<SearchResults> {
            anyhow::bail!("Spotify search needs the built-in player")
        }
        pub async fn radio(&self, _seed: &str) -> anyhow::Result<Vec<String>> {
            anyhow::bail!("Spotify radio needs the built-in player")
        }
    }
}

#[cfg(feature = "local-player")]
mod imp {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use anyhow::{Result, anyhow};
    use librespot_core::{Session, SpotifyUri};
    use serde_json::json;

    use super::*;

    const PATHFINDER: &str = "https://api-partner.spotify.com/pathfinder/v2/query";

    pub struct Internal {
        session: Session,
        http: reqwest::Client,
        /// Index into `SEARCH_HASHES` of the query that last worked.
        hash: AtomicUsize,
    }

    impl Internal {
        pub fn new(session: Session, http: reqwest::Client) -> Self {
            Self { session, http, hash: AtomicUsize::new(0) }
        }

        /// The search the Spotify apps run: Spotify's ranking, its top result
        /// and your listening history all included.
        pub async fn search(&self, q: &str, limit: u32, offset: u32) -> Result<SearchResults> {
            let token = self.session.login5().auth_token().await.map_err(|e| anyhow!("login5: {e}"))?;
            let client_token = self.session.spclient().client_token().await.map_err(|e| anyhow!("client token: {e}"))?;
            let start = self.hash.load(Ordering::Relaxed);
            let mut last = anyhow!("no search query available");
            for i in (0..SEARCH_HASHES.len()).map(|k| (start + k) % SEARCH_HASHES.len()) {
                let body = json!({
                    "operationName": "searchDesktop",
                    "variables": {
                        "searchTerm": q,
                        "offset": offset,
                        "limit": limit,
                        "numberOfTopResults": 5,
                        "includeAudiobooks": false,
                        "includeArtistHasConcertsField": false,
                        "includePreReleases": false,
                        "includeLocalConcertsField": false,
                        "includeAuthors": false,
                    },
                    "extensions": { "persistedQuery": { "version": 1, "sha256Hash": SEARCH_HASHES[i] } },
                });
                let resp = self
                    .http
                    .post(PATHFINDER)
                    .bearer_auth(&token.access_token)
                    .header("client-token", &client_token)
                    .header("app-platform", "WebPlayer")
                    .timeout(Duration::from_secs(8))
                    .json(&body)
                    .send()
                    .await;
                let resp = match resp {
                    Ok(r) => r,
                    Err(e) => {
                        last = e.into();
                        continue;
                    }
                };
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                match v.pointer("/data/searchV2") {
                    Some(s) if status.is_success() && s.is_object() => {
                        self.hash.store(i, Ordering::Relaxed);
                        return Ok(parse_search(s));
                    }
                    _ => {
                        let msg = v.pointer("/errors/0/message").and_then(|m| m.as_str()).unwrap_or("unexpected response");
                        log::debug!("searchDesktop #{i}: {status} {msg}");
                        last = anyhow!("{msg} ({status})");
                    }
                }
            }
            Err(last)
        }

        /// Spotify's song radio for `seed`: the seed first, then what Spotify
        /// would play after it (about 50 songs).
        pub async fn radio(&self, seed: &str) -> Result<Vec<String>> {
            let uri = SpotifyUri::from_uri(seed).map_err(|e| anyhow!("{e}"))?;
            let sp = self.session.spclient();
            let raw = sp.get_radio_for_track(&uri).await.map_err(|e| anyhow!("{e}"))?;
            let v: Value = serde_json::from_slice(&raw)?;
            let mix = v
                .pointer("/mediaItems/0/uri")
                .and_then(|u| u.as_str())
                .ok_or_else(|| anyhow!("Spotify has no radio for this song"))?;
            let ctx = sp.get_context(mix).await.map_err(|e| anyhow!("{e}"))?;
            let tracks: Vec<String> = ctx
                .pages
                .iter()
                .flat_map(|p| p.tracks.iter())
                .filter_map(|t| t.uri.clone())
                .filter(|u| u.starts_with("spotify:track:"))
                .collect();
            if tracks.is_empty() {
                return Err(anyhow!("Spotify's radio for this song is empty"));
            }
            Ok(radio_order(seed, tracks))
        }
    }
}

/// A radio mix starts with the seed — or another release of it (a remaster,
/// the single…). Play exactly the song that was picked, then the rest.
pub fn radio_order(seed: &str, mix: Vec<String>) -> Vec<String> {
    let mut out = vec![seed.to_string()];
    out.extend(mix.into_iter().skip(1).filter(|u| u != seed));
    out.truncate(100);
    out
}

// ------------------------------------------------------------------ parsing

/// Read a `searchV2` object (either schema) into Spotr's models.
pub fn parse_search(s: &Value) -> SearchResults {
    let section = |keys: &[&str]| keys.iter().find_map(|k| s.get(*k)).cloned().unwrap_or(Value::Null);
    let items = |v: &Value| -> Vec<Value> {
        v.get("items")
            .and_then(|i| i.as_array())
            .map(|a| a.iter().map(unwrap_item).cloned().collect())
            .unwrap_or_default()
    };
    let total = |v: &Value| v.get("totalCount").and_then(|t| t.as_u64()).unwrap_or(0) as u32;

    let tracks_v = section(&["tracksV2", "tracks"]);
    let artists_v = section(&["artists"]);
    let albums_v = section(&["albumsV2", "albums"]);
    let playlists_v = section(&["playlists"]);
    let tracks: Vec<Track> = items(&tracks_v).iter().filter_map(track).collect();
    let artists: Vec<Artist> = items(&artists_v).iter().filter_map(artist).collect();
    let albums: Vec<Album> = items(&albums_v).iter().filter_map(album).collect();
    let playlists: Vec<Playlist> = items(&playlists_v).iter().filter_map(playlist).collect();

    let top = section(&["topResultsV2", "topResults"])
        .get("itemsV2")
        .and_then(|i| i.as_array())
        .and_then(|a| a.iter().map(unwrap_item).find_map(item));

    SearchResults {
        totals: [total(&tracks_v), total(&artists_v), total(&albums_v), total(&playlists_v)],
        tracks,
        artists,
        albums,
        playlists,
        top,
    }
}

/// Results come wrapped as `{item: {data: …}}` or `{data: …}`.
fn unwrap_item(v: &Value) -> &Value {
    let v = v.get("item").unwrap_or(v);
    v.get("data").unwrap_or(v)
}

fn str_at<'a>(v: &'a Value, path: &str) -> Option<&'a str> {
    v.pointer(path).and_then(|s| s.as_str())
}

fn images(sources: Option<&Value>) -> Vec<Image> {
    sources
        .and_then(|s| s.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|i| {
                    Some(Image {
                        url: i.get("url")?.as_str()?.to_string(),
                        width: i.get("width").and_then(|w| w.as_u64()).map(|w| w as u32),
                        height: i.get("height").and_then(|h| h.as_u64()).map(|h| h as u32),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn artists_of(v: &Value) -> Vec<SimpleArtist> {
    v.pointer("/artists/items")
        .and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let uri = x.get("uri")?.as_str()?.to_string();
                    Some(SimpleArtist { id: Some(uri_id(&uri).to_string()), name: str_at(x, "/profile/name")?.to_string(), uri })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn item(v: &Value) -> Option<Item> {
    match v.get("__typename")?.as_str()? {
        "Track" => track(v).map(Item::Track),
        "Artist" => artist(v).map(Item::Artist),
        "Album" => album(v).map(Item::Album),
        "Playlist" => playlist(v).map(Item::Playlist),
        _ => None,
    }
}

fn track(v: &Value) -> Option<Track> {
    if v.get("__typename").and_then(|t| t.as_str()).is_some_and(|t| t != "Track") {
        return None;
    }
    // Greyed out in the apps (not available in your country, removed…).
    if v.pointer("/playability/playable").and_then(|p| p.as_bool()) == Some(false) {
        return None;
    }
    let uri = v.get("uri")?.as_str()?.to_string();
    let a = v.get("albumOfTrack");
    let album = a.map(|a| {
        let auri = a.get("uri").and_then(|u| u.as_str()).unwrap_or("").to_string();
        Album {
            id: Some(uri_id(&auri).to_string()).filter(|s| !s.is_empty()),
            name: a.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
            uri: auri,
            images: images(a.pointer("/coverArt/sources")),
            ..Default::default()
        }
    });
    Some(Track {
        id: Some(uri_id(&uri).to_string()),
        name: v.get("name")?.as_str()?.to_string(),
        duration_ms: v.pointer("/duration/totalMilliseconds").and_then(|d| d.as_u64()).unwrap_or(0),
        artists: artists_of(v),
        album,
        explicit: str_at(v, "/contentRating/label") == Some("EXPLICIT"),
        uri,
        ..Default::default()
    })
}

fn artist(v: &Value) -> Option<Artist> {
    let uri = v.get("uri")?.as_str()?.to_string();
    Some(Artist {
        id: Some(uri_id(&uri).to_string()),
        name: str_at(v, "/profile/name")?.to_string(),
        images: images(v.pointer("/visuals/avatarImage/sources")),
        uri,
        ..Default::default()
    })
}

fn album(v: &Value) -> Option<Album> {
    let uri = v.get("uri")?.as_str()?.to_string();
    let kind = match v.get("type").and_then(|t| t.as_str()).unwrap_or("ALBUM") {
        "SINGLE" | "EP" => "single",
        "COMPILATION" => "compilation",
        _ => "album",
    };
    Some(Album {
        id: Some(uri_id(&uri).to_string()),
        name: v.get("name")?.as_str()?.to_string(),
        images: images(v.pointer("/coverArt/sources")),
        artists: artists_of(v),
        release_date: v.pointer("/date/year").and_then(|y| y.as_u64()).map(|y| y.to_string()),
        album_type: Some(kind.into()),
        uri,
        ..Default::default()
    })
}

fn playlist(v: &Value) -> Option<Playlist> {
    let uri = v.get("uri")?.as_str()?.to_string();
    let owner = v.pointer("/ownerV2/data");
    Some(Playlist {
        id: uri_id(&uri).to_string(),
        name: v.get("name")?.as_str()?.to_string(),
        images: images(v.pointer("/images/items/0/sources")),
        owner: Owner {
            id: owner.and_then(|o| str_at(o, "/username")).unwrap_or("").to_string(),
            display_name: owner.and_then(|o| str_at(o, "/name")).map(String::from),
        },
        description: v.get("description").and_then(|d| d.as_str()).filter(|d| !d.is_empty()).map(String::from),
        uri,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn wrap(v: Value) -> Value {
        json!({ "item": { "data": v } })
    }

    #[test]
    fn parses_both_schemas() {
        let track = json!({
            "__typename": "Track", "uri": "spotify:track:t1", "name": "God's Plan",
            "duration": { "totalMilliseconds": 198973 },
            "contentRating": { "label": "EXPLICIT" },
            "playability": { "playable": true },
            "artists": { "items": [{ "uri": "spotify:artist:a1", "profile": { "name": "Drake" } }] },
            "albumOfTrack": { "uri": "spotify:album:al1", "name": "Scorpion",
                "coverArt": { "sources": [{ "url": "https://i.scdn.co/image/x", "width": 300, "height": 300 }] } }
        });
        let gone = json!({ "__typename": "Track", "uri": "spotify:track:t2", "name": "Gone", "playability": { "playable": false } });
        let artist = json!({ "__typename": "Artist", "uri": "spotify:artist:a1", "profile": { "name": "Drake" } });
        for (albums_key, top_key) in [("albumsV2", "topResultsV2"), ("albums", "topResults")] {
            let s = json!({
                "tracksV2": { "totalCount": 149, "items": [wrap(track.clone()), wrap(gone.clone())] },
                "artists": { "totalCount": 9, "items": [{ "data": artist.clone() }] },
                albums_key: { "totalCount": 3, "items": [{ "data": {
                    "__typename": "Album", "uri": "spotify:album:al1", "name": "Scorpion", "type": "EP",
                    "date": { "year": 2018 }, "artists": { "items": [] } } }] },
                "playlists": { "totalCount": 1, "items": [{ "data": {
                    "__typename": "Playlist", "uri": "spotify:playlist:p1", "name": "Best of",
                    "ownerV2": { "data": { "name": "Ana", "username": "ana" } } } }] },
                top_key: { "itemsV2": [wrap(artist.clone())] },
            });
            let r = parse_search(&s);
            assert_eq!(r.tracks.len(), 1, "unplayable songs are skipped");
            let t = &r.tracks[0];
            assert_eq!((t.name.as_str(), t.duration_ms, t.explicit), ("God's Plan", 198973, true));
            assert_eq!(t.artist_names(), "Drake");
            assert_eq!(t.album_name(), "Scorpion");
            assert_eq!(t.image(300).as_deref(), Some("https://i.scdn.co/image/x"));
            assert_eq!(r.albums[0].album_type.as_deref(), Some("single"));
            assert_eq!(r.albums[0].year(), Some("2018"));
            assert_eq!(r.playlists[0].owner_name(), "Ana");
            assert_eq!(r.totals, [149, 9, 3, 1]);
            assert!(matches!(r.top, Some(Item::Artist(ref a)) if a.name == "Drake"));
        }
    }

    #[test]
    fn radio_starts_with_the_seed() {
        let mix = vec!["spotify:track:remaster".to_string(), "spotify:track:b".into(), "spotify:track:seed".into(), "spotify:track:c".into()];
        assert_eq!(radio_order("spotify:track:seed", mix), ["spotify:track:seed", "spotify:track:b", "spotify:track:c"]);
    }
}
