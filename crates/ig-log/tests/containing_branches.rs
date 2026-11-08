//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/ContainingBranchesTest.java`
//! (fixtures `containingBranches/`, string and integer commit ids).

mod common;

use std::collections::HashSet;

use common::str_utils::containing_branches_getter_to_str;
use common::{CommitIdManager, IntegerCommitIdManager, StringCommitIdManager, do_test};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::facade::reachable_nodes::ReachableNodes;
use ig_log::permanent::permanent_linear_graph_builder::PermanentLinearGraphBuilder;
use ig_log::utils::linear_graph_utils::as_lite_linear_graph;

const SEPARATOR: &str = "\nBRANCH NODES:\n";

fn parse_branch_node_index(s: &str) -> HashSet<i32> {
    common::parser::split_whitespace_java(s)
        .into_iter()
        .map(|sub_str| {
            sub_str
                .parse()
                .unwrap_or_else(|e| panic!("{sub_str:?}: {e}"))
        })
        .collect()
}

fn containing_branches_test<M: CommitIdManager>(test_name: &str) {
    do_test("containingBranches/", test_name, |input, out| {
        let i = input.find(SEPARATOR).expect("no branch nodes separator");
        let commits = M::parse_commit_list(&input[..i]);

        let graph = PermanentLinearGraphBuilder::new_instance(&commits).build();
        let branches = parse_branch_node_index(&input[i + SEPARATOR.len()..]);
        let reachable_nodes = ReachableNodes::new(as_lite_linear_graph(&graph));

        assert_eq!(
            out,
            containing_branches_getter_to_str(&reachable_nodes, &branches, graph.nodes_count())
        );
    });
}

macro_rules! containing_branches_tests {
    ($module:ident, $manager:ty) => {
        mod $module {
            fn run(test_name: &str) {
                crate::containing_branches_test::<$manager>(test_name);
            }

            #[test]
            fn simple() {
                run("simple");
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
        }
    };
}

/// `ContainingBranchesTest.StringTest` / `IntegerTest`
mod containing_branches_test {
    containing_branches_tests!(string_test, crate::StringCommitIdManager);
    containing_branches_tests!(integer_test, crate::IntegerCommitIdManager);
}
