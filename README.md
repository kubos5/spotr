# Spotr

**A better Spotify client, for the terminal.**

> [!TIP]
> **Install** (macOS & Linux)
> ```bash
> curl -fsSL https://raw.githubusercontent.com/kubos5/spotr/main/install.sh | bash
> ```
> **Install** (Windows, PowerShell)
> ```powershell
> irm https://raw.githubusercontent.com/kubos5/spotr/main/install.ps1 | iex
> ```

> [!NOTE]
> Also available on **Homebrew**
> 
> _I put a lot of effort into the install script though..._
> ```bash
> brew install kubost/tap/spotr
> ```

> See [more installation options](#installation) below

---

<img width="1941" height="1276" alt="CleanShot 2026-10-10 at 16 41 55@2x" src="https://github.com/user-attachments/assets/6c158526-368d-4460-9d58-be2aa75acd12" />

Spotr is a super lightweight, blazingly fast and keyboard-first Spotify client, built in Rust.
It features polished animations throughout the UI, so that every action feels super satisfying.

You can use Spotr as a Player device, or to control other Spotify devices linked to your account.

No Spotify? Spotr also plays the music files in a folder: `spotr -d ~/Music` (see [Local files](#local-files)).

Want to try it out? Run `spotr --demo` to take a look at the UI without an account.

---

<br><br><br><br>

## Contents

- [Features](#features)
- [Requirements](#requirements)
- [Installation](#installation)
  - [Uninstall](#uninstall)
- [First launch](#first-launch)
- [Using Spotr](#using-spotr)
  - [Layout](#layout)
  - [Keyboard](#keyboard)
  - [Commands](#commands)
  - [Nice to know](#nice-to-know)
  - [Queue](#queue)
  - [Local files](#local-files)
  - [Lyrics](#lyrics)
  - [Devices](#devices)
  - [Mouse](#mouse)
- [Configuration](#configuration)
- [Command-line options](#command-line-options)
- [How it works](#how-it-works)
- [Troubleshooting](#troubleshooting)
- [Development](#development)
- [License](#license)

<br><br><br><br>

## Features

| Feature                 | Description                                                                                                                                                         |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Now playing**         | The song name is drawn in ASCII art. Use any of the 9 built-in fonts, or [add your own](#title-fonts).                                                              |
| **The command bar**     | The main way you'll interact with Spotr is the Command bar. Everything is a command. `play`/`pause`, `next`/`previous`, `skip 10`, `volume 60`, and many more.      |
| **The search bar**      | To search up songs, artists, albums or playlists, you can use the Search bar. Focus it with `/`, or the `search` command. It uses Spotify's own search, the one in the Spotify apps. The **Liked Songs** tab searches just the songs you like. |
| **Two queues**          | Spotr's own queue, which you can rearrange freely, or Spotify's queue, where Spotify's radio picks what plays after a song you searched. `Tab` in the queue switches. |
| **Local files**         | Play the music in a folder, no account needed: `spotr -d`. Likes, playlists, play counts and a resume point live in a `.spotr/` folder next to your music.          |
| **OS controls**         | Spotr reports Now playing data to the Operating System, so that you can control it through media keys.                                                              |
| **Cover art rendering** | If you don't like images in the terminal, or your terminal doesn't support them, you can render the cover art as text instead, choosing from the 4 built-in styles. |
| **Visualizer**          | Above the command bar, you'll see a pretty real time audio visualizer, available in 5 different styles.                                                             |

... and many more! [Try it for yourself](#installation)

<br><br><br><br>

## Requirements

_For Spotify:_
- Spotify Premium
- access to the Spotify Developer Console

_For local files:_ nothing — any folder of MP3, FLAC, Ogg Vorbis, WAV, AIFF or M4A (AAC / ALAC) files.

_For compiling:_
- Rust 1.90+
- a C compiler

_On Linux:_
- ALSA development headers for the built-in player (`libasound2-dev` on Debian/Ubuntu, `alsa-lib-devel` on Fedora, `alsa-lib` on Arch)
- `pkg-config`

_Optional:_
- a terminal with a graphics protocol, such as kitty, WezTerm, iTerm2 or Ghostty.
  Every other terminal gets the text renderer.

<br><br><br><br>

## Installation

Custom installer (macOS & Linux)

> [!TIP]
> **Recommended**
> 
> macOS & Linux
> ```bash
> curl -fsSL https://raw.githubusercontent.com/kubos5/spotr/main/install.sh | bash
> ```
> Windows, PowerShell
> ```powershell
> irm https://raw.githubusercontent.com/kubos5/spotr/main/install.ps1 | iex
> ```

---

Homebrew (macOS & Linux)

```bash
brew install kubos5/tap/spotr
```

---

Release installers (cargo-dist)

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/kubos5/spotr/releases/latest/download/spotr-installer.ps1 | iex"
```

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/kubos5/spotr/releases/latest/download/spotr-installer.sh | sh
```

---

Compile manually

```bash
git clone https://github.com/kubos5/spotr.git
cd spotr
cargo install --path . --locked
```

This puts the binary in `~/.cargo/bin`.

`--locked`: `Cargo.lock` pins a build-time dependency of librespot (`vergen` 9.0.x), because a newer one breaks librespot's build script. 

Rebuilding after `git pull` takes most of a minute with the fully optimized `release` profile. `--profile local` (what the install script uses) skips link-time optimization, so updates take seconds:

```bash
cargo install --path . --locked --profile local
```

---

Build without installing:

```bash
cargo build --release
```

The binary is then `target/release/spotr`.

---

Build without audio features

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

---

### Uninstall

> [!CAUTION]
> Changed your mind?
> 
> **Uninstallers**
> 
> macOS & Linux
> ```bash
> curl -fsSL https://raw.githubusercontent.com/kubos5/spotr/main/uninstall.sh | bash
> ```
> Windows, PowerShell
> ```powershell
> irm https://raw.githubusercontent.com/kubos5/spotr/main/uninstall.ps1 | iex
> ```

<br><br><br><br>

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

<br><br><br><br>

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
| `Ctrl+E` or `F4`                      | Queue (`Tab` there switches between Spotr's and Spotify's queue)                                              |
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
- `Tab` to switch between Spotr's queue and Spotify's queue

In a local library:
- `Tab` switches the list: Songs, Albums, Artists, Playlists, Liked Songs, Recent
- `Enter` plays (the list keeps playing from there), `*` likes, `Ctrl+A` queues
- `Del` on a local playlist's page removes the song from it

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
| `queue mode [spotr\|spotify]`                                 | Who manages "up next": Spotr's queue or Spotify's queue and radio                                                                                          |
| `local [folder]` · `online`                                   | Open a folder as a local library (default: the current one) · back to Spotify                                                                             |
| `init [folder]`                                               | Set up a folder as a library (creates `.spotr/` in it)                                                                                                     |
| `rescan`                                                      | Look for new, changed or removed files                                                                                                                     |
| `sort <artist\|title\|album\|added\|plays\|length>`             | How a local library's song list is ordered                                                                                                                 |
| `playlist <new\|delete\|rename> <name>`                       | Local playlists (`addto <name>` adds the selected or current song)                                                                                        |
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
  `All`, `Songs`, `Artists`, `Albums`, `Playlists` and `Liked Songs`
- the **Liked Songs** tab searches only the songs you like (with nothing typed, it lists them all). Playing one plays your Liked Songs from there
- songs you like have a `♥` before their title in every list, and show up in red in search results and your local library (*Settings → Search → Liked songs in red*)
- you can paste a Spotify link anywhere (outside a text field) to play it.

### Queue

Spotr has two queues. Switch with `Tab` in the queue view (`Ctrl+E`), `queue mode spotr` / `queue mode spotify`, or *Settings → Playback → Queue*.

**Spotr's queue** (the default). The Spotify Web API can add to Spotify's queue but can't remove or reorder anything in it. So Spotr keeps **its own queue**, which you can edit freely:

- add songs with `Ctrl+A` on a search result, `queue add <song>`, or `queue add` (current)
- reorder songs with `Shift+↑/↓`
- remove songs with `Del`
- press `Enter` to play a song right away

Shortly before the current song ends (3 seconds by default, see *Settings → Playback → Queue handoff*), Spotr hands the next song to Spotify, so it plays seamlessly on any device. `next` also takes from your queue first.

Anything already in Spotify's own queue (for example, added from your phone) is shown read-only underneath.

What plays after a song you picked in search is up to *Settings → Playback → Songs from search*: the rest of its **album**, a Spotify **radio** of similar songs, or just that **single** song.

**Spotify's queue.** Spotify manages "up next", like in the Spotify apps:

- `Ctrl+A` adds songs straight to Spotify's queue (songs waiting in Spotr's queue move over when you switch)
- a song you play from search starts **Spotify's song radio**: the song, then about 50 songs Spotify picks to go with it
- the queue view lists what Spotify will play next; `Enter` skips ahead to a song

Spotify's queue can't be reordered from outside Spotify, so `Shift+↑/↓` and `Del` only work in Spotr's queue.

Song radio and Spotify's own search use the built-in player's sign-in. Without it, search falls back to the Web API and songs play on their own.

### Local files

Spotr can play the music files in a folder instead of Spotify. No account, no network needed.

```bash
spotr -d                  # the current folder
spotr -d ~/Music/vinyl    # another folder
```

The first time, Spotr asks before setting the folder up. It then creates a `.spotr/` folder inside it:

```
my_music/
    .spotr/        likes, playlists, play counts, where you left off, a scan cache
    file1.mp3
    file2.flac
    …
```

Your music files are never changed. Delete `.spotr/` to reset the library.

**Local-only mode.** If you don't use Spotify at all, turn on *Settings → Local → Local-only mode*, or pick **Use local-only mode** on the Client ID step of the first-time setup (it skips the Spotify sign-in and player steps). Then plain `spotr` always acts like `spotr -d`: it opens the folder you start it in, and never asks you to sign in to Spotify. Spotify-only settings are grayed out. `online` still switches to Spotify for that session.

Spotr warns you before setting up your home folder, the root of the disk, or a folder outside your home folder.

Once a folder has a `.spotr/` folder, launching `spotr` inside it opens it as a local library automatically. Elsewhere, Spotr starts with Spotify as usual. Switch at any time with `local [folder]` and `online`.

Instead of "Nothing is playing", a local library shows its songs:

- `Tab` switches between **Songs**, **Albums**, **Artists**, **Playlists**, **Liked Songs** and **Recent**
- `Enter` plays, and the list keeps playing from there; `*` likes or unlikes; `Ctrl+A` queues
- **Continue** at the top picks up where you left off (song, position, queue and shuffle)
- `sort title` (or `artist`, `album`, `added`, `plays`, `length`) orders the songs

Everything else works the same: the search bar searches the library as you type (accents and small typos are fine), album and artist pages, the queue, shuffle and loop, speed, the real visualizer, OS media keys.

Other details:

- **Tags**: title, artist, album, album artist, track and disc numbers, year and genre are read from the files. Untagged files are named from their path (`Artist/Album/01 - Title.mp3`, `Artist - Title.mp3`).
- **Cover art**: embedded in the file, or a `cover.jpg` / `folder.png`… next to it.
- **Lyrics**: an `.lrc` file next to the song (`01 - Title.lrc`), then lyrics embedded in its tags, then LRCLIB (turn off in *Settings → Local*).
- **Playlists**: `playlist new Road trip`, then `addto road` on a selected song. `Del` on a playlist's page removes a song.
- **Plays** count after half a song or 30 seconds, for *Recent*, *Most played* and `sort plays`.
- **Song radio**: with *Songs from search* set to *Radio*, a searched song is followed by songs from the same artist and genre and your favorites.
- **Scanning** reads only new or changed files after the first time. `rescan` picks up changes while Spotr runs.
- **Formats**: MP3, FLAC, Ogg Vorbis, WAV, AIFF, M4A (AAC and ALAC), CAF and MKA. Opus isn't supported yet.

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

Turn it off under *Settings → Playback → OS media controls* (applies after a restart) or for just one session with `--no-media`.

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

<br><br><br><br>

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
local_only = false          # `spotr` always acts like `spotr -d` (no Spotify)
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
queue_mode = "spotr"        # spotr | spotify (Spotify's queue and song radio)
search_context = "album"    # album | radio | single: what plays after a searched song

[lyrics]
ascii = false               # true = ASCII-art lyrics
font = ""                   # "" = same as title_font

[search]
live = true
default_filter = "all"      # all | tracks | artists | albums | playlists | liked
highlight_liked = true      # liked songs in red in search results and the local library
remember_position = true
close_on_play = true

[local]
sort = "artist"             # artist | title | album | recently-added | most-played | length
online_lyrics = true        # look up LRCLIB when a local song has no .lrc file
resume = true               # offer to continue where you left off

[keys]
play_pause = ["space"]
lyrics = ["ctrl+l", "f3"]
bracket_right = ["]"]
next = ["f8"]
```

### Title fonts

Built in:

| Font                     | Lines | Look                               |
| ------------------------ | ----- | ---------------------------------- |
| `ansi-compact` (default) | 3     | ANSI Compact by Loïc Cressot       |
| `block`                  | 3     | Spotr's own chunky half-block font |
| `slim`                   | 2     | A smaller version of `block`       |
| `future`                 | 3     | Thin box-drawing letters           |
| `pagga`                  | 3     | Blocks on a shaded background      |
| `emboss`                 | 3     | Heavy box-drawing letters          |
| `small-block`            | 3–4   | Quadrant blocks                    |
| `small-braille`          | 2     | Braille dots, the most compact     |
| `mini`                   | 3–4   | Classic FIGlet "Mini"              |
| `plain`                  | 1     | No ASCII art                       |

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

<br><br><br><br>

## Command-line options

```
spotr [OPTIONS]

    -d, --directory [FOLDER]
                    Play the music files in FOLDER (default: the current folder)
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

<br><br><br><br>

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
| Liked Songs (`src/app/liked.rs`) | To show a `♥` on every liked song and search them, Spotr keeps your whole Liked Songs list, loaded 50 at a time in the background and saved in the cache folder. On later launches one request is enough: if Spotify's count and its newest 50 songs still match the saved copy, nothing changed. Likes made in Spotr update it right away. |
| Spotify's search and radio (`src/spotify/internal.rs`) | The Web API's `/search` ranks results differently from the Spotify apps. The built-in player is signed in like a real Spotify client, so Spotr borrows its session to run the apps' own search query (Spotify's ranking and top result, personalized) and to fetch song radio mixes. If that fails, Spotr falls back to the Web API. |
| Local files (`src/local/`) | `library.rs` walks the folder and reads tags with [Symphonia](https://github.com/pdeljanov/Symphonia), on several threads, caching them in `.spotr/index.json`. Local songs use Spotr's Spotify models with `local:` URIs, so every list, page and the now-playing view work unchanged. `engine.rs` decodes on its own thread into a short ring buffer; the `cpal` callback resamples it to the device rate (times the speed factor) and feeds the visualizer, like the built-in player's sink. `store.rs` keeps likes, plays, playlists and the resume point in `.spotr/`. |

Source layout:

```
src/
  main.rs          terminal setup, CLI flags, event loop
  app/             state & logic: input, commands, playback, browse, settings, onboarding
  ui/              drawing: now playing, browse, bars, views, overlays, effects, onboarding
  spotify/         Web API client, OAuth (PKCE), Spotify's own search & radio, models, demo catalog
  local/           local files: library scan, .spotr store, audio engine (feature "local-files")
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
install.sh, uninstall.sh     installer for macOS & Linux
install.ps1, uninstall.ps1   installer for Windows (PowerShell 5.1+)
  anim.rs, theme.rs, textinput.rs, audio.rs, logger.rs
assets/fonts/      bundled FIGlet / TOIlet fonts (+ licenses)
```

<br><br><br><br>

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
- Web API search returns at most 10 results per type (load more by scrolling in a single-type filter). Spotify's own search, used with the built-in player, has no such limit.

**A playlist shows no songs:** Spotify now only lists the songs of playlists you own or collaborate on. Others still play fine with the **Play** button at the top of their page.

**Local files: "No music found":** Spotr looks in the folder and every subfolder (except hidden ones) for MP3, FLAC, Ogg Vorbis, WAV, AIFF, M4A, CAF and MKA files. Opus files aren't supported yet. Run `rescan` after adding music.

**Local files: wrong artist or album names:** Spotr uses the files' tags. Untagged files get their names from the path, which works best as `Artist/Album/01 - Title.mp3`. Fix the tags with any tag editor and run `rescan`.

**Logs:** `spotr --debug` writes `spotr.log` to the cache directory (`spotr --print-paths`).

<br><br><br><br>

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

<br><br><br><br>

## License

Spotr is free software: you can redistribute it and/or modify it under the terms of the **GNU General Public License, version 3** (or, at your option, any later version). See [`LICENSE`](LICENSE).

Spotr is not affiliated with, endorsed by, or connected to Spotify AB. "Spotify" is a trademark of Spotify AB. Lyrics are provided by LRCLIB.
