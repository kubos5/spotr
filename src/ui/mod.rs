//! Rendering. Every frame is drawn from `App` state; clickable regions are
//! recorded into `app.clicks` as we go.

mod bars;
mod browse;
mod effects;
mod nowplaying;
mod onboarding;
mod overlays;
mod views;
pub mod widgets;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};

use crate::app::{App, Focus, MainView, Overlay};

pub fn draw(f: &mut Frame, app: &mut App) {
    app.clicks.clear();
    let area = f.area();
    let buf = f.buffer_mut();

    if area.width < 30 || area.height < 12 {
        widgets::put(buf, area.x, area.y, "Spotr needs a bigger window", app.theme.dim(), area.width);
        return;
    }

    if app.onboarding.is_some() {
        onboarding::draw(app, buf, area);
        effects::toasts(app, buf, area);
        return;
    }
    if app.splash.is_some() {
        effects::splash(app, buf, area);
        return;
    }

    let mut cursor: Option<(u16, u16)> = None;
    if app.search_open {
        draw_search_mode(app, buf, area, &mut cursor);
    } else {
        draw_main_mode(app, buf, area, &mut cursor);
    }

    match app.overlay {
        Overlay::Help => overlays::help(app, buf, area),
        Overlay::Settings => overlays::settings(app, buf, area, &mut cursor),
        Overlay::About => overlays::about(app, buf, area),
        Overlay::Devices => overlays::devices(app, buf, area),
        Overlay::None => {}
    }
    if app.focus == Focus::Command {
        bars::suggestions(app, buf, area);
    }
    effects::osd(app, buf, area);
    effects::particles(app, buf, area);
    effects::toasts(app, buf, area);
    effects::quit(app, buf, area);

    if let Some((x, y)) = cursor {
        f.set_cursor_position((x, y));
    }
}

fn draw_main_mode(app: &mut App, buf: &mut ratatui::buffer::Buffer, area: Rect, cursor: &mut Option<(u16, u16)>) {
    let vis_h = if app.cfg.visualizer.enabled {
        app.cfg.visualizer.height.min(area.height.saturating_sub(16) / 2)
    } else {
        0
    };
    let hints_h = if app.cfg.appearance.show_hints { 1 } else { 0 };
    let [top, main, vis, cmd, hints] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(6),
        Constraint::Length(vis_h),
        Constraint::Length(3),
        Constraint::Length(hints_h),
    ])
    .areas(area);

    bars::search_bar(app, buf, top, cursor);
    match app.view {
        MainView::NowPlaying => nowplaying::draw(app, buf, main),
        MainView::Lyrics => views::lyrics(app, buf, main),
        MainView::Queue => views::queue(app, buf, main),
    }
    if vis_h > 0 {
        bars::visualizer(app, buf, vis);
    }
    bars::command_bar(app, buf, cmd, cursor);
    if hints_h > 0 {
        bars::hints(app, buf, hints);
    }
}

fn draw_search_mode(app: &mut App, buf: &mut ratatui::buffer::Buffer, area: Rect, cursor: &mut Option<(u16, u16)>) {
    let hints_h = if app.cfg.appearance.show_hints { 1 } else { 0 };
    let mini_h = 5;
    let [top, tabs, main, bottom, hints] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(mini_h),
        Constraint::Length(hints_h),
    ])
    .areas(area);
    bars::search_bar(app, buf, top, cursor);
    browse::tabs(app, buf, tabs);
    browse::draw(app, buf, main);
    // The command bar takes the mini player's place while it's focused.
    if app.focus == Focus::Command {
        let [_, cmd, _] = Layout::vertical([Constraint::Length(1), Constraint::Length(3), Constraint::Min(0)]).areas(bottom);
        bars::command_bar(app, buf, cmd, cursor);
    } else {
        nowplaying::mini(app, buf, bottom);
    }
    if hints_h > 0 {
        bars::hints(app, buf, hints);
    }
}
