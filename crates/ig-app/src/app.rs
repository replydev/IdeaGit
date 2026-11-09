//! Application state (settings, the detected git) and opening windows.
//!
//! Git detection follows `GitExecutableManager` / `GitVersion` checks: `git version` is run once at
//! startup (and again when the git path changes); a missing or too old git shows the welcome
//! window's banner instead of opening projects (`GitExecutableProblemsNotifier`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui::{App, Global};
use ig_git::console::NullConsole;
use ig_git::runner::Git;
use ig_git::version::GitVersion;

use crate::settings::Settings;
use crate::{menus, paths, project, welcome};

/// How deep `detect_roots` looks for nested repositories in an opened folder.
pub const ROOTS_SCAN_DEPTH: usize = 3;

/// The result of `Git::detect`.
#[derive(Clone, Debug)]
pub enum GitStatus {
    Ok {
        executable: PathBuf,
        version: GitVersion,
    },
    /// `git version` couldn't run (missing executable); the error text.
    Missing(String),
    Unsupported(GitVersion),
}

impl GitStatus {
    pub fn detect(executable: &Path) -> GitStatus {
        match Git::detect(executable, Arc::new(NullConsole)) {
            Ok(git) if git.version().is_supported() => GitStatus::Ok {
                executable: executable.to_path_buf(),
                version: git.version(),
            },
            Ok(git) => GitStatus::Unsupported(git.version()),
            Err(e) => GitStatus::Missing(e.to_string()),
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, GitStatus::Ok { .. })
    }
}

pub struct AppState {
    pub settings: Settings,
    pub settings_path: PathBuf,
    pub git: GitStatus,
}

impl Global for AppState {}

impl AppState {
    pub fn init(cx: &mut App) {
        let settings_path = paths::settings_file();
        let settings = Settings::load(&settings_path);
        let git = GitStatus::detect(&settings.git_executable());
        log::info!("git: {git:?}");
        cx.set_global(AppState {
            settings,
            settings_path,
            git,
        });
    }

    pub fn get(cx: &App) -> &AppState {
        cx.global::<AppState>()
    }

    pub fn save(cx: &mut App) {
        let state = cx.global::<AppState>();
        if let Err(e) = state.settings.save(&state.settings_path) {
            log::warn!("saving settings: {e}");
        }
    }

    /// Sets the git executable (from the welcome screen), re-detects and saves.
    pub fn set_git_path(path: Option<PathBuf>, cx: &mut App) {
        let state = cx.global_mut::<AppState>();
        state.settings.git_path = path;
        state.git = GitStatus::detect(&state.settings.git_executable());
        AppState::save(cx);
    }
}

/// Opens `path` in a project window (or the welcome window if git isn't usable).
pub fn open_project(path: &Path, cx: &mut App) {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let GitStatus::Ok {
        executable,
        version,
    } = AppState::get(cx).git.clone()
    else {
        welcome::open(cx);
        return;
    };
    cx.global_mut::<AppState>().settings.add_recent(&path);
    AppState::save(cx);
    menus::install(cx);
    let roots = ig_git::roots::detect_roots(&path, ROOTS_SCAN_DEPTH);
    log::info!("opening {} with roots {roots:?}", path.display());
    project::open(path, roots, executable, version, cx);
}

/// When the last window closes: closing the last project shows the welcome window (as IntelliJ
/// does); closing the welcome window quits.
pub fn on_window_closed(cx: &mut App) {
    if !cx.windows().is_empty() {
        return;
    }
    if welcome::take_open(cx) {
        cx.quit();
    } else {
        welcome::open(cx);
    }
}
