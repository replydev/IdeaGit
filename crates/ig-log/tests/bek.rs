//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/BekTest.kt`.

mod common;

use common::linear_bek::{assert_graph_equals, run_bek};
use common::test_graph_builder::{TestLinearGraph, test_graph};

fn run_test(before_bek: TestLinearGraph, after_bek: TestLinearGraph) {
    assert_graph_equals(after_bek, &run_bek(before_bek));
}

#[test]
fn ea68497() {
    run_test(
        test_graph! {
            0(1, 3)
            1(4, 2)
            2(4, 3)
            3()
            4()
        },
        test_graph! {
            0(1, 3)
            1(4, 2)
            2(4, 3)
            3()
            4()
        },
    )
}

#[test]
fn simple_merge() {
    run_test(
        test_graph! {
            0(1, 2)
            1(3)
            2(4)
            3(5)
            4(5)
            5()
        },
        test_graph! {
            0(1, 2)
            2(4)
            4(5)
            1(3)
            3(5)
            5()
        },
    )
}

#[test]
fn two_heads() {
    run_test(
        test_graph! {
            0(2)
            1(2)
            2()
        },
        test_graph! {
            1(2)
            0(2)
            2()
        },
    )
}

#[test]
fn simple_merge_with_two_heads() {
    run_test(
        test_graph! {
            0(2, 3)
            1(4)
            2(4)
            3(5)
            4(6)
            5(6)
            6()
        },
        test_graph! {
            1(4)
            0(2, 3)
            3(5)
            5(6)
            2(4)
            4(6)
            6()
        },
    )
}
