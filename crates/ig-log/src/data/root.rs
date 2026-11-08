//! Repository roots.
//!
//! Upstream identifies a repository by its root `VirtualFile`. Here a root is a shared path,
//! cheap to clone into every [`VcsRef`](crate::data::VcsRef) and
//! [`CommitId`](crate::data::CommitId).

use std::cmp::Ordering;
use std::path::Path;
use std::sync::Arc;

/// The root directory of a repository.
pub type VcsRoot = Arc<Path>;

/// Creates a [`VcsRoot`].
pub fn vcs_root(path: impl AsRef<Path>) -> VcsRoot {
    Arc::from(path.as_ref())
}

/// `VcsLogUtil.compareRoots`: compares the presentable paths as strings.
pub fn compare_roots(root1: &VcsRoot, root2: &VcsRoot) -> Ordering {
    root1.to_string_lossy().cmp(&root2.to_string_lossy())
}
