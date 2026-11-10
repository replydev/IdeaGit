//! An updatable [`IntToIntMap`] over visibility [`Flags`], with cached visible counts per block.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/ListIntToIntMap.java`.
//!
//! Deviations:
//! - The map owns its flags and exposes them ([`ListIntToIntMap::flags`] /
//!   [`ListIntToIntMap::flags_mut`]): upstream keeps a reference to flags that its owner mutates.
//! - `new_instance` always returns a `ListIntToIntMap`. For `long_size == 0` it behaves like
//!   `IDIntToIntMap.EMPTY`, which upstream returns instead: no size checks, identity lookups and a
//!   no-op `update`.

use crate::utils::flags::Flags;
use crate::utils::int_to_int_map::{
    IntToIntMap, UpdatableIntToIntMap, check_long_index, check_short_index, check_update_parameters,
};
use crate::utils::predicate_flags::PredicateFlags;

/// `ListIntToIntMap.DEFAULT_BLOCK_SIZE`.
pub const DEFAULT_BLOCK_SIZE: i32 = 30;

pub struct ListIntToIntMap<F> {
    visibility: F,
    long_size: i32,
    block_size: i32,
    sub_sum_of_blocks: Vec<i32>,
}

impl<P: Fn(i32) -> bool> ListIntToIntMap<PredicateFlags<P>> {
    /// `newInstance(thisIsVisible, longSize)`.
    pub fn with_predicate(this_is_visible: P, long_size: i32) -> Self {
        ListIntToIntMap::new_instance(PredicateFlags::new(this_is_visible, long_size))
    }
}

impl<F: Flags> ListIntToIntMap<F> {
    /// `newInstance(visibleNodes)`, with [`DEFAULT_BLOCK_SIZE`].
    pub fn new_instance(visible_nodes: F) -> ListIntToIntMap<F> {
        ListIntToIntMap::with_block_size(visible_nodes, DEFAULT_BLOCK_SIZE)
    }

    /// `newInstance(visibility, blockSize)`. Memory usage is `long_size / block_size`;
    /// `get_long_index` needs `log(long_size) + block_size` steps, `get_short_index` `block_size`.
    ///
    /// # Panics
    /// If `visibility.size() < 0` (upstream `NegativeArraySizeException`).
    pub fn with_block_size(visibility: F, block_size: i32) -> ListIntToIntMap<F> {
        let long_size = visibility.size();
        assert!(long_size >= 0, "size < 0: {long_size}");

        if long_size == 0 {
            return ListIntToIntMap {
                visibility,
                long_size,
                block_size,
                sub_sum_of_blocks: Vec::new(),
            };
        }

        let sum_size = (long_size - 1) / block_size + 1;
        let mut list_int_to_int_map = ListIntToIntMap {
            visibility,
            long_size,
            block_size,
            sub_sum_of_blocks: vec![0; sum_size as usize],
        };
        list_int_to_int_map.update(0, long_size - 1);
        list_int_to_int_map
    }

    pub fn flags(&self) -> &F {
        &self.visibility
    }

    /// Changing the flags requires an [`update`](UpdatableIntToIntMap::update) of the changed
    /// range.
    pub fn flags_mut(&mut self) -> &mut F {
        &mut self.visibility
    }

    fn is_empty_map(&self) -> bool {
        self.sub_sum_of_blocks.is_empty()
    }

    fn get_relevant_sum_index(&self, long_index: i32) -> i32 {
        long_index / self.block_size
    }

    /// Sum of visible nodes before `block_index`, plus those of the block up to `last_long_index`.
    fn calculate_sum_for_block(&self, block_index: i32, last_long_index: i32) -> i32 {
        let mut sum = 0;
        if block_index > 0 {
            sum = self.sub_sum_of_blocks[(block_index - 1) as usize];
        }

        for long_index in block_index * self.block_size..=last_long_index {
            if self.visibility.get(long_index) {
                sum += 1;
            }
        }
        sum
    }
}

impl<F: Flags> IntToIntMap for ListIntToIntMap<F> {
    fn short_size(&self) -> i32 {
        self.sub_sum_of_blocks.last().copied().unwrap_or(0)
    }

    fn long_size(&self) -> i32 {
        self.long_size
    }

    fn get_long_index(&self, short_index: i32) -> i32 {
        if self.is_empty_map() {
            return short_index;
        }
        check_short_index(self, short_index);

        let block_index = last_insertion_point(&self.sub_sum_of_blocks, short_index);
        let mut pref_visible_count = 0;
        if block_index > 0 {
            pref_visible_count = self.sub_sum_of_blocks[block_index - 1];
        }

        for long_index in block_index as i32 * self.block_size..self.long_size {
            if self.visibility.get(long_index) {
                pref_visible_count += 1;
            }
            if pref_visible_count > short_index {
                return long_index;
            }
        }

        unreachable!("This should never happen!");
    }

    fn get_short_index(&self, long_index: i32) -> i32 {
        if self.is_empty_map() {
            return long_index;
        }
        check_long_index(self, long_index);

        let block_index = self.get_relevant_sum_index(long_index);
        let count_visible = self.calculate_sum_for_block(block_index, long_index);
        if count_visible > 0 {
            count_visible - 1
        } else {
            0
        }
    }
}

impl<F: Flags> UpdatableIntToIntMap for ListIntToIntMap<F> {
    fn update(&mut self, start_long_index: i32, end_long_index: i32) {
        if self.is_empty_map() {
            return;
        }
        check_update_parameters(self, start_long_index, end_long_index);
        let start_sum_index = self.get_relevant_sum_index(start_long_index);
        let end_sum_index = self.get_relevant_sum_index(end_long_index);
        let prev_end_sum = self.sub_sum_of_blocks[end_sum_index as usize];

        for block_index in start_sum_index..=end_sum_index {
            let end_index = self.long_size.min((block_index + 1) * self.block_size);
            self.sub_sum_of_blocks[block_index as usize] =
                self.calculate_sum_for_block(block_index, end_index - 1);
        }

        let sum_delta = self.sub_sum_of_blocks[end_sum_index as usize] - prev_end_sum;
        for block_index in (end_sum_index + 1) as usize..self.sub_sum_of_blocks.len() {
            self.sub_sum_of_blocks[block_index] += sum_delta;
        }
    }
}

/// The index of the first element greater than `key` in the sorted `array` (the last insertion
/// point; `Arrays.binarySearch` does not say which of several equal elements it finds).
fn last_insertion_point(array: &[i32], key: i32) -> usize {
    let mut l = 0;
    let mut u = array.len() - 1;
    while u > l {
        let middle = (l + u) / 2;
        if array[middle] <= key {
            l = middle + 1;
        } else {
            u = middle;
        }
    }
    debug_assert_eq!(l, u);
    l
}
