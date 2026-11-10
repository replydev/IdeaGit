//! IdeaGit tests for `PermanentGraphImpl::create_visible_graph` with every option: the controller
//! chains, the shared BEK map, linear BEK actions and `SimpleGraphInfo`. Upstream has no tests for
//! these paths; expected values follow from the upstream algorithms.

use std::collections::HashSet;
use std::sync::Arc;

use ig_log::actions::{GraphAction, GraphActionType};
use ig_log::api::permanent::PermanentGraphInfo;
use ig_log::api::printer::GraphPrintElement;
use ig_log::facade::permanent_graph_impl::PermanentGraphImpl;
use ig_log::facade::visible_graph_impl::VisibleGraphImpl;
use ig_log::graph_commit::{GraphCommit, GraphCommitImpl};
use ig_log::permanent_graph::{Options, PermanentGraph, SortType};
use ig_log::print::graph_color_getter::GraphColorGetterByNodeFactory;
use ig_log::print_element::PrintElement;
use ig_log::visible_graph::VisibleGraph;

/// `a|-b c` lines; timestamps are the line numbers counted from the end.
fn commits(text: &str) -> Vec<GraphCommitImpl<String>> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let count = lines.len() as i64;
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let (id, parents) = line.split_once("|-").unwrap();
            let parents = parents.split_whitespace().map(str::to_string).collect();
            GraphCommitImpl::create_commit(id.to_string(), parents, count - i as i64)
        })
        .collect()
}

fn set(ids: &[&str]) -> HashSet<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

fn permanent_graph(text: &str, branches: &[&str]) -> PermanentGraphImpl<String> {
    let factory = GraphColorGetterByNodeFactory::new(|_: &String, layout_index| layout_index);
    PermanentGraphImpl::new_instance(
        &commits(text),
        &factory,
        |a: &String, b: &String| a.cmp(b),
        &set(branches),
    )
}

/// Two merged feature branches, a branch that is not merged, and not-loaded parents (`x`, `y`).
const GRAPH: &str = "
m0|-m1 f2
f2|-f1
m1|-m2 g1
g1|-g0 y
m2|-m3
f1|-f0
f0|-m3
g0|-m3
m3|-m4
b1|-b0
b0|-m4
m4|-x
";

fn options() -> [Options; 4] {
    [
        Options::Base(SortType::Normal),
        Options::Base(SortType::Bek),
        Options::LinearBek,
        Options::FirstParent,
    ]
}

fn act(
    visible: &mut VisibleGraphImpl<String>,
    element: Option<GraphPrintElement>,
    kind: GraphActionType,
) {
    let answer = visible
        .action_controller()
        .perform_action(GraphAction::new(element, kind));
    if let Some(updater) = answer.graph_updater {
        visible.run_graph_updater(updater);
    }
}

/// Reads every row and performs every action on every element (re-reading the rows, as actions
/// may change the graph).
fn exercise(visible: &mut VisibleGraphImpl<String>, stale: &[GraphPrintElement]) {
    for kind in [GraphActionType::MouseOver, GraphActionType::MouseClick] {
        let mut row = 0;
        while row < visible.visible_commit_count() {
            let elements = {
                let info = visible.get_row_info(row);
                let _ = (info.commit(), info.one_of_heads(), info.row_type());
                let _ = (info.adjacent_rows(true), info.adjacent_rows(false));
                info.print_elements()
            };
            for element in elements {
                if element.row_index() < visible.visible_commit_count() {
                    act(visible, Some(element), kind);
                }
            }
            row += 1;
        }
        act(visible, None, kind);
    }
    for element in stale {
        act(visible, Some(element.clone()), GraphActionType::MouseOver);
        act(visible, Some(element.clone()), GraphActionType::MouseClick);
    }
    for kind in [
        GraphActionType::ButtonCollapse,
        GraphActionType::ButtonExpand,
        GraphActionType::ButtonCollapse,
    ] {
        act(visible, None, kind);
        assert!(visible.recommended_width() >= 0);
    }
    assert_eq!(visible.get_visible_row_index(&"zz".to_string()), None);
    assert_eq!(visible.get_visible_row_index(&"x".to_string()), None);
}

/// IdeaGit: every option with every kind of visible heads and matched commits, including empty
/// sets and ids that are unknown or not loaded, builds a visible graph that survives every action.
#[test]
fn every_option_combination_works() {
    let graph = permanent_graph(GRAPH, &["m0", "b1"]);
    // elements from a bigger graph: out of range here
    let mut long_text = String::from("n0|-n1 n40\n");
    for i in 1..40 {
        long_text.push_str(&format!("n{i}|-n{}\n", i + 1));
    }
    long_text.push_str("n40|-\n");
    let big = permanent_graph(&long_text, &[]).create_visible_graph(Options::DEFAULT, None, None);
    let stale: Vec<GraphPrintElement> = [1, 20, 39]
        .iter()
        .flat_map(|&row| big.get_row_info(row).print_elements())
        .collect();

    let heads = [
        None,
        Some(set(&[])),
        Some(set(&["b1"])),
        Some(set(&["m1", "f1"])),
        Some(set(&["zz", "x", "y", "g1"])),
    ];
    let matched = [
        None,
        Some(set(&[])),
        Some(set(&["m0", "f0", "m3", "b0"])),
        Some(set(&["zz", "x", "g1", "m4"])),
    ];
    for options in options() {
        for heads in &heads {
            for matched in &matched {
                let mut visible =
                    graph.create_visible_graph(options, heads.as_ref(), matched.as_ref());
                let count = visible.visible_commit_count();
                if matched.as_ref().is_some_and(HashSet::is_empty) {
                    assert_eq!(count, 0, "{options:?} {heads:?} {matched:?}");
                }
                exercise(&mut visible, &stale);
            }
        }
    }
}

#[test]
fn an_empty_graph_works_with_every_option() {
    let graph = permanent_graph("", &[]);
    for options in options() {
        let mut visible = graph.create_visible_graph(options, None, None);
        assert_eq!(visible.visible_commit_count(), 0);
        exercise(&mut visible, &[]);
    }
}

#[test]
fn the_bek_map_is_computed_once_and_shared() {
    let graph = permanent_graph(GRAPH, &["m0", "b1"]);
    let first = graph.bek_int_map();
    let bek = graph.create_visible_graph(Options::Base(SortType::Bek), None, None);
    let linear_bek = graph.create_visible_graph(Options::LinearBek, None, None);
    assert!(Arc::ptr_eq(&first, &graph.bek_int_map()));
    assert_eq!(first.size(), 12);
    // the Bek graph shows the permanent rows in BEK order
    for row in 0..bek.visible_commit_count() {
        assert_eq!(
            bek.get_row_info(row).commit(),
            graph
                .all_commits()
                .get(first.get_usual_index(row) as usize)
                .id()
                .clone()
        );
        assert_eq!(
            linear_bek.get_row_info(row).commit(),
            bek.get_row_info(row).commit()
        );
    }
}

/// Commits of the rows whose node is printed selected.
fn selected_commits(visible: &VisibleGraphImpl<String>) -> Vec<String> {
    (0..visible.visible_commit_count())
        .filter(|&row| {
            visible
                .get_row_info(row)
                .print_elements()
                .iter()
                .any(|e| matches!(e, GraphPrintElement::Node(_)) && e.is_selected())
        })
        .map(|row| visible.get_row_info(row).commit())
        .collect()
}

fn dotted_edges(visible: &VisibleGraphImpl<String>) -> usize {
    use ig_log::api::EdgeFilter;
    use ig_log::api::elements::GraphEdgeType;
    let graph = visible.linear_graph();
    (0..graph.nodes_count())
        .flat_map(|row| graph.get_adjacent_edges(row, EdgeFilter::NormalDown))
        .filter(|edge| edge.edge_type == GraphEdgeType::Dotted)
        .count()
}

fn node(visible: &VisibleGraphImpl<String>, commit: &str) -> GraphPrintElement {
    let row = visible.get_visible_row_index(&commit.to_string()).unwrap();
    visible
        .get_row_info(row)
        .print_elements()
        .into_iter()
        .find(|e| matches!(e, GraphPrintElement::Node(_)))
        .unwrap()
}

fn act_node(visible: &mut VisibleGraphImpl<String>, commit: &str, kind: GraphActionType) {
    let element = node(visible, commit);
    act(visible, Some(element), kind);
}

/// The linear BEK example of `LinearBekTest.simpleGraphTest`, through the facade.
#[test]
fn linear_bek_collapses_merges_and_expands_them_on_click() {
    let graph = permanent_graph(
        "
        c0|-c4 c1
        c1|-c2
        c2|-c3
        c3|-c7
        c4|-c5
        c5|-c6
        c6|-c7
        c7|-
        ",
        &["c0"],
    );
    let mut visible = graph.create_visible_graph(Options::LinearBek, None, None);
    assert_eq!(
        format!("{visible:?}"),
        r#"VisibleGraph["c0", "c1", "c2", "c3", "c4", "c5", "c6", "c7"]"#
    );
    assert_eq!(dotted_edges(&visible), 1);

    // hovering the upper node of a dotted edge highlights the edge's ends
    act_node(&mut visible, "c3", GraphActionType::MouseOver);
    assert_eq!(selected_commits(&visible), ["c3", "c4"]);

    // a click on it expands the merge
    act_node(&mut visible, "c3", GraphActionType::MouseClick);
    assert_eq!(dotted_edges(&visible), 0);

    // hovering a commit of an expanded merge highlights the merge's fragment
    act_node(&mut visible, "c2", GraphActionType::MouseOver);
    assert_eq!(selected_commits(&visible), ["c0", "c1", "c2", "c3", "c4"]);

    // a click on it collapses the merge again
    act_node(&mut visible, "c2", GraphActionType::MouseClick);
    assert_eq!(dotted_edges(&visible), 1);

    act(&mut visible, None, GraphActionType::ButtonExpand);
    assert_eq!(dotted_edges(&visible), 0);
    act(&mut visible, None, GraphActionType::ButtonCollapse);
    assert_eq!(dotted_edges(&visible), 1);
}

#[test]
fn simple_graph_info_copies_the_rows_around_a_row() {
    let graph = permanent_graph(GRAPH, &["m0", "b1"]);
    let visible = graph.create_visible_graph(Options::DEFAULT, None, None);
    let info = visible.build_simple_graph_info(3, 2);
    let commits = info.permanent_commits_info();
    // rows 1..5
    let ids: Vec<String> = (0..4).map(|i| commits.get_commit_id(i)).collect();
    let expected: Vec<String> = (1..5).map(|r| visible.get_row_info(r).commit()).collect();
    assert_eq!(ids, expected);
    assert_eq!(info.linear_graph().nodes_count(), 4);
    assert_eq!(commits.get_node_id(&expected[2]), 2);
    assert_eq!(commits.get_node_id(&"zz".to_string()), -1);
    assert!(
        !info
            .permanent_graph_layout()
            .get_head_node_index()
            .is_empty()
    );
}

/// IdeaGit: a huge `visible_range` must not overflow (Java wraps; the clamping makes saturation
/// give the whole graph).
#[test]
fn simple_graph_info_does_not_overflow_for_a_huge_visible_range() {
    let graph = permanent_graph(GRAPH, &["m0", "b1"]);
    let visible = graph.create_visible_graph(Options::DEFAULT, None, None);
    let count = visible.visible_commit_count();
    for visible_row in [-5, 0, 3, count, i32::MAX] {
        for visible_range in [i32::MAX, i32::MAX / 2 + 1] {
            let info = visible.build_simple_graph_info(visible_row, visible_range);
            assert!(info.linear_graph().nodes_count() <= count);
        }
    }
    let info = visible.build_simple_graph_info(3, i32::MAX);
    assert_eq!(info.linear_graph().nodes_count(), count);
    let info = visible.build_simple_graph_info(-5, i32::MAX);
    assert_eq!(info.linear_graph().nodes_count(), count);
}

// Expected rows below: Normal keeps the input order; BEK moves the merged branch `f` directly below
// the merge `m`; LinearBek has the BEK order; filters keep the matched commits reachable from the
// visible heads in the delegate order; FirstParent follows first parents only (`f` is dropped).
/// Commits: a merge `m` of `a` and `f`, a second branch tip `x`, and a common base `b`, `c`.
const OPTIONS_GRAPH: &str = "
m|-a f
x|-c
a|-b
f|-b
b|-c
c|-
";

fn visible_rows(visible: &VisibleGraphImpl<String>) -> Vec<String> {
    (0..visible.visible_commit_count())
        .map(|row| visible.get_row_info(row).commit())
        .collect()
}

/// IdeaGit: exact visible rows and collapse support for the option and filter combinations of
/// `PermanentGraphImpl::createFilteredController`. Collapse / expand are unsupported whenever the
/// top controller is a `FilteredController` (any commit filter, and first parent), supported
/// otherwise.
#[test]
fn option_and_filter_combinations_show_the_expected_rows() {
    let graph = permanent_graph(OPTIONS_GRAPH, &["m", "x"]);
    let bek = Options::Base(SortType::Bek);
    #[allow(clippy::type_complexity)]
    let table: Vec<(
        &str,
        Options,
        Option<HashSet<String>>,
        Option<HashSet<String>>,
        &[&str],
        bool,
    )> = vec![
        (
            "bek + commit filter",
            bek,
            None,
            Some(set(&["m", "a", "b", "c"])),
            &["m", "a", "b", "c"],
            false,
        ),
        (
            "linear bek + commit filter",
            Options::LinearBek,
            None,
            Some(set(&["m", "a", "b", "c"])),
            &["m", "a", "b", "c"],
            false,
        ),
        (
            "first parent + commit filter",
            Options::FirstParent,
            None,
            Some(set(&["m", "x", "a", "b", "c"])),
            &["m", "x", "a", "b", "c"],
            false,
        ),
        (
            "normal + branch + commit filter",
            Options::DEFAULT,
            Some(set(&["m"])),
            Some(set(&["m", "x", "a", "f", "c"])),
            &["m", "a", "f", "c"],
            false,
        ),
        (
            "bek + branch + commit filter",
            bek,
            Some(set(&["m"])),
            Some(set(&["m", "x", "a", "f", "c"])),
            &["m", "f", "a", "c"],
            false,
        ),
        (
            "linear bek + branch + commit filter",
            Options::LinearBek,
            Some(set(&["m"])),
            Some(set(&["m", "x", "a", "f", "c"])),
            &["m", "f", "a", "c"],
            false,
        ),
        (
            "first parent + branch + commit filter",
            Options::FirstParent,
            Some(set(&["m"])),
            Some(set(&["m", "x", "a", "f", "c"])),
            &["m", "a", "c"],
            false,
        ),
        (
            "first parent alone",
            Options::FirstParent,
            None,
            None,
            &["m", "x", "a", "b", "c"],
            false,
        ),
        (
            "linear bek + branch filter",
            Options::LinearBek,
            Some(set(&["m"])),
            None,
            &["m", "f", "a", "b", "c"],
            true,
        ),
        (
            "normal alone",
            Options::DEFAULT,
            None,
            None,
            &["m", "x", "a", "f", "b", "c"],
            true,
        ),
        (
            "bek alone",
            bek,
            None,
            None,
            &["m", "f", "x", "a", "b", "c"],
            true,
        ),
        (
            "linear bek alone",
            Options::LinearBek,
            None,
            None,
            &["m", "f", "x", "a", "b", "c"],
            true,
        ),
    ];
    for (name, options, heads, matched, expected, collapse) in table {
        let mut visible = graph.create_visible_graph(options, heads.as_ref(), matched.as_ref());
        assert_eq!(visible_rows(&visible), expected, "{name}");
        let controller = visible.action_controller();
        for kind in [
            GraphActionType::ButtonCollapse,
            GraphActionType::ButtonExpand,
        ] {
            assert_eq!(
                controller.is_action_supported(&GraphAction::new(None, kind)),
                collapse,
                "{name} {kind:?}"
            );
        }
    }
}
