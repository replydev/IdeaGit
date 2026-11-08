//! Turns a row of the linear graph into print elements: the node, edge halves going up and down,
//! arrows for long edges, and terminal arrows.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/PrintElementGeneratorImpl.kt`.
//!
//! Long edges (at least `long_edge_size` rows) are only drawn within `visible_part_size` rows of
//! their ends, with arrows at the cut. Deviations: the generator owns its presentation manager
//! (upstream shares it with `VisibleGraphImpl`; reach it with
//! [`presentation_manager_mut`](PrintElementGeneratorImpl::presentation_manager_mut)), and the
//! row cache is a [`BoundedCache`]. `recommended_width` clamps the variance at 0 (upstream bug:
//! linear histories give NaN, hence 0).
//!
//! The in-row edge comparator is not a strict weak order. In-row edges arrive in `BTreeSet` order
//! instead of Java `HashSet` order, so ties can resolve differently from IntelliJ in rare rows,
//! mostly rows with not-loaded commit edges. Positions there can differ.

use std::cell::{OnceCell, RefCell};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphEdgeType, GraphElement};
use crate::api::linear_graph::LinearGraph;
use crate::api::printer::{
    GraphPrintElement, PrintElementGenerator, PrintElementPresentationManager,
};
use crate::print::elements::{EdgePrintElementImpl, SimplePrintElementImpl};
use crate::print_element::EdgePrintElementType;
use crate::utils::bounded_cache::BoundedCache;
use crate::utils::java_sort;
use crate::utils::linear_graph_utils::{as_normal_edge, int_equal, is_edge_down, is_edge_up};
use crate::utils::normal_edge::NormalEdge;
use crate::visible_graph::VcsLogVisibleGraphIndex;

use super::edges_in_row_generator::EdgesInRowGenerator;

const VERY_LONG_EDGE_SIZE: i32 = 1000;
pub const LONG_EDGE_SIZE: i32 = 30;
const VERY_LONG_EDGE_PART_SIZE: i32 = 250;
const LONG_EDGE_PART_SIZE: i32 = 1;

const CACHE_SIZE: usize = 100;
const SAMPLE_SIZE: i32 = 20000;
const K: f64 = 0.1;

type ElementComparator = Box<dyn Fn(&GraphElement, &GraphElement) -> Ordering + Send + Sync>;

pub struct PrintElementGeneratorImpl<P> {
    linear_graph: Arc<dyn LinearGraph>,
    presentation_manager: P,
    element_comparator: ElementComparator,
    long_edge_size: i32,
    visible_part_size: i32,
    edge_with_arrow_size: i32,
    cache: RefCell<BoundedCache<i32, Vec<GraphElement>>>,
    edges_in_row_generator: EdgesInRowGenerator,
    recommended_width: OnceCell<i32>,
}

/// Positions of the elements of a neighbour row (`createEndPositionFunction`).
struct EndPositions {
    to_position: Option<HashMap<GraphElement, i32>>,
    up: bool,
}

impl EndPositions {
    fn get(&self, edge: &GraphEdge, linear_graph: &dyn LinearGraph) -> Option<i32> {
        let to_position = self.to_position.as_ref()?;
        if let Some(&position) = to_position.get(&GraphElement::Edge(*edge)) {
            return Some(position);
        }
        let node_index = if self.up {
            edge.up_node_index
        } else {
            edge.down_node_index
        }?;
        to_position
            .get(&GraphElement::Node(linear_graph.get_graph_node(node_index)))
            .copied()
    }
}

impl<P: PrintElementPresentationManager> PrintElementGeneratorImpl<P> {
    /// Production sizes: long edges are cut unless `show_long_edges`.
    pub fn new(
        graph: Arc<dyn LinearGraph>,
        presentation_manager: P,
        show_long_edges: bool,
        element_comparator: impl Fn(&GraphElement, &GraphElement) -> Ordering + Send + Sync + 'static,
    ) -> PrintElementGeneratorImpl<P> {
        let (long_edge_size, visible_part_size, edge_with_arrow_size) = if show_long_edges {
            (
                VERY_LONG_EDGE_SIZE,
                VERY_LONG_EDGE_PART_SIZE,
                LONG_EDGE_SIZE,
            )
        } else {
            (LONG_EDGE_SIZE, LONG_EDGE_PART_SIZE, i32::MAX)
        };
        PrintElementGeneratorImpl::with_sizes(
            graph,
            presentation_manager,
            element_comparator,
            long_edge_size,
            visible_part_size,
            edge_with_arrow_size,
        )
    }

    /// The `@VisibleForTesting` constructor with explicit sizes.
    pub fn with_sizes(
        graph: Arc<dyn LinearGraph>,
        presentation_manager: P,
        element_comparator: impl Fn(&GraphElement, &GraphElement) -> Ordering + Send + Sync + 'static,
        long_edge_size: i32,
        visible_part_size: i32,
        edge_with_arrow_size: i32,
    ) -> PrintElementGeneratorImpl<P> {
        PrintElementGeneratorImpl {
            edges_in_row_generator: EdgesInRowGenerator::new(graph.clone()),
            linear_graph: graph,
            presentation_manager,
            element_comparator: Box::new(element_comparator),
            long_edge_size,
            visible_part_size,
            edge_with_arrow_size,
            cache: RefCell::new(BoundedCache::new(CACHE_SIZE, CACHE_SIZE * 2)),
            recommended_width: OnceCell::new(),
        }
    }

    pub fn presentation_manager(&self) -> &P {
        &self.presentation_manager
    }

    pub fn presentation_manager_mut(&mut self) -> &mut P {
        &mut self.presentation_manager
    }

    /// Weighted mean plus standard deviation of the row widths of the first rows (computed once).
    pub fn recommended_width(&self) -> i32 {
        *self
            .recommended_width
            .get_or_init(|| self.calculate_recommended_width())
    }

    fn calculate_recommended_width(&self) -> i32 {
        let nodes_count = self.linear_graph.nodes_count();
        if nodes_count == 0 {
            return 0;
        }
        if nodes_count == 1 {
            return 1;
        }

        let n = SAMPLE_SIZE.min(nodes_count);

        let mut sum = 0.0;
        let mut sum_squares = 0.0;
        let mut edges_count = 0;
        let mut current_normal_edges: HashSet<NormalEdge> = HashSet::new();

        for i in 0..n {
            let adjacent_edges = self.linear_graph.get_adjacent_edges(i, EdgeFilter::All);
            let mut up_arrows = 0;
            let mut down_arrows = 0;
            for e in &adjacent_edges {
                match as_normal_edge(e) {
                    Some(normal_edge) => {
                        if is_edge_up(e, i) {
                            current_normal_edges.remove(&normal_edge);
                        } else {
                            current_normal_edges.insert(normal_edge);
                        }
                    }
                    None => {
                        if e.edge_type == GraphEdgeType::DottedArrowUp {
                            up_arrows += 1;
                        } else {
                            down_arrows += 1;
                        }
                    }
                }
            }

            let mut new_edges_count = 0;
            for e in &current_normal_edges {
                if self.is_normal_edge_visible_in_row(e, i) {
                    new_edges_count += 1;
                } else {
                    match self.get_normal_arrow_type(e, i) {
                        Some(EdgePrintElementType::Down) => down_arrows += 1,
                        Some(EdgePrintElementType::Up) => up_arrows += 1,
                        None => {}
                    }
                }
            }

            // 0 <= K < 1; weight is an arithmetic progression, starting at 2 / (n * (k + 1)) and
            // ending at k * 2 / (n * (k + 1)); this ensures that the sum of all weights is 1
            let width = (edges_count + up_arrows).max(new_edges_count + down_arrows);
            let weight = 2.0 / (f64::from(n) * (K + 1.0))
                * (1.0 + (K - 1.0) * f64::from(i) / f64::from(n - 1));
            sum += f64::from(width) * weight;
            sum_squares += f64::from(width) * f64::from(width) * weight;

            edges_count = new_edges_count;
        }

        // weighted variance, see
        // http://stackoverflow.com/questions/30383270/how-do-i-calculate-the-standard-deviation-between-weighted-measurements
        let average = sum;
        // Clamped at 0: rounding can make the variance a tiny negative number (e.g. in a linear
        // history), and `sqrt` of that is NaN, which rounds to 0. Upstream has this bug.
        let deviation = (sum_squares - average * average).max(0.0).sqrt();
        // `Math.round`
        (average + deviation).round() as i32
    }

    fn create_end_position_function(
        &self,
        visible_row_index: VcsLogVisibleGraphIndex,
        up: bool,
    ) -> EndPositions {
        if visible_row_index < 0 || visible_row_index >= self.linear_graph.nodes_count() {
            return EndPositions {
                to_position: None,
                up,
            };
        }

        let visible_elements_in_next_row =
            self.get_sorted_visible_elements_in_row(visible_row_index);
        let to_position = visible_elements_in_next_row
            .into_iter()
            .enumerate()
            .map(|(position, element)| (element, position as i32))
            .collect();
        EndPositions {
            to_position: Some(to_position),
            up,
        }
    }

    fn get_arrow_type(
        &self,
        edge: &GraphEdge,
        row_index: VcsLogVisibleGraphIndex,
    ) -> Option<EdgePrintElementType> {
        if let Some(normal_edge) = as_normal_edge(edge) {
            return self.get_normal_arrow_type(&normal_edge, row_index);
        }
        // special edges
        match edge.edge_type {
            GraphEdgeType::DottedArrowDown | GraphEdgeType::NotLoadCommit => {
                if int_equal(edge.up_node_index, row_index - 1) {
                    return Some(EdgePrintElementType::Down);
                }
            }
            GraphEdgeType::DottedArrowUp => {
                // todo case 0-row arrow
                if int_equal(edge.down_node_index, row_index + 1) {
                    return Some(EdgePrintElementType::Up);
                }
            }
            // upstream logs "Unknown special edge type"; unreachable for normal edge types
            GraphEdgeType::Usual | GraphEdgeType::Dotted => {}
        }
        None
    }

    fn get_normal_arrow_type(
        &self,
        normal_edge: &NormalEdge,
        row_index: VcsLogVisibleGraphIndex,
    ) -> Option<EdgePrintElementType> {
        let edge_size = normal_edge.down - normal_edge.up;
        let up_offset = row_index - normal_edge.up;
        let down_offset = normal_edge.down - row_index;

        if edge_size >= self.long_edge_size {
            if up_offset == self.visible_part_size {
                // can not happen due to how constants are picked out, but just in case
                debug_assert!(
                    down_offset != self.visible_part_size,
                    "Both up and down arrow at row {row_index}"
                );
                return Some(EdgePrintElementType::Down);
            }
            if down_offset == self.visible_part_size {
                return Some(EdgePrintElementType::Up);
            }
        }
        if edge_size >= self.edge_with_arrow_size {
            if up_offset == 1 {
                debug_assert!(
                    down_offset != 1,
                    "Both up and down arrow at row {row_index}"
                );
                return Some(EdgePrintElementType::Down);
            }
            if down_offset == 1 {
                return Some(EdgePrintElementType::Up);
            }
        }
        None
    }

    /// Special edges are never "visible in row" (see `get_sorted_visible_elements_in_row`).
    fn is_edge_visible_in_row(&self, edge: &GraphEdge, visible_row_index: i32) -> bool {
        match as_normal_edge(edge) {
            Some(normal_edge) => {
                self.is_normal_edge_visible_in_row(&normal_edge, visible_row_index)
            }
            None => false,
        }
    }

    fn is_normal_edge_visible_in_row(
        &self,
        normal_edge: &NormalEdge,
        visible_row_index: i32,
    ) -> bool {
        normal_edge.down - normal_edge.up < self.long_edge_size
            || get_attachment_distance(normal_edge, visible_row_index) <= self.visible_part_size
    }

    fn get_sorted_visible_elements_in_row(
        &self,
        row_index: VcsLogVisibleGraphIndex,
    ) -> Vec<GraphElement> {
        if let Some(graph_elements) = self.cache.borrow().get(&row_index) {
            return graph_elements.clone();
        }

        let mut result = vec![GraphElement::Node(
            self.linear_graph.get_graph_node(row_index),
        )];

        result.extend(
            self.edges_in_row_generator
                .get_edges_in_row(row_index)
                .into_iter()
                .filter(|edge| self.is_edge_visible_in_row(edge, row_index))
                .map(GraphElement::Edge),
        );
        if row_index > 0 {
            result.extend(
                self.linear_graph
                    .get_adjacent_edges(row_index - 1, EdgeFilter::Special)
                    .into_iter()
                    .filter(|edge| is_edge_down(edge, row_index - 1))
                    .map(GraphElement::Edge),
            );
        }
        if row_index < self.linear_graph.nodes_count() - 1 {
            result.extend(
                self.linear_graph
                    .get_adjacent_edges(row_index + 1, EdgeFilter::Special)
                    .into_iter()
                    .filter(|edge| is_edge_up(edge, row_index + 1))
                    .map(GraphElement::Edge),
            );
        }

        java_sort::sort_by(&mut result, |a, b| (self.element_comparator)(a, b));
        self.cache.borrow_mut().put(row_index, result.clone());
        result
    }

    fn edge_element(
        &self,
        row_index: i32,
        position: i32,
        other_position: i32,
        edge_type: EdgePrintElementType,
        edge: GraphEdge,
        arrow_type: Option<EdgePrintElementType>,
    ) -> GraphPrintElement {
        GraphPrintElement::Edge(EdgePrintElementImpl::new(
            row_index,
            position,
            other_position,
            edge_type,
            edge,
            arrow_type == Some(edge_type),
            &self.presentation_manager,
        ))
    }
}

impl<P: PrintElementPresentationManager> PrintElementGenerator for PrintElementGeneratorImpl<P> {
    fn get_print_elements(&self, row_index: VcsLogVisibleGraphIndex) -> Vec<GraphPrintElement> {
        let mut result = Vec::new();
        // nodes at the end, to be drawn over the edges
        let mut nodes = Vec::new();

        let visible_elements = self.get_sorted_visible_elements_in_row(row_index);
        let up_position = self.create_end_position_function(row_index - 1, true);
        let down_position = self.create_end_position_function(row_index + 1, false);
        let graph = &*self.linear_graph;

        for (position, element) in visible_elements.into_iter().enumerate() {
            let position = position as i32;
            match element {
                GraphElement::Node(node) => {
                    nodes.push(GraphPrintElement::Node(SimplePrintElementImpl::new(
                        row_index,
                        position,
                        node,
                        &self.presentation_manager,
                    )));
                    for edge in graph.get_adjacent_edges(node.node_index, EdgeFilter::All) {
                        let arrow_type = self.get_arrow_type(&edge, row_index);
                        let down = down_position.get(&edge, graph);
                        let up = up_position.get(&edge, graph);
                        if let Some(down) = down {
                            let down_type = EdgePrintElementType::Down;
                            result.push(self.edge_element(
                                row_index, position, down, down_type, edge, arrow_type,
                            ));
                        }
                        if let Some(up) = up {
                            let up_type = EdgePrintElementType::Up;
                            result.push(
                                self.edge_element(
                                    row_index, position, up, up_type, edge, arrow_type,
                                ),
                            );
                        }
                    }
                }
                GraphElement::Edge(edge) => {
                    let down = down_position.get(&edge, graph);
                    let up = up_position.get(&edge, graph);
                    let arrow_type = self.get_arrow_type(&edge, row_index);

                    if let Some(down) = down {
                        let down_type = EdgePrintElementType::Down;
                        result.push(
                            self.edge_element(
                                row_index, position, down, down_type, edge, arrow_type,
                            ),
                        );
                    } else if arrow_type == Some(EdgePrintElementType::Down) {
                        result.push(GraphPrintElement::Edge(EdgePrintElementImpl::terminal(
                            row_index,
                            position,
                            EdgePrintElementType::Down,
                            edge,
                            &self.presentation_manager,
                        )));
                    }
                    if let Some(up) = up {
                        let up_type = EdgePrintElementType::Up;
                        result.push(
                            self.edge_element(row_index, position, up, up_type, edge, arrow_type),
                        );
                    } else if arrow_type == Some(EdgePrintElementType::Up) {
                        result.push(GraphPrintElement::Edge(EdgePrintElementImpl::terminal(
                            row_index,
                            position,
                            EdgePrintElementType::Up,
                            edge,
                            &self.presentation_manager,
                        )));
                    }
                }
            }
        }
        result.extend(nodes);
        result
    }
}

fn get_attachment_distance(e1: &NormalEdge, row_index: VcsLogVisibleGraphIndex) -> i32 {
    (row_index - e1.up).min(e1.down - row_index)
}
