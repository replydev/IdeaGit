//! A controller owns the graph shown by a visible graph and reacts to actions on it.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphController.kt`.
//!
//! Deviation: `compiled_graph` returns a shared handle; controllers that modify their graph (Plan 2b)
//! hand out a new handle after each change, and the visible graph re-reads it.

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
}
