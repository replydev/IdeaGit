//! Application settings persisted in `settings.toml`: recent projects and the git executable.
//!
//! Upstream keeps these in `recentProjects.xml` (`RecentProjectsManagerBase`) and `git.xml`
//! (`GitVcsApplicationSettings.myPathToGit`). IdeaGit uses one TOML file in the OS config
//! directory ([`crate::paths::config_dir`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Upstream `RecentProjectsManagerBase` keeps 50 entries (`ide.max.recent.projects`).
pub const MAX_RECENT_PROJECTS: usize = 50;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Git executable; `None` means `git` from `PATH` (upstream's auto-detected path).
    pub git_path: Option<PathBuf>,
    /// Most recent first.
    pub recent_projects: Vec<PathBuf>,
}

impl Settings {
    /// Reads `path`; a missing or unreadable file gives the defaults (logged).
    pub fn load(path: &Path) -> Settings {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                log::warn!("ignoring malformed {}: {e}", path.display());
                Settings::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                log::warn!("can't read {}: {e}", path.display());
                Settings::default()
            }
        }
    }

    /// Writes `path` atomically (temp file + rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)
    }

    /// Moves `project` to the top of the recent list (upstream `addRecentPath`).
    pub fn add_recent(&mut self, project: &Path) {
        self.recent_projects.retain(|p| p != project);
        self.recent_projects.insert(0, project.to_path_buf());
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);
    }

    pub fn remove_recent(&mut self, project: &Path) {
        self.recent_projects.retain(|p| p != project);
    }

    /// The git executable to run.
    pub fn git_executable(&self) -> PathBuf {
        self.git_path
            .clone()
            .unwrap_or_else(|| PathBuf::from("git"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_projects_round_trip_most_recent_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let mut settings = Settings::load(&path);
        assert_eq!(settings, Settings::default());
        settings.add_recent(Path::new("/a"));
        settings.add_recent(Path::new("/b"));
        settings.add_recent(Path::new("/a"));
        settings.git_path = Some(PathBuf::from("/opt/git/bin/git"));
        settings.save(&path).unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(
            loaded.recent_projects,
            [PathBuf::from("/a"), PathBuf::from("/b")]
        );
        assert_eq!(loaded.git_executable(), PathBuf::from("/opt/git/bin/git"));
    }

    #[test]
    fn recent_list_is_bounded_and_malformed_files_are_ignored() {
        let mut settings = Settings::default();
        for i in 0..60 {
            settings.add_recent(Path::new(&format!("/p{i}")));
        }
        assert_eq!(settings.recent_projects.len(), MAX_RECENT_PROJECTS);
        assert_eq!(settings.recent_projects[0], PathBuf::from("/p59"));
        settings.remove_recent(Path::new("/p59"));
        assert_eq!(settings.recent_projects[0], PathBuf::from("/p58"));

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "recent_projects = 3").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }
}
