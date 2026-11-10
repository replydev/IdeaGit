//! [`Flags`] backed by a hash set of the set indexes (cheap for sparse flags over a big range).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntHashSetFlags.kt`.

use std::collections::HashSet;

use crate::utils::flags::Flags;

#[derive(Clone, Debug, Default)]
pub struct IntHashSetFlags {
    size: i32,
    pub data: HashSet<i32>,
}

impl IntHashSetFlags {
    pub fn new(size: i32) -> IntHashSetFlags {
        IntHashSetFlags {
            size,
            data: HashSet::new(),
        }
    }
}

impl Flags for IntHashSetFlags {
    fn size(&self) -> i32 {
        self.size
    }

    fn get(&self, index: i32) -> bool {
        self.data.contains(&index)
    }

    fn set(&mut self, index: i32, value: bool) {
        if value {
            self.data.insert(index);
        } else {
            self.data.remove(&index);
        }
    }

    fn set_all(&mut self, value: bool) {
        if value {
            self.data.extend(0..self.size);
        } else {
            self.data.clear();
        }
    }
}
