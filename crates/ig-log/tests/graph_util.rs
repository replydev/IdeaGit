//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/GraphUtilTests.kt`.

mod common;

use std::collections::HashSet;

use common::str_utils::graph_as_string;
use common::test_graph_builder::{TestLinearGraph, test_graph};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::utils::flags::BitSetFlags;
use ig_log::utils::graph_util::{exclusive_nodes, get_corresponding_parent, subgraph_difference};
use ig_log::utils::linear_graph_utils::as_lite_linear_graph;

fn assert_corresponding_parent(
    start_node: i32,
    end_node: i32,
    expected_parent: i32,
    graph: TestLinearGraph,
) {
    let actual_parent = get_corresponding_parent(
        &as_lite_linear_graph(&graph),
        start_node,
        end_node,
        &mut BitSetFlags::new(graph.nodes_count()),
    );
    assert_eq!(
        actual_parent,
        expected_parent,
        "Incorrect parent found when walking from {start_node} to {end_node} in {}",
        graph_as_string(&graph, true)
    );
}

#[test]
fn one_parent() {
    assert_corresponding_parent(
        0,
        5,
        1,
        test_graph! {
            0(1)
            1(2)
            2(3, 4)
            3(5)
            4(5)
            5()
        },
    );
}

#[test]
fn target_parent() {
    assert_corresponding_parent(
        0,
        2,
        2,
        test_graph! {
            0(1, 2)
            1(3)
            2(3)
            3()
        },
    );
}

#[test]
fn merge_commit() {
    assert_corresponding_parent(
        0,
        4,
        2,
        test_graph! {
            0(1, 2)
            1(3)
            2(3, 4)
            3()
            4()
        },
    );
}

fn assert_subgraph_difference(
    node1: i32,
    node2: i32,
    expected_difference: HashSet<i32>,
    graph: TestLinearGraph,
) {
    let actual_difference = subgraph_difference(&graph, node1, node2);
    assert_eq!(
        actual_difference,
        expected_difference,
        "Incorrect subgraph difference calculated between {node1} to {node2} in {}",
        graph_as_string(&graph, true)
    );
}

fn int_set_of(elements: &[i32]) -> HashSet<i32> {
    elements.iter().copied().collect()
}

/*
0
| 1
2 |
| 3
4 |
| 5
6 |
|/
7
8
9
*/
#[test]
fn single_branches() {
    assert_subgraph_difference(
        0,
        1,
        int_set_of(&[0, 2, 4, 6]),
        test_graph! {
            0(2)
            1(3)
            2(4)
            3(5)
            4(6)
            5(7)
            6(7)
            7(8)
            8(9)
            9()
        },
    );
}

/*
0
|\
| 1
2 |\
| | 3
| 4 |
5 |/
| 6
|/
7
8
*/
#[test]
fn ancestor() {
    assert_subgraph_difference(
        0,
        3,
        int_set_of(&[0, 1, 2, 4, 5]),
        test_graph! {
            0(1, 2)
            1(3, 4)
            2(5)
            3(6)
            4(6)
            5(7)
            6(7)
            7(8)
            8()
        },
    );
}

/*
 0
 |\
 | 1
 2 |\
 | | 3
 | 4 |
 5 |/
 | 6
 |/
 7
 8
*/
#[test]
fn descendant() {
    assert_subgraph_difference(
        3,
        0,
        HashSet::new(),
        test_graph! {
            0(1, 2)
            1(3, 4)
            2(5)
            3(6)
            4(6)
            5(7)
            6(7)
            7(8)
            8()
        },
    );
}

/*
0
|  1
2  |
|\ 3
| \|\
|  4 |
|  | 5
|  |/
| /|
|/ |
6  |
|  7
8 /
|/
9
*/
#[test]
fn multiple_bases() {
    assert_subgraph_difference(
        0,
        1,
        int_set_of(&[0, 2]),
        test_graph! {
            0(2)
            1(3)
            2(4, 6)
            3(4, 5)
            4(7)
            5(6)
            6(8)
            7(9)
            8(9)
            9()
        },
    );
}

fn assert_exclusive_nodes(
    node: i32,
    expected_exclusive_nodes: HashSet<i32>,
    other_heads: HashSet<i32>,
    graph: &TestLinearGraph,
) {
    let actual_exclusive_nodes = exclusive_nodes(graph, node, |n| other_heads.contains(&n));
    assert_eq!(
        actual_exclusive_nodes,
        expected_exclusive_nodes,
        "Incorrect exclusive nodes for {node} {}",
        graph_as_string(graph, true)
    );
}

/*
0
| 1
2 |
| 3
4 |
| 5
6 |
|/
7
8
9
*/
#[test]
fn simple_branch() {
    let graph = test_graph! {
        0(2)
        1(3)
        2(4)
        3(5)
        4(6)
        5(7)
        6(7)
        7(8)
        8(9)
        9()
    };
    assert_exclusive_nodes(0, int_set_of(&[0, 2, 4, 6]), HashSet::new(), &graph);
    assert_exclusive_nodes(0, int_set_of(&[0, 2]), int_set_of(&[0, 1, 4]), &graph);
    assert_exclusive_nodes(1, int_set_of(&[1, 3, 5]), HashSet::new(), &graph);
    assert_exclusive_nodes(1, int_set_of(&[1, 3]), int_set_of(&[0, 1, 5]), &graph);
}

/*
0
|   1
2   |
|\  |
3 | |
| 4 |
|/  5
6  /
| /
7
8
*/
#[test]
fn branch_with_merge_commit() {
    let graph = test_graph! {
        0(2)
        1(5)
        2(3, 4)
        3(6)
        4(6)
        5(7)
        6(7)
        7(8)
        8()
    };
    assert_exclusive_nodes(0, int_set_of(&[0, 2, 3, 4, 6]), HashSet::new(), &graph);
    assert_exclusive_nodes(0, int_set_of(&[0, 2, 4]), int_set_of(&[0, 1, 3]), &graph);
    assert_exclusive_nodes(1, int_set_of(&[1, 5]), HashSet::new(), &graph);
    assert_exclusive_nodes(1, int_set_of(&[1]), int_set_of(&[0, 1, 5]), &graph);
}
