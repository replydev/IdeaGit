//! Additional edges stored by node id, each packed with its type into one `i32`.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/EdgeStorage.java`.

use crate::api::elements::GraphEdgeType;
use crate::utils::int_int_multi_map::IntIntMultiMap;

const EDGE_TYPE_BITS: u32 = 4;
const EDGE_BITS_OFFSET: u32 = i32::BITS - EDGE_TYPE_BITS;
const COMPRESSED_NODE_ID_MASK: i32 = (u32::MAX >> EDGE_TYPE_BITS) as i32;
const MAX_EDGE_TYPE_COUNT: usize = 1 << EDGE_TYPE_BITS;
const MAX_NODE_ID: i32 = i32::MAX >> EDGE_TYPE_BITS;
const MIN_NODE_ID: i32 = i32::MIN >> EDGE_TYPE_BITS;

const _: () = assert!(GraphEdgeType::VALUES.len() <= MAX_EDGE_TYPE_COUNT);

/// Normal edges are stored under both node ids; special edges only under their node.
///
/// Node ids must fit 28 bits (together with the 4-bit edge type they pack into one `i32`). This is
/// checked only by `debug_assert`, as with upstream's `assert`.
#[derive(Clone, Debug, Default)]
pub struct EdgeStorage {
    edges: IntIntMultiMap,
}

impl EdgeStorage {
    /// Stands for a missing target id.
    pub const NULL_ID: i32 = MIN_NODE_ID;

    pub fn new() -> EdgeStorage {
        EdgeStorage::default()
    }

    pub fn create_edge(&mut self, main_node_id: i32, addition_id: i32, edge_type: GraphEdgeType) {
        // compute both values first so a failing assertion cannot leave a half-written edge
        let main_value = compress_edge(addition_id, edge_type);
        let addition_value = edge_type
            .is_normal_edge()
            .then(|| compress_edge(main_node_id, edge_type));
        self.edges.put_value(main_node_id, main_value);
        if let Some(value) = addition_value {
            self.edges.put_value(addition_id, value);
        }
    }

    pub fn remove_edge(&mut self, main_node_id: i32, addition_id: i32, edge_type: GraphEdgeType) {
        let main_value = compress_edge(addition_id, edge_type);
        let addition_value = edge_type
            .is_normal_edge()
            .then(|| compress_edge(main_node_id, edge_type));
        self.edges.remove(main_node_id, main_value);
        if let Some(value) = addition_value {
            self.edges.remove(addition_id, value);
        }
    }

    /// Edges of `node_id` as (other node id or target id, type); `None` for [`Self::NULL_ID`].
    pub fn get_edges(&self, node_id: i32) -> Vec<(Option<i32>, GraphEdgeType)> {
        self.edges
            .get(node_id)
            .iter()
            .map(|&compressed| {
                (
                    convert_to_integer(retrieved_node_id(compressed)),
                    retrieved_type(compressed),
                )
            })
            .collect()
    }

    pub fn get_known_ids(&self) -> Vec<i32> {
        self.edges.keys()
    }

    pub fn remove_all(&mut self) {
        self.edges.clear();
    }
}

fn convert_to_integer(value: i32) -> Option<i32> {
    if value == EdgeStorage::NULL_ID {
        None
    } else {
        Some(value)
    }
}

fn compress_edge(node_id: i32, edge_type: GraphEdgeType) -> i32 {
    debug_assert!(
        node_id == EdgeStorage::NULL_ID || (node_id < MAX_NODE_ID && node_id > MIN_NODE_ID)
    );
    (edge_type.ordinal() << EDGE_BITS_OFFSET) | (COMPRESSED_NODE_ID_MASK & node_id)
}

fn retrieved_type(compress_edge: i32) -> GraphEdgeType {
    let edge_type = (compress_edge as u32) >> EDGE_BITS_OFFSET;
    GraphEdgeType::VALUES[edge_type as usize]
}

fn retrieved_node_id(compress_edge: i32) -> i32 {
    (compress_edge << EDGE_TYPE_BITS) >> EDGE_TYPE_BITS
}
