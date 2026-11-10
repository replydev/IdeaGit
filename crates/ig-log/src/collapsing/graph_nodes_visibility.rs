//! Visibility of a graph's nodes, stored by node id.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/GraphNodesVisibility.java`.
//!
//! Deviation: `GraphNodesVisibility` implements [`Flags`] itself (upstream: the `asFlags()` view),
//! so the [`ListIntToIntMap`](crate::utils::list_int_to_int_map::ListIntToIntMap) of
//! [`CollapsedGraph`](crate::collapsing::collapsed_graph::CollapsedGraph) can own it.

use std::sync::Arc;

use crate::api::linear_graph::LinearGraph;
use crate::utils::flags::Flags;
use crate::utils::unsigned_bit_set::UnsignedBitSet;

pub struct GraphNodesVisibility {
    linear_graph: Arc<dyn LinearGraph>,
    node_visibility_by_id: UnsignedBitSet,
}

impl GraphNodesVisibility {
    pub fn new(
        linear_graph: Arc<dyn LinearGraph>,
        node_visibility_by_id: UnsignedBitSet,
    ) -> GraphNodesVisibility {
        GraphNodesVisibility {
            linear_graph,
            node_visibility_by_id,
        }
    }

    pub fn get_node_visibility_by_id(&self) -> &UnsignedBitSet {
        &self.node_visibility_by_id
    }

    pub fn set_node_visibility_by_id(&mut self, node_visibility_by_id: UnsignedBitSet) {
        self.node_visibility_by_id = node_visibility_by_id;
    }

    pub fn is_visible(&self, node_index: i32) -> bool {
        self.node_visibility_by_id.get(self.node_id(node_index))
    }

    pub fn show(&mut self, node_index: i32) {
        let node_id = self.node_id(node_index);
        self.node_visibility_by_id.set(node_id, true);
    }

    pub fn hide(&mut self, node_index: i32) {
        let node_id = self.node_id(node_index);
        self.node_visibility_by_id.set(node_id, false);
    }

    fn node_id(&self, node_index: i32) -> i32 {
        self.linear_graph.get_node_id(node_index)
    }
}

/// `asFlags()`: visibility by node index.
impl Flags for GraphNodesVisibility {
    fn size(&self) -> i32 {
        self.linear_graph.nodes_count()
    }

    fn get(&self, index: i32) -> bool {
        self.node_visibility_by_id.get(self.node_id(index))
    }

    fn set(&mut self, index: i32, value: bool) {
        let node_id = self.node_id(index);
        self.node_visibility_by_id.set(node_id, value);
    }

    fn set_all(&mut self, value: bool) {
        for index in 0..self.size() {
            self.set(index, value);
        }
    }
}
