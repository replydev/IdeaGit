//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/FragmentGeneratorTest.kt`.

mod common;

use std::collections::HashSet;
use std::sync::LazyLock;

use common::test_graph_builder::{TestLinearGraph, test_graph};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::collapsing::fragment_generator::{FragmentGenerator, GreenFragment};
use ig_log::utils::linear_graph_utils::{LiteLinearGraphAdapter, as_lite_linear_graph};

type Lite = LiteLinearGraphAdapter<&'static TestLinearGraph>;

fn lite(graph: &'static TestLinearGraph) -> &'static Lite {
    Box::leak(Box::new(as_lite_linear_graph(graph)))
}

fn get_middle_nodes(graph: &'static TestLinearGraph, up_node: i32, down_node: i32) -> HashSet<i32> {
    FragmentGenerator::new(lite(graph), |_| false).get_middle_nodes(up_node, down_node, false)
}

fn assert_nodes(nodes: HashSet<i32>, s: &str) {
    let mut sorted: Vec<i32> = nodes.into_iter().collect();
    sorted.sort();
    let joined: Vec<String> = sorted.iter().map(i32::to_string).collect();
    assert_eq!(s, joined.join(","));
}

fn assert_node(actual: Option<i32>, i: Option<i32>) {
    assert_eq!(i, actual);
}

fn red_nodes(
    graph: &'static TestLinearGraph,
    red_node: &[i32],
) -> FragmentGenerator<'static, Lite, impl Fn(i32) -> bool> {
    let red_nodes: HashSet<i32> = red_node.iter().copied().collect();

    FragmentGenerator::new(lite(graph), move |it| {
        red_nodes.contains(&graph.get_node_id(it))
    })
}

fn s(value: Option<i32>) -> String {
    value.map_or_else(|| "n".to_string(), |v| v.to_string())
}

fn assert_fragment(fragment: GreenFragment, expected: &str) {
    let mut middle: Vec<i32> = fragment.middle_green_nodes().iter().copied().collect();
    middle.sort();
    let middle: Vec<String> = middle.iter().map(i32::to_string).collect();
    assert_eq!(
        expected,
        format!(
            "{}|{}|{}",
            s(fragment.up_red_node()),
            s(fragment.down_red_node()),
            middle.join(",")
        )
    );
}

static SIMPLE: LazyLock<TestLinearGraph> = LazyLock::new(|| {
    test_graph! {
        0(1, 2)
        1(2)
        2()
    }
});

static TWO_BRANCH: LazyLock<TestLinearGraph> = LazyLock::new(|| {
    test_graph! {
        0(2)
        1(2)
        2()
    }
});

static DOWN_TREE: LazyLock<TestLinearGraph> = LazyLock::new(|| {
    test_graph! {
        0(1, 2)
        1(3, 4)
        2(4, 5)
        3()
        4()
        5()
    }
});

static UP_TREE: LazyLock<TestLinearGraph> = LazyLock::new(|| {
    test_graph! {
        0(3)
        1(3, 4)
        2(4)
        3(5)
        4(5)
        5()
    }
});

static DIFFICULT: LazyLock<TestLinearGraph> = LazyLock::new(|| {
    test_graph! {
        0(1, 2)
        1(3, 4)
        2(6)
        3(5)
        4(6)
        5(7, 8)
        6(7, 8)
        7()
        8()
    }
});

mod middle_nodes_test {
    use super::*;

    #[test]
    fn simple() {
        assert_nodes(get_middle_nodes(&SIMPLE, 0, 2), "0,1,2")
    }
    #[test]
    fn empty() {
        assert_nodes(get_middle_nodes(&TWO_BRANCH, 0, 1), "")
    }
    #[test]
    fn with_down_redundant_branches() {
        assert_nodes(get_middle_nodes(&DOWN_TREE, 0, 4), "0,1,2,4")
    }
    #[test]
    fn with_up_redundant_branches() {
        assert_nodes(get_middle_nodes(&UP_TREE, 1, 5), "1,3,4,5")
    }

    #[test]
    fn difficult1() {
        assert_nodes(get_middle_nodes(&DIFFICULT, 1, 7), "1,3,4,5,6,7")
    }
    #[test]
    fn difficult2() {
        assert_nodes(get_middle_nodes(&DIFFICULT, 0, 5), "0,1,3,5")
    }
    #[test]
    fn difficult3() {
        assert_nodes(get_middle_nodes(&DIFFICULT, 1, 8), "1,3,4,5,6,8")
    }
}

mod near_red_node {
    use super::*;

    #[test]
    fn simple1() {
        assert_node(
            red_nodes(&SIMPLE, &[0]).get_near_red_node(2, 10, true),
            Some(0),
        )
    }
    #[test]
    fn simple2() {
        assert_node(
            red_nodes(&SIMPLE, &[2]).get_near_red_node(0, 10, false),
            Some(2),
        )
    }
    #[test]
    fn simple3() {
        assert_node(
            red_nodes(&SIMPLE, &[2, 1]).get_near_red_node(0, 10, false),
            Some(1),
        )
    }
    #[test]
    fn simple4() {
        assert_node(
            red_nodes(&SIMPLE, &[0, 1]).get_near_red_node(0, 10, false),
            Some(0),
        )
    }
    #[test]
    fn simple5() {
        assert_node(
            red_nodes(&SIMPLE, &[0, 1]).get_near_red_node(2, 10, true),
            Some(1),
        )
    }

    #[test]
    fn down_tree1() {
        assert_node(
            red_nodes(&DOWN_TREE, &[4, 5]).get_near_red_node(0, 10, false),
            Some(4),
        )
    }
    #[test]
    fn down_tree2() {
        assert_node(
            red_nodes(&DOWN_TREE, &[2, 3]).get_near_red_node(0, 10, false),
            Some(2),
        )
    }
    #[test]
    fn up_tree1() {
        assert_node(
            red_nodes(&UP_TREE, &[1, 2]).get_near_red_node(5, 10, true),
            Some(2),
        )
    }
    #[test]
    fn up_tree2() {
        assert_node(
            red_nodes(&UP_TREE, &[4, 0]).get_near_red_node(5, 10, true),
            Some(4),
        )
    }

    #[test]
    fn difficult1() {
        assert_node(
            red_nodes(&DIFFICULT, &[4, 5, 6]).get_near_red_node(0, 10, false),
            Some(4),
        )
    }
    #[test]
    fn difficult2() {
        assert_node(
            red_nodes(&DIFFICULT, &[2, 5, 6]).get_near_red_node(1, 10, false),
            Some(5),
        )
    }

    #[test]
    fn null_answer1() {
        assert_node(
            red_nodes(&DIFFICULT, &[8]).get_near_red_node(0, 6, false),
            None,
        )
    }
    #[test]
    fn null_answer2() {
        assert_node(
            red_nodes(&DIFFICULT, &[8]).get_near_red_node(0, 7, false),
            Some(8),
        )
    }
}

mod green_fragment {
    use super::*;

    #[test]
    fn simple1() {
        assert_fragment(
            red_nodes(&SIMPLE, &[0]).get_green_fragment_for_collapse(2, 10),
            "0|n|1,2",
        )
    }
    #[test]
    fn simple2() {
        assert_fragment(
            red_nodes(&SIMPLE, &[0, 2]).get_green_fragment_for_collapse(1, 10),
            "0|2|1",
        )
    }
    #[test]
    fn simple3() {
        assert_fragment(
            red_nodes(&SIMPLE, &[0, 2]).get_green_fragment_for_collapse(0, 10),
            "n|n|",
        )
    }

    #[test]
    fn down_tree1() {
        assert_fragment(
            red_nodes(&DOWN_TREE, &[4]).get_green_fragment_for_collapse(2, 10),
            "n|4|0,2",
        )
    }
    #[test]
    fn down_tree2() {
        assert_fragment(
            red_nodes(&DOWN_TREE, &[4]).get_green_fragment_for_collapse(1, 10),
            "n|4|0,1",
        )
    }

    #[test]
    fn difficult1() {
        assert_fragment(
            red_nodes(&DIFFICULT, &[1, 7]).get_green_fragment_for_collapse(3, 10),
            "1|7|3,5",
        )
    }
    #[test]
    fn difficult2() {
        assert_fragment(
            red_nodes(&DIFFICULT, &[1, 7]).get_green_fragment_for_collapse(8, 10),
            "1|n|3,4,5,6,8",
        )
    }
    #[test]
    fn difficult3() {
        assert_fragment(
            red_nodes(&DIFFICULT, &[1, 7]).get_green_fragment_for_collapse(2, 10),
            "n|7|0,2,6",
        )
    }
}
