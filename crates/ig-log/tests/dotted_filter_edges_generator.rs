//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/DottedFilterEdgesGeneratorTest.kt`
//! and `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/InplaceFilterEdgesGeneratorTest.kt`
//! (which uses the former's top-level `LinearGraph.assert`).

mod common;

use std::sync::Arc;

use common::test_graph_builder::{TestLinearGraph, as_test_graph_string, test_graph};
use ig_log::api::elements::GraphNodeType;
use ig_log::api::linear_graph::LinearGraph;
use ig_log::collapsing::collapsed_graph::CollapsedGraph;
use ig_log::utils::unsigned_bit_set::UnsignedBitSet;

/// `LinearGraph.assert(process, result)`.
fn assert_collapsed(
    graph: TestLinearGraph,
    process: impl FnOnce(&CollapsedGraph),
    result: TestLinearGraph,
) {
    let mut nodes_visibility = UnsignedBitSet::new();
    for node_index in 0..graph.nodes_count() {
        let graph_node = graph.get_graph_node(node_index);
        nodes_visibility.set(
            graph.get_node_id(node_index),
            graph_node.node_type == GraphNodeType::Usual,
        );
    }
    let collapsed_graph = CollapsedGraph::new_instance(Arc::new(graph), nodes_visibility);
    process(&collapsed_graph);

    let expected_result_graph = result;
    let actual_result_graph = collapsed_graph.compiled_graph();

    assert_eq!(
        as_test_graph_string(&expected_result_graph, true),
        as_test_graph_string(&*actual_result_graph, true)
    );
}

mod dotted_filter_edges_generator_test {
    use super::*;
    use ig_log::collapsing::dotted_filter_edges_generator;

    /// `LinearGraph.assert(upIndex = 0, downIndex = nodesCount() - 1, result)`.
    fn assert_graph(graph: TestLinearGraph, result: TestLinearGraph) {
        let down_index = graph.nodes_count() - 1;
        assert_range(graph, 0, down_index, result);
    }

    fn assert_range(
        graph: TestLinearGraph,
        up_index: i32,
        down_index: i32,
        result: TestLinearGraph,
    ) {
        assert_collapsed(
            graph,
            |collapsed_graph| {
                dotted_filter_edges_generator::update(collapsed_graph, up_index, down_index)
            },
            result,
        )
    }

    #[test]
    fn simple() {
        assert_graph(
            test_graph! {
                1(2)
                2.UNM(3)
                3()
            },
            test_graph! {
                1(3.dot)
                3()
            },
        )
    }

    #[test]
    fn simple2_up() {
        assert_graph(
            test_graph! {
                1(3)
                2(3)
                3.UNM(4)
                4()
            },
            test_graph! {
                1(4.dot)
                2(4.dot)
                4()
            },
        )
    }

    #[test]
    fn simple2_down() {
        assert_graph(
            test_graph! {
                1(2)
                2.UNM(3, 4)
                3()
                4()
            },
            test_graph! {
                1(3.dot, 4.dot)
                3()
                4()
            },
        )
    }

    /*
     0
     |\
     1 2
     |\|\
     3 4 5
    */
    #[test]
    fn down_tree() {
        assert_graph(
            test_graph! {
                0(1, 2)
                1.UNM(3, 4)
                2.UNM(4, 5)
                3()
                4()
                5()
            },
            test_graph! {
                0(3.dot, 4.dot, 5.dot)
                3()
                4()
                5()
            },
        )
    }

    /*
     0 1 2
     \/\/
     3 4
     \/
     5
    */
    #[test]
    fn up_tree() {
        assert_graph(
            test_graph! {
                0(3)
                1(3, 4)
                2(4)
                3.UNM(5)
                4.UNM(5)
                5()
            },
            test_graph! {
                0(5.dot)
                1(5.dot)
                2(5.dot)
                5()
            },
        )
    }

    /*
     1
     |\
     2 |
     | 3
     4 |
     | 5
     |/
     6
    */
    #[test]
    fn simple_merge() {
        assert_graph(
            test_graph! {
                1(2, 3)
                2(4)
                3(5)
                4.UNM(6)
                5.UNM(6)
                6()
            },
            test_graph! {
                1(2, 3)
                2(6.dot)
                3(6.dot)
                6()
            },
        )
    }

    #[test]
    fn simple_merge2() {
        assert_graph(
            test_graph! {
                1(2, 3)
                2.UNM(4)
                3.UNM(5)
                4(6)
                5(6)
                6()
            },
            test_graph! {
                1(4.dot, 5.dot)
                4(6)
                5(6)
                6()
            },
        )
    }

    #[test]
    fn fork() {
        assert_graph(
            test_graph! {
                0(2)
                1(3)
                2(3)
                3.UNM(4)
                4.UNM(5)
                5()
            },
            test_graph! {
                0(2)
                1(5.dot)
                2(5.dot)
                5()
            },
        )
    }

    /*
    0
    |
    1
    |\
    | 2
    3
    | 4
    |/
    5
    |
    6
    */
    #[test]
    #[ignore = "This test fails, there has to be a 0-6 dotted edge, but there isn't"]
    fn another_root() {
        assert_graph(
            test_graph! {
                0(1)
                1.UNM(2, 3)
                2()
                3.UNM(5)
                4(5)
                5.UNM(6)
                6()
            },
            test_graph! {
                0(2.dot, 6.dot)
                2()
                4(6.dot)
                6()
            },
        )
    }
}

mod inplace_filter_edges_generator_test {
    use super::*;
    use ig_log::collapsing::dotted_filter_edges_generator;
    use std::collections::HashSet;

    /// `LinearGraph.assert(vararg toHide, result)`.
    fn assert_hidden(graph: TestLinearGraph, to_hide: &[i32], result: TestLinearGraph) {
        let nodes_count = graph.nodes_count();
        assert_collapsed(
            graph,
            |collapsed_graph| {
                dotted_filter_edges_generator::update(collapsed_graph, 0, nodes_count - 1);
                let to_hide: HashSet<i32> = to_hide
                    .iter()
                    .map(|&it| collapsed_graph.compiled_graph().get_node_index(it).unwrap())
                    .collect();
                dotted_filter_edges_generator::hide_inplace(collapsed_graph, &to_hide);
            },
            result,
        )
    }

    /*
    1
    |
    2
    |
    3
    |
    4
    */
    #[test]
    fn simple_line() {
        assert_hidden(
            test_graph! {
                1(2)
                2.UNM(3)
                3(4)
                4()
            },
            &[3],
            test_graph! {
                1(4.dot)
                4()
            },
        )
    }

    /*
     0
     |\
     | 1
     2 |
     |/
     3
    */
    #[test]
    fn simple_triangular_merge() {
        assert_hidden(
            test_graph! {
                0(1, 2)
                1(3)
                2.UNM(3)
                3()
            },
            &[0],
            test_graph! {
                1(3)
                3()
            },
        )
    }

    /*
    1
    |
    2
    |\
    | 3
    5  \
    |   4
    6
    */
    #[test]
    fn two_roots_merge() {
        assert_hidden(
            test_graph! {
                1(2)
                2(3, 5)
                3.UNM(4)
                4()
                5.UNM(6)
                6()
            },
            &[2],
            test_graph! {
                1(4.dot, 6.dot)
                4()
                6()
            },
        )
    }

    /*
      0
     /|
    1 |
    | 2
    3 |
    | 4
    |/|
    5 |
    | 6
    7 |
    | 8
    |/|
    9 |
    |/
    10
    */
    #[test]
    fn multiple_merges() {
        assert_hidden(
            test_graph! {
                0(1, 2)
                1(3)
                2.UNM(4)
                3.UNM(5)
                4(5, 6)
                5(7)
                6.UNM(8)
                7.UNM(9)
                8(9, 10)
                9(10)
                10.UNM()
            },
            &[0, 4, 8],
            test_graph! {
                1(5.dot)
                5(9.dot)
                9()
            },
        )
    }
}
