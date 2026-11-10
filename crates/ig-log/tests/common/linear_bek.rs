//! Helpers of the BEK and linear BEK tests.
//!
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/TestUtils.kt`.

use std::sync::Arc;

use ig_log::api::linear_graph::LinearGraph;
use ig_log::facade::sort::bek::bek_sorter;
use ig_log::facade::sort::sort_checker;
use ig_log::facade::sort::sorted_base_controller::SortedLinearGraph;
use ig_log::linear_bek::linear_bek_graph::LinearBekGraph;
use ig_log::linear_bek::linear_bek_graph_builder::LinearBekGraphBuilder;
use ig_log::permanent::graph_layout_builder;
use ig_log::permanent::graph_layout_impl::GraphLayoutImpl;
use ig_log::utils::TimestampGetter;

use super::test_graph_builder::{TestLinearGraph, as_test_graph_string};

pub struct DummyTimestampGetter {
    pub nodes_count: i32,
}

impl TimestampGetter for DummyTimestampGetter {
    fn get_timestamp(&self, _index: i32) -> i64 {
        0
    }
}

pub fn build_layout(graph: &TestLinearGraph) -> GraphLayoutImpl {
    graph_layout_builder::build(graph, |node_index1, node_index2| {
        node_index1.cmp(&node_index2)
    })
}

/// `runBek`: takes the graph built by the test's builder (Kotlin builds it twice).
pub fn run_bek(before_bek: TestLinearGraph) -> SortedLinearGraph {
    let before_layout = build_layout(&before_bek);

    let bek_map = bek_sorter::create_bek_map(
        &before_bek,
        &before_layout,
        &DummyTimestampGetter {
            nodes_count: before_bek.nodes_count(),
        },
    );
    let bek_map = Arc::new(bek_map);
    let after_bek = SortedLinearGraph::new(bek_map.clone(), Arc::new(before_bek));

    let edge = sort_checker::find_reversed_edge(&after_bek);
    if let Some(edge) = edge {
        panic!(
            "Found reversed edge: {:?}",
            (
                bek_map.get_usual_index(edge.0),
                bek_map.get_usual_index(edge.1)
            )
        );
    }

    after_bek
}

/// `assertEquals(expected: TestGraphBuilder.() -> Unit, actual: LinearGraph)`.
pub fn assert_graph_equals(expected: TestLinearGraph, actual: &(impl LinearGraph + ?Sized)) {
    assert_eq!(
        as_test_graph_string(&expected, false),
        as_test_graph_string(actual, false)
    );
}

/// `runLinearBek`: takes the graph built by the test's builder (Kotlin builds it twice).
pub fn run_linear_bek(before_linear_bek: TestLinearGraph) -> LinearBekGraph {
    let before_linear_bek_layout = build_layout(&before_linear_bek);

    let after_linear_bek = LinearBekGraph::new(Arc::new(before_linear_bek));
    LinearBekGraphBuilder::new(&after_linear_bek, &before_linear_bek_layout).collapse_all();
    after_linear_bek
}
