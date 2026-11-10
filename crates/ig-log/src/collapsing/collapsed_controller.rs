//! Shows the graph of visible branches and handles collapsing, expanding and fragment hover.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/CollapsedController.java`.
//!
//! Upstream quirk kept: `convert_to_delegate` checks whether an edge is "my collapsed edge" with
//! the edge's compiled node indexes, while the edge storage is keyed by delegate node indexes.
//! Delegate actions on edges do nothing in every chain that has this controller, so it does not
//! matter. `BranchFilterController::convert_to_delegate` reuses the same function over linear BEK,
//! which does act on edges; there the quirk is harmless because a `BranchFilterController` never
//! adds edges, so its edge storage is empty and `is_my_collapsed_edge` is always false.

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use crate::api::elements::{GraphEdge, GraphElement, GraphNode};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::collapsing::collapsed_action_manager;
use crate::collapsing::collapsed_graph::CollapsedGraph;
use crate::facade::cascade_controller::{self, CascadeController};
use crate::facade::graph_changes::GraphChanges;
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};
use crate::utils::graph_util::get_reachable_nodes;

pub struct CollapsedController<CommitId> {
    delegate_controller: Box<dyn LinearGraphController>,
    permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    collapsed_graph: CollapsedGraph,
}

impl<CommitId: 'static> CollapsedController<CommitId> {
    /// Shows the nodes reachable from `ids_of_visible_branches` (all nodes if `None`).
    pub fn new(
        delegate_linear_graph_controller: Box<dyn LinearGraphController>,
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
        ids_of_visible_branches: Option<&HashSet<i32>>,
    ) -> CollapsedController<CommitId> {
        let init_visibility = get_reachable_nodes(
            &*permanent_graph_info.linear_graph(),
            ids_of_visible_branches,
        );
        let collapsed_graph = CollapsedGraph::new_instance(
            delegate_linear_graph_controller.compiled_graph(),
            init_visibility,
        );
        CollapsedController {
            delegate_controller: delegate_linear_graph_controller,
            permanent_graph_info,
            collapsed_graph,
        }
    }

    pub fn collapsed_graph(&self) -> &CollapsedGraph {
        &self.collapsed_graph
    }

    pub fn permanent_graph_info(&self) -> &dyn PermanentGraphInfo<CommitId> {
        &*self.permanent_graph_info
    }

    fn apply_delegate_changes(&self, graph: &dyn LinearGraph, changes: &GraphChanges<i32>) {
        // ordered for determinism (upstream: a HashSet)
        let mut nodes_to_show = BTreeSet::new();

        for e in &changes.changed_edges {
            if !e.removed {
                if let Some(up_index) = e.up_node_id.and_then(|up_id| graph.get_node_index(up_id)) {
                    nodes_to_show.insert(up_index);
                }
                if let Some(down_index) = e
                    .down_node_id
                    .and_then(|down_id| graph.get_node_index(down_id))
                {
                    nodes_to_show.insert(down_index);
                }
            }
        }

        for e in &changes.changed_nodes {
            if !e.removed
                && let Some(node_index) = graph.get_node_index(e.node_id)
            {
                nodes_to_show.insert(node_index);
            }
        }

        collapsed_action_manager::expand_nodes(&self.collapsed_graph, &nodes_to_show);
    }
}

/// `CollapsedController.convertToDelegate(graphElement, collapsedGraph)`.
pub fn convert_to_delegate(
    graph_element: &GraphElement,
    collapsed_graph: &CollapsedGraph,
) -> Option<GraphElement> {
    match graph_element {
        GraphElement::Edge(edge) => {
            let up_index = edge.up_node_index;
            let down_index = edge.down_node_index;
            if let (Some(up), Some(down)) = (up_index, down_index)
                && collapsed_graph.is_my_collapsed_edge(up, down)
            {
                return None;
            }

            let converted_up_index =
                up_index.map(|up| collapsed_graph.convert_to_delegate_node_index(up));
            let converted_down_index =
                down_index.map(|down| collapsed_graph.convert_to_delegate_node_index(down));

            Some(GraphElement::Edge(GraphEdge::new(
                converted_up_index,
                converted_down_index,
                edge.target_id,
                edge.edge_type,
            )))
        }
        GraphElement::Node(node) => Some(GraphElement::Node(GraphNode::with_type(
            collapsed_graph.convert_to_delegate_node_index(node.node_index),
            node.node_type,
        ))),
    }
}

impl<CommitId: 'static> LinearGraphController for CollapsedController<CommitId> {
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

    /// Upstream: "if somebody outside actually uses changes we return here they are screwed".
    fn delegate_graph_changed(&mut self, delegate_answer: LinearGraphAnswer) -> LinearGraphAnswer {
        if let Some(graph_changes) = &delegate_answer.graph_changes {
            let delegate_graph = self.delegate_controller.compiled_graph();
            self.collapsed_graph =
                CollapsedGraph::update_instance(&self.collapsed_graph, delegate_graph.clone());

            // some new edges and node appeared, so we expand them
            self.apply_delegate_changes(&*delegate_graph, graph_changes);
        }
        delegate_answer
    }
}

impl<CommitId: 'static> CascadeController for CollapsedController<CommitId> {
    fn perform_action(&mut self, action: &LinearGraphAction) -> Option<LinearGraphAnswer> {
        collapsed_action_manager::perform_action(
            &self.collapsed_graph,
            &*self.permanent_graph_info,
            action,
        )
    }

    fn convert_to_delegate(&self, graph_element: &GraphElement) -> Option<GraphElement> {
        convert_to_delegate(graph_element, &self.collapsed_graph)
    }
}
