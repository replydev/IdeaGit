//! Application settings persisted in `settings.toml`: recent projects and the git executable.
//!
//! Upstream keeps these in `recentProjects.xml` (`RecentProjectsManagerBase`) and `git.xml`
//! (`GitVcsApplicationSettings.myPathToGit`). IdeaGit uses one TOML file in the OS config
//! directory ([`crate::paths::config_dir`]).

use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

/// Strips `.` components and trailing separators (no filesystem access).
fn normalize(path: &Path) -> PathBuf {
    let normalized: PathBuf = path
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect();
    if normalized.as_os_str().is_empty() {
        path.to_path_buf()
    } else {
        normalized
    }
}

impl Settings {
    /// Reads `path`; a missing or unreadable file gives the defaults (logged). A malformed file
    /// is moved aside to `<path>.corrupt` (replacing an older one) so the next save doesn't
    /// destroy it.
    pub fn load(path: &Path) -> Settings {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                let mut corrupt = path.as_os_str().to_os_string();
                corrupt.push(".corrupt");
                let corrupt = PathBuf::from(corrupt);
                match std::fs::rename(path, &corrupt) {
                    Ok(()) => log::warn!(
                        "malformed {}: {e}; moved to {}, using defaults",
                        path.display(),
                        corrupt.display()
                    ),
                    Err(re) => log::warn!(
                        "malformed {}: {e}; can't move it aside ({re}), using defaults",
                        path.display()
                    ),
                }
                Settings::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => {
                log::warn!("can't read {}: {e}", path.display());
                Settings::default()
            }
        }
    }

    /// Writes `path` atomically: a unique temp file in the same directory, flushed to disk, then
    /// renamed over `path` (removed again on failure).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = path.parent().filter(|d| !d.as_os_str().is_empty());
        if let Some(dir) = dir {
            std::fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let tmp = path.with_file_name(name);
        let write = || -> std::io::Result<()> {
            let mut file = std::fs::File::create(&tmp)?;
            file.write_all(text.as_bytes())?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&tmp, path)
        };
        write().inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    }

    /// Moves `project` to the top of the recent list (upstream `addRecentPath`). Paths are
    /// compared after dropping `.` components and trailing separators.
    pub fn add_recent(&mut self, project: &Path) {
        let project = normalize(project);
        self.recent_projects.retain(|p| normalize(p) != project);
        self.recent_projects.insert(0, project);
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);
    }

    pub fn remove_recent(&mut self, project: &Path) {
        let project = normalize(project);
        self.recent_projects.retain(|p| normalize(p) != project);
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

    #[test]
    fn malformed_file_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let corrupt = dir.path().join("settings.toml.corrupt");
        std::fs::write(&corrupt, "older").unwrap();
        std::fs::write(&path, "recent_projects = 3").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        assert!(!path.exists());
        assert_eq!(
            std::fs::read_to_string(corrupt).unwrap(),
            "recent_projects = 3"
        );
    }

    #[test]
    fn save_leaves_no_temp_files_and_replaces_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let mut settings = Settings::default();
        settings.add_recent(Path::new("/a"));
        settings.save(&path).unwrap();
        settings.add_recent(Path::new("/b"));
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).recent_projects.len(), 2);
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["settings.toml"]);
        // failure (target is a directory): temp removed
        let blocked = dir.path().join("blocked");
        std::fs::create_dir(&blocked).unwrap();
        assert!(settings.save(&blocked).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn recent_paths_are_deduped_after_normalizing() {
        let mut settings = Settings::default();
        settings.add_recent(Path::new("/a/b"));
        settings.add_recent(Path::new("/a/b/"));
        settings.add_recent(Path::new("/a/./b"));
        assert_eq!(settings.recent_projects, [PathBuf::from("/a/b")]);
        settings.remove_recent(Path::new("/a/b/."));
        assert!(settings.recent_projects.is_empty());
    }
}
