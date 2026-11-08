//! The interface a VCS implements to feed the log, and the reference manager it supplies.
//!
//! Ported from the data-reading parts of `platform/vcs-log/api/src/com/intellij/vcs/log/VcsLogProvider.kt`,
//! `VcsLogRefManager.java`, `VcsLogProviderRequirementsEx.java` (same package),
//! `platform/vcs-log/impl/src/com/intellij/vcs/log/impl/RequirementsImpl.java`,
//! `SimpleLogProviderRequirements.kt` and `LogDataImpl.java`.
//!
//! Deviations:
//! - Calls are synchronous (`readRecentCommits` is the `readFirstBlock` default; there is no
//!   experimental refs loading policy). The refresher runs them on the caller's thread.
//! - Not ported yet: filters (`getCommitsMatchingFilter`), full details (`readFullDetails`),
//!   containing branches, diff/file-history handlers, root refresh subscription (the UI calls
//!   [`VcsLogRefresher::refresh`](crate::data::VcsLogRefresher::refresh) after repository changes),
//!   ref grouping for the table and branch filter, favorites and ref type serialization.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::Arc;

use crate::data::commit::{TimedVcsCommit, VcsCommitMetadata};
use crate::data::root::VcsRoot;
use crate::data::vcs_ref::{VcsRef, VcsRefsContainer};
use crate::data::vcs_user::VcsUser;

/// A VCS operation failed (`VcsException`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct VcsException(pub String);

/// Orders references (`VcsLogRefManager`, comparators only).
pub trait VcsLogRefManager: Send + Sync {
    /// `getBranchLayoutComparator`: the most important branch is the smallest; it is laid out
    /// leftmost and gives its color to the commits below it.
    fn branch_layout_compare(&self, ref1: &VcsRef, ref2: &VcsRef) -> Ordering;

    /// `getLabelsOrderComparator`: the order of reference labels on a commit row.
    fn labels_order_compare(&self, ref1: &VcsRef, ref2: &VcsRef) -> Ordering;

    /// Upstream `toString()`, used to order heads of different VCSs.
    fn name(&self) -> String;
}

/// What a refresh may rely on (`VcsLogProviderRequirementsEx`).
pub struct RequirementsEx<'a> {
    /// Refresh rather than initialization.
    pub refresh: bool,
    /// Whether all refs must be reloaded.
    pub refresh_refs: bool,
    /// The refs that were in the log before the refresh.
    pub previous_refs: &'a dyn VcsRefsContainer,
}

/// `VcsLogProvider.Requirements`: how many recent commits to read, plus refresh information for
/// [`RequirementsImpl`](Requirements::refresh) (`None` for `SimpleLogProviderRequirements`).
pub struct Requirements<'a> {
    pub commit_count: usize,
    pub ex: Option<RequirementsEx<'a>>,
}

impl<'a> Requirements<'a> {
    /// `SimpleLogProviderRequirements`.
    pub fn simple(commit_count: usize) -> Requirements<'static> {
        Requirements {
            commit_count,
            ex: None,
        }
    }

    /// `RequirementsImpl(count, refresh, previousRefs)`: refs are reloaded.
    pub fn refresh(
        commit_count: usize,
        refresh: bool,
        previous_refs: &'a dyn VcsRefsContainer,
    ) -> Requirements<'a> {
        Requirements::refresh_with(commit_count, refresh, previous_refs, true)
    }

    /// `RequirementsImpl(count, refresh, previousRefs, isRefreshRefs)`.
    pub fn refresh_with(
        commit_count: usize,
        refresh: bool,
        previous_refs: &'a dyn VcsRefsContainer,
        refresh_refs: bool,
    ) -> Requirements<'a> {
        Requirements {
            commit_count,
            ex: Some(RequirementsEx {
                refresh,
                refresh_refs,
                previous_refs,
            }),
        }
    }
}

/// Recent commits with metadata and the references (`DetailedLogData` / `LogDataImpl`).
#[derive(Clone, Debug, Default)]
pub struct DetailedLogData {
    pub commits: Vec<VcsCommitMetadata>,
    pub refs: Vec<VcsRef>,
}

/// All references and users of a repository (`LogData` / `LogDataImpl`).
#[derive(Clone, Debug, Default)]
pub struct LogData {
    pub refs: HashSet<VcsRef>,
    pub users: HashSet<VcsUser>,
}

/// Supplies the log data of a VCS (`VcsLogProvider`, data-reading part).
pub trait VcsLogProvider: Send + Sync {
    /// `readFirstBlock` (and the default `readRecentCommits`): the most recent commits, at least
    /// topologically ordered, with all references (or, without
    /// [`RequirementsEx::refresh_refs`], those found on these commits).
    fn read_first_block(
        &self,
        root: &VcsRoot,
        requirements: &Requirements<'_>,
    ) -> Result<DetailedLogData, VcsException>;

    /// `readAllHashes`: reports every commit, in topological order, to `commit_consumer`; returns
    /// all references and users.
    fn read_all_hashes(
        &self,
        root: &VcsRoot,
        commit_consumer: &mut dyn FnMut(TimedVcsCommit),
    ) -> Result<LogData, VcsException>;

    /// `readMetadata`: the metadata of the given commits.
    fn read_metadata(
        &self,
        root: &VcsRoot,
        hashes: &[String],
        consumer: &mut dyn FnMut(VcsCommitMetadata),
    ) -> Result<(), VcsException>;

    /// `getReferenceManager`. Roots sharing a provider must return the same `Arc`.
    fn reference_manager(&self) -> Arc<dyn VcsLogRefManager>;

    /// `getCurrentUser`: the configured user, or `None` if no user name is configured.
    fn get_current_user(&self, root: &VcsRoot) -> Result<Option<VcsUser>, VcsException>;

    /// `getCurrentBranch`: the current branch, `HEAD` when detached, `None` without commits.
    fn get_current_branch(&self, root: &VcsRoot) -> Option<String>;

    /// `isFullHash`: whether `hash` is a full commit hash in `root` (SHA-1 by default).
    fn is_full_hash(&self, _root: &VcsRoot, hash: &str) -> bool {
        hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit())
    }
}
