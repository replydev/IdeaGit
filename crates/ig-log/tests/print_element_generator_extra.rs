//! IdeaGit tests for `recommended_width` and for the layout tie-break of branch heads.
//! Upstream has no tests for these; expected values follow from the upstream algorithms.

use std::collections::HashSet;

use ig_log::api::GraphLayout;
use ig_log::facade::permanent_graph_impl::PermanentGraphImpl;
use ig_log::graph_commit::GraphCommitImpl;
use ig_log::permanent_graph::{Options, PermanentGraph};
use ig_log::print::graph_color_getter::GraphColorGetterByNodeFactory;
use ig_log::visible_graph::VisibleGraph;

fn build(
    commits: &[GraphCommitImpl<String>],
    branches: &HashSet<String>,
) -> PermanentGraphImpl<String> {
    let factory = GraphColorGetterByNodeFactory::new(|_: &String, layout_index| layout_index);
    PermanentGraphImpl::new_instance(
        commits,
        &factory,
        |_: &String, _: &String| std::cmp::Ordering::Equal,
        branches,
    )
}

/// `c{n-1} -> ... -> c0`, `c0` has no parents.
fn linear_chain(n: usize) -> Vec<GraphCommitImpl<String>> {
    (0..n)
        .rev()
        .map(|i| {
            let parents = if i == 0 {
                vec![]
            } else {
                vec![format!("c{}", i - 1)]
            };
            GraphCommitImpl::create_commit(format!("c{i}"), parents, i as i64)
        })
        .collect()
}

#[test]
fn recommended_width_of_linear_history_is_one() {
    for n in [10, 36] {
        let graph = build(&linear_chain(n), &HashSet::new());
        let visible = graph
            .create_visible_graph(Options::DEFAULT, None, None)
            .unwrap();
        assert_eq!(visible.recommended_width(), 1, "n = {n}");
    }
}

#[test]
fn layout_is_independent_of_branch_input_order_when_heads_tie() {
    // Five unrelated single-commit branches; the comparator never separates them.
    let commits: Vec<_> = (0..5)
        .map(|i| GraphCommitImpl::create_commit(format!("h{i}"), vec![], 10 - i as i64))
        .collect();
    let branches: Vec<String> = vec!["h4".into(), "h2".into()];
    // Branch heads first in ascending node id (2, 4), then the remaining graph heads (0, 1, 3).
    let expected = vec![3, 4, 1, 5, 2];
    // Every `HashSet` has its own random hasher, so each run iterates the branches in another order.
    for round in 0..50 {
        let mut shuffled = branches.clone();
        if round % 2 == 1 {
            shuffled.reverse();
        }
        let set: HashSet<String> = shuffled.into_iter().collect();
        let graph = build(&commits, &set);
        let layout = graph.permanent_graph_layout_impl();
        let actual: Vec<i32> = (0..5).map(|i| layout.get_layout_index(i)).collect();
        assert_eq!(actual, expected, "round {round}");
    }
}
