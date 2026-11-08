//! Orders the elements of a row from left to right by layout index.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphElementComparatorByLayoutIndex.java`.
//!
//! This is not a total order (special edges tie with everything at the same layout index), so sort
//! with [`java_sort`](crate::utils::java_sort), not `slice::sort_by`. Integer arithmetic wraps like
//! Java's.

use std::cmp::Ordering;

use crate::api::elements::{GraphEdge, GraphElement, GraphNode};
use crate::utils::linear_graph_utils::{as_normal_edge, get_not_null_node_index};

pub struct GraphElementComparatorByLayoutIndex {
    layout_index_getter: Box<dyn Fn(i32) -> i32 + Send + Sync>,
}

impl GraphElementComparatorByLayoutIndex {
    /// `layout_index_getter` maps a node index to its layout index.
    pub fn new(
        layout_index_getter: impl Fn(i32) -> i32 + Send + Sync + 'static,
    ) -> GraphElementComparatorByLayoutIndex {
        GraphElementComparatorByLayoutIndex {
            layout_index_getter: Box::new(layout_index_getter),
        }
    }

    /// Upstream `compare`: negative, zero or positive.
    pub fn compare(&self, o1: &GraphElement, o2: &GraphElement) -> i32 {
        match (o1, o2) {
            (GraphElement::Edge(edge1), GraphElement::Edge(edge2)) => {
                let Some(normal_edge1) = as_normal_edge(edge1) else {
                    return self
                        .compare2(edge2, GraphNode::new(get_not_null_node_index(edge1)))
                        .wrapping_neg();
                };
                let Some(normal_edge2) = as_normal_edge(edge2) else {
                    return self.compare2(edge1, GraphNode::new(get_not_null_node_index(edge2)));
                };

                if normal_edge1.up == normal_edge2.up {
                    return if normal_edge1.down < normal_edge2.down {
                        self.compare2(edge2, GraphNode::new(normal_edge1.down))
                            .wrapping_neg()
                    } else {
                        self.compare2(edge1, GraphNode::new(normal_edge2.down))
                    };
                }

                if normal_edge1.up < normal_edge2.up {
                    self.compare2(edge1, GraphNode::new(normal_edge2.up))
                } else {
                    self.compare2(edge2, GraphNode::new(normal_edge1.up))
                        .wrapping_neg()
                }
            }
            (GraphElement::Edge(edge), GraphElement::Node(node)) => self.compare2(edge, *node),
            (GraphElement::Node(node), GraphElement::Edge(edge)) => {
                self.compare2(edge, *node).wrapping_neg()
            }
            (GraphElement::Node(_), GraphElement::Node(_)) => {
                // upstream: `assert false; return 0;`
                if cfg!(debug_assertions) {
                    panic!("Comparing two nodes: {o1:?} and {o2:?}");
                }
                0
            }
        }
    }

    /// [`compare`](Self::compare) as an [`Ordering`].
    pub fn ordering(&self, o1: &GraphElement, o2: &GraphElement) -> Ordering {
        self.compare(o1, o2).cmp(&0)
    }

    fn compare2(&self, edge: &GraphEdge, node: GraphNode) -> i32 {
        let Some(normal_edge) = as_normal_edge(edge) else {
            return self
                .get_layout_index(get_not_null_node_index(edge))
                .wrapping_sub(self.get_layout_index(node.node_index));
        };

        let up_edge_li = self.get_layout_index(normal_edge.up);
        let down_edge_li = self.get_layout_index(normal_edge.down);

        let node_li = self.get_layout_index(node.node_index);
        let max_li = up_edge_li.max(down_edge_li);
        if max_li != node_li {
            max_li.wrapping_sub(node_li)
        } else {
            normal_edge.up.wrapping_sub(node.node_index)
        }
    }

    fn get_layout_index(&self, node_index: i32) -> i32 {
        (self.layout_index_getter)(node_index)
    }
}
