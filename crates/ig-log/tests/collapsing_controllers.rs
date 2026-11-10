//! IdeaGit tests for `CollapsedController`, `FilteredController` and `GraphModificationUtil` behind
//! a `VisibleGraphImpl`: fragment hover, collapse / expand on click and with the buttons, and hiding
//! commits. Upstream has no tests for these; expected values follow from the upstream algorithms.

mod common;

use std::collections::HashSet;
use std::sync::Arc;

use common::test_graph_builder::{TestPermanentGraphInfo, as_test_graph_string, test_graph};
use ig_log::actions::{Cursor, GraphAction, GraphActionType};
use ig_log::api::elements::GraphElement;
use ig_log::api::permanent::PermanentGraphInfo;
use ig_log::api::printer::{GraphColorGetter, GraphPrintElement};
use ig_log::collapsing::collapsed_controller::CollapsedController;
use ig_log::facade::base_controller::BaseController;
use ig_log::facade::filtered_controller::FilteredController;
use ig_log::facade::graph_modification_util::hide_commits;
use ig_log::facade::linear_graph_controller::LinearGraphController;
use ig_log::facade::visible_graph_impl::VisibleGraphImpl;
use ig_log::visible_graph::VisibleGraph;

struct NoColors;

impl GraphColorGetter for NoColors {
    fn get_node_color(&self, _node_id: i32, _layout_index: i32) -> i32 {
        0
    }
}

/*
0
1
|\
2 |
3 |
4 |
| 5
|/
6
7
*/
fn info(branch_nodes: &[i32]) -> Arc<TestPermanentGraphInfo> {
    Arc::new(TestPermanentGraphInfo::new(
        test_graph! {
            0(1)
            1(2, 5)
            2(3)
            3(4)
            4(6)
            5(6)
            6(7)
            7()
        },
        &[],
        branch_nodes.iter().copied().collect(),
    ))
}

fn visible(
    info: &Arc<TestPermanentGraphInfo>,
    controller: impl FnOnce(Arc<dyn PermanentGraphInfo<i32>>) -> Box<dyn LinearGraphController>,
) -> VisibleGraphImpl<i32> {
    let info: Arc<dyn PermanentGraphInfo<i32>> = info.clone();
    VisibleGraphImpl::new(controller(info.clone()), info, Arc::new(NoColors))
}

fn collapsed(info: &Arc<TestPermanentGraphInfo>) -> VisibleGraphImpl<i32> {
    visible(info, |info| {
        Box::new(CollapsedController::new(
            Box::new(BaseController::new(info.clone())),
            info,
            None,
        ))
    })
}

fn node(visible: &VisibleGraphImpl<i32>, row: i32) -> GraphPrintElement {
    visible
        .get_row_info(row)
        .print_elements()
        .into_iter()
        .find(|e| matches!(e, GraphPrintElement::Node(_)))
        .unwrap()
}

fn act_node(
    visible: &mut VisibleGraphImpl<i32>,
    row: i32,
    kind: GraphActionType,
) -> ig_log::actions::GraphAnswer<i32> {
    let element = node(visible, row);
    act(visible, Some(element), kind)
}

fn act(
    visible: &mut VisibleGraphImpl<i32>,
    element: Option<GraphPrintElement>,
    kind: GraphActionType,
) -> ig_log::actions::GraphAnswer<i32> {
    visible
        .action_controller()
        .perform_action(GraphAction::new(element, kind))
}

/// Commits of the rows whose node is printed selected.
fn selected_rows(visible: &VisibleGraphImpl<i32>) -> Vec<i32> {
    use ig_log::print_element::PrintElement as _;
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

fn graph_string(visible: &VisibleGraphImpl<i32>) -> String {
    as_test_graph_string(&*visible.linear_graph(), true)
}

#[test]
fn hovering_a_node_selects_its_linear_fragment() {
    let info = info(&[0]);
    let mut visible = collapsed(&info);
    let answer = act_node(&mut visible, 3, GraphActionType::MouseOver);
    assert_eq!(answer.cursor_to_set, Some(Cursor::Hand));
    assert!(answer.is_repaint_required);
    assert!(answer.graph_updater.is_none());
    assert_eq!(selected_rows(&visible), [2, 3, 4]);

    // hovering nothing clears the selection
    let answer = act(&mut visible, None, GraphActionType::MouseOver);
    assert!(answer.is_repaint_required);
    assert!(selected_rows(&visible).is_empty());
}

#[test]
fn clicking_collapses_and_expands_a_fragment() {
    let info = info(&[0]);
    let mut visible = collapsed(&info);
    let before = graph_string(&visible);

    act_node(&mut visible, 3, GraphActionType::MouseClick);
    assert_eq!(
        graph_string(&visible),
        "0(1)\n1(5, 2)\n2(4.dot)\n4(6)\n5(6)\n6(7)\n7()\n"
    );

    // hovering the collapsed edge's upper node selects the ends of the collapsed fragment
    act_node(&mut visible, 2, GraphActionType::MouseOver);
    assert_eq!(selected_rows(&visible), [2, 4]);

    act_node(&mut visible, 2, GraphActionType::MouseClick);
    assert_eq!(graph_string(&visible), before);
}

#[test]
fn collapse_all_and_expand_all_run_through_the_updater() {
    let info = info(&[0]);
    let mut visible = collapsed(&info);
    let before = graph_string(&visible);

    let answer = act(&mut visible, None, GraphActionType::ButtonCollapse);
    // nothing changes before the updater runs
    assert_eq!(graph_string(&visible), before);
    visible.run_graph_updater(answer.graph_updater.expect("collapse all updater"));
    assert_eq!(graph_string(&visible), "0(7.dot)\n7()\n");

    let answer = act(&mut visible, None, GraphActionType::ButtonExpand);
    visible.run_graph_updater(answer.graph_updater.expect("expand all updater"));
    assert_eq!(graph_string(&visible), before);

    // an updater that is never run does not block later modifications
    drop(act(&mut visible, None, GraphActionType::ButtonCollapse));
    act_node(&mut visible, 3, GraphActionType::MouseClick);
    assert_eq!(visible.visible_commit_count(), 7);
}

#[test]
fn visible_branches_hide_the_other_branches() {
    // only what is reachable from 5
    let info = info(&[0]);
    let mut visible = visible(&info, |info| {
        Box::new(CollapsedController::new(
            Box::new(BaseController::new(info.clone())),
            info,
            Some(&HashSet::from([5])),
        ))
    });
    assert_eq!(graph_string(&visible), "5(6)\n6(7)\n7()\n");
    // unknown and not-loaded heads are skipped
    let skipped = visible_with_heads(&info, &[-2, 5]);
    assert_eq!(graph_string(&skipped), "5(6)\n6(7)\n7()\n");
    // no-panic check: the hover's answer is not asserted
    act_node(&mut visible, 0, GraphActionType::MouseOver);
}

fn visible_with_heads(info: &Arc<TestPermanentGraphInfo>, heads: &[i32]) -> VisibleGraphImpl<i32> {
    let heads: HashSet<i32> = heads.iter().copied().collect();
    visible(info, |info| {
        Box::new(CollapsedController::new(
            Box::new(BaseController::new(info.clone())),
            info,
            Some(&heads),
        ))
    })
}

#[test]
fn filtered_graph_ignores_actions_and_hides_commits() {
    let info = info(&[0]);
    let matched: HashSet<i32> = [0, 2, 4, 5, 7].into_iter().collect();
    let mut visible = visible(&info, |info| {
        Box::new(FilteredController::create(
            Box::new(BaseController::new(info.clone())),
            &*info,
            &matched,
            None,
        ))
    });
    assert_eq!(
        graph_string(&visible),
        "0(5.dot, 2.dot)\n2(4.dot)\n4(7.dot)\n5(7.dot)\n7()\n"
    );
    let answer = act_node(&mut visible, 1, GraphActionType::MouseClick);
    assert_eq!(answer.cursor_to_set, Some(Cursor::Default));

    let info_dyn: Arc<dyn PermanentGraphInfo<i32>> = info.clone();
    let mut controller = FilteredController::create(
        Box::new(BaseController::new(info_dyn.clone())),
        &*info_dyn,
        &matched,
        None,
    );
    assert!(hide_commits(
        &mut controller,
        &*info_dyn,
        &HashSet::from([2, 4])
    ));
    assert_eq!(
        as_test_graph_string(&*controller.compiled_graph(), true),
        "0(7.dot, 5.dot)\n5(7.dot)\n7()\n"
    );

    // no filtered controller in the chain
    let mut base = BaseController::new(info_dyn.clone());
    assert!(!hide_commits(&mut base, &*info_dyn, &HashSet::from([2])));
}

#[test]
fn stale_elements_out_of_range_act_as_no_element() {
    let info = info(&[0]);
    let big = collapsed(&info);
    let stale = node(&big, 7);
    let mut small = visible(&info, |info| {
        Box::new(CollapsedController::new(
            Box::new(BaseController::new(info.clone())),
            info,
            Some(&HashSet::from([6])),
        ))
    });
    assert_eq!(small.visible_commit_count(), 2);
    for kind in [GraphActionType::MouseOver, GraphActionType::MouseClick] {
        let answer = act(&mut small, Some(stale.clone()), kind);
        assert_eq!(answer.cursor_to_set, Some(Cursor::Default));
        assert!(answer.graph_updater.is_none());
    }
    assert!(matches!(stale.graph_element(), GraphElement::Node(_)));
}

/// IdeaGit: an edge element whose node indexes are out of range acts as no element too (the node
/// case is covered above).
#[test]
fn stale_edge_elements_out_of_range_act_as_no_element() {
    use ig_log::print_element::PrintElement as _;
    let info = info(&[0]);
    let big = collapsed(&info);
    // the edge 1 -> 5 of the big graph, whose rows do not exist in the small one
    let stale = (0..big.visible_commit_count())
        .flat_map(|row| big.get_row_info(row).print_elements())
        .find(|e| matches!(e, GraphPrintElement::Edge(_)) && e.row_index() >= 5)
        .expect("an edge printed at a high row");
    let mut small = visible(&info, |info| {
        Box::new(CollapsedController::new(
            Box::new(BaseController::new(info.clone())),
            info,
            Some(&HashSet::from([6])),
        ))
    });
    assert_eq!(small.visible_commit_count(), 2);
    for kind in [GraphActionType::MouseOver, GraphActionType::MouseClick] {
        let answer = act(&mut small, Some(stale.clone()), kind);
        assert_eq!(answer.cursor_to_set, Some(Cursor::Default));
        assert!(answer.graph_updater.is_none());
    }
    assert!(matches!(stale.graph_element(), GraphElement::Edge(_)));
}

mod branch_filter_over_linear_bek {
    use ig_log::api::EdgeFilter;
    use ig_log::api::elements::GraphEdgeType;
    use ig_log::facade::permanent_graph_impl::PermanentGraphImpl;
    use ig_log::graph_commit::GraphCommitImpl;
    use ig_log::permanent_graph::{Options, PermanentGraph};
    use ig_log::print::graph_color_getter::GraphColorGetterByNodeFactory;

    use super::*;

    fn rows(visible: &VisibleGraphImpl<String>) -> Vec<String> {
        (0..visible.visible_commit_count())
            .map(|row| visible.get_row_info(row).commit())
            .collect()
    }

    fn dotted_edges(visible: &VisibleGraphImpl<String>) -> usize {
        let graph = visible.linear_graph();
        (0..graph.nodes_count())
            .flat_map(|row| graph.get_adjacent_edges(row, EdgeFilter::NormalDown))
            .filter(|edge| edge.edge_type == GraphEdgeType::Dotted)
            .count()
    }

    fn click(visible: &mut VisibleGraphImpl<String>, commit: &str) {
        let row = visible.get_visible_row_index(&commit.to_string()).unwrap();
        let element = visible
            .get_row_info(row)
            .print_elements()
            .into_iter()
            .find(|e| matches!(e, GraphPrintElement::Node(_)))
            .unwrap();
        let answer = visible
            .action_controller()
            .perform_action(GraphAction::new(Some(element), GraphActionType::MouseClick));
        if let Some(updater) = answer.graph_updater {
            visible.run_graph_updater(updater);
        }
    }

    /// IdeaGit: `BranchFilterController` converts the element of its compiled graph to the linear
    /// BEK delegate, and rebuilds its graph when the delegate changed (`delegate_graph_changed`).
    #[test]
    fn a_click_collapses_and_expands_a_linear_bek_fragment_through_the_branch_filter() {
        // the example of `LinearBekTest.simpleGraphTest`, plus the branch `z` that is filtered out
        let text = "
            z|-c7
            c0|-c4 c1
            c1|-c2
            c2|-c3
            c3|-c7
            c4|-c5
            c5|-c6
            c6|-c7
            c7|-
        ";
        let count = text.lines().filter(|l| !l.trim().is_empty()).count() as i64;
        let commits: Vec<_> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .enumerate()
            .map(|(i, line)| {
                let (id, parents) = line.split_once("|-").unwrap();
                let parents = parents.split_whitespace().map(str::to_string).collect();
                GraphCommitImpl::create_commit(id.to_string(), parents, count - i as i64)
            })
            .collect();
        let factory = GraphColorGetterByNodeFactory::new(|_: &String, layout_index| layout_index);
        let branches: HashSet<String> = ["z", "c0"].iter().map(|s| s.to_string()).collect();
        let graph = PermanentGraphImpl::new_instance(
            &commits,
            &factory,
            |a: &String, b: &String| a.cmp(b),
            &branches,
        );
        let heads: HashSet<String> = HashSet::from(["c0".to_string()]);
        let mut visible = graph.create_visible_graph(Options::LinearBek, Some(&heads), None);

        let collapsed_rows = rows(&visible);
        assert!(!collapsed_rows.contains(&"z".to_string()));
        assert_eq!(collapsed_rows.len(), 8);
        assert_eq!(dotted_edges(&visible), 1);

        // c3 is the upper end of the dotted edge: a click expands the merge
        click(&mut visible, "c3");
        assert_eq!(dotted_edges(&visible), 0);
        assert_eq!(rows(&visible).len(), 8);
        assert!(!rows(&visible).contains(&"z".to_string()));

        // a click on a commit of the expanded merge collapses it again, restoring the rows
        click(&mut visible, "c2");
        assert_eq!(dotted_edges(&visible), 1);
        assert_eq!(rows(&visible), collapsed_rows);
    }
}
