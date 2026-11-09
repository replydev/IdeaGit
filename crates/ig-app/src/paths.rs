//! Where IdeaGit keeps its files: settings in the OS config directory, `ideagit.log` in the OS log
//! directory (macOS `~/Library/Logs/IdeaGit`, Linux `$XDG_STATE_HOME/IdeaGit` or
//! `~/.local/state/IdeaGit`, Windows `%LOCALAPPDATA%\IdeaGit\logs`).
//!
//! `IDEAGIT_CONFIG_DIR` overrides the config directory (tests, screenshots).

use std::path::PathBuf;

const APP_DIR: &str = "IdeaGit";

pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("IDEAGIT_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR)
}

pub fn settings_file() -> PathBuf {
    config_dir().join("settings.toml")
}

pub fn log_dir() -> PathBuf {
    if cfg!(target_os = "macos")
        && let Some(home) = dirs::home_dir()
    {
        return home.join("Library/Logs").join(APP_DIR);
    }
    if cfg!(windows)
        && let Some(dir) = dirs::data_local_dir()
    {
        return dir.join(APP_DIR).join("logs");
    }
    dirs::state_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/state")))
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_DIR)
}

pub fn log_file() -> PathBuf {
    log_dir().join("ideagit.log")
}
