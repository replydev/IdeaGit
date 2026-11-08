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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_oldest_first_and_stays_within_capacity() {
        let mut cache = BoundedCache::new(1, 2); // capacity 3
        for k in 0..3 {
            cache.put(k, k * 10);
        }
        assert_eq!(cache.get(&0), Some(&0));
        // reads do not refresh age: 0 is still the first to go
        cache.put(3, 30);
        assert_eq!(cache.get(&0), None);
        assert_eq!(cache.get(&1), Some(&10));
        cache.put(4, 40);
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), Some(&20));
        assert_eq!(cache.map.len(), 3);
        assert_eq!(cache.order.len(), 3);
        for k in 5..100 {
            cache.put(k, k);
            assert!(cache.map.len() <= 3 && cache.order.len() <= 3);
        }
        assert_eq!(cache.get(&99), Some(&99));
        assert_eq!(cache.get(&97), Some(&97));
        assert_eq!(cache.get(&96), None);
    }

    #[test]
    fn reput_replaces_value_without_growing_or_evicting() {
        let mut cache = BoundedCache::new(1, 1); // capacity 2
        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("a", 3);
        cache.put("b", 4);
        assert_eq!(cache.order.len(), 2);
        assert_eq!(cache.get(&"a"), Some(&3));
        assert_eq!(cache.get(&"b"), Some(&4));
        // re-put did not refresh age: "a" is still the oldest
        cache.put("c", 5);
        assert_eq!(cache.get(&"a"), None);
        assert_eq!(cache.get(&"b"), Some(&4));
        assert_eq!(cache.get(&"c"), Some(&5));
    }

    #[test]
    fn clear_empties_the_cache() {
        let mut cache = BoundedCache::new(1, 1);
        cache.put(1, 1);
        cache.put(2, 2);
        cache.clear();
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&2), None);
        assert!(cache.map.is_empty() && cache.order.is_empty());
        cache.put(3, 3);
        assert_eq!(cache.get(&3), Some(&3));
    }

    #[test]
    fn get_missing_key_is_none() {
        let cache: BoundedCache<i32, i32> = BoundedCache::new(1, 1);
        assert_eq!(cache.get(&7), None);
    }
}
