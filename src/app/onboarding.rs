//! First-run walkthrough: Spotify app setup, sign-in, built-in player and preferences.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::*;
use crate::spotify::auth;
use crate::textinput::TextInput;

pub const DASHBOARD_URL: &str = "https://developer.spotify.com/dashboard";
pub const PREVIEW_IMAGE: &str = "spotr://preview";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Welcome,
    App,
    SignIn,
    Player,
    Look,
    Behavior,
    Done,
}

impl Step {
    pub const ALL: [Step; 7] = [Step::Welcome, Step::App, Step::SignIn, Step::Player, Step::Look, Step::Behavior, Step::Done];
    pub fn title(self) -> &'static str {
        match self {
            Step::Welcome => "Welcome",
            Step::App => "Spotify app",
            Step::SignIn => "Sign in",
            Step::Player => "Player",
            Step::Look => "Look",
            Step::Behavior => "Behavior",
            Step::Done => "Ready",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AuthState {
    Idle,
    Waiting { url: String, since: Instant },
    Done(String),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlayerSetup {
    Idle,
    Connecting(Instant),
    NeedsLogin,
    Ready,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OnboardClick {
    Next,
    Back,
    Row(usize),
    OpenBrowser,
    OpenDashboard,
    Step(usize),
}

pub struct Onboarding {
    pub step: Step,
    pub entered: Instant,
    pub dir: i32,
    pub client_id: TextInput,
    pub device_name: TextInput,
    pub auth: AuthState,
    pub player: PlayerSetup,
    pub row: usize,
    pub editing_name: bool,
    /// Only re-run the sign-in part (from the `login` command).
    pub only_login: bool,
    pub error: Option<(String, Instant)>,
    pub had_token: bool,
}

impl Onboarding {
    pub fn new(cfg: &Config, had_token: bool) -> Self {
        Self {
            step: Step::Welcome,
            entered: Instant::now(),
            dir: 1,
            client_id: TextInput::with(&cfg.general.client_id),
            device_name: TextInput::with(&cfg.general.device_name),
            auth: AuthState::Idle,
            player: PlayerSetup::Idle,
            row: 0,
            editing_name: false,
            only_login: false,
            error: None,
            had_token,
        }
    }

    pub fn at_login(cfg: &Config) -> Self {
        let mut o = Self::new(cfg, false);
        o.only_login = true;
        o.step = if auth::valid_client_id(&cfg.general.client_id) { Step::SignIn } else { Step::App };
        o
    }

    pub fn steps(&self) -> Vec<Step> {
        if self.only_login {
            vec![Step::App, Step::SignIn]
        } else {
            Step::ALL.to_vec()
        }
    }

    pub fn index(&self) -> usize {
        self.steps().iter().position(|s| *s == self.step).unwrap_or(0)
    }

    pub fn paste(&mut self, s: &str) {
        match self.step {
            Step::App => self.client_id.insert_str(s.trim()),
            Step::Player if self.editing_name => self.device_name.insert_str(s),
            _ => {}
        }
    }

    pub fn fail(&mut self, msg: impl Into<String>) {
        self.error = Some((msg.into(), Instant::now()));
    }

    pub fn player_ready(&mut self) {
        self.player = PlayerSetup::Ready;
    }
    pub fn player_failed(&mut self, e: String) {
        if self.player != PlayerSetup::NeedsLogin {
            self.player = PlayerSetup::Failed(e);
        }
    }
    pub fn player_needs_login(&mut self) {
        self.player = PlayerSetup::NeedsLogin;
    }

    /// Number of selectable rows on the current step.
    pub fn row_count(&self) -> usize {
        match self.step {
            Step::Player => 3,
            Step::Look => LOOK_ROWS.len(),
            Step::Behavior => BEHAVIOR_ROWS.len(),
            _ => 0,
        }
    }
}

pub const LOOK_ROWS: &[&str] = &["Cover art", "Text art style", "Pixelation", "Title font", "Accent color", "Icons"];
pub const BEHAVIOR_ROWS: &[&str] = &["← / → keys", "[ / ] keys", "Skip amount", "Visualizer", "Visualizer style", "Animations", "Mouse"];

impl App {
    /// A procedurally generated "album cover" for previews.
    pub fn ensure_preview_image(&mut self) {
        if self.images.get(PREVIEW_IMAGE).is_some() {
            return;
        }
        let size = 128u32;
        let img = image::RgbImage::from_fn(size, size, |x, y| {
            let fx = x as f32 / size as f32;
            let fy = y as f32 / size as f32;
            // Sunset gradient with a sun and stripes, synthwave style.
            let mut r = 40.0 + 200.0 * fy;
            let mut g = 20.0 + 60.0 * (1.0 - fy);
            let mut b = 120.0 + 100.0 * (1.0 - fy);
            let dx = fx - 0.5;
            let dy = fy - 0.45;
            let d = (dx * dx + dy * dy).sqrt();
            if d < 0.25 && !(fy > 0.45 && ((fy * 30.0) as i32) % 3 == 0) {
                r = 255.0;
                g = 120.0 + 120.0 * (1.0 - (fy / 0.7));
                b = 60.0;
            }
            if fy > 0.72 {
                let line = ((fy - 0.72) * 60.0).fract() < 0.12 || (((fx - 0.5) / (fy - 0.6)) * 6.0).fract().abs() < 0.08;
                r = if line { 255.0 } else { 30.0 };
                g = if line { 60.0 } else { 10.0 };
                b = if line { 200.0 } else { 50.0 };
            }
            image::Rgb([r.clamp(0.0, 255.0) as u8, g.clamp(0.0, 255.0) as u8, b.clamp(0.0, 255.0) as u8])
        });
        self.images.insert(PREVIEW_IMAGE.to_string(), image::DynamicImage::ImageRgb8(img));
    }

    pub(super) fn on_auth(&mut self, r: Result<User, String>) {
        match r {
            Ok(u) => {
                let name = u.name().to_string();
                self.user = Some(u);
                if let Some(o) = self.onboarding.as_mut() {
                    o.auth = AuthState::Done(name.clone());
                    o.error = None;
                }
                self.toast(ToastKind::Success, format!("Signed in as {name}"));
            }
            Err(e) => {
                if let Some(o) = self.onboarding.as_mut() {
                    o.auth = AuthState::Failed(e.clone());
                }
            }
        }
    }

    fn start_auth(&mut self) {
        let Some(o) = self.onboarding.as_mut() else { return };
        let client_id = self.cfg.general.client_id.clone();
        if !auth::valid_client_id(&client_id) {
            o.step = Step::App;
            o.fail("Enter your Client ID first");
            return;
        }
        let redirect = format!("http://127.0.0.1:{}/callback", self.cfg.general.redirect_port);
        let pkce = auth::begin(&client_id, &redirect, auth::SCOPES);
        o.auth = AuthState::Waiting { url: pkce.url.clone(), since: Instant::now() };
        o.error = None;
        let http = self.http.clone();
        let sp = self.spotify.clone();
        self.spawn(async move {
            let r = async {
                let token = auth::login(&http, &pkce, true).await?;
                sp.set_token(token).await?;
                sp.me().await
            }
            .await;
            AppEvent::Auth(r.map_err(|e| e.to_string()))
        });
    }

    fn verify_existing_token(&mut self) {
        let sp = self.spotify.clone();
        if let Some(o) = self.onboarding.as_mut() {
            o.auth = AuthState::Waiting { url: String::new(), since: Instant::now() };
        }
        self.spawn(async move {
            AppEvent::Auth(async { sp.me().await }.await.map_err(|e| format!("Saved login didn't work ({e}) — sign in again")))
        });
    }

    fn onboarding_go(&mut self, dir: i32) {
        let Some(o) = self.onboarding.as_mut() else { return };
        let steps = o.steps();
        let i = o.index() as i32 + dir;
        if i < 0 {
            return;
        }
        if i as usize >= steps.len() {
            self.finish_onboarding();
            return;
        }
        o.step = steps[i as usize];
        o.entered = Instant::now();
        o.dir = dir;
        o.row = 0;
        o.editing_name = false;
        o.error = None;
        let step = o.step;
        let auth_idle = matches!(o.auth, AuthState::Idle | AuthState::Failed(_));
        let had_token = o.had_token;
        if step == Step::SignIn && auth_idle && had_token {
            self.verify_existing_token();
        }
        if step == Step::Look {
            self.ensure_preview_image();
        }
    }

    /// Validate the current step and advance.
    fn onboarding_next(&mut self) {
        let Some(o) = self.onboarding.as_mut() else { return };
        match o.step {
            Step::App => {
                let id = o.client_id.value.trim().to_string();
                if !auth::valid_client_id(&id) {
                    o.fail("That doesn't look like a Client ID — it's 32 characters of 0-9 and a-f.");
                    return;
                }
                if id != self.cfg.general.client_id {
                    // A new app means the old token is useless.
                    o.auth = AuthState::Idle;
                    o.had_token = false;
                }
                self.cfg.general.client_id = id;
                let _ = self.cfg.save(&self.paths);
            }
            Step::SignIn => match &o.auth {
                AuthState::Done(_) => {}
                AuthState::Waiting { .. } => return,
                _ => {
                    self.start_auth();
                    return;
                }
            },
            Step::Player => {
                let name = o.device_name.value.trim().to_string();
                if !name.is_empty() {
                    self.cfg.general.device_name = name;
                }
                o.editing_name = false;
                let enabled = self.cfg.general.local_player && crate::player::AVAILABLE;
                match (&o.player, enabled) {
                    (PlayerSetup::Idle | PlayerSetup::Failed(_), true) => {
                        o.player = PlayerSetup::Connecting(Instant::now());
                        let _ = self.cfg.save(&self.paths);
                        self.start_local_player(false);
                        return;
                    }
                    (PlayerSetup::NeedsLogin, true) => {
                        o.player = PlayerSetup::Connecting(Instant::now());
                        self.local_status = LocalStatus::Off;
                        self.start_local_player(true);
                        return;
                    }
                    (PlayerSetup::Connecting(_), true) => return,
                    _ => {}
                }
            }
            _ => {}
        }
        self.onboarding_go(1);
    }

    fn finish_onboarding(&mut self) {
        let only_login = self.onboarding.as_ref().map(|o| o.only_login).unwrap_or(false);
        self.onboarding = None;
        self.cfg.general.onboarded = true;
        self.save_config();
        self.needs_clear = true;
        self.view_changed_at = Instant::now();
        if only_login {
            self.toast(ToastKind::Success, "You're signed in");
            self.after_login_light();
        } else {
            self.toast(ToastKind::Success, "All set — press / to search, ? for help. Enjoy!");
            self.after_login_light();
        }
    }

    /// Like `after_login` but doesn't restart the built-in player if it's already up.
    fn after_login_light(&mut self) {
        let sp = self.spotify.clone();
        self.spawn(async move {
            match sp.me().await {
                Ok(u) => AppEvent::User(u),
                Err(e) => AppEvent::Toast(ToastKind::Error, format!("Couldn't load your profile: {e}")),
            }
        });
        let sp = self.spotify.clone();
        self.spawn(async move { AppEvent::Playlists(sp.my_playlists().await.unwrap_or_default()) });
        self.poll_now();
        if self.cfg.general.local_player && crate::player::AVAILABLE && !self.no_player && self.local.is_none() && self.local_status != LocalStatus::Connecting {
            self.start_local_player(false);
        } else if self.local.is_some() && self.cfg.general.auto_activate {
            // Became ready during onboarding: hand playback to it now.
            self.on_local_event(LocalEvent::Connected { device_name: self.cfg.general.device_name.clone() });
        }
    }

    fn onboarding_adjust(&mut self, row: usize, dir: i32) {
        let Some(step) = self.onboarding.as_ref().map(|o| o.step) else { return };
        let c = &mut self.cfg;
        match (step, row) {
            (Step::Player, 0) => c.general.local_player = !c.general.local_player && crate::player::AVAILABLE,
            (Step::Player, 2) => c.general.auto_activate = !c.general.auto_activate,
            (Step::Look, 0) => c.appearance.cover_mode = c.appearance.cover_mode.cycle(dir),
            (Step::Look, 1) => c.appearance.text_art = c.appearance.text_art.cycle(dir),
            (Step::Look, 2) => {
                const STEPS: [u32; 6] = [0, 8, 12, 16, 24, 32];
                let i = STEPS.iter().position(|s| *s >= c.appearance.pixelation).unwrap_or(0) as i32;
                c.appearance.pixelation = STEPS[(i + dir).clamp(0, STEPS.len() as i32 - 1) as usize];
            }
            (Step::Look, 3) => {
                let cur = crate::font::resolve(&c.appearance.title_font);
                c.appearance.title_font = crate::font::cycle(cur, dir).to_string();
            }
            (Step::Look, 4) => c.appearance.accent = c.appearance.accent.cycle(dir),
            (Step::Look, 5) => c.appearance.icons = c.appearance.icons.cycle(dir),
            (Step::Behavior, 0) => c.playback.arrow_mode = c.playback.arrow_mode.cycle(dir),
            (Step::Behavior, 1) => c.playback.bracket_mode = c.playback.bracket_mode.cycle(dir),
            (Step::Behavior, 2) => {
                const STEPS: [u32; 7] = [1, 5, 10, 15, 20, 30, 60];
                let i = STEPS.iter().position(|s| *s >= c.playback.seek_step).unwrap_or(2) as i32;
                c.playback.seek_step = STEPS[(i + dir).clamp(0, STEPS.len() as i32 - 1) as usize];
            }
            (Step::Behavior, 3) => c.visualizer.enabled = !c.visualizer.enabled,
            (Step::Behavior, 4) => c.visualizer.style = c.visualizer.style.cycle(dir),
            (Step::Behavior, 5) => c.animations.enabled = !c.animations.enabled,
            (Step::Behavior, 6) => c.general.mouse = !c.general.mouse,
            _ => return,
        }
        let _ = self.cfg.save(&self.paths);
        self.apply_config();
    }

    pub(super) fn onboarding_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        let Some(o) = self.onboarding.as_mut() else { return };
        // Ctrl+O opens the relevant page in the browser.
        if ctrl && k.code == KeyCode::Char('o') {
            match o.step {
                Step::App => {
                    let _ = open::that_detached(DASHBOARD_URL);
                }
                Step::SignIn => {
                    if let AuthState::Waiting { url, .. } = &o.auth
                        && !url.is_empty() {
                            let _ = open::that_detached(url);
                        }
                }
                _ => {}
            }
            return;
        }
        // Text fields.
        if o.step == Step::App {
            match k.code {
                KeyCode::Enter => self.onboarding_next(),
                KeyCode::Esc | KeyCode::BackTab => self.onboarding_go(-1),
                _ => {
                    o.client_id.handle(&k);
                    o.error = None;
                }
            }
            return;
        }
        if o.step == Step::Player && o.editing_name {
            match k.code {
                KeyCode::Enter | KeyCode::Esc | KeyCode::Tab | KeyCode::Up | KeyCode::Down => {
                    o.editing_name = false;
                    let name = o.device_name.value.trim().to_string();
                    if !name.is_empty() {
                        self.cfg.general.device_name = name;
                    }
                }
                _ => {
                    o.device_name.handle(&k);
                }
            }
            return;
        }
        let rows = o.row_count();
        match k.code {
            KeyCode::Enter => {
                if o.step == Step::Player && o.row == 1 {
                    o.editing_name = true;
                } else {
                    self.onboarding_next();
                }
            }
            KeyCode::Esc | KeyCode::BackTab => self.onboarding_go(-1),
            KeyCode::Tab => self.onboarding_next(),
            KeyCode::Up if rows > 0 => o.row = o.row.saturating_sub(1),
            KeyCode::Down if rows > 0 => o.row = (o.row + 1).min(rows - 1),
            KeyCode::Left | KeyCode::Right if rows > 0 => {
                let dir = if k.code == KeyCode::Right { 1 } else { -1 };
                let row = o.row;
                self.onboarding_adjust(row, dir);
            }
            KeyCode::Char(' ') if rows > 0 => {
                let row = o.row;
                if o.step == Step::Player && row == 1 {
                    o.editing_name = true;
                } else {
                    self.onboarding_adjust(row, 1);
                }
            }
            KeyCode::Right | KeyCode::Char(' ') => self.onboarding_next(),
            KeyCode::Left => self.onboarding_go(-1),
            _ => {}
        }
    }

    pub(super) fn onboarding_click(&mut self, c: OnboardClick) {
        let Some(o) = self.onboarding.as_mut() else { return };
        match c {
            OnboardClick::Next => self.onboarding_next(),
            OnboardClick::Back => self.onboarding_go(-1),
            OnboardClick::Row(i) => {
                if o.row == i || o.step != Step::Player {
                    if o.step == Step::Player && i == 1 {
                        o.editing_name = true;
                    } else {
                        o.row = i;
                        self.onboarding_adjust(i, 1);
                    }
                }
                if let Some(o) = self.onboarding.as_mut() {
                    o.row = i;
                }
            }
            OnboardClick::OpenBrowser => {
                if let AuthState::Waiting { url, .. } = &o.auth {
                    if !url.is_empty() {
                        let _ = open::that_detached(url);
                    }
                } else {
                    self.onboarding_next();
                }
            }
            OnboardClick::OpenDashboard => {
                let _ = open::that_detached(DASHBOARD_URL);
            }
            OnboardClick::Step(i) => {
                let cur = o.index();
                if i < cur {
                    self.onboarding_go(i as i32 - cur as i32);
                }
            }
        }
    }

    pub fn onboarding_timeout_check(&mut self) {
        if let Some(o) = self.onboarding.as_mut()
            && let AuthState::Waiting { since, url } = &o.auth
                && !url.is_empty() && since.elapsed() > Duration::from_secs(305) {
                    o.auth = AuthState::Failed("Timed out waiting for the browser".into());
                }
    }
}
