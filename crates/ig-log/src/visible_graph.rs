//! The part of the permanent graph that is drawn on screen, and its rows.
//!
//! Ported from `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/VisibleGraph.kt`,
//! `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/RowInfo.java` and
//! `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/RowType.java`.

use crate::actions::ActionController;
use crate::api::printer::GraphPrintElement;

/// Row index used in [`VisibleGraph`]; see [`VisibleGraph::get_visible_row_index`].
pub type VcsLogVisibleGraphIndex = i32;

/// Visible commit graph row type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowType {
    Normal,
    /// The commit on this row doesn't match current filters.
    Unmatched,
}

/// Visible commit graph row data.
///
/// # Panics
/// Obtaining it with [`VisibleGraph::get_row_info`] panics unless `visible_row` is in
/// `0..visible_commit_count()`.
pub trait RowInfo<Id> {
    fn commit(&self) -> Id;

    fn one_of_heads(&self) -> Id;

    /// Edges first, nodes last (nodes are drawn over the edges).
    ///
    /// Snapshot: re-fetch after an answer with `is_repaint_required` or after `run_graph_updater`.
    fn print_elements(&self) -> Vec<GraphPrintElement>;

    fn row_type(&self) -> RowType;

    /// Rows of parents (`parent == true`) or children.
    fn adjacent_rows(&self, parent: bool) -> Vec<VcsLogVisibleGraphIndex>;
}

/// A part of the permanent graph which is drawn on screen (e.g. with applied filters). One per
/// client; all access must be synchronized. It refers to the permanent graph and occupies little
/// memory on its own.
pub trait VisibleGraph<Id> {
    fn visible_commit_count(&self) -> i32;

    fn recommended_width(&self) -> i32;

    fn action_controller(&mut self) -> &mut dyn ActionController<Id>;

    /// # Panics
    /// If `visible_row` is not in `0..visible_commit_count()`.
    fn get_row_info(&self, visible_row: VcsLogVisibleGraphIndex) -> Box<dyn RowInfo<Id> + '_>;

    fn get_visible_row_index(&self, id: &Id) -> Option<VcsLogVisibleGraphIndex>;

    /// Runs the graph updater of an action answer and refreshes the printed rows (upstream wraps
    /// this into the answer's `Runnable`).
    fn run_graph_updater(&mut self, updater: crate::actions::GraphUpdater);
}
