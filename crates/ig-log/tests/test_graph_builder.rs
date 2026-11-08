//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/TestGraphBuilderTest.kt`
//! (fixtures `testGraphBuilder/`) and
//! `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/TestGraphBuilderTest.kt`
//! (the `LinearGraphParser` test, fixtures `testParser/`).

mod common;

/// `com.intellij.vcs.log.graph.TestGraphBuilderTest`
mod test_graph_builder_test {
    use super::common::load_text;
    use super::common::str_utils::graph_as_string;
    use super::common::test_graph_builder::*;

    fn run_test(test_name: &str, builder: impl FnOnce(&mut TestGraphBuilder)) {
        let actual = graph_as_string(&graph(builder), false);
        assert_eq!(
            load_text("testGraphBuilder", &format!("{test_name}.txt")),
            actual
        );
    }

    #[test]
    fn simple() {
        run_test("simple", |b| {
            b.node(1, [2]);
            b.node(2, [3]);
            b.node(3, [0; 0]);
        });
    }

    #[test]
    fn simple_many_nodes() {
        run_test("simpleManyNodes", |b| {
            b.node(3, [4, 6, 8]);
            b.node(4, [5, 6, 7, 8]);
            b.node(7, [8, 6]);
            b.node(6, [9, 10, 11]);
            b.node(9, [10, 11]);
            b.node(10, [11]);
            b.node(5, [8, 11]);
            b.node(8, [11]);
            b.node(11, [0; 0]);
        });
    }

    #[test]
    fn special_edges() {
        run_test("specialEdges", |b| {
            b.node(
                1,
                [
                    u(2),
                    dot(3),
                    not_load(Some(100)),
                    down_dot(Some(42)),
                    down_dot(None),
                    not_load(None),
                    up_dot(Some(0)),
                    up_dot(None),
                ],
            );
            b.node(2, [up_dot(None)]);
            b.node(3, [up_dot(Some(0))]);
        });
    }

    #[test]
    fn special_nodes() {
        run_test("specialNodes", |b| {
            b.node(usual(2), [3, 4]);
            b.node(unm(3), [dot(4)]);
            b.node(usual(4), [0; 0]);
            b.node(not_load_node(100), [0; 0]);
        });
    }
}

/// `com.intellij.vcs.log.graph.impl.print.TestGraphBuilderTest` (`LinearGraphParser`)
mod test_parser_test {
    use super::common::do_test;
    use super::common::parser::parse_linear_graph;
    use super::common::str_utils::graph_as_string;

    fn run(test_name: &str) {
        do_test("testParser", test_name, |input, out| {
            let graph = parse_linear_graph(input);
            assert_eq!(out, graph_as_string(&graph, false));
        });
    }

    #[test]
    fn special_elements() {
        run("specialElements");
    }

    #[test]
    fn many_nodes() {
        run("manyNodes");
    }

    #[test]
    fn not_load_node() {
        run("notLoadNode");
    }
}
