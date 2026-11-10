//! An updatable [`IntToIntMap`] over a visibility predicate, backed by a segment tree of counts.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/TreeIntToIntMap.java`.
//!
//! Deviation: `new_instance` always returns a `TreeIntToIntMap`. For `long_size == 0` it behaves
//! like `IDIntToIntMap.EMPTY`, which upstream returns instead.

use crate::utils::int_to_int_map::{
    IntToIntMap, UpdatableIntToIntMap, abstract_get_short_index, check_short_index,
    check_update_parameters,
};

pub struct TreeIntToIntMap<P> {
    this_is_visible: P,
    long_size: i32,
    count_levels: i32,
    tree: Vec<i32>,
}

fn count_digits(mut long_size: i32) -> i32 {
    let mut count = 0;
    while long_size != 0 {
        count += 1;
        long_size >>= 1;
    }
    count
}

impl<P: Fn(i32) -> bool> TreeIntToIntMap<P> {
    /// # Panics
    /// If `long_size < 0` (upstream `NegativeArraySizeException`).
    pub fn new_instance(this_is_visible: P, long_size: i32) -> TreeIntToIntMap<P> {
        assert!(long_size >= 0, "size < 0: {long_size}");

        if long_size == 0 {
            return TreeIntToIntMap {
                this_is_visible,
                long_size,
                count_levels: 0,
                tree: Vec::new(),
            };
        }

        // longSize -> countLevels: 1..2 -> 2; 3..4 -> 3;  5..8 -> 4
        let count_levels = if long_size == 1 {
            2
        } else {
            count_digits(long_size - 1) + 1
        };

        let empty_tree = vec![0; 1 << (count_levels - 1)];
        let mut int_to_int_map = TreeIntToIntMap {
            this_is_visible,
            long_size,
            count_levels,
            tree: empty_tree,
        };
        int_to_int_map.update(0, long_size - 1);
        int_to_int_map
    }

    fn is_empty_map(&self) -> bool {
        self.tree.is_empty()
    }

    fn tree_len(&self) -> i32 {
        self.tree.len() as i32
    }

    fn is_last_level(&self, node: i32) -> bool {
        node >= self.tree_len()
    }

    fn update_node_count(&mut self, node: i32) -> i32 {
        if self.is_last_level(node) {
            return self.get_count_in_last_level(node);
        }

        let child = node << 1;
        let count = self.update_node_count(child) + self.update_node_count(child + 1);
        self.tree[node as usize] = count;
        count
    }

    fn get_count_in_last_level(&self, node: i32) -> i32 {
        let node = node - self.tree_len();
        if node < self.long_size && (self.this_is_visible)(node) {
            1
        } else {
            0
        }
    }

    fn get_count_in_node(&self, node: i32) -> i32 {
        if self.is_last_level(node) {
            self.get_count_in_last_level(node)
        } else {
            self.tree[node as usize]
        }
    }
}

impl<P: Fn(i32) -> bool> IntToIntMap for TreeIntToIntMap<P> {
    fn short_size(&self) -> i32 {
        if self.is_empty_map() {
            return 0;
        }
        self.tree[1]
    }

    fn long_size(&self) -> i32 {
        self.long_size
    }

    fn get_long_index(&self, mut short_index: i32) -> i32 {
        if self.is_empty_map() {
            return short_index;
        }
        check_short_index(self, short_index);

        let mut node = 1;
        for _ in 0..self.count_levels - 1 {
            let child = node << 1;
            let count_in_child_node = self.get_count_in_node(child);
            if count_in_child_node > short_index {
                node = child;
            } else {
                node = child + 1;
                short_index -= count_in_child_node;
            }
        }
        node - self.tree_len()
    }

    fn get_short_index(&self, long_index: i32) -> i32 {
        if self.is_empty_map() {
            return long_index;
        }
        abstract_get_short_index(self, long_index)
    }
}

impl<P: Fn(i32) -> bool> UpdatableIntToIntMap for TreeIntToIntMap<P> {
    fn update(&mut self, start_long_index: i32, end_long_index: i32) {
        if self.is_empty_map() {
            return;
        }
        check_update_parameters(self, start_long_index, end_long_index);

        let start_node = start_long_index + self.tree_len();
        let end_node = end_long_index + self.tree_len();
        let common_node = start_node >> count_digits(start_node ^ end_node);
        self.update_node_count(common_node);

        let mut parent = common_node >> 1;
        while parent != 0 {
            self.tree[parent as usize] =
                self.get_count_in_node(parent << 1) + self.get_count_in_node((parent << 1) + 1);
            parent >>= 1;
        }
    }
}
