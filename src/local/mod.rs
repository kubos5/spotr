//! Local files mode: a folder of music, played without Spotify.
//!
//! * `library` scans the folder and builds tracks, albums and artists.
//! * `store` keeps likes, plays, playlists and the resume point in `.spotr/`.
//! * `engine` decodes and plays the files.

pub mod engine;
pub mod library;
pub mod store;

use crate::spotify::models::*;
use library::{Library, norm};
use store::Store;

/// How well `hay` (already normalized) matches every word of the query.
fn word_score(hay: &str, words: &[String]) -> Option<i64> {
    let mut score = 0;
    for w in words {
        let pos = hay.find(w.as_str())?;
        // Matches at the start of a word count for more.
        let at_word = pos == 0 || !hay.as_bytes()[pos - 1].is_ascii_alphanumeric();
        score += if at_word { 3 } else { 1 };
    }
    Some(score)
}

/// Letters of `needle` appear in order in `hay` ("bhrhpsdy" → "bohemian rhapsody").
fn subsequence(hay: &str, needle: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().filter(|c| !c.is_whitespace()).all(|n| it.any(|h| h == n))
}

/// Search the library like the search bar does online.
pub fn search(lib: &Library, store: &Store, query: &str) -> SearchResults {
    let q = norm(query);
    let words: Vec<String> = q.split_whitespace().map(String::from).collect();
    if words.is_empty() {
        return SearchResults::default();
    }
    let rank_name = |name: &str| -> i64 {
        let n = norm(name);
        if n == q {
            400
        } else if n.starts_with(&q) {
            200
        } else {
            word_score(&n, &words).map(|s| s * 20).unwrap_or(0)
        }
    };

    let mut tracks: Vec<(i64, usize)> = Vec::new();
    for (i, e) in lib.entries.iter().enumerate() {
        let t = &e.track;
        let Some(base) = word_score(&e.hay, &words) else { continue };
        let plays = store.plays(&e.rel).min(50) as i64;
        let liked = if store.is_liked(&e.rel) { 15 } else { 0 };
        tracks.push((base + rank_name(&t.name) + plays * 2 + liked, i));
    }
    // Nothing matched word by word: try a looser match on titles (typos, missing spaces).
    if tracks.is_empty() && q.len() >= 3 {
        for (i, e) in lib.entries.iter().enumerate() {
            if subsequence(&norm(&e.track.name), &q) {
                tracks.push((1, i));
            }
        }
    }
    tracks.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

    let mut artists: Vec<(i64, usize)> = lib
        .artists
        .iter()
        .enumerate()
        .filter_map(|(i, a)| word_score(&norm(&a.artist.name), &words).map(|s| (s + rank_name(&a.artist.name) + a.tracks.len().min(40) as i64, i)))
        .collect();
    artists.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

    let mut albums: Vec<(i64, usize)> = lib
        .albums
        .iter()
        .enumerate()
        .filter_map(|(i, a)| {
            let artist = a.album.artists.first().map(|x| x.name.as_str()).unwrap_or("");
            word_score(&norm(&format!("{} {artist}", a.album.name)), &words).map(|s| (s + rank_name(&a.album.name), i))
        })
        .collect();
    albums.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));

    let mut playlists: Vec<(i64, Playlist)> = store
        .data
        .playlists
        .iter()
        .filter_map(|p| word_score(&norm(&p.name), &words).map(|s| (s + rank_name(&p.name), playlist_model(p))))
        .collect();
    playlists.sort_by_key(|p| std::cmp::Reverse(p.0));

    // The top result is whichever kind matched the name best.
    let mut top: Option<(i64, Item)> = None;
    let mut consider = |score: i64, item: Item| {
        if top.as_ref().is_none_or(|(s, _)| score > *s) {
            top = Some((score, item));
        }
    };
    if let Some(&(_, i)) = artists.first() {
        consider(rank_name(&lib.artists[i].artist.name) + 6, Item::Artist(lib.artists[i].artist.clone()));
    }
    if let Some(&(_, i)) = tracks.first() {
        consider(rank_name(&lib.entries[i].track.name) + 5, Item::Track(lib.entries[i].track.clone()));
    }
    if let Some(&(_, i)) = albums.first() {
        consider(rank_name(&lib.albums[i].album.name) + 2, Item::Album(lib.albums[i].album.clone()));
    }
    if let Some((_, p)) = playlists.first() {
        consider(rank_name(&p.name), Item::Playlist(p.clone()));
    }

    let r = SearchResults {
        tracks: tracks.iter().map(|&(_, i)| lib.entries[i].track.clone()).collect(),
        artists: artists.iter().map(|&(_, i)| lib.artists[i].artist.clone()).collect(),
        albums: albums.iter().map(|&(_, i)| lib.albums[i].album.clone()).collect(),
        playlists: playlists.into_iter().map(|(_, p)| p).collect(),
        totals: [0; 4],
        top: top.map(|(_, i)| i),
    };
    SearchResults { totals: [r.tracks.len() as u32, r.artists.len() as u32, r.albums.len() as u32, r.playlists.len() as u32], ..r }
}

/// The songs in `tracks` matching every word of `query` (in the title, artists or
/// album), best first; ties keep their order. An empty query keeps them all.
pub fn filter_tracks(tracks: &[Track], query: &str) -> Vec<usize> {
    let q = norm(query);
    let words: Vec<String> = q.split_whitespace().map(String::from).collect();
    if words.is_empty() {
        return (0..tracks.len()).collect();
    }
    let mut found: Vec<(i64, usize)> = tracks
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let title = norm(&t.name);
            let base = word_score(&format!("{title} {} {}", norm(&t.artist_names()), norm(t.album_name())), &words)?;
            let bonus = if title == q {
                400
            } else if title.starts_with(&q) {
                200
            } else {
                word_score(&title, &words).map(|s| s * 20).unwrap_or(0)
            };
            Some((base + bonus, i))
        })
        .collect();
    // Nothing matched word by word: try a looser match on titles (typos, missing spaces).
    if found.is_empty() && q.len() >= 3 {
        found = tracks.iter().enumerate().filter(|(_, t)| subsequence(&norm(&t.name), &q)).map(|(i, _)| (1, i)).collect();
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.into_iter().map(|(_, i)| i).collect()
}

/// A local playlist as Spotr's playlist model.
pub fn playlist_model(p: &store::LocalPlaylist) -> Playlist {
    Playlist {
        id: p.id.clone(),
        name: p.name.clone(),
        uri: format!("{}{}", library::PLAYLIST_PREFIX, p.id),
        owner: Owner { id: String::new(), display_name: Some("you".into()) },
        tracks: Some(TracksRef { total: p.tracks.len() as u32 }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::library::*;
    use super::*;

    #[test]
    fn finds_songs_artists_and_typos() {
        let e = |title: &str, artist: &str| IndexEntry {
            size: 1,
            mtime: 1,
            tags: Tags { title: Some(title.into()), artists: vec![artist.into()], album: Some("A Night at the Opera".into()), ..Default::default() },
        };
        let lib = Library::build(
            std::path::PathBuf::from("/m"),
            vec![("1.mp3".into(), e("Bohemian Rhapsody", "Queen")), ("2.mp3".into(), e("Love of My Life", "Queen")), ("3.mp3".into(), e("Café", "Zoë"))],
        );
        let root = std::env::temp_dir().join(format!("spotr-search-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let store = Store::init(&root).unwrap();

        let r = search(&lib, &store, "bohemian");
        assert_eq!(r.tracks[0].name, "Bohemian Rhapsody");
        assert!(matches!(r.top, Some(Item::Track(_))));
        let r = search(&lib, &store, "queen");
        assert_eq!(r.tracks.len(), 2);
        assert!(matches!(r.top, Some(Item::Artist(ref a)) if a.name == "Queen"));
        // Accents don't matter; neither do small typos.
        assert_eq!(search(&lib, &store, "cafe zoe").tracks[0].name, "Café");
        assert_eq!(search(&lib, &store, "bhmnrhpsdy").tracks[0].name, "Bohemian Rhapsody");
        assert_eq!(search(&lib, &store, "opera").albums.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn filters_a_list_of_songs() {
        let t = |name: &str, artist: &str| Track { name: name.into(), artists: vec![SimpleArtist { name: artist.into(), ..Default::default() }], ..Default::default() };
        let tracks = [t("Under Pressure", "Queen"), t("Pressure", "Billy Joel"), t("Somebody to Love", "Queen"), t("Café", "Zoë")];
        assert_eq!(filter_tracks(&tracks, ""), vec![0, 1, 2, 3]);
        // The exact title first, then the rest in their order.
        assert_eq!(filter_tracks(&tracks, "pressure"), vec![1, 0]);
        assert_eq!(filter_tracks(&tracks, "queen"), vec![0, 2]);
        assert_eq!(filter_tracks(&tracks, "queen love"), vec![2]);
        assert_eq!(filter_tracks(&tracks, "cafe"), vec![3]);
        assert_eq!(filter_tracks(&tracks, "smbdy"), vec![2]);
        assert!(filter_tracks(&tracks, "nothing like this").is_empty());
    }
}
