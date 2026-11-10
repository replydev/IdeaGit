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
        let visible = graph.create_visible_graph(Options::DEFAULT, None, None);
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

/// IdeaGit addition (`EdgePrintElementImpl::is_terminal`, needed by the Log painter): the cut ends
/// of a hidden long edge are terminal arrows; ordinary edges are not terminal.
#[test]
fn long_edge_cut_points_are_terminal_arrows() {
    use ig_log::GraphPrintElement;
    use ig_log::print_element::{EdgePrintElement, PrintElement};

    // `side` (newest) -> c0, next to the 80-commit chain c79 -> ... -> c0
    let mut commits = vec![GraphCommitImpl::create_commit(
        "side".to_string(),
        vec!["c0".to_string()],
        1000,
    )];
    commits.extend(linear_chain(80));
    let graph = build(&commits, &HashSet::new());
    let visible = graph.create_visible_graph(Options::DEFAULT, None, None);
    let edges: Vec<_> = (0..visible.visible_commit_count())
        .flat_map(|row| visible.get_row_info(row).print_elements())
        .filter_map(|e| match e {
            GraphPrintElement::Edge(edge) => Some(edge),
            GraphPrintElement::Node(_) => None,
        })
        .collect();
    let terminal: Vec<_> = edges.iter().filter(|e| e.is_terminal()).collect();
    assert!(!terminal.is_empty(), "the long edge is cut");
    for edge in &terminal {
        assert!(edge.has_arrow());
        assert_eq!(edge.position_in_current_row(), edge.position_in_other_row());
    }
    assert!(edges.iter().any(|e| !e.is_terminal()));
}
