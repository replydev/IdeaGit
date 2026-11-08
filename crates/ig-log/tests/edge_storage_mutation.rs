//! IdeaGit tests (upstream has none for these APIs): mutation, filtering and ordering of
//! `EdgeStorage`/`EdgeStorageWrapper`, and the dotted/hidden edge merging of `LinearGraphWrapper`.

mod common;

use std::sync::{Arc, RwLock};

use common::test_graph_builder::{graph, u};
use ig_log::api::EdgeFilter;
use ig_log::api::elements::{GraphEdge, GraphEdgeType};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::collapsing::edge_storage::EdgeStorage;
use ig_log::collapsing::edge_storage_wrapper::EdgeStorageWrapper;
use ig_log::facade::linear_graph_wrapper::LinearGraphWrapper;

/// Wrapper over a fresh storage where node ids are node indexes.
fn identity() -> (Arc<RwLock<EdgeStorage>>, EdgeStorageWrapper) {
    let storage = Arc::new(RwLock::new(EdgeStorage::new()));
    let wrapper = EdgeStorageWrapper::new(storage.clone(), Some, |index| index);
    (storage, wrapper)
}

fn known_ids(storage: &Arc<RwLock<EdgeStorage>>) -> Vec<i32> {
    let mut ids = storage.read().unwrap().get_known_ids();
    ids.sort_unstable();
    ids
}

#[test]
fn create_then_remove_normal_edge_leaves_storage_empty() {
    let mut storage = EdgeStorage::new();
    storage.create_edge(1, 2, GraphEdgeType::Usual);
    let mut ids = storage.get_known_ids();
    ids.sort_unstable();
    assert_eq!(ids, vec![1, 2]);
    storage.remove_edge(1, 2, GraphEdgeType::Usual);
    assert!(storage.get_known_ids().is_empty());
}

#[test]
fn create_then_remove_special_edge_leaves_storage_empty() {
    for edge_type in [
        GraphEdgeType::NotLoadCommit,
        GraphEdgeType::DottedArrowUp,
        GraphEdgeType::DottedArrowDown,
    ] {
        let mut storage = EdgeStorage::new();
        storage.create_edge(3, 7, edge_type);
        assert_eq!(storage.get_known_ids(), vec![3]);
        assert_eq!(storage.get_edges(3), vec![(Some(7), edge_type)]);
        storage.remove_edge(3, 7, edge_type);
        assert!(storage.get_known_ids().is_empty(), "{edge_type:?}");
    }
}

#[test]
fn special_edge_with_null_target_round_trips() {
    let mut storage = EdgeStorage::new();
    storage.create_edge(3, EdgeStorage::NULL_ID, GraphEdgeType::NotLoadCommit);
    assert_eq!(
        storage.get_edges(3),
        vec![(None, GraphEdgeType::NotLoadCommit)]
    );
    storage.remove_edge(3, EdgeStorage::NULL_ID, GraphEdgeType::NotLoadCommit);
    assert!(storage.get_known_ids().is_empty());
}

#[test]
fn wrapper_create_then_remove_returns_to_empty() {
    let (storage, wrapper) = identity();
    let normal = GraphEdge::create_normal_edge(1, 4, GraphEdgeType::Dotted);
    let special = GraphEdge::create_edge_with_target_id(2, Some(50), GraphEdgeType::NotLoadCommit);
    wrapper.create_edge(&normal);
    wrapper.create_edge(&special);
    assert_eq!(known_ids(&storage), vec![1, 2, 4]);
    wrapper.remove_edge(&normal);
    wrapper.remove_edge(&special);
    assert!(known_ids(&storage).is_empty());
}

#[test]
fn has_edge_is_true_between_two_ids_until_removed() {
    let (_storage, wrapper) = identity();
    let edge = GraphEdge::create_normal_edge(3, 6, GraphEdgeType::Usual);
    assert!(!wrapper.has_edge(3, 6));
    wrapper.create_edge(&edge);
    assert!(wrapper.has_edge(3, 6));
    assert!(wrapper.has_edge(6, 3));
    assert!(!wrapper.has_edge(3, 7));
    wrapper.remove_edge(&edge);
    assert!(!wrapper.has_edge(3, 6));
    assert!(!wrapper.has_edge(6, 3));
}

#[test]
fn adjacent_edges_are_filtered_by_edge_filter() {
    let (_storage, wrapper) = identity();
    let up = GraphEdge::create_normal_edge(1, 5, GraphEdgeType::Usual);
    let down = GraphEdge::create_normal_edge(5, 8, GraphEdgeType::Dotted);
    let special = GraphEdge::create_edge_with_target_id(5, Some(99), GraphEdgeType::NotLoadCommit);
    for edge in [&up, &down, &special] {
        wrapper.create_edge(edge);
    }

    let sorted = |filter| {
        let mut edges = wrapper.get_adjacent_edges(5, filter);
        edges.sort();
        edges
    };
    let sorted_of = |mut edges: Vec<GraphEdge>| {
        edges.sort();
        edges
    };

    assert_eq!(sorted(EdgeFilter::NormalUp), vec![up]);
    assert_eq!(sorted(EdgeFilter::NormalDown), vec![down]);
    assert_eq!(sorted(EdgeFilter::Special), vec![special]);
    assert_eq!(sorted(EdgeFilter::NormalAll), sorted_of(vec![up, down]));
    assert_eq!(sorted(EdgeFilter::All), sorted_of(vec![up, down, special]));
}

#[test]
fn get_edges_round_trips_in_a_stable_order() {
    let (_storage, wrapper) = identity();
    let created = [
        GraphEdge::create_normal_edge(1, 5, GraphEdgeType::Usual),
        GraphEdge::create_normal_edge(5, 8, GraphEdgeType::Dotted),
        GraphEdge::create_normal_edge(2, 3, GraphEdgeType::Usual),
        GraphEdge::create_edge_with_target_id(5, Some(99), GraphEdgeType::NotLoadCommit),
        GraphEdge::create_edge_with_target_id(8, None, GraphEdgeType::DottedArrowDown),
    ];
    for edge in &created {
        wrapper.create_edge(edge);
    }

    let first: Vec<GraphEdge> = wrapper.get_edges().into_iter().collect();
    let second: Vec<GraphEdge> = wrapper.get_edges().into_iter().collect();
    assert_eq!(first, second);
    assert!(first.windows(2).all(|pair| pair[0] < pair[1]));

    let mut expected = created.to_vec();
    expected.sort();
    assert_eq!(first, expected);
}

#[test]
fn linear_graph_wrapper_puts_dotted_first_and_removes_hidden() {
    // 0 -> 1 -> 2, ids equal to indexes
    let base = graph(|b| {
        b.node(0, [u(1)]);
        b.node(1, [u(2)]);
        b.node(2, Vec::<common::test_graph_builder::SimpleEdge>::new());
    });
    let wrapper = LinearGraphWrapper::new(Arc::new(base));

    let dotted = GraphEdge::create_normal_edge(0, 2, GraphEdgeType::Dotted);
    wrapper.dotted_edges.create_edge(&dotted);
    let edge_0_1 = GraphEdge::create_normal_edge(0, 1, GraphEdgeType::Usual);

    assert_eq!(
        wrapper.get_adjacent_edges(0, EdgeFilter::All),
        vec![dotted, edge_0_1]
    );

    wrapper.hidden_edges.create_edge(&edge_0_1);
    assert_eq!(wrapper.get_adjacent_edges(0, EdgeFilter::All), vec![dotted]);
    // node 1 only had the hidden edge going up and its own edge going down
    let edge_1_2 = GraphEdge::create_normal_edge(1, 2, GraphEdgeType::Usual);
    assert_eq!(
        wrapper.get_adjacent_edges(1, EdgeFilter::All),
        vec![edge_1_2]
    );

    wrapper.hidden_edges.remove_edge(&edge_0_1);
    assert_eq!(
        wrapper.get_adjacent_edges(0, EdgeFilter::All),
        vec![dotted, edge_0_1]
    );
}
