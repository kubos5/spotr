//! Search results and browse pages (album / artist / playlist / library).

use std::time::Instant;

use super::*;
use crate::config::SearchContext;
use crate::spotify::PlayRequest;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageKind {
    Album,
    Artist,
    Playlist,
    Liked,
    Recent,
    Top,
    Library,
}

pub struct Page {
    pub id: u64,
    pub kind: PageKind,
    pub title: String,
    pub subtitle: String,
    pub meta: String,
    pub image: Option<String>,
    pub context_uri: Option<String>,
    pub items: Vec<Item>,
    pub list: ListNav,
    pub loading: bool,
    pub loading_more: bool,
    pub next: Option<String>,
    pub error: Option<String>,
    pub opened: Instant,
    /// Raw album, for paging album tracks.
    pub album: Option<Album>,
}

pub struct PageData {
    pub title: String,
    pub subtitle: String,
    pub meta: String,
    pub image: Option<String>,
    pub context_uri: Option<String>,
    pub items: Vec<Item>,
    pub next: Option<String>,
    pub album: Option<Album>,
}

const PLAY_ALL: &str = "Play";
const SHUFFLE_ALL: &str = "Shuffle play";

fn play_buttons(items: &mut Vec<Item>, has_tracks: bool) {
    if has_tracks {
        items.insert(0, Item::Link { title: PLAY_ALL.into(), subtitle: String::new(), target: LinkTarget::PlayAll });
        items.insert(1, Item::Link { title: SHUFFLE_ALL.into(), subtitle: String::new(), target: LinkTarget::ShuffleAll });
    }
}

impl App {
    pub fn open_search(&mut self, focus_input: bool) {
        if self.overlay != Overlay::None {
            self.overlay = Overlay::None;
        }
        if !self.search_open {
            self.search_open = true;
            self.search.shown_at = Instant::now();
            self.needs_clear = true;
        }
        if focus_input {
            if self.focus != Focus::SearchInput && !self.search.input.is_empty() {
                self.search.select_all = true;
            }
            self.focus = Focus::SearchInput;
        } else if self.focus != Focus::SearchInput {
            self.focus = Focus::SearchList;
        }
    }

    /// Hide the search UI. All of its state is kept for next time.
    pub fn close_search(&mut self) {
        if self.search_open {
            self.search_open = false;
            self.view_changed_at = Instant::now();
            self.needs_clear = true;
        }
        self.focus = Focus::Main;
        if !self.cfg.search.remember_position {
            self.search.stack.clear();
            self.search.list = ListNav::default();
        }
    }

    pub fn search_query_changed(&mut self) {
        // Typing a new query leaves any open pages.
        if !self.search.stack.is_empty() {
            self.search.stack.clear();
            self.needs_clear = true;
        }
        if self.cfg.search.live {
            self.search.pending = Some(Instant::now());
        }
    }

    pub fn run_search(&mut self, append: bool) {
        let q = self.search.input.value.trim().to_string();
        if q.is_empty() {
            self.search.items.clear();
            self.search.query.clear();
            self.search.error = None;
            self.search.loading = false;
            return;
        }
        if !append && q == self.search.query && !self.search.items.is_empty() && self.search.error.is_none() {
            return;
        }
        self.search.seq += 1;
        let seq = self.search.seq;
        let filter = self.search.filter;
        let types: Vec<&str> = match filter {
            SearchFilter::All => vec!["track", "artist", "album", "playlist"],
            SearchFilter::Tracks => vec!["track"],
            SearchFilter::Artists => vec!["artist"],
            SearchFilter::Albums => vec!["album"],
            SearchFilter::Playlists => vec!["playlist"],
        };
        let limit = self.cfg.search.results_per_type;
        let offset = if append { self.search_count_of(filter) as u32 } else { 0 };
        if append {
            self.search.loading_more = true;
        } else {
            self.search.loading = true;
            self.search.error = None;
        }
        let sp = self.spotify.clone();
        self.spawn(async move {
            let result = sp.search(&q, &types, limit, offset).await.map_err(|e| e.to_string());
            AppEvent::Search { seq, append, result }
        });
        if !append {
            self.search.query = self.search.input.value.trim().to_string();
        }
    }

    fn search_count_of(&self, f: SearchFilter) -> usize {
        let r = &self.search.results;
        match f {
            SearchFilter::Tracks => r.tracks.len(),
            SearchFilter::Artists => r.artists.len(),
            SearchFilter::Albums => r.albums.len(),
            SearchFilter::Playlists => r.playlists.len(),
            SearchFilter::All => 0,
        }
    }

    pub(super) fn on_search_results(&mut self, seq: u64, append: bool, result: Result<SearchResults, String>) {
        if seq != self.search.seq {
            return;
        }
        self.search.loading = false;
        self.search.loading_more = false;
        match result {
            Err(e) => {
                self.search.error = Some(e);
            }
            Ok(r) => {
                if append {
                    let cur = &mut self.search.results;
                    cur.tracks.extend(r.tracks);
                    cur.artists.extend(r.artists);
                    cur.albums.extend(r.albums);
                    cur.playlists.extend(r.playlists);
                } else {
                    self.search.results = r;
                    self.search.list = ListNav::default();
                    self.search.shown_at = Instant::now();
                }
                self.rebuild_search_items();
                if !append {
                    self.search.list.first(&self.search.items);
                }
            }
        }
    }

    pub fn rebuild_search_items(&mut self) {
        let r = &self.search.results;
        let q = self.search.query.to_lowercase();
        let mut items = Vec::new();
        let f = self.search.filter;
        let cap = |n: usize| n;
        match f {
            SearchFilter::All => {
                if self.cfg.search.show_top_result
                    && let Some(top) = top_result(r, &q) {
                        items.push(top);
                    }
                let sections: [(&str, Vec<Item>); 4] = [
                    ("Songs", r.tracks.iter().cloned().map(Item::Track).collect()),
                    ("Artists", r.artists.iter().cloned().map(Item::Artist).collect()),
                    ("Albums", r.albums.iter().cloned().map(Item::Album).collect()),
                    ("Playlists", r.playlists.iter().cloned().map(Item::Playlist).collect()),
                ];
                for (name, list) in sections {
                    if !list.is_empty() {
                        items.push(Item::Header(name.into()));
                        items.extend(list.into_iter().take(cap(usize::MAX)));
                    }
                }
            }
            SearchFilter::Tracks => items.extend(r.tracks.iter().cloned().map(Item::Track)),
            SearchFilter::Artists => items.extend(r.artists.iter().cloned().map(Item::Artist)),
            SearchFilter::Albums => items.extend(r.albums.iter().cloned().map(Item::Album)),
            SearchFilter::Playlists => items.extend(r.playlists.iter().cloned().map(Item::Playlist)),
        }
        let idx = match f {
            SearchFilter::Tracks => 0,
            SearchFilter::Artists => 1,
            SearchFilter::Albums => 2,
            SearchFilter::Playlists => 3,
            SearchFilter::All => 4,
        };
        self.search.has_more = idx < 4 && (self.search_count_of(f) as u32) < r.totals[idx];
        self.search.items = items;
        self.search.list.fix(&self.search.items);
    }

    pub fn set_filter(&mut self, f: SearchFilter) {
        if self.search.filter == f {
            return;
        }
        self.search.filter = f;
        self.search.shown_at = Instant::now();
        // Re-query so single-type filters get a full page of results.
        let q = std::mem::take(&mut self.search.query);
        self.search.stack.clear();
        if q.is_empty() {
            return;
        }
        self.search.input.set(&q);
        self.run_search(false);
    }

    /// The list currently shown in the browse area.
    pub fn browse_items(&self) -> &[Item] {
        match self.search.stack.last() {
            Some(p) => &p.items,
            None => &self.search.items,
        }
    }

    pub fn browse_nav(&mut self) -> &mut ListNav {
        match self.search.stack.last_mut() {
            Some(p) => &mut p.list,
            None => &mut self.search.list,
        }
    }

    pub fn browse_step(&mut self, delta: i32) {
        let items: Vec<Item> = self.browse_items().to_vec();
        self.browse_nav().step(delta, &items);
        self.maybe_load_more();
    }

    pub fn selected_item(&self) -> Option<&Item> {
        match self.search.stack.last() {
            Some(p) => p.items.get(p.list.selected),
            None => self.search.items.get(self.search.list.selected),
        }
    }

    pub fn maybe_load_more(&mut self) {
        if let Some(p) = self.search.stack.last() {
            if p.next.is_some() && !p.loading_more && p.list.selected + 8 >= p.items.len() {
                self.load_more_page();
            }
            return;
        }
        let s = &self.search;
        if s.has_more && !s.loading_more && !s.loading && s.list.selected + 5 >= s.items.len() {
            self.run_search(true);
        }
    }

    /// Enter on the selected row.
    pub fn activate_selected(&mut self) {
        let Some(item) = self.selected_item().cloned() else { return };
        self.activate_item(item);
    }

    pub fn activate_item(&mut self, item: Item) {
        match item {
            Item::Track(t) => {
                self.play_track_in_context(&t);
                if self.cfg.search.close_on_play {
                    self.close_search();
                }
            }
            Item::Album(a) => self.open_album(&a),
            Item::Artist(a) => self.open_artist(&a),
            Item::Playlist(p) => self.open_playlist(&p),
            Item::Link { target, .. } => match target {
                LinkTarget::LikedSongs => self.open_liked(),
                LinkTarget::Recent => self.open_recent(),
                LinkTarget::TopTracks => self.open_top(),
                LinkTarget::PlayAll | LinkTarget::ShuffleAll => {
                    if target == LinkTarget::ShuffleAll {
                        self.set_shuffle(true);
                    } else if self.pb.shuffle {
                        self.set_shuffle(false);
                    }
                    self.play_page();
                    if self.cfg.search.close_on_play {
                        self.close_search();
                    }
                }
            },
            Item::Header(_) => {}
        }
    }

    fn play_page(&mut self) {
        let Some(p) = self.search.stack.last() else { return };
        let label = p.title.clone();
        let tracks: Vec<String> = p
            .items
            .iter()
            .filter_map(|i| if let Item::Track(t) = i { Some(t.uri.clone()) } else { None })
            .take(100)
            .collect();
        // No song list for playlists Spotify won't list: play the context alone.
        let uris = if tracks.is_empty() { None } else { Some(tracks) };
        let req = match &p.context_uri {
            Some(c) => PlayRequest { context_uri: Some(c.clone()), uris, ..Default::default() },
            None => PlayRequest { uris, ..Default::default() },
        };
        self.play_request(req, label);
    }

    /// Play a track respecting where it was found.
    fn play_track_in_context(&mut self, t: &Track) {
        let page = self.search.stack.last();
        let label = t.name.clone();
        match page {
            Some(p) if p.context_uri.is_some() => {
                let ctx = p.context_uri.clone().unwrap();
                // Keep a URI list around in case the context is refused.
                let uris: Vec<String> = p
                    .items
                    .iter()
                    .filter_map(|i| if let Item::Track(t) = i { Some(t.uri.clone()) } else { None })
                    .take(100)
                    .collect();
                if p.kind == PageKind::Artist {
                    // Artist pages show top tracks: play them as a list.
                    self.play_request(
                        PlayRequest { uris: Some(uris), offset_uri: Some(t.uri.clone()), ..Default::default() },
                        label,
                    );
                } else {
                    self.play_request(
                        PlayRequest {
                            context_uri: Some(ctx),
                            uris: Some(uris),
                            offset_uri: Some(t.uri.clone()),
                            ..Default::default()
                        },
                        label,
                    );
                }
            }
            Some(p) => {
                let uris: Vec<String> = p
                    .items
                    .iter()
                    .filter_map(|i| if let Item::Track(t) = i { Some(t.uri.clone()) } else { None })
                    .take(100)
                    .collect();
                self.play_request(PlayRequest { uris: Some(uris), offset_uri: Some(t.uri.clone()), ..Default::default() }, label);
            }
            None => match (&t.album, self.cfg.playback.search_context) {
                (Some(a), SearchContext::Album) if !a.uri.is_empty() && !t.is_episode => {
                    self.play_request(
                        PlayRequest {
                            context_uri: Some(a.uri.clone()),
                            offset_uri: Some(t.uri.clone()),
                            uris: Some(vec![t.uri.clone()]),
                            ..Default::default()
                        },
                        label,
                    );
                }
                _ => self.play_track_alone(t),
            },
        }
    }

    // ------------------------------------------------------------ pages

    fn push_page(&mut self, kind: PageKind, title: String, image: Option<String>) -> u64 {
        let id = self.next_page_id();
        if let Some(url) = &image {
            self.want_image(url);
        }
        self.search.stack.push(Page {
            id,
            kind,
            title,
            subtitle: String::new(),
            meta: String::new(),
            image,
            context_uri: None,
            items: Vec::new(),
            list: ListNav::default(),
            loading: true,
            loading_more: false,
            next: None,
            error: None,
            opened: Instant::now(),
            album: None,
        });
        if !self.search_open {
            self.search_open = true;
            self.search.shown_at = Instant::now();
        }
        self.focus = Focus::SearchList;
        self.needs_clear = true;
        id
    }

    pub fn pop_page(&mut self) -> bool {
        if self.search.stack.pop().is_some() {
            self.needs_clear = true;
            self.view_changed_at = Instant::now();
            true
        } else {
            false
        }
    }

    pub(super) fn on_page(&mut self, id: u64, result: Result<PageData, String>) {
        let mut want = None;
        if let Some(p) = self.search.stack.iter_mut().find(|p| p.id == id) {
            p.loading = false;
            match result {
                Ok(d) => {
                    p.title = d.title;
                    p.subtitle = d.subtitle;
                    p.meta = d.meta;
                    if d.image.is_some() {
                        p.image = d.image;
                    }
                    p.context_uri = d.context_uri;
                    p.items = d.items;
                    p.next = d.next;
                    p.album = d.album;
                    p.opened = Instant::now();
                    p.list.fix(&p.items);
                    // Skip the play buttons so Enter plays the first song.
                    if let Some(i) = p.items.iter().position(|i| matches!(i, Item::Track(_)))
                        && p.items.len() > 2 && matches!(p.items[0], Item::Link { target: LinkTarget::PlayAll, .. }) {
                            p.list.selected = i;
                        }
                    want = p.image.clone();
                }
                Err(e) => p.error = Some(e),
            }
        }
        if let Some(url) = want {
            self.want_image(&url);
        }
    }

    pub(super) fn on_page_more(&mut self, id: u64, result: Result<(Vec<Item>, Option<String>), String>) {
        if let Some(p) = self.search.stack.iter_mut().find(|p| p.id == id) {
            p.loading_more = false;
            match result {
                Ok((items, next)) => {
                    p.items.extend(items);
                    p.next = next;
                }
                Err(e) => {
                    p.next = None;
                    self.toasts.push(Toast { msg: e, kind: ToastKind::Error, born: Instant::now(), ttl: std::time::Duration::from_secs(5) });
                }
            }
        }
    }

    fn load_more_page(&mut self) {
        let Some(p) = self.search.stack.last_mut() else { return };
        let Some(next) = p.next.clone() else { return };
        p.loading_more = true;
        let id = p.id;
        let kind = p.kind;
        let album = p.album.clone();
        let sp = self.spotify.clone();
        self.spawn(async move {
            let result = async {
                let (items, next) = match kind {
                    PageKind::Album => {
                        let (t, n) = sp.album_tracks_page(album.as_ref().unwrap_or(&Album::default()), &next).await?;
                        (t, n)
                    }
                    PageKind::Playlist => {
                        let (t, n, _) = sp.playlist_tracks(&next, 0).await?;
                        (t, n)
                    }
                    PageKind::Liked => {
                        let (t, n, _) = sp.saved_tracks(Some(&next)).await?;
                        (t, n)
                    }
                    _ => (Vec::new(), None),
                };
                Ok::<_, anyhow::Error>((items.into_iter().map(Item::Track).collect(), next))
            }
            .await
            .map_err(|e| e.to_string());
            AppEvent::PageMore { id, result }
        });
    }

    pub fn open_album(&mut self, a: &Album) {
        let Some(album_id) = a.id.clone().or_else(|| Some(uri_id(&a.uri).to_string())).filter(|s| !s.is_empty()) else {
            return;
        };
        let id = self.push_page(PageKind::Album, a.name.clone(), best_image(&a.images, 300));
        let sp = self.spotify.clone();
        self.spawn(async move {
            let result = async {
                let (album, tracks, next) = sp.album(&album_id).await?;
                let total_ms: u64 = tracks.iter().map(|t| t.duration_ms).sum();
                let mut items: Vec<Item> = tracks.into_iter().map(Item::Track).collect();
                let has_tracks = !items.is_empty();
                play_buttons(&mut items, has_tracks);
                Ok::<_, anyhow::Error>(PageData {
                    title: album.name.clone(),
                    subtitle: album.artists.iter().map(|a| a.name.clone()).collect::<Vec<_>>().join(", "),
                    meta: format!(
                        "{} · {} · {} songs · {}",
                        cap_first(album.album_type.as_deref().unwrap_or("album")),
                        album.year().unwrap_or("—"),
                        album.total_tracks.unwrap_or(0),
                        fmt_long(total_ms)
                    ),
                    image: best_image(&album.images, 300),
                    context_uri: Some(album.uri.clone()),
                    items,
                    next,
                    album: Some(album),
                })
            }
            .await
            .map_err(|e| e.to_string());
            AppEvent::Page { id, result }
        });
    }

    pub fn open_artist(&mut self, a: &Artist) {
        let artist_id = a.id.clone().unwrap_or_else(|| uri_id(&a.uri).to_string());
        self.open_artist_id(&artist_id, &a.name, best_image(&a.images, 300));
    }

    pub fn open_artist_id(&mut self, artist_id: &str, name: &str, image: Option<String>) {
        let artist_id = artist_id.to_string();
        let id = self.push_page(PageKind::Artist, name.to_string(), image);
        let sp = self.spotify.clone();
        self.spawn(async move {
            let result = async {
                let artist = sp.artist(&artist_id).await?;
                let top = sp.artist_top_tracks(&artist_id, &artist.name).await.unwrap_or_default();
                let albums = sp.artist_albums(&artist_id).await.unwrap_or_default();
                let mut items = Vec::new();
                if !top.is_empty() {
                    items.push(Item::Header("Popular".into()));
                    items.extend(top.into_iter().map(Item::Track));
                }
                let (full, singles): (Vec<Album>, Vec<Album>) =
                    albums.into_iter().partition(|a| a.album_type.as_deref() != Some("single"));
                if !full.is_empty() {
                    items.push(Item::Header("Albums".into()));
                    items.extend(full.into_iter().map(Item::Album));
                }
                if !singles.is_empty() {
                    items.push(Item::Header("Singles & EPs".into()));
                    items.extend(singles.into_iter().map(Item::Album));
                }
                let has_tracks = items.iter().any(|i| matches!(i, Item::Track(_)));
                play_buttons(&mut items, has_tracks);
                // Spotify stopped sending follower counts to development-mode apps.
                let followers = artist.followers.as_ref().map(|f| f.total).filter(|n| *n > 0);
                Ok::<_, anyhow::Error>(PageData {
                    title: artist.name.clone(),
                    subtitle: if artist.genres.is_empty() { "Artist".into() } else { artist.genres.iter().take(3).cloned().collect::<Vec<_>>().join(" · ") },
                    meta: followers.map(|f| format!("{} followers", fmt_count(f))).unwrap_or_else(|| "Artist".into()),
                    image: best_image(&artist.images, 300),
                    context_uri: Some(artist.uri.clone()),
                    items,
                    next: None,
                    album: None,
                })
            }
            .await
            .map_err(|e| e.to_string());
            AppEvent::Page { id, result }
        });
    }

    pub fn open_playlist(&mut self, p: &Playlist) {
        let pid = if p.id.is_empty() { uri_id(&p.uri).to_string() } else { p.id.clone() };
        let id = self.push_page(PageKind::Playlist, p.name.clone(), best_image(&p.images, 300));
        let sp = self.spotify.clone();
        let fallback = p.clone();
        self.spawn(async move {
            let result = async {
                let pl = sp.playlist(&pid).await.unwrap_or(fallback);
                // Since Feb 2026 Spotify only lists the songs of playlists you own or
                // collaborate on; others can still be played as a whole.
                let (tracks, next, total, hidden) = match sp.playlist_tracks(&pid, 0).await {
                    Ok((t, n, total)) => (t, n, total, false),
                    Err(e) if e.downcast_ref::<crate::spotify::api::ApiError>().is_some_and(|a| a.status == 403 || a.status == 404) => {
                        (Vec::new(), None, 0, true)
                    }
                    Err(e) => return Err(e),
                };
                let mut items: Vec<Item> = tracks.into_iter().map(Item::Track).collect();
                let has_tracks = !items.is_empty() || hidden;
                play_buttons(&mut items, has_tracks);
                if hidden {
                    items.push(Item::Header("Spotify only lists songs for playlists you own or collaborate on — you can still play this one".into()));
                }
                let desc = pl.description.clone().unwrap_or_default();
                Ok::<_, anyhow::Error>(PageData {
                    title: pl.name.clone(),
                    subtitle: if desc.is_empty() { format!("by {}", pl.owner_name()) } else { strip_html(&desc) },
                    meta: format!(
                        "Playlist · by {}{}{}",
                        pl.owner_name(),
                        match total.max(pl.total()) {
                            0 => String::new(),
                            n => format!(" · {n} songs"),
                        },
                        pl.followers.as_ref().map(|f| format!(" · {} saves", fmt_count(f.total))).unwrap_or_default()
                    ),
                    image: best_image(&pl.images, 300),
                    context_uri: Some(pl.uri.clone()),
                    items,
                    next,
                    album: None,
                })
            }
            .await
            .map_err(|e| e.to_string());
            AppEvent::Page { id, result }
        });
    }

    pub fn open_liked(&mut self) {
        let id = self.push_page(PageKind::Liked, "Liked Songs".into(), None);
        let sp = self.spotify.clone();
        let user = self.user.as_ref().map(|u| u.id.clone());
        self.spawn(async move {
            let result = async {
                let (tracks, next, total) = sp.saved_tracks(None).await?;
                let mut items: Vec<Item> = tracks.into_iter().map(Item::Track).collect();
                let has_tracks = !items.is_empty();
                play_buttons(&mut items, has_tracks);
                Ok::<_, anyhow::Error>(PageData {
                    title: "Liked Songs".into(),
                    subtitle: "Your favorites".into(),
                    meta: format!("{total} songs"),
                    image: None,
                    context_uri: user.map(|u| format!("spotify:user:{u}:collection")),
                    items,
                    next,
                    album: None,
                })
            }
            .await
            .map_err(|e| e.to_string());
            AppEvent::Page { id, result }
        });
    }

    pub fn open_recent(&mut self) {
        let id = self.push_page(PageKind::Recent, "Recently Played".into(), None);
        let sp = self.spotify.clone();
        self.spawn(async move {
            let result = async {
                let tracks = sp.recently_played().await?;
                let n = tracks.len();
                let mut items: Vec<Item> = tracks.into_iter().map(Item::Track).collect();
                let has_tracks = !items.is_empty();
                play_buttons(&mut items, has_tracks);
                Ok::<_, anyhow::Error>(PageData {
                    title: "Recently Played".into(),
                    subtitle: "Pick up where you left off".into(),
                    meta: format!("{n} songs"),
                    image: None,
                    context_uri: None,
                    items,
                    next: None,
                    album: None,
                })
            }
            .await
            .map_err(|e| e.to_string());
            AppEvent::Page { id, result }
        });
    }

    pub fn open_top(&mut self) {
        let id = self.push_page(PageKind::Top, "Your Top Songs".into(), None);
        let sp = self.spotify.clone();
        self.spawn(async move {
            let result = async {
                let tracks = sp.top_tracks().await?;
                let n = tracks.len();
                let mut items: Vec<Item> = tracks.into_iter().map(Item::Track).collect();
                let has_tracks = !items.is_empty();
                play_buttons(&mut items, has_tracks);
                Ok::<_, anyhow::Error>(PageData {
                    title: "Your Top Songs".into(),
                    subtitle: "On repeat these past weeks".into(),
                    meta: format!("{n} songs"),
                    image: None,
                    context_uri: None,
                    items,
                    next: None,
                    album: None,
                })
            }
            .await
            .map_err(|e| e.to_string());
            AppEvent::Page { id, result }
        });
    }

    pub fn open_library(&mut self, only_playlists: bool) {
        let title = if only_playlists { "Your Playlists" } else { "Your Library" };
        let id = self.push_page(PageKind::Library, title.into(), None);
        let sp = self.spotify.clone();
        let name = self.user.as_ref().map(|u| u.name().to_string()).unwrap_or_else(|| "you".into());
        self.spawn(async move {
            let result = async {
                let mut items = Vec::new();
                if !only_playlists {
                    items.push(Item::Header("Collections".into()));
                    items.push(Item::Link { title: "Liked Songs".into(), subtitle: "Your saved songs".into(), target: LinkTarget::LikedSongs });
                    items.push(Item::Link { title: "Recently Played".into(), subtitle: "History".into(), target: LinkTarget::Recent });
                    items.push(Item::Link { title: "Your Top Songs".into(), subtitle: "Lately on repeat".into(), target: LinkTarget::TopTracks });
                }
                let playlists = sp.my_playlists().await?;
                if !playlists.is_empty() {
                    items.push(Item::Header("Playlists".into()));
                    items.extend(playlists.into_iter().map(Item::Playlist));
                }
                if !only_playlists {
                    let albums = sp.saved_albums().await.unwrap_or_default();
                    if !albums.is_empty() {
                        items.push(Item::Header("Albums".into()));
                        items.extend(albums.into_iter().map(Item::Album));
                    }
                    let artists = sp.followed_artists().await.unwrap_or_default();
                    if !artists.is_empty() {
                        items.push(Item::Header("Artists".into()));
                        items.extend(artists.into_iter().map(Item::Artist));
                    }
                }
                Ok::<_, anyhow::Error>(PageData {
                    title: title.into(),
                    subtitle: format!("Everything saved by {name}"),
                    meta: String::new(),
                    image: None,
                    context_uri: None,
                    items,
                    next: None,
                    album: None,
                })
            }
            .await
            .map_err(|e| e.to_string());
            AppEvent::Page { id, result }
        });
    }
}

/// Pick the best "top result" from the first item of each type.
fn top_result(r: &SearchResults, q: &str) -> Option<Item> {
    let score = |name: &str, pop: Option<u32>, bias: i32| -> i32 {
        let n = name.to_lowercase();
        let mut s = bias + pop.unwrap_or(0) as i32 / 4;
        if n == q {
            s += 100;
        } else if n.starts_with(q) {
            s += 40;
        } else if n.contains(q) {
            s += 15;
        }
        s
    };
    let mut best: Option<(i32, Item)> = None;
    let mut consider = |s: i32, item: Item| {
        if best.as_ref().map(|(b, _)| s > *b).unwrap_or(true) {
            best = Some((s, item));
        }
    };
    if let Some(a) = r.artists.first() {
        consider(score(&a.name, a.popularity, 6), Item::Artist(a.clone()));
    }
    if let Some(t) = r.tracks.first() {
        consider(score(&t.name, t.popularity, 5), Item::Track(t.clone()));
    }
    if let Some(a) = r.albums.first() {
        consider(score(&a.name, None, 2), Item::Album(a.clone()));
    }
    if let Some(p) = r.playlists.first() {
        consider(score(&p.name, None, 0), Item::Playlist(p.clone()));
    }
    best.map(|(_, i)| i)
}

pub fn fmt_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 10_000 {
        format!("{}K", n / 1000)
    } else {
        n.to_string()
    }
}

fn fmt_long(ms: u64) -> String {
    let m = ms / 60_000;
    if m >= 60 { format!("{} h {} min", m / 60, m % 60) } else { format!("{m} min") }
}

fn cap_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&#x27;", "'").replace("&quot;", "\"").replace("&#39;", "'")
}
