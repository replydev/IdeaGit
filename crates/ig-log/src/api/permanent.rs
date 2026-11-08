//! Access to the permanent graph's commits, linear graph and layout.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/permanent/PermanentCommitsInfo.kt`
//! and `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/permanent/PermanentGraphInfo.java`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::api::GraphLayout;
use crate::api::linear_graph::LinearGraph;

/// Commit node id: the row of a loaded commit in the permanent graph (`>= 0`), or a synthetic
/// negative id (`<= -2`) for a parent that is not loaded. `-1` means "unknown".
pub type VcsLogGraphNodeId = i32;

/// Maps node ids of the permanent graph to commit ids and timestamps.
pub trait PermanentCommitsInfo<CommitId>: Send + Sync {
    fn get_commit_id(&self, node_id: VcsLogGraphNodeId) -> CommitId;

    /// Non-panicking [`get_commit_id`](Self::get_commit_id): `None` if `node_id` is neither a row
    /// nor a known not-loaded id.
    fn try_get_commit_id(&self, node_id: VcsLogGraphNodeId) -> Option<CommitId>;

    fn get_timestamp(&self, node_id: VcsLogGraphNodeId) -> i64;

    /// See [`PermanentCommitsInfoImpl::get_node_id`](crate::permanent::permanent_commits_info_impl::PermanentCommitsInfoImpl::get_node_id)
    /// for the possible values.
    fn get_node_id(&self, commit_id: &CommitId) -> VcsLogGraphNodeId;

    fn convert_to_node_ids(&self, commit_ids: &HashSet<CommitId>) -> HashSet<VcsLogGraphNodeId>;
}

/// Everything a visible graph needs to know about the permanent graph it was created from.
pub trait PermanentGraphInfo<CommitId>: Send + Sync {
    fn permanent_commits_info(&self) -> &dyn PermanentCommitsInfo<CommitId>;

    /// Returns a shared handle (upstream returns the object itself).
    fn linear_graph(&self) -> Arc<dyn LinearGraph>;

    fn permanent_graph_layout(&self) -> &dyn GraphLayout;

    fn branch_node_ids(&self) -> &HashSet<VcsLogGraphNodeId>;
}
