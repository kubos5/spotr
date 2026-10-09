//! Keyboard and mouse handling.
//!
//! Rules:
//! * When a text field (command bar, search bar, settings field) is focused,
//!   keys edit the field and no shortcut fires.
//! * Otherwise, letters open the command bar (and are typed into it), while
//!   every other key may trigger a shortcut.

use std::time::Instant;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::*;
use crate::config::{ArrowMode, CoverMode};
use crate::keys::Chord;

impl App {
    pub(super) fn handle_input(&mut self, ev: Event) {
        if self.quit_at.is_some() {
            return;
        }
        // Any key or click skips the splash — and still does what it normally would.
        if self.splash.is_some() && matches!(ev, Event::Key(_) | Event::Mouse(MouseEvent { kind: MouseEventKind::Down(_), .. })) {
            self.splash = None;
            self.needs_clear = true;
        }
        match ev {
            Event::Key(k) if k.kind != KeyEventKind::Release => self.on_key(k),
            Event::Mouse(m) => self.on_mouse(m),
            Event::Paste(s) => self.on_paste(&s),
            Event::Resize(_, _) => {
                self.needs_clear = true;
                self.images.clear_protocols();
            }
            _ => {}
        }
    }

    fn on_paste(&mut self, s: &str) {
        let s = s.replace(['\n', '\r'], " ");
        if let Some(o) = self.onboarding.as_mut() {
            o.paste(&s);
            return;
        }
        if self.settings.editing.is_some() {
            if let Some(e) = self.settings.editing.as_mut() {
                e.insert_str(&s);
            }
            return;
        }
        match self.focus {
            Focus::SearchInput => {
                if std::mem::take(&mut self.search.select_all) {
                    self.search.input.clear();
                }
                self.search.input.insert_str(&s);
                self.search_query_changed();
            }
            Focus::Command => {
                self.command.input.insert_str(&s);
                self.command.suggestion = None;
            }
            _ => {
                // Pasting a Spotify link plays it; anything else goes to the command bar.
                if normalize_uri(&s).is_some() {
                    self.execute(&format!("play {s}"));
                } else {
                    self.focus_command(Some(&s));
                }
            }
        }
    }

    fn on_key(&mut self, k: KeyEvent) {
        // Ctrl+C always quits, wherever focus is.
        if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit();
            return;
        }
        if self.onboarding.is_some() {
            self.onboarding_key(k);
            return;
        }
        if self.focus == Focus::Command {
            self.command_key(k);
            return;
        }
        if self.overlay != Overlay::None && self.overlay_key(k) {
            return;
        }
        if self.focus == Focus::SearchInput {
            self.search_input_key(k);
            return;
        }

        let chord = Chord::from_event(&k);
        if chord.is_typing_key() {
            if let KeyCode::Char(c) = k.code {
                self.focus_command(Some(&c.to_string()));
            }
            return;
        }

        // List navigation in browse / queue / lyrics views takes precedence.
        if self.overlay == Overlay::None && self.list_key(k) {
            return;
        }

        // Digits jump to 0%…90% of the song.
        if self.cfg.playback.digit_seek && k.modifiers.is_empty()
            && let KeyCode::Char(d @ '0'..='9') = k.code {
                let dur = self.pb.duration();
                if dur > 0 {
                    let pct = d.to_digit(10).unwrap() as u64 * 10;
                    self.osd("◷", format!("{pct}%"), Some(pct as f32 / 100.0));
                    self.seek_to(dur * pct / 100);
                }
                return;
            }

        if let Some(action) = self.keymap.action_for(&k) {
            self.do_action(action);
        }
    }

    pub fn focus_command(&mut self, initial: Option<&str>) {
        if self.focus != Focus::Command {
            self.prev_focus = self.focus;
        }
        self.focus = Focus::Command;
        self.command.hist_idx = None;
        self.command.suggestion = None;
        if let Some(s) = initial {
            self.command.input.insert_str(s);
        }
    }

    pub fn unfocus_command(&mut self) {
        self.command.base = None;
        self.command.suggestion = None;
        self.focus = match self.prev_focus {
            Focus::Command => Focus::Main,
            f if (f == Focus::SearchInput || f == Focus::SearchList) && !self.search_open => Focus::Main,
            f => f,
        };
    }

    pub fn do_action(&mut self, action: Action) {
        use Action::*;
        let step_ms = self.cfg.playback.seek_step as i64 * 1000;
        match action {
            TogglePlay => self.toggle_play(),
            FocusSearch => self.open_search(true),
            Help => self.set_overlay(if self.overlay == Overlay::Help { Overlay::None } else { Overlay::Help }),
            Settings => self.set_overlay(if self.overlay == Overlay::Settings { Overlay::None } else { Overlay::Settings }),
            CommandBar => self.focus_command(None),
            ArrowLeft | ArrowRight => self.pair_action(self.cfg.playback.arrow_mode, if action == ArrowRight { 1 } else { -1 }),
            BracketLeft | BracketRight => {
                self.pair_action(self.cfg.playback.bracket_mode, if action == BracketRight { 1 } else { -1 })
            }
            CycleArrowMode => {
                self.cfg.playback.arrow_mode = self.cfg.playback.arrow_mode.cycle(1);
                let keys = format!("{}{}", self.keymap.hint(ArrowLeft, self.theme.unicode), self.keymap.hint(ArrowRight, self.theme.unicode));
                let label = format!("{keys}  {}", self.pair_label(self.cfg.playback.arrow_mode));
                self.osd("⇄", label, None);
                self.save_config();
            }
            CycleBracketMode => {
                self.cfg.playback.bracket_mode = self.cfg.playback.bracket_mode.cycle(1);
                let keys = format!("{} {}", self.keymap.hint(BracketLeft, self.theme.unicode), self.keymap.hint(BracketRight, self.theme.unicode));
                let label = format!("{keys}  {}", self.pair_label(self.cfg.playback.bracket_mode));
                self.osd("⇄", label, None);
                self.save_config();
            }
            VolumeUp => self.change_volume(self.cfg.playback.volume_step as i32),
            VolumeDown => self.change_volume(-(self.cfg.playback.volume_step as i32)),
            Mute => self.toggle_mute(),
            NextTrack => self.next(),
            PrevTrack => self.previous(),
            SeekForward => self.seek_by(step_ms),
            SeekBackward => self.seek_by(-step_ms),
            SpeedUp => self.change_speed(self.cfg.playback.speed_step),
            SpeedDown => self.change_speed(-self.cfg.playback.speed_step),
            SpeedReset => self.set_speed(1.0),
            Shuffle => self.set_shuffle(!self.pb.shuffle),
            Repeat => self.cycle_repeat(),
            Like => {
                if self.search_open && self.focus == Focus::SearchList
                    && let Some(Item::Track(t)) = self.selected_item().cloned() {
                        let current = self.pb.track.as_ref().map(|c| c.uri == t.uri).unwrap_or(false);
                        let like = if current { !self.saved_current.unwrap_or(false) } else { true };
                        self.set_liked(Some(t), like);
                        return;
                    }
                self.toggle_like_current();
            }
            Lyrics => self.set_view(if self.view == MainView::Lyrics && !self.search_open { MainView::NowPlaying } else { MainView::Lyrics }),
            Queue => self.set_view(if self.view == MainView::Queue && !self.search_open { MainView::NowPlaying } else { MainView::Queue }),
            Devices => self.set_overlay(if self.overlay == Overlay::Devices { Overlay::None } else { Overlay::Devices }),
            Library => {
                self.open_search(false);
                if !matches!(self.search.stack.last().map(|p| p.kind), Some(PageKind::Library)) {
                    self.open_library(false);
                }
            }
            ToggleCover => {
                self.cfg.appearance.cover_mode = self.cfg.appearance.cover_mode.cycle(1);
                if self.cfg.appearance.cover_mode == CoverMode::Image && !self.images.graphics {
                    self.toast(ToastKind::Info, "This terminal has no graphics protocol — using the text renderer");
                }
                let label = format!("Cover: {}", self.cfg.appearance.cover_mode.label());
                self.osd("◧", label, None);
                self.save_config();
            }
            CycleVisualizer => {
                if !self.cfg.visualizer.enabled {
                    self.cfg.visualizer.enabled = true;
                } else {
                    self.cfg.visualizer.style = self.cfg.visualizer.style.cycle(1);
                }
                let label = format!("Visualizer: {}", self.cfg.visualizer.style.label());
                self.osd("▁▃▅", label, None);
                self.save_config();
            }
            AddToQueue => {
                let item = if self.search_open { self.selected_item().cloned() } else { None };
                match item {
                    Some(Item::Track(t)) => self.enqueue(t),
                    Some(_) => self.toast(ToastKind::Info, "Only songs can be queued"),
                    None => match self.pb.track.clone() {
                        Some(t) => self.enqueue(t),
                        None => self.toast(ToastKind::Info, "Select a song to queue"),
                    },
                }
            }
            Back => self.go_back(),
            Quit => self.quit(),
        }
    }

    /// What a key pair (← / → or [ / ]) does in `mode`; `dir` is -1 or 1.
    pub fn pair_action(&mut self, mode: ArrowMode, dir: i32) {
        match mode {
            ArrowMode::Seek => self.seek_by(self.cfg.playback.seek_step as i64 * 1000 * dir as i64),
            ArrowMode::Track => {
                if dir > 0 { self.next() } else { self.previous() }
            }
            ArrowMode::Speed => self.change_speed(self.cfg.playback.speed_step * dir as f32),
            ArrowMode::Volume => self.change_volume(self.cfg.playback.volume_step as i32 * dir),
        }
    }

    /// Short description of a key pair mode, for hints and badges.
    pub fn pair_label(&self, mode: ArrowMode) -> String {
        match mode {
            ArrowMode::Seek => format!("skip {}s", self.cfg.playback.seek_step),
            ArrowMode::Track => "prev/next".into(),
            ArrowMode::Speed => "speed".into(),
            ArrowMode::Volume => "volume".into(),
        }
    }

    pub fn go_back(&mut self) {
        if self.overlay != Overlay::None {
            self.set_overlay(Overlay::None);
        } else if self.search_open {
            if !self.pop_page() {
                self.close_search();
            }
        } else if self.view != MainView::NowPlaying {
            self.set_view(MainView::NowPlaying);
        }
    }

    /// Navigation keys for the list in the current view. Returns true if consumed.
    fn list_key(&mut self, k: KeyEvent) -> bool {
        let shift = k.modifiers.contains(KeyModifiers::SHIFT);
        let alt = k.modifiers.contains(KeyModifiers::ALT);
        if self.search_open && self.focus == Focus::SearchList {
            match k.code {
                KeyCode::Up if !shift => {
                    let first = self.browse_items().iter().position(|i| i.is_selectable()).unwrap_or(0);
                    if self.browse_nav().selected <= first && self.search.stack.is_empty() {
                        self.focus = Focus::SearchInput;
                    } else {
                        self.browse_step(-1);
                    }
                }
                KeyCode::Down if !shift => self.browse_step(1),
                KeyCode::PageUp => self.browse_step(-10),
                KeyCode::PageDown => self.browse_step(10),
                KeyCode::Home => {
                    let items: Vec<Item> = self.browse_items().to_vec();
                    self.browse_nav().first(&items);
                }
                KeyCode::End => {
                    let items: Vec<Item> = self.browse_items().to_vec();
                    self.browse_nav().last(&items);
                    self.maybe_load_more();
                }
                KeyCode::Enter => self.activate_selected(),
                KeyCode::Backspace => {
                    if !self.pop_page() {
                        self.focus = Focus::SearchInput;
                    }
                }
                KeyCode::Tab | KeyCode::BackTab if self.search.stack.is_empty() => {
                    let dir = if k.code == KeyCode::BackTab || shift { -1 } else { 1 };
                    self.set_filter(self.search.filter.cycle(dir));
                }
                _ => return false,
            }
            return true;
        }
        if self.search_open || self.overlay != Overlay::None {
            return false;
        }
        match self.view {
            MainView::Queue => {
                let n = self.queue.items.len();
                match k.code {
                    KeyCode::Up if shift || alt => self.queue_move(-1),
                    KeyCode::Down if shift || alt => self.queue_move(1),
                    KeyCode::Up => self.queue.list.simple_step(-1, n),
                    KeyCode::Down => self.queue.list.simple_step(1, n),
                    KeyCode::PageUp => self.queue.list.simple_step(-10, n),
                    KeyCode::PageDown => self.queue.list.simple_step(10, n),
                    KeyCode::Home => self.queue.list.selected = 0,
                    KeyCode::End => self.queue.list.selected = n.saturating_sub(1),
                    KeyCode::Delete | KeyCode::Backspace => {
                        let i = self.queue.list.selected;
                        self.queue_remove(i);
                    }
                    KeyCode::Enter => {
                        let i = self.queue.list.selected;
                        if i < self.queue.items.len() {
                            let t = self.queue.items.remove(i);
                            self.play_track_alone(&t);
                        }
                    }
                    _ => return false,
                }
                true
            }
            MainView::Lyrics => {
                let n = self.lyrics.data.as_ref().map(|l| l.lines.len()).unwrap_or(0);
                if n == 0 {
                    return false;
                }
                let cur = self
                    .lyrics
                    .manual
                    .map(|(l, _)| l)
                    .or_else(|| self.lyrics.data.as_ref().and_then(|l| l.current(self.position())))
                    .unwrap_or(0);
                let target = match k.code {
                    KeyCode::Up => cur.saturating_sub(1),
                    KeyCode::Down => (cur + 1).min(n - 1),
                    KeyCode::PageUp => cur.saturating_sub(8),
                    KeyCode::PageDown => (cur + 8).min(n - 1),
                    KeyCode::Home => 0,
                    KeyCode::End => n - 1,
                    KeyCode::Enter => {
                        if let Some(t) = self.lyrics.data.as_ref().and_then(|l| l.lines.get(cur)).and_then(|l| l.0) {
                            self.lyrics.manual = None;
                            self.seek_to(t);
                        }
                        return true;
                    }
                    _ => return false,
                };
                self.lyrics.manual = Some((target, Instant::now()));
                true
            }
            MainView::NowPlaying => false,
        }
    }

    pub fn queue_move(&mut self, dir: i32) {
        let i = self.queue.list.selected;
        let j = i as i32 + dir;
        if j < 0 || j as usize >= self.queue.items.len() || i >= self.queue.items.len() {
            return;
        }
        self.queue.items.swap(i, j as usize);
        self.queue.list.selected = j as usize;
        self.queue.list.moved_at = Some(Instant::now());
    }

    pub fn queue_remove(&mut self, i: usize) {
        if i < self.queue.items.len() {
            let t = self.queue.items.remove(i);
            self.toast(ToastKind::Info, format!("Removed “{}” from the queue", t.name));
            if self.queue.list.selected >= self.queue.items.len() {
                self.queue.list.selected = self.queue.items.len().saturating_sub(1);
            }
        }
    }

    fn command_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc => {
                self.command.input.clear();
                self.unfocus_command();
            }
            KeyCode::Enter => {
                // Accept a highlighted suggestion that completes the command name.
                let text = self.command.input.value.clone();
                self.command.input.clear();
                self.unfocus_command();
                let text = text.trim().to_string();
                if !text.is_empty() {
                    if self.command.history.last() != Some(&text) {
                        self.command.history.push(text.clone());
                    }
                    self.execute(&text);
                }
            }
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down
                if matches!(k.code, KeyCode::Tab | KeyCode::BackTab) || self.command.suggestion.is_some() =>
            {
                // Cycle through completions computed from the text typed before cycling began.
                let base = self.command.base.clone().unwrap_or_else(|| self.command.input.value.clone());
                let sugg = crate::commands::suggest(&base, &self.device_names(), &self.playlist_names());
                if sugg.is_empty() {
                    return;
                }
                let dir: i32 = if matches!(k.code, KeyCode::BackTab | KeyCode::Up) { -1 } else { 1 };
                let n = sugg.len() as i32;
                let next = match self.command.suggestion {
                    None => if dir > 0 { 0 } else { n - 1 },
                    Some(i) => ((i as i32 + dir) % n + n) % n,
                } as usize;
                self.command.input.set(&sugg[next].replace);
                if sugg.len() == 1 {
                    self.command.suggestion = None;
                    self.command.base = None;
                } else {
                    self.command.suggestion = Some(next);
                    self.command.base = Some(base);
                }
            }
            KeyCode::Right if self.command.input.cursor == self.command.input.len() && self.command.base.is_none() => {
                // Accept the ghost completion.
                let sugg = crate::commands::suggest(&self.command.input.value, &self.device_names(), &self.playlist_names());
                if let Some(s) = sugg.first() {
                    self.command.input.set(&s.replace);
                }
            }
            KeyCode::Up => {
                if self.command.history.is_empty() {
                    return;
                }
                let idx = match self.command.hist_idx {
                    None => {
                        self.command.draft = self.command.input.value.clone();
                        self.command.history.len() - 1
                    }
                    Some(0) => 0,
                    Some(i) => i - 1,
                };
                self.command.hist_idx = Some(idx);
                let h = self.command.history[idx].clone();
                self.command.input.set(&h);
            }
            KeyCode::Down => match self.command.hist_idx {
                Some(i) if i + 1 < self.command.history.len() => {
                    self.command.hist_idx = Some(i + 1);
                    let h = self.command.history[i + 1].clone();
                    self.command.input.set(&h);
                }
                Some(_) => {
                    self.command.hist_idx = None;
                    let d = self.command.draft.clone();
                    self.command.input.set(&d);
                }
                None => {}
            },
            KeyCode::Backspace if self.command.input.is_empty() && !ctrl => self.unfocus_command(),
            _ => {
                if let Some(true) = self.command.input.handle(&k) {
                    self.command.suggestion = None;
                    self.command.hist_idx = None;
                }
            }
        }
    }

    fn search_input_key(&mut self, k: KeyEvent) {
        if std::mem::take(&mut self.search.select_all) {
            let ctrl = k.modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
            match k.code {
                KeyCode::Char(_) if !ctrl => self.search.input.clear(),
                KeyCode::Backspace | KeyCode::Delete => {
                    self.search.input.clear();
                    self.search_query_changed();
                    return;
                }
                _ => {}
            }
        }
        match k.code {
            KeyCode::Esc => self.close_search(),
            KeyCode::Enter => {
                self.search.pending = None;
                if self.search.input.value.trim() != self.search.query || self.search.error.is_some() {
                    self.search.query.clear();
                    self.run_search(false);
                }
                if !self.browse_items().is_empty() || self.search.loading {
                    self.focus = Focus::SearchList;
                }
            }
            KeyCode::Down | KeyCode::PageDown => {
                if !self.browse_items().is_empty() {
                    self.focus = Focus::SearchList;
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                let dir = if k.code == KeyCode::BackTab { -1 } else { 1 };
                self.set_filter(self.search.filter.cycle(dir));
            }
            _ => {
                if let Some(true) = self.search.input.handle(&k) {
                    self.search_query_changed();
                }
            }
        }
    }

    /// Keys for modal overlays. Returns true when consumed.
    fn overlay_key(&mut self, k: KeyEvent) -> bool {
        match self.overlay {
            Overlay::Settings => self.settings_key(k),
            Overlay::Help => {
                match k.code {
                    KeyCode::Esc | KeyCode::Char('?') | KeyCode::F(1) => self.set_overlay(Overlay::None),
                    KeyCode::Up => self.help.scroll = self.help.scroll.saturating_sub(1),
                    KeyCode::Down => self.help.scroll += 1,
                    KeyCode::PageUp => self.help.scroll = self.help.scroll.saturating_sub(10),
                    KeyCode::PageDown | KeyCode::Char(' ') => self.help.scroll += 10,
                    KeyCode::Home => self.help.scroll = 0,
                    KeyCode::Backspace if !self.help.filter.is_empty() => {
                        self.help.filter.clear();
                        self.help.scroll = 0;
                    }
                    _ => return self.overlay_fallthrough(k),
                }
                true
            }
            Overlay::About => match k.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.set_overlay(Overlay::None);
                    true
                }
                _ => self.overlay_fallthrough(k),
            },
            Overlay::Devices => {
                let n = self.devices.list.len() + self.devices.outputs.len();
                match k.code {
                    KeyCode::Esc => self.set_overlay(Overlay::None),
                    KeyCode::Up => self.devices.nav.simple_step(-1, n),
                    KeyCode::Down => self.devices.nav.simple_step(1, n),
                    KeyCode::Enter => {
                        let i = self.devices.nav.selected;
                        self.select_device(i);
                    }
                    _ => return self.overlay_fallthrough(k),
                }
                true
            }
            Overlay::None => false,
        }
    }

    /// Inside overlays, letters still open the command bar and a few global
    /// playback shortcuts keep working.
    fn overlay_fallthrough(&mut self, k: KeyEvent) -> bool {
        let chord = Chord::from_event(&k);
        if chord.is_typing_key() {
            return false;
        }
        match self.keymap.action_for(&k) {
            Some(a @ (Action::TogglePlay | Action::Quit | Action::Help | Action::Settings | Action::Devices
            | Action::NextTrack | Action::PrevTrack | Action::BracketLeft | Action::BracketRight | Action::VolumeUp | Action::VolumeDown | Action::Mute
            | Action::CommandBar | Action::FocusSearch | Action::Back)) => {
                self.do_action(a);
                true
            }
            _ => true,
        }
    }

    pub fn select_device(&mut self, i: usize) {
        if i < self.devices.list.len() {
            let d = self.devices.list[i].clone();
            let Some(id) = d.id.clone() else { return };
            if d.is_restricted {
                self.toast(ToastKind::Warn, format!("{} can't be controlled through the Web API", d.name));
                return;
            }
            self.osd("◈", format!("Playing on {}", d.name), None);
            let play = self.pb.is_playing;
            let name = d.name.clone();
            self.pb.device = Some(d);
            self.pb.hold(1500);
            self.run(Some(format!("Switched to {name}")), move |sp| async move { sp.transfer(&id, play).await });
            self.fetch_devices();
        } else if let Some(name) = self.devices.outputs.get(i - self.devices.list.len()).cloned() {
            self.audio.set_device(&name);
            self.cfg.general.audio_device = name.clone();
            self.save_config();
            self.toast(ToastKind::Success, format!("Built-in player now outputs to {name}"));
        }
    }

    // ------------------------------------------------------------- mouse

    fn on_mouse(&mut self, m: MouseEvent) {
        self.mouse = (m.column, m.row);
        let hit = self
            .clicks
            .iter()
            .rev()
            .find(|(r, _)| r.contains((m.column, m.row).into()))
            .map(|(r, c)| (*r, *c));
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((rect, c)) = hit {
                    self.on_click(c, rect, m.column);
                } else if self.focus == Focus::Command {
                    self.unfocus_command();
                }
            }
            MouseEventKind::Moved => {
                // Hover selects rows.
                if let Some((_, c)) = hit {
                    match c {
                        Click::Row(i) if self.search_open => {
                            let nav = self.browse_nav();
                            if nav.selected != i {
                                nav.selected = i;
                                nav.moved_at = Some(Instant::now());
                            }
                        }
                        Click::QueueRow(i) => self.queue.list.selected = i,
                        Click::DeviceRow(i) => self.devices.nav.selected = i,
                        Click::SettingsRow(i) if self.settings.editing.is_none() && self.settings.capture.is_none() => {
                            self.settings.selected = i
                        }
                        _ => {}
                    }
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let dir = if m.kind == MouseEventKind::ScrollUp { -1 } else { 1 };
                self.on_scroll(dir, hit.map(|h| h.1));
            }
            _ => {}
        }
    }

    fn on_scroll(&mut self, dir: i32, target: Option<Click>) {
        if self.onboarding.is_some() {
            return;
        }
        match self.overlay {
            Overlay::Help => {
                self.help.scroll = (self.help.scroll as i32 + dir * 3).max(0) as usize;
                return;
            }
            Overlay::Settings => {
                self.settings_scroll(dir);
                return;
            }
            Overlay::Devices => {
                let n = self.devices.list.len() + self.devices.outputs.len();
                self.devices.nav.simple_step(dir, n);
                return;
            }
            Overlay::About => return,
            Overlay::None => {}
        }
        if matches!(target, Some(Click::Volume { .. })) {
            self.change_volume(-dir * self.cfg.playback.volume_step as i32);
            return;
        }
        if self.search_open {
            self.browse_step(dir * 2);
            return;
        }
        match self.view {
            MainView::Queue => self.queue.list.simple_step(dir, self.queue.items.len()),
            MainView::Lyrics => {
                let n = self.lyrics.data.as_ref().map(|l| l.lines.len()).unwrap_or(0);
                if n > 0 {
                    let cur = self
                        .lyrics
                        .manual
                        .map(|(l, _)| l)
                        .or_else(|| self.lyrics.data.as_ref().and_then(|l| l.current(self.position())))
                        .unwrap_or(0) as i32;
                    self.lyrics.manual = Some(((cur + dir * 2).clamp(0, n as i32 - 1) as usize, Instant::now()));
                }
            }
            MainView::NowPlaying => self.change_volume(-dir * self.cfg.playback.volume_step as i32),
        }
    }

    fn on_click(&mut self, c: Click, rect: Rect, col: u16) {
        if self.onboarding.is_some() {
            if let Click::Onboard(oc) = c {
                self.onboarding_click(oc);
            }
            return;
        }
        // Clicking anywhere outside the command bar closes it.
        if self.focus == Focus::Command && !matches!(c, Click::CommandBar | Click::Suggestion(_)) {
            self.unfocus_command();
        }
        match c {
            Click::SearchBar => self.open_search(true),
            Click::CommandBar => {
                if self.focus != Focus::Command {
                    self.focus_command(None);
                }
            }
            Click::Action(a) => self.do_action(a),
            Click::Progress { x, width } => {
                let dur = self.pb.duration();
                if dur > 0 && width > 0 {
                    let frac = (col.saturating_sub(x) as f32 / width as f32).clamp(0.0, 1.0);
                    self.seek_to((dur as f32 * frac) as u64);
                }
            }
            Click::Volume { x, width } => {
                let frac = (col.saturating_sub(x) as f32 / width.max(1) as f32).clamp(0.0, 1.0);
                self.set_volume((frac * 100.0).round() as u32);
            }
            Click::Cover => self.do_action(Action::ToggleCover),
            Click::Like => self.toggle_like_current(),
            Click::PlayPause => self.toggle_play(),
            Click::Shuffle => self.set_shuffle(!self.pb.shuffle),
            Click::Repeat => self.cycle_repeat(),
            Click::Row(i) => {
                self.focus = Focus::SearchList;
                self.browse_nav().selected = i;
                self.activate_selected();
            }
            Click::Filter(f) => {
                self.focus = Focus::SearchList;
                self.set_filter(f);
            }
            Click::Back => {
                if !self.pop_page() {
                    self.close_search();
                }
            }
            Click::QueueRow(i) => {
                if self.queue.list.selected == i {
                    if i < self.queue.items.len() {
                        let t = self.queue.items.remove(i);
                        self.play_track_alone(&t);
                    }
                } else {
                    self.queue.list.selected = i;
                }
            }
            Click::LyricsLine(i) => {
                if let Some(t) = self.lyrics.data.as_ref().and_then(|l| l.lines.get(i)).and_then(|l| l.0) {
                    self.lyrics.manual = None;
                    self.seek_to(t);
                }
            }
            Click::DeviceRow(i) => {
                self.devices.nav.selected = i;
                self.select_device(i);
            }
            Click::SettingsRow(i) => self.settings_click(i, rect, col),
            Click::SettingsTab(i) => {
                self.settings.tab = i;
                self.settings.selected = 0;
                self.settings.scroll = 0;
            }
            Click::CloseOverlay => self.set_overlay(Overlay::None),
            Click::Suggestion(i) => {
                let base = self.command.base.clone().unwrap_or_else(|| self.command.input.value.clone());
                let sugg = crate::commands::suggest(&base, &self.device_names(), &self.playlist_names());
                if let Some(s) = sugg.get(i) {
                    self.command.input.set(&s.replace);
                    self.command.suggestion = None;
                    self.command.base = None;
                }
                self.focus = Focus::Command;
            }
            Click::Toast(i) => {
                if i < self.toasts.len() {
                    self.toasts.remove(i);
                }
            }
            Click::Run(cmd) => self.execute(cmd),
            Click::Onboard(_) => {}
        }
    }
}
