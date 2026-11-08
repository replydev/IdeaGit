//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphBuilderTest.kt`
//! (fixtures `graphBuilder/`) and
//! `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphBuilderHashIndexTest.java`
//! (fixtures `graphHashIndex/`), each for string and integer commit ids.

mod common;

use std::collections::HashMap;

use common::str_utils::{commits_info_to_str, graph_as_string};
use common::{CommitIdManager, IntegerCommitIdManager, StringCommitIdManager, do_test};
use ig_log::permanent::permanent_commits_info_impl::PermanentCommitsInfoImpl;
use ig_log::permanent::permanent_linear_graph_builder::PermanentLinearGraphBuilder;

fn graph_builder_test<M: CommitIdManager>(test_name: &str) {
    do_test("graphBuilder", test_name, |input, out| {
        let commits = M::parse_commit_list(input);

        let graph_builder = PermanentLinearGraphBuilder::new_instance(&commits);
        let graph = graph_builder.build_with(|dom| -M::to_int(dom));

        assert_eq!(out, graph_as_string(&graph, false));
    });
}

fn graph_builder_hash_index_test<M: CommitIdManager>(test_name: &str) {
    do_test("graphHashIndex", test_name, |input, out| {
        let commits = M::parse_commit_list(input);
        let commits_info = PermanentCommitsInfoImpl::new_instance(&commits, HashMap::new());

        assert_eq!(
            out,
            commits_info_to_str(&commits_info, commits.len(), M::to_str)
        );
    });
}

macro_rules! graph_builder_tests {
    ($module:ident, $manager:ty) => {
        mod $module {
            fn run(test_name: &str) {
                crate::graph_builder_test::<$manager>(test_name);
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
            fn many_up_nodes() {
                run("manyUpNodes");
            }
            #[test]
            fn many_down_nodes() {
                run("manyDownNodes");
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
            fn not_full_graph() {
                run("notFullGraph");
            }
            #[test]
            fn parents_order() {
                run("parentsOrder");
            }
            #[test]
            fn duplicate_parents() {
                run("duplicateParents");
            }
        }
    };
}

macro_rules! graph_builder_hash_index_tests {
    ($module:ident, $manager:ty) => {
        mod $module {
            fn run(test_name: &str) {
                crate::graph_builder_hash_index_test::<$manager>(test_name);
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

/// `GraphBuilderTest.StringTest` / `IntegerTest`
mod graph_builder_test {
    graph_builder_tests!(string_test, crate::StringCommitIdManager);
    graph_builder_tests!(integer_test, crate::IntegerCommitIdManager);
}

/// `GraphBuilderHashIndexTest.StringTest` / `IntegerTest`
mod graph_builder_hash_index_test {
    graph_builder_hash_index_tests!(string_test, crate::StringCommitIdManager);
    graph_builder_hash_index_tests!(integer_test, crate::IntegerCommitIdManager);
}
