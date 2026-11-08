//! IdeaGit tests for documented deviations of `PermanentLinearGraphBuilder` from upstream; these
//! are not upstream tests (see `reference/upstream-map.toml`).

mod common;

use std::collections::HashMap;

use common::parser::parse_string_commit_list;
use ig_log::permanent::permanent_linear_graph_builder::PermanentLinearGraphBuilder;
use ig_log::permanent::permanent_linear_graph_impl::PermanentLinearGraphImpl;

/// Builds the graph like `PermanentGraphImpl::new_instance`: not-loaded parents get ids
/// `-2, -3, ...` in the order the builder asks for them. Returns the graph and the id -> commit map.
fn build(text: &str) -> (PermanentLinearGraphImpl, HashMap<i32, String>) {
    let commits = parse_string_commit_list(text);
    let mut not_loaded: HashMap<i32, String> = HashMap::new();
    let graph = PermanentLinearGraphBuilder::new_instance(&commits).build_with(|commit| {
        let id = -(not_loaded.len() as i32 + 2);
        not_loaded.insert(id, commit.clone());
        id
    });
    (graph, not_loaded)
}

/// Upstream filled the first free edge slot with the not-loaded id, swapping parents `p` and `q`
/// of `b` here (`q` is processed first because its first child `a` comes earlier).
#[test]
fn not_loaded_parents_keep_parent_order() {
    let (graph, not_loaded) = build("a|-q\nb|-p q\n");
    let mut parents = Vec::new();
    graph.for_each_parent_node(1, |id| parents.push(not_loaded[&id].clone()));
    assert_eq!(parents, ["p", "q"]);
}

/// `p` and `q` tie on their smallest child; the synthetic ids must not depend on `HashMap` order.
#[test]
fn not_loaded_parent_ids_are_deterministic() {
    for _ in 0..20 {
        let (_, not_loaded) = build("b|-p q\n");
        assert_eq!(not_loaded.len(), 2);
        assert_eq!(not_loaded[&-2], "p");
        assert_eq!(not_loaded[&-3], "q");
    }
}
