//! Spotr: A better Spotify client, for the terminal.
//!
//! Copyright (C) 2026 Spotr contributors
//!
//! This program is free software: you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation, either version 3 of the License, or (at your option)
//! any later version. See the LICENSE file for details.

mod anim;
mod app;
mod audio;
mod commands;
mod config;
mod figlet;
mod font;
mod images;
mod keys;
mod logger;
mod media;
mod lyrics;
mod player;
mod spotify;
mod term;
mod textinput;
mod theme;
mod ui;
mod visualizer;

use std::io;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{EnableBracketedPaste, EnableMouseCapture};
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen, enable_raw_mode};
use crossterm::{cursor, execute};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui_image::picker::Picker;

use crate::app::{App, AppEvent};
use crate::config::{Config, Paths};

const HELP: &str = "\
Spotr — a better Spotify client, for the terminal

USAGE:
    spotr [OPTIONS]

OPTIONS:
    --demo             Explore Spotr with a fictional catalog (no account needed)
    --setup            Run the first-time setup again
    --no-player        Don't start the built-in player this session
    --no-media         Don't report playback to the OS (Now Playing / media keys)
    --text-art         Force text cover art (skip terminal graphics detection)
    --debug            Write a debug log to the cache directory
    --test-audio       Play a short test tone through the built-in player's audio output
    --print-paths      Show where config, cache and logs live, then exit
    --reset            Delete the config and saved login, then exit
    -h, --help         Show this help
    -V, --version      Show the version

Inside Spotr press ? for all keyboard shortcuts, or type `help`.";

struct Args {
    setup: bool,
    no_player: bool,
    text_art: bool,
    debug: bool,
    demo: bool,
    no_media: bool,
}

impl Args {
    /// `--no-media` turns the OS integration off for this session.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn media_disabled(&self) -> bool {
        self.no_media
    }
}

fn parse_args() -> Option<Args> {
    let mut a = Args { setup: false, no_player: false, text_art: false, debug: false, demo: false, no_media: false };
    let paths = Paths::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--setup" | "--onboarding" => a.setup = true,
            "--no-player" => a.no_player = true,
            "--text-art" => a.text_art = true,
            "--debug" => a.debug = true,
            "--demo" => a.demo = true,
            "--no-media" => a.no_media = true,
            "--print-paths" => {
                println!("config  {}", paths.config_file().display());
                println!("token   {}", paths.token_file().display());
                println!("cache   {}", paths.cache_dir.display());
                println!("log     {}", paths.log_file().display());
                return None;
            }
            "--reset" => {
                let _ = std::fs::remove_file(paths.config_file());
                let _ = std::fs::remove_file(paths.token_file());
                let _ = std::fs::remove_dir_all(paths.librespot_dir());
                println!("Spotr's config and saved logins were removed.");
                return None;
            }
            "--test-connect" => {
                // Diagnostic: start the built-in player with a dummy token (expected to fail auth).
                let rt = tokio::runtime::Runtime::new().expect("runtime");
                let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
                let dir = std::env::temp_dir().join("spotr-test-connect");
                let opts = player::LocalOptions {
                    device_name: "Spotr test".into(),
                    device_id: "0".repeat(40),
                    bitrate: 160,
                    normalisation: false,
                    cache_dir: dir,
                    audio: Default::default(),
                    volume: 70,
                };
                let r = rt.block_on(player::start(opts, Some("not-a-real-token".into()), tx));
                println!("connect result: {:?}", r.err().map(|e| e.to_string()));
                return None;
            }
            "--test-audio" => {
                #[cfg(feature = "local-player")]
                match player::self_test() {
                    Ok(report) => println!("audio OK\n{report}"),
                    Err(e) => println!("audio FAILED: {e:#}"),
                }
                #[cfg(not(feature = "local-player"))]
                println!("built without the local-player feature");
                return None;
            }
            "-h" | "--help" => {
                println!("{HELP}");
                return None;
            }
            "-V" | "--version" => {
                println!("spotr {}", env!("CARGO_PKG_VERSION"));
                return None;
            }
            other => {
                eprintln!("spotr: unknown option `{other}` (try --help)");
                std::process::exit(2);
            }
        }
    }
    Some(a)
}

/// Clear the screen and force a full redraw.
///
/// `Terminal::clear` asks the terminal for the cursor position, which races with
/// the input thread; resizing to the same size clears without reading stdin.
fn full_clear<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>) -> Result<()>
where
    B::Error: Send + Sync + 'static,
{
    let size = terminal.size()?;
    terminal.resize(ratatui::layout::Rect::new(0, 0, size.width, size.height))?;
    Ok(())
}

fn main() -> Result<()> {
    // Everything uses rustls with ring (reqwest is built without a provider of
    // its own, which keeps the slow-to-compile aws-lc out of the build). Install
    // it as the process default before any client is created.
    let _ = rustls::crypto::ring::default_provider().install_default();
    term::install_panic_hook();
    let Some(args) = parse_args() else { return Ok(()) };
    let paths = Paths::new();
    paths.ensure()?;
    font::init(Some(&paths.config_dir.join("fonts")));
    if args.debug {
        logger::init(&paths.log_file(), log::LevelFilter::Debug);
    } else {
        logger::init(&paths.log_file(), log::LevelFilter::Warn);
    }

    let mut cfg = Config::load(&paths).unwrap_or_else(|e| {
        eprintln!("spotr: couldn't read config ({e:#}); using defaults");
        Config::default()
    });
    cfg.sanitize();
    if args.setup {
        cfg.general.onboarded = false;
    }
    let _ = cfg.save(&paths);

    // On macOS the OS media integration needs an AppKit loop on the main
    // thread, so the UI runs on its own thread there.
    #[cfg(target_os = "macos")]
    if cfg.general.media_controls && !args.media_disabled() {
        std::thread::Builder::new()
            .name("spotr-ui".into())
            .spawn(move || {
                term::set_ui_thread();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_app(cfg, paths, args)));
                let code = match result {
                    Ok(Ok(())) => 0,
                    Ok(Err(e)) => {
                        eprintln!("spotr: {e:#}");
                        1
                    }
                    // The panic hook already restored the terminal and printed the message.
                    Err(_) => 101,
                };
                std::process::exit(code);
            })?;
        media::run_main_loop();
    }

    term::set_ui_thread();
    run_app(cfg, paths, args)
}

fn run_app(cfg: Config, paths: Paths, args: Args) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(run(cfg, paths, args))
}

async fn run(cfg: Config, paths: Paths, args: Args) -> Result<()> {
    enable_raw_mode().context("enabling raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableBracketedPaste, cursor::Hide)?;
    if cfg.general.mouse {
        execute!(stdout, EnableMouseCapture)?;
    }

    // Ask the terminal what graphics it supports (must happen before we read events).
    log::debug!("querying terminal graphics support");
    let picker = if args.text_art {
        Picker::halfblocks()
    } else {
        let opts = ratatui_image::picker::cap_parser::QueryStdioOptions { timeout: Duration::from_millis(1000), ..Default::default() };
        Picker::from_query_stdio_with_options(opts).unwrap_or_else(|_| Picker::halfblocks())
    };
    log::debug!("graphics: {:?}", picker.protocol_type());

    // From here on, stray output from libraries goes to the log, not the screen.
    term::redirect_output(&paths.log_file());
    // Buffer each frame so it reaches the terminal in one write.
    let backend = CrosstermBackend::new(io::BufWriter::with_capacity(1 << 16, term::out()));
    let mut terminal = Terminal::new(backend)?;
    full_clear(&mut terminal)?;

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<AppEvent>();

    // Terminal input on its own thread.
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            loop {
                match crossterm::event::read() {
                    Ok(ev) => {
                        if tx.send(AppEvent::Input(ev)).is_err() {
                            break;
                        }
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(50)),
                }
            }
        });
    }

    let http = reqwest::Client::builder()
        .user_agent(concat!("spotr/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .build()?;
    let images = images::ImageStore::new(picker, paths.cache_dir.join("covers"));
    let mut app = App::new(cfg, paths, tx, http, images, args.demo);
    // Session-only: not saved to the config.
    app.no_player |= args.no_player;
    if app.cfg.general.media_controls && !args.no_media {
        app.media = media::start(app.tx.clone());
    }
    app.start();

    let result: Result<()> = async {
        loop {
            if app.needs_clear {
                app.needs_clear = false;
                full_clear(&mut terminal)?;
            }
            // Draw atomically: terminals that support synchronized output show the
            // whole frame at once, and the cursor stays hidden while cells are
            // written (otherwise it visibly jumps around while the visualizer
            // animates).
            crossterm::queue!(terminal.backend_mut(), BeginSynchronizedUpdate, cursor::Hide)?;
            terminal.draw(|f| ui::draw(f, &mut app))?;
            crossterm::execute!(terminal.backend_mut(), EndSynchronizedUpdate)?;

            let timeout = app.frame_interval();
            tokio::select! {
                ev = rx.recv() => {
                    if let Some(ev) = ev {
                        app.handle_event(ev);
                    }
                    // Drain whatever else is queued before redrawing.
                    while let Ok(ev) = rx.try_recv() {
                        app.handle_event(ev);
                    }
                }
                _ = tokio::time::sleep(timeout) => {}
            }
            app.tick();
            app.onboarding_timeout_check();
            if !app.running {
                break;
            }
        }
        Ok(())
    }
    .await;

    if let Some(p) = app.local.take() {
        p.shutdown();
        // Dropping the player joins its audio thread; never block the exit on it.
        std::thread::spawn(move || drop(p));
    }
    term::restore();
    result
}
