//! Dotted edges (and dotted arrows) that connect visible nodes across hidden ones, so a filtered
//! graph keeps its shape.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/DottedFilterEdgesGenerator.kt`
//! (`DottedFilterEdgesGenerator`, `DottedFilterEdgesGeneratorImpl`, `InplaceFilterEdgesGenerator`,
//! `ShiftNumber`).
//!
//! Deviation: the abstract class becomes [`DottedFilterEdgesGenerator`] over a
//! [`DottedFilterEdgesCallbacks`] implementation.
//!
//! Upstream quirk kept: `hasDottedEdges` returns `false` for the down direction even when it finds
//! a down dotted edge, so no edge or arrow is added to nodes below the updated range. Every caller
//! updates the whole graph, so there are no such nodes.

use std::collections::HashSet;

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphEdgeType};
use crate::api::linear_graph::{LiteLinearGraph, NodeFilter};
use crate::collapsing::collapsed_graph::{CollapsedGraph, Modification};
use crate::utils::linear_graph_utils::{as_lite_linear_graph, is_edge_down, is_edge_up};
use crate::visible_graph::VcsLogVisibleGraphIndex;

/// The abstract methods of `DottedFilterEdgesGenerator`.
pub trait DottedFilterEdgesCallbacks {
    fn node_is_visible(&self, node_index: VcsLogVisibleGraphIndex) -> bool;

    fn add_dotted_edge(
        &mut self,
        node_index1: VcsLogVisibleGraphIndex,
        node_index2: VcsLogVisibleGraphIndex,
    );

    fn add_dotted_arrow(&mut self, node_index: VcsLogVisibleGraphIndex, is_up: bool);

    fn has_dotted_edges(&self, node_index: VcsLogVisibleGraphIndex, is_up: bool) -> bool;
}

pub struct DottedFilterEdgesGenerator<'g, G: ?Sized, C> {
    graph: &'g G,
    up_index: VcsLogVisibleGraphIndex,
    down_index: VcsLogVisibleGraphIndex,
    numbers: ShiftNumber,
    callbacks: C,
}

impl<'g, G: LiteLinearGraph + ?Sized, C: DottedFilterEdgesCallbacks>
    DottedFilterEdgesGenerator<'g, G, C>
{
    pub fn new(
        graph: &'g G,
        up_index: VcsLogVisibleGraphIndex,
        down_index: VcsLogVisibleGraphIndex,
        callbacks: C,
    ) -> Self {
        DottedFilterEdgesGenerator {
            graph,
            up_index,
            down_index,
            numbers: ShiftNumber::new(up_index, down_index),
            callbacks,
        }
    }

    /// Updates the specified range.
    pub fn update(&mut self) {
        self.down_walk();
        self.cleanup();
        self.up_walk();
    }

    fn cleanup(&mut self) {
        for current_node_index in self.up_index..=self.down_index {
            self.numbers.set_number(current_node_index, i32::MAX);
        }
    }

    fn add_edge_or_arrow(
        &mut self,
        current_node_index: VcsLogVisibleGraphIndex,
        another_node_index: VcsLogVisibleGraphIndex,
        is_up: bool,
    ) {
        if self.callbacks.has_dotted_edges(current_node_index, is_up) {
            if self.callbacks.node_is_visible(another_node_index) {
                self.callbacks
                    .add_dotted_edge(current_node_index, another_node_index);
            } else {
                self.callbacks.add_dotted_arrow(current_node_index, is_up);
            }
        }
    }

    fn down_walk(&mut self) {
        for current_node_index in self.up_index..=self.down_index {
            if self.callbacks.node_is_visible(current_node_index) {
                let mut nearly_up = i32::MIN;
                let mut max_adj_number = i32::MIN;
                for up_node in self.graph.get_nodes(current_node_index, NodeFilter::Up) {
                    if up_node < self.up_index {
                        self.add_edge_or_arrow(current_node_index, up_node, true);
                        continue;
                    }

                    if self.callbacks.node_is_visible(up_node) {
                        max_adj_number = max_adj_number.max(self.numbers.get_number(up_node));
                    } else {
                        nearly_up = nearly_up.max(self.numbers.get_number(up_node));
                    }
                }

                if nearly_up == max_adj_number || nearly_up == i32::MIN {
                    self.numbers.set_number(current_node_index, max_adj_number);
                } else {
                    self.callbacks
                        .add_dotted_edge(current_node_index, nearly_up);
                    self.numbers.set_number(current_node_index, nearly_up);
                }
            } else {
                // node currentNodeIndex invisible

                let mut nearly_up = i32::MIN;
                for up_node in self.graph.get_nodes(current_node_index, NodeFilter::Up) {
                    if self.callbacks.node_is_visible(up_node) {
                        nearly_up = nearly_up.max(up_node);
                    } else if up_node >= self.up_index {
                        nearly_up = nearly_up.max(self.numbers.get_number(up_node));
                    }
                }
                self.numbers.set_number(current_node_index, nearly_up);
            }
        }
    }

    fn up_walk(&mut self) {
        for current_node_index in (self.up_index..=self.down_index).rev() {
            if self.callbacks.node_is_visible(current_node_index) {
                let mut nearly_down = i32::MAX;
                let mut min_adj_number = i32::MAX;
                for down_node in self.graph.get_nodes(current_node_index, NodeFilter::Down) {
                    if down_node > self.down_index {
                        self.add_edge_or_arrow(current_node_index, down_node, false);
                        continue;
                    }

                    if self.callbacks.node_is_visible(down_node) {
                        min_adj_number = min_adj_number.min(self.numbers.get_number(down_node));
                    } else {
                        nearly_down = nearly_down.min(self.numbers.get_number(down_node));
                    }
                }

                if nearly_down == min_adj_number || nearly_down == i32::MAX {
                    self.numbers.set_number(current_node_index, min_adj_number);
                } else {
                    self.callbacks
                        .add_dotted_edge(current_node_index, nearly_down);
                    self.numbers.set_number(current_node_index, nearly_down);
                }
            } else {
                // node currentNodeIndex invisible

                let mut nearly_down = i32::MAX;
                for down_node in self.graph.get_nodes(current_node_index, NodeFilter::Down) {
                    if self.callbacks.node_is_visible(down_node) {
                        nearly_down = nearly_down.min(down_node);
                    } else if down_node <= self.down_index {
                        nearly_down = nearly_down.min(self.numbers.get_number(down_node));
                    }
                }
                self.numbers.set_number(current_node_index, nearly_down);
            }
        }
    }
}

/// `DottedFilterEdgesGenerator.update(collapsedGraph, up, down)`: adds the dotted edges of the
/// delegate node range `up_delegate_node_index..=down_delegate_node_index`.
pub fn update(
    collapsed_graph: &CollapsedGraph,
    up_delegate_node_index: VcsLogVisibleGraphIndex,
    down_delegate_node_index: VcsLogVisibleGraphIndex,
) {
    let mut modification = collapsed_graph.start_modification();
    let delegated_graph = collapsed_graph.delegated_graph();
    let lite = as_lite_linear_graph(delegated_graph);
    DottedFilterEdgesGenerator::new(
        &lite,
        up_delegate_node_index,
        down_delegate_node_index,
        DottedFilterEdgesGeneratorImpl {
            collapsed_graph,
            modification: &mut modification,
        },
    )
    .update();
    modification.apply();
}

/// `DottedFilterEdgesGenerator.hideInplace`: hides the compiled rows `to_hide` and connects the
/// remaining rows with dotted edges.
pub fn hide_inplace(collapsed_graph: &CollapsedGraph, to_hide: &HashSet<VcsLogVisibleGraphIndex>) {
    let mut modification = collapsed_graph.start_modification();
    let compiled_graph = collapsed_graph.compiled_graph();
    let lite = as_lite_linear_graph(compiled_graph.clone());
    DottedFilterEdgesGenerator::new(
        &lite,
        0,
        compiled_graph.nodes_count() - 1,
        InplaceFilterEdgesGenerator {
            collapsed_graph,
            modification: &mut modification,
            to_hide,
        },
    )
    .update();
    for &it in to_hide {
        modification.hide_node(collapsed_graph.convert_to_delegate_node_index(it));
    }
    modification.apply();
}

/// `DottedFilterEdgesGeneratorImpl`: works on the delegate graph of a collapsed graph.
pub struct DottedFilterEdgesGeneratorImpl<'a> {
    collapsed_graph: &'a CollapsedGraph,
    modification: &'a mut Modification,
}

impl DottedFilterEdgesCallbacks for DottedFilterEdgesGeneratorImpl<'_> {
    fn node_is_visible(&self, node_index: VcsLogVisibleGraphIndex) -> bool {
        self.collapsed_graph.is_node_visible(node_index)
    }

    fn add_dotted_edge(
        &mut self,
        node_index1: VcsLogVisibleGraphIndex,
        node_index2: VcsLogVisibleGraphIndex,
    ) {
        self.modification
            .create_edge(&GraphEdge::create_normal_edge(
                node_index1,
                node_index2,
                GraphEdgeType::Dotted,
            ));
    }

    fn add_dotted_arrow(&mut self, node_index: VcsLogVisibleGraphIndex, is_up: bool) {
        self.modification
            .create_edge(&GraphEdge::create_edge_with_target_id(
                node_index,
                None,
                dotted_arrow_type(is_up),
            ));
    }

    fn has_dotted_edges(&self, node_index: VcsLogVisibleGraphIndex, is_up: bool) -> bool {
        has_dotted_edges(self.modification, node_index, is_up)
    }
}

fn dotted_arrow_type(is_up: bool) -> GraphEdgeType {
    if is_up {
        GraphEdgeType::DottedArrowUp
    } else {
        GraphEdgeType::DottedArrowDown
    }
}

/// `CollapsedGraph.Modification.hasDottedEdges` (see the module docs for its quirk).
fn has_dotted_edges(
    modification: &Modification,
    node_index: VcsLogVisibleGraphIndex,
    is_up: bool,
) -> bool {
    for edge in modification
        .edges_to_add()
        .get_adjacent_edges(node_index, EdgeFilter::NormalAll)
    {
        if edge.edge_type == GraphEdgeType::Dotted {
            if is_up && is_edge_up(&edge, node_index) {
                return true;
            }
            if !is_up && is_edge_down(&edge, node_index) {
                return false;
            }
        }
    }
    false
}

/// `InplaceFilterEdgesGenerator`: works on the compiled graph, hiding `to_hide`.
pub struct InplaceFilterEdgesGenerator<'a> {
    collapsed_graph: &'a CollapsedGraph,
    modification: &'a mut Modification,
    to_hide: &'a HashSet<VcsLogVisibleGraphIndex>,
}

impl DottedFilterEdgesCallbacks for InplaceFilterEdgesGenerator<'_> {
    fn node_is_visible(&self, node_index: VcsLogVisibleGraphIndex) -> bool {
        !self.to_hide.contains(&node_index)
    }

    fn add_dotted_edge(
        &mut self,
        node_index1: VcsLogVisibleGraphIndex,
        node_index2: VcsLogVisibleGraphIndex,
    ) {
        let delegate_index1 = self
            .collapsed_graph
            .convert_to_delegate_node_index(node_index1);
        let delegate_index2 = self
            .collapsed_graph
            .convert_to_delegate_node_index(node_index2);
        self.modification
            .create_edge(&GraphEdge::create_normal_edge(
                delegate_index1,
                delegate_index2,
                GraphEdgeType::Dotted,
            ));
    }

    fn add_dotted_arrow(&mut self, node_index: VcsLogVisibleGraphIndex, is_up: bool) {
        let delegate_index = self
            .collapsed_graph
            .convert_to_delegate_node_index(node_index);
        self.modification
            .create_edge(&GraphEdge::create_edge_with_target_id(
                delegate_index,
                None,
                dotted_arrow_type(is_up),
            ));
    }

    fn has_dotted_edges(&self, node_index: VcsLogVisibleGraphIndex, is_up: bool) -> bool {
        let delegate_index = self
            .collapsed_graph
            .convert_to_delegate_node_index(node_index);
        has_dotted_edges(self.modification, delegate_index, is_up)
    }
}

/// Numbers of the nodes `start_index..=end_index`; `-1` outside.
struct ShiftNumber {
    start_index: i32,
    end_index: i32,
    numbers: Vec<i32>,
}

impl ShiftNumber {
    fn new(start_index: i32, end_index: i32) -> ShiftNumber {
        ShiftNumber {
            start_index,
            end_index,
            numbers: vec![0; (end_index - start_index + 1).max(0) as usize],
        }
    }

    fn in_range(&self, node_index: i32) -> bool {
        (self.start_index..=self.end_index).contains(&node_index)
    }

    fn get_number(&self, node_index: i32) -> i32 {
        if self.in_range(node_index) {
            self.numbers[(node_index - self.start_index) as usize]
        } else {
            -1
        }
    }

    fn set_number(&mut self, node_index: i32, value: i32) {
        if self.in_range(node_index) {
            self.numbers[(node_index - self.start_index) as usize] = value;
        }
    }
}
