//! The compact linear graph of all loaded commits.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentLinearGraphImpl.java`.
//!
//! A "simple" node has exactly one parent, the next row; its down edge is implicit. All other
//! edges are stored in `long_edges`, indexed per node by `node_to_edge_index`: the up edges first,
//! then the down edges in parent order. Negative entries are ids of commits that are not loaded.

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphEdgeType, GraphNode};
use crate::api::linear_graph::LinearGraph;
use crate::utils::flags::{BitSetFlags, Flags};

#[derive(Clone, Debug)]
pub struct PermanentLinearGraphImpl {
    simple_nodes: BitSetFlags,
    node_to_edge_index: Vec<i32>,
    long_edges: Vec<i32>,
}

impl PermanentLinearGraphImpl {
    pub(crate) fn new(
        simple_nodes: BitSetFlags,
        node_to_edge_index: Vec<i32>,
        long_edges: Vec<i32>,
    ) -> PermanentLinearGraphImpl {
        PermanentLinearGraphImpl {
            simple_nodes,
            node_to_edge_index,
            long_edges,
        }
    }

    fn edge_range(&self, node_index: i32) -> std::ops::Range<usize> {
        self.node_to_edge_index[node_index as usize] as usize
            ..self.node_to_edge_index[node_index as usize + 1] as usize
    }

    /// Calls `handler` for every node that can be considered a parent of `node_index`, including
    /// the (negative) ids of [`GraphEdgeType::NotLoadCommit`] targets, in parent order.
    pub fn for_each_parent_node(&self, node_index: i32, mut handler: impl FnMut(i32)) {
        for i in self.edge_range(node_index) {
            let adjacent_node = self.long_edges[i];
            if adjacent_node < 0 || node_index < adjacent_node {
                handler(adjacent_node);
            }
        }
        if self.simple_nodes.get(node_index) {
            handler(node_index + 1);
        }
    }
}

impl LinearGraph for PermanentLinearGraphImpl {
    fn nodes_count(&self) -> i32 {
        self.simple_nodes.size()
    }

    /// # Panics
    /// If `node_index` is not a node of the graph.
    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        let mut result = Vec::new();

        let has_up_simple_edge = node_index != 0 && self.simple_nodes.get(node_index - 1);
        if has_up_simple_edge && filter.up_normal() {
            result.push(GraphEdge::new(
                Some(node_index - 1),
                Some(node_index),
                None,
                GraphEdgeType::Usual,
            ));
        }

        for i in self.edge_range(node_index) {
            let adjacent_node = self.long_edges[i];

            if adjacent_node < 0 && filter.special() {
                result.push(GraphEdge::create_edge_with_target_id(
                    node_index,
                    Some(adjacent_node),
                    GraphEdgeType::NotLoadCommit,
                ));
            }
            if adjacent_node < 0 {
                continue;
            }

            if node_index > adjacent_node && filter.up_normal() {
                result.push(GraphEdge::new(
                    Some(adjacent_node),
                    Some(node_index),
                    None,
                    GraphEdgeType::Usual,
                ));
            }
            if node_index < adjacent_node && filter.down_normal() {
                result.push(GraphEdge::new(
                    Some(node_index),
                    Some(adjacent_node),
                    None,
                    GraphEdgeType::Usual,
                ));
            }
        }

        if self.simple_nodes.get(node_index) && filter.down_normal() {
            result.push(GraphEdge::new(
                Some(node_index),
                Some(node_index + 1),
                None,
                GraphEdgeType::Usual,
            ));
        }

        result
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        GraphNode::new(node_index)
    }

    fn get_node_id(&self, node_index: i32) -> i32 {
        debug_assert!(
            node_index >= 0 && node_index < self.nodes_count(),
            "Bad nodeIndex: {node_index}"
        );
        node_index
    }

    fn get_node_index(&self, node_id: i32) -> Option<i32> {
        if node_id >= 0 && node_id < self.nodes_count() {
            Some(node_id)
        } else {
            None
        }
    }
}
