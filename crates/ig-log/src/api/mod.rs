//! Internal graph API: edge filters and the graph layout, plus the submodules for elements, linear
//! graphs, permanent graph info and printing.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/EdgeFilter.java` and
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/GraphLayout.java`.

pub mod elements;
pub mod linear_graph;
pub mod permanent;
pub mod printer;

/// Which adjacent edges [`LinearGraph::get_adjacent_edges`](linear_graph::LinearGraph::get_adjacent_edges)
/// returns: normal edges going up and/or down, and/or special (non-normal) edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EdgeFilter {
    All,
    NormalDown,
    NormalUp,
    NormalAll,
    Special,
}

impl EdgeFilter {
    pub fn up_normal(self) -> bool {
        matches!(
            self,
            EdgeFilter::All | EdgeFilter::NormalUp | EdgeFilter::NormalAll
        )
    }

    pub fn down_normal(self) -> bool {
        matches!(
            self,
            EdgeFilter::All | EdgeFilter::NormalDown | EdgeFilter::NormalAll
        )
    }

    pub fn special(self) -> bool {
        matches!(self, EdgeFilter::All | EdgeFilter::Special)
    }
}

/// Layout of the permanent graph: every node belongs to a "layout index" (a fragment started by one
/// of the head nodes, in head order). Node indexes must be in `0..graph.nodes_count()`.
pub trait GraphLayout: Send + Sync {
    fn get_layout_index(&self, node_index: i32) -> i32;

    fn get_one_of_head_node_index(&self, node_index: i32) -> i32;

    /// Head nodes that start a layout fragment, sorted by layout index.
    fn get_head_node_index(&self) -> &[i32];
}
