//! Your Liked Songs, kept in memory (and in the cache folder) so every list can
//! show which songs you like, and the search's Liked Songs tab can search them.
//!
//! Spotify lists Liked Songs newest first, 50 at a time. The copy on disk makes
//! later launches instant: if Spotify's count and its newest 50 songs still match,
//! nothing changed (a new like would be on that first page, and with no new likes
//! the same count means nothing was removed either), so one request is enough.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::*;
use crate::config::SearchFilter;
use crate::spotify::PlayRequest;

/// Pages of 50: enough for 25,000 songs.
const MAX_PAGES: usize = 500;
/// How often switching to the Liked Songs tab asks Spotify what changed.
const RECHECK: Duration = Duration::from_secs(60);

pub enum LikedEvent {
    /// The copy saved by an earlier session (`None`: there isn't one).
    Cached { user: String, saved: Option<Saved> },
    /// Spotify's newest songs and count match what we have.
    Unchanged { seq: u64 },
    Page { seq: u64, tracks: Vec<Track>, total: u32, done: bool },
    Failed { seq: u64, error: String },
}

#[derive(Serialize, Deserialize)]
pub struct Saved {
    total: u32,
    tracks: Vec<Track>,
}

#[derive(Default)]
pub struct Liked {
    /// Newest first.
    pub tracks: Vec<Track>,
    uris: HashSet<String>,
    /// Spotify's count (it also counts songs that are no longer available).
    pub total: u32,
    /// `tracks` is the whole list (possibly from the copy on disk).
    pub ready: bool,
    /// A refresh is running: songs fetched so far, of how many.
    pub loading: Option<(usize, u32)>,
    pub error: Option<String>,
    checked: Option<Instant>,
    pub user: Option<String>,
    /// A refresh of a list we already have, swapped in when it's complete.
    incoming: Vec<Track>,
    seq: u64,
    /// Changed since it was last saved.
    dirty: bool,
    /// Changed while a refresh was running (its pages may have shifted): fetch again.
    redo: bool,
}

impl Liked {
    pub fn contains(&self, uri: &str) -> bool {
        self.uris.contains(uri)
    }

    fn replace(&mut self, tracks: Vec<Track>) {
        self.tracks.clear();
        self.uris.clear();
        self.append(tracks);
    }

    fn append(&mut self, tracks: Vec<Track>) {
        for t in tracks {
            if self.uris.insert(t.uri.clone()) {
                self.tracks.push(t);
            }
        }
    }

    /// Like or unlike one song here (Spotify is told separately). Returns whether anything changed.
    pub fn set(&mut self, t: &Track, like: bool) -> bool {
        let changed = if like {
            let new = self.uris.insert(t.uri.clone());
            if new {
                self.tracks.insert(0, slim(t.clone()));
                self.total += 1;
            }
            new
        } else {
            let gone = self.uris.remove(&t.uri);
            if gone {
                self.tracks.retain(|x| x.uri != t.uri);
                self.total = self.total.saturating_sub(1);
            }
            gone
        };
        if changed {
            self.dirty = true;
            if self.loading.is_some() {
                self.redo = true;
            }
        }
        changed
    }
}

/// Spotify's count and newest songs match ours (`known`): nothing changed (see above).
/// Songs Spotify no longer has are left out of its pages, so the first page can
/// be shorter than ours.
fn unchanged(known: &(u32, Vec<String>), total: u32, first: &[Track]) -> bool {
    known.0 == total && first.len() <= known.1.len() && first.iter().zip(&known.1).all(|(t, u)| t.uri == *u)
}

/// Only what the lists need: one cover size instead of three.
fn slim(mut t: Track) -> Track {
    if let Some(a) = t.album.as_mut() {
        let keep = best_image(&a.images, 300);
        a.images.retain(|i| Some(&i.url) == keep.as_ref());
    }
    t
}

fn cache_file(cache_dir: &std::path::Path, user: &str) -> PathBuf {
    let safe: String = user.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    cache_dir.join(format!("liked-{safe}.json"))
}

fn read_cache(path: &std::path::Path) -> Option<Saved> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn write_cache(path: PathBuf, saved: &Saved) {
    let tmp = path.with_extension("json.tmp");
    let result = serde_json::to_vec(saved).map_err(std::io::Error::other).and_then(|data| std::fs::write(&tmp, data)).and_then(|()| std::fs::rename(&tmp, &path));
    if let Err(e) = result {
        log::warn!("couldn't save Liked Songs to {}: {e}", path.display());
    }
}

impl App {
    /// Whether a song is in your Liked Songs (local likes in a local library).
    pub fn is_liked(&self, t: &Track) -> bool {
        if self.offline.is_some() {
            return self.local_liked(&t.uri);
        }
        if let Some(saved) = self.saved_current
            && self.pb.track.as_ref().is_some_and(|c| c.uri == t.uri)
        {
            return saved;
        }
        self.liked.contains(&t.uri)
    }

    /// Signed in as `user`: load the saved copy, then ask Spotify what changed.
    pub(super) fn load_liked(&mut self, user: &str) {
        if self.liked.user.as_deref() == Some(user) {
            if self.liked.loading.is_none() && self.liked.checked.is_none_or(|c| c.elapsed() > RECHECK) {
                self.refresh_liked(false);
            }
            return;
        }
        self.liked = Liked { user: Some(user.to_string()), ..Default::default() };
        if self.spotify.is_demo() {
            self.refresh_liked(false);
            return;
        }
        let path = cache_file(&self.paths.cache_dir, user);
        let user = user.to_string();
        self.spawn(async move {
            let saved = tokio::task::spawn_blocking(move || read_cache(&path)).await.ok().flatten();
            AppEvent::Liked(LikedEvent::Cached { user, saved })
        });
    }

    /// Fetch Liked Songs from Spotify. Unless `full`, stop after the first page
    /// when nothing changed.
    pub(super) fn refresh_liked(&mut self, full: bool) {
        if self.offline.is_some() || self.liked.user.is_none() {
            return;
        }
        if self.liked.loading.is_some() {
            self.liked.redo |= full;
            return;
        }
        self.liked.seq += 1;
        let seq = self.liked.seq;
        let known = (!full && self.liked.ready).then(|| (self.liked.total, self.liked.tracks.iter().take(50).map(|t| t.uri.clone()).collect::<Vec<_>>()));
        self.liked.loading = Some((0, self.liked.total));
        self.liked.incoming.clear();
        self.liked.error = None;
        self.liked.checked = Some(Instant::now());
        let (sp, tx) = (self.spotify.clone(), self.tx.clone());
        tokio::spawn(async move {
            let send = |e| {
                let _ = tx.send(AppEvent::Liked(e));
            };
            let mut next: Option<String> = None;
            for page in 0..MAX_PAGES {
                let (tracks, n, total) = match sp.saved_tracks(next.as_deref()).await {
                    Ok(p) => p,
                    Err(e) => return send(LikedEvent::Failed { seq, error: e.to_string() }),
                };
                if page == 0 && known.as_ref().is_some_and(|k| unchanged(k, total, &tracks)) {
                    return send(LikedEvent::Unchanged { seq });
                }
                let done = n.is_none() || page + 1 == MAX_PAGES;
                send(LikedEvent::Page { seq, tracks: tracks.into_iter().map(slim).collect(), total, done });
                if done {
                    return;
                }
                next = n;
            }
        });
    }

    pub(super) fn on_liked_event(&mut self, e: LikedEvent) {
        match e {
            LikedEvent::Cached { user, saved } => {
                if self.liked.user.as_deref() != Some(user.as_str()) {
                    return;
                }
                match saved {
                    Some(s) => {
                        self.liked.replace(s.tracks);
                        self.liked.total = s.total;
                        self.liked.ready = true;
                        self.refresh_liked_tab();
                        self.refresh_liked(false);
                    }
                    None => self.refresh_liked(true),
                }
            }
            LikedEvent::Unchanged { seq } if seq == self.liked.seq => {
                self.liked.loading = None;
                self.after_liked_refresh();
            }
            LikedEvent::Page { seq, tracks, total, done } if seq == self.liked.seq => {
                let l = &mut self.liked;
                // A first load shows songs as they arrive; a refresh keeps showing
                // the old list until the new one is complete.
                let got = if l.ready {
                    l.incoming.extend(tracks);
                    l.incoming.len()
                } else {
                    l.append(tracks);
                    l.tracks.len()
                };
                l.loading = Some((got, total));
                if done {
                    if l.ready {
                        let fresh = std::mem::take(&mut l.incoming);
                        l.replace(fresh);
                    }
                    l.total = total;
                    l.ready = true;
                    l.loading = None;
                    l.dirty = true;
                    self.save_liked(false);
                    self.after_liked_refresh();
                }
                self.refresh_liked_tab();
            }
            LikedEvent::Failed { seq, error } if seq == self.liked.seq => {
                log::warn!("Liked Songs: {error}");
                self.liked.loading = None;
                self.liked.incoming.clear();
                self.liked.error = Some(error);
                self.refresh_liked_tab();
            }
            _ => {}
        }
    }

    fn after_liked_refresh(&mut self) {
        if std::mem::take(&mut self.liked.redo) {
            self.refresh_liked(true);
        }
    }

    /// Write the list to the cache folder: in the background, or right away (`now`, when quitting).
    pub(super) fn save_liked(&mut self, now: bool) {
        let l = &mut self.liked;
        let Some(user) = l.user.clone() else { return };
        if !l.dirty || !l.ready || self.spotify.is_demo() {
            return;
        }
        l.dirty = false;
        let path = cache_file(&self.paths.cache_dir, &user);
        let saved = Saved { total: l.total, tracks: l.tracks.clone() };
        if now {
            write_cache(path, &saved);
        } else {
            tokio::task::spawn_blocking(move || write_cache(path, &saved));
        }
    }

    /// A like / unlike made here: update the list right away.
    pub(super) fn liked_changed(&mut self, t: &Track, like: bool) {
        if self.offline.is_none() {
            self.liked.set(t, like);
        }
        self.refresh_liked_tab();
    }

    /// Spotify said whether the current song is liked: fix our list if it was changed elsewhere.
    pub(super) fn liked_reconcile(&mut self, saved: bool) {
        if self.offline.is_some() || !self.liked.ready {
            return;
        }
        if let Some(t) = self.pb.track.clone()
            && !t.is_episode
            && self.liked.set(&t, saved)
        {
            self.refresh_liked_tab();
        }
    }

    /// The Liked Songs tab of the search: your liked songs matching `q` (all of them for an empty query).
    pub(super) fn liked_search(&self, q: &str) -> SearchResults {
        let all: Vec<Track>;
        let tracks: &[Track] = if self.offline.is_some() {
            all = self.liked_tracks();
            &all
        } else {
            &self.liked.tracks
        };
        let found: Vec<Track> = crate::local::filter_tracks(tracks, q).into_iter().map(|i| tracks[i].clone()).collect();
        let n = found.len() as u32;
        SearchResults { tracks: found, totals: [n, 0, 0, 0], ..Default::default() }
    }

    /// Show the latest Liked Songs in the search's Liked Songs tab, keeping the selection.
    pub(super) fn refresh_liked_tab(&mut self) {
        if self.search.filter != SearchFilter::Liked {
            return;
        }
        let q = self.search.query.clone();
        self.search.results = self.liked_search(&q);
        let sel = self.search.list.selected;
        self.rebuild_search_items();
        // An unliked song leaves the list: stay near where it was.
        if sel >= self.search.items.len() && !self.search.items.is_empty() {
            self.search.list.selected = self.search.items.len() - 1;
        }
    }

    /// Switching to the Liked Songs tab: see if anything changed on Spotify.
    pub(super) fn liked_tab_opened(&mut self) {
        if self.liked.checked.is_none_or(|c| c.elapsed() > RECHECK) {
            self.refresh_liked(false);
        }
    }

    /// Play from your Liked Songs, starting at `t` (picked in the Liked Songs tab).
    pub(super) fn play_liked_from(&mut self, t: &Track) {
        if self.offline.is_some() {
            let tracks = self.liked_tracks();
            let start = tracks.iter().position(|x| x.uri == t.uri).unwrap_or(0);
            self.off_play_list(tracks, start, "Liked Songs".into(), None);
            return;
        }
        let start = self.liked.tracks.iter().position(|x| x.uri == t.uri);
        // In case Spotify refuses the collection as a context.
        let uris: Vec<String> = match start {
            Some(i) => self.liked.tracks[i..].iter().take(100).map(|x| x.uri.clone()).collect(),
            None => vec![t.uri.clone()],
        };
        let context_uri = self.user.as_ref().map(|u| format!("spotify:user:{}:collection", u.id));
        self.play_request(PlayRequest { context_uri, uris: Some(uris), offset_uri: Some(t.uri.clone()), ..Default::default() }, "Liked Songs".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(n: u32) -> Track {
        Track { name: format!("Song {n}"), uri: format!("spotify:track:{n}"), ..Default::default() }
    }

    #[test]
    fn likes_and_unlikes() {
        let mut l = Liked::default();
        l.replace(vec![song(2), song(1), song(2)]);
        l.total = 2;
        assert_eq!(l.tracks.len(), 2);
        assert!(l.set(&song(3), true));
        assert!(!l.set(&song(3), true));
        assert_eq!((l.tracks[0].uri.as_str(), l.total), ("spotify:track:3", 3));
        assert!(l.set(&song(1), false) && !l.contains("spotify:track:1"));
        assert_eq!(l.total, 2);
        assert!(l.dirty && !l.redo);
        // A change while a refresh runs means fetching again.
        l.loading = Some((0, 2));
        l.set(&song(4), true);
        assert!(l.redo);
    }

    #[test]
    fn spots_changes_from_the_first_page() {
        let known = (3, vec!["spotify:track:3".to_string(), "spotify:track:2".into(), "spotify:track:1".into()]);
        assert!(unchanged(&known, 3, &[song(3), song(2), song(1)]));
        // Songs Spotify no longer has leave its first page shorter than our list.
        assert!(unchanged(&known, 3, &[song(3), song(2)]));
        assert!(!unchanged(&known, 3, &[song(3), song(1)]));
        assert!(!unchanged(&known, 4, &[song(4), song(3), song(2), song(1)]));
        assert!(!unchanged(&known, 3, &[song(4), song(3), song(2)]));
        assert!(!unchanged(&known, 2, &[song(3), song(2)]));
    }

    #[test]
    fn saves_and_loads() {
        let dir = std::env::temp_dir().join(format!("spotr-liked-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = cache_file(&dir, "some/user:name");
        assert_eq!(path.file_name().unwrap(), "liked-some_user_name.json");
        assert!(read_cache(&path).is_none());
        write_cache(path.clone(), &Saved { total: 7, tracks: vec![song(1), song(2)] });
        let s = read_cache(&path).unwrap();
        assert_eq!((s.total, s.tracks.len(), s.tracks[1].name.as_str()), (7, 2, "Song 2"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
