//! Playback control, state sync, the Spotr queue and the built-in player.

use std::time::Instant;

use super::*;
use crate::config::QueueMode;
use crate::player::LocalOptions;
use crate::spotify::PlayRequest;
use crate::spotify::api::is_no_device;

impl App {
    /// Device to target when nothing is active (the built-in player, if ready).
    pub fn fallback_device(&self) -> Option<String> {
        if self.pb.device.is_none() && self.local_status == LocalStatus::Ready {
            Some(self.cfg.general.device_id.clone())
        } else {
            None
        }
    }

    pub(super) fn apply_playback(&mut self, p: Option<Playback>) {
        if Instant::now() < self.pb.hold_until {
            return;
        }
        self.pb.loaded = true;
        let Some(p) = p else {
            if self.pb.is_playing || self.pb.device.is_some() {
                self.pb.is_playing = false;
                self.pb.device = None;
            }
            return;
        };
        let old_uri = self.pb.track.as_ref().map(|t| t.uri.clone());
        let new_uri = p.item.as_ref().map(|t| t.uri.clone());
        // When the built-in player is active, its own events give a finer clock.
        let local = self.is_local_active();
        let drift = (self.position() as i64 - p.progress_ms as i64).abs();
        if !local || old_uri != new_uri || drift > 2500 || self.pb.is_playing != p.is_playing {
            self.pb.set_position(p.progress_ms);
        }
        self.pb.is_playing = p.is_playing;
        self.pb.shuffle = p.shuffle_state;
        self.pb.repeat = p.repeat_state;
        self.pb.volume = p.device.as_ref().and_then(|d| d.volume_percent);
        self.pb.device = p.device;
        if let Some(id) = self.pb.device.as_ref().and_then(|d| d.id.clone()) {
            self.last_device_id = Some(id);
        }
        if self.is_local_active()
            && let Some(v) = self.pb.volume
        {
            self.remember_volume(v);
        }
        self.pb.context_uri = p.context.map(|c| c.uri);
        self.pb.track = p.item;
        if old_uri != new_uri {
            self.on_track_change();
        }
    }

    pub(super) fn on_track_change(&mut self) {
        self.track_changed_at = Instant::now();
        self.saved_current = None;
        let Some(track) = self.pb.track.clone() else { return };
        if let Some(url) = track.image(300) {
            self.want_image(&url);
        }
        if let Some(id) = track.id.clone().filter(|_| !track.is_episode && self.offline.is_none()) {
            let sp = self.spotify.clone();
            self.spawn(async move {
                match sp.is_saved(&id).await {
                    Ok(saved) => AppEvent::Saved { track_id: id, saved },
                    Err(_) => AppEvent::Saved { track_id: String::new(), saved: false },
                }
            });
        }
        // Spotr queue bookkeeping: whatever we handed over has now been consumed.
        if self.queue.committed.is_some() {
            self.queue.committed = None;
        }
        self.lyrics.manual = None;
        self.ensure_lyrics();
        if self.view == MainView::Queue {
            self.fetch_spotify_queue();
        }
    }

    pub fn ensure_lyrics(&mut self) {
        let Some(t) = self.pb.track.clone() else { return };
        if self.lyrics.uri == t.uri {
            return;
        }
        self.lyrics = LyricsState { uri: t.uri.clone(), loading: true, ..Default::default() };
        if self.offline.is_some() {
            self.off_lyrics(t);
            return;
        }
        if self.spotify.is_demo() {
            self.lyrics.loading = false;
            self.lyrics.data = Some(crate::spotify::demo::lyrics());
            return;
        }
        let http = self.http.clone();
        self.spawn(async move {
            let result = crate::lyrics::fetch(&http, &t.name, &t.artist_names(), t.album_name(), t.duration_ms)
                .await
                .map_err(|e| e.to_string());
            AppEvent::Lyrics { uri: t.uri, result }
        });
    }

    // ----------------------------------------------------------- transport

    pub fn toggle_play(&mut self) {
        if self.pb.is_playing { self.pause() } else { self.resume() }
    }

    pub fn resume(&mut self) {
        self.press(Ctl::Play);
        let was = self.pb.is_playing;
        self.pb.is_playing = true;
        self.pb.at = Instant::now();
        self.pb.hold(900);
        self.osd(self.theme.icon(crate::theme::Icon::Play), "Play", None);
        if was {
            return;
        }
        if self.offline.is_some() {
            self.off_resume();
            return;
        }
        let dev = self.fallback_device();
        if self.pb.device.is_none() && dev.is_none() {
            self.pb.is_playing = false;
            self.toast(ToastKind::Warn, "No active device — pick one with Ctrl+D / `devices`");
            self.set_overlay(Overlay::Devices);
            return;
        }
        let local_ready = self.local_status == LocalStatus::Ready;
        let local_id = self.cfg.general.device_id.clone();
        self.run(None, move |sp| async move {
            match sp.play(dev.as_deref(), None).await {
                Err(e) if is_no_device(&e) && local_ready => {
                    sp.transfer(&local_id, true).await
                }
                r => r,
            }
        });
    }

    pub fn pause(&mut self) {
        self.press(Ctl::Play);
        // Freeze the clock where it is.
        let pos = self.position();
        self.pb.set_position(pos);
        self.pb.is_playing = false;
        self.pb.hold(900);
        self.osd(self.theme.icon(crate::theme::Icon::Pause), "Pause", None);
        if self.offline.is_some() {
            self.off_pause();
            return;
        }
        self.run(None, |sp| async move { sp.pause(None).await });
    }

    pub fn next(&mut self) {
        self.press(Ctl::Next);
        self.osd(self.theme.icon(crate::theme::Icon::Next), "Next", None);
        if self.offline.is_some() {
            self.off_next(true);
            return;
        }
        let dev = self.fallback_device();
        // Hand the head of the Spotr queue to Spotify first so it plays next.
        let head = if !self.spotify_queue() && self.queue.committed.is_none() && !self.queue.items.is_empty() {
            let t = self.queue.items.remove(0);
            self.queue.committed = Some(t.clone());
            Some(t.uri)
        } else {
            None
        };
        self.run(None, move |sp| async move {
            if let Some(uri) = head {
                sp.add_to_queue(&uri, dev.as_deref()).await?;
            }
            sp.next(dev.as_deref()).await
        });
        self.poll_soon(400);
    }

    pub fn previous(&mut self) {
        self.press(Ctl::Prev);
        self.osd(self.theme.icon(crate::theme::Icon::Prev), "Previous", None);
        if self.offline.is_some() {
            self.off_previous();
            return;
        }
        let dev = self.fallback_device();
        self.run(None, move |sp| async move { sp.previous(dev.as_deref()).await });
        self.poll_soon(400);
    }

    pub fn seek_to(&mut self, ms: u64) {
        let dur = self.pb.duration();
        let ms = if dur > 0 { ms.min(dur.saturating_sub(500)) } else { ms };
        self.pb.set_position(ms);
        if self.offline.is_some() {
            self.pb.hold(250);
            self.off_seek(ms);
            return;
        }
        self.pb.hold(1200);
        self.run(None, move |sp| async move { sp.seek(ms).await });
    }

    pub fn seek_by(&mut self, delta_ms: i64) {
        let pos = self.position() as i64;
        let target = (pos + delta_ms).max(0) as u64;
        let secs = delta_ms.abs() / 1000;
        let icon = if delta_ms >= 0 { "»" } else { "«" };
        let sign = if delta_ms >= 0 { "+" } else { "−" };
        let dur = self.pb.duration().max(1);
        self.osd(icon, format!("{sign}{secs}s  {}", fmt_duration(target.min(dur))), Some(target as f32 / dur as f32));
        self.seek_to(target);
    }

    /// Keep the built-in player's volume for the next session.
    fn remember_volume(&mut self, v: u32) {
        if self.cfg.playback.volume != v {
            self.cfg.playback.volume = v;
            self.volume_save_at = Some(Instant::now() + std::time::Duration::from_secs(2));
        }
    }

    pub fn set_volume(&mut self, v: u32) {
        let v = v.min(100);
        self.pb.volume = Some(v);
        if self.is_local_active() || self.pb.device.is_none() || self.offline.is_some() {
            self.remember_volume(v);
        }
        self.pb.hold(1200);
        let icon = if v == 0 { self.theme.icon(crate::theme::Icon::Mute) } else { self.theme.icon(crate::theme::Icon::Volume) };
        self.osd(icon, format!("Volume {v}%"), Some(v as f32 / 100.0));
        if self.offline.is_some() {
            self.off_volume(v);
            return;
        }
        if self.pb.device.as_ref().is_some_and(|d| !d.supports_volume) {
            self.toast(ToastKind::Warn, "This device doesn't support remote volume");
            return;
        }
        self.run(None, move |sp| async move { sp.volume(v).await });
    }

    pub fn change_volume(&mut self, delta: i32) {
        let cur = self.pb.volume.unwrap_or(50) as i32;
        self.muted_volume = None;
        self.set_volume((cur + delta).clamp(0, 100) as u32);
    }

    pub fn toggle_mute(&mut self) {
        match self.muted_volume.take() {
            Some(v) => self.set_volume(v),
            None => {
                self.muted_volume = Some(self.pb.volume.unwrap_or(50).max(5));
                self.set_volume(0);
            }
        }
    }

    pub fn set_shuffle(&mut self, on: bool) {
        self.press(Ctl::Shuffle);
        self.pb.shuffle = on;
        self.pb.hold(1200);
        self.osd(self.theme.icon(crate::theme::Icon::Shuffle), if on { "Shuffle on" } else { "Shuffle off" }, None);
        if self.offline.is_some() {
            self.off_shuffle(on);
            return;
        }
        self.run(None, move |sp| async move { sp.shuffle(on).await });
    }

    pub fn set_repeat(&mut self, state: &str) {
        self.press(Ctl::Repeat);
        let state = state.to_string();
        self.pb.repeat = state.clone();
        self.pb.hold(1200);
        let label = match state.as_str() {
            "track" => "Loop: this song",
            "context" => "Loop: on",
            _ => "Loop: off",
        };
        let icon = if state == "track" { crate::theme::Icon::RepeatOne } else { crate::theme::Icon::Repeat };
        self.osd(self.theme.icon(icon), label, None);
        if self.offline.is_some() {
            return;
        }
        self.run(None, move |sp| async move { sp.repeat(&state).await });
    }

    pub fn cycle_repeat(&mut self) {
        let next = match self.pb.repeat.as_str() {
            "off" => "context",
            "context" => "track",
            _ => "off",
        };
        self.set_repeat(next);
    }

    pub fn set_speed(&mut self, s: f32) {
        let s = (s * 100.0).round() / 100.0;
        let s = s.clamp(0.25, 3.0);
        // Keep the clock continuous across the change.
        let pos = self.position();
        self.speed = s;
        self.pb.set_position(pos);
        self.audio.set_speed(s);
        self.osd(self.theme.icon(crate::theme::Icon::Speed), format!("Speed {s}×"), Some((s - 0.25) / 2.75));
        if !self.is_local_active() && self.offline.is_none() && (s - 1.0).abs() > f32::EPSILON {
            if crate::player::AVAILABLE {
                self.toast(
                    ToastKind::Warn,
                    format!("Speed applies to the built-in player — switch to \"{}\" in devices", self.cfg.general.device_name),
                );
            } else {
                self.toast(ToastKind::Warn, "Speed control needs the built-in player (build with --features local-player)");
            }
        }
    }

    pub fn change_speed(&mut self, delta: f32) {
        self.set_speed(self.speed + delta);
    }

    /// Like / unlike a track (defaults to the current one).
    pub fn set_liked(&mut self, track: Option<Track>, like: bool) {
        let Some(track) = track.or_else(|| self.pb.track.clone()) else {
            self.toast(ToastKind::Warn, "Nothing to like");
            return;
        };
        if self.offline.is_some() {
            self.off_set_liked(track, like);
            return;
        }
        let Some(id) = track.id.clone() else {
            self.toast(ToastKind::Warn, "This item can't be saved");
            return;
        };
        let is_current = self.pb.track.as_ref().map(|t| t.uri == track.uri).unwrap_or(false);
        if is_current {
            self.press(Ctl::Like);
            self.saved_current = Some(like);
            if like
                && let Some((x, y)) = self.like_pos {
                    self.burst(x, y, &['♥', '•', '♡', '·']);
                }
        }
        let msg = if like { format!("Added “{}” to Liked Songs", track.name) } else { format!("Removed “{}” from Liked Songs", track.name) };
        self.osd(
            self.theme.icon(if like { crate::theme::Icon::Heart } else { crate::theme::Icon::HeartEmpty }),
            if like { "Liked" } else { "Unliked" },
            None,
        );
        self.liked_changed(&track, like);
        let sp = self.spotify.clone();
        self.spawn(async move {
            match sp.set_saved(&id, like).await {
                Ok(()) => AppEvent::Done { ok: Some(msg), err: None, refresh: false },
                Err(e) => AppEvent::LikeFailed { track, like, error: e.to_string() },
            }
        });
    }

    pub fn toggle_like_current(&mut self) {
        let liked = self.saved_current.unwrap_or(false);
        self.set_liked(None, !liked);
    }

    // ---------------------------------------------------------------- play

    pub fn play_request(&mut self, req: PlayRequest, label: String) {
        if self.offline.is_some() {
            // Local URIs: play them as a list from the offset.
            let Some(off) = self.offline.as_ref() else { return };
            let tracks: Vec<Track> = req
                .uris
                .unwrap_or_default()
                .iter()
                .filter_map(|u| crate::local::library::rel_of(u).and_then(|r| off.lib.track(r)).cloned())
                .collect();
            let start = req.offset_uri.as_ref().and_then(|o| tracks.iter().position(|t| &t.uri == o)).or(req.offset_position.map(|p| p as usize)).unwrap_or(0);
            self.off_play_list(tracks, start, label, req.context_uri);
            return;
        }
        let dev = self.fallback_device();
        if self.pb.device.is_none() && dev.is_none() {
            self.toast(ToastKind::Warn, "No active device — pick one first");
            self.set_overlay(Overlay::Devices);
            return;
        }
        self.pb.is_playing = true;
        self.pb.set_position(req.position_ms.unwrap_or(0));
        self.pb.hold(700);
        self.osd(self.theme.icon(crate::theme::Icon::Play), label, None);
        let fallback_uris = req.uris.clone();
        self.run(None, move |sp| async move {
            match sp.play(dev.as_deref(), Some(req.clone())).await {
                // Some contexts (e.g. Liked Songs) may be refused — fall back to a URI list.
                Err(e) if req.context_uri.is_some() && fallback_uris.is_some() && !is_no_device(&e) => {
                    let r = PlayRequest { context_uri: None, ..req };
                    sp.play(dev.as_deref(), Some(r)).await
                }
                r => r,
            }
        });
        self.poll_soon(500);
    }

    /// Spotify's song radio: the song, then what Spotify would play after it.
    /// Falls back to the song alone when the radio isn't available.
    pub fn play_radio(&mut self, track: &Track) {
        if self.offline.is_some() {
            self.off_play_radio(track);
            return;
        }
        let dev = self.fallback_device();
        if self.pb.device.is_none() && dev.is_none() {
            self.toast(ToastKind::Warn, "No active device — pick one first");
            self.set_overlay(Overlay::Devices);
            return;
        }
        self.pb.is_playing = true;
        self.pb.set_position(0);
        self.pb.hold(700);
        self.osd(self.theme.icon(crate::theme::Icon::Play), format!("{} · radio", track.name), None);
        let seed = track.uri.clone();
        let name = track.name.clone();
        let tx = self.tx.clone();
        self.run(None, move |sp| async move {
            let (uris, error) = match sp.radio(&seed).await {
                Ok(u) => (u, None),
                Err(e) => (vec![seed.clone()], Some(e.to_string())),
            };
            let _ = tx.send(AppEvent::Radio { name, uris: uris.clone(), error });
            sp.play(dev.as_deref(), Some(PlayRequest { uris: Some(uris), ..Default::default() })).await
        });
        self.poll_soon(600);
    }

    pub(super) fn on_radio(&mut self, name: String, uris: Vec<String>, error: Option<String>) {
        match error {
            Some(e) => {
                self.radio = None;
                log::info!("radio unavailable: {e}");
                self.toast(ToastKind::Info, format!("Spotify radio isn't available ({e}) — playing just “{name}”"));
            }
            None => {
                let n = uris.len().saturating_sub(1);
                self.radio = Some(RadioInfo { name: name.clone(), uris: uris.into_iter().collect() });
                self.toast(ToastKind::Success, format!("Radio for “{name}” — {n} songs picked by Spotify"));
            }
        }
    }

    pub fn play_track_alone(&mut self, track: &Track) {
        if self.offline.is_some() {
            self.off_play_now(track.clone());
            return;
        }
        self.play_request(
            PlayRequest { uris: Some(vec![track.uri.clone()]), ..Default::default() },
            track.name.clone(),
        );
    }

    pub fn play_context(&mut self, uri: &str, offset_uri: Option<String>, label: String) {
        self.play_request(
            PlayRequest { context_uri: Some(uri.to_string()), offset_uri, ..Default::default() },
            label,
        );
    }

    // --------------------------------------------------------------- queue

    /// Spotify manages "up next" (its queue, its radio) instead of Spotr.
    pub fn spotify_queue(&self) -> bool {
        self.offline.is_none() && self.cfg.playback.queue_mode == QueueMode::Spotify
    }

    pub fn set_queue_mode(&mut self, mode: QueueMode) {
        if self.offline.is_some() {
            self.toast(ToastKind::Info, "A local library always uses Spotr's queue");
            return;
        }
        if self.cfg.playback.queue_mode == mode {
            return;
        }
        self.cfg.playback.queue_mode = mode;
        self.save_config();
        self.queue.list = ListNav::default();
        self.view_changed_at = Instant::now();
        let icon = self.theme.icon(crate::theme::Icon::Queue);
        match mode {
            QueueMode::Spotify => {
                self.osd(icon, "Spotify queue", None);
                // Whatever was waiting in Spotr's queue moves over, in order.
                let moving: Vec<Track> = std::mem::take(&mut self.queue.items);
                self.queue.committed = None;
                if moving.is_empty() {
                    self.toast(ToastKind::Info, "Spotify's queue — Spotify's radio picks what plays after a searched song");
                } else {
                    let n = moving.len();
                    let dev = self.fallback_device();
                    let uris: Vec<String> = moving.into_iter().map(|t| t.uri).collect();
                    self.run(Some(format!("Moved {n} song{} to Spotify's queue", if n == 1 { "" } else { "s" })), move |sp| async move {
                        for u in &uris {
                            sp.add_to_queue(u, dev.as_deref()).await?;
                        }
                        Ok(())
                    });
                }
            }
            QueueMode::Spotr => {
                self.osd(icon, "Spotr queue", None);
                self.toast(ToastKind::Info, "Spotr's queue — reorder it freely; Spotify's own queue plays after it");
            }
        }
        self.fetch_spotify_queue();
    }

    /// Skip ahead to song `i` of Spotify's queue.
    pub fn queue_jump(&mut self, i: usize) {
        let Some(t) = self.queue.spotify.get(i).cloned() else { return };
        // The Web API can't pick a song from the queue, but skipping lands on it.
        let skips = (i + 1).min(30);
        let dev = self.fallback_device();
        self.press(Ctl::Next);
        self.osd(self.theme.icon(crate::theme::Icon::Next), format!("Skipping to “{}”", t.name), None);
        self.queue.spotify.drain(..skips.min(self.queue.spotify.len()));
        self.queue.list.selected = 0;
        self.run(None, move |sp| async move {
            for _ in 0..skips {
                sp.next(dev.as_deref()).await?;
            }
            Ok(())
        });
        self.poll_soon(500);
    }

    pub fn enqueue(&mut self, track: Track) {
        let name = track.name.clone();
        if self.spotify_queue() {
            let uri = track.uri.clone();
            let dev = self.fallback_device();
            self.osd(self.theme.icon(crate::theme::Icon::Queue), "Added to Spotify's queue", None);
            self.run(Some(format!("Queued “{name}” on Spotify")), move |sp| async move { sp.add_to_queue(&uri, dev.as_deref()).await });
            return;
        }
        self.queue.items.push(track);
        self.osd(self.theme.icon(crate::theme::Icon::Queue), "Added to queue", None);
        self.toast(ToastKind::Success, format!("Queued “{name}” (#{})", self.queue.items.len()));
    }

    /// Hand the next Spotr-queue track to Spotify shortly before the current one ends.
    pub(super) fn queue_handoff(&mut self) {
        if self.spotify_queue() || self.offline.is_some() || self.queue.items.is_empty() || self.queue.committed.is_some() || !self.pb.is_playing {
            return;
        }
        let dur = self.pb.duration();
        if dur == 0 {
            return;
        }
        let remaining = dur.saturating_sub(self.position());
        let window = (self.cfg.playback.queue_handoff as f32 * 1000.0 * self.effective_speed()) as u64;
        if remaining > window {
            return;
        }
        let t = self.queue.items.remove(0);
        self.queue.committed = Some(t.clone());
        let dev = self.fallback_device();
        let tx = self.tx.clone();
        let sp = self.spotify.clone();
        tokio::spawn(async move {
            if let Err(e) = sp.add_to_queue(&t.uri, dev.as_deref()).await {
                let _ = tx.send(AppEvent::Toast(ToastKind::Error, format!("Couldn't queue “{}”: {e}", t.name)));
            }
        });
    }

    // ------------------------------------------------------- local player

    /// Shut the built-in player down. Dropping it joins librespot's audio
    /// thread, so that happens off the UI thread.
    pub fn stop_local_player(&mut self) {
        if let Some(p) = self.local.take() {
            p.shutdown();
            std::thread::spawn(move || drop(p));
            self.spotify.set_internal(None);
        }
        self.local_status = LocalStatus::Off;
    }

    pub fn start_local_player(&mut self, interactive_login: bool) {
        if !crate::player::AVAILABLE {
            self.local_status = LocalStatus::Failed("not compiled in".into());
            return;
        }
        if matches!(self.local_status, LocalStatus::Connecting) {
            return;
        }
        self.stop_local_player();
        self.local_status = LocalStatus::Connecting;
        if let Some((at, n)) = &mut self.local_retry {
            // Rescheduled by `on_local_failed` if this attempt fails too.
            *at = Instant::now() + std::time::Duration::from_secs(3600);
            *n += 1;
        }
        let opts = LocalOptions {
            device_name: self.cfg.general.device_name.clone(),
            device_id: self.cfg.general.device_id.clone(),
            bitrate: self.cfg.general.bitrate,
            normalisation: self.cfg.general.normalisation,
            cache_dir: self.paths.librespot_dir(),
            audio: self.audio.clone(),
            volume: self.cfg.playback.volume,
        };
        let sp = self.spotify.clone();
        let http = self.http.clone();
        let tx = self.tx.clone();
        let cached = crate::player::has_cached_credentials(&opts.cache_dir);
        self.spawn_or(|msg| AppEvent::LocalReady(Err(format!("crashed while connecting: {msg}"))), async move {
            let token = if interactive_login {
                match crate::player::browser_login(&http).await {
                    Ok(t) => Some(t),
                    Err(e) => return AppEvent::LocalReady(Err(format!("sign-in failed: {e}"))),
                }
            } else if cached {
                None
            } else {
                sp.access_token().await.ok()
            };
            AppEvent::LocalReady(crate::player::start(opts, token, tx).await.map_err(|e| e.to_string()))
        });
    }

    pub(super) fn on_local_event(&mut self, e: LocalEvent) {
        match e {
            LocalEvent::Connected { device_name } => {
                log::info!("built-in player ready as {device_name}");
                // Become the active device if nothing else is playing.
                if self.cfg.general.auto_activate && !self.pb.is_playing && self.onboarding.is_none() {
                    if let Some(p) = &self.local {
                        p.activate();
                    }
                    let id = self.cfg.general.device_id.clone();
                    let has_device = self.pb.device.is_some();
                    if !has_device {
                        let sp = self.spotify.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                            let _ = sp.transfer(&id, false).await;
                        });
                    }
                }
                self.toast(ToastKind::Success, format!("Built-in player ready — “{device_name}” is now a Spotify Connect device"));
                self.poll_soon(1200);
            }
            LocalEvent::NeedsLogin => {
                self.local_status = LocalStatus::NeedsLogin;
                if let Some(o) = self.onboarding.as_mut() {
                    o.player_needs_login();
                } else {
                    self.toast(ToastKind::Warn, "The built-in player needs a one-time sign-in — run `player login`");
                }
            }
            LocalEvent::Playing { ref uri, position_ms } | LocalEvent::Paused { ref uri, position_ms } | LocalEvent::Position { ref uri, position_ms } => {
                let playing = !matches!(e, LocalEvent::Paused { .. });
                let uri = uri.clone();
                if self.is_local_active() || self.pb.device.is_none() {
                    let same = self.pb.track.as_ref().map(|t| t.uri == uri).unwrap_or(false);
                    if same && Instant::now() >= self.pb.hold_until {
                        self.pb.set_position(position_ms as u64);
                        if !matches!(e, LocalEvent::Position { .. }) {
                            self.pb.is_playing = playing;
                        }
                    } else if !same {
                        self.poll_soon(150);
                    }
                }
            }
            LocalEvent::Stopped | LocalEvent::TrackEnded | LocalEvent::TrackChanged => self.poll_soon(250),
            LocalEvent::Disconnected { session } => {
                let current = self.local.as_ref().is_some_and(|p| p.session() == session);
                if current && self.local_status == LocalStatus::Ready && self.quit_at.is_none() {
                    log::warn!("built-in player lost its Spotify session; reconnecting");
                    self.toast(ToastKind::Warn, "Built-in player lost its connection — reconnecting…");
                    self.reconnect_local_player();
                }
            }
        }
    }

    /// Restart the built-in player, retrying with backoff until it comes back.
    pub fn reconnect_local_player(&mut self) {
        if self.local_retry.is_none() {
            self.local_retry = Some((Instant::now(), 0));
        }
        self.start_local_player(false);
    }

    /// The built-in player failed to start. Returns true if a retry is scheduled.
    pub(super) fn on_local_failed(&mut self) -> bool {
        let Some((_, n)) = self.local_retry else { return false };
        let wait = (5u64 << n.min(4)).min(60);
        log::info!("built-in player reconnect #{n} failed; next try in {wait}s");
        self.local_retry = Some((Instant::now() + std::time::Duration::from_secs(wait), n));
        true
    }

    /// A player command failed because its device has disappeared.
    pub(super) fn on_device_lost(&mut self, device: Option<String>) {
        self.pb.hold_until = Instant::now();
        self.poll_now();
        let local = device.as_deref() == Some(self.cfg.general.device_id.as_str());
        if local && self.local_status == LocalStatus::Ready {
            // Our own Connect device is gone: its session died without telling us.
            self.toast(ToastKind::Warn, "The built-in player dropped off Spotify — reconnecting, try again in a moment");
            self.reconnect_local_player();
        } else {
            self.last_device_id = None;
            self.toast(ToastKind::Warn, "That device is no longer available — pick one with Ctrl+D / `devices`");
            self.set_overlay(Overlay::Devices);
        }
    }
}
