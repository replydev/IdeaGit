//! A controller owns the graph shown by a visible graph and reacts to actions on it.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphController.kt`.
//!
//! Deviations:
//! - `compiled_graph` returns a shared handle. Controllers that change their graph change it in
//!   place or hand out a new handle, and the visible graph re-reads it.
//! - Upstream's downcasts are default methods: [`LinearGraphController::delegate`] /
//!   [`delegate_mut`](LinearGraphController::delegate_mut) /
//!   [`delegate_graph_changed`](LinearGraphController::delegate_graph_changed) for `is
//!   CascadeController` (`performActionRecursively`), [`as_any_mut`](LinearGraphController::as_any_mut)
//!   for `is FilteredController` (`GraphModificationUtil`) and
//!   [`supports_collapse`](LinearGraphController::supports_collapse) for `VisibleGraphImpl`'s
//!   `isActionSupported`.

use std::any::Any;
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use crate::actions::{Cursor, GraphAction, GraphUpdater};
use crate::api::linear_graph::LinearGraph;
use crate::facade::graph_changes::GraphChanges;

/// A [`GraphAction`] whose affected element belongs to this controller's graph
/// (`LinearGraphAction` / `VisibleGraphImpl.LinearGraphActionImpl`).
pub type LinearGraphAction = GraphAction;

/// Answer of a controller; ids are node ids.
pub struct LinearGraphAnswer {
    pub graph_changes: Option<GraphChanges<i32>>,
    pub cursor_to_set: Option<Cursor>,
    pub selected_node_ids: Option<HashSet<i32>>,
    pub graph_updater: Option<GraphUpdater>,
}

impl LinearGraphAnswer {
    pub fn new(
        graph_changes: Option<GraphChanges<i32>>,
        cursor_to_set: Option<Cursor>,
        selected_node_ids: Option<HashSet<i32>>,
        graph_updater: Option<GraphUpdater>,
    ) -> LinearGraphAnswer {
        LinearGraphAnswer {
            graph_changes,
            cursor_to_set,
            selected_node_ids,
            graph_updater,
        }
    }

    /// `LinearGraphAnswer(cursor, selectedNodeIds)`.
    pub fn with_cursor(cursor: Option<Cursor>, selected_node_ids: Option<HashSet<i32>>) -> Self {
        LinearGraphAnswer::new(None, cursor, selected_node_ids, None)
    }

    /// `LinearGraphAnswer(graphChanges, graphUpdater = null)`.
    pub fn with_changes(
        graph_changes: Option<GraphChanges<i32>>,
        graph_updater: Option<GraphUpdater>,
    ) -> Self {
        LinearGraphAnswer::new(graph_changes, None, None, graph_updater)
    }
}

impl fmt::Debug for LinearGraphAnswer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinearGraphAnswer")
            .field("graph_changes", &self.graph_changes)
            .field("cursor_to_set", &self.cursor_to_set)
            .field("selected_node_ids", &self.selected_node_ids)
            .field("graph_updater", &self.graph_updater.is_some())
            .finish()
    }
}

pub trait LinearGraphController: Send {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph>;

    fn perform_linear_graph_action(&mut self, action: &LinearGraphAction) -> LinearGraphAnswer;

    /// The delegate of a `CascadeController`; `None` for other controllers.
    fn delegate(&self) -> Option<&dyn LinearGraphController> {
        None
    }

    /// See [`delegate`](Self::delegate).
    fn delegate_mut(&mut self) -> Option<&mut dyn LinearGraphController> {
        None
    }

    /// `CascadeController.delegateGraphChanged`: called after the delegate answered with
    /// `delegate_answer`. Only called on controllers with a [`delegate`](Self::delegate).
    fn delegate_graph_changed(&mut self, delegate_answer: LinearGraphAnswer) -> LinearGraphAnswer {
        delegate_answer
    }

    /// For downcasts to a concrete controller; `None` unless the controller opts in.
    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        None
    }

    /// Whether `BUTTON_COLLAPSE` / `BUTTON_EXPAND` are supported (upstream: `!is
    /// FilteredController`).
    fn supports_collapse(&self) -> bool {
        true
    }
}
