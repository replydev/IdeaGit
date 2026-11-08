//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/EdgesInRowTest.java`
//! (fixtures `edgesInRow/`).

mod common;

use std::sync::Arc;

use common::do_test;
use common::parser::parse_linear_graph;
use common::str_utils::edges_in_row_to_str;
use ig_log::api::linear_graph::LinearGraph;
use ig_log::print::edges_in_row_generator::EdgesInRowGenerator;

fn run(test_name: &str) {
    do_test("edgesInRow", test_name, |input, out| {
        let graph: Arc<dyn LinearGraph> = Arc::new(parse_linear_graph(input));
        let edges_in_row_generator = EdgesInRowGenerator::new(graph.clone());
        assert_eq!(
            out,
            edges_in_row_to_str(&edges_in_row_generator, graph.nodes_count())
        );
    });
}

#[test]
fn simple() {
    run("simple");
}

#[test]
fn many_nodes() {
    run("manyNodes");
}

#[test]
fn many_up_nodes() {
    run("manyUpNodes");
}

#[test]
fn many_down_nodes() {
    run("manyDownNodes");
}

#[test]
fn one_node() {
    run("oneNode");
}

#[test]
fn one_node_not_full_graph() {
    run("oneNodeNotFullGraph");
}

#[test]
fn not_full_graph() {
    run("notFullGraph");
}

#[test]
fn long_graph() {
    run("longGraph");
}

#[test]
fn not_load_node() {
    run("notLoadNode");
}
