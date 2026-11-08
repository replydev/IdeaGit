//! A linear graph with extra (dotted) edges added and some edges hidden.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphWrapper.kt`.
//! Used by the filter and first-parent controllers (Plan 2b); not on the unfiltered `Normal` path.

use std::sync::Arc;

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphNode};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::VcsLogGraphNodeId;
use crate::collapsing::edge_storage_wrapper::EdgeStorageWrapper;
use crate::visible_graph::VcsLogVisibleGraphIndex;

/// Upstream's `LinearBekGraph`/`WorkingLinearBekGraph` subclass this; in Rust embed it and delegate
/// `LinearGraph`.
pub struct LinearGraphWrapper {
    pub graph: Arc<dyn LinearGraph>,
    pub hidden_edges: EdgeStorageWrapper,
    pub dotted_edges: EdgeStorageWrapper,
}

impl LinearGraphWrapper {
    /// With fresh, empty hidden and dotted edge storages.
    pub fn new(graph: Arc<dyn LinearGraph>) -> LinearGraphWrapper {
        LinearGraphWrapper::with_edges(
            graph,
            EdgeStorageWrapper::create_simple_edge_storage(),
            EdgeStorageWrapper::create_simple_edge_storage(),
        )
    }

    pub fn with_edges(
        graph: Arc<dyn LinearGraph>,
        hidden_edges: EdgeStorageWrapper,
        dotted_edges: EdgeStorageWrapper,
    ) -> LinearGraphWrapper {
        LinearGraphWrapper {
            graph,
            hidden_edges,
            dotted_edges,
        }
    }
}

impl LinearGraph for LinearGraphWrapper {
    /// Dotted edges first, then the graph's own edges, minus every edge equal to a hidden one.
    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        let mut result = self.dotted_edges.get_adjacent_edges(node_index, filter);
        result.extend(self.graph.get_adjacent_edges(node_index, filter));
        let hidden = self.hidden_edges.get_adjacent_edges(node_index, filter);
        result.retain(|edge| !hidden.contains(edge));
        result
    }

    fn nodes_count(&self) -> i32 {
        self.graph.nodes_count()
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        self.graph.get_graph_node(node_index)
    }

    fn get_node_id(&self, node_index: VcsLogVisibleGraphIndex) -> VcsLogGraphNodeId {
        self.graph.get_node_id(node_index)
    }

    fn get_node_index(&self, node_id: VcsLogGraphNodeId) -> Option<VcsLogVisibleGraphIndex> {
        self.graph.get_node_index(node_id)
    }
}
