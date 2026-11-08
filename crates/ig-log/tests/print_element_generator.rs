//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/PrintElementGeneratorTest.kt`
//! (fixtures `elementGenerator/`).

mod common;

use std::sync::Arc;

use common::parser::parse_linear_graph;
use common::str_utils::print_element_generator_as_string;
use common::{IN_POSTFIX, OUT_POSTFIX, do_test, load_text};
use ig_log::api::GraphLayout;
use ig_log::api::elements::GraphElement;
use ig_log::api::linear_graph::LinearGraph;
use ig_log::api::printer::{GraphPrintElement, PrintElementPresentationManager};
use ig_log::permanent::graph_layout_builder;
use ig_log::print::graph_element_comparator_by_layout_index::GraphElementComparatorByLayoutIndex;
use ig_log::print::print_element_generator_impl::PrintElementGeneratorImpl;
use ig_log::utils::linear_graph_utils::{as_normal_edge, get_not_null_node_index};

const DIRECTORY: &str = "elementGenerator";

/// Nothing selected; a node's color is its index, a normal edge's is `up + down`, a special
/// edge's is its node index.
struct TestPrintElementPresentationManager;

impl PrintElementPresentationManager for TestPrintElementPresentationManager {
    fn is_selected(&self, _print_element: &GraphPrintElement) -> bool {
        false
    }

    fn get_color_id(&self, element: &GraphElement) -> i32 {
        match element {
            GraphElement::Node(node) => node.node_index,
            GraphElement::Edge(edge) => match as_normal_edge(edge) {
                Some(normal_edge) => normal_edge.up + normal_edge.down,
                None => get_not_null_node_index(edge),
            },
        }
    }
}

fn run_test_with_sizes(
    input: &str,
    out: &str,
    long_edge_size: i32,
    visible_part_size: i32,
    edge_with_arrow_size: i32,
) {
    let graph: Arc<dyn LinearGraph> = Arc::new(parse_linear_graph(input));
    let graph_layout = Arc::new(graph_layout_builder::build(&*graph, |o1, o2| o1.cmp(&o2)));
    let graph_element_comparator = GraphElementComparatorByLayoutIndex::new(move |node_index| {
        graph_layout.get_layout_index(node_index)
    });
    let print_element_generator = PrintElementGeneratorImpl::with_sizes(
        graph.clone(),
        TestPrintElementPresentationManager,
        move |a: &GraphElement, b: &GraphElement| graph_element_comparator.ordering(a, b),
        long_edge_size,
        visible_part_size,
        edge_with_arrow_size,
    );
    let actual = print_element_generator_as_string(&print_element_generator, graph.nodes_count());
    assert_eq!(out, actual);
}

fn run(test_name: &str) {
    do_test(DIRECTORY, test_name, |input, out| {
        run_test_with_sizes(input, out, 7, 2, 10)
    });
}

#[test]
fn one_node() {
    run("oneNode");
}

#[test]
fn many_nodes() {
    run("manyNodes");
}

#[test]
fn long_edges() {
    run("longEdges");
}

#[test]
fn special_elements() {
    run("specialElements");
}

// oneUpOneDown tests were created in order to investigate some arrow behavior in upsource

#[test]
fn one_up_one_down1() {
    let test_name = "oneUpOneDown1";
    run_test_with_sizes(
        &load_text(DIRECTORY, &format!("{test_name}{IN_POSTFIX}")),
        &load_text(DIRECTORY, &format!("{test_name}{OUT_POSTFIX}")),
        7,
        1,
        10,
    );
}

#[test]
fn one_up_one_down2() {
    let test_name = "oneUpOneDown2";
    run_test_with_sizes(
        &load_text(DIRECTORY, &format!("{test_name}{IN_POSTFIX}")),
        &load_text(DIRECTORY, &format!("{test_name}{OUT_POSTFIX}")),
        10,
        1,
        10,
    );
}
