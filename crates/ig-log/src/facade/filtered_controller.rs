//! Shows the commits that match a filter, connected by dotted edges; no actions are passed on.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/FilteredController.kt`.
//!
//! Deviations:
//! - The constructor takes the built [`CollapsedGraph`] (upstream: a builder lambda it calls
//!   right away).
//! - `delegateGraphChanged` compares the answer by identity with `DEFAULT_GRAPH_ANSWER` upstream;
//!   here an answer without graph changes or updater counts as the default answer. Upstream throws
//!   for any other answer; that cannot happen, since a filtered controller never passes actions on
//!   and is never below another controller.

use std::any::Any;
use std::collections::HashSet;
use std::sync::Arc;

use crate::api::elements::GraphElement;
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::collapsing::collapsed_graph::CollapsedGraph;
use crate::collapsing::dotted_filter_edges_generator;
use crate::facade::cascade_controller::CascadeController;
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};
use crate::utils::graph_util::get_reachable_matching_nodes;
use crate::utils::linear_graph_utils::default_graph_answer;

pub struct FilteredController {
    delegate_controller: Box<dyn LinearGraphController>,
    pub collapsed_graph: CollapsedGraph,
}

impl FilteredController {
    pub fn new(
        delegate_linear_graph_controller: Box<dyn LinearGraphController>,
        collapsed_graph: CollapsedGraph,
    ) -> FilteredController {
        FilteredController {
            delegate_controller: delegate_linear_graph_controller,
            collapsed_graph,
        }
    }

    /// `FilteredController.create`: shows the `matched_ids` reachable from `visible_heads_ids`
    /// (all matched ids if `None`).
    pub fn create<CommitId>(
        delegate_controller: Box<dyn LinearGraphController>,
        permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
        matched_ids: &HashSet<i32>,
        visible_heads_ids: Option<&HashSet<i32>>,
    ) -> FilteredController {
        let visibility = get_reachable_matching_nodes(
            &*permanent_graph_info.linear_graph(),
            visible_heads_ids,
            Some(matched_ids),
        );
        let collapsed_graph =
            CollapsedGraph::new_instance(delegate_controller.compiled_graph(), visibility);
        dotted_filter_edges_generator::update(
            &collapsed_graph,
            0,
            collapsed_graph.delegated_graph().nodes_count() - 1,
        );
        FilteredController::new(delegate_controller, collapsed_graph)
    }
}

impl LinearGraphController for FilteredController {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.collapsed_graph.compiled_graph()
    }

    /// Filter prohibits any actions on delegate graph for now.
    fn perform_linear_graph_action(&mut self, action: &LinearGraphAction) -> LinearGraphAnswer {
        self.perform_action(action)
            .unwrap_or_else(default_graph_answer)
    }

    fn delegate(&self) -> Option<&dyn LinearGraphController> {
        Some(&*self.delegate_controller)
    }

    fn delegate_mut(&mut self) -> Option<&mut dyn LinearGraphController> {
        Some(&mut *self.delegate_controller)
    }

    /// # Panics
    /// For an answer with graph changes or an updater (upstream `UnsupportedOperationException`,
    /// "todo fix later"); unreachable, see the module docs.
    fn delegate_graph_changed(&mut self, delegate_answer: LinearGraphAnswer) -> LinearGraphAnswer {
        if delegate_answer.graph_changes.is_none() && delegate_answer.graph_updater.is_none() {
            return delegate_answer;
        }
        panic!("FilteredController does not support changes of its delegate graph");
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }

    fn supports_collapse(&self) -> bool {
        false
    }
}

impl CascadeController for FilteredController {
    fn perform_action(&mut self, _action: &LinearGraphAction) -> Option<LinearGraphAnswer> {
        None
    }

    /// Filter prohibits any actions on delegate graph for now.
    fn convert_to_delegate(&self, _graph_element: &GraphElement) -> Option<GraphElement> {
        None
    }
}
