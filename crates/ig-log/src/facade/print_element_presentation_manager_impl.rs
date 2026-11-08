//! Colors and selection of the visible graph's print elements.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/PrintElementPresentationManagerImpl.kt`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::api::elements::GraphElement;
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::{PermanentGraphInfo, VcsLogGraphNodeId};
use crate::api::printer::{GraphColorGetter, GraphPrintElement, PrintElementPresentationManager};
use crate::utils::linear_graph_utils::{as_normal_edge, get_not_null_node_index};

/// What is selected: one hovered print element, or a set of node ids.
#[derive(Clone, Debug)]
pub enum Selection {
    FromPrintElement(GraphPrintElement),
    FromNodeIds(HashSet<VcsLogGraphNodeId>),
}

impl PartialEq for Selection {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Selection::FromPrintElement(a), Selection::FromPrintElement(b)) => a == b,
            (Selection::FromNodeIds(a), Selection::FromNodeIds(b)) => a == b,
            _ => false,
        }
    }
}

impl Selection {
    fn is_selected(
        &self,
        print_element: &GraphPrintElement,
        linear_graph: &dyn LinearGraph,
    ) -> bool {
        match self {
            Selection::FromPrintElement(selected) => print_element == selected,
            Selection::FromNodeIds(selected_node_ids) => match print_element.graph_element() {
                GraphElement::Node(node) => {
                    selected_node_ids.contains(&linear_graph.get_node_id(node.node_index))
                }
                GraphElement::Edge(edge) => {
                    let contains_index = |index: Option<i32>| {
                        index.is_none_or(|index| {
                            selected_node_ids.contains(&linear_graph.get_node_id(index))
                        })
                    };
                    edge.target_id
                        .is_none_or(|target| selected_node_ids.contains(&target))
                        && contains_index(edge.up_node_index)
                        && contains_index(edge.down_node_index)
                }
            },
        }
    }
}

pub struct PrintElementPresentationManagerImpl<CommitId> {
    permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    linear_graph: Arc<dyn LinearGraph>,
    color_getter: Arc<dyn GraphColorGetter>,
    selection: Selection,
}

impl<CommitId> PrintElementPresentationManagerImpl<CommitId> {
    pub fn new(
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
        linear_graph: Arc<dyn LinearGraph>,
        color_getter: Arc<dyn GraphColorGetter>,
    ) -> Self {
        PrintElementPresentationManagerImpl {
            permanent_graph_info,
            linear_graph,
            color_getter,
            selection: Selection::FromNodeIds(HashSet::new()),
        }
    }

    /// Returns whether the selection changed.
    pub fn set_selected_element(&mut self, print_element: GraphPrintElement) -> bool {
        self.set_selection(Selection::FromPrintElement(print_element))
    }

    /// Returns whether the selection changed.
    pub fn set_selected_elements(&mut self, selected_node_id: HashSet<VcsLogGraphNodeId>) -> bool {
        self.set_selection(Selection::FromNodeIds(selected_node_id))
    }

    fn set_selection(&mut self, new_selection: Selection) -> bool {
        if new_selection == self.selection {
            return false;
        }
        self.selection = new_selection;
        true
    }

    fn get_layout_index(&self, node_id: VcsLogGraphNodeId) -> i32 {
        if node_id < 0 {
            node_id
        } else {
            self.permanent_graph_info
                .permanent_graph_layout()
                .get_layout_index(node_id)
        }
    }

    fn node_color(&self, node_id: VcsLogGraphNodeId) -> i32 {
        self.color_getter
            .get_node_color(node_id, self.get_layout_index(node_id))
    }
}

impl<CommitId> PrintElementPresentationManager for PrintElementPresentationManagerImpl<CommitId> {
    fn is_selected(&self, print_element: &GraphPrintElement) -> bool {
        self.selection
            .is_selected(print_element, &*self.linear_graph)
    }

    /// Nodes use their own color; a normal edge uses the color of its end with the larger layout
    /// index (the up end on ties); a special edge uses the color of its node.
    fn get_color_id(&self, element: &GraphElement) -> i32 {
        let edge = match element {
            GraphElement::Node(node) => {
                return self.node_color(self.linear_graph.get_node_id(node.node_index));
            }
            GraphElement::Edge(edge) => edge,
        };
        let Some(normal_edge) = as_normal_edge(edge) else {
            return self.node_color(self.linear_graph.get_node_id(get_not_null_node_index(edge)));
        };
        let up_node_id = self.linear_graph.get_node_id(normal_edge.up);
        let down_node_id = self.linear_graph.get_node_id(normal_edge.down);
        let up_layout_index = self.get_layout_index(up_node_id);
        let down_layout_index = self.get_layout_index(down_node_id);
        if up_layout_index >= down_layout_index {
            self.color_getter
                .get_node_color(up_node_id, up_layout_index)
        } else {
            self.color_getter
                .get_node_color(down_node_id, down_layout_index)
        }
    }
}
