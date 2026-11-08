//! Node-index view of an [`EdgeStorage`] (which is keyed by node id).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/EdgeStorageWrapper.java`.
//!
//! Deviation: the storage is shared through `Arc<RwLock<_>>` (upstream shares the mutable object
//! between wrappers and `CollapsedGraph`), so mutating methods take `&self`.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphEdgeType};
use crate::api::linear_graph::LinearGraph;
use crate::collapsing::edge_storage::EdgeStorage;
use crate::utils::linear_graph_utils::int_equal;

type IndexById = Arc<dyn Fn(i32) -> Option<i32> + Send + Sync>;
type IdByIndex = Arc<dyn Fn(i32) -> i32 + Send + Sync>;

#[derive(Clone)]
pub struct EdgeStorageWrapper {
    edge_storage: Arc<RwLock<EdgeStorage>>,
    get_node_index_by_id: IndexById,
    get_node_id_by_index: IdByIndex,
}

impl EdgeStorageWrapper {
    pub fn new(
        edge_storage: Arc<RwLock<EdgeStorage>>,
        get_node_index_by_id: impl Fn(i32) -> Option<i32> + Send + Sync + 'static,
        get_node_id_by_index: impl Fn(i32) -> i32 + Send + Sync + 'static,
    ) -> EdgeStorageWrapper {
        EdgeStorageWrapper {
            edge_storage,
            get_node_index_by_id: Arc::new(get_node_index_by_id),
            get_node_id_by_index: Arc::new(get_node_id_by_index),
        }
    }

    /// Converts with the node ids / indexes of `graph`.
    pub fn for_graph(
        edge_storage: Arc<RwLock<EdgeStorage>>,
        graph: Arc<dyn LinearGraph>,
    ) -> EdgeStorageWrapper {
        let graph2 = graph.clone();
        EdgeStorageWrapper::new(
            edge_storage,
            move |node_id| graph.get_node_index(node_id),
            move |node_index| graph2.get_node_id(node_index),
        )
    }

    /// `createSimpleEdgeStorage`: a fresh storage where node ids are node indexes.
    pub fn create_simple_edge_storage() -> EdgeStorageWrapper {
        EdgeStorageWrapper::new(Arc::new(RwLock::new(EdgeStorage::new())), Some, |index| {
            index
        })
    }

    fn storage(&self) -> std::sync::RwLockReadGuard<'_, EdgeStorage> {
        self.edge_storage
            .read()
            .expect("edge storage lock poisoned")
    }

    fn storage_mut(&self) -> std::sync::RwLockWriteGuard<'_, EdgeStorage> {
        self.edge_storage
            .write()
            .expect("edge storage lock poisoned")
    }

    pub fn remove_edge(&self, graph_edge: &GraphEdge) {
        let (main_id, addition_id) = self.get_node_ids(graph_edge);
        self.storage_mut()
            .remove_edge(main_id, addition_id, graph_edge.edge_type);
    }

    pub fn create_edge(&self, graph_edge: &GraphEdge) {
        let (main_id, addition_id) = self.get_node_ids(graph_edge);
        self.storage_mut()
            .create_edge(main_id, addition_id, graph_edge.edge_type);
    }

    /// Whether a normal edge between the two node indexes is stored.
    pub fn has_edge(&self, from_index: i32, to_index: i32) -> bool {
        let to_id = (self.get_node_id_by_index)(to_index);
        self.storage()
            .get_edges((self.get_node_id_by_index)(from_index))
            .into_iter()
            .any(|(other, edge_type)| edge_type.is_normal_edge() && int_equal(other, to_id))
    }

    pub fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        let retrieved = self
            .storage()
            .get_edges((self.get_node_id_by_index)(node_index));
        retrieved
            .into_iter()
            .filter_map(|(target_id, edge_type)| {
                self.decompress_edge(node_index, target_id, edge_type)
            })
            .filter(|edge| matched_edge(node_index, edge, filter))
            .collect()
    }

    pub fn get_edges(&self) -> HashSet<GraphEdge> {
        let known_ids = self.storage().get_known_ids();
        let mut result = HashSet::new();
        for id in known_ids {
            // upstream passes a possibly null index on; an unknown id has no edges to report
            if let Some(index) = (self.get_node_index_by_id)(id) {
                result.extend(self.get_adjacent_edges(index, EdgeFilter::All));
            }
        }
        result
    }

    fn get_node_ids(&self, graph_edge: &GraphEdge) -> (i32, i32) {
        match (graph_edge.up_node_index, graph_edge.down_node_index) {
            (Some(up), Some(down)) => (
                (self.get_node_id_by_index)(up),
                (self.get_node_id_by_index)(down),
            ),
            (Some(up), None) => (
                (self.get_node_id_by_index)(up),
                convert_to_int(graph_edge.target_id),
            ),
            (None, down) => {
                let down = down.expect("edge without node indexes");
                (
                    (self.get_node_id_by_index)(down),
                    convert_to_int(graph_edge.target_id),
                )
            }
        }
    }

    fn decompress_edge(
        &self,
        node_index: i32,
        target_id: Option<i32>,
        edge_type: GraphEdgeType,
    ) -> Option<GraphEdge> {
        if edge_type.is_normal_edge() {
            let target_id = target_id.expect("normal edge without target");
            // todo edge to hide node
            let another_node_index = (self.get_node_index_by_id)(target_id)?;
            Some(GraphEdge::create_normal_edge(
                node_index,
                another_node_index,
                edge_type,
            ))
        } else {
            Some(GraphEdge::create_edge_with_target_id(
                node_index, target_id, edge_type,
            ))
        }
    }

    pub fn remove_all(&self) {
        self.storage_mut().remove_all();
    }
}

fn matched_edge(start_node_index: i32, edge: &GraphEdge, filter: EdgeFilter) -> bool {
    if edge.edge_type.is_normal_edge() {
        (start_node_index == convert_to_int(edge.down_node_index) && filter.up_normal())
            || (start_node_index == convert_to_int(edge.up_node_index) && filter.down_normal())
    } else {
        filter.special()
    }
}

fn convert_to_int(value: Option<i32>) -> i32 {
    value.unwrap_or(EdgeStorage::NULL_ID)
}
