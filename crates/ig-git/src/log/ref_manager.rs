//! Reference types and ordering for git.
//!
//! Port of `plugins/git4idea/backend/src/log/GitRefManager.kt`: ref types, `getRefType`, the label
//! order comparator and the branch layout comparator.
//!
//! Not ported (UI or other plans): ref groups for the table and the branch filter
//! (`groupForTable`, `groupForBranchFilter`, tracked groups), favorites, ref type serialization
//! and the label colors.

use std::cmp::Ordering;
use std::sync::Arc;

use ig_log::data::{VcsLogRefManager, VcsRef, VcsRefType, compare_roots};

use crate::log::repository_manager::GitRepositoryManager;
use crate::natural_comparator::natural_compare;
use crate::refs::{REFS_HEADS_PREFIX, REFS_REMOTES_PREFIX, REFS_TAGS_PREFIX};

pub const HEAD: VcsRefType = VcsRefType::new("HEAD", true);
pub const LOCAL_BRANCH: VcsRefType = VcsRefType::new("LOCAL_BRANCH", true);
pub const REMOTE_BRANCH: VcsRefType = VcsRefType::new("REMOTE_BRANCH", true);
pub const TAG: VcsRefType = VcsRefType::new("TAG", false);
pub const OTHER: VcsRefType = VcsRefType::new("OTHER", false);

pub const MASTER: &str = "master";
pub const MAIN: &str = "main";
pub const ORIGIN_MASTER: &str = "origin/master";
pub const ORIGIN_MAIN: &str = "origin/main";

/// `getRefType`: the type of a full ref name (`refs/heads/...`, `refs/remotes/...`,
/// `refs/tags/...`, `HEAD...`).
pub fn get_ref_type(ref_name: &str) -> VcsRefType {
    if ref_name.starts_with(REFS_HEADS_PREFIX) {
        return LOCAL_BRANCH;
    }
    if ref_name.starts_with(REFS_REMOTES_PREFIX) {
        return REMOTE_BRANCH;
    }
    if ref_name.starts_with(REFS_TAGS_PREFIX) {
        return TAG;
    }
    if ref_name.starts_with("HEAD") {
        return HEAD;
    }
    OTHER
}

/// `GitRefManager.RefType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefType {
    Other,
    Head,
    CurrentBranch,
    Tag,
    LocalBranch,
    Master,
    RemoteBranch,
    OriginMaster,
}

/// `GitLabelComparator.orderedTypes`.
const LABEL_ORDER: [RefType; 8] = [
    RefType::Head,
    RefType::CurrentBranch,
    RefType::Master,
    RefType::OriginMaster,
    RefType::LocalBranch,
    RefType::RemoteBranch,
    RefType::Tag,
    RefType::Other,
];

/// `GitBranchLayoutComparator.orderedTypes`.
const BRANCH_LAYOUT_ORDER: [RefType; 8] = [
    RefType::OriginMaster,
    RefType::RemoteBranch,
    RefType::Master,
    RefType::LocalBranch,
    RefType::Tag,
    RefType::CurrentBranch,
    RefType::Head,
    RefType::Other,
];

/// `GitRefComparator.getType`.
fn base_ref_type(vcs_ref: &VcsRef) -> RefType {
    let ref_type = vcs_ref.ref_type();
    let name = vcs_ref.name();
    if ref_type == HEAD {
        RefType::Head
    } else if ref_type == TAG {
        RefType::Tag
    } else if ref_type == LOCAL_BRANCH {
        if name == MASTER || name == MAIN {
            RefType::Master
        } else {
            RefType::LocalBranch
        }
    } else if ref_type == REMOTE_BRANCH {
        if name == ORIGIN_MASTER || name == ORIGIN_MAIN {
            RefType::OriginMaster
        } else {
            RefType::RemoteBranch
        }
    } else {
        RefType::Other
    }
}

/// `GitRefComparator.compare`: by type power, then by name (natural order), then by root.
fn compare_refs(
    ordered_types: &[RefType; 8],
    get_type: impl Fn(&VcsRef) -> RefType,
    ref1: &VcsRef,
    ref2: &VcsRef,
) -> Ordering {
    let power = |r: &VcsRef| ordered_types.iter().position(|t| *t == get_type(r));
    let (power1, power2) = (power(ref1), power(ref2));
    if power1 != power2 {
        return power1.cmp(&power2);
    }
    let names_comparison = natural_compare(ref1.name(), ref2.name());
    if names_comparison != Ordering::Equal {
        return names_comparison;
    }
    compare_roots(ref1.root(), ref2.root())
}

/// The git [`VcsLogRefManager`] (`GitRefManager`).
pub struct GitRefManager {
    repository_manager: Arc<GitRepositoryManager>,
}

impl GitRefManager {
    pub fn new(repository_manager: Arc<GitRepositoryManager>) -> GitRefManager {
        GitRefManager { repository_manager }
    }

    /// `GitLabelComparator.isCurrentBranch`.
    fn is_current_branch(&self, vcs_ref: &VcsRef) -> bool {
        let Some(repo) = self
            .repository_manager
            .get_repository_for_root_quick(vcs_ref.root())
        else {
            return false;
        };
        repo.current_branch_name() == Some(vcs_ref.name())
    }

    /// `GitLabelComparator.getType`: local branches (including master) that are current.
    fn label_ref_type(&self, vcs_ref: &VcsRef) -> RefType {
        let ref_type = base_ref_type(vcs_ref);
        if (ref_type == RefType::LocalBranch || ref_type == RefType::Master)
            && self.is_current_branch(vcs_ref)
        {
            return RefType::CurrentBranch;
        }
        ref_type
    }
}

impl VcsLogRefManager for GitRefManager {
    fn branch_layout_compare(&self, ref1: &VcsRef, ref2: &VcsRef) -> Ordering {
        compare_refs(&BRANCH_LAYOUT_ORDER, base_ref_type, ref1, ref2)
    }

    fn labels_order_compare(&self, ref1: &VcsRef, ref2: &VcsRef) -> Ordering {
        compare_refs(&LABEL_ORDER, |r| self.label_ref_type(r), ref1, ref2)
    }

    fn name(&self) -> String {
        "GitRefManager".to_string()
    }
}
