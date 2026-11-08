//! The repository state the log provider and reference manager rely on.
//!
//! A minimal stand-in for `GitRepositoryManager` / `GitRepository.update()`
//! (`plugins/git4idea/backend/src/repo/GitRepositoryManager.kt`, `GitRepositoryImpl.kt`): per
//! root, the branch state read by [`GitRepositoryReader`] plus the remotes and the hash length.
//! Upstream updates repositories on file-system events; here [`GitRepositoryManager::update`] is
//! called by the log provider when it reads the first block with refs, and by the UI as needed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::config::{get_values, remotes_from_values};
use crate::reader::{GitBranchState, GitRepositoryReader};
use crate::repo_files::{GitRepositoryFiles, find_git_dir};
use crate::runner::Git;

/// The state of one repository (`GitRepoInfo`, partial).
#[derive(Debug, Clone)]
pub struct GitRepoInfo {
    pub root: PathBuf,
    pub state: GitBranchState,
    pub remotes: Vec<crate::config::GitRemote>,
    /// 40 for SHA-1, 64 for SHA-256 repositories (`GitRepository.fullHashLength`).
    pub full_hash_length: usize,
}

impl GitRepoInfo {
    /// `Repository.isFresh`: no commits yet.
    pub fn is_fresh(&self) -> bool {
        self.state.current_revision.is_none()
    }

    /// `getCurrentBranchName`.
    pub fn current_branch_name(&self) -> Option<&str> {
        self.state.current_branch.as_ref().map(|b| b.name())
    }

    /// Reads the state of the repository at `root`.
    pub fn read(git: &Git, root: &Path) -> GitRepoInfo {
        let git_dir = find_git_dir(root).unwrap_or_else(|| root.to_path_buf());
        let files = GitRepositoryFiles::create(root, &git_dir);
        let config = get_values(git, root).ok();
        let remotes = config.as_ref().map(remotes_from_values).unwrap_or_default();
        let object_format = config
            .as_ref()
            .and_then(|c| c.get("extensions.objectformat"))
            .and_then(|values| values.last().cloned());
        let full_hash_length = match object_format.as_deref() {
            Some(format) if format.eq_ignore_ascii_case("sha256") => 64,
            _ => 40,
        };
        let state = GitRepositoryReader::new(&files, git).read_state(&remotes);
        GitRepoInfo {
            root: root.to_path_buf(),
            state,
            remotes,
            full_hash_length,
        }
    }
}

/// Caches [`GitRepoInfo`] per root.
///
/// Roots are keys as given: they are not normalized, so pass the same `VcsRoot` values
/// consistently (`/a/b` and `/a/b/` are different roots). An unknown or non-git root gets an
/// empty cached state (no branch, no remotes). [`GitRepositoryManager::update`] is meant to be
/// called from a single refresh worker: concurrent updates for one root may leave the older read
/// cached.
pub struct GitRepositoryManager {
    git: Arc<Git>,
    repositories: RwLock<HashMap<PathBuf, Arc<GitRepoInfo>>>,
}

impl GitRepositoryManager {
    pub fn new(git: Arc<Git>) -> GitRepositoryManager {
        GitRepositoryManager {
            git,
            repositories: RwLock::new(HashMap::new()),
        }
    }

    pub fn git(&self) -> &Arc<Git> {
        &self.git
    }

    /// `getRepositoryForRoot`: the cached state, read on first use.
    pub fn get_repository(&self, root: &Path) -> Arc<GitRepoInfo> {
        if let Some(info) = self.get_repository_for_root_quick(root) {
            return info;
        }
        self.update(root)
    }

    /// `getRepositoryForRootQuick`: the cached state only.
    pub fn get_repository_for_root_quick(&self, root: &Path) -> Option<Arc<GitRepoInfo>> {
        self.repositories
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(root)
            .cloned()
    }

    /// `getRepositoryUnlessFresh`: `None` for a repository without commits.
    ///
    /// A cached fresh state is reread first: upstream gets this from file-system events, which
    /// are not ported, and without it a root first seen empty would stay empty forever. The
    /// reread only costs anything while the repository has no commits.
    pub fn get_repository_unless_fresh(&self, root: &Path) -> Option<Arc<GitRepoInfo>> {
        let mut info = self.get_repository(root);
        if info.is_fresh() {
            info = self.update(root);
        }
        (!info.is_fresh()).then_some(info)
    }

    /// `GitRepository.update()`: rereads the state.
    pub fn update(&self, root: &Path) -> Arc<GitRepoInfo> {
        let info = Arc::new(GitRepoInfo::read(&self.git, root));
        self.repositories
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(root.to_path_buf(), info.clone());
        info
    }

    /// Drops the cached state of `root` (upstream drops repositories on mapping changes).
    pub fn remove(&self, root: &Path) {
        self.repositories
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(root);
    }

    /// Drops the cached state of every root not in `roots`.
    pub fn retain_roots<P: AsRef<Path>>(&self, roots: &[P]) {
        self.repositories
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|root, _| roots.iter().any(|r| r.as_ref() == root.as_path()));
    }
}
