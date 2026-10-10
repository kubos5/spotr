//! First-run walkthrough screens.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};

use super::widgets::*;
use crate::anim;
use crate::app::onboarding::{AuthState, BEHAVIOR_ROWS, LOOK_ROWS, OnboardClick, PREVIEW_IMAGE, PlayerSetup, Step};
use crate::app::{App, Click};
use crate::config::CoverMode;

pub fn draw(app: &mut App, buf: &mut Buffer, area: Rect) {
    let Some(o) = app.onboarding.as_ref() else { return };
    let step = o.step;
    let entered = o.entered;
    let steps = o.steps();
    let idx = o.index();
    let dir = o.dir;
    let th = app.theme.clone();

    // Card.
    let w = area.width.min(96);
    let h = area.height.min(30);
    let card = centered(area, w, h);
    let inner = panel(app, buf, card, Some(if o.only_login { "Sign in" } else { "Welcome to Spotr" }), true);

    // Step indicator.
    let mut x = inner.x + 2;
    for (i, s) in steps.iter().enumerate() {
        let done = i < idx;
        let cur = i == idx;
        let dot = if th.unicode { if done { "●" } else if cur { "◉" } else { "○" } } else if done || cur { "*" } else { "o" };
        let label = format!("{dot} {}", s.title());
        let style = if cur { th.accent_bold() } else if done { th.accent() } else { th.muted() };
        let lw = width(&label);
        if x + lw >= inner.right() {
            break;
        }
        put(buf, x, inner.y, &label, style, lw);
        if done {
            app.clicks.push((Rect { x, y: inner.y, width: lw, height: 1 }, Click::Onboard(OnboardClick::Step(i))));
        }
        x += lw + 3;
    }

    // Content slides in from the side we're moving toward.
    let t = app.motion.t(entered, 300);
    let slide = ((1.0 - anim::ease_out_cubic(t)) * 10.0) as i32 * dir;
    let content = Rect {
        x: (inner.x as i32 + 3 + slide).max(inner.x as i32) as u16,
        y: inner.y + 2,
        width: inner.width.saturating_sub(6),
        height: inner.height.saturating_sub(5),
    };
    match step {
        Step::Welcome => welcome(app, buf, content),
        Step::App => app_step(app, buf, content),
        Step::SignIn => signin(app, buf, content),
        Step::Player => player(app, buf, content),
        Step::Look => look(app, buf, content),
        Step::Behavior => behavior(app, buf, content),
        Step::Done => done(app, buf, content),
    }

    // Error line.
    if let Some((e, at)) = app.onboarding.as_ref().and_then(|o| o.error.clone()) {
        let dx = anim::shake(app.motion.t(at, 500));
        let s = format!("{} {e}", th.icon(crate::theme::Icon::Cross));
        put(buf, (inner.x as i32 + 3 + dx as i32) as u16, inner.bottom() - 3, &ellipsize(&s, inner.width as usize - 6), th.error(), inner.width - 4);
    }

    // Buttons.
    let by = inner.bottom() - 1;
    if idx > 0 {
        let back = if th.unicode { "‹ Back (Esc)" } else { "< Back (Esc)" };
        put(buf, inner.x + 2, by, back, th.dim(), 14);
        app.clicks.push((Rect { x: inner.x + 2, y: by, width: width(back), height: 1 }, Click::Onboard(OnboardClick::Back)));
    }
    let next_label = match step {
        Step::Welcome => "Get started ⏎",
        Step::SignIn => match app.onboarding.as_ref().map(|o| &o.auth) {
            Some(AuthState::Done(_)) => "Continue ⏎",
            Some(AuthState::Waiting { .. }) => "Waiting…",
            _ => "Sign in with Spotify ⏎",
        },
        Step::Player => match app.onboarding.as_ref().map(|o| &o.player) {
            Some(PlayerSetup::Connecting(_)) => "Connecting…",
            Some(PlayerSetup::NeedsLogin) if app.cfg.general.local_player => "Sign in player ⏎",
            Some(PlayerSetup::Idle | PlayerSetup::Failed(_)) if app.cfg.general.local_player => "Start player ⏎",
            _ => "Continue ⏎",
        },
        Step::Done => "Start listening ⏎",
        _ => "Continue ⏎",
    };
    let next_label = if th.unicode { next_label.to_string() } else { next_label.replace('⏎', "(Enter)") };
    let nw = width(&next_label) + 4;
    let nx = inner.right().saturating_sub(nw + 2);
    let pulse = anim::pulse(1800);
    let nstyle = if pulse > 0.5 || !app.motion.enabled { th.selected() } else { th.selected().add_modifier(Modifier::DIM) };
    put(buf, nx, by, &format!("  {next_label}  "), nstyle, nw);
    app.clicks.push((Rect { x: nx, y: by, width: nw, height: 1 }, Click::Onboard(OnboardClick::Next)));
}

fn paragraph(buf: &mut Buffer, area: Rect, y: u16, text: &str, style: Style) -> u16 {
    // Simple word wrap.
    let mut line = String::new();
    let mut yy = y;
    for word in text.split_whitespace() {
        if width(&line) as usize + word.len() + 1 > area.width as usize && !line.is_empty() {
            put(buf, area.x, yy, &line, style, area.width);
            yy += 1;
            line.clear();
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        put(buf, area.x, yy, &line, style, area.width);
        yy += 1;
    }
    yy - y
}

fn welcome(app: &mut App, buf: &mut Buffer, area: Rect) {
    let th = app.theme.clone();
    let entered = app.onboarding.as_ref().map(|o| o.entered).unwrap_or(app.started);
    // Everything below is laid out as if the title were 3 rows tall.
    let y = area.y + 2;
    let y = y + brand_title(app, buf, Rect { x: area.x, y, width: area.width, height: 3 }, "SPOTR", entered) - 3;
    // Rainbow underline wave.
    let pal = th.spectrum();
    let ww = 30u16.min(area.width);
    let x0 = area.x + (area.width - ww) / 2;
    for i in 0..ww {
        let lvl = (((anim::clock() * 5.0 + i as f32 * 0.45).sin() * 0.5 + 0.5) * 7.0) as usize;
        let ch = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'][lvl.min(7)];
        if let Some(c) = buf.cell_mut((x0 + i, y + 4)) {
            c.set_char(if th.unicode { ch } else { '~' }).set_style(Style::new().fg(pal[i as usize % pal.len()]));
        }
    }
    let tag = "A better Spotify client, for the terminal.";
    let t = app.motion.t(entered, 1400);
    let shown = anim::typewriter(tag, ((t - 0.25) / 0.75).clamp(0.0, 1.0));
    put(buf, area.x + area.width.saturating_sub(width(tag)) / 2, y + 6, &shown, th.bold(), area.width);
    let body = Rect { x: area.x + 6, y: y + 9, width: area.width.saturating_sub(12), height: 8 };
    let mut yy = body.y;
    for (k, d) in [
        ("1.", "Connect Spotr to your Spotify account (Premium needed for playback control)."),
        ("2.", "Optionally turn Spotr itself into a Spotify Connect speaker."),
        ("3.", "Pick how things look — you can change everything later in settings."),
    ] {
        put(buf, body.x, yy, k, th.accent_bold(), 3);
        yy += paragraph(buf, Rect { x: body.x + 3, width: body.width - 3, ..body }, yy, d, th.dim());
    }
    yy += 1;
    paragraph(buf, body, yy, "Everything works with the keyboard alone. Press Enter to begin.", th.muted());
}

fn app_step(app: &mut App, buf: &mut Buffer, area: Rect) {
    let th = app.theme.clone();
    let mut y = area.y;
    put(buf, area.x, y, "Create your Spotify app", th.accent_bold(), area.width);
    y += 2;
    y += paragraph(buf, area, y, "Spotify requires every client to have its own developer app. It takes a minute and it's free:", Style::new());
    y += 1;
    let redirect = format!("http://127.0.0.1:{}/callback", app.cfg.general.redirect_port);
    let steps = [
        format!("Open {} (Ctrl+O) and log in.", crate::app::onboarding::DASHBOARD_URL),
        "Click “Create app”. Any name and description will do.".to_string(),
        format!("Add this Redirect URI exactly: {redirect}"),
        "Tick “Web API” (and “Web Playback SDK”), then save.".to_string(),
        "Open the app's settings and copy its Client ID below.".to_string(),
    ];
    for (i, s) in steps.iter().enumerate() {
        put(buf, area.x + 1, y, &format!("{}.", i + 1), th.accent_bold(), 3);
        y += paragraph(buf, Rect { x: area.x + 4, width: area.width - 4, ..area }, y, s, th.dim());
    }
    let link = "[ Open the dashboard ]";
    put(buf, area.x + 4, y, link, th.accent(), width(link));
    app.clicks.push((Rect { x: area.x + 4, y, width: width(link), height: 1 }, Click::Onboard(OnboardClick::OpenDashboard)));
    y += 2;
    put(buf, area.x, y, "Client ID", th.bold(), 10);
    let field = Rect { x: area.x, y: y + 1, width: area.width.min(50), height: 3 };
    let inner = panel(app, buf, field, None, true);
    let o = app.onboarding.as_ref().unwrap();
    let (text, _col) = o.client_id.view(inner.width.saturating_sub(2) as usize);
    let valid = crate::spotify::auth::valid_client_id(o.client_id.value.trim());
    if text.is_empty() {
        put(buf, inner.x + 1, inner.y, "paste it here (32 characters)", th.muted(), inner.width);
    } else {
        put(buf, inner.x + 1, inner.y, &text, th.bold(), inner.width);
    }
    let cursor_x = inner.x + 1 + width(&text);
    if anim::pulse(1000) > 0.4 {
        put(buf, cursor_x.min(inner.right() - 1), inner.y, "▏", th.accent(), 1);
    }
    let mark = if valid { format!("{} looks good", th.icon(crate::theme::Icon::Check)) } else { format!("{}/32", o.client_id.value.trim().len()) };
    put(buf, field.right() + 2, field.y + 1, &mark, if valid { th.success() } else { th.muted() }, 20);
}

fn signin(app: &mut App, buf: &mut Buffer, area: Rect) {
    let th = app.theme.clone();
    let o = app.onboarding.as_ref().unwrap();
    let auth = o.auth.clone();
    let mut y = area.y;
    put(buf, area.x, y, "Connect your account", th.accent_bold(), area.width);
    y += 2;
    match auth {
        AuthState::Idle => {
            y += paragraph(buf, area, y, "Press Enter and your browser will open Spotify's authorization page. Approve access and come back here — Spotr notices automatically.", Style::new());
            y += 1;
            paragraph(buf, area, y, "Spotr asks for permission to control playback, read your library and playlists, and save songs you like. Your login token is stored only on this computer.", th.dim());
        }
        AuthState::Waiting { url, since } => {
            let dots = ".".repeat((since.elapsed().as_millis() / 400 % 4) as usize);
            let msg = if url.is_empty() { format!("{} Checking your saved login{dots}", th.spinner(app.frame)) } else { format!("{} Waiting for you to approve in the browser{dots}", th.spinner(app.frame)) };
            put(buf, area.x, y, &msg, th.bold(), area.width);
            y += 2;
            if !url.is_empty() {
                y += paragraph(buf, area, y, "If the browser didn't open, press Ctrl+O or click below:", th.dim());
                let link = "[ Open the authorization page ]";
                put(buf, area.x + 2, y, link, th.accent(), width(link));
                app.clicks.push((Rect { x: area.x + 2, y, width: width(link), height: 1 }, Click::Onboard(OnboardClick::OpenBrowser)));
                y += 2;
                let short = ellipsize(&url, (area.width as usize).saturating_sub(2) * 2);
                paragraph(buf, area, y, &short, th.muted());
            }
        }
        AuthState::Done(name) => {
            let t = app.motion.t(o.entered, 600);
            let msg = format!("{} Signed in as {name}", th.icon(crate::theme::Icon::Check));
            let s = if app.motion.text_effects { anim::scramble(&msg, t, app.frame, th.unicode) } else { msg };
            put(buf, area.x, y, &s, th.success().add_modifier(Modifier::BOLD), area.width);
            y += 2;
            paragraph(buf, area, y, "Great! Press Enter to continue.", th.dim());
        }
        AuthState::Failed(e) => {
            put(buf, area.x, y, &format!("{} Sign-in failed", th.icon(crate::theme::Icon::Cross)), th.error(), area.width);
            y += 2;
            y += paragraph(buf, area, y, &e, Style::new());
            y += 1;
            paragraph(buf, area, y, "Check that the Redirect URI in your Spotify app matches exactly, then press Enter to retry.", th.dim());
        }
    }
}

fn option_rows(app: &mut App, buf: &mut Buffer, area: Rect, rows: &[(String, String)], selected: usize) -> u16 {
    let th = app.theme.clone();
    for (i, (label, value)) in rows.iter().enumerate() {
        let y = area.y + i as u16 * 2;
        if y >= area.bottom() {
            break;
        }
        let sel = i == selected;
        let rect = Rect { x: area.x, y, width: area.width, height: 1 };
        if sel {
            fill(buf, rect, th.selected());
        }
        let base = if sel { th.selected() } else { Style::new() };
        put(buf, area.x + 1, y, if sel { th.icon(crate::theme::Icon::Pointer) } else { " " }, base, 1);
        put(buf, area.x + 3, y, label, base, area.width / 2);
        let v = if th.unicode { format!("‹ {value} ›") } else { format!("< {value} >") };
        put(buf, area.x + area.width / 2, y, &v, if sel { base } else { th.accent() }, area.width / 2);
        app.clicks.push((rect, Click::Onboard(OnboardClick::Row(i))));
    }
    rows.len() as u16 * 2
}

fn player(app: &mut App, buf: &mut Buffer, area: Rect) {
    let th = app.theme.clone();
    let o = app.onboarding.as_ref().unwrap();
    let (row, editing, setup, name) = (o.row, o.editing_name, o.player.clone(), o.device_name.clone());
    let mut y = area.y;
    put(buf, area.x, y, "Built-in player", th.accent_bold(), area.width);
    y += 2;
    if !crate::player::AVAILABLE {
        paragraph(buf, area, y, "This build of Spotr doesn't include the built-in player (librespot). Spotr will control your other Spotify devices instead. Press Enter to continue.", th.dim());
        return;
    }
    y += paragraph(buf, area, y, "Spotr can be a Spotify Connect speaker itself, so you don't need the Spotify app open. It also powers the real-time audio visualizer and playback speed control.", Style::new());
    y += 1;
    let enabled = app.cfg.general.local_player;
    let name_val = if editing { format!("{}▏", name.value) } else { name.value.clone() };
    let rows = vec![
        ("Use the built-in player".to_string(), if enabled { "on".into() } else { "off".into() }),
        ("Device name (⏎ to edit)".to_string(), name_val),
        ("Activate on start".to_string(), if app.cfg.general.auto_activate { "on".into() } else { "off".into() }),
    ];
    y += option_rows(app, buf, Rect { y, height: 6, ..area }, &rows, row);
    y += 1;
    let (msg, style) = match setup {
        PlayerSetup::Idle => (if enabled { "Press Enter to start it.".to_string() } else { "Disabled — Spotr will control your other devices.".to_string() }, th.dim()),
        PlayerSetup::Connecting(_) => (format!("{} Connecting to Spotify…", th.spinner(app.frame)), th.bold()),
        PlayerSetup::NeedsLogin => ("The player needs a one-time sign-in of its own. Press Enter to open your browser (you're likely already logged in).".into(), th.warn()),
        PlayerSetup::Ready => (format!("{} “{}” is live as a Spotify Connect device.", th.icon(crate::theme::Icon::Check), app.cfg.general.device_name), th.success()),
        PlayerSetup::Failed(e) => (format!("Couldn't start: {e}. Enter to retry, or turn it off to skip."), th.error()),
    };
    paragraph(buf, area, y, &msg, style);
}

fn look(app: &mut App, buf: &mut Buffer, area: Rect) {
    let th = app.theme.clone();
    let row = app.onboarding.as_ref().map(|o| o.row).unwrap_or(0);
    put(buf, area.x, area.y, "Make it yours", th.accent_bold(), area.width);
    let c = &app.cfg.appearance;
    let cover = if c.cover_mode == CoverMode::Image && !app.images.graphics { "Image (not supported here)".to_string() } else { c.cover_mode.label().to_string() };
    let values = vec![
        cover,
        c.text_art.label().to_string(),
        if c.pixelation == 0 { "off".into() } else { format!("{0}×{0}", c.pixelation) },
        crate::font::label(app.title_font()).to_string(),
        c.accent.label().to_string(),
        c.icons.label().to_string(),
    ];
    let rows: Vec<(String, String)> = LOOK_ROWS.iter().map(|s| s.to_string()).zip(values).collect();
    let left_w = area.width.saturating_sub(4) / 2;
    option_rows(app, buf, Rect { x: area.x, y: area.y + 2, width: left_w, height: area.height - 2 }, &rows, row);
    // Live preview.
    let px = area.x + left_w + 3;
    let pw = area.right().saturating_sub(px);
    let prev = Rect { x: px, y: area.y + 2, width: pw, height: area.height.saturating_sub(3) };
    let inner = panel(app, buf, prev, Some("Preview"), false);
    let ch = inner.height.saturating_sub(5).min(10);
    let crect = Rect { x: inner.x, y: inner.y, width: inner.width, height: ch };
    let born = app.onboarding.as_ref().map(|o| o.entered).unwrap_or(app.started);
    cover_preview(app, buf, crect, born);
    let font = app.title_font();
    big_title(app, buf, Rect { x: inner.x, y: inner.y + ch + 1, width: inner.width, height: crate::font::lines_for(font) }, "Night Drive", born, true, font);
    let line = format!("{} The Synthwaves", th.icon(crate::theme::Icon::Artist));
    put(buf, inner.x + inner.width.saturating_sub(width(&line)) / 2, inner.bottom() - 1, &line, th.bold(), inner.width);
}

fn cover_preview(app: &mut App, buf: &mut Buffer, area: Rect, born: std::time::Instant) {
    app.ensure_preview_image();
    cover(app, buf, area, Some(PREVIEW_IMAGE), born, false);
}

fn behavior(app: &mut App, buf: &mut Buffer, area: Rect) {
    let th = app.theme.clone();
    let row = app.onboarding.as_ref().map(|o| o.row).unwrap_or(0);
    put(buf, area.x, area.y, "How it behaves", th.accent_bold(), area.width);
    let c = &app.cfg;
    let values = vec![
        pair_text(c.playback.arrow_mode),
        pair_text(c.playback.bracket_mode),
        format!("{} s", c.playback.seek_step),
        if c.visualizer.enabled { "on".into() } else { "off".into() },
        c.visualizer.style.label().to_string(),
        if c.animations.enabled { "on".into() } else { "off (reduced motion)".into() },
        if c.general.mouse { "on".into() } else { "off".into() },
    ];
    let rows: Vec<(String, String)> = BEHAVIOR_ROWS.iter().map(|s| s.to_string()).zip(values).collect();
    let used = option_rows(app, buf, Rect { x: area.x, y: area.y + 2, width: area.width, height: area.height - 2 }, &rows, row);
    let y = area.y + 3 + used;
    paragraph(buf, Rect { y, ..area }, y, "Tip: Tab switches what ← / → do at any time, Shift+Tab does the same for [ / ].", th.dim());
}

fn pair_text(m: crate::config::ArrowMode) -> String {
    match m {
        crate::config::ArrowMode::Seek => "skip seconds".into(),
        crate::config::ArrowMode::Track => "previous / next song".into(),
        crate::config::ArrowMode::Speed => "change speed".into(),
        crate::config::ArrowMode::Volume => "change volume".into(),
    }
}

fn done(app: &mut App, buf: &mut Buffer, area: Rect) {
    let th = app.theme.clone();
    let entered = app.onboarding.as_ref().map(|o| o.entered).unwrap_or(app.started);
    let mut y = area.y + 1;
    y += brand_title(app, buf, Rect { x: area.x, y, width: area.width, height: 3 }, "READY", entered) + 2;
    let u = th.unicode;
    let k = |a| app.keymap.hint(a, u);
    use crate::keys::Action as A;
    let tips = [
        (k(A::TogglePlay), "play / pause"),
        (k(A::FocusSearch), "search"),
        ("letters".into(), "type any command: play, next, volume 50, lyrics…"),
        (format!("{} {}", k(A::ArrowLeft), k(A::ArrowRight)), "skip / song / speed / volume (Tab switches)"),
        (format!("{} {}", k(A::BracketLeft), k(A::BracketRight)), "the same, with their own mode (Shift+Tab)"),
        (k(A::Help), "every shortcut and command"),
        (k(A::Settings), "settings"),
    ];
    for (key, d) in tips {
        if y >= area.bottom() {
            break;
        }
        put(buf, area.x + 8, y, &key, th.key(), 16);
        put(buf, area.x + 26, y, d, th.dim(), area.width.saturating_sub(26));
        y += 1;
    }
    // Celebration sparkles.
    if app.motion.enabled {
        let pal = th.spectrum();
        for i in 0..18u64 {
            let h = anim::hash(i * 31 + (anim::clock() * 3.0) as u64);
            let x = area.x + (h % area.width as u64) as u16;
            let yy = area.y + ((h >> 16) % 5) as u16;
            if let Some(c) = buf.cell_mut((x, yy))
                && c.symbol() == " " {
                    c.set_char(if u { '✦' } else { '*' }).set_style(Style::new().fg(pal[(h >> 32) as usize % pal.len()]).add_modifier(Modifier::DIM));
                }
        }
    }
}
