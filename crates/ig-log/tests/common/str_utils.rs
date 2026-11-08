//! String renderings of graphs, layouts and print elements used as expected test output.
//!
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/GraphStrUtils.java` and
//! `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/StrUtils.kt`.

use std::collections::{BTreeSet, HashSet};
use std::fmt::Display;

use ig_log::api::EdgeFilter;
use ig_log::api::GraphLayout;
use ig_log::api::elements::{GraphEdge, GraphElement, GraphNode};
use ig_log::api::linear_graph::{LinearGraph, LiteLinearGraph};
use ig_log::api::permanent::PermanentCommitsInfo;
use ig_log::api::printer::{GraphPrintElement, PrintElementGenerator};
use ig_log::facade::reachable_nodes::ReachableNodes;
use ig_log::print::edges_in_row_generator::EdgesInRowGenerator;
use ig_log::print::graph_element_comparator_by_layout_index::GraphElementComparatorByLayoutIndex;
use ig_log::print_element::{EdgePrintElement, PrintElement};
use ig_log::utils::java_sort;

use super::parser::{SEPARATOR, edge_type_to_char, node_type_to_char};

/// `GRAPH_ELEMENT_COMPARATOR`: by layout index, with every layout index 0.
pub fn graph_element_comparator() -> GraphElementComparatorByLayoutIndex {
    GraphElementComparatorByLayoutIndex::new(|_| 0)
}

pub fn commits_info_to_str<CommitId>(
    commits_info: &dyn PermanentCommitsInfo<CommitId>,
    size: usize,
    to_str: impl Fn(&CommitId) -> String,
) -> String {
    let mut s = String::new();
    for i in 0..size as i32 {
        if i != 0 {
            s.push('\n');
        }
        let commit_id = commits_info.get_commit_id(i);
        let commit_index = commits_info.get_node_id(&commit_id);
        let timestamp = commits_info.get_timestamp(i);
        s.push_str(&format!(
            "{commit_index}{SEPARATOR}{}{SEPARATOR}{timestamp}",
            to_str(&commit_id)
        ));
    }
    s
}

pub fn permanent_graph_layout_model_to_str(
    graph_layout: &dyn GraphLayout,
    nodes_count: i32,
) -> String {
    (0..nodes_count)
        .map(|node_index| {
            format!(
                "{}{SEPARATOR}{}",
                graph_layout.get_layout_index(node_index),
                graph_layout.get_one_of_head_node_index(node_index)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn containing_branches_getter_to_str<G: LiteLinearGraph>(
    reachable_nodes: &ReachableNodes<G>,
    branches: &HashSet<i32>,
    nodes_count: i32,
) -> String {
    (0..nodes_count)
        .map(|node_index| {
            let mut branch_node_indexes: Vec<i32> = reachable_nodes
                .get_containing_branches(node_index, branches)
                .into_iter()
                .collect();
            if branch_node_indexes.is_empty() {
                return "none".to_string();
            }
            branch_node_indexes.sort();
            branch_node_indexes
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn edges_in_row_to_str(
    edges_in_row_generator: &EdgesInRowGenerator,
    nodes_count: i32,
) -> String {
    (0..nodes_count)
        .map(|i| edges_to_str(&edges_in_row_generator.get_edges_in_row(i)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Java's `"" + Integer` for a nullable integer.
fn java_int(value: Option<i32>) -> String {
    value.map_or_else(|| "null".to_string(), |v| v.to_string())
}

pub fn edges_to_str(edges: &BTreeSet<GraphEdge>) -> String {
    if edges.is_empty() {
        return "none".to_string();
    }
    let comparator = graph_element_comparator();
    let mut sorted_edges: Vec<GraphElement> =
        edges.iter().map(|e| GraphElement::Edge(*e)).collect();
    java_sort::sort_by(&mut sorted_edges, |a, b| comparator.ordering(a, b));
    sorted_edges
        .iter()
        .map(|element| {
            let GraphElement::Edge(edge) = element else {
                unreachable!()
            };
            format!(
                "{}_{}_{}",
                java_int(edge.up_node_index),
                java_int(edge.down_node_index),
                edge_type_to_char(edge.edge_type)
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// StrUtils.kt

/// `Int?.asString()`.
fn opt_as_string(value: Option<i32>) -> String {
    value.map_or_else(|| "n".to_string(), |v| v.to_string())
}

pub fn node_as_string(node: &GraphNode) -> String {
    format!("{}_{}", node.node_index, node_type_to_char(node.node_type))
}

pub fn edge_as_string(edge: &GraphEdge) -> String {
    format!(
        "{}:{}:{}_{}",
        opt_as_string(edge.up_node_index),
        opt_as_string(edge.down_node_index),
        opt_as_string(edge.target_id),
        edge_type_to_char(edge.edge_type)
    )
}

pub fn element_as_string(element: &GraphElement) -> String {
    match element {
        GraphElement::Node(node) => node_as_string(node),
        GraphElement::Edge(edge) => edge_as_string(edge),
    }
}

/// `LinearGraph.asString(sorted)`.
pub fn graph_as_string(graph: &(impl LinearGraph + ?Sized), sorted: bool) -> String {
    let mut s = String::new();
    for node_index in 0..graph.nodes_count() {
        if node_index > 0 {
            s.push('\n');
        }
        let node = graph.get_graph_node(node_index);
        s.push_str(&node_as_string(&node));
        s.push_str(SEPARATOR);

        let mut adj_edges: Vec<GraphElement> = graph
            .get_adjacent_edges(node_index, EdgeFilter::All)
            .into_iter()
            .map(GraphElement::Edge)
            .collect();
        if sorted {
            let comparator = graph_element_comparator();
            java_sort::sort_by(&mut adj_edges, |a, b| comparator.ordering(a, b));
        }
        s.push_str(&join(adj_edges.iter().map(element_as_string), " "));
    }
    s
}

fn join(items: impl Iterator<Item = impl Display>, separator: &str) -> String {
    items
        .map(|item| item.to_string())
        .collect::<Vec<_>>()
        .join(separator)
}

/// `GraphPrintElement.asString()`.
pub fn print_element_as_string(print_element: &GraphPrintElement) -> String {
    let element = element_as_string(&print_element.graph_element());
    let row = print_element.row_index();
    let color = print_element.color_id();
    let pos = print_element.position_in_current_row();
    let sel = if print_element.is_selected() {
        "Select"
    } else {
        "Unselect"
    };
    match print_element {
        GraphPrintElement::Node(_) => format!("Node|-{row}:{pos}|-{color}:{sel}({element})"),
        GraphPrintElement::Edge(edge) => {
            let t = edge.edge_print_type().name();
            let ls = edge.line_style().name();
            let pos_o = edge.position_in_other_row();
            let arrow = if edge.has_arrow() { "_ARROW" } else { "" };
            format!("Edge:{t}{arrow}:{ls}|-{row}:{pos}:{pos_o}|-{color}:{sel}({element})")
        }
    }
}

/// `PrintElementGenerator.asString(size)`.
pub fn print_element_generator_as_string(
    generator: &dyn PrintElementGenerator,
    size: i32,
) -> String {
    let mut s = String::new();
    for row in 0..size {
        if row > 0 {
            s.push('\n');
        }
        let mut elements = generator.get_print_elements(row);
        // stable, like Kotlin `sortedBy`
        elements.sort_by_key(|element| {
            let pos = element.position_in_current_row();
            match element {
                GraphPrintElement::Node(_) => 1024 * pos,
                GraphPrintElement::Edge(edge) => {
                    1024 * pos
                        + (edge.edge_print_type().ordinal() + 1) * 64
                        + edge.position_in_other_row()
                }
            }
        });
        s.push_str(&join(elements.iter().map(print_element_as_string), "\n  "));
    }
    s
}
