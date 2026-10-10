# Spotr

**A better Spotify client, for the terminal.**

<img width="1941" height="1276" alt="CleanShot 2026-10-10 at 16 41 55@2x" src="https://github.com/user-attachments/assets/6c158526-368d-4460-9d58-be2aa75acd12" />

Spotr is a super lightweight, blazingly fast and keyboard-first Spotify client, built in Rust.
It features polished animations throughout the UI, so that every action feels super satisfying.

You can use Spotr as a Player device, or to control other Spotify devices linked to your account.

Want to try it out? Run `spotr --demo` to take a look at the UI without an account.

---

## Contents

- [Features](#features)
- [Requirements](#requirements)
- [Installation](#installation)
- [First launch](#first-launch)
- [Using Spotr](#using-spotr)
  - [Layout](#layout)
  - [Keyboard](#keyboard)
  - [Commands](#commands)
  - [Nice to know](#nice-to-know)
  - [Queue](#queue)
  - [Lyrics](#lyrics)
  - [Devices](#devices)
  - [Mouse](#mouse)
- [Configuration](#configuration)
- [Command-line options](#command-line-options)
- [How it works](#how-it-works)
- [Troubleshooting](#troubleshooting)
- [Development](#development)
- [License](#license)

## Features

| Feature                 | Description                                                                                                                                                         |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Now playing**         | The song name is drawn in ASCII art. Use any of the 9 built-in fonts, or [add your own](#title-fonts).                                                              |
| **The command bar**     | The main way you'll interact with Spotr is the Command bar. Everything is a command. `play`/`pause`, `next`/`previous`, `skip 10`, `volume 60`, and many more.      |
| **The search bar**      | To search up songs, artists, albums or playlists, you can use the Search bar. Focus it with `/`, or the `search` command.                                           |
| **OS controls**         | Spotr reports Now playing data to the Operating System, so that you can control it through media keys.                                                              |
| **Cover art rendering** | If you don't like images in the terminal, or your terminal doesn't support them, you can render the cover art as text instead, choosing from the 4 built-in styles. |
| **Visualizer**          | Above the command bar, you'll see a pretty real time audio visualizer, available in 5 different styles.                                                             |

... and many more! [Try it for yourself](#installation)

## Requirements

_Mandatory:_
- Spotify Premium
- access to the Spotify Developer Console

_For compiling:_
- Rust 1.88+
- a C compiler

_On Linux:_
- ALSA development headers for the built-in player (`libasound2-dev` on Debian/Ubuntu, `alsa-lib-devel` on Fedora, `alsa-lib` on Arch)
- `pkg-config`

_Optional:_
- a terminal with a graphics protocol, such as kitty, WezTerm, iTerm2 or Ghostty.
  Every other terminal gets the text renderer.

## Installation

### Using package managers

> [!NOTE]
> For now, the only way to use Spotr is compiling it from source.
> I'm planning on distributing Spotr through [Homebrew](https://brew.sh/), the [AUR](https://aur.archlinux.org/), [WinGet](https://learn.microsoft.com/en-us/windows/package-manager/winget/), and more. Stay tuned!

For now:
### Compile from source

```bash
git clone https://github.com/kubos5/spotr.git
cd spotr
cargo install --path . --locked
```

This puts the binary in `~/.cargo/bin`.

`--locked`: `Cargo.lock` pins a build-time dependency of librespot (`vergen` 9.0.x), because a newer one breaks librespot's build script. 

#### Build without installing:

```bash
cargo build --release
```

The binary is then `target/release/spotr`.

#### Compile without audio features

> [!WARNING]
> **This probably isn't for you.**
> Building Spotr without autio features has no benefits other than making the executable a little lighter.
> I heavily recommend building normally (`cargo install --path . --locked`).

If you only want to control other devices (your phone, desktop app or speakers), you can skip librespot and the audio stack entirely:

```bash
cargo install --path . --locked --no-default-features
```

In this build the visualizer always uses its simulation and `speed` is unavailable. Everything else works the same.

On Linux the OS media integration talks to D-Bus in pure Rust, so no extra system libraries are needed for it.

## First launch

1. run `spotr`
2. create a [Spotify app](https://developer.spotify.com/dashboard) (press `Ctrl+O`)
3. paste the Client ID into Spotr
4. customize your settings
5. done!

When creating the app in the Spotify Developer Console:
- `http://127.0.0.1:8888/callback` as the Redirect URI
- tick `Web API` and `Web Playback SDK`

You can run the walkthrough again any time with `spotr --setup` or the `onboarding` command.

## Using Spotr

### Layout

```
┌ search bar ─────────────────────────────────────┐   ← search (/)
│                                                 │
│   ┌ cover ┐     BIG SONG TITLE                  │   ← now playing
│   │       │     artist · album                  │
│   │       │     0:44 ━━━━━━●────────── 2:30     │
│   └───────┘     x << || >> o  <3                │
│                                                 │
│ ▁▇▃▅▃▇▃▅▃▁▇▅▇▃▅▁▃▅ visualizer ▇▁▅▇▁▅▃▇▃▅▇▃▅▁▅▁▅ │
├ command bar ────────────────────────────────────┤   ← command bar
└ quick keys: play/pause · ←→ · prev/next · help ─┘
```

While you search, the current song gets moved down to where the command bar and visualizer normally are.

### Keyboard

While on the Now playing screen, start typing to enter a command. To search, press `/`, or use the `search` command.

While typing, keyboard shortcuts (except `Ctrl+C`) get disabled until you exit out of the text field with `Esc` (or by confirming with `Enter`).

Here are the default shortcuts:

| Key                                   | Action                                                                                                        |
| ------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `Space`                               | Play / pause                                                                                                  |
| `/`                                   | Focus the search bar                                                                                          |
| `:`                                   | Focus the command bar                                                                                         |
| `?` or `F1`                           | Help                                                                                                          |
| `,` or `F2`                           | Settings                                                                                                      |
| `←` / `→`                             | Depends on the **arrow mode**:<br>- skip seconds _(default)_<br>- previous/next song<br>- speed<br>- volume   |
| `[` / `]`                             | Depends on the **bracket mode**:<br>- skip seconds<br>- previous/next song _(default)_<br>- speed<br>- volume |
| `Tab`                                 | Cycle arrow mode                                                                                              |
| `Shift+Tab`                           | Cycle bracket mode                                                                                            |
| `Shift+←` / `Shift+→`<br>or `{` / `}` | Rewind / skip forward                                                                                         |
| `F7` / `F8`                           | Previous / next song                                                                                          |
| `<` / `>` / `=`                       | Slower / faster / reset speed                                                                                 |
| `↑` `↓` (or `+` `-`)                  | Volume up / down                                                                                              |
| `Ctrl+U`                              | Mute / unmute                                                                                                 |
| `0-9`                                 | Jump to 0%–90% of the song                                                                                    |
| `Ctrl+S`                              | Shuffle                                                                                                       |
| `Ctrl+R`                              | Repeat: off → context → track                                                                                 |
| `*`                                   | Like / unlike (the current song, or the selected one in lists)                                                |
| `Ctrl+A`                              | Add the selected song to the queue                                                                            |
| `Ctrl+L` or `F3`                      | Lyrics                                                                                                        |
| `Ctrl+E` or `F4`                      | Queue                                                                                                         |
| `Ctrl+D` or `F5`                      | Devices                                                                                                       |
| `Ctrl+B` or `F6`                      | Library                                                                                                       |
| `Ctrl+T`                              | Cover art: image ↔ text                                                                                       |
| `Ctrl+V`                              | Cycle visualizer style                                                                                        |
| `Esc`                                 | Back / close                                                                                                  |
| `Ctrl+C` or `Ctrl+Q`                  | Quit                                                                                                          |
Show them anytime with `?`.

In lists:
- `↑/↓, PgUp/PgDn, Home, End` to move
- `Enter` to play or open
- `Backspace` to go back a page
- `Tab` to switch search filters

In the queue:
- `Shift+↑/↓` to move a song
- `Del` to remove a song

In text fields:
- `Ctrl+W` deletes a word
- `Ctrl+U` deletes the entire line
- `Alt+←/→` jumps by word

Change your shortcuts in **Settings → Keys**.

### Commands

- start typing to focus the command bar
- accept completion with `→`
- cycle completions with `Tab`
- cycle commands history with `↑/↓`

Here all of the commands:

| Command                                                       | Action                                                                                                                                                     |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `play [query \| link]`                                        | Resume, or search and play the best match. Spotify links work too.                                                                                         |
| `pause` · `toggle`                                            | Pause, or toggle play / pause                                                                                                                              |
| `next` · `previous`                                           | Skip to the next / previous song                                                                                                                           |
| `skip <seconds>` · `rewind <seconds>`                         | e.g. `skip 10`, `rewind 30`                                                                                                                                |
| `seek <time>`                                                 | `seek 1:23`, `seek 83`, `seek 50%`                                                                                                                         |
| `restart`                                                     | Start the song over                                                                                                                                        |
| `speed <multiplier>`                                          | `speed 1.5`, `speed 0.75` (built-in player)                                                                                                                |
| `volume <n \| +n \| -n>`                                      | `volume 60`, `volume +10`                                                                                                                                  |
| `mute` · `unmute`                                             |                                                                                                                                                            |
| `shuffle [on\|off]`                                           |                                                                                                                                                            |
| `loop [off\|context\|track]`                                  | Alias: `repeat`                                                                                                                                            |
| `sleep <minutes \| off>`                                      | Pause after a while                                                                                                                                        |
| `like` · `unlike`                                             | The current song, or the selected one                                                                                                                      |
| `follow` · `unfollow`                                         | The current (or selected) artist                                                                                                                           |
| `savealbum`                                                   | Save the current album                                                                                                                                     |
| `addto <playlist>`                                            | Add the current song to one of your playlists (Tab completes names)                                                                                        |
| `queue`                                                       | Show the queue                                                                                                                                             |
| `queue add [query]`                                           | Add the selected / current song, or search for one                                                                                                         |
| `queue remove <n>` · `queue move <from> <to>` · `queue clear` |                                                                                                                                                            |
| `search [query]`                                              | Open search, optionally with a query                                                                                                                       |
| `library` · `liked` · `playlists` · `recent` · `top`          | Browse your library                                                                                                                                        |
| `album` · `artist`                                            | Open the current song's album or artist                                                                                                                    |
| `lyrics [ascii\|text]`                                        | Show lyrics, optionally switching between plain text and ASCII art                                                                                         |
| `devices [name]` · `home`                                     | Switch views, or play on a device                                                                                                                          |
| `settings` · `help [topic]` · `about`                         |                                                                                                                                                            |
| `cover [image\|text\|blocks\|ascii\|braille\|on\|off]`        | Cover art mode                                                                                                                                             |
| `pixel <0\|8\|16\|32…>`                                       | Pixelation for text cover art                                                                                                                              |
| `visualizer [on\|off\|bars\|mirror\|wave\|blocks\|dots]`      |                                                                                                                                                            |
| `font [name]`                                                 | Title font: `block`, `slim`, `ansi-compact`, `future`, `pagga`, `emboss`, `small-block`, `small-braille`, `mini`, `plain`, or one of yours (Tab completes) |
| `accent <color>`                                              | Accent color from your terminal palette                                                                                                                    |
| `mode [arrows\|brackets] <seek\|track\|speed\|volume>`        | What ← / → (or `[` / `]`) do, e.g. `mode brackets volume`                                                                                                  |
| `animations [on\|off]`                                        |                                                                                                                                                            |
| `share`                                                       | Copy the song's link to the clipboard (OSC 52)                                                                                                             |
| `open`                                                        | Open the song in your browser / Spotify                                                                                                                    |
| `refresh`                                                     | Re-sync playback state                                                                                                                                     |
| `player [login\|restart\|off]`                                | Manage the built-in player                                                                                                                                 |
| `login` · `logout` · `whoami`                                 | Account                                                                                                                                                    |
| `onboarding`                                                  | Run the first-time setup again                                                                                                                             |
| `quit`                                                        | Alias: `exit`                                                                                                                                              |

Short aliases include:
- `n` = `next`
- `prev` = `previous`
- `vol` = `volume`
- `rw` = `rewind`
- `ff` = `skip`
- `q` = `quit`

`help <word>` to filter the help screen.

### Nice to know

- when searching, use `Tab` / `Shift+Tab` to cycle between
  `All`, `Songs`, `Artists`, `Albums`, and `Playlists`
- you can paste a Spotify link anywhere (outside a text field) to play it.

### Queue

The Spotify Web API can add to Spotify's queue but can't remove or reorder anything in it. So Spotr keeps **its own queue**, which you can edit freely:

- add songs with `Ctrl+A` on a search result, `queue add <song>`, or `queue add` (current)
- reorder songs with `Shift+↑/↓`
- remove songs with `Del`
- press `Enter` to play a song right away

Shortly before the current song ends (3 seconds by default, see *Settings → Playback → Queue handoff*), Spotr hands the next song to Spotify, so it plays seamlessly on any device. `next` also takes from your queue first.

Anything already in Spotify's own queue (for example, added from your phone) is shown read-only underneath.

### Lyrics

`Ctrl+L` or `lyrics` shows lyrics from [LRCLIB](https://lrclib.net), a free and open lyrics database.

Scroll with `↑ ↓`, and press `Enter` or click a line to jump to it.

`lyrics ascii` (or *Settings → Playback → Lyrics style*) to draw the lyrics as ASCII art, with each line word-wrapped in the big font.
`lyrics text` to switch back.

*Lyrics font* picks the font (by default the title font). Compact fonts like `slim`, `small-braille` or `ansi-compact` fit the most lines on screen. 

### OS media controls

While a song plays, Spotr reports it to the operating system:

- **macOS**: Now Playing in Control Center
- **Linux**: MPRIS over D-Bus
- **Windows**: not supported yet

Turn it off under *Settings → Player → OS media controls* (applies after a restart) or for just one session with `--no-media`.

On macOS this needs an AppKit event loop on the process's main thread, so Spotr runs its UI on a second thread. It has no Dock icon and no windows.

### Devices

`Ctrl+D` lists your Spotify Connect devices.
If the built-in player is running, the same panel lists your computer's audio outputs too.

The **built-in player** uses [librespot](https://github.com/librespot-org/librespot) to make Spotr a Connect device (named *Spotr* by default).

When music plays on another device, Spotr can't access the audio, so the visualizer runs a simulation and labels it `◌ simulated`.

### Mouse

- click the search bar or command bar to focus them.
- click results, queue rows and devices to activate them. Hovering selects rows.
- click the progress bar to seek, and the volume meter to set the volume.
- click the controls to use them, the cover to toggle image / text, and the visualizer to cycle its style.
- click the hints line to trigger those actions, and a toast to dismiss it.
- scroll lists. On the now-playing view, scrolling changes the volume.

Turn it off in *Settings → Look → Mouse support* if you'd rather use the mouse select text.

## Configuration

Almost everything is in the Settings panel (`,`), which saves automatically. The file lives at:

| OS      | Config                                            | Cache (covers, librespot, log) |
| ------- | ------------------------------------------------- | ------------------------------ |
| Linux   | `~/.config/spotr/config.toml`                     | `~/.cache/spotr/`              |
| macOS   | `~/Library/Application Support/spotr/config.toml` | `~/Library/Caches/spotr/`      |
| Windows | `%APPDATA%\spotr\config.toml`                     | `%LOCALAPPDATA%\spotr\`        |

Show the exact locations with `spotr --print-paths`.
Override them with `SPOTR_CONFIG_DIR` and `SPOTR_CACHE_DIR`.

Example config:

```toml
[general]
splash = true               # startup animation
goodbye = true              # closing animation

[appearance]
accent = "green"            # green, cyan, blue, magenta, red, yellow, white
cover_mode = "image"        # image | text
text_art = "blocks"         # blocks | ascii | ascii-mono | braille
pixelation = 0              # 0 = sharp; 8, 16, 32… = chunky pixels
title_font = "ansi-compact" # block, slim, ansi-compact, future, pagga, emboss,
                            # small-block, small-braille, mini, plain, or a user font
                            # (also used by the startup animation and About)
big_controls = true
show_shuffle = true         # the shuffle / loop buttons can be hidden
show_repeat = true
long_title = "shrink"       # shrink (wrap / smaller font) | marquee | plain
high_contrast = false
icons = "unicode"           # unicode | ascii

[animations]
enabled = true              # false = reduced motion
duration = 1.0              # multiplier on all animation durations

[visualizer]
enabled = true
style = "bars"              # bars | mirror | wave | blocks | dots
colors = "spectrum"         # spectrum | accent | mono
height = 7

[playback]
seek_step = 10
arrow_mode = "seek"         # ← / →: seek | track | speed | volume
bracket_mode = "track"      # [ / ]: same choices
volume = 70                 # built-in player volume, remembered automatically
search_context = "album"    # album: keep playing the album after a searched song

[lyrics]
ascii = false               # true = ASCII-art lyrics
font = ""                   # "" = same as title_font

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

| Font              | Lines | Look                               |
| ----------------- | ----- | ---------------------------------- |
| `block` (default) | 3     | Spotr's own chunky half-block font |
| `slim`            | 2     | A smaller version of `block`       |
| `ansi-compact`    | 3     | ANSI Compact by Loïc Cressot       |
| `future`          | 3     | Thin box-drawing letters           |
| `pagga`           | 3     | Blocks on a shaded background      |
| `emboss`          | 3     | Heavy box-drawing letters          |
| `small-block`     | 3–4   | Quadrant blocks                    |
| `small-braille`   | 2     | Braille dots, the most compact     |
| `mini`            | 3–4   | Classic FIGlet "Mini"              |
| `plain`           | 1     | No ASCII art                       |

To add your own:
- navigate to the directory containing the config file
  (`spotr --print-paths`)
- create a directory called `fonts`
- place your FIGlet (`.flf`) or TOIlet (`.tlf`) font file into said directory
- restart Spotr

You can find great fonts at [patorjk.com/taag](https://patorjk.com/software/taag/).
Fonts taller than 4 lines work, but are not recommended.

Licenses for the bundled fonts are in [`assets/fonts/README.md`](assets/fonts/README.md).

### Security

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

<img width="720" height="420" alt="image" src="https://github.com/user-attachments/assets/83e556ac-9c24-48ae-9ed2-bb11020fd8a9" />

| Topic | Description |
|---|---|
| Protecting the screen | While the UI runs, stdout and stderr go to the log file and the UI draws to a private terminal handle. Printing libraries and audio frameworks that log to stderr can't corrupt the screen. A panic on a background thread (e.g. inside librespot) becomes an error toast instead of taking the terminal down. An exit handler restores the terminal even if a library calls `exit()`. |
| OS media integration (`src/media.rs`) | Uses [souvlaki](https://github.com/Sinono3/souvlaki) on a dedicated thread. Metadata is sent when the track changes. Playback state is sent when it changes or drifts. Media keys come back as events and are handled like Spotr's own shortcuts. |
| One event loop | Terminal input runs on its own thread. Network work (Spotify Web API, cover downloads, LRCLIB, OAuth) runs as async tasks. Everything reports to a single loop that owns the app state. This keeps the UI responsive. Frames are drawn at the configured FPS while something is animating and a few times per second otherwise. |
| Spotify Web API (`src/spotify/api.rs`) | A small reqwest client. It refreshes tokens automatically, retries after `429 Retry-After` and turns Spotify's errors into readable messages. Authentication (`src/spotify/auth.rs`) is the OAuth Authorization Code flow with PKCE. A one-shot HTTP listener on `127.0.0.1` catches the redirect. No client secret is involved. |
| Playback state | Polled about once a second and interpolated between polls for a smooth progress bar. When the built-in player is active, its own events give a finer clock. Actions update the UI immediately and reconcile with Spotify afterwards. |
| Built-in player (`src/player.rs`) | Runs librespot's `Spirc` (Spotify Connect) and `Player` with a custom audio sink. The sink resamples from 44.1 kHz to the device's rate (scaled by the speed factor), plays through `cpal` and copies what is actually heard into a ring buffer. The visualizer (`src/visualizer.rs`) runs a windowed FFT on that buffer and maps it to log-spaced bands with fast attack, slow decay and falling peaks. |
| Cover art (`src/images.rs`) | Downloaded once and cached on disk. Image mode uses [ratatui-image](https://github.com/benjajaja/ratatui-image), which detects Kitty, iTerm2 or Sixel support at startup. Text mode downsamples the image (optionally to a coarse grid first, for chunky pixels) and draws it with `▀` half blocks in true color, an ASCII brightness ramp or braille dots. |
| ASCII fonts (`src/font.rs`, `src/figlet.rs`) | `block` and `slim` are tiny bitmap fonts (3×5 and 3×4 pixels per letter) drawn with half-block characters. The other fonts are FIGlet / TOIlet files, parsed at startup and laid out with "fitting" (letters slide together until they touch). Blank rows are trimmed, so a 6-line FIGlet font like ANSI Compact takes 3 lines. Accented letters fall back to an ASCII transliteration when a font lacks them (Beyoncé still renders). |
| Theme | Spotr uses only the 16 ANSI palette colors plus "default" foreground and background. Your terminal theme decides how it looks. Only cover art uses true color. |
| Search memory | Search keeps its query, filter, results, page stack (album → artist → …) and every list position in memory. Closing search only hides it. |

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

**"No active device":** Spotify needs somewhere to play. Open Spotify on any device, enable the built-in player, or pick a device with `Ctrl+D`.

**"Spotify Premium is required":** Spotify only allows playback control for Premium accounts. Browsing and library features still work.

**`INVALID_CLIENT: Invalid redirect URI` in the browser:** The Redirect URI in your Spotify app must be exactly `http://127.0.0.1:8888/callback`. Spotify no longer accepts `localhost`.

**The built-in player doesn't play any sound:** Run `spotr --test-audio`. It plays a one-second tone through the same audio path and reports the device and timings. If it fails, pick another output in `Ctrl+D` (*Built-in player output*).

**The built-in player asks to sign in again / won't start:** Run `player login`. This does a one-time browser sign-in for streaming, after which the credentials are cached under the cache directory. `player restart` reconnects. Run with `--debug` and check the log for details.

**Cover art shows as text even though my terminal supports images:** Detection relies on the terminal answering a query at startup. Inside tmux, enable passthrough (`set -g allow-passthrough on`). Over slow SSH links, detection can time out. `Ctrl+T` toggles modes, and *Settings → Look → Cover art* shows what was detected.

**Startup takes about a second / the first key is ignored in an unusual terminal:** Some terminals never answer the graphics query, so Spotr waits for a timeout. Start with `--text-art` to skip detection.

**"Spotify refused this (Forbidden)":** Since February 2026, Spotify only gives *development-mode* apps some Web API endpoints, and only for users on the app's allowlist (*User Management* in the dashboard, up to 5 users). The app owner also needs Premium. Spotr uses the endpoints that are still available:

- Liking, saving albums and following artists use the new unified library API (`/me/library`).
- Playlist songs come from `/playlists/{id}/items`.
- Artists' *Popular* songs come from a search, because Spotify removed the top-tracks endpoint.
- Search returns at most 10 results per type (load more by scrolling in a single-type filter).

**A playlist shows no songs:** Spotify now only lists the songs of playlists you own or collaborate on. Others still play fine with the **Play** button at the top of their page.

**Logs:** `spotr --debug` writes `spotr.log` to the cache directory (`spotr --print-paths`).

## Development

```bash
cargo run -- --demo        # try the UI without an account
cargo test                 # unit tests (fonts, keys, commands, LRC parsing, FFT…)
cargo clippy               # lints
cargo run -- --test-audio  # play a test tone through the built-in player's audio path
cargo run -- --test-connect  # connect librespot with a dummy token: should end in "Bad credentials"
```

The `local-player` feature (on by default) pulls in librespot and cpal. Build with `--no-default-features` for a lighter, Web-API-only build.

Contributions are welcome.

## License

Spotr is free software: you can redistribute it and/or modify it under the terms of the **GNU General Public License, version 3** (or, at your option, any later version). See [`LICENSE`](LICENSE).

Spotr is not affiliated with, endorsed by, or connected to Spotify AB. "Spotify" is a trademark of Spotify AB. Lyrics are provided by LRCLIB.
