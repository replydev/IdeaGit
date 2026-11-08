//! A small bounded cache replacing IntelliJ's `SLRUMap` in the print generators.
//!
//! IdeaGit helper standing in for `com.intellij.util.containers.SLRUMap` as used by
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/EdgesInRowGenerator.java` and
//! `PrintElementGeneratorImpl.kt`. Deviation: eviction is first-in-first-out instead of segmented
//! LRU. The caches only memoise pure computations, so results are unaffected.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

#[derive(Clone, Debug)]
pub struct BoundedCache<K, V> {
    capacity: usize,
    map: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K: Eq + Hash + Clone, V> BoundedCache<K, V> {
    /// `SLRUMap(protectedQueueSize, probationalQueueSize)` holds up to the sum of both.
    pub fn new(protected_size: usize, probational_size: usize) -> BoundedCache<K, V> {
        BoundedCache {
            capacity: (protected_size + probational_size).max(1),
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    pub fn put(&mut self, key: K, value: V) {
        if self.map.insert(key.clone(), value).is_none() {
            self.order.push_back(key);
            while self.order.len() > self.capacity {
                if let Some(oldest) = self.order.pop_front() {
                    self.map.remove(&oldest);
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}
