//! IdeaGit behaviour tests for the facade: colors, arrow actions, selection and stale elements.
//! Upstream has no direct tests for this path; expected values follow from the upstream algorithms.

use std::collections::HashSet;
use std::sync::Arc;

use ig_log::actions::{Cursor, GraphAction, GraphActionType};
use ig_log::api::printer::GraphPrintElement;
use ig_log::facade::permanent_graph_impl::PermanentGraphImpl;
use ig_log::facade::visible_graph_impl::VisibleGraphImpl;
use ig_log::graph_color_manager::GraphColorManager;
use ig_log::graph_commit::GraphCommitImpl;
use ig_log::permanent_graph::{Options, PermanentGraph, SortType};
use ig_log::print::graph_color_getter::{
    GraphColorGetterByHeadFactory, GraphColorGetterByNodeFactory,
};
use ig_log::print_element::{EdgePrintElement, EdgePrintElementType, PrintElement};
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

/// The `containingBranches/notFullGraph` graph: `a6` and `a5` are not loaded.
const NOT_FULL_GRAPH: &str = "
a0|-a1 a6 a3
a1|-a6
a2|-a3
a3|-
a4|-a5
";

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

fn colored_graph(text: &str, branches: &[&str]) -> PermanentGraphImpl<String> {
    let factory = GraphColorGetterByHeadFactory::new(Arc::new(ColorByHead));
    PermanentGraphImpl::new_instance(
        &commits(text),
        &factory,
        |a: &String, b: &String| a.cmp(b),
        &set(branches),
    )
}

fn visible(graph: &PermanentGraphImpl<String>) -> VisibleGraphImpl<String> {
    graph
        .create_visible_graph_sorted(SortType::Normal, None, None)
        .unwrap()
}

fn not_full() -> VisibleGraphImpl<String> {
    visible(&colored_graph(NOT_FULL_GRAPH, &["a0", "a2"]))
}

fn long_edge_text() -> String {
    let mut text = String::from("n0|-n1 n40\n");
    for i in 1..40 {
        text.push_str(&format!("n{i}|-n{}\n", i + 1));
    }
    text.push_str("n40|-\n");
    text
}

fn edge_to(visible: &VisibleGraphImpl<String>, row: i32, up: i32, down: i32) -> GraphPrintElement {
    visible
        .get_row_info(row)
        .print_elements()
        .into_iter()
        .find_map(|e| match e {
            GraphPrintElement::Edge(edge)
                if edge.graph_edge().up_node_index == Some(up)
                    && edge.graph_edge().down_node_index == Some(down) =>
            {
                Some(GraphPrintElement::Edge(edge))
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no edge {up}->{down} in row {row}"))
}

fn node_of(visible: &VisibleGraphImpl<String>, row: i32) -> GraphPrintElement {
    visible.get_row_info(row).print_elements().pop().unwrap()
}

fn arrow_in(
    visible: &VisibleGraphImpl<String>,
    row: i32,
    kind: EdgePrintElementType,
) -> GraphPrintElement {
    visible
        .get_row_info(row)
        .print_elements()
        .into_iter()
        .find(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.has_arrow() && edge.edge_print_type() == kind))
        .unwrap_or_else(|| panic!("no {kind:?} arrow in row {row}"))
}

fn act(
    visible: &mut VisibleGraphImpl<String>,
    element: &GraphPrintElement,
    kind: GraphActionType,
) -> ig_log::actions::GraphAnswer<String> {
    visible
        .action_controller()
        .perform_action(GraphAction::new(Some(element.clone()), kind))
}

#[test]
fn node_and_edge_colors() {
    let visible = not_full();
    // layout indices [1,1,3,2,4], heads [0,0,2,0,4]
    assert_eq!(node_of(&visible, 3).color_id(), 0xa0 * 10000 + 100 + 2);
    assert_eq!(node_of(&visible, 2).color_id(), 0xa2 * 10000 + 3 * 100 + 3);
    // an edge takes the color of its lower node's fragment when the lower node is a parent
    // shared by several branches, and of the upper node otherwise
    let a2_a3 = edge_to(&visible, 2, 2, 3);
    assert_eq!(a2_a3.color_id(), 0xa2 * 10000 + 3 * 100 + 3);
    let a0_a3 = edge_to(&visible, 0, 0, 3);
    assert_eq!(a0_a3.color_id(), node_of(&visible, 3).color_id());
}

#[test]
fn hover_up_arrow_of_long_edge_jumps_to_top_node() {
    let graph = permanent_graph(&long_edge_text(), &[]);
    let mut visible = graph
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap();
    let row = (0..41)
        .find(|&r| {
            visible.get_row_info(r).print_elements().iter().any(|e| {
                matches!(e, GraphPrintElement::Edge(edge) if edge.has_arrow() && edge.edge_print_type() == EdgePrintElementType::Up)
            })
        })
        .expect("an Up arrow");
    assert_eq!(row, 39);
    let arrow = arrow_in(&visible, row, EdgePrintElementType::Up);
    let answer = act(&mut visible, &arrow, GraphActionType::MouseOver);
    assert_eq!(answer.commit_to_jump.as_deref(), Some("n0"));
    assert_eq!(answer.cursor_to_set, Some(Cursor::Hand));
    assert!(!answer.do_jump);
    let click = act(&mut visible, &arrow, GraphActionType::MouseClick);
    assert_eq!(click.commit_to_jump.as_deref(), Some("n0"));
    assert!(click.do_jump);
}

#[test]
fn hover_not_loaded_arrow_reports_not_loaded_commit() {
    let mut visible = not_full();
    let arrow = arrow_in(&visible, 1, EdgePrintElementType::Down);
    let GraphPrintElement::Edge(edge) = &arrow else {
        unreachable!()
    };
    assert_eq!(edge.graph_edge().target_id, Some(-2));
    let answer = act(&mut visible, &arrow, GraphActionType::MouseOver);
    assert_eq!(answer.commit_to_jump.as_deref(), Some("a6"));
    assert!(!answer.do_jump);
}

#[test]
fn click_on_arrow_repaints_and_clears_selection() {
    let mut visible = not_full();
    let arrow = arrow_in(&visible, 1, EdgePrintElementType::Down);
    assert!(act(&mut visible, &arrow, GraphActionType::MouseOver).is_repaint_required);
    let click = act(&mut visible, &arrow, GraphActionType::MouseClick);
    assert!(click.is_repaint_required);
    assert!(click.do_jump);
    assert!(
        visible
            .get_row_info(1)
            .print_elements()
            .iter()
            .all(|e| !e.is_selected())
    );
}

#[test]
fn repaint_flag_arrow_node_node() {
    let mut visible = not_full();
    let arrow = arrow_in(&visible, 1, EdgePrintElementType::Down);
    let node = node_of(&visible, 3);
    assert!(act(&mut visible, &arrow, GraphActionType::MouseOver).is_repaint_required);
    assert!(act(&mut visible, &node, GraphActionType::MouseOver).is_repaint_required);
    assert!(!act(&mut visible, &node, GraphActionType::MouseOver).is_repaint_required);
}

#[test]
fn long_edges_hidden_flag() {
    let mut visible = not_full();
    assert!(visible.action_controller().are_long_edges_hidden());
    visible.action_controller().set_long_edges_hidden(false);
    assert!(!visible.action_controller().are_long_edges_hidden());
    visible.action_controller().set_long_edges_hidden(true);
    assert!(visible.action_controller().are_long_edges_hidden());
}

#[test]
fn action_support() {
    let mut visible = not_full();
    let node = node_of(&visible, 0);
    let controller = visible.action_controller();
    let action = |kind| GraphAction::new(Some(node.clone()), kind);
    assert!(!controller.is_action_supported(&action(GraphActionType::ButtonCollapse)));
    assert!(!controller.is_action_supported(&action(GraphActionType::ButtonExpand)));
    assert!(controller.is_action_supported(&action(GraphActionType::MouseOver)));
    assert!(controller.is_action_supported(&action(GraphActionType::MouseClick)));
}

#[test]
fn hover_then_refetch_shows_selection_and_old_snapshot_does_not() {
    // only arrows are selectable on hover: a node hover goes to the BaseController, which
    // selects nothing
    let mut visible = not_full();
    let before = visible.get_row_info(1).print_elements();
    let arrow = arrow_in(&visible, 1, EdgePrintElementType::Down);
    assert!(before.iter().all(|e| !e.is_selected()));
    act(&mut visible, &arrow, GraphActionType::MouseOver);
    assert!(
        before.iter().all(|e| !e.is_selected()),
        "snapshots do not change"
    );
    let after = visible.get_row_info(1).print_elements();
    assert!(after.iter().any(|e| *e == arrow && e.is_selected()));
}

#[test]
fn stale_arrow_elements_are_ignored() {
    // long-edge arrow from a 41-row graph on a 2-row graph
    let big = permanent_graph(&long_edge_text(), &[]);
    let mut big = big
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap();
    let long_arrow = arrow_in(&big, 1, EdgePrintElementType::Down);
    let up_arrow = arrow_in(&big, 39, EdgePrintElementType::Up);
    // not-loaded arrow (target -2) from NOT_FULL_GRAPH on a graph without not-loaded commits
    let not_loaded_arrow = arrow_in(&not_full(), 1, EdgePrintElementType::Down);

    let small = permanent_graph("x0|-x1\nx1|-\n", &[]);
    let mut small = small
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap();
    assert_eq!(small.visible_commit_count(), 2);
    for stale in [&long_arrow, &up_arrow, &not_loaded_arrow] {
        for kind in [GraphActionType::MouseOver, GraphActionType::MouseClick] {
            let answer = act(&mut small, stale, kind);
            assert!(!answer.do_jump);
            assert!(answer.commit_to_jump.is_none());
            assert_eq!(answer.cursor_to_set, Some(Cursor::Default));
        }
    }
    // the small graph still works
    let node = node_of(&small, 0);
    assert_eq!(
        act(&mut small, &node, GraphActionType::MouseClick).cursor_to_set,
        Some(Cursor::Default)
    );
    let _ = &mut big;
}

fn two_row_graph() -> impl VisibleGraph<String> {
    permanent_graph("x0|-x1\nx1|-\n", &[])
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap()
}

#[test]
#[should_panic(expected = "out of range")]
fn get_row_info_negative_row_panics() {
    let graph = two_row_graph();
    let _ = graph.get_row_info(-1);
}

#[test]
#[should_panic(expected = "out of range")]
fn get_row_info_row_equal_to_count_panics() {
    let graph = two_row_graph();
    let _ = graph.get_row_info(graph.visible_commit_count());
}
