//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/EdgeStorageTest.kt`.

mod common;

use std::sync::{Arc, RwLock};

use common::str_utils::edge_as_string;
use common::test_graph_builder::{SimpleEdge, down_dot, not_load, up_dot};
use ig_log::api::EdgeFilter;
use ig_log::api::elements::GraphEdgeType;
use ig_log::collapsing::edge_storage::EdgeStorage;
use ig_log::collapsing::edge_storage_wrapper::EdgeStorageWrapper;

fn node_id_by_index(index: i32) -> i32 {
    index - 10
}

fn node_index_by_id(id: i32) -> Option<i32> {
    Some(id + 10)
}

/// `Int to SimpleEdge`
struct FullEdge {
    main_id: i32,
    addition_id: Option<i32>,
    edge_type: GraphEdgeType,
}

fn to(main_id: i32, edge: impl Into<SimpleEdge>) -> FullEdge {
    let edge = edge.into();
    FullEdge {
        main_id,
        addition_id: edge.to_node,
        edge_type: edge.edge_type,
    }
}

fn dot(to_node: i32) -> SimpleEdge {
    common::test_graph_builder::dot(to_node)
}

struct Storage(Arc<RwLock<EdgeStorage>>);

fn create() -> Storage {
    Storage(Arc::new(RwLock::new(EdgeStorage::new())))
}

impl Storage {
    /// `EdgeStorage + FullEdge`
    fn plus(self, edge: FullEdge) -> Storage {
        self.0.write().unwrap().create_edge(
            edge.main_id,
            edge.addition_id.unwrap_or(EdgeStorage::NULL_ID),
            edge.edge_type,
        );
        self
    }

    fn adapter(&self) -> EdgeStorageWrapper {
        EdgeStorageWrapper::new(self.0.clone(), node_index_by_id, node_id_by_index)
    }

    fn as_string(&self) -> String {
        let mut known_ids = self.0.read().unwrap().get_known_ids();
        known_ids.sort();
        let adapter = self.adapter();
        known_ids
            .iter()
            .map(|&id| {
                adapter
                    .get_adjacent_edges(node_index_by_id(id).unwrap(), EdgeFilter::All)
                    .iter()
                    .map(edge_as_string)
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect::<Vec<_>>()
            .join("|-")
    }

    fn assert(self, s: &str) {
        assert_eq!(s, self.as_string());
    }
}

#[test]
fn simple() {
    create().plus(to(1, 2)).assert("11:12:n_U|-11:12:n_U");
}

#[test]
fn dotted() {
    create().plus(to(1, dot(3))).assert("11:13:n_D|-11:13:n_D");
}

#[test]
fn not_load_test() {
    create().plus(to(1, not_load(Some(-3)))).assert("11:n:-3_N");
}

#[test]
fn not_load_null() {
    create().plus(to(1, not_load(None))).assert("11:n:n_N");
}

#[test]
fn down_dot_test() {
    create().plus(to(1, down_dot(Some(3)))).assert("11:n:3_O");
}

#[test]
fn up_dot_test() {
    create().plus(to(1, up_dot(Some(3)))).assert("n:11:3_P");
}

#[test]
fn negative_id() {
    create().plus(to(-2, -3)).assert("7:8:n_U|-7:8:n_U");
}

#[test]
fn big_id() {
    create()
        .plus(to(0x7fffff0, -0x7fffff0))
        .assert("-134217702:134217722:n_U|--134217702:134217722:n_U");
}

#[test]
fn two_equal_edge() {
    create()
        .plus(to(1, 2))
        .plus(to(1, 2))
        .assert("11:12:n_U|-11:12:n_U");
}

#[test]
fn two_from_one_node() {
    create()
        .plus(to(1, 2))
        .plus(to(1, 3))
        .assert("11:12:n_U,11:13:n_U|-11:12:n_U|-11:13:n_U");
}
