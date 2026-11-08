//! Helpers over [`LinearGraph`] and [`GraphEdge`].
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/LinearGraphUtils.java`.

use std::collections::HashSet;
use std::ops::Deref;

use crate::actions::Cursor;
use crate::api::EdgeFilter;
use crate::api::elements::GraphEdge;
use crate::api::linear_graph::{LinearGraph, LiteLinearGraph, NodeFilter};
use crate::facade::linear_graph_controller::LinearGraphAnswer;
use crate::utils::normal_edge::NormalEdge;

/// `DEFAULT_GRAPH_ANSWER`: default cursor, no selection.
pub fn default_graph_answer() -> LinearGraphAnswer {
    LinearGraphAnswer::with_cursor(Some(Cursor::Default), None)
}

pub fn int_equal(value: Option<i32>, number: i32) -> bool {
    value == Some(number)
}

/// The edge goes up from `node_index` (the node is its down end).
pub fn is_edge_up(edge: &GraphEdge, node_index: i32) -> bool {
    int_equal(edge.down_node_index, node_index)
}

/// The edge goes down from `node_index` (the node is its up end).
pub fn is_edge_down(edge: &GraphEdge, node_index: i32) -> bool {
    int_equal(edge.up_node_index, node_index)
}

pub fn is_normal_edge(edge: Option<&GraphEdge>) -> bool {
    match edge {
        Some(edge) if edge.edge_type.is_normal_edge() => {
            debug_assert!(edge.up_node_index.is_some() && edge.down_node_index.is_some());
            true
        }
        _ => false,
    }
}

pub fn as_normal_edge(edge: &GraphEdge) -> Option<NormalEdge> {
    if is_normal_edge(Some(edge)) {
        Some(NormalEdge::create(
            edge.up_node_index?,
            edge.down_node_index?,
        ))
    } else {
        None
    }
}

/// # Panics
/// If the edge has neither node index.
pub fn get_not_null_node_index(edge: &GraphEdge) -> i32 {
    edge.up_node_index
        .or(edge.down_node_index)
        .expect("edge without node indexes")
}

pub fn get_up_nodes(graph: &(impl LinearGraph + ?Sized), node_index: i32) -> Vec<i32> {
    graph
        .get_adjacent_edges(node_index, EdgeFilter::NormalUp)
        .iter()
        .filter_map(|edge| edge.up_node_index)
        .collect()
}

pub fn get_down_nodes(graph: &(impl LinearGraph + ?Sized), node_index: i32) -> Vec<i32> {
    graph
        .get_adjacent_edges(node_index, EdgeFilter::NormalDown)
        .iter()
        .filter_map(|edge| edge.down_node_index)
        .collect()
}

/// [`LiteLinearGraph`] view of a [`LinearGraph`] (`asLiteLinearGraph`). `G` is any pointer to a
/// graph: `&G`, `Arc<G>`, `&dyn LinearGraph`, ...
#[derive(Clone, Debug)]
pub struct LiteLinearGraphAdapter<G>(pub G);

pub fn as_lite_linear_graph<G>(graph: G) -> LiteLinearGraphAdapter<G>
where
    G: Deref,
    G::Target: LinearGraph,
{
    LiteLinearGraphAdapter(graph)
}

impl<G> LiteLinearGraph for LiteLinearGraphAdapter<G>
where
    G: Deref,
    G::Target: LinearGraph,
{
    fn nodes_count(&self) -> i32 {
        self.0.nodes_count()
    }

    fn get_nodes(&self, node_index: i32, filter: NodeFilter) -> Vec<i32> {
        self.0
            .get_adjacent_edges(node_index, filter.edge_filter())
            .iter()
            .filter_map(|edge| {
                if is_edge_up(edge, node_index) {
                    return edge.up_node_index;
                }
                if is_edge_down(edge, node_index) {
                    return edge.down_node_index;
                }
                None
            })
            .collect()
    }
}

pub fn get_cursor(hand: bool) -> Cursor {
    if hand { Cursor::Hand } else { Cursor::Default }
}

/// Hand cursor and the node ids of `selected_node_indexes` as selection.
pub fn create_selected_answer(
    linear_graph: &(impl LinearGraph + ?Sized),
    selected_node_indexes: impl IntoIterator<Item = i32>,
) -> LinearGraphAnswer {
    let selected_ids: HashSet<i32> = selected_node_indexes
        .into_iter()
        .map(|node_index| linear_graph.get_node_id(node_index))
        .collect();
    LinearGraphAnswer::with_cursor(Some(get_cursor(true)), Some(selected_ids))
}

pub fn get_edge(graph: &(impl LinearGraph + ?Sized), up: i32, down: i32) -> Option<GraphEdge> {
    graph
        .get_adjacent_edges(up, EdgeFilter::NormalDown)
        .into_iter()
        .find(|edge| int_equal(edge.down_node_index, down))
}

pub fn convert_node_indexes_to_ids(
    graph: &(impl LinearGraph + ?Sized),
    node_indexes: impl IntoIterator<Item = i32>,
) -> HashSet<i32> {
    node_indexes
        .into_iter()
        .map(|node_index| graph.get_node_id(node_index))
        .collect()
}

pub fn convert_ids_to_node_indexes(
    graph: &(impl LinearGraph + ?Sized),
    ids: impl IntoIterator<Item = i32>,
) -> HashSet<i32> {
    ids.into_iter()
        .filter_map(|id| graph.get_node_index(id))
        .collect()
}
