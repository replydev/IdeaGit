//! The permanent graph layout: a layout index per node and the heads that start each fragment.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutImpl.kt`.

use crate::api::GraphLayout;

#[derive(Clone, Debug)]
pub struct GraphLayoutImpl {
    layout_index: Vec<i32>,
    head_nodes: Vec<i32>,
    layout_index_for_head_nodes: Vec<i32>,
}

impl GraphLayoutImpl {
    /// `layout_index` per node; `head_nodes` in increasing layout index order.
    pub fn new(layout_index: Vec<i32>, head_nodes: Vec<i32>) -> GraphLayoutImpl {
        let layout_index_for_head_nodes = head_nodes
            .iter()
            .map(|&head| layout_index[head as usize])
            .collect();
        GraphLayoutImpl {
            layout_index,
            head_nodes,
            layout_index_for_head_nodes,
        }
    }

    fn get_head_node_index_for_layout(&self, layout_index: i32) -> i32 {
        self.head_nodes[self.get_head_order(layout_index)]
    }

    /// Position of the last head whose layout index is `<= layout_index` (0 if none), like
    /// upstream's `Arrays.binarySearch` arithmetic.
    fn get_head_order(&self, layout_index: i32) -> usize {
        match self
            .layout_index_for_head_nodes
            .binary_search(&layout_index)
        {
            Ok(i) => i,
            Err(insertion_point) => insertion_point.saturating_sub(1),
        }
    }
}

impl GraphLayout for GraphLayoutImpl {
    fn get_layout_index(&self, node_index: i32) -> i32 {
        self.layout_index[node_index as usize]
    }

    fn get_one_of_head_node_index(&self, node_index: i32) -> i32 {
        self.get_head_node_index_for_layout(self.get_layout_index(node_index))
    }

    fn get_head_node_index(&self) -> &[i32] {
        &self.head_nodes
    }
}
