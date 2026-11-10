//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/LinearBekExpandTest.kt`.

mod common;

use common::linear_bek::{assert_graph_equals, build_layout, run_linear_bek};
use common::test_graph_builder::{TestLinearGraph, test_graph};
use ig_log::api::elements::{GraphEdge, GraphEdgeType};
use ig_log::linear_bek::linear_bek_graph_builder::LinearBekGraphBuilder;

fn run_test(
    before_linear_bek: TestLinearGraph,
    after_expansion: TestLinearGraph,
    from_node_to_expand: i32,
    to_node_to_expand: i32,
) {
    let after_linear_bek = run_linear_bek(before_linear_bek);
    after_linear_bek.expand_edge(&GraphEdge::create_normal_edge(
        from_node_to_expand,
        to_node_to_expand,
        GraphEdgeType::Dotted,
    ));
    assert_graph_equals(after_expansion, &after_linear_bek);
}

/*
0              0
|\             |
| 1            1
| |\           |
| | 2          2
| | |          :
| 3 |   ->     3
| |/           |
| 4            4
| |            :
5 |            5
|/             |
6              6
 */
#[test]
fn recursive_expansion_test() {
    run_test(
        test_graph! {
            0(5, 1)
            1(3, 2)
            2(4)
            3(4)
            4(6)
            5(6)
            6()
        },
        test_graph! {
            0(5, 1)
            1(2)
            2(3.dot)
            3(4)
            4(6)
            5(6)
            6()
        },
        4,
        5,
    )
}

/*
0                0
|\               |
| 1              1
| |\             |
| | 2            2
| | |            :
3 | |    ->      3
4 | |            4
|/ /             |
5 /              5
\/               |
6                6
 */
#[test]
fn small_recursive_branches_test() {
    run_test(
        test_graph! {
            0(3, 1)
            1(5, 2)
            2(6)
            3(4)
            4(5)
            5(6)
            6()
        },
        test_graph! {
            0(3, 1)
            1(5, 2)
            2(6)
            3(4)
            4(5)
            5(6)
            6()
        },
        2,
        3,
    )
}

/*
0
|\
| 1
|  \
|  2
|  |\
|  | 3
|  | |
4  | |
|\ | |
|5 | |
|\/  |
|/\  |
6 | /
|/ /
8 /
|/
9
*/
#[test]
fn several_collapses_test() {
    let builder = || {
        test_graph! {
            0(4, 1)
            1(2)
            2(6, 3)
            3(9)
            4(6, 5)
            5(8)
            6(8)
            7(8)
            8(9)
            9()
        }
    };
    let layout = build_layout(&builder());
    let after_linear_bek = run_linear_bek(builder());

    after_linear_bek.expand_edge(&GraphEdge::create_normal_edge(5, 6, GraphEdgeType::Dotted));
    LinearBekGraphBuilder::new(&after_linear_bek, &layout).collapse_fragment(4);
    after_linear_bek.expand_edge(&GraphEdge::create_normal_edge(3, 4, GraphEdgeType::Dotted));

    assert_graph_equals(
        test_graph! {
            0(4, 1)
            1(2)
            2(6, 3)
            3(9)
            4(5)
            5(6.dot)
            6(8)
            7(8)
            8(9)
            9()
        },
        &after_linear_bek,
    );
}

/*
0
|\
| 1
| |\
|/ 2
3 /
|/
4
*/
#[test]
fn replace_dotted_edge_test() {
    let builder = || {
        test_graph! {
            0(3, 1)
            1(3, 2)
            2(4)
            3(4)
            4()
        }
    };
    let after_linear_bek = run_linear_bek(builder());

    after_linear_bek.expand_edge(&GraphEdge::create_normal_edge(2, 3, GraphEdgeType::Dotted));

    assert_graph_equals(builder(), &after_linear_bek);
}
