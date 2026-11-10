//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/facade/FirstParentTest.kt`.

mod common;

use std::collections::HashSet;
use std::sync::Arc;

use common::linear_bek::assert_graph_equals;
use common::test_graph_builder::{TestLinearGraph, test_graph};
use ig_log::facade::first_parent_controller::build_collapsed_graph;

fn set_of(elements: &[i32]) -> HashSet<i32> {
    elements.iter().copied().collect()
}

fn assert_first_parent(
    graph: TestLinearGraph,
    start_nodes: HashSet<i32>,
    matched_nodes: Option<HashSet<i32>>,
    expected: TestLinearGraph,
) {
    let actual_graph = build_collapsed_graph(Arc::new(graph), &start_nodes, matched_nodes.as_ref())
        .compiled_graph();
    assert_graph_equals(expected, &*actual_graph);
}

#[test]
fn simple_merge() {
    assert_first_parent(
        test_graph! {
            0(1)
            1(2, 3)
            2(4)
            3(4)
            4()
        },
        set_of(&[0]),
        None,
        test_graph! {
            0(1)
            1(2)
            2(4)
            4()
        },
    );
}

#[test]
fn two_branches() {
    assert_first_parent(
        test_graph! {
            0(2)
            1(5)
            2(3, 4)
            3(6)
            4(5)
            5(6)
            6()
        },
        set_of(&[0, 1]),
        None,
        test_graph! {
            0(2)
            1(5)
            2(3)
            3(6)
            5(6)
            6()
        },
    );
}

#[test]
fn start_not_at_head() {
    assert_first_parent(
        test_graph! {
            0(1)
            1(2, 3)
            2(4)
            3(4)
            4()
        },
        set_of(&[3]),
        None,
        test_graph! {
            3(4)
            4()
        },
    );
}

#[test]
fn feature_branch_merges() {
    // main branch merged into feature, then feature merged into master
    assert_first_parent(
        test_graph! {
            0(2)
            1(3)
            2(5, 3)
            3(4)
            4(6, 5)
            5(7)
            6(7)
            7()
        },
        set_of(&[0, 1]),
        None,
        test_graph! {
            0(2)
            1(3)
            2(5)
            3(4)
            4(6)
            5(7)
            6(7)
            7()
        },
    );
}

#[test]
fn filtered_merge() {
    assert_first_parent(
        test_graph! {
            0(1)
            1(2, 3)
            2(4)
            3(4)
            4()
        },
        set_of(&[0]),
        Some(set_of(&[1, 3, 4])),
        test_graph! {
            1(4.dot)
            4()
        },
    );
}
