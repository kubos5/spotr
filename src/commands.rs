//! The command language used by the command bar.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Args {
    None,
    /// Free text (a search query, a number…).
    Free,
    Choices(&'static [&'static str]),
    Devices,
    Playlists,
    /// Title fonts (built-in and user-provided).
    Fonts,
}

pub struct Spec {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub usage: &'static str,
    pub desc: &'static str,
    pub args: Args,
    pub group: &'static str,
}

const ON_OFF: &[&str] = &["on", "off", "toggle"];

pub const COMMANDS: &[Spec] = &[
    // Playback
    Spec { name: "play", aliases: &["resume"], usage: "play [query | spotify link]", desc: "Resume, or search and play a song", args: Args::Free, group: "Playback" },
    Spec { name: "pause", aliases: &[], usage: "pause", desc: "Pause playback", args: Args::None, group: "Playback" },
    Spec { name: "toggle", aliases: &["pp"], usage: "toggle", desc: "Toggle play / pause", args: Args::None, group: "Playback" },
    Spec { name: "next", aliases: &["n"], usage: "next", desc: "Skip to the next song", args: Args::None, group: "Playback" },
    Spec { name: "previous", aliases: &["prev", "back"], usage: "previous", desc: "Go to the previous song", args: Args::None, group: "Playback" },
    Spec { name: "skip", aliases: &["ff", "forward"], usage: "skip <seconds>", desc: "Skip forward by some seconds", args: Args::Free, group: "Playback" },
    Spec { name: "rewind", aliases: &["rw"], usage: "rewind <seconds>", desc: "Rewind by some seconds", args: Args::Free, group: "Playback" },
    Spec { name: "seek", aliases: &["goto"], usage: "seek <1:23 | 83 | 50%>", desc: "Jump to a position in the song", args: Args::Free, group: "Playback" },
    Spec { name: "restart", aliases: &["replay"], usage: "restart", desc: "Play the current song from the start", args: Args::None, group: "Playback" },
    Spec { name: "speed", aliases: &["rate"], usage: "speed <multiplier>", desc: "Playback speed, e.g. speed 1.5 (built-in player)", args: Args::Choices(&["0.5", "0.75", "1", "1.25", "1.5", "2"]), group: "Playback" },
    Spec { name: "volume", aliases: &["vol", "v"], usage: "volume <0-100 | +n | -n>", desc: "Set or change the volume", args: Args::Free, group: "Playback" },
    Spec { name: "mute", aliases: &[], usage: "mute", desc: "Mute (run again to unmute)", args: Args::None, group: "Playback" },
    Spec { name: "unmute", aliases: &[], usage: "unmute", desc: "Restore the volume", args: Args::None, group: "Playback" },
    Spec { name: "shuffle", aliases: &[], usage: "shuffle [on | off]", desc: "Toggle shuffle", args: Args::Choices(ON_OFF), group: "Playback" },
    Spec { name: "loop", aliases: &["repeat"], usage: "loop [off | context | track]", desc: "Toggle or set repeat mode", args: Args::Choices(&["off", "context", "track"]), group: "Playback" },
    Spec { name: "sleep", aliases: &["timer"], usage: "sleep <minutes | off>", desc: "Pause playback after a while", args: Args::Free, group: "Playback" },
    // Library
    Spec { name: "like", aliases: &["love", "save"], usage: "like", desc: "Save the current (or selected) song", args: Args::None, group: "Library" },
    Spec { name: "unlike", aliases: &["unsave"], usage: "unlike", desc: "Remove the song from Liked Songs", args: Args::None, group: "Library" },
    Spec { name: "library", aliases: &["lib"], usage: "library", desc: "Browse your playlists, albums and artists", args: Args::None, group: "Library" },
    Spec { name: "liked", aliases: &["favorites"], usage: "liked", desc: "Open your Liked Songs", args: Args::None, group: "Library" },
    Spec { name: "playlists", aliases: &[], usage: "playlists", desc: "Your playlists", args: Args::None, group: "Library" },
    Spec { name: "recent", aliases: &["history"], usage: "recent", desc: "Recently played songs", args: Args::None, group: "Library" },
    Spec { name: "top", aliases: &[], usage: "top", desc: "Your top songs lately", args: Args::None, group: "Library" },
    Spec { name: "album", aliases: &[], usage: "album", desc: "Open the current song's album", args: Args::None, group: "Library" },
    Spec { name: "artist", aliases: &[], usage: "artist", desc: "Open the current song's artist", args: Args::None, group: "Library" },
    Spec { name: "follow", aliases: &[], usage: "follow", desc: "Follow the current (or selected) artist", args: Args::None, group: "Library" },
    Spec { name: "unfollow", aliases: &[], usage: "unfollow", desc: "Unfollow the artist", args: Args::None, group: "Library" },
    Spec { name: "savealbum", aliases: &[], usage: "savealbum", desc: "Save the current album to your library", args: Args::None, group: "Library" },
    Spec { name: "addto", aliases: &["add-to-playlist"], usage: "addto <playlist>", desc: "Add the current song to a playlist", args: Args::Playlists, group: "Library" },
    // Queue
    Spec { name: "queue", aliases: &["q"], usage: "queue [add <query> | remove <n> | move <from> <to> | clear]", desc: "Show or edit the queue", args: Args::Choices(&["add", "remove", "move", "clear"]), group: "Queue" },
    // Views
    Spec { name: "search", aliases: &["find", "s"], usage: "search [query]", desc: "Focus the search bar", args: Args::Free, group: "Views" },
    Spec { name: "lyrics", aliases: &["ly"], usage: "lyrics [ascii | text]", desc: "Show synced lyrics (optionally switch style)", args: Args::Choices(&["ascii", "text"]), group: "Views" },
    Spec { name: "devices", aliases: &["device", "output"], usage: "devices [name]", desc: "Choose where music plays", args: Args::Devices, group: "Views" },
    Spec { name: "home", aliases: &["main", "nowplaying"], usage: "home", desc: "Back to the now playing view", args: Args::None, group: "Views" },
    Spec { name: "settings", aliases: &["config", "prefs"], usage: "settings", desc: "Open the settings", args: Args::None, group: "Views" },
    Spec { name: "help", aliases: &["keys", "h"], usage: "help [topic]", desc: "Keyboard shortcuts and commands", args: Args::Free, group: "Views" },
    Spec { name: "about", aliases: &["version"], usage: "about", desc: "About Spotr", args: Args::None, group: "Views" },
    // Appearance
    Spec { name: "cover", aliases: &["art"], usage: "cover [image | text | toggle]", desc: "Cover art rendering mode", args: Args::Choices(&["image", "text", "toggle", "blocks", "ascii", "braille", "off", "on"]), group: "Appearance" },
    Spec { name: "pixel", aliases: &["pixelate"], usage: "pixel <0 | 8 | 16 | 32 | 64>", desc: "Pixelation of text cover art (0 = sharp)", args: Args::Choices(&["0", "8", "12", "16", "24", "32", "64"]), group: "Appearance" },
    Spec { name: "visualizer", aliases: &["viz", "vis"], usage: "visualizer [on | off | bars | mirror | wave | blocks | dots]", desc: "Toggle or restyle the visualizer", args: Args::Choices(&["on", "off", "bars", "mirror", "wave", "blocks", "dots"]), group: "Appearance" },
    Spec { name: "font", aliases: &[], usage: "font [name]", desc: "ASCII font for titles (ansi-compact, future, block…)", args: Args::Fonts, group: "Appearance" },
    Spec { name: "accent", aliases: &["color", "theme"], usage: "accent <color>", desc: "Accent color (from your terminal palette)", args: Args::Choices(&["green", "cyan", "blue", "magenta", "red", "yellow", "white"]), group: "Appearance" },
    Spec { name: "mode", aliases: &["arrows"], usage: "mode [arrows | brackets] <seek | track | speed | volume>", desc: "What ← / → (or [ / ]) do", args: Args::Choices(&["seek", "track", "speed", "volume", "brackets", "arrows"]), group: "Appearance" },
    Spec { name: "animations", aliases: &["anim"], usage: "animations [on | off]", desc: "Toggle UI animations", args: Args::Choices(ON_OFF), group: "Appearance" },
    // Misc
    Spec { name: "share", aliases: &["link", "copy"], usage: "share", desc: "Copy a link to the song (OSC 52 clipboard)", args: Args::None, group: "Other" },
    Spec { name: "open", aliases: &["web"], usage: "open", desc: "Open the song in Spotify / browser", args: Args::None, group: "Other" },
    Spec { name: "refresh", aliases: &["sync"], usage: "refresh", desc: "Re-sync playback state", args: Args::None, group: "Other" },
    Spec { name: "player", aliases: &[], usage: "player [login | restart | off]", desc: "Manage the built-in player", args: Args::Choices(&["login", "restart", "off", "on"]), group: "Other" },
    Spec { name: "login", aliases: &["auth"], usage: "login", desc: "Sign in to Spotify again", args: Args::None, group: "Other" },
    Spec { name: "logout", aliases: &[], usage: "logout", desc: "Forget the saved Spotify login", args: Args::None, group: "Other" },
    Spec { name: "whoami", aliases: &["me", "profile"], usage: "whoami", desc: "Show the signed-in account", args: Args::None, group: "Other" },
    Spec { name: "onboarding", aliases: &["setup", "welcome"], usage: "onboarding", desc: "Run the first-time setup again", args: Args::None, group: "Other" },
    Spec { name: "quit", aliases: &["exit", ":q"], usage: "quit", desc: "Quit Spotr", args: Args::None, group: "Other" },
];

pub fn find(name: &str) -> Option<&'static Spec> {
    let name = name.to_lowercase();
    COMMANDS
        .iter()
        .find(|c| c.name == name || c.aliases.contains(&name.as_str()))
}

/// A completion candidate. `replace` is the full new command-bar text.
#[derive(Clone, Debug)]
pub struct Suggestion {
    pub replace: String,
    pub label: String,
    pub desc: String,
}

/// Split into (command, rest) — the rest keeps its spacing.
pub fn split(input: &str) -> (&str, &str) {
    let input = input.trim_start();
    match input.find(char::is_whitespace) {
        Some(i) => (&input[..i], input[i..].trim_start()),
        None => (input, ""),
    }
}

pub fn suggest(input: &str, devices: &[String], playlists: &[String]) -> Vec<Suggestion> {
    let (cmd, rest) = split(input);
    let has_space = input.trim_start().contains(char::is_whitespace);
    if !has_space {
        let q = cmd.to_lowercase();
        let mut out: Vec<(u8, Suggestion)> = COMMANDS
            .iter()
            .filter_map(|c| {
                let score = if c.name.starts_with(&q) {
                    0
                } else if c.aliases.iter().any(|a| a.starts_with(&q)) {
                    1
                } else if !q.is_empty() && c.name.contains(&q) {
                    2
                } else if !q.is_empty() && fuzzy(c.name, &q) {
                    3
                } else {
                    return None;
                };
                Some((
                    score,
                    Suggestion {
                        replace: format!("{} ", c.name),
                        label: c.usage.to_string(),
                        desc: c.desc.to_string(),
                    },
                ))
            })
            .collect();
        out.sort_by_key(|(s, _)| *s);
        return out.into_iter().map(|(_, s)| s).collect();
    }
    let Some(spec) = find(cmd) else { return Vec::new() };
    let choices: Vec<String> = match spec.args {
        Args::Choices(c) => c.iter().map(|s| s.to_string()).collect(),
        Args::Devices => devices.to_vec(),
        Args::Playlists => playlists.to_vec(),
        Args::Fonts => crate::font::all().map(|(id, _, _)| id.to_string()).collect(),
        _ => Vec::new(),
    };
    let q = rest.to_lowercase();
    let mut matched: Vec<(bool, String)> = choices
        .into_iter()
        .filter(|c| c.to_lowercase() != q)
        .filter_map(|c| {
            let l = c.to_lowercase();
            if l.starts_with(&q) {
                Some((false, c))
            } else if !q.is_empty() && l.contains(&q) {
                Some((true, c))
            } else {
                None
            }
        })
        .collect();
    // Prefix matches first; the sort is stable so the original order is kept otherwise.
    matched.sort_by_key(|(contains, _)| *contains);
    matched
        .into_iter()
        .map(|(_, c)| Suggestion { replace: format!("{} {}", spec.name, c), label: c.clone(), desc: String::new() })
        .collect()
}

fn fuzzy(hay: &str, needle: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|n| it.by_ref().any(|h| h == n))
}

/// Parse a time like `1:23`, `83`, `1m20s` into milliseconds.
pub fn parse_time(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some((m, sec)) = s.split_once(':') {
        let m: u64 = m.parse().ok()?;
        let sec: f64 = sec.parse().ok()?;
        return Some(m * 60_000 + (sec * 1000.0) as u64);
    }
    if let Some(mins) = s.strip_suffix('m') {
        return mins.parse::<f64>().ok().map(|m| (m * 60_000.0) as u64);
    }
    if s.contains('m') && s.ends_with('s') {
        let (m, rest) = s.split_once('m')?;
        let sec = rest.trim_end_matches('s');
        return Some(m.parse::<u64>().ok()? * 60_000 + sec.parse::<u64>().ok()? * 1000);
    }
    let sec = s.trim_end_matches('s').parse::<f64>().ok()?;
    if sec < 0.0 {
        return None;
    }
    Some((sec * 1000.0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(parse_time("1:23"), Some(83_000));
        assert_eq!(parse_time("10"), Some(10_000));
        assert_eq!(parse_time("1m20s"), Some(80_000));
        assert_eq!(parse_time("2m"), Some(120_000));
        assert_eq!(parse_time("x"), None);
    }

    #[test]
    fn suggestions() {
        let s = suggest("vo", &[], &[]);
        assert_eq!(s[0].replace, "volume ");
        let s = suggest("loop t", &[], &[]);
        assert_eq!(s[0].replace, "loop track");
        assert!(find("prev").is_some());
    }

    #[test]
    fn no_duplicate_names() {
        let mut seen = std::collections::HashSet::new();
        for c in COMMANDS {
            assert!(seen.insert(c.name), "{}", c.name);
            for a in c.aliases {
                assert!(seen.insert(a), "{a}");
            }
        }
    }
}
