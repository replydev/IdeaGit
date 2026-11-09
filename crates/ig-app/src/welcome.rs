//! The welcome window: Open…, Clone (disabled for now), the recent projects list, and the
//! "Git is not installed" banner with a git path field.
//!
//! Look after IntelliJ's New UI welcome screen (`platform/platform-impl/src/com/intellij/openapi/wm/impl/welcomeScreen/`):
//! a left pane with the product name, a projects pane with action buttons on top and the recent
//! projects below. The git banner follows `GitExecutableProblemsNotifier` texts.
//!
//! Deviations: no search field, no project groups or icons, no "Customize" / "Plugins" / "Learn"
//! tabs.

use std::path::PathBuf;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyWindowHandle, App, AppContext as _, Context, Entity, Global, InteractiveElement,
    IntoElement, ParentElement, PathPromptOptions, Render, StatefulInteractiveElement, Styled,
    Window, WindowBounds, WindowOptions, div, px, size,
};
use gpui_component::Disableable as _;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputState};
use ig_ui_kit::notification::{Action, Banner, Severity};
use ig_ui_kit::{ActiveIjTheme, IjTheme, color};

use crate::actions::OpenFile;
use crate::app::{AppState, GitStatus, open_project};
use crate::strings;

/// The open welcome window, if any.
#[derive(Default)]
struct WelcomeWindow(Option<AnyWindowHandle>);

impl Global for WelcomeWindow {}

pub struct Welcome {
    git_path: Entity<InputState>,
}

/// Shows the folder picker and opens the chosen folder (`OpenFileAction`).
pub fn prompt_open(cx: &mut App) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Open".into()),
    });
    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = paths.await
            && let Some(path) = paths.into_iter().next()
        {
            cx.update(|cx| open_project(&path, cx));
        }
    })
    .detach();
}

impl Welcome {
    fn new(window: &mut Window, cx: &mut Context<Welcome>) -> Welcome {
        let current = AppState::get(cx)
            .settings
            .git_path
            .clone()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let git_path = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("git")
                .default_value(current)
        });
        cx.observe_window_appearance(window, |_, window, cx| {
            if IjTheme::sync_with_window(window, cx) {
                cx.refresh_windows();
            }
        })
        .detach();
        Welcome { git_path }
    }

    fn git_banner(&self, cx: &App) -> Option<Banner> {
        let text = match &AppState::get(cx).git {
            GitStatus::Ok { .. } => return None,
            GitStatus::Missing(_) => format!(
                "{} {}",
                strings::text("GitBundle", "executable.error.git.not.installed"),
                strings::text("GitBundle", "executable.error.git.not.installed.hint")
            ),
            GitStatus::Unsupported(version) => format!(
                "{}. {}",
                strings::message(
                    "GitBundle",
                    "git.executable.validation.error.version.title",
                    &[&version.presentation()]
                ),
                strings::message(
                    "GitBundle",
                    "git.executable.validation.error.version.message",
                    &[&ig_git::version::GitVersion::min().presentation()]
                ),
            ),
        };
        Some(Banner::new("git-banner", Severity::Warning, text))
    }

    fn save_git_path(&mut self, cx: &mut Context<Welcome>) {
        let text = self.git_path.read(cx).value().trim().to_string();
        AppState::set_git_path((!text.is_empty()).then(|| PathBuf::from(text)), cx);
        cx.notify();
    }
}

impl Render for Welcome {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Welcome>) -> impl IntoElement {
        let theme = cx.ij();
        let t = theme.tokens;
        let state = AppState::get(cx);
        let git_ok = state.git.is_ok();
        let recents = state.settings.recent_projects.clone();
        let banner = self.git_banner(cx);
        let recent_items = recents.into_iter().enumerate().map(|(ix, path)| {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let full = path.display().to_string();
            div()
                .id(("recent", ix))
                .flex()
                .items_center()
                .gap(px(10.0))
                .px(px(12.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .hover(|d| d.bg(color(t.list_hover_background)))
                .child(ig_ui_kit::icons::icon(
                    "nodes/ideaProject",
                    theme.dark,
                    20.0,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(name))
                        .child(div().text_color(color(t.secondary_foreground)).child(full)),
                )
                .on_click(move |_, _, cx| {
                    let path = path.clone();
                    // outside this window's update: `open_project` may prompt in it
                    cx.defer(move |cx| open_project(&path, cx));
                })
        });
        div()
            .id("welcome")
            .size_full()
            .flex()
            .bg(color(t.welcome_background))
            .text_color(color(t.foreground))
            .font_family(ig_ui_kit::fonts::UI_FAMILY)
            .text_size(px(ig_ui_kit::metrics::UI_FONT_SIZE))
            .on_action(|_: &OpenFile, _, cx| prompt_open(cx))
            .child(
                div()
                    .w(px(220.0))
                    .h_full()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .p(px(20.0))
                    .pt(px(40.0))
                    .border_r_1()
                    .border_color(color(t.border))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("IdeaGit"),
                    )
                    .child(
                        div()
                            .text_color(color(t.secondary_foreground))
                            .child(env!("CARGO_PKG_VERSION")),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .p(px(20.0))
                    .pt(px(40.0))
                    .bg(color(t.welcome_details_background))
                    .when_some(banner, |d, banner| {
                        let this = cx.entity().downgrade();
                        d.child(banner.action(Action::new("Save", move |_, _, cx| {
                            let _ = this.update(cx, |this, cx| this.save_git_path(cx));
                        })))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child("Path to Git executable:")
                                .child(div().flex_1().child(Input::new(&self.git_path))),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(
                                Button::new("open")
                                    .primary()
                                    .label("Open\u{2026}")
                                    .disabled(!git_ok)
                                    .on_click(|_, _, cx| prompt_open(cx)),
                            )
                            .child(
                                Button::new("clone")
                                    .label("Clone Repository\u{2026}")
                                    .disabled(true),
                            ),
                    )
                    .child(
                        div()
                            .id("recents")
                            .flex()
                            .flex_col()
                            .overflow_y_scroll()
                            .children(recent_items),
                    ),
            )
    }
}

/// Opens (or focuses) the welcome window.
pub fn open(cx: &mut App) {
    if let Some(handle) = cx.try_global::<WelcomeWindow>().and_then(|w| w.0)
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(gpui::Bounds::centered(
            None,
            size(px(800.0), px(650.0)),
            cx,
        ))),
        app_id: Some("IdeaGit".into()),
        ..Default::default()
    };
    match cx.open_window(options, |window, cx| {
        let welcome = cx.new(|cx| Welcome::new(window, cx));
        cx.new(|cx| gpui_component::Root::new(welcome, window, cx))
    }) {
        Ok(handle) => cx.set_global(WelcomeWindow(Some(handle.into()))),
        Err(e) => log::error!("opening the welcome window: {e}"),
    }
}

/// Closes the welcome window (a project was opened).
pub fn close(cx: &mut App) {
    if let Some(handle) = cx.try_global::<WelcomeWindow>().and_then(|w| w.0) {
        cx.set_global(WelcomeWindow(None));
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

/// The welcome window, if open.
pub fn handle(cx: &App) -> Option<AnyWindowHandle> {
    cx.try_global::<WelcomeWindow>().and_then(|w| w.0)
}

/// Whether `closed` is the welcome window; if so, forgets it.
pub fn forget_if(closed: gpui::WindowId, cx: &mut App) -> bool {
    let is_welcome = handle(cx).is_some_and(|h| h.window_id() == closed);
    if is_welcome {
        cx.set_global(WelcomeWindow(None));
    }
    is_welcome
}
