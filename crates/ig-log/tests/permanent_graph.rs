//! IdeaGit tests for the facade (`PermanentGraphImpl`, `VisibleGraphImpl`).
//! Upstream has no direct tests for this path; expected values follow from the upstream algorithms
//! (and from the ported `containingBranches` fixtures).

use std::collections::HashSet;
use std::sync::Arc;

use ig_log::actions::{Cursor, GraphAction, GraphActionType};
use ig_log::api::printer::GraphPrintElement;
use ig_log::facade::permanent_graph_impl::PermanentGraphImpl;
use ig_log::graph_color_manager::GraphColorManager;
use ig_log::graph_commit::{GraphCommit, GraphCommitImpl};
use ig_log::permanent_graph::{Options, PermanentGraph, SortType};
use ig_log::print::graph_color_getter::{
    GraphColorGetterByHeadFactory, GraphColorGetterByNodeFactory,
};
use ig_log::print_element::{EdgePrintElement, EdgePrintElementType, PrintElement};
use ig_log::visible_graph::{RowType, VisibleGraph};

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

/// The `containingBranches/notFullGraph` graph: `a6` and `a5` are not loaded.
const NOT_FULL_GRAPH: &str = "
a0|-a1 a6 a3
a1|-a6
a2|-a3
a3|-
a4|-a5
";

#[test]
fn all_commits_round_trip_including_not_loaded_parents() {
    let graph = permanent_graph(NOT_FULL_GRAPH, &[]);
    let original = commits(NOT_FULL_GRAPH);
    let all = graph.all_commits();
    assert_eq!(all.len(), original.len());
    for (actual, expected) in all.iter().zip(&original) {
        assert_eq!(actual.id(), expected.id());
        assert_eq!(actual.parents(), expected.parents());
        assert_eq!(actual.timestamp(), expected.timestamp());
    }
}

/// Upstream filled the first free edge slot with the not-loaded id, swapping parents `p` and `q`
/// of `b` here (`q` is processed first because its first child `a` comes earlier).
#[test]
fn not_loaded_parents_keep_their_order() {
    let text = "
a|-q
b|-p q
";
    let graph = permanent_graph(text, &[]);
    let all = graph.all_commits();
    assert_eq!(all.get(0).parents(), ["q".to_string()]);
    assert_eq!(all.get(1).parents(), ["p".to_string(), "q".to_string()]);
}

#[test]
fn children_and_containing_branches() {
    let graph = permanent_graph(NOT_FULL_GRAPH, &["a0", "a2", "a5"]);
    assert_eq!(
        graph.get_children(&"a3".to_string()),
        ["a2".to_string(), "a0".to_string()]
    );
    assert!(graph.get_children(&"a0".to_string()).is_empty());

    // same as the containingBranches/notFullGraph fixture (branch nodes 0 2 5 -> a0, a2)
    assert_eq!(
        graph.get_containing_branches(&"a1".to_string()),
        set(&["a0"])
    );
    assert_eq!(
        graph.get_containing_branches(&"a3".to_string()),
        set(&["a0", "a2"])
    );
    assert_eq!(graph.get_containing_branches(&"a4".to_string()), set(&[]));
    // unknown and not-loaded commits are skipped by the walk
    assert_eq!(graph.get_containing_branches(&"zz".to_string()), set(&[]));
    assert_eq!(graph.get_containing_branches(&"a6".to_string()), set(&[]));

    let condition = graph.get_contained_in_branch_condition(&["a2".to_string()]);
    assert!(condition.test(&"a2".to_string()));
    assert!(condition.test(&"a3".to_string()));
    assert!(!condition.test(&"a0".to_string()));
    assert!(!condition.test(&"a6".to_string()));
}

#[test]
fn visible_graph_normal_rows() {
    let graph = permanent_graph(NOT_FULL_GRAPH, &["a0", "a2"]);
    let visible = graph.create_visible_graph(Options::DEFAULT, None, None);

    assert_eq!(visible.visible_commit_count(), 5);
    assert_eq!(
        format!("{visible:?}"),
        r#"VisibleGraph["a0", "a1", "a2", "a3", "a4"]"#
    );
    let row = visible.get_row_info(3);
    assert_eq!(row.commit(), "a3");
    assert_eq!(row.one_of_heads(), "a0");
    assert_eq!(row.row_type(), RowType::Normal);
    assert_eq!(row.adjacent_rows(false), [2, 0]);
    assert!(row.adjacent_rows(true).is_empty());
    assert_eq!(visible.get_row_info(0).adjacent_rows(true), [1, 3]);
    assert_eq!(visible.get_visible_row_index(&"a4".to_string()), Some(4));
    assert_eq!(visible.get_visible_row_index(&"a6".to_string()), None);
    assert_eq!(visible.get_visible_row_index(&"zz".to_string()), None);
}

#[test]
fn visible_graph_print_elements_and_colors() {
    struct ColorByHead;
    impl GraphColorManager<String> for ColorByHead {
        fn get_color(
            &self,
            head_commit: &String,
            head_fragment_index: i32,
            fragment_index: i32,
        ) -> i32 {
            let head = i32::from_str_radix(head_commit, 16).unwrap();
            head * 10000 + head_fragment_index * 100 + fragment_index
        }
    }
    let factory = GraphColorGetterByHeadFactory::new(Arc::new(ColorByHead));
    let graph = PermanentGraphImpl::new_instance(
        &commits(NOT_FULL_GRAPH),
        &factory,
        |a: &String, b: &String| a.cmp(b),
        &set(&["a0", "a2"]),
    );
    let visible = graph.create_visible_graph_sorted(SortType::Normal, None, None);

    // row 0: node a0 with its down edges to a1 and a3 and to the not-loaded a6
    let elements = visible.get_row_info(0).print_elements();
    let node = elements.last().unwrap();
    assert!(matches!(node, GraphPrintElement::Node(_)));
    assert_eq!(node.position_in_current_row(), 0);
    // a0 is the head of layout index 1
    assert_eq!(node.color_id(), 0xa0 * 10000 + 100 + 1);
    let down_edges = elements
        .iter()
        .filter(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.edge_print_type() == EdgePrintElementType::Down))
        .count();
    assert_eq!(down_edges, 3);
    assert!(elements.iter().all(|e| !e.is_selected()));
    assert!(visible.recommended_width() >= 1);
}

#[test]
fn long_edge_arrow_actions() {
    // n0 -> n1 -> ... -> n40 and a long edge n0 -> n40
    let mut text = String::from("n0|-n1 n40\n");
    for i in 1..40 {
        text.push_str(&format!("n{i}|-n{}\n", i + 1));
    }
    text.push_str("n40|-\n");
    let graph = permanent_graph(&text, &[]);
    let mut visible = graph.create_visible_graph(Options::DEFAULT, None, None);

    // long edges are hidden by default: at row 1 the edge ends in a terminal down arrow
    assert!(visible.action_controller().are_long_edges_hidden());
    let arrow = visible
        .get_row_info(1)
        .print_elements()
        .into_iter()
        .find(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.has_arrow()))
        .expect("arrow in row 1");
    let GraphPrintElement::Edge(edge) = &arrow else {
        unreachable!()
    };
    assert_eq!(edge.edge_print_type(), EdgePrintElementType::Down);
    assert_eq!(edge.position_in_other_row(), edge.position_in_current_row());

    let hover = visible.action_controller().perform_action(GraphAction::new(
        Some(arrow.clone()),
        GraphActionType::MouseOver,
    ));
    assert_eq!(hover.commit_to_jump.as_deref(), Some("n40"));
    assert_eq!(hover.cursor_to_set, Some(Cursor::Hand));
    assert!(!hover.do_jump);
    assert!(hover.is_repaint_required);
    // the hovered arrow is now selected
    assert!(
        visible
            .get_row_info(1)
            .print_elements()
            .iter()
            .any(|e| *e == arrow && e.is_selected())
    );

    let click = visible
        .action_controller()
        .perform_action(GraphAction::new(Some(arrow), GraphActionType::MouseClick));
    assert_eq!(click.commit_to_jump.as_deref(), Some("n40"));
    assert_eq!(click.cursor_to_set, Some(Cursor::Default));
    assert!(click.do_jump);

    // hidden long edges are not drawn in the middle; shown ones are (with arrows next to the ends)
    let long_edge_parts = |visible: &ig_log::facade::visible_graph_impl::VisibleGraphImpl<
        String,
    >,
                           row| {
        visible
            .get_row_info(row)
            .print_elements()
            .iter()
            .filter(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.graph_edge().down_node_index == Some(40) && edge.graph_edge().up_node_index == Some(0)))
            .count()
    };
    assert_eq!(long_edge_parts(&visible, 20), 0);
    visible.action_controller().set_long_edges_hidden(false);
    assert_eq!(long_edge_parts(&visible, 20), 2);
    assert!(
        visible
            .get_row_info(1)
            .print_elements()
            .iter()
            .any(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.has_arrow()))
    );

    // a click on a node collapses its linear fragment (CollapsedController): n2..=n38 are hidden
    // behind a dotted edge n1 - n39; nothing to jump to
    let node = visible.get_row_info(5).print_elements().pop().unwrap();
    let answer = visible
        .action_controller()
        .perform_action(GraphAction::new(Some(node), GraphActionType::MouseClick));
    assert_eq!(answer.cursor_to_set, None);
    assert!(answer.commit_to_jump.is_none());
    assert_eq!(
        format!("{visible:?}"),
        r#"VisibleGraph["n0", "n1", "n39", "n40"]"#
    );
}

#[test]
fn every_option_is_supported() {
    let graph = permanent_graph(NOT_FULL_GRAPH, &[]);
    let heads = set(&["a2"]);
    let shown = |options, heads: Option<&HashSet<String>>, matched: Option<&HashSet<String>>| {
        format!("{:?}", graph.create_visible_graph(options, heads, matched))
    };
    assert_eq!(
        shown(Options::Base(SortType::Bek), None, None),
        r#"VisibleGraph["a2", "a0", "a3", "a1", "a4"]"#
    );
    assert_eq!(
        shown(Options::LinearBek, None, None),
        r#"VisibleGraph["a2", "a0", "a3", "a1", "a4"]"#
    );
    assert_eq!(
        shown(Options::FirstParent, None, None),
        r#"VisibleGraph["a0", "a1", "a2", "a3", "a4"]"#
    );
    assert_eq!(
        shown(Options::DEFAULT, Some(&heads), None),
        r#"VisibleGraph["a2", "a3"]"#
    );
    assert_eq!(
        shown(Options::DEFAULT, None, Some(&set(&["a0", "a3", "a4"]))),
        r#"VisibleGraph["a0", "a3", "a4"]"#
    );
    assert_eq!(
        shown(Options::LinearBek, Some(&heads), None),
        r#"VisibleGraph["a2", "a3"]"#
    );
    assert_eq!(
        shown(Options::FirstParent, Some(&heads), None),
        r#"VisibleGraph["a2", "a3"]"#
    );
}

#[test]
fn empty_graph() {
    let graph = permanent_graph("", &[]);
    assert!(graph.all_commits().is_empty());
    let visible = graph.create_visible_graph(Options::DEFAULT, None, None);
    assert_eq!(visible.visible_commit_count(), 0);
    assert_eq!(visible.recommended_width(), 0);
}
