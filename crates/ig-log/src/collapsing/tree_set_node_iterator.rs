//! A sorted work list of nodes: going up pops the greatest node first, going down the smallest.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/TreeSetNodeIterator.java`.

use std::collections::BTreeSet;

pub struct TreeSetNodeIterator {
    walk_nodes: BTreeSet<i32>,
    is_up: bool,
}

impl TreeSetNodeIterator {
    pub fn new(start_node: i32, is_up: bool) -> TreeSetNodeIterator {
        TreeSetNodeIterator {
            walk_nodes: BTreeSet::from([start_node]),
            is_up,
        }
    }

    /// # Panics
    /// If empty (upstream `NoSuchElementException`).
    pub fn pop(&mut self) -> i32 {
        let next = if self.is_up {
            self.walk_nodes.pop_last()
        } else {
            self.walk_nodes.pop_first()
        };
        next.expect("TreeSetNodeIterator is empty")
    }

    pub fn not_empty(&self) -> bool {
        !self.walk_nodes.is_empty()
    }

    pub fn add_all(&mut self, nodes: &[i32]) {
        self.walk_nodes.extend(nodes.iter().copied());
    }
}
