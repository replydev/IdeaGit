//! Shows the nodes reachable from the visible branches over a delegate that may change its graph
//! (linear BEK); no collapsing.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/BranchFilterController.kt`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::api::elements::GraphElement;
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::collapsing::collapsed_controller;
use crate::collapsing::collapsed_graph::CollapsedGraph;
use crate::facade::cascade_controller::{self, CascadeController};
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};
use crate::utils::graph_util::get_reachable_nodes;
use crate::utils::unsigned_bit_set::UnsignedBitSet;

pub struct BranchFilterController {
    delegate_controller: Box<dyn LinearGraphController>,
    collapsed_graph: CollapsedGraph,
    visibility: UnsignedBitSet,
}

impl BranchFilterController {
    pub fn new<CommitId>(
        delegate_linear_graph_controller: Box<dyn LinearGraphController>,
        permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
        ids_of_visible_branches: Option<&HashSet<i32>>,
    ) -> BranchFilterController {
        let visibility = get_reachable_nodes(
            &*permanent_graph_info.linear_graph(),
            ids_of_visible_branches,
        );
        let collapsed_graph = Self::update(&*delegate_linear_graph_controller, &visibility);
        BranchFilterController {
            delegate_controller: delegate_linear_graph_controller,
            collapsed_graph,
            visibility,
        }
    }

    fn update(
        delegate_controller: &dyn LinearGraphController,
        visibility: &UnsignedBitSet,
    ) -> CollapsedGraph {
        CollapsedGraph::new_instance(delegate_controller.compiled_graph(), visibility.clone())
    }
}

impl LinearGraphController for BranchFilterController {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.collapsed_graph.compiled_graph()
    }

    fn perform_linear_graph_action(&mut self, action: &LinearGraphAction) -> LinearGraphAnswer {
        cascade_controller::perform_linear_graph_action(self, action)
    }

    fn delegate(&self) -> Option<&dyn LinearGraphController> {
        Some(&*self.delegate_controller)
    }

    fn delegate_mut(&mut self) -> Option<&mut dyn LinearGraphController> {
        Some(&mut *self.delegate_controller)
    }

    fn delegate_graph_changed(&mut self, delegate_answer: LinearGraphAnswer) -> LinearGraphAnswer {
        if delegate_answer.graph_changes.is_some() {
            self.collapsed_graph = Self::update(&*self.delegate_controller, &self.visibility);
        }
        delegate_answer
    }
}

impl CascadeController for BranchFilterController {
    fn perform_action(&mut self, _action: &LinearGraphAction) -> Option<LinearGraphAnswer> {
        None
    }

    fn convert_to_delegate(&self, graph_element: &GraphElement) -> Option<GraphElement> {
        collapsed_controller::convert_to_delegate(graph_element, &self.collapsed_graph)
    }
}
