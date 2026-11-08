//! Changes of a linear graph reported by controllers, by node id.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/GraphChanges.java`
//! and `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/GraphChangesUtil.java`.
//!
//! Deviation: the interfaces and their `*Impl` classes collapse into plain structs. Upstream's
//! `edgesReplaced` collects into a `HashSet` of `EdgeImpl`s, which has identity equality, so it is
//! effectively a list; here it is a `Vec`.

use crate::api::elements::GraphEdge;
use crate::api::linear_graph::LinearGraph;

/// A node that appeared or was removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node<NodeId> {
    pub node_id: NodeId,
    pub removed: bool,
}

/// An edge that appeared or was removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge<NodeId> {
    pub up_node_id: Option<NodeId>,
    pub down_node_id: Option<NodeId>,
    pub target_id: Option<NodeId>,
    pub removed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphChanges<NodeId> {
    pub changed_nodes: Vec<Node<NodeId>>,
    pub changed_edges: Vec<Edge<NodeId>>,
}

impl<NodeId> GraphChanges<NodeId> {
    pub fn new(changed_nodes: Vec<Node<NodeId>>, changed_edges: Vec<Edge<NodeId>>) -> Self {
        GraphChanges {
            changed_nodes,
            changed_edges,
        }
    }

    /// `GraphChangesUtil.SOME_CHANGES`: "something changed", without details.
    pub fn some_changes() -> Self {
        GraphChanges::new(Vec::new(), Vec::new())
    }
}

/// `GraphChangesUtil.edgeChanged`: the edge with node indexes of `graph` converted to node ids.
pub fn edge_changed(edge: &GraphEdge, graph: &dyn LinearGraph, removed: bool) -> Edge<i32> {
    Edge {
        up_node_id: edge.up_node_index.map(|index| graph.get_node_id(index)),
        down_node_id: edge.down_node_index.map(|index| graph.get_node_id(index)),
        target_id: edge.target_id,
        removed,
    }
}

/// `GraphChangesUtil.edgesReplaced`.
pub fn edges_replaced(
    removed_edges: &[GraphEdge],
    added_edges: &[GraphEdge],
    delegate_graph: &dyn LinearGraph,
) -> GraphChanges<i32> {
    let edge_changes = removed_edges
        .iter()
        .map(|edge| edge_changed(edge, delegate_graph, true))
        .chain(
            added_edges
                .iter()
                .map(|edge| edge_changed(edge, delegate_graph, false)),
        )
        .collect();
    GraphChanges::new(Vec::new(), edge_changes)
}
