//! Executing command-bar commands.

use std::time::{Duration, Instant};

use super::*;
use crate::commands::{self, parse_time};
use crate::config::{AccentColor, ArrowMode, CoverMode, TextArt, VisStyle};
use crate::spotify::PlayRequest;

impl App {
    /// Run a command line. Feedback goes to the command bar message + toasts.
    pub fn execute(&mut self, line: &str) {
        let (cmd, rest) = commands::split(line);
        let rest = rest.trim().to_string();
        let result = match commands::find(cmd) {
            Some(spec) => self.run_command(spec.name, &rest),
            None => {
                // Bare Spotify links play directly.
                if normalize_uri(line).is_some() {
                    self.run_command("play", line)
                } else {
                    let sugg = commands::suggest(cmd, &[], &[]);
                    match sugg.first() {
                        Some(s) => Err(format!("Unknown command “{cmd}” — did you mean `{}`?", s.replace.trim())),
                        None => Err(format!("Unknown command “{cmd}” — type `help` for a list")),
                    }
                }
            }
        };
        match result {
            Ok(Some(msg)) => {
                self.command.flash = Some((Instant::now(), true));
                self.command.message = Some((msg, true, Instant::now()));
            }
            Ok(None) => {
                self.command.flash = Some((Instant::now(), true));
            }
            Err(e) => {
                self.command.flash = Some((Instant::now(), false));
                self.command.message = Some((e.clone(), false, Instant::now()));
                self.toast(ToastKind::Error, e);
            }
        }
    }

    fn run_command(&mut self, name: &str, arg: &str) -> Result<Option<String>, String> {
        let num = |s: &str| s.trim().trim_end_matches(['s', 'x', '×', '%']).parse::<f32>().ok();
        match name {
            "play" => {
                if arg.is_empty() {
                    if !self.pb.is_playing {
                        self.resume();
                    }
                    return Ok(None);
                }
                if let Some(uri) = normalize_uri(arg) {
                    return self.play_uri(&uri).map(|_| None);
                }
                let q = arg.to_string();
                let dev = self.fallback_device();
                let tx = self.tx.clone();
                let sp = self.spotify.clone();
                self.osd("⌕", format!("Searching “{q}”…"), None);
                tokio::spawn(async move {
                    let r = async {
                        let res = sp.search(&q, &["track"], 1, 0).await?;
                        let t = res.tracks.into_iter().next().ok_or_else(|| anyhow::anyhow!("No song found for “{q}”"))?;
                        sp.play(dev.as_deref(), Some(PlayRequest { uris: Some(vec![t.uri.clone()]), ..Default::default() }))
                            .await?;
                        Ok::<_, anyhow::Error>(t)
                    }
                    .await;
                    let ev = match r {
                        Ok(t) => AppEvent::Done { ok: Some(format!("Playing “{}” by {}", t.name, t.artist_names())), err: None, refresh: true },
                        Err(e) => AppEvent::Done { ok: None, err: Some(e.to_string()), refresh: true },
                    };
                    let _ = tx.send(ev);
                });
                Ok(None)
            }
            "pause" => {
                self.pause();
                Ok(None)
            }
            "toggle" => {
                self.toggle_play();
                Ok(None)
            }
            "next" => {
                self.next();
                Ok(None)
            }
            "previous" => {
                self.previous();
                Ok(None)
            }
            "skip" | "rewind" => {
                let secs = if arg.is_empty() {
                    self.cfg.playback.seek_step as f32
                } else {
                    parse_time(arg).map(|ms| ms as f32 / 1000.0).ok_or_else(|| format!("`{name}` needs a number of seconds, e.g. `{name} 10`"))?
                };
                let ms = (secs * 1000.0) as i64;
                self.seek_by(if name == "skip" { ms } else { -ms });
                Ok(None)
            }
            "seek" => {
                let dur = self.pb.duration();
                let ms = if let Some(p) = arg.strip_suffix('%') {
                    let p: f32 = p.trim().parse().map_err(|_| "Bad percentage".to_string())?;
                    (dur as f32 * p.clamp(0.0, 100.0) / 100.0) as u64
                } else {
                    parse_time(arg).ok_or("Usage: seek 1:23 | seek 83 | seek 50%")?
                };
                self.seek_to(ms);
                Ok(Some(format!("Jumped to {}", fmt_duration(ms))))
            }
            "restart" => {
                self.seek_to(0);
                Ok(None)
            }
            "speed" => {
                let s = if arg.is_empty() { 1.0 } else { num(arg).ok_or("Usage: speed <multiplier>, e.g. speed 1.5")? };
                if !(0.25..=3.0).contains(&s) {
                    return Err("Speed must be between 0.25 and 3".into());
                }
                self.set_speed(s);
                Ok(Some(format!("Speed {}×", self.speed)))
            }
            "volume" => {
                if arg.is_empty() {
                    return Ok(Some(format!("Volume is {}%", self.pb.volume.unwrap_or(0))));
                }
                let a = arg.trim();
                if let Some(d) = a.strip_prefix('+') {
                    self.change_volume(num(d).ok_or("Usage: volume +10")? as i32);
                } else if a.starts_with('-') && a.len() > 1 {
                    self.change_volume(-(num(&a[1..]).ok_or("Usage: volume -10")? as i32));
                } else {
                    let v = num(a).ok_or("Usage: volume <0-100>")?;
                    self.muted_volume = None;
                    self.set_volume(v.clamp(0.0, 100.0) as u32);
                }
                Ok(None)
            }
            "mute" => {
                self.toggle_mute();
                Ok(None)
            }
            "unmute" => {
                if let Some(v) = self.muted_volume.take() {
                    self.set_volume(v);
                }
                Ok(None)
            }
            "shuffle" => {
                let on = match arg {
                    "on" | "true" | "1" => true,
                    "off" | "false" | "0" => false,
                    _ => !self.pb.shuffle,
                };
                self.set_shuffle(on);
                Ok(None)
            }
            "loop" => {
                match arg {
                    "" | "toggle" => self.cycle_repeat(),
                    "off" | "none" => self.set_repeat("off"),
                    "on" | "context" | "all" | "playlist" | "album" => self.set_repeat("context"),
                    "track" | "song" | "one" => self.set_repeat("track"),
                    _ => return Err("Usage: loop [off | context | track]".into()),
                }
                Ok(None)
            }
            "sleep" => {
                if arg.is_empty() {
                    return Ok(Some(match self.sleep_at {
                        Some(t) => format!("Pausing in {} min", t.saturating_duration_since(Instant::now()).as_secs().div_ceil(60)),
                        None => "No sleep timer set".into(),
                    }));
                }
                if arg == "off" || arg == "cancel" {
                    self.sleep_at = None;
                    return Ok(Some("Sleep timer cancelled".into()));
                }
                // The number is read in minutes: `sleep 30` is half an hour, `sleep 1:30` is 1h30.
                let mins = parse_time(arg).map(|ms| ms as f32 / 1000.0).ok_or("Usage: sleep <minutes>")?;
                let secs = mins * 60.0;
                self.sleep_at = Some(Instant::now() + Duration::from_secs_f32(secs.max(1.0)));
                Ok(Some(format!("Sleep timer: pausing in {} min ☾", (secs / 60.0).round())))
            }
            "like" | "unlike" => {
                let like = name == "like";
                let selected = if self.search_open {
                    match self.selected_item() {
                        Some(Item::Track(t)) => Some(t.clone()),
                        _ => None,
                    }
                } else {
                    None
                };
                self.set_liked(selected, like);
                Ok(None)
            }
            "library" => {
                self.open_search(false);
                self.open_library(false);
                Ok(None)
            }
            "playlists" => {
                self.open_search(false);
                self.open_library(true);
                Ok(None)
            }
            "liked" => {
                self.open_search(false);
                self.open_liked();
                Ok(None)
            }
            "recent" => {
                self.open_search(false);
                self.open_recent();
                Ok(None)
            }
            "top" => {
                self.open_search(false);
                self.open_top();
                Ok(None)
            }
            "album" => {
                let a = self.pb.track.as_ref().and_then(|t| t.album.clone()).filter(|a| !a.uri.is_empty()).ok_or("Nothing playing")?;
                self.open_search(false);
                self.open_album(&a);
                Ok(None)
            }
            "artist" => {
                let a = self
                    .pb
                    .track
                    .as_ref()
                    .and_then(|t| t.first_artist().cloned())
                    .filter(|a| !a.uri.is_empty())
                    .ok_or("Nothing playing")?;
                self.open_search(false);
                let id = a.id.clone().unwrap_or_else(|| uri_id(&a.uri).to_string());
                self.open_artist_id(&id, &a.name, None);
                Ok(None)
            }
            "follow" | "unfollow" => {
                let follow = name == "follow";
                let (id, label) = match self.selected_item() {
                    Some(Item::Artist(a)) if self.search_open => (a.id.clone().unwrap_or_default(), a.name.clone()),
                    _ => {
                        let a = self.pb.track.as_ref().and_then(|t| t.first_artist().cloned()).ok_or("No artist to follow")?;
                        (a.id.clone().unwrap_or_else(|| uri_id(&a.uri).to_string()), a.name)
                    }
                };
                let msg = if follow { format!("Following {label}") } else { format!("Unfollowed {label}") };
                self.run(Some(msg), move |sp| async move { sp.set_following(&id, follow).await });
                Ok(None)
            }
            "savealbum" => {
                let a = match self.selected_item() {
                    Some(Item::Album(a)) if self.search_open => a.clone(),
                    _ => self.pb.track.as_ref().and_then(|t| t.album.clone()).ok_or("No album")?,
                };
                let id = a.id.clone().unwrap_or_else(|| uri_id(&a.uri).to_string());
                self.run(Some(format!("Saved “{}” to your library", a.name)), move |sp| async move {
                    sp.set_album_saved(&id, true).await
                });
                Ok(None)
            }
            "addto" => {
                if arg.is_empty() {
                    return Err("Usage: addto <playlist name>".into());
                }
                let track = match self.selected_item() {
                    Some(Item::Track(t)) if self.search_open => t.clone(),
                    _ => self.pb.track.clone().ok_or("Nothing playing")?,
                };
                let q = arg.to_lowercase();
                let pl = self
                    .playlists
                    .iter()
                    .find(|p| p.name.to_lowercase() == q)
                    .or_else(|| self.playlists.iter().find(|p| p.name.to_lowercase().contains(&q)))
                    .cloned()
                    .ok_or_else(|| format!("No playlist matching “{arg}”"))?;
                let uri = track.uri.clone();
                self.run(Some(format!("Added “{}” to {}", track.name, pl.name)), move |sp| async move {
                    sp.add_to_playlist(&pl.id, &[uri]).await
                });
                Ok(None)
            }
            "queue" => self.queue_command(arg),
            "search" => {
                self.open_search(true);
                if !arg.is_empty() {
                    self.search.input.set(arg);
                    self.search.stack.clear();
                    self.search.pending = None;
                    self.run_search(false);
                    self.focus = Focus::SearchList;
                }
                Ok(None)
            }
            "lyrics" => {
                self.set_view(MainView::Lyrics);
                match arg {
                    "" => Ok(None),
                    "ascii" | "art" | "big" => {
                        self.cfg.lyrics.ascii = true;
                        self.save_config();
                        Ok(Some("Lyrics: ASCII art".into()))
                    }
                    "text" | "plain" => {
                        self.cfg.lyrics.ascii = false;
                        self.save_config();
                        Ok(Some("Lyrics: text".into()))
                    }
                    _ => Err("Usage: lyrics [ascii | text]".into()),
                }
            }
            "devices" => {
                if arg.is_empty() {
                    self.set_overlay(Overlay::Devices);
                    return Ok(None);
                }
                let q = arg.to_lowercase();
                let i = self
                    .devices
                    .list
                    .iter()
                    .position(|d| d.name.to_lowercase() == q)
                    .or_else(|| self.devices.list.iter().position(|d| d.name.to_lowercase().contains(&q)));
                match i {
                    Some(i) => {
                        self.select_device(i);
                        Ok(None)
                    }
                    None => {
                        self.set_overlay(Overlay::Devices);
                        Err(format!("No device matching “{arg}”"))
                    }
                }
            }
            "home" => {
                self.set_view(MainView::NowPlaying);
                Ok(None)
            }
            "settings" => {
                self.set_overlay(Overlay::Settings);
                Ok(None)
            }
            "help" => {
                self.help.filter = arg.to_lowercase();
                self.help.scroll = 0;
                self.set_overlay(Overlay::Help);
                Ok(None)
            }
            "about" => {
                self.set_overlay(Overlay::About);
                Ok(None)
            }
            "cover" => {
                let a = &mut self.cfg.appearance;
                match arg {
                    "" | "toggle" => a.cover_mode = a.cover_mode.cycle(1),
                    "image" | "img" => a.cover_mode = CoverMode::Image,
                    "text" => a.cover_mode = CoverMode::Text,
                    "off" => a.show_cover = false,
                    "on" => a.show_cover = true,
                    other => match TextArt::from_id(other) {
                        Some(t) => {
                            a.cover_mode = CoverMode::Text;
                            a.text_art = t;
                        }
                        None => return Err("Usage: cover [image | text | blocks | ascii | braille | on | off]".into()),
                    },
                }
                let msg = format!(
                    "Cover art: {}{}",
                    self.cfg.appearance.cover_mode.label(),
                    if self.cfg.appearance.cover_mode == CoverMode::Text { format!(" ({})", self.cfg.appearance.text_art.label()) } else { String::new() }
                );
                if self.cfg.appearance.cover_mode == CoverMode::Image && !self.images.graphics {
                    self.toast(ToastKind::Info, "No terminal graphics detected — text art will be used instead");
                }
                self.save_config();
                Ok(Some(msg))
            }
            "pixel" => {
                let p: u32 = arg.trim().parse().map_err(|_| "Usage: pixel <0 | 8 | 16 | 32 | 64>".to_string())?;
                self.cfg.appearance.pixelation = p.min(128);
                if self.cfg.appearance.cover_mode == CoverMode::Image {
                    self.cfg.appearance.cover_mode = CoverMode::Text;
                }
                self.save_config();
                Ok(Some(if p == 0 { "Pixelation off".into() } else { format!("Pixelated to {p}×{p}") }))
            }
            "visualizer" => {
                match arg {
                    "" | "toggle" => self.cfg.visualizer.enabled = !self.cfg.visualizer.enabled,
                    "on" => self.cfg.visualizer.enabled = true,
                    "off" => self.cfg.visualizer.enabled = false,
                    other => {
                        let s = VisStyle::from_id(other).ok_or("Styles: bars, mirror, wave, blocks, dots")?;
                        self.cfg.visualizer.style = s;
                        self.cfg.visualizer.enabled = true;
                    }
                }
                self.save_config();
                Ok(Some(if self.cfg.visualizer.enabled {
                    format!("Visualizer: {}", self.cfg.visualizer.style.label())
                } else {
                    "Visualizer hidden".into()
                }))
            }
            "font" => {
                let id = if arg.is_empty() {
                    crate::font::cycle(self.title_font(), 1)
                } else {
                    crate::font::find(arg).ok_or_else(|| {
                        let names: Vec<&str> = crate::font::all().map(|(id, _, _)| id).collect();
                        format!("Fonts: {}", names.join(", "))
                    })?
                };
                self.cfg.appearance.title_font = id.to_string();
                self.save_config();
                Ok(Some(format!("Title font: {}", crate::font::label(id))))
            }
            "accent" => {
                self.cfg.appearance.accent = AccentColor::from_id(arg).ok_or("Colors: green, cyan, blue, magenta, red, yellow, white")?;
                self.save_config();
                Ok(Some(format!("Accent: {}", self.cfg.appearance.accent.label())))
            }
            "mode" => {
                // `mode [arrows|brackets] [seek|track|speed|volume]`
                let (pair, value) = match commands::split(arg) {
                    ("brackets" | "bracket" | "[]", v) => (true, v),
                    ("arrows" | "arrow", v) => (false, v),
                    (_, _) => (false, arg),
                };
                let slot = if pair { &mut self.cfg.playback.bracket_mode } else { &mut self.cfg.playback.arrow_mode };
                *slot = if value.is_empty() {
                    slot.cycle(1)
                } else {
                    ArrowMode::from_id(value).ok_or("Usage: mode [arrows | brackets] <seek | track | speed | volume>")?
                };
                let label = slot.label();
                self.save_config();
                Ok(Some(format!("{} keys: {label}", if pair { "[ ]" } else { "← →" })))
            }
            "animations" => {
                self.cfg.animations.enabled = match arg {
                    "on" => true,
                    "off" => false,
                    _ => !self.cfg.animations.enabled,
                };
                self.save_config();
                Ok(Some(format!("Animations {}", if self.cfg.animations.enabled { "on" } else { "off" })))
            }
            "share" => {
                let t = self.pb.track.as_ref().ok_or("Nothing playing")?;
                let url = share_url(&t.uri).ok_or("This item has no public link")?;
                osc52_copy(&url);
                Ok(Some(format!("Copied {url}")))
            }
            "open" => {
                let t = self.pb.track.as_ref().ok_or("Nothing playing")?;
                let url = share_url(&t.uri).ok_or("This item has no public link")?;
                open::that_detached(&url).map_err(|e| e.to_string())?;
                Ok(Some("Opened in your browser".into()))
            }
            "refresh" => {
                self.pb.hold_until = Instant::now();
                self.poll_now();
                self.fetch_devices();
                Ok(Some("Refreshed".into()))
            }
            "player" => {
                if !crate::player::AVAILABLE {
                    return Err("Spotr was built without the built-in player".into());
                }
                match arg {
                    "login" => {
                        self.start_local_player(true);
                        Ok(Some("Opening your browser to sign in the built-in player…".into()))
                    }
                    "" | "restart" | "on" => {
                        self.cfg.general.local_player = true;
                        self.save_config();
                        self.local_status = LocalStatus::Off;
                        self.start_local_player(false);
                        Ok(Some("Starting the built-in player…".into()))
                    }
                    "off" => {
                        self.stop_local_player();
                        self.cfg.general.local_player = false;
                        self.save_config();
                        Ok(Some("Built-in player stopped".into()))
                    }
                    _ => Err("Usage: player [login | restart | off]".into()),
                }
            }
            "login" => {
                self.onboarding = Some(Onboarding::at_login(&self.cfg));
                self.needs_clear = true;
                Ok(None)
            }
            "logout" => {
                let sp = self.spotify.clone();
                tokio::spawn(async move { sp.logout().await });
                self.user = None;
                Ok(Some("Signed out. Run `login` to sign in again.".into()))
            }
            "whoami" => {
                let u = self.user.as_ref().ok_or("Not signed in")?;
                Ok(Some(format!(
                    "Signed in as {} ({}){}",
                    u.name(),
                    u.product.as_deref().unwrap_or("unknown plan"),
                    u.country.as_deref().map(|c| format!(" · {c}")).unwrap_or_default()
                )))
            }
            "onboarding" => {
                self.onboarding = Some(Onboarding::new(&self.cfg, true));
                self.needs_clear = true;
                Ok(None)
            }
            "quit" => {
                self.quit();
                Ok(None)
            }
            _ => Err(format!("`{name}` isn't implemented")),
        }
    }

    fn play_uri(&mut self, uri: &str) -> Result<(), String> {
        let kind = uri.split(':').nth(1).unwrap_or("");
        match kind {
            "track" | "episode" => {
                self.play_request(PlayRequest { uris: Some(vec![uri.to_string()]), ..Default::default() }, "Playing link".into());
            }
            "album" | "playlist" | "artist" | "show" => {
                self.play_context(uri, None, "Playing link".into());
            }
            _ => return Err(format!("Can't play “{uri}”")),
        }
        Ok(())
    }

    fn queue_command(&mut self, arg: &str) -> Result<Option<String>, String> {
        let (sub, rest) = commands::split(arg);
        match sub {
            "" | "show" => {
                self.set_view(MainView::Queue);
                Ok(None)
            }
            "add" => {
                if rest.is_empty() {
                    let t = match self.selected_item() {
                        Some(Item::Track(t)) if self.search_open => t.clone(),
                        _ => self.pb.track.clone().ok_or("Nothing to add — `queue add <song>`")?,
                    };
                    self.enqueue(t);
                    return Ok(None);
                }
                let q = rest.to_string();
                let tx = self.tx.clone();
                let sp = self.spotify.clone();
                tokio::spawn(async move {
                    let r = sp.search(&q, &["track"], 1, 0).await;
                    let ev = match r.map(|r| r.tracks.into_iter().next()) {
                        Ok(Some(t)) => AppEvent::Enqueue(t),
                        Ok(None) => AppEvent::Toast(ToastKind::Warn, format!("No song found for “{q}”")),
                        Err(e) => AppEvent::Toast(ToastKind::Error, e.to_string()),
                    };
                    let _ = tx.send(ev);
                });
                Ok(Some(format!("Looking up “{rest}”…")))
            }
            "remove" | "rm" | "del" => {
                let n: usize = rest.trim().parse().map_err(|_| "Usage: queue remove <position>".to_string())?;
                if n == 0 || n > self.queue.items.len() {
                    return Err(format!("The queue has {} songs", self.queue.items.len()));
                }
                self.queue_remove(n - 1);
                Ok(None)
            }
            "move" | "mv" => {
                let mut it = rest.split_whitespace().filter_map(|s| s.parse::<usize>().ok());
                let (Some(a), Some(b)) = (it.next(), it.next()) else {
                    return Err("Usage: queue move <from> <to>".into());
                };
                let n = self.queue.items.len();
                if a == 0 || b == 0 || a > n || b > n {
                    return Err(format!("Positions must be between 1 and {n}"));
                }
                let t = self.queue.items.remove(a - 1);
                self.queue.items.insert(b - 1, t);
                self.queue.list.selected = b - 1;
                Ok(Some(format!("Moved #{a} → #{b}")))
            }
            "clear" => {
                let n = self.queue.items.len();
                self.queue.items.clear();
                Ok(Some(format!("Cleared {n} songs from the queue")))
            }
            _ => Err("Usage: queue [add <song> | remove <n> | move <from> <to> | clear]".into()),
        }
    }
}

/// Copy text to the system clipboard through the terminal (OSC 52).
fn osc52_copy(text: &str) {
    use base64::Engine;
    use std::io::Write;
    let b64 = base64::engine::general_purpose::STANDARD.encode(text);
    let mut out = crate::term::out();
    let _ = write!(out, "\x1b]52;c;{b64}\x07");
    let _ = out.flush();
}
