//! Hiding commits in, and modifying, the collapsed graph of a filtered controller chain.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/GraphModificationUtil.kt`.
//! The `CollapsedGraph` / `Modification` extension functions are free functions.

use std::collections::HashSet;

use crate::api::elements::{GraphEdge, GraphEdgeType};
use crate::api::permanent::PermanentGraphInfo;
use crate::collapsing::collapsed_graph::{CollapsedGraph, Modification};
use crate::collapsing::dotted_filter_edges_generator;
use crate::facade::cascade_controller::perform_action_recursively;
use crate::facade::filtered_controller::FilteredController;
use crate::facade::graph_changes::GraphChanges;
use crate::facade::linear_graph_controller::LinearGraphController;

/// Hides `commits_to_hide` in the first filtered graph of the chain; `false` if there is none.
pub fn hide_commits<CommitId>(
    graph_controller: &mut dyn LinearGraphController,
    permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
    commits_to_hide: &HashSet<CommitId>,
) -> bool {
    modify_graph(graph_controller, |graph| {
        let node_ids = permanent_graph_info
            .permanent_commits_info()
            .convert_to_node_ids(commits_to_hide);
        let compiled_graph = graph.compiled_graph();
        let rows_to_hide: HashSet<i32> = node_ids
            .into_iter()
            .filter_map(|it| compiled_graph.get_node_index(it))
            .collect();
        dotted_filter_edges_generator::hide_inplace(graph, &rows_to_hide);
    })
}

/// Runs `consumer` on the collapsed graph of the first [`FilteredController`] of the chain;
/// `false` if there is none.
pub fn modify_graph(
    graph_controller: &mut dyn LinearGraphController,
    mut consumer: impl FnMut(&CollapsedGraph),
) -> bool {
    let result = perform_action_recursively(graph_controller, &mut |cc| {
        let filtered = cc
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<FilteredController>())?;
        consumer(&filtered.collapsed_graph);
        Some(GraphChanges::some_changes())
    });
    result.is_some()
}

/// `CollapsedGraph.modify`: starts a modification, lets `modifier` fill it, and applies it.
pub fn modify(collapsed_graph: &CollapsedGraph, modifier: impl FnOnce(&mut Modification)) {
    let mut modification = collapsed_graph.start_modification();
    modifier(&mut modification);
    modification.apply();
}

/// `Modification.hideRow`: hides the compiled row `row`.
pub fn hide_row(modification: &mut Modification, row: i32) {
    let node_index = modification.convert_to_delegate_node_index(row);
    modification.hide_node(node_index);
}

/// `Modification.connectRows`: a dotted edge between the compiled rows.
pub fn connect_rows(modification: &mut Modification, up_row: i32, down_row: i32) {
    let up = modification.convert_to_delegate_node_index(up_row);
    let down = modification.convert_to_delegate_node_index(down_row);
    modification.create_edge(&GraphEdge::new(
        Some(up),
        Some(down),
        None,
        GraphEdgeType::Dotted,
    ));
}
