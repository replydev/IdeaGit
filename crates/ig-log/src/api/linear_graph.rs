//! The linear graph: commits as rows (node indexes) connected by edges.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/LinearGraph.kt` and
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/LiteLinearGraph.java`.

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphNode};
use crate::api::permanent::VcsLogGraphNodeId;
use crate::visible_graph::VcsLogVisibleGraphIndex;

/// A graph whose nodes are rows `0..nodes_count()`. Each node also has a stable node id
/// (the node index in the permanent graph).
pub trait LinearGraph: Send + Sync {
    fn nodes_count(&self) -> i32;

    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge>;

    fn get_graph_node(&self, node_index: i32) -> GraphNode;

    fn get_node_id(&self, node_index: VcsLogVisibleGraphIndex) -> VcsLogGraphNodeId;

    /// `None` if the node doesn't exist in this graph.
    fn get_node_index(&self, node_id: VcsLogGraphNodeId) -> Option<VcsLogVisibleGraphIndex>;
}

/// Which neighbours [`LiteLinearGraph::get_nodes`] returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeFilter {
    Up,
    Down,
    All,
}

impl NodeFilter {
    pub fn up(self) -> bool {
        matches!(self, NodeFilter::Up | NodeFilter::All)
    }

    pub fn down(self) -> bool {
        matches!(self, NodeFilter::Down | NodeFilter::All)
    }

    pub fn edge_filter(self) -> EdgeFilter {
        match self {
            NodeFilter::Up => EdgeFilter::NormalUp,
            NodeFilter::Down => EdgeFilter::NormalDown,
            NodeFilter::All => EdgeFilter::NormalAll,
        }
    }

    pub fn filter(is_up: bool) -> NodeFilter {
        if is_up {
            NodeFilter::Up
        } else {
            NodeFilter::Down
        }
    }
}

/// A graph view that only knows adjacent node indexes (normal edges only).
pub trait LiteLinearGraph {
    fn nodes_count(&self) -> i32;

    fn get_nodes(&self, node_index: i32, filter: NodeFilter) -> Vec<i32>;
}
