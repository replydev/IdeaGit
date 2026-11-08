//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilderTest.java`
//! (fixtures `layoutBuilder/`, string and integer commit ids) and
//! `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilderHeadOrderTest.kt`.

mod common;

use common::str_utils::permanent_graph_layout_model_to_str;
use common::{CommitIdManager, IntegerCommitIdManager, StringCommitIdManager, do_test};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::graph_commit::GraphCommit;
use ig_log::permanent::graph_layout_builder;
use ig_log::permanent::permanent_linear_graph_builder::PermanentLinearGraphBuilder;

fn graph_layout_builder_test<M: CommitIdManager>(test_name: &str) {
    do_test("layoutBuilder", test_name, |input, out| {
        let commits = M::parse_commit_list(input);

        let graph = PermanentLinearGraphBuilder::new_instance(&commits).build();

        let graph_layout = graph_layout_builder::build(&graph, |o1, o2| {
            let id1 = commits[o1 as usize].id();
            let id2 = commits[o2 as usize].id();
            M::to_str(id1).cmp(&M::to_str(id2))
        });
        assert_eq!(
            out,
            permanent_graph_layout_model_to_str(&graph_layout, graph.nodes_count())
        );
    });
}

macro_rules! graph_layout_builder_tests {
    ($module:ident, $manager:ty) => {
        mod $module {
            fn run(test_name: &str) {
                crate::graph_layout_builder_test::<$manager>(test_name);
            }

            #[test]
            fn many_nodes() {
                run("manyNodes");
            }
            #[test]
            fn not_full_graph() {
                run("notFullGraph");
            }
            #[test]
            fn one_node() {
                run("oneNode");
            }
            #[test]
            fn one_node_not_full_graph() {
                run("oneNodeNotFullGraph");
            }
            #[test]
            fn heads_order() {
                run("headsOrder");
            }
        }
    };
}

/// `GraphLayoutBuilderTest.StringTest` / `IntegerTest`
mod graph_layout_builder_test {
    graph_layout_builder_tests!(string_test, crate::StringCommitIdManager);
    graph_layout_builder_tests!(integer_test, crate::IntegerCommitIdManager);
}

/// `GraphLayoutBuilderHeadOrderTest`
mod graph_layout_builder_head_order_test {
    use std::cmp::Ordering;

    use super::common::test_graph_builder::graph;
    use ig_log::api::GraphLayout;
    use ig_log::api::linear_graph::LinearGraph;
    use ig_log::permanent::graph_layout_builder;

    /// Asserts that the layout built for `graph` has the expected head commit for each commit.
    fn assert_heads_for_commits(
        graph: &impl LinearGraph,
        branch_heads: &[i32],
        head_comparator: impl FnMut(i32, i32) -> Ordering,
        expected_heads: &[i32],
    ) {
        let graph_layout =
            graph_layout_builder::build_with_branches(graph, branch_heads, head_comparator);
        let heads: Vec<i32> = (0..graph.nodes_count())
            .map(|i| graph_layout.get_one_of_head_node_index(i))
            .collect();
        assert_eq!(expected_heads, heads.as_slice());
    }

    /// `list.indexOf(i) - list.indexOf(j)` (`-1` for nodes not in the list).
    fn order(list: &[i32]) -> impl FnMut(i32, i32) -> Ordering + '_ {
        let index_of = |x: i32| list.iter().position(|&y| y == x).map_or(-1, |p| p as i32);
        move |i, j| (index_of(i) - index_of(j)).cmp(&0)
    }

    #[test]
    fn linear_graph() {
        let graph = graph(|b| {
            b.node(0, [1]);
            b.node(1, [2]);
            b.node(2, [3]);
            b.node(3, [0; 0]);
        });
        assert_heads_for_commits(&graph, &[0, 2], order(&[0, 2]), &[0, 0, 0, 0]);
        assert_heads_for_commits(&graph, &[0, 2], order(&[2, 0]), &[0, 0, 2, 2]);
    }

    #[test]
    fn branching_graph() {
        let graph = graph(|b| {
            b.node(0, [2]);
            b.node(1, [3]);
            b.node(2, [3]);
            b.node(3, [4]);
            b.node(4, [0; 0]);
        });
        assert_heads_for_commits(&graph, &[0, 2], order(&[0, 1, 2]), &[0, 1, 0, 0, 0]);
        assert_heads_for_commits(&graph, &[0, 2], order(&[0, 2, 1]), &[0, 1, 0, 0, 0]);
        assert_heads_for_commits(&graph, &[0, 2], order(&[1, 0, 2]), &[0, 1, 0, 1, 1]);
        assert_heads_for_commits(&graph, &[0, 2], order(&[1, 2, 0]), &[0, 1, 2, 1, 1]);
        assert_heads_for_commits(&graph, &[0, 2], order(&[2, 0, 1]), &[0, 1, 2, 2, 2]);
        assert_heads_for_commits(&graph, &[0, 2], order(&[2, 1, 0]), &[0, 1, 2, 2, 2]);
    }
}
