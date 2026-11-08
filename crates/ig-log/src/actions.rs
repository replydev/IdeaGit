//! Mouse and button actions on the visible graph, and the graph's answers to them.
//!
//! Ported from `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/actions/`
//! (`ActionController.java`, `GraphAction.kt`, `GraphAnswer.java`).
//!
//! Deviations: the AWT `Cursor` becomes [`Cursor`]; the affected element is always a
//! [`GraphPrintElement`] (the UI passes back what `RowInfo::print_elements` gave it); the
//! `Runnable` graph updater becomes a [`GraphUpdater`] that the caller hands back to
//! [`VisibleGraph::run_graph_updater`](crate::visible_graph::VisibleGraph::run_graph_updater),
//! the only public way to run it (upstream's `Runnable` also rebuilds the print elements).

use std::fmt;

use crate::api::printer::GraphPrintElement;

/// Mouse cursor the UI should show (`Cursor.getDefaultCursor()` / `Cursor.HAND_CURSOR`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cursor {
    Default,
    Hand,
}

/// `GraphAction.Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GraphActionType {
    MouseClick,
    MouseOver,
    ButtonCollapse,
    ButtonExpand,
}

/// An action on the graph (`GraphAction.GraphActionImpl`).
#[derive(Clone, Debug, PartialEq)]
pub struct GraphAction {
    pub affected_element: Option<GraphPrintElement>,
    pub action_type: GraphActionType,
}

impl GraphAction {
    pub fn new(
        affected_element: Option<GraphPrintElement>,
        action_type: GraphActionType,
    ) -> GraphAction {
        GraphAction {
            affected_element,
            action_type,
        }
    }
}

/// Deferred graph modification returned by an action (upstream `Runnable`).
pub struct GraphUpdater(Box<dyn FnOnce() + Send>);

impl GraphUpdater {
    pub fn new(update: impl FnOnce() + Send + 'static) -> GraphUpdater {
        GraphUpdater(Box::new(update))
    }

    pub(crate) fn run(self) {
        (self.0)()
    }
}

impl fmt::Debug for GraphUpdater {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GraphUpdater")
    }
}

/// The graph's reaction to [`ActionController::perform_action`].
#[derive(Debug)]
pub struct GraphAnswer<Id> {
    pub cursor_to_set: Option<Cursor>,
    pub commit_to_jump: Option<Id>,
    pub graph_updater: Option<GraphUpdater>,
    pub do_jump: bool,
    pub is_repaint_required: bool,
}

/// Handles user actions on a visible graph.
pub trait ActionController<Id> {
    fn perform_action(&mut self, graph_action: GraphAction) -> GraphAnswer<Id>;

    fn are_long_edges_hidden(&self) -> bool;

    fn set_long_edges_hidden(&mut self, long_edges_hidden: bool);

    fn is_action_supported(&self, _action: &GraphAction) -> bool {
        true
    }
}
