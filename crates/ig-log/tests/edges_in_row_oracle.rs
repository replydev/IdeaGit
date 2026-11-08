//! IdeaGit tests, not upstream tests: compare `EdgesInRowGenerator` against a naive oracle on
//! pseudo-random linear graphs large enough to exercise block caching, multi-block walks, FIFO
//! eviction, `with_walk_size` and `invalidate` (the upstream fixtures have at most 17 rows).
//!
//! The generator returns the normal edges with `up < row < down`. Upstream only walks `walk_size`
//! rows from the block boundaries, so an edge spanning more than about `2 * walk_size` rows may be
//! missing for rows far from both ends. Both sides are therefore restricted to edges shorter than
//! the walk size, which are always visible.

mod common;

use std::collections::BTreeSet;
use std::sync::Arc;

use common::test_graph_builder::{TestLinearGraph, dot, graph, not_load, u};
use ig_log::api::EdgeFilter;
use ig_log::api::elements::GraphEdge;
use ig_log::api::linear_graph::LinearGraph;
use ig_log::print::edges_in_row_generator::EdgesInRowGenerator;

/// Minimal deterministic LCG (Knuth MMIX constants).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A graph of `n` nodes (node id == index): mostly a chain, plus short edges, long edges
/// (up to `max_long` rows), merges (several parents), dotted edges and not-loaded parents.
fn random_graph(n: i32, max_long: i32, seed: u64) -> TestLinearGraph {
    let mut rng = Lcg(seed);
    let mut parents: Vec<Vec<(i32, bool)>> = vec![Vec::new(); n as usize];
    let mut not_loaded: Vec<bool> = vec![false; n as usize];
    for i in 0..n {
        let mut targets: BTreeSet<i32> = BTreeSet::new();
        if i + 1 < n && rng.below(10) < 8 {
            targets.insert(i + 1);
        }
        if i + 2 < n && rng.below(10) < 3 {
            targets.insert((i + 2 + rng.below(6) as i32).min(n - 1));
        }
        if i + 10 < n && rng.below(10) < 2 {
            targets.insert((i + 10 + rng.below(40) as i32).min(n - 1));
        }
        if i + 50 < n && rng.below(100) < 4 {
            targets.insert((i + 50 + rng.below((max_long - 49) as u64) as i32).min(n - 1));
        }
        targets.remove(&i);
        parents[i as usize] = targets
            .into_iter()
            .map(|t| (t, rng.below(5) == 0))
            .collect();
        not_loaded[i as usize] = rng.below(25) == 0;
    }
    graph(|b| {
        for i in 0..n {
            let mut edges = Vec::new();
            for &(t, dotted) in &parents[i as usize] {
                edges.push(if dotted { dot(t) } else { u(t) });
            }
            if not_loaded[i as usize] {
                edges.push(not_load(Some(1_000_000 + i)));
            }
            b.node(i, edges);
        }
    })
}

/// All normal edges of the graph, found through the down-adjacency of every node.
fn all_normal_edges(g: &dyn LinearGraph) -> Vec<GraphEdge> {
    let mut edges = BTreeSet::new();
    for i in 0..g.nodes_count() {
        edges.extend(g.get_adjacent_edges(i, EdgeFilter::NormalDown));
    }
    edges.into_iter().collect()
}

fn oracle(edges: &[GraphEdge], row: i32, max_len: i32) -> BTreeSet<GraphEdge> {
    edges
        .iter()
        .filter(|e| {
            let (up, down) = (e.up_node_index.unwrap(), e.down_node_index.unwrap());
            up < row && row < down && down - up < max_len
        })
        .copied()
        .collect()
}

fn actual(gen_: &EdgesInRowGenerator, row: i32, max_len: i32) -> BTreeSet<GraphEdge> {
    gen_.get_edges_in_row(row)
        .into_iter()
        .filter(|e| e.up_node_index.unwrap() < row && row < e.down_node_index.unwrap())
        .filter(|e| e.down_node_index.unwrap() - e.up_node_index.unwrap() < max_len)
        .collect()
}

fn check_row(gen_: &EdgesInRowGenerator, edges: &[GraphEdge], row: i32, max_len: i32) {
    assert_eq!(
        oracle(edges, row, max_len),
        actual(gen_, row, max_len),
        "row {row}, window {max_len}"
    );
}

fn check_all_rows(g: &Arc<dyn LinearGraph>, gen_: &EdgesInRowGenerator, max_len: i32) {
    let edges = all_normal_edges(g.as_ref());
    for row in 0..g.nodes_count() {
        check_row(gen_, &edges, row, max_len);
    }
}

fn shared(g: TestLinearGraph) -> Arc<dyn LinearGraph> {
    Arc::new(g)
}

#[test]
fn graph_has_mixed_edges() {
    let g = random_graph(300, 150, 1);
    let edges = all_normal_edges(&g);
    let lens: Vec<i32> = edges
        .iter()
        .map(|e| e.down_node_index.unwrap() - e.up_node_index.unwrap())
        .collect();
    assert!(lens.contains(&1));
    assert!(lens.iter().any(|&l| l > 5 && l < 50));
    assert!(lens.iter().any(|&l| l >= 50));
    assert!((0..300).any(|i| !g.get_adjacent_edges(i, EdgeFilter::Special).is_empty()));
    assert!((0..300).any(|i| g.get_adjacent_edges(i, EdgeFilter::NormalDown).len() > 1));
}

#[test]
fn default_walk_size() {
    let g = shared(random_graph(1500, 400, 2));
    let gen_ = EdgesInRowGenerator::new(g.clone());
    check_all_rows(&g, &gen_, 1000);
}

#[test]
fn default_walk_size_small_graph_sees_everything() {
    let g = shared(random_graph(320, 250, 3));
    let gen_ = EdgesInRowGenerator::new(g.clone());
    check_all_rows(&g, &gen_, i32::MAX);
}

#[test]
fn walk_size_5() {
    let g = shared(random_graph(400, 150, 4));
    let gen_ = EdgesInRowGenerator::with_walk_size(g.clone(), 5);
    check_all_rows(&g, &gen_, 5);
}

#[test]
fn walk_size_50() {
    let g = shared(random_graph(500, 200, 5));
    let gen_ = EdgesInRowGenerator::with_walk_size(g.clone(), 50);
    check_all_rows(&g, &gen_, 50);
}

#[test]
fn random_access_across_more_than_cache_capacity_blocks() {
    let n = 1500;
    let g = shared(random_graph(n, 200, 6));
    let edges = all_normal_edges(g.as_ref());
    let gen_ = EdgesInRowGenerator::with_walk_size(g.clone(), 50);

    let mut rng = Lcg(77);
    let mut rows: Vec<i32> = (0..n).collect();
    for i in (1..rows.len()).rev() {
        rows.swap(i, rng.below(i as u64 + 1) as usize);
    }
    let blocks: BTreeSet<i32> = rows.iter().map(|r| r / 40).collect();
    assert!(blocks.len() > 30, "only {} blocks", blocks.len());
    for &row in &rows {
        check_row(&gen_, &edges, row, 50);
    }
    // second pass in a different order: early blocks were evicted and are recomputed
    for &row in rows.iter().rev() {
        check_row(&gen_, &edges, row, 50);
    }
}

#[test]
fn invalidate_gives_same_results() {
    let g = shared(random_graph(600, 200, 7));
    let edges = all_normal_edges(g.as_ref());
    let gen_ = EdgesInRowGenerator::with_walk_size(g.clone(), 50);
    let before: Vec<_> = (0..600).map(|r| gen_.get_edges_in_row(r)).collect();
    gen_.invalidate();
    let after: Vec<_> = (0..600).map(|r| gen_.get_edges_in_row(r)).collect();
    assert_eq!(before, after);
    for row in 0..600 {
        check_row(&gen_, &edges, row, 50);
    }
}

/// Edges up to `2 * walk_size` rows long are visible through the combination of the up walk from
/// the block start and the down walk from the next block start, so this exercises the down walk.
/// The last block has no down neighbour (upstream returns an empty set), so it is skipped.
fn check_double_window(n: i32, walk: i32, seed: u64) {
    let g = shared(random_graph(n, 3 * walk, seed));
    let edges = all_normal_edges(g.as_ref());
    let gen_ = EdgesInRowGenerator::with_walk_size(g.clone(), walk);
    let mut checked_long = 0;
    for row in 0..n {
        if (row / 40) * 40 + 40 >= n {
            continue;
        }
        check_row(&gen_, &edges, row, 2 * walk + 1);
        checked_long += oracle(&edges, row, 2 * walk + 1)
            .iter()
            .filter(|e| e.down_node_index.unwrap() - e.up_node_index.unwrap() >= walk)
            .count();
    }
    assert!(
        checked_long > 0,
        "no edge longer than the walk size was checked"
    );
}

#[test]
fn edges_up_to_twice_the_walk_size_walk_size_50() {
    check_double_window(700, 50, 8);
}

#[test]
fn edges_up_to_twice_the_walk_size_walk_size_5() {
    check_double_window(400, 5, 9);
}
