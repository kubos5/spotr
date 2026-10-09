//! Thin async client for the Spotify Web API.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Result, anyhow};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::Mutex;

use super::auth::{self, TokenSet};
use super::models::*;

const API: &str = "https://api.spotify.com/v1";

/// An error returned by the Web API.
#[derive(Debug)]
pub struct ApiError {
    pub status: u16,
    pub reason: Option<String>,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.reason.as_deref() {
            Some("NO_ACTIVE_DEVICE") => write!(f, "No active device — open devices (Ctrl+D) to pick one"),
            Some("PREMIUM_REQUIRED") => write!(f, "Spotify Premium is required for playback control"),
            _ if self.status == 401 => write!(f, "Not signed in — run `login`"),
            _ if self.status == 403 => write!(
                f,
                "Spotify refused this ({}). Development-mode apps only get some endpoints, and only for users on the app's allowlist",
                self.message
            ),
            _ if self.status == 429 => write!(f, "Rate limited by Spotify, slow down a bit"),
            _ => write!(f, "{} ({})", self.message, self.status),
        }
    }
}
impl std::error::Error for ApiError {}

pub struct Spotify {
    http: reqwest::Client,
    token: Mutex<Option<TokenSet>>,
    token_path: PathBuf,
    country: std::sync::Mutex<Option<String>>,
    demo: Option<super::demo::Demo>,
}

#[derive(Default, Clone)]
pub struct PlayRequest {
    pub context_uri: Option<String>,
    pub uris: Option<Vec<String>>,
    pub offset_uri: Option<String>,
    pub offset_position: Option<u32>,
    pub position_ms: Option<u64>,
}

impl Spotify {
    pub fn new(http: reqwest::Client, token_path: PathBuf) -> Self {
        let token = auth::load_token(&token_path);
        Self { http, token: Mutex::new(token), token_path, country: Default::default(), demo: None }
    }

    /// A client backed by the built-in fictional catalog (`--demo`).
    pub fn new_demo(http: reqwest::Client, token_path: PathBuf) -> Self {
        Self { demo: Some(super::demo::Demo::new()), ..Self::new(http, token_path) }
    }

    pub fn is_demo(&self) -> bool {
        self.demo.is_some()
    }

    pub async fn set_token(&self, t: TokenSet) -> Result<()> {
        auth::save_token(&self.token_path, &t)?;
        *self.token.lock().await = Some(t);
        Ok(())
    }

    pub async fn logout(&self) {
        auth::delete_token(&self.token_path);
        *self.token.lock().await = None;
    }

    /// A valid access token, refreshing if needed.
    pub async fn access_token(&self) -> Result<String> {
        let mut guard = self.token.lock().await;
        let tok = guard.as_ref().ok_or_else(|| anyhow!("Not signed in — run `login`"))?;
        if tok.expires_soon() {
            let fresh = auth::refresh(&self.http, &tok.client_id, &tok.refresh_token).await?;
            auth::save_token(&self.token_path, &fresh)?;
            *guard = Some(fresh);
        }
        Ok(guard.as_ref().unwrap().access_token.clone())
    }

    async fn force_refresh(&self) -> Result<()> {
        let mut guard = self.token.lock().await;
        if let Some(tok) = guard.as_ref() {
            let fresh = auth::refresh(&self.http, &tok.client_id, &tok.refresh_token).await?;
            auth::save_token(&self.token_path, &fresh)?;
            *guard = Some(fresh);
        }
        Ok(())
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Option<Value>> {
        let url = if path.starts_with("http") { path.to_string() } else { format!("{API}{path}") };
        let mut attempt = 0;
        loop {
            attempt += 1;
            let token = self.access_token().await?;
            let mut req = self
                .http
                .request(method.clone(), &url)
                .bearer_auth(&token)
                .timeout(Duration::from_secs(15));
            if !query.is_empty() {
                req = req.query(query);
            }
            req = match &body {
                Some(b) => req.json(b),
                None if method != Method::GET => req.header("Content-Length", "0"),
                None => req,
            };
            let resp = req.send().await?;
            let status = resp.status();
            if status == StatusCode::UNAUTHORIZED && attempt == 1 {
                self.force_refresh().await?;
                continue;
            }
            if status == StatusCode::TOO_MANY_REQUESTS && attempt <= 2 {
                let wait = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(1)
                    .min(5);
                tokio::time::sleep(Duration::from_secs(wait)).await;
                continue;
            }
            let text = resp.text().await.unwrap_or_default();
            if !status.is_success() {
                let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                let err = v.get("error");
                let message = err
                    .and_then(|e| e.get("message").or(Some(e)))
                    .and_then(|m| m.as_str())
                    .unwrap_or(status.canonical_reason().unwrap_or("request failed"))
                    .to_string();
                let reason = err.and_then(|e| e.get("reason")).and_then(|r| r.as_str()).map(String::from);
                return Err(ApiError { status: status.as_u16(), reason, message }.into());
            }
            if text.trim().is_empty() {
                return Ok(None);
            }
            // Some player endpoints answer with a bare snapshot id or plain text.
            return Ok(Some(serde_json::from_str(&text).unwrap_or(Value::Null)));
        }
    }

    async fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, String)]) -> Result<T> {
        let v = self.request(Method::GET, path, query, None).await?.unwrap_or(Value::Null);
        Ok(serde_json::from_value(v)?)
    }

    fn dev(device_id: Option<&str>) -> Vec<(&'static str, String)> {
        device_id.map(|d| vec![("device_id", d.to_string())]).unwrap_or_default()
    }

    // ---------------------------------------------------------------- user

    pub async fn me(&self) -> Result<User> {
        if let Some(d) = &self.demo {
            return Ok(d.me());
        }
        let u: User = self.get("/me", &[]).await?;
        *self.country.lock().unwrap() = u.country.clone();
        Ok(u)
    }

    fn market(&self) -> String {
        self.country.lock().unwrap().clone().unwrap_or_else(|| "from_token".into())
    }

    // -------------------------------------------------------------- player

    pub async fn playback(&self) -> Result<Option<Playback>> {
        if let Some(d) = &self.demo {
            return Ok(d.playback());
        }
        let v = self
            .request(Method::GET, "/me/player", &[("additional_types", "track,episode".into())], None)
            .await?;
        Ok(v.filter(|v| !v.is_null()).map(Playback::from_json))
    }

    pub async fn play(&self, device: Option<&str>, req: Option<PlayRequest>) -> Result<()> {
        if let Some(d) = &self.demo {
            if let Some(r) = req {
                d.play(r.context_uri, r.uris, r.offset_uri, r.offset_position);
                if let Some(p) = r.position_ms {
                    d.seek(p);
                }
            } else {
                d.play(None, None, None, None);
            }
            return Ok(());
        }
        let body = req.map(|r| {
            let mut b = json!({});
            // Spotify accepts either a context or a list of URIs, not both.
            if let Some(c) = r.context_uri {
                b["context_uri"] = json!(c);
            } else if let Some(u) = r.uris {
                b["uris"] = json!(u);
            }
            if let Some(o) = r.offset_uri {
                b["offset"] = json!({ "uri": o });
            } else if let Some(p) = r.offset_position {
                b["offset"] = json!({ "position": p });
            }
            if let Some(p) = r.position_ms {
                b["position_ms"] = json!(p);
            }
            b
        });
        self.request(Method::PUT, "/me/player/play", &Self::dev(device), body).await?;
        Ok(())
    }

    pub async fn pause(&self, device: Option<&str>) -> Result<()> {
        if let Some(d) = &self.demo {
            d.pause();
            return Ok(());
        }
        self.request(Method::PUT, "/me/player/pause", &Self::dev(device), None).await?;
        Ok(())
    }

    pub async fn next(&self, device: Option<&str>) -> Result<()> {
        if let Some(d) = &self.demo {
            d.skip(1);
            return Ok(());
        }
        self.request(Method::POST, "/me/player/next", &Self::dev(device), None).await?;
        Ok(())
    }

    pub async fn previous(&self, device: Option<&str>) -> Result<()> {
        if let Some(d) = &self.demo {
            d.skip(-1);
            return Ok(());
        }
        self.request(Method::POST, "/me/player/previous", &Self::dev(device), None).await?;
        Ok(())
    }

    pub async fn seek(&self, position_ms: u64) -> Result<()> {
        if let Some(d) = &self.demo {
            d.seek(position_ms);
            return Ok(());
        }
        self.request(Method::PUT, "/me/player/seek", &[("position_ms", position_ms.to_string())], None)
            .await?;
        Ok(())
    }

    pub async fn volume(&self, percent: u32) -> Result<()> {
        if let Some(d) = &self.demo {
            d.volume(percent.min(100));
            return Ok(());
        }
        self.request(
            Method::PUT,
            "/me/player/volume",
            &[("volume_percent", percent.min(100).to_string())],
            None,
        )
        .await?;
        Ok(())
    }

    pub async fn shuffle(&self, on: bool) -> Result<()> {
        if let Some(d) = &self.demo {
            d.shuffle(on);
            return Ok(());
        }
        self.request(Method::PUT, "/me/player/shuffle", &[("state", on.to_string())], None).await?;
        Ok(())
    }

    /// `state` is one of `off`, `context`, `track`.
    pub async fn repeat(&self, state: &str) -> Result<()> {
        if let Some(d) = &self.demo {
            d.repeat(state);
            return Ok(());
        }
        self.request(Method::PUT, "/me/player/repeat", &[("state", state.to_string())], None).await?;
        Ok(())
    }

    pub async fn devices(&self) -> Result<Vec<Device>> {
        if let Some(d) = &self.demo {
            return Ok(d.devices());
        }
        #[derive(serde::Deserialize)]
        struct R {
            devices: Vec<Device>,
        }
        Ok(self.get::<R>("/me/player/devices", &[]).await?.devices)
    }

    pub async fn transfer(&self, device_id: &str, play: bool) -> Result<()> {
        if let Some(d) = &self.demo {
            let _ = (d, device_id, play);
            return Ok(());
        }
        self.request(
            Method::PUT,
            "/me/player",
            &[],
            Some(json!({ "device_ids": [device_id], "play": play })),
        )
        .await?;
        Ok(())
    }

    pub async fn queue(&self) -> Result<SpotifyQueue> {
        if let Some(d) = &self.demo {
            return Ok(d.queue());
        }
        let v = self.request(Method::GET, "/me/player/queue", &[], None).await?.unwrap_or(Value::Null);
        Ok(SpotifyQueue {
            queue: v
                .get("queue")
                .and_then(|q| q.as_array())
                .map(|a| a.iter().cloned().filter_map(Track::from_playable).collect())
                .unwrap_or_default(),
        })
    }

    pub async fn add_to_queue(&self, uri: &str, device: Option<&str>) -> Result<()> {
        if let Some(d) = &self.demo {
            d.add_to_queue(uri);
            return Ok(());
        }
        let mut q = vec![("uri", uri.to_string())];
        q.extend(Self::dev(device));
        self.request(Method::POST, "/me/player/queue", &q, None).await?;
        Ok(())
    }

    pub async fn recently_played(&self) -> Result<Vec<Track>> {
        if let Some(d) = &self.demo {
            return Ok(d.recently_played());
        }
        let v: Value = self.get("/me/player/recently-played", &[("limit", "50".into())]).await?;
        let mut seen = std::collections::HashSet::new();
        Ok(v.get("items")
            .and_then(|i| i.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|it| it.get("track").cloned().and_then(Track::from_playable))
                    .filter(|t| seen.insert(t.uri.clone()))
                    .collect()
            })
            .unwrap_or_default())
    }

    // -------------------------------------------------------------- search

    pub async fn search(&self, q: &str, types: &[&str], limit: u32, offset: u32) -> Result<SearchResults> {
        if let Some(d) = &self.demo {
            let mut r = d.search(q);
            let skip = offset as usize;
            r.tracks = r.tracks.into_iter().skip(skip).take(limit as usize).collect();
            r.artists = r.artists.into_iter().skip(skip).take(limit as usize).collect();
            r.albums = r.albums.into_iter().skip(skip).take(limit as usize).collect();
            r.playlists = r.playlists.into_iter().skip(skip).take(limit as usize).collect();
            let _ = types;
            return Ok(r);
        }
        let v: Value = self
            .get(
                "/search",
                &[
                    ("q", q.to_string()),
                    ("type", types.join(",")),
                    ("limit", limit.to_string()),
                    ("offset", offset.to_string()),
                ],
            )
            .await?;
        fn list<T: DeserializeOwned>(v: &Value, key: &str) -> (Vec<T>, u32) {
            let section = &v[key];
            let items = section["items"]
                .as_array()
                .map(|a| a.iter().filter(|x| !x.is_null()).filter_map(|x| serde_json::from_value(x.clone()).ok()).collect())
                .unwrap_or_default();
            (items, section["total"].as_u64().unwrap_or(0) as u32)
        }
        let (tracks, t0) = list(&v, "tracks");
        let (artists, t1) = list(&v, "artists");
        let (albums, t2) = list(&v, "albums");
        let (playlists, t3) = list(&v, "playlists");
        Ok(SearchResults { tracks, artists, albums, playlists, totals: [t0, t1, t2, t3] })
    }

    // ------------------------------------------------------------- catalog

    pub async fn album(&self, id: &str) -> Result<(Album, Vec<Track>, Option<String>)> {
        if let Some(d) = &self.demo {
            let (a, t) = d.album(id).ok_or_else(|| anyhow!("album not found"))?;
            return Ok((a, t, None));
        }
        let v: Value = self.get(&format!("/albums/{id}"), &[("market", self.market())]).await?;
        let album: Album = serde_json::from_value(v.clone())?;
        let page: Paging<Value> = serde_json::from_value(v["tracks"].clone()).unwrap_or_default();
        let tracks = page
            .items
            .into_iter()
            .filter_map(|t| serde_json::from_value::<Track>(t).ok())
            .map(|mut t| {
                // Album tracks come without the album object.
                t.album = Some(album.clone());
                t
            })
            .collect();
        Ok((album, tracks, page.next))
    }

    pub async fn album_tracks_page(&self, album: &Album, url: &str) -> Result<(Vec<Track>, Option<String>)> {
        let page: Paging<Track> = self.get(url, &[]).await?;
        let tracks = page
            .items
            .into_iter()
            .map(|mut t| {
                t.album = Some(album.clone());
                t
            })
            .collect();
        Ok((tracks, page.next))
    }

    pub async fn artist(&self, id: &str) -> Result<Artist> {
        if let Some(d) = &self.demo {
            return d.artist(id).ok_or_else(|| anyhow!("artist not found"));
        }
        self.get(&format!("/artists/{id}"), &[]).await
    }

    /// An artist's popular songs. Spotify removed `/artists/{id}/top-tracks` for
    /// development-mode apps (Feb 2026), so this searches by artist instead.
    pub async fn artist_top_tracks(&self, id: &str, name: &str) -> Result<Vec<Track>> {
        if let Some(d) = &self.demo {
            return Ok(d.artist_top(id));
        }
        let q = format!("artist:\"{}\"", name.replace('"', ""));
        let mut tracks = Vec::new();
        // Search returns at most 10 results per request.
        for offset in [0, 10] {
            let r = self.search(&q, &["track"], 10, offset).await?;
            let done = r.tracks.len() < 10;
            tracks.extend(r.tracks);
            if done {
                break;
            }
        }
        // Prefer songs where they're the main artist, keep features after.
        let mut seen = std::collections::HashSet::new();
        tracks.retain(|t| t.artists.iter().any(|a| a.id.as_deref() == Some(id)) && seen.insert(t.name.to_lowercase()));
        tracks.sort_by_key(|t| t.artists.first().and_then(|a| a.id.as_deref()) != Some(id));
        tracks.truncate(10);
        Ok(tracks)
    }

    pub async fn artist_albums(&self, id: &str) -> Result<Vec<Album>> {
        if let Some(d) = &self.demo {
            return Ok(d.artist_albums(id));
        }
        let page: Paging<Album> = self
            .get(
                &format!("/artists/{id}/albums"),
                &[("include_groups", "album,single,compilation".into()), ("limit", "40".into())],
            )
            .await?;
        Ok(page.items)
    }

    pub async fn playlist(&self, id: &str) -> Result<Playlist> {
        if let Some(d) = &self.demo {
            return d.playlist_by_id(id).map(|p| p.0).ok_or_else(|| anyhow!("playlist not found"));
        }
        self.get(
            &format!("/playlists/{id}"),
            &[("fields", "id,name,uri,images,owner,description,followers,collaborative,items.total,tracks.total".into())],
        )
        .await
    }

    pub async fn playlist_tracks(&self, id_or_url: &str, offset: u32) -> Result<(Vec<Track>, Option<String>, u32)> {
        if let Some(d) = &self.demo {
            let (_, t) = d.playlist_by_id(id_or_url).ok_or_else(|| anyhow!("playlist not found"))?;
            let n = t.len() as u32;
            let _ = offset;
            return Ok((t, None, n));
        }
        let v: Value = if id_or_url.starts_with("http") {
            self.get(id_or_url, &[]).await?
        } else {
            self.get(
                &format!("/playlists/{id_or_url}/items"),
                &[
                    ("limit", "50".into()),
                    ("offset", offset.to_string()),
                    ("additional_types", "track,episode".into()),
                    ("market", self.market()),
                ],
            )
            .await?
        };
        Ok(Self::track_page(v))
    }

    fn track_page(v: Value) -> (Vec<Track>, Option<String>, u32) {
        let tracks = v["items"]
            .as_array()
            .map(|a| {
                a.iter()
                    // `item` replaced `track` in Feb 2026; accept both.
                    .filter_map(|it| it.get("item").or(it.get("track")).cloned().and_then(Track::from_playable))
                    .collect()
            })
            .unwrap_or_default();
        (tracks, v["next"].as_str().map(String::from), v["total"].as_u64().unwrap_or(0) as u32)
    }

    // ------------------------------------------------------------- library

    pub async fn saved_tracks(&self, url: Option<&str>) -> Result<(Vec<Track>, Option<String>, u32)> {
        if let Some(d) = &self.demo {
            let t = d.saved_tracks();
            let n = t.len() as u32;
            let _ = url;
            return Ok((t, None, n));
        }
        let v: Value = match url {
            Some(u) => self.get(u, &[]).await?,
            None => self.get("/me/tracks", &[("limit", "50".into()), ("market", self.market())]).await?,
        };
        Ok(Self::track_page(v))
    }

    pub async fn my_playlists(&self) -> Result<Vec<Playlist>> {
        if let Some(d) = &self.demo {
            return Ok(d.my_playlists());
        }
        let mut out = Vec::new();
        let mut url: Option<String> = None;
        for _ in 0..5 {
            let page: Paging<Option<Playlist>> = match &url {
                Some(u) => self.get(u, &[]).await?,
                None => self.get("/me/playlists", &[("limit", "50".into())]).await?,
            };
            out.extend(page.items.into_iter().flatten());
            match page.next {
                Some(n) => url = Some(n),
                None => break,
            }
        }
        Ok(out)
    }

    pub async fn saved_albums(&self) -> Result<Vec<Album>> {
        if let Some(d) = &self.demo {
            return Ok(d.saved_albums());
        }
        let v: Value = self.get("/me/albums", &[("limit", "50".into())]).await?;
        Ok(v["items"]
            .as_array()
            .map(|a| a.iter().filter_map(|it| serde_json::from_value(it["album"].clone()).ok()).collect())
            .unwrap_or_default())
    }

    pub async fn followed_artists(&self) -> Result<Vec<Artist>> {
        if let Some(d) = &self.demo {
            return Ok(d.followed_artists());
        }
        let v: Value = self.get("/me/following", &[("type", "artist".into()), ("limit", "50".into())]).await?;
        Ok(v["artists"]["items"]
            .as_array()
            .map(|a| a.iter().filter_map(|it| serde_json::from_value(it.clone()).ok()).collect())
            .unwrap_or_default())
    }

    pub async fn top_tracks(&self) -> Result<Vec<Track>> {
        if let Some(d) = &self.demo {
            return Ok(d.top_tracks());
        }
        let page: Paging<Track> =
            self.get("/me/top/tracks", &[("limit", "50".into()), ("time_range", "short_term".into())]).await?;
        Ok(page.items)
    }

    /// Save (`true`) or remove URIs (tracks, albums, artists, playlists…) from the library.
    /// Replaces the per-type `/me/tracks`, `/me/albums`, `/me/following` endpoints.
    pub async fn set_in_library(&self, uris: &[String], saved: bool) -> Result<()> {
        let m = if saved { Method::PUT } else { Method::DELETE };
        self.request(m, "/me/library", &[("uris", uris.join(","))], None).await?;
        Ok(())
    }

    pub async fn library_contains(&self, uris: &[String]) -> Result<Vec<bool>> {
        self.get("/me/library/contains", &[("uris", uris.join(","))]).await
    }

    pub async fn is_saved(&self, track_id: &str) -> Result<bool> {
        if let Some(d) = &self.demo {
            return Ok(d.is_saved(track_id));
        }
        let v = self.library_contains(&[format!("spotify:track:{track_id}")]).await?;
        Ok(v.first().copied().unwrap_or(false))
    }

    pub async fn set_saved(&self, track_id: &str, saved: bool) -> Result<()> {
        if let Some(d) = &self.demo {
            d.set_saved(track_id, saved);
            return Ok(());
        }
        self.set_in_library(&[format!("spotify:track:{track_id}")], saved).await
    }

    pub async fn set_album_saved(&self, album_id: &str, saved: bool) -> Result<()> {
        if self.demo.is_some() {
            return Ok(());
        }
        self.set_in_library(&[format!("spotify:album:{album_id}")], saved).await
    }

    pub async fn set_following(&self, artist_id: &str, follow: bool) -> Result<()> {
        if self.demo.is_some() {
            return Ok(());
        }
        self.set_in_library(&[format!("spotify:artist:{artist_id}")], follow).await
    }

    pub async fn add_to_playlist(&self, playlist_id: &str, uris: &[String]) -> Result<()> {
        if self.demo.is_some() {
            return Ok(());
        }
        self.request(
            Method::POST,
            &format!("/playlists/{playlist_id}/items"),
            &[],
            Some(json!({ "uris": uris })),
        )
        .await?;
        Ok(())
    }
}

/// Is this error "there is no active device"?
pub fn is_no_device(e: &anyhow::Error) -> bool {
    e.downcast_ref::<ApiError>()
        .map(|a| a.reason.as_deref() == Some("NO_ACTIVE_DEVICE") || (a.status == 404 && a.message.contains("device")))
        .unwrap_or(false)
}
