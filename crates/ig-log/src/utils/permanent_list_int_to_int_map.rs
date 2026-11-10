//! An immutable [`IntToIntMap`] over visibility [`Flags`], storing every `block_size`-th long index.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/PermanentListIntToIntMap.java`.
//!
//! Deviation: `new_instance` always returns a `PermanentListIntToIntMap`. For `short_size == 0` it
//! behaves like upstream's `createEmptyIntToIntMap` (no checks, every lookup returns 0).

use crate::utils::flags::Flags;
use crate::utils::int_to_int_map::{IntToIntMap, abstract_get_short_index, check_short_index};

/// `PermanentListIntToIntMap.DEFAULT_BLOCK_SIZE`.
pub const DEFAULT_BLOCK_SIZE: i32 = 30;

pub struct PermanentListIntToIntMap<F> {
    visible_indexes: F,
    long_size: i32,
    short_size: i32,
    block_size: i32,
    strong_short_indexes: Vec<i32>,
}

impl<F: Flags> PermanentListIntToIntMap<F> {
    /// `newInstance(visibleIndexes, shortSize)`, with [`DEFAULT_BLOCK_SIZE`].
    pub fn new_instance(visible_indexes: F, short_size: i32) -> PermanentListIntToIntMap<F> {
        PermanentListIntToIntMap::with_block_size(visible_indexes, short_size, DEFAULT_BLOCK_SIZE)
    }

    /// `newInstance(visibleIndexes, shortSize, blockSize)`; `short_size` must be the number of
    /// visible indexes.
    ///
    /// # Panics
    /// If `short_size < 0` (upstream `NegativeArraySizeException`).
    pub fn with_block_size(
        visible_indexes: F,
        short_size: i32,
        block_size: i32,
    ) -> PermanentListIntToIntMap<F> {
        assert!(short_size >= 0, "shortSize < 0: {short_size}");
        let long_size = visible_indexes.size();
        if short_size == 0 {
            return PermanentListIntToIntMap {
                visible_indexes,
                long_size,
                short_size,
                block_size,
                strong_short_indexes: Vec::new(),
            };
        }

        let mut strong_short_indexes = vec![0; ((short_size - 1) / block_size + 1) as usize];

        let mut current_short_index = -1;
        for long_index in 0..visible_indexes.size() {
            if visible_indexes.get(long_index) {
                current_short_index += 1;
                if current_short_index % block_size == 0 {
                    strong_short_indexes[(current_short_index / block_size) as usize] = long_index;
                }
            }
        }

        PermanentListIntToIntMap {
            visible_indexes,
            long_size,
            short_size,
            block_size,
            strong_short_indexes,
        }
    }
}

impl<F: Flags> IntToIntMap for PermanentListIntToIntMap<F> {
    fn short_size(&self) -> i32 {
        self.short_size
    }

    fn long_size(&self) -> i32 {
        self.long_size
    }

    fn get_long_index(&self, short_index: i32) -> i32 {
        if self.short_size == 0 {
            return 0;
        }
        check_short_index(self, short_index);

        let strong_index = short_index / self.block_size;

        let mut sub = short_index - strong_index * self.block_size;
        for long_index in self.strong_short_indexes[strong_index as usize]..self.long_size {
            if self.visible_indexes.get(long_index) {
                sub -= 1;
            }
            if sub == -1 {
                return long_index;
            }
        }
        panic!(
            "Not found long index for short index: {short_index}. Long & short size is: {}, {}.",
            self.long_size, self.short_size
        );
    }

    fn get_short_index(&self, long_index: i32) -> i32 {
        if self.short_size == 0 {
            return 0;
        }
        abstract_get_short_index(self, long_index)
    }
}
