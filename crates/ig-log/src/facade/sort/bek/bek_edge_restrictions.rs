//! Edges whose up node must not be inserted before their down node.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekEdgeRestrictions.java`.

use crate::utils::int_int_multi_map::IntIntMultiMap;

#[derive(Clone, Debug, Default)]
pub struct BekEdgeRestrictions {
    up_to_edge: IntIntMultiMap,
    down_to_edge: IntIntMultiMap,
}

impl BekEdgeRestrictions {
    pub fn add_restriction(&mut self, up_node: i32, down_node: i32) {
        self.up_to_edge.put_value(up_node, down_node);
        self.down_to_edge.put_value(down_node, up_node);
    }

    pub fn remove_restriction(&mut self, down_node: i32) {
        for &up_node in self.down_to_edge.get(down_node) {
            self.up_to_edge.remove(up_node, down_node);
        }
    }

    pub fn has_restriction(&self, up_node: i32) -> bool {
        self.up_to_edge.contains_key(up_node)
    }
}
