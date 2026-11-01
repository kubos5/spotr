//! What Spotr remembers about a local library, kept in its `.spotr/` folder:
//! likes, play counts, playlists, where you left off, and a scan cache.
//! Music files themselves are never touched.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::library::Index;

pub const DATA_DIR: &str = ".spotr";
const LIBRARY_FILE: &str = "library.json";
const SESSION_FILE: &str = "session.json";
const INDEX_FILE: &str = "index.json";
const README: &str = "\
This folder holds Spotr's data for the music library it's in:
likes, play counts, playlists and where you left off.

Your music files are never modified. Delete this folder to reset
the library (Spotr will offer to set it up again).
";

pub fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Plays {
    pub count: u32,
    /// Unix seconds.
    pub last: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalPlaylist {
    pub id: String,
    pub name: String,
    /// Relative paths of the songs, in order.
    pub tracks: Vec<String>,
    #[serde(default)]
    pub created: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Data {
    pub version: u32,
    /// Relative path → when it was liked (unix seconds).
    pub liked: BTreeMap<String, u64>,
    pub plays: BTreeMap<String, Plays>,
    pub playlists: Vec<LocalPlaylist>,
}

/// Where you left off, saved on quit.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Session {
    pub track: Option<String>,
    pub position_ms: u64,
    pub queue: Vec<String>,
    pub context: Vec<String>,
    pub context_label: String,
    pub context_uri: Option<String>,
    pub shuffle: bool,
    pub repeat: String,
    pub volume: Option<u32>,
}

pub struct Store {
    pub dir: PathBuf,
    pub data: Data,
    save_at: Option<Instant>,
}

pub fn data_dir(root: &Path) -> PathBuf {
    root.join(DATA_DIR)
}

pub fn is_initialized(root: &Path) -> bool {
    data_dir(root).is_dir()
}

fn read_json<T: DeserializeOwned + Default>(path: &Path) -> T {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
            // Keep a damaged file around instead of silently losing it.
            log::warn!("{} is damaged ({e}); starting fresh", path.display());
            let _ = std::fs::rename(path, path.with_extension("json.damaged"));
            T::default()
        }),
        Err(_) => T::default(),
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T, pretty: bool) -> Result<()> {
    let text = if pretty { serde_json::to_string_pretty(value)? } else { serde_json::to_string(value)? };
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("saving {}", path.display()))?;
    Ok(())
}

impl Store {
    /// Create `.spotr/` in `root`.
    pub fn init(root: &Path) -> Result<Store> {
        let dir = data_dir(root);
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let _ = std::fs::write(dir.join("README.txt"), README);
        let store = Store { dir, data: Data { version: 1, ..Default::default() }, save_at: None };
        store.save()?;
        Ok(store)
    }

    pub fn open(root: &Path) -> Result<Store> {
        let dir = data_dir(root);
        if !dir.is_dir() {
            anyhow::bail!("{} isn't a Spotr library yet", root.display());
        }
        let data = read_json(&dir.join(LIBRARY_FILE));
        Ok(Store { dir, data, save_at: None })
    }

    pub fn save(&self) -> Result<()> {
        write_json(&self.dir.join(LIBRARY_FILE), &self.data, true)
    }

    /// Note a change; it's written a moment later (several changes, one write).
    pub fn touch(&mut self) {
        if self.save_at.is_none() {
            self.save_at = Some(Instant::now() + Duration::from_millis(800));
        }
    }

    /// Write pending changes. `force` ignores the delay. Returns an error message.
    pub fn flush(&mut self, force: bool) -> Option<String> {
        match self.save_at {
            Some(at) if force || Instant::now() >= at => {
                self.save_at = None;
                self.save().err().map(|e| format!("{e:#}"))
            }
            _ => None,
        }
    }

    pub fn is_liked(&self, rel: &str) -> bool {
        self.data.liked.contains_key(rel)
    }

    pub fn set_liked(&mut self, rel: &str, liked: bool) {
        if liked {
            self.data.liked.entry(rel.to_string()).or_insert_with(now);
        } else {
            self.data.liked.remove(rel);
        }
        self.touch();
    }

    pub fn record_play(&mut self, rel: &str) {
        let p = self.data.plays.entry(rel.to_string()).or_default();
        p.count += 1;
        p.last = now();
        self.touch();
    }

    pub fn plays(&self, rel: &str) -> u32 {
        self.data.plays.get(rel).map(|p| p.count).unwrap_or(0)
    }

    pub fn playlist(&self, id: &str) -> Option<&LocalPlaylist> {
        self.data.playlists.iter().find(|p| p.id == id)
    }

    pub fn playlist_mut(&mut self, id: &str) -> Option<&mut LocalPlaylist> {
        self.data.playlists.iter_mut().find(|p| p.id == id)
    }

    /// Find a playlist by name: exact (ignoring case) first, then partial.
    pub fn find_playlist(&self, name: &str) -> Option<&LocalPlaylist> {
        let q = name.trim().to_lowercase();
        self.data
            .playlists
            .iter()
            .find(|p| p.name.to_lowercase() == q)
            .or_else(|| self.data.playlists.iter().find(|p| p.name.to_lowercase().contains(&q)))
    }

    pub fn create_playlist(&mut self, name: &str) -> String {
        let base: String = name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase();
        let base = if base.is_empty() { "playlist".to_string() } else { base };
        let mut id = base.clone();
        let mut n = 2;
        while self.playlist(&id).is_some() {
            id = format!("{base}{n}");
            n += 1;
        }
        self.data.playlists.push(LocalPlaylist { id: id.clone(), name: name.trim().to_string(), tracks: Vec::new(), created: now() });
        self.touch();
        id
    }

    pub fn delete_playlist(&mut self, id: &str) -> Option<LocalPlaylist> {
        let i = self.data.playlists.iter().position(|p| p.id == id)?;
        self.touch();
        Some(self.data.playlists.remove(i))
    }

    pub fn load_session(&self) -> Option<Session> {
        let s: Session = read_json(&self.dir.join(SESSION_FILE));
        s.track.is_some().then_some(s)
    }

    pub fn save_session(&self, s: &Session) -> Result<()> {
        write_json(&self.dir.join(SESSION_FILE), s, true)
    }

    pub fn load_index(root: &Path) -> Index {
        read_json(&data_dir(root).join(INDEX_FILE))
    }

    pub fn save_index(root: &Path, index: &Index) -> Result<()> {
        write_json(&data_dir(root).join(INDEX_FILE), index, false)
    }
}

/// Why setting up `dir` as a library deserves a second thought, if it does.
pub fn init_warning(dir: &Path) -> Option<String> {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let home = dirs::home_dir().map(|h| h.canonicalize().unwrap_or(h));
    if dir.parent().is_none() {
        return Some("This is the root of your disk — Spotr would look through every file on it.".into());
    }
    match home {
        Some(h) if h == dir => Some("This is your home folder — Spotr would look through everything in it, which can take a long time.".into()),
        Some(h) if !dir.starts_with(&h) => Some("This folder is outside your home folder.".into()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warnings() {
        assert!(init_warning(Path::new("/")).unwrap().contains("root"));
        if let Some(home) = dirs::home_dir() {
            assert!(init_warning(&home).unwrap().contains("home folder"));
            let inside = home.join("Music");
            if inside.is_dir() {
                assert_eq!(init_warning(&inside), None);
            }
        }
        assert!(init_warning(Path::new("/usr")).unwrap().contains("outside"));
    }

    #[test]
    fn likes_playlists_and_session() {
        let root = std::env::temp_dir().join(format!("spotr-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert!(!is_initialized(&root));
        let mut s = Store::init(&root).unwrap();
        assert!(is_initialized(&root));
        s.set_liked("a.mp3", true);
        s.record_play("a.mp3");
        let id = s.create_playlist("Road trip");
        assert_eq!(s.create_playlist("Road trip"), "roadtrip2");
        s.playlist_mut(&id).unwrap().tracks.push("a.mp3".into());
        assert!(s.flush(true).is_none());
        s.save_session(&Session { track: Some("a.mp3".into()), position_ms: 1234, ..Default::default() }).unwrap();

        let s = Store::open(&root).unwrap();
        assert!(s.is_liked("a.mp3"));
        assert_eq!(s.plays("a.mp3"), 1);
        assert_eq!(s.find_playlist("road").unwrap().tracks, ["a.mp3"]);
        assert_eq!(s.load_session().unwrap().position_ms, 1234);
        let _ = std::fs::remove_dir_all(&root);
    }
}
