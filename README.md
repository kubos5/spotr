# Spotr

**A better Spotify client, for the terminal.**

Spotr is a keyboard-first Spotify client with a big-type now-playing screen, cover art drawn either as a real image or as pixel art made of text, a live audio visualizer, synced lyrics, and a command bar that does everything. It follows your terminal's color theme, works with the mouse too, and can turn itself into a Spotify Connect speaker.

<img width="1950" height="1296" alt="CleanShot 2026-10-10 at 01 02 41@2x" src="https://github.com/user-attachments/assets/c5bc9730-eb43-489b-a174-07a03ec78ce3" />

Want a look before setting anything up? Run **`spotr --demo`** — a fictional catalog with a simulated player, no account needed.

---

## Contents

- [Features](#features)
- [Requirements](#requirements)
- [Installing](#installing)
- [First launch](#first-launch)
- [Using Spotr](#using-spotr)
  - [The screen](#the-screen)
  - [How the keyboard works](#how-the-keyboard-works)
  - [Default shortcuts](#default-shortcuts)
  - [Commands](#commands)
  - [Searching and browsing](#searching-and-browsing)
  - [The queue](#the-queue)
  - [Lyrics](#lyrics)
  - [Devices and the built-in player](#devices-and-the-built-in-player)
  - [Mouse](#mouse)
- [Configuration](#configuration)
- [Command-line options](#command-line-options)
- [How it works](#how-it-works)
- [Troubleshooting](#troubleshooting)
- [Development](#development)
- [License](#license)

## Features

- **Now playing, in big type.** Song, album, artist and playlist names are drawn as ASCII art in compact 2- or 3-line fonts, with a shimmer and a reveal animation. Long titles wrap onto a second line. Nine fonts are built in, including **ANSI Compact**, and you can [add any FIGlet font](#title-fonts).
- **Big, clickable controls.** Shuffle, previous, play/pause, next, loop and like are drawn three rows tall and dip in when you use them. Shuffle and loop can be hidden.
- **Works with your OS media controls.** The current song, artist, album, artwork and progress show up in macOS Now Playing / Control Center and Linux MPRIS (GNOME, KDE, `playerctl`). Media keys work too. The OS always gets the original album art, never Spotr's pixelated or ASCII rendering of it.
- **Cover art two ways.** Toggle (`Ctrl+T` / `cover`) between:
  1. **Image**: real pixels via the Kitty, iTerm2 or Sixel graphics protocol, when your terminal supports one.
  2. **Text**: a pixelated rendition built from characters that works everywhere, in four styles (half blocks, colored ASCII, mono ASCII, braille) with adjustable pixelation.
- **Command bar for everything.** `play`, `pause`, `next`, `skip 10`, `rewind 10`, `speed 1.5`, `volume 60`, `loop track`, `queue add …`, `sleep 30` and about 50 more, with completion, ghost text and history.
- **Fully keyboard-driven, with mouse support too.** Every feature has a key or a command, and all shortcuts can be rebound in Settings.
- **Search that remembers.** Results fill the screen with a big *Top result* card. Leave to play a song, come back, and you're exactly where you were, even three pages deep in an artist's discography.
- **Album, artist and playlist pages** with the same big titles and cover art.
- **Audio visualizer** in five styles (bars, mirror, wave, LED blocks, dots). It shows the real spectrum when the built-in player is playing, and a clearly labeled simulation otherwise.
- **Synced lyrics** from [LRCLIB](https://lrclib.net) with a karaoke fill, as plain text or **ASCII art** in any title font. Click or press Enter on a line to jump to it.
- **A real queue.** Add, remove and reorder songs, which the Spotify Web API alone can't do.
- **Built-in Spotify Connect player** (librespot). Spotr can play audio itself, which also enables **playback speed control** and the real visualizer.
- **Output device picker** for Connect devices and local sound cards.
- **Library**: Liked Songs, playlists, saved albums, followed artists, recently played and top songs. You can like, follow, save and add to playlists.
- **Follows your terminal theme.** It only uses your palette's ANSI colors and the default foreground and background. You choose an accent color from that palette.
- **Polished feedback.** Toasts slide in, badges pop up for volume, seek and speed, track changes "decode" in, the cover dissolves in, a heart bursts when you like a song, the command bar shakes on errors, and more. Everything can be turned off for reduced motion.
- **Accessible.** High-contrast mode, ASCII-only icons, reduced motion, rebindable keys, and no information is shown by color alone.
- **Guided onboarding** for creating the Spotify app, signing in, setting up the player and choosing a look.
- Also: a sleep timer, share links (copied through OSC 52), opening in Spotify, playing pasted Spotify links, and a demo mode.

## Requirements

- **Spotify Premium.** Spotify only allows playback control (and streaming through librespot) for Premium accounts. Browsing works with any account.
- **A Spotify developer app** (free, about one minute to create). The onboarding walks you through it.
- A terminal with Unicode support. For the *Image* cover mode you need a terminal with a graphics protocol, such as kitty, WezTerm, iTerm2, Ghostty, Konsole, foot or another Sixel-capable terminal. Every other terminal gets the text renderer.
- To build: **Rust 1.88+** and a C compiler. On Linux you also need the ALSA development headers for the built-in player (`libasound2-dev` on Debian/Ubuntu, `alsa-lib-devel` on Fedora, `alsa-lib` on Arch) and `pkg-config`.

## Installing

### From source

```bash
git clone https://github.com/kubos5/spotr.git
cd spotr
cargo install --path . --locked
```

This puts a `spotr` binary in `~/.cargo/bin`. Keep `--locked`: `Cargo.lock` pins a build-time dependency of librespot (`vergen` 9.0.x), because a newer one breaks librespot's build script. To build without installing:

```bash
cargo build --release
```

The binary is then `target/release/spotr`.

### Without the built-in player

If you only want to control other devices (your phone, desktop app or speakers), you can skip librespot and the audio stack entirely:

```bash
cargo install --path . --locked --no-default-features
```

In this build the visualizer always uses its simulation and `speed` is unavailable. Everything else works the same.

On Linux the OS media integration talks to D-Bus in pure Rust, so no extra system libraries are needed for it.

## First launch

Run `spotr`. The onboarding screen guides you through these steps:

1. **Create a Spotify app.** Open <https://developer.spotify.com/dashboard> (press `Ctrl+O` in that step) and click *Create app*.
   - Name and description: anything.
   - **Redirect URI: `http://127.0.0.1:8888/callback`** (exactly this).
   - APIs: tick *Web API* (and *Web Playback SDK*).
   - Paste the app's **Client ID** into Spotr. No client secret is needed because Spotr uses PKCE.
2. **Sign in.** Your browser opens Spotify's consent page. Approve it and Spotr picks up the response automatically.
3. **Built-in player** (optional). Spotr registers itself as a Spotify Connect device. If Spotify asks for a separate one-time sign-in for streaming, Spotr opens the browser again.
4. **Look**: cover art mode, text-art style, pixelation, title font, accent color and icons, all with a live preview.
5. **Behavior**: what ← / → do, skip length, visualizer, animations and mouse.

You can run the walkthrough again any time with `spotr --setup` or the `onboarding` command.

> Spotify developer apps start in *development mode*, which is limited to accounts you add under *User Management* in the dashboard. Your own account works right away.

## Using Spotr

### The screen

```
┌ search bar ─────────────────────────────────────┐   ← / or click to search
│                                                 │
│   cover art     BIG SONG TITLE                  │   ← now playing (or lyrics / queue)
│                 artist · album                  │
│                 0:44 ━━━━━━●────────── 2:30     │
│                 ⤮ ⏮ ⏸ ⏭ ↻   ♥   ♪ ▮▮▮▮▯ 65%     │
│                                                 │
│ ▁▃▅▇▅▃▁ visualizer ▁▃▅▇▅▃▁                       │
├ command bar ────────────────────────────────────┤   ← start typing anywhere
└ quick keys: play/pause · ←→ · prev/next · help  ┘
```

While you search, the results take over the middle of the screen, the visualizer hides, and a compact player (cover, title, artist, progress) moves down to where the command bar normally is. When you start a song, the main view comes back.

### How the keyboard works

There are three simple rules:

1. **Typing a letter opens the command bar** and types into it. Just type `next` and press Enter.
2. **Every other key can be a shortcut**: space, arrows, symbols, digits, F-keys and Ctrl-combinations. Shortcuts never open the command bar.
3. **While a text field is focused** (command bar, search bar, a settings field), keys only edit the text. No shortcut fires, except `Ctrl+C` to quit.

Press `Esc` to leave a text field or go back. `?` shows every shortcut and command, including the ones you've rebound.

### Default shortcuts

| Key | Action |
| --- | --- |
| `Space` | Play / pause |
| `/` | Focus the search bar |
| `:` | Focus the command bar (empty) |
| `?` or `F1` | Help |
| `,` or `F2` | Settings |
| `←` `→` | Depends on the **arrow mode**: skip seconds · previous/next song · speed · volume (default: skip seconds) |
| `Tab` | Cycle the arrow mode |
| `[` `]` | Depends on the **bracket mode**, with the same choices (default: previous/next song) |
| `Shift+Tab` | Cycle the bracket mode |
| `Shift+←` `Shift+→` (or `{` `}`) | Rewind / skip forward |
| `F7` `F8` | Previous / next song, whatever the modes are |
| `<` `>` `=` | Slower / faster / reset speed |
| `↑` `↓` (or `+` `-`) | Volume up / down |
| `Ctrl+U` | Mute / unmute |
| `0`–`9` | Jump to 0%–90% of the song |
| `Ctrl+S` | Shuffle |
| `Ctrl+R` | Repeat: off → context → track |
| `*` | Like / unlike (the current song, or the selected one in lists) |
| `Ctrl+A` | Add the selected song to the queue |
| `Ctrl+L` or `F3` | Lyrics |
| `Ctrl+E` or `F4` | Queue |
| `Ctrl+D` or `F5` | Devices |
| `Ctrl+B` or `F6` | Library |
| `Ctrl+T` | Cover art: image ↔ text |
| `Ctrl+V` | Cycle visualizer style |
| `Esc` | Back / close |
| `Ctrl+C` or `Ctrl+Q` | Quit |

In lists: `↑ ↓ PgUp PgDn Home End` to move, `Enter` to play or open, `Backspace` to go back a page, and `Tab` to switch search filters. In the queue: `Shift+↑ ↓` to move a song and `Del` to remove it. In text fields: `Ctrl+W` deletes a word, `Ctrl+U` the line, and `Alt+← →` jumps by word.

Every one of these can be rebound under **Settings → Keys**: select an action, press Enter, then press the new key. Plain letters can't be bound because they're reserved for typing commands.

### Commands

Type them in the command bar. `Tab` cycles through completions, `→` accepts the ghost text, and `↑ ↓` walk through your history.

| Command | What it does |
| --- | --- |
| `play [query \| link]` | Resume, or search and play the best match. Spotify links work too. |
| `pause` · `toggle` | Pause, or toggle play / pause |
| `next` · `previous` | Skip to the next / previous song |
| `skip <seconds>` · `rewind <seconds>` | e.g. `skip 10`, `rewind 30` |
| `seek <time>` | `seek 1:23`, `seek 83`, `seek 50%` |
| `restart` | Start the song over |
| `speed <multiplier>` | `speed 1.5`, `speed 0.75` (built-in player) |
| `volume <n \| +n \| -n>` | `volume 60`, `volume +10` |
| `mute` · `unmute` | |
| `shuffle [on\|off]` | |
| `loop [off\|context\|track]` | Alias: `repeat` |
| `sleep <minutes \| off>` | Pause after a while |
| `like` · `unlike` | The current song, or the selected one |
| `follow` · `unfollow` | The current (or selected) artist |
| `savealbum` | Save the current album |
| `addto <playlist>` | Add the current song to one of your playlists (Tab completes names) |
| `queue` | Show the queue |
| `queue add [query]` | Add the selected / current song, or search for one |
| `queue remove <n>` · `queue move <from> <to>` · `queue clear` | |
| `search [query]` | Open search, optionally with a query |
| `library` · `liked` · `playlists` · `recent` · `top` | Browse your library |
| `album` · `artist` | Open the current song's album or artist |
| `lyrics [ascii\|text]` | Show lyrics, optionally switching between plain text and ASCII art |
| `devices [name]` · `home` | Switch views, or play on a device |
| `settings` · `help [topic]` · `about` | |
| `cover [image\|text\|blocks\|ascii\|braille\|on\|off]` | Cover art mode |
| `pixel <0\|8\|16\|32…>` | Pixelation for text cover art |
| `visualizer [on\|off\|bars\|mirror\|wave\|blocks\|dots]` | |
| `font [name]` | Title font: `block`, `slim`, `ansi-compact`, `future`, `pagga`, `emboss`, `small-block`, `small-braille`, `mini`, `plain`, or one of yours (Tab completes) |
| `accent <color>` | Accent color from your terminal palette |
| `mode [arrows\|brackets] <seek\|track\|speed\|volume>` | What ← / → (or `[` / `]`) do, e.g. `mode brackets volume` |
| `animations [on\|off]` | |
| `share` | Copy the song's link to the clipboard (OSC 52) |
| `open` | Open the song in your browser / Spotify |
| `refresh` | Re-sync playback state |
| `player [login\|restart\|off]` | Manage the built-in player |
| `login` · `logout` · `whoami` | Account |
| `onboarding` | Run the first-time setup again |
| `quit` | Alias: `exit` |

Many commands have short aliases, such as `n`, `prev`, `vol`, `rw`, `ff` and `q`. `help <word>` filters the help screen.

### Searching and browsing

- Press `/` (or click the search bar) and type. With *search as you type* on, results update after a short pause. Otherwise press Enter.
- Results appear as one list. The **first result is shown big**, with its cover and its name in the ASCII font. Under it are sections for Songs, Artists, Albums and Playlists.
- `Tab` / `Shift+Tab` switches between **All · Songs · Artists · Albums · Playlists**. In a single-type filter, more results load as you scroll.
- `↓` or `Enter` moves into the list. `Enter` plays a song or opens an album, artist or playlist page, which has a **Play** and a **Shuffle play** button at the top. `Backspace` / `Esc` goes back.
- Starting a song closes search. **Opening search again restores exactly what you saw**: the query, the filter, the page stack and the selected row. The query is pre-selected, so typing starts a fresh search while the arrow keys keep the old one. This memory lasts until you quit Spotr.
- Paste a Spotify link anywhere (outside a text field) to play it.

### The queue

The Spotify Web API can add to Spotify's queue but can't remove or reorder anything in it. So Spotr keeps **its own queue**, which you can edit freely:

- Add songs with `Ctrl+A` on a search result, `queue add <song>`, or `queue add` (current / selected song).
- Reorder with `Shift+↑ ↓`, remove with `Del`, or press `Enter` to play one right away.
- Shortly before the current song ends (3 seconds by default, see *Settings → Playback → Queue handoff*), Spotr hands the next song to Spotify, so it plays seamlessly on any device. `next` also takes from your queue first.
- Anything already in Spotify's own queue (for example, added from your phone) is shown read-only underneath.

### Lyrics

`Ctrl+L` or `lyrics` shows lyrics from [LRCLIB](https://lrclib.net), a free and open lyrics database. Synced lyrics scroll on their own, and the current line fills in like karaoke. Scroll with `↑ ↓`, and press `Enter` or click a line to jump to it. Auto-follow resumes after a few seconds.

`lyrics ascii` (or *Settings → Playback → Lyrics style*) draws the lyrics as ASCII art, with each line word-wrapped in the big font. *Lyrics font* picks the font (by default the title font). Compact fonts like `slim`, `small-braille` or `ansi-compact` fit the most lines on screen. `lyrics text` switches back.

### OS media controls

While a song plays, Spotr reports it to the operating system:

- **macOS**: Now Playing in Control Center and on the lock screen, plus the media keys (play/pause, next, previous) and AirPods/headphone controls.
- **Linux**: MPRIS over D-Bus, so GNOME's and KDE's media widgets, `playerctl` and media keys work.
- **Windows**: not supported yet. The system media controls need a real window, which a terminal app doesn't have.

Title, artist, album, duration, position and **the original cover image** are sent. Text-art, pixelation and other effects only apply inside the terminal. Turn it off under *Settings → Player → OS media controls* (applies after a restart) or for one session with `--no-media`.

On macOS this needs an AppKit event loop on the process's main thread, so Spotr runs its UI on a second thread. It has no Dock icon and no windows.

### Devices and the built-in player

`Ctrl+D` lists your Spotify Connect devices. Press Enter to move playback to one. If the built-in player is running, the same panel lists your computer's audio outputs too.

The **built-in player** uses [librespot](https://github.com/librespot-org/librespot) to make Spotr a Connect device (named *Spotr* by default). When it's the active device:

- the **visualizer shows the real spectrum** of what you hear (Spotr runs an FFT on its own audio output),
- **`speed` works**. Speed is done by resampling, so pitch changes with speed, like a turntable,
- volume, seeking and so on behave like any other Connect device.

When music plays on another device, Spotr can't access the audio, so the visualizer runs a simulation and labels it `◌ simulated`.

### Mouse

With mouse support on (the default):

- Click the search bar or command bar to focus them.
- Click results, queue rows and devices to activate them. Hovering selects rows.
- Click the progress bar to seek, and the volume meter to set the volume.
- Click ⤮ ⏮ ⏸ ⏭ ↻ ♥ to use them, the cover to toggle image / text, and the visualizer to cycle its style.
- Click the hints line to trigger those actions, and a toast to dismiss it.
- Scroll lists. On the now-playing view, scrolling changes the volume.

Turn it off in *Settings → Look → Mouse support* if you'd rather select text in your terminal.

## Configuration

Almost everything is in the Settings panel (`,`), which saves automatically. The file lives at:

| OS | Config | Cache (covers, librespot, log) |
| --- | --- | --- |
| Linux | `~/.config/spotr/config.toml` | `~/.cache/spotr/` |
| macOS | `~/Library/Application Support/spotr/config.toml` | `~/Library/Caches/spotr/` |
| Windows | `%APPDATA%\spotr\config.toml` | `%LOCALAPPDATA%\spotr\` |

`spotr --print-paths` shows the exact locations. You can override them with `SPOTR_CONFIG_DIR` and `SPOTR_CACHE_DIR`.

An excerpt:

```toml
[appearance]
accent = "green"          # green, cyan, blue, magenta, red, yellow, white
cover_mode = "image"      # image | text
text_art = "blocks"       # blocks | ascii | ascii-mono | braille
pixelation = 0            # 0 = sharp; 8, 16, 32… = chunky pixels
title_font = "ansi-compact" # block, slim, ansi-compact, future, pagga, emboss,
                            # small-block, small-braille, mini, plain, or a user font
big_controls = true
show_shuffle = true       # the shuffle / loop buttons can be hidden
show_repeat = true
long_title = "shrink"     # shrink (wrap / smaller font) | marquee | plain
high_contrast = false
icons = "unicode"         # unicode | ascii

[animations]
enabled = true            # false = reduced motion
duration = 1.0            # multiplier on all animation durations

[visualizer]
enabled = true
style = "bars"            # bars | mirror | wave | blocks | dots
colors = "spectrum"       # spectrum | accent | mono
height = 7

[playback]
seek_step = 10
arrow_mode = "seek"       # ← / →: seek | track | speed | volume
bracket_mode = "track"    # [ / ]: same choices
search_context = "album"  # album: keep playing the album after a searched song

[lyrics]
ascii = false             # true = ASCII-art lyrics
font = ""                 # "" = same as title_font

[search]
live = true
remember_position = true
close_on_play = true

[keys]
play_pause = ["space"]
lyrics = ["ctrl+l", "f3"]
bracket_right = ["]"]
next = ["f8"]
```

### Title fonts

Built in:

| Font | Lines | Look |
| --- | --- | --- |
| `block` (default) | 3 | Spotr's own chunky half-block font |
| `slim` | 2 | A smaller version of `block` |
| `ansi-compact` | 3 | ANSI Compact by Loïc Cressot (from [patorjk.com/taag](https://patorjk.com/software/taag/)) |
| `future` | 3 | Thin box-drawing letters |
| `pagga` | 3 | Blocks on a shaded background |
| `emboss` | 3 | Heavy box-drawing letters |
| `small-block` | 3–4 | Quadrant blocks |
| `small-braille` | 2 | Braille dots, the most compact |
| `mini` | 3–4 | Classic FIGlet "Mini" |
| `plain` | 1 | No ASCII art |

To add your own, put any FIGlet (`.flf`) or TOIlet (`.tlf`) font file into a `fonts` folder next to the config file (`spotr --print-paths` shows where), then restart Spotr. For example, download a font from [patorjk.com/taag](https://patorjk.com/software/taag/) and save it as `fonts/doom.flf`. It then appears in Settings → Look → Title font and in `font <name>`. Fonts taller than about 4 lines work, but they leave less room for everything else. If a title doesn't fit in the chosen font, Spotr wraps it, then falls back to `slim`, then to plain text.

Licenses for the bundled fonts are in [`assets/fonts/README.md`](assets/fonts/README.md).

### Key strings

Key strings look like `space`, `enter`, `esc`, `tab`, `left`, `pageup`, `f5`, `ctrl+l`, `alt+x`, `shift+right`, or any single symbol (`?`, `+`, `[`).

Your Spotify token is stored separately in `token.json` next to the config, readable only by your user. Spotr never sees your password.

## Command-line options

```
spotr [OPTIONS]

    --demo          Explore Spotr with a fictional catalog (no account needed)
    --setup         Run the first-time setup again
    --no-player     Don't start the built-in player this session
    --no-media      Don't report playback to the OS (Now Playing, media keys)
    --text-art      Force text cover art (skip terminal graphics detection)
    --test-audio    Play a short test tone through the built-in player's audio output
    --debug         Write a debug log to the cache directory
    --print-paths   Show where config, cache and logs live, then exit
    --reset         Delete the config and saved login, then exit
    -h, --help      Show help
    -V, --version   Show the version
```

## How it works

```
             ┌─────────────── tokio runtime ──────────────────────────┐
 keyboard ──►│ input thread ─┐                                        │
 & mouse     │               ▼                                        │
             │        ┌──────────────┐   spawn   ┌─────────────────┐  │
             │        │  App (state) │──────────►│ async tasks     │  │
             │        │  event loop  │◄──────────│ Web API, images,│  │
             │        └──────┬───────┘  events   │ lyrics, OAuth   │  │
             │               │ draw              └─────────────────┘  │
             │               ▼                                        │
             │        ratatui + ratatui-image ──► your terminal       │
             │                                                        │
             │ librespot (Spirc + Player) ──► cpal sink ──► speakers  │
             │                   │ speed resampling, FFT tap ─► visualizer
             └────────────────────────────────────────────────────────┘
```

- **Protecting the screen.** While the UI runs, the process's stdout and stderr are redirected to the log file, and the UI draws to a private handle on the terminal. Libraries that print, or audio frameworks that log to stderr, can't corrupt the screen. A panic on a background thread (for example inside librespot) becomes an error toast instead of taking the terminal down. An exit handler restores the terminal even if a library calls `exit()`.
- **OS media integration** (`src/media.rs`) uses [souvlaki](https://github.com/Sinono3/souvlaki) on a dedicated thread. Spotr sends metadata when the track changes and the playback state when it changes or drifts. Media keys come back in as events, and the app handles them like its own shortcuts.
- **One event loop.** Terminal input runs on its own thread. Network work (the Spotify Web API, cover downloads, LRCLIB, OAuth) runs as async tasks. Everything reports back to a single loop that owns the app state, which keeps the UI responsive. Frames are drawn at the configured FPS while something is animating and a few times per second otherwise.
- **Spotify Web API** (`src/spotify/api.rs`) is a small client built on reqwest. It refreshes tokens automatically, retries after `429 Retry-After`, and turns Spotify's errors into readable messages. Authentication (`src/spotify/auth.rs`) is the OAuth Authorization Code flow with **PKCE**. A one-shot HTTP listener on `127.0.0.1` catches the redirect, so no client secret is involved.
- **Playback state** is polled about once a second and interpolated between polls, so the progress bar moves smoothly. When the built-in player is active, its own events give a finer clock. Actions update the UI immediately and reconcile with Spotify afterwards.
- **The built-in player** (`src/player.rs`) runs librespot's `Spirc` (Spotify Connect) and `Player` with a custom audio sink. The sink resamples from 44.1 kHz to your device's rate (scaled by the speed factor), plays through `cpal`, and copies what is actually heard into a ring buffer. The visualizer (`src/visualizer.rs`) runs a windowed FFT on that buffer and maps it to log-spaced bands with fast attack, slow decay and falling peaks.
- **Cover art** (`src/images.rs`) is downloaded once and cached on disk. *Image* mode uses [ratatui-image](https://github.com/benjajaja/ratatui-image), which detects Kitty, iTerm2 or Sixel support at startup. *Text* mode downsamples the image (optionally to a coarse grid first, for chunky pixels) and draws it with `▀` half blocks in true color, an ASCII brightness ramp, or braille dots.
- **ASCII fonts** (`src/font.rs`, `src/figlet.rs`). `block` and `slim` are tiny bitmap fonts (3×5 and 3×4 pixels per letter) rendered with half-block characters. The other fonts are FIGlet / TOIlet files, parsed at startup and laid out with "fitting" (letters slide together until they touch). Blank rows are trimmed, so a 6-line FIGlet font like ANSI Compact really takes 3 lines. Accented letters fall back to an ASCII transliteration when a font lacks them, so *Beyoncé* still renders.
- **Theme.** Spotr only uses the 16 ANSI palette colors plus "default" foreground and background, so your terminal theme decides how it looks. Only cover art uses true color.
- **Search memory.** Search keeps its query, filter, results, page stack (album → artist → …) and every list position in memory. Closing search only hides it.

Source layout:

```
src/
  main.rs          terminal setup, CLI flags, event loop
  app/             state & logic: input, commands, playback, browse, settings, onboarding
  ui/              drawing: now playing, browse, bars, views, overlays, effects, onboarding
  spotify/         Web API client, OAuth (PKCE), models, demo catalog
  player.rs        librespot integration + cpal sink (feature "local-player")
  visualizer.rs    FFT + rendering
  images.rs        cover cache, graphics protocols, text art
  font.rs          font registry + bitmap ASCII fonts
  figlet.rs        FIGlet / TOIlet font loader
  term.rs          terminal ownership, output redirection, panic handling
  media.rs         OS Now Playing / MPRIS / media keys
  commands.rs      command table, completion, parsing
  keys.rs          actions, key parsing, keymap
  lyrics.rs        LRCLIB client + LRC parser
  config.rs        settings file
  anim.rs, theme.rs, textinput.rs, audio.rs, logger.rs
assets/fonts/      bundled FIGlet / TOIlet fonts (+ licenses)
```

## Troubleshooting

**"No active device".** Spotify needs somewhere to play. Open Spotify on any device, enable the built-in player, or pick a device with `Ctrl+D`.

**"Spotify Premium is required".** Spotify only allows playback control for Premium accounts. Browsing and library features still work.

**`INVALID_CLIENT: Invalid redirect URI` in the browser.** The Redirect URI in your Spotify app must be exactly `http://127.0.0.1:8888/callback`. Spotify no longer accepts `localhost`.

**The built-in player doesn't play any sound.** Run `spotr --test-audio`. It plays a one-second tone through the same audio path and reports the device and timings. If it fails, pick another output in `Ctrl+D` (*Built-in player output*).

**The built-in player asks to sign in again / won't start.** Run `player login`. This does a one-time browser sign-in for streaming, after which the credentials are cached under the cache directory. `player restart` reconnects. Run with `--debug` and check the log for details.

**Cover art shows as text even though my terminal supports images.** Detection relies on the terminal answering a query at startup. Inside tmux, enable passthrough (`set -g allow-passthrough on`). Over slow SSH links, detection can time out. `Ctrl+T` toggles modes, and *Settings → Look → Cover art* shows what was detected.

**Startup takes about a second / the first key is ignored in an unusual terminal.** Some terminals never answer the graphics query, so Spotr waits for a timeout. Start with `--text-art` to skip detection.

**"Spotify refused this (Forbidden)".** Since February 2026, Spotify only gives *development-mode* apps some Web API endpoints, and only for users on the app's allowlist (*User Management* in the dashboard, up to 5 users). The app owner also needs Premium. Spotr uses the endpoints that are still available:

- Liking, saving albums and following artists use the new unified library API (`/me/library`).
- Playlist songs come from `/playlists/{id}/items`.
- Artists' *Popular* songs come from a search, because Spotify removed the top-tracks endpoint.
- Search returns at most 10 results per type (load more by scrolling in a single-type filter).

**A playlist shows no songs.** Spotify now only lists the songs of playlists you own or collaborate on. Others still play fine with the **Play** button at the top of their page.

**Logs.** `spotr --debug` writes `spotr.log` to the cache directory (`spotr --print-paths`).

## Development

```bash
cargo run -- --demo        # try the UI without an account
cargo test                 # unit tests (fonts, keys, commands, LRC parsing, FFT…)
cargo clippy               # lints
cargo run -- --test-audio  # play a test tone through the built-in player's audio path
cargo run -- --test-connect  # connect librespot with a dummy token: should end in "Bad credentials"
```

The `local-player` feature (on by default) pulls in librespot and cpal. Build with `--no-default-features` for a lighter, Web-API-only build.

Contributions are welcome. Please keep everything reachable by keyboard and keep colors to the terminal palette.

## License

Spotr is free software: you can redistribute it and/or modify it under the terms of the **GNU General Public License, version 3** (or, at your option, any later version). See [`LICENSE`](LICENSE).

Spotr is not affiliated with, endorsed by, or connected to Spotify AB. "Spotify" is a trademark of Spotify AB. Lyrics are provided by LRCLIB.
