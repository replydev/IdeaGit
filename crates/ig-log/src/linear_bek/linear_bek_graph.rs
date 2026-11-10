//! The linear BEK graph: the BEK-sorted graph with some edges hidden and dotted edges added.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/linearBek/LinearBekGraph.java`.
//!
//! Deviations:
//! - `LinearBekGraph extends LinearGraphWrapper` becomes a struct embedding a
//!   [`LinearGraphWrapper`]; `WorkingLinearBekGraph` embeds a `LinearBekGraph`.
//! - Edge sets are `BTreeSet`s (deterministic order; upstream `HashSet`).

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphEdgeType, GraphNode};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::VcsLogGraphNodeId;
use crate::collapsing::edge_storage_wrapper::EdgeStorageWrapper;
use crate::facade::linear_graph_wrapper::LinearGraphWrapper;
use crate::visible_graph::VcsLogVisibleGraphIndex;

pub struct LinearBekGraph {
    wrapper: LinearGraphWrapper,
}

impl LinearBekGraph {
    pub fn new(graph: Arc<dyn LinearGraph>) -> LinearBekGraph {
        LinearBekGraph {
            wrapper: LinearGraphWrapper::new(graph),
        }
    }

    pub fn graph(&self) -> &Arc<dyn LinearGraph> {
        &self.wrapper.graph
    }

    pub fn hidden_edges(&self) -> &EdgeStorageWrapper {
        &self.wrapper.hidden_edges
    }

    pub fn dotted_edges(&self) -> &EdgeStorageWrapper {
        &self.wrapper.dotted_edges
    }

    /// Replaces the dotted `edge` by the edges it stands for (recursively); returns them.
    ///
    /// # Panics
    /// If `edge` is not a normal edge (upstream: an assertion).
    pub fn expand_edge(&self, edge: &GraphEdge) -> BTreeSet<GraphEdge> {
        let mut result = BTreeSet::new();

        debug_assert_eq!(edge.edge_type, GraphEdgeType::Dotted);
        self.dotted_edges().remove_edge(edge);

        let tail = edge
            .up_node_index
            .expect("Collapsed from to an unloaded node");
        let first_child = edge
            .down_node_index
            .expect("Collapsed edge to an unloaded node");

        let down_dotted_edges = self
            .hidden_edges()
            .get_adjacent_edges(tail, EdgeFilter::NormalDown);
        let up_dotted_edges = self
            .hidden_edges()
            .get_adjacent_edges(first_child, EdgeFilter::NormalUp);
        for e in down_dotted_edges.iter().chain(up_dotted_edges.iter()) {
            self.hidden_edges().remove_edge(e);
            if e.edge_type == GraphEdgeType::Dotted {
                result.extend(self.expand_edge(e));
            } else {
                result.insert(*e);
            }
        }

        result
    }
}

impl LinearGraph for LinearBekGraph {
    fn nodes_count(&self) -> i32 {
        self.wrapper.nodes_count()
    }

    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        self.wrapper.get_adjacent_edges(node_index, filter)
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        self.wrapper.get_graph_node(node_index)
    }

    fn get_node_id(&self, node_index: VcsLogVisibleGraphIndex) -> VcsLogGraphNodeId {
        self.wrapper.get_node_id(node_index)
    }

    fn get_node_index(&self, node_id: VcsLogGraphNodeId) -> Option<VcsLogVisibleGraphIndex> {
        self.wrapper.get_node_index(node_id)
    }
}

/// A scratch copy of a [`LinearBekGraph`] (same base graph, fresh edges) whose differences can be
/// reported and applied to the original.
pub struct WorkingLinearBekGraph {
    pub working: LinearBekGraph,
    linear_graph: Arc<LinearBekGraph>,
}

fn dotted(edges: &BTreeSet<GraphEdge>) -> impl Iterator<Item = &GraphEdge> {
    edges
        .iter()
        .filter(|edge| edge.edge_type == GraphEdgeType::Dotted)
}

impl WorkingLinearBekGraph {
    pub fn new(graph: Arc<LinearBekGraph>) -> WorkingLinearBekGraph {
        WorkingLinearBekGraph {
            working: LinearBekGraph::new(graph.graph().clone()),
            linear_graph: graph,
        }
    }

    pub fn get_added_edges(&self) -> BTreeSet<GraphEdge> {
        let mut result = self.working.dotted_edges().get_edges();
        let hidden = self.working.hidden_edges().get_edges();
        for edge in dotted(&hidden) {
            result.remove(edge);
        }
        for edge in &self.linear_graph.dotted_edges().get_edges() {
            result.remove(edge);
        }
        result
    }

    pub fn get_removed_edges(&self) -> BTreeSet<GraphEdge> {
        let mut result = BTreeSet::new();
        let hidden = self.working.hidden_edges().get_edges();
        result.extend(
            hidden
                .iter()
                .filter(|edge| edge.edge_type != GraphEdgeType::Dotted),
        );
        let original_dotted = self.linear_graph.dotted_edges().get_edges();
        result.extend(hidden.intersection(&original_dotted));
        for edge in &self.linear_graph.hidden_edges().get_edges() {
            result.remove(edge);
        }
        result
    }

    pub fn apply_changes(&self) {
        self.linear_graph.dotted_edges().remove_all();
        self.linear_graph.hidden_edges().remove_all();

        for e in self.working.dotted_edges().get_edges() {
            self.linear_graph.dotted_edges().create_edge(&e);
        }
        for e in self.working.hidden_edges().get_edges() {
            self.linear_graph.hidden_edges().create_edge(&e);
        }
    }
}
