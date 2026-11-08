//! A multimap from `i32` to sets of `i32`, keeping values in insertion order.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntIntMultiMap.java`.

use std::collections::HashMap;

/// Each key maps to a small array of distinct values (duplicates are ignored on insert).
#[derive(Clone, Debug, Default)]
pub struct IntIntMultiMap {
    key_to_array_map: HashMap<i32, Vec<i32>>,
}

impl IntIntMultiMap {
    pub fn new() -> IntIntMultiMap {
        IntIntMultiMap::default()
    }

    pub fn put_value(&mut self, key: i32, value: i32) {
        let values = self.key_to_array_map.entry(key).or_default();
        if !values.contains(&value) {
            values.push(value);
        }
    }

    /// Removes `value` from `key`; drops the key when no values are left.
    pub fn remove(&mut self, key: i32, value: i32) {
        let Some(values) = self.key_to_array_map.get_mut(&key) else {
            return;
        };
        let Some(remove_index) = values.iter().position(|&v| v == value) else {
            return;
        };
        if values.len() == 1 {
            self.key_to_array_map.remove(&key);
            return;
        }
        values.remove(remove_index);
    }

    /// Values of `key` in insertion order (empty if none).
    pub fn get(&self, key: i32) -> &[i32] {
        self.get_as_array(key)
    }

    pub fn get_as_array(&self, key: i32) -> &[i32] {
        self.key_to_array_map
            .get(&key)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn is_empty(&self) -> bool {
        self.key_to_array_map.is_empty()
    }

    pub fn contains_key(&self, key: i32) -> bool {
        self.key_to_array_map.contains_key(&key)
    }

    /// Keys in unspecified order; callers must not depend on it.
    pub fn keys(&self) -> Vec<i32> {
        self.key_to_array_map.keys().copied().collect()
    }

    pub fn clear(&mut self) {
        self.key_to_array_map.clear();
    }
}
