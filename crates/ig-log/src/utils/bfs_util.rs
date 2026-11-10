//! Breadth-first walks and searches.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/BfsUtil.kt`.
//!
//! Deviations:
//! - The visited [`Flags`] are passed to [`BfsWalk::step`] / [`BfsWalk::walk`] instead of being
//!   captured by the walk: `GraphUtil.subgraphDifference` uses visited flags that read the state of
//!   another walk, which Rust's borrows only allow when the flags are built per step. The upstream
//!   constructor without flags (a fresh `BitSetFlags`) is the caller's `BitSetFlags::new`.
//! - The overridable queue is a type parameter ([`BfsQueue`]): a FIFO [`VecDeque`] by default, or
//!   [`ReversePriorityQueue`] (`PriorityQueue(Collections.reverseOrder())`, used by linear BEK).
//! - [`BfsSearch`] is a struct holding a [`BfsWalk`] rather than a subclass.
//!
//! Upstream quirk kept: [`BfsSearch`]'s `count` only grows when a node is found, and finding a
//! node ends the search, so the `limit` never stops it.

use std::collections::{BinaryHeap, VecDeque};

use crate::api::linear_graph::{LiteLinearGraph, NodeFilter};
use crate::utils::flags::Flags;

/// The queue of a [`BfsWalk`] (upstream `Queue<Int>`).
pub trait BfsQueue {
    fn poll(&mut self) -> Option<i32>;

    fn add_all(&mut self, nodes: &[i32]);

    fn is_empty(&self) -> bool;

    fn contains(&self, node: i32) -> bool;

    fn min_node(&self) -> Option<i32>;

    fn max_node(&self) -> Option<i32>;
}

/// FIFO queue (`ContainerUtil.newLinkedList`).
impl BfsQueue for VecDeque<i32> {
    fn poll(&mut self) -> Option<i32> {
        self.pop_front()
    }

    fn add_all(&mut self, nodes: &[i32]) {
        self.extend(nodes.iter().copied());
    }

    fn is_empty(&self) -> bool {
        VecDeque::is_empty(self)
    }

    fn contains(&self, node: i32) -> bool {
        VecDeque::contains(self, &node)
    }

    fn min_node(&self) -> Option<i32> {
        self.iter().copied().min()
    }

    fn max_node(&self) -> Option<i32> {
        self.iter().copied().max()
    }
}

/// `PriorityQueue(Collections.reverseOrder())`: polls the greatest node first. Equal `i32`s are
/// indistinguishable, so the order matches Java's heap exactly.
#[derive(Clone, Debug, Default)]
pub struct ReversePriorityQueue(BinaryHeap<i32>);

impl ReversePriorityQueue {
    pub fn new(start: i32) -> ReversePriorityQueue {
        ReversePriorityQueue(BinaryHeap::from([start]))
    }
}

impl BfsQueue for ReversePriorityQueue {
    fn poll(&mut self) -> Option<i32> {
        self.0.pop()
    }

    fn add_all(&mut self, nodes: &[i32]) {
        self.0.extend(nodes.iter().copied());
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn contains(&self, node: i32) -> bool {
        self.0.iter().any(|&n| n == node)
    }

    fn min_node(&self) -> Option<i32> {
        self.0.iter().copied().min()
    }

    fn max_node(&self) -> Option<i32> {
        self.0.peek().copied()
    }
}

/// A breadth-first walk from `start`, down (to parents, smallest first) or up (to children,
/// greatest first).
pub struct BfsWalk<'g, G: LiteLinearGraph + ?Sized, Q: BfsQueue = VecDeque<i32>> {
    pub start: i32,
    graph: &'g G,
    down: bool,
    queue: Q,
}

impl<'g, G: LiteLinearGraph + ?Sized> BfsWalk<'g, G> {
    pub fn new(start: i32, graph: &'g G, down: bool) -> BfsWalk<'g, G> {
        BfsWalk::with_queue(start, graph, down, VecDeque::from([start]))
    }
}

impl<'g, G: LiteLinearGraph + ?Sized, Q: BfsQueue> BfsWalk<'g, G, Q> {
    /// A walk with a custom queue, which must contain `start`.
    pub fn with_queue(start: i32, graph: &'g G, down: bool, queue: Q) -> BfsWalk<'g, G, Q> {
        BfsWalk {
            start,
            graph,
            down,
            queue,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn current_nodes(&self) -> &Q {
        &self.queue
    }

    /// Visits the next unvisited node and queues its neighbours, which it returns. Returns an empty
    /// list if the queue ran out or `consumer` returned `false` for the node.
    pub fn step(
        &mut self,
        visited: &mut dyn Flags,
        mut consumer: impl FnMut(i32) -> bool,
    ) -> Vec<i32> {
        while let Some(node) = self.queue.poll() {
            if !visited.get(node) {
                visited.set(node, true);
                if !consumer(node) {
                    return Vec::new();
                }
                let mut next = if self.down {
                    self.graph.get_nodes(node, NodeFilter::Down)
                } else {
                    self.graph.get_nodes(node, NodeFilter::Up)
                };
                if self.down {
                    next.sort_unstable();
                } else {
                    next.sort_unstable_by(|a, b| b.cmp(a));
                }
                self.queue.add_all(&next);
                return next;
            }
        }
        Vec::new()
    }

    pub fn walk(&mut self, visited: &mut dyn Flags, mut consumer: impl FnMut(i32) -> bool) {
        while !self.is_finished() {
            self.step(visited, &mut consumer);
        }
    }
}

/// A breadth-first search for the first node that `consumer` maps to a value.
pub struct BfsSearch<'g, G: LiteLinearGraph + ?Sized, Q: BfsQueue = VecDeque<i32>> {
    walk: BfsWalk<'g, G, Q>,
    limit: i32,
    pub count: i32,
}

impl<'g, G: LiteLinearGraph + ?Sized, Q: BfsQueue> BfsSearch<'g, G, Q> {
    /// `limit` defaults to `graph.nodes_count()` upstream.
    pub fn new(walk: BfsWalk<'g, G, Q>, limit: i32) -> BfsSearch<'g, G, Q> {
        BfsSearch {
            walk,
            limit,
            count: 0,
        }
    }

    pub fn find<T>(
        &mut self,
        visited: &mut dyn Flags,
        mut consumer: impl FnMut(i32) -> Option<T>,
    ) -> Option<T> {
        self.count = 0;
        let mut result = None;
        let mut count = 0;
        while !(result.is_some() || self.walk.is_finished() || count > self.limit) {
            self.walk.step(visited, |node| match consumer(node) {
                None => true,
                Some(value) => {
                    result = Some(value);
                    count += 1;
                    false
                }
            });
        }
        self.count = count;
        result
    }
}
