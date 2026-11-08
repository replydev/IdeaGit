//! Branch colors.
//!
//! Ported from `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/GraphColorManager.java`.

/// Supplies color ids for fragments of branches.
pub trait GraphColorManager<CommitId>: Send + Sync {
    /// Color for drawing a fragment of a branch, based on the branch head, the index of the head
    /// commit's fragment and the index of the target fragment. This allows coloring non-main
    /// fragments separately (e.g. a feature branch merged into the main branch), even though they
    /// belong to the same branch in terms of VCS.
    fn get_color(
        &self,
        head_commit: &CommitId,
        head_fragment_index: i32,
        fragment_index: i32,
    ) -> i32;
}
