//! Shows only the first parent of every merge, walking down from the heads.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/FirstParentController.kt`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::api::EdgeFilter;
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::collapsing::collapsed_graph::CollapsedGraph;
use crate::collapsing::dotted_filter_edges_generator;
use crate::collapsing::edge_storage_wrapper::EdgeStorageWrapper;
use crate::facade::filtered_controller::FilteredController;
use crate::facade::linear_graph_controller::LinearGraphController;
use crate::facade::linear_graph_wrapper::LinearGraphWrapper;
use crate::permanent::graph_layout_builder::get_heads;
use crate::utils::dfs_util::NODE_NOT_FOUND;
use crate::utils::flags::{BitSetFlags, Flags};
use crate::utils::unsigned_bit_set::UnsignedBitSet;

/// `FirstParentController.create`: walks from `visible_heads_ids`, or from the branch heads and
/// the graph's heads if `None`; shows the `matched_ids` among the walked nodes (all if `None`).
pub fn create<CommitId>(
    delegate_controller: Box<dyn LinearGraphController>,
    permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
    matched_ids: Option<&HashSet<i32>>,
    visible_heads_ids: Option<&HashSet<i32>>,
) -> FilteredController {
    let linear_graph = permanent_graph_info.linear_graph();
    let start_nodes = match visible_heads_ids {
        Some(visible_heads_ids) => visible_heads_ids.clone(),
        None => permanent_graph_info
            .branch_node_ids()
            .iter()
            .copied()
            .chain(get_heads(&*linear_graph))
            .collect(),
    };
    let collapsed_graph = build_collapsed_graph(linear_graph, &start_nodes, matched_ids);
    FilteredController::new(delegate_controller, collapsed_graph)
}

/// The first-parent graph (visible for testing upstream).
pub fn build_collapsed_graph(
    linear_graph: Arc<dyn LinearGraph>,
    start_nodes: &HashSet<i32>,
    matched_nodes: Option<&HashSet<i32>>,
) -> CollapsedGraph {
    let (visible_nodes, hidden_edges) =
        get_visible_nodes_and_hidden_edges(&*linear_graph, start_nodes, matched_nodes);

    let collapsed_graph = CollapsedGraph::new_instance(
        Arc::new(LinearGraphWrapper::with_edges(
            linear_graph,
            hidden_edges,
            EdgeStorageWrapper::create_simple_edge_storage(),
        )),
        visible_nodes,
    );
    if matched_nodes.is_some() {
        dotted_filter_edges_generator::update(
            &collapsed_graph,
            0,
            collapsed_graph.delegated_graph().nodes_count() - 1,
        );
    }
    collapsed_graph
}

fn get_visible_nodes_and_hidden_edges(
    graph: &dyn LinearGraph,
    start_nodes: &HashSet<i32>,
    matched_nodes: Option<&HashSet<i32>>,
) -> (UnsignedBitSet, EdgeStorageWrapper) {
    let mut visible_nodes = UnsignedBitSet::new();
    let hidden_edges = EdgeStorageWrapper::create_simple_edge_storage();

    let mut visited = BitSetFlags::new(graph.nodes_count());
    for &start in start_nodes {
        if start < 0 {
            continue;
        }

        let mut node = start;
        while node != NODE_NOT_FOUND {
            if visited.get(node) {
                break;
            }
            visited.set(node, true);

            if matched_nodes.is_none_or(|matched| matched.contains(&node)) {
                visible_nodes.set(node, true);
            }

            let down_edges = graph.get_adjacent_edges(node, EdgeFilter::NormalDown);
            down_edges
                .iter()
                .skip(1)
                .for_each(|it| hidden_edges.create_edge(it));
            node = down_edges
                .first()
                .and_then(|edge| edge.down_node_index)
                .unwrap_or(NODE_NOT_FOUND);
        }
    }
    (visible_nodes, hidden_edges)
}
