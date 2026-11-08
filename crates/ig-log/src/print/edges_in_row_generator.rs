//! Finds the normal edges passing through a row (neither starting nor ending in it).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/EdgesInRowGenerator.java`.
//!
//! Rows are grouped in blocks of 40; the edges at block boundaries are computed by walking up to
//! `walk_size` rows and cached. Deviations: sets are `BTreeSet`s (deterministic iteration; upstream
//! `HashSet` order depends on identity hash codes), and the cache is a [`BoundedCache`].
//!
//! **Upstream limitation:** an edge spanning more than about `2 * walk_size` rows is not reported
//! for rows far from both of its ends. Rendering is unaffected because the middles of long edges
//! are hidden, but callers should not use [`EdgesInRowGenerator::get_edges_in_row`] as an exact
//! edge index.
//!
//! **Caching:** block results are cached FIFO by insertion age (not LRU). Callers must call
//! [`EdgesInRowGenerator::invalidate`] after the graph changes; Plan 2b's mutating controllers
//! must clear this cache and the print element generator's row cache together.
//!
//! **Thread safety:** `Send` but not `Sync` (the caches live in `RefCell`s).

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;

use crate::api::EdgeFilter;
use crate::api::elements::GraphEdge;
use crate::api::linear_graph::LinearGraph;
use crate::utils::bounded_cache::BoundedCache;

const CACHE_SIZE: usize = 10;
const BLOCK_SIZE: i32 = 40;

/// Edges crossing `row`.
#[derive(Clone, Debug)]
struct GraphEdges {
    edges: BTreeSet<GraphEdge>,
    row: i32,
}

impl GraphEdges {
    fn empty(row: i32) -> GraphEdges {
        GraphEdges {
            edges: BTreeSet::new(),
            row,
        }
    }
}

pub struct EdgesInRowGenerator {
    walk_size: i32,
    graph: Arc<dyn LinearGraph>,
    cache_nu: RefCell<BoundedCache<i32, GraphEdges>>,
    cache_nd: RefCell<BoundedCache<i32, GraphEdges>>,
}

impl EdgesInRowGenerator {
    pub fn new(graph: Arc<dyn LinearGraph>) -> EdgesInRowGenerator {
        EdgesInRowGenerator::with_walk_size(graph, 1000)
    }

    pub fn with_walk_size(graph: Arc<dyn LinearGraph>, walk_size: i32) -> EdgesInRowGenerator {
        EdgesInRowGenerator {
            walk_size,
            graph,
            cache_nu: RefCell::new(BoundedCache::new(CACHE_SIZE, CACHE_SIZE * 2)),
            cache_nd: RefCell::new(BoundedCache::new(CACHE_SIZE, CACHE_SIZE * 2)),
        }
    }

    pub fn get_edges_in_row(&self, row_index: i32) -> BTreeSet<GraphEdge> {
        let mut neighbor_u = self.get_neighbor_u(row_index);
        while neighbor_u.row < row_index {
            neighbor_u = self.one_down_step(neighbor_u);
        }

        let mut neighbor_d = self.get_neighbor_d(row_index);
        while neighbor_d.row > row_index {
            neighbor_d = self.one_up_step(neighbor_d);
        }

        let mut result = neighbor_u.edges;
        result.extend(neighbor_d.edges);
        result
    }

    pub fn invalidate(&self) {
        self.cache_nu.borrow_mut().clear();
        self.cache_nd.borrow_mut().clear();
    }

    fn get_neighbor_u(&self, row_index: i32) -> GraphEdges {
        let up_neighbor_index = get_up_neighbor_index(row_index);
        if let Some(graph_edges) = self.cache_nu.borrow().get(&up_neighbor_index) {
            return graph_edges.clone();
        }
        let graph_edges = self.get_u_correct_edges(up_neighbor_index);
        self.cache_nu
            .borrow_mut()
            .put(up_neighbor_index, graph_edges.clone());
        graph_edges
    }

    fn get_neighbor_d(&self, row_index: i32) -> GraphEdges {
        let down_neighbor_index = get_up_neighbor_index(row_index) + BLOCK_SIZE;

        if down_neighbor_index >= self.graph.nodes_count() {
            return GraphEdges::empty(self.graph.nodes_count() - 1);
        }

        if let Some(graph_edges) = self.cache_nd.borrow().get(&down_neighbor_index) {
            return graph_edges.clone();
        }
        let graph_edges = self.get_d_correct_edges(down_neighbor_index);
        self.cache_nd
            .borrow_mut()
            .put(down_neighbor_index, graph_edges.clone());
        graph_edges
    }

    fn get_u_correct_edges(&self, row_index: i32) -> GraphEdges {
        let start_calculate_index = (row_index - self.walk_size).max(0);
        let mut graph_edges = GraphEdges::empty(start_calculate_index);

        for _ in start_calculate_index..row_index {
            graph_edges = self.one_down_step(graph_edges);
        }
        graph_edges
    }

    fn get_d_correct_edges(&self, row_index: i32) -> GraphEdges {
        let end_calculate_index = (row_index + self.walk_size).min(self.graph.nodes_count() - 1);
        let mut graph_edges = GraphEdges::empty(end_calculate_index);

        for _ in (row_index + 1..=end_calculate_index).rev() {
            graph_edges = self.one_up_step(graph_edges);
        }
        graph_edges
    }

    fn one_down_step(&self, graph_edges: GraphEdges) -> GraphEdges {
        let GraphEdges {
            mut edges,
            row: current_row,
        } = graph_edges;

        edges.extend(
            self.graph
                .get_adjacent_edges(current_row, EdgeFilter::NormalDown),
        );
        for edge in self
            .graph
            .get_adjacent_edges(current_row + 1, EdgeFilter::NormalUp)
        {
            edges.remove(&edge);
        }

        GraphEdges {
            edges,
            row: current_row + 1,
        }
    }

    fn one_up_step(&self, graph_edges: GraphEdges) -> GraphEdges {
        let GraphEdges {
            mut edges,
            row: current_row,
        } = graph_edges;

        edges.extend(
            self.graph
                .get_adjacent_edges(current_row, EdgeFilter::NormalUp),
        );
        for edge in self
            .graph
            .get_adjacent_edges(current_row - 1, EdgeFilter::NormalDown)
        {
            edges.remove(&edge);
        }

        GraphEdges {
            edges,
            row: current_row - 1,
        }
    }
}

fn get_up_neighbor_index(row_index: i32) -> i32 {
    (row_index / BLOCK_SIZE) * BLOCK_SIZE
}
