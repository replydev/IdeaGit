//! IdeaGit's application: windows, actions, the Log (in the project window's main area), the
//! Console tool window and the log worker. Built on `ig-git` / `ig-log` for Git and `ig-ui-kit` for
//! the IntelliJ New UI look.
//!
//! Every module names the upstream sources it ports or imitates; see also
//! `reference/upstream-map.toml`.

pub mod actions;
pub mod app;
pub mod console;
pub mod log;
pub mod logging;
pub mod menus;
pub mod paths;
pub mod project;
#[cfg(feature = "screenshots")]
pub mod screenshots;
pub mod settings;
pub mod strings;
pub mod welcome;

use std::path::PathBuf;

use gpui::App;

use crate::actions::{About, Exit, OpenFile};

/// Registers IdeaGit's globals, theme, fonts, actions, keymap and menus, with the user's settings.
pub fn init(cx: &mut App) {
    init_with_settings(paths::settings_file(), cx);
}

/// [`init`] with the settings read from (and saved to) `settings_path`: tests and screenshots use
/// a scratch file so they never touch the user's settings.
pub fn init_with_settings(settings_path: PathBuf, cx: &mut App) {
    gpui_component::init(cx);
    if let Err(e) = ig_ui_kit::init(cx.window_appearance(), cx) {
        ::log::error!("loading fonts: {e}");
    }
    app::AppState::init(settings_path, cx);
    actions::bind_keys(cx);
    cx.on_action(|_: &OpenFile, cx| welcome::prompt_open(cx));
    cx.on_action(|_: &Exit, cx| cx.quit());
    cx.on_action(|_: &About, cx| cx.defer(show_about));
    cx.on_action(|action: &menus::OpenRecent, cx| {
        let path = action.path.clone();
        // outside the dispatching window's update: `open_project` may activate or prompt in it
        cx.defer(move |cx| app::open_project(&path, cx));
    });
    menus::install(cx);
    cx.on_window_closed(|cx, closed| app::on_window_closed(closed, cx))
        .detach();
}

fn show_about(cx: &mut App) {
    let Some(window) = cx
        .active_window()
        .or_else(|| cx.windows().into_iter().next())
    else {
        return;
    };
    let detail = format!(
        "Version {}\nA Rust port of IntelliJ IDEA's Git features (intellij-community {}).",
        env!("CARGO_PKG_VERSION"),
        &crate::UPSTREAM_COMMIT[..7]
    );
    let _ = window.update(cx, |_, window, cx| {
        // the answer isn't needed: the dialog only has OK
        drop(window.prompt(
            gpui::PromptLevel::Info,
            "IdeaGit",
            Some(&detail),
            &["OK"],
            cx,
        ));
    });
}

/// The upstream commit the port matches (`reference/UPSTREAM`).
pub const UPSTREAM_COMMIT: &str = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98";

/// `ideagit [path]`: opens `path`, or the welcome window.
pub fn run(path: Option<PathBuf>) {
    gpui_platform::application()
        .with_assets(ig_ui_kit::icons::Assets)
        .run(move |cx: &mut App| {
            init(cx);
            match &path {
                Some(path) => app::open_project(path, cx),
                None => welcome::open(cx),
            }
            cx.activate(true);
        });
}
