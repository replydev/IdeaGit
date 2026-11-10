//! The bottom controller of the BEK chains: shows the permanent graph in a sorted order.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/SortedBaseController.java`.

use std::sync::Arc;

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphNode, GraphNodeType};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::{PermanentGraphInfo, VcsLogGraphNodeId};
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};
use crate::facade::sort::sort_checker;
use crate::facade::sort::sort_index_map::SortIndexMap;
use crate::utils::linear_graph_utils::default_graph_answer;
use crate::visible_graph::VcsLogVisibleGraphIndex;

pub struct SortedBaseController {
    sort_index_map: Arc<SortIndexMap>,
    sorted_graph: Arc<SortedLinearGraph>,
}

impl SortedBaseController {
    pub fn new<CommitId>(
        permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
        sort_index_map: Arc<SortIndexMap>,
    ) -> SortedBaseController {
        let sorted_graph = Arc::new(SortedLinearGraph::new(
            sort_index_map.clone(),
            permanent_graph_info.linear_graph(),
        ));

        sort_checker::check_linear_graph(&*sorted_graph);
        SortedBaseController {
            sort_index_map,
            sorted_graph,
        }
    }

    pub fn bek_int_map(&self) -> &Arc<SortIndexMap> {
        &self.sort_index_map
    }
}

impl LinearGraphController for SortedBaseController {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.sorted_graph.clone()
    }

    fn perform_linear_graph_action(&mut self, _action: &LinearGraphAction) -> LinearGraphAnswer {
        default_graph_answer()
    }
}

/// The permanent graph with rows reordered by a [`SortIndexMap`]; node ids stay the permanent
/// graph's rows.
pub struct SortedLinearGraph {
    linear_graph: Arc<dyn LinearGraph>,
    sort_index_map: Arc<SortIndexMap>,
}

impl SortedLinearGraph {
    pub fn new(
        sort_index_map: Arc<SortIndexMap>,
        linear_graph: Arc<dyn LinearGraph>,
    ) -> SortedLinearGraph {
        SortedLinearGraph {
            linear_graph,
            sort_index_map,
        }
    }

    fn sorted_node_index(&self, node_id: Option<i32>) -> Option<i32> {
        node_id.map(|node_id| self.sort_index_map.get_sorted_index(node_id))
    }

    fn in_ranges(&self, index: i32) -> bool {
        index >= 0 && index < self.nodes_count()
    }
}

impl LinearGraph for SortedLinearGraph {
    fn nodes_count(&self) -> i32 {
        self.linear_graph.nodes_count()
    }

    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        self.linear_graph
            .get_adjacent_edges(self.sort_index_map.get_usual_index(node_index), filter)
            .into_iter()
            .map(|edge| {
                GraphEdge::new(
                    self.sorted_node_index(edge.up_node_index),
                    self.sorted_node_index(edge.down_node_index),
                    edge.target_id,
                    edge.edge_type,
                )
            })
            .collect()
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        debug_assert!(self.in_ranges(node_index));

        GraphNode::with_type(node_index, GraphNodeType::Usual)
    }

    /// See [`PermanentLinearGraphImpl::get_node_id`](crate::permanent::permanent_linear_graph_impl::PermanentLinearGraphImpl).
    fn get_node_id(&self, node_index: VcsLogVisibleGraphIndex) -> VcsLogGraphNodeId {
        self.sort_index_map.get_usual_index(node_index)
    }

    fn get_node_index(&self, node_id: VcsLogGraphNodeId) -> Option<VcsLogVisibleGraphIndex> {
        if !self.in_ranges(node_id) {
            return None;
        }

        Some(self.sort_index_map.get_sorted_index(node_id))
    }
}
