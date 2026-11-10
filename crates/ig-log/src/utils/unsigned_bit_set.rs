//! A bit set indexed by any `i32`, negative indexes included (node ids of not-loaded commits are
//! negative).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/UnsignedBitSet.java`.
//! The two `java.util.BitSet`s become [`JavaBitSet`], a growable bit set with the same semantics
//! (bits beyond the end read `false`).

/// The subset of `java.util.BitSet` used here: a growable set of non-negative bit indexes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JavaBitSet {
    words: Vec<u64>,
}

impl JavaBitSet {
    pub fn new() -> JavaBitSet {
        JavaBitSet::default()
    }

    /// # Panics
    /// If `bit_index < 0` (upstream `IndexOutOfBoundsException`).
    pub fn get(&self, bit_index: i32) -> bool {
        assert!(bit_index >= 0, "bitIndex < 0: {bit_index}");
        let index = bit_index as usize;
        self.words
            .get(index / 64)
            .is_some_and(|word| word & (1 << (index % 64)) != 0)
    }

    /// # Panics
    /// If `bit_index < 0`.
    pub fn set(&mut self, bit_index: i32, value: bool) {
        assert!(bit_index >= 0, "bitIndex < 0: {bit_index}");
        let index = bit_index as usize;
        let word = index / 64;
        if value {
            if word >= self.words.len() {
                self.words.resize(word + 1, 0);
            }
            self.words[word] |= 1 << (index % 64);
        } else if let Some(w) = self.words.get_mut(word) {
            *w &= !(1 << (index % 64));
        }
    }

    /// Sets the bits `from_index..to_index` (exclusive end) to `value`.
    ///
    /// # Panics
    /// If `from_index < 0` or `from_index > to_index` (upstream `IndexOutOfBoundsException`).
    pub fn set_range(&mut self, from_index: i32, to_index: i32, value: bool) {
        assert!(from_index >= 0, "fromIndex < 0: {from_index}");
        assert!(
            from_index <= to_index,
            "fromIndex: {from_index} > toIndex: {to_index}"
        );
        if from_index == to_index {
            return;
        }
        let (from, to) = (from_index as usize, to_index as usize);
        if value {
            let words = to.div_ceil(64);
            if words > self.words.len() {
                self.words.resize(words, 0);
            }
        }
        for index in from..to.min(self.words.len() * 64) {
            let mask = 1 << (index % 64);
            if value {
                self.words[index / 64] |= mask;
            } else {
                self.words[index / 64] &= !mask;
            }
        }
    }
}

/// A bit set over all `i32` indexes: non-negative ones in one [`JavaBitSet`], negative ones
/// (negated) in another.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnsignedBitSet {
    positive_set: JavaBitSet,
    negative_set: JavaBitSet,
}

fn to_positive(bit_index: i32) -> i32 {
    -bit_index
}

impl UnsignedBitSet {
    pub fn new() -> UnsignedBitSet {
        UnsignedBitSet::default()
    }

    pub fn with_sets(positive_set: JavaBitSet, negative_set: JavaBitSet) -> UnsignedBitSet {
        UnsignedBitSet {
            positive_set,
            negative_set,
        }
    }

    /// # Panics
    /// For `i32::MIN`, whose negation overflows (upstream: `IndexOutOfBoundsException`).
    pub fn set(&mut self, bit_index: i32, value: bool) {
        if bit_index >= 0 {
            self.positive_set.set(bit_index, value);
        } else {
            self.negative_set.set(to_positive(bit_index), value);
        }
    }

    /// Sets `from_index..=to_index` (both boundaries included).
    ///
    /// # Panics
    /// If `from_index > to_index` (upstream `IndexOutOfBoundsException`).
    pub fn set_range(&mut self, from_index: i32, to_index: i32, value: bool) {
        assert!(
            from_index <= to_index,
            "fromIndex: {from_index} > toIndex: {to_index}"
        );
        if from_index >= 0 {
            self.positive_set.set_range(from_index, to_index + 1, value);
        } else if to_index < 0 {
            self.negative_set
                .set_range(to_positive(to_index), to_positive(from_index) + 1, value);
        } else {
            self.positive_set.set_range(0, to_index + 1, value);
            self.negative_set
                .set_range(to_positive(-1), to_positive(from_index) + 1, value);
        }
    }

    pub fn get(&self, bit_index: i32) -> bool {
        if bit_index >= 0 {
            self.positive_set.get(bit_index)
        } else {
            self.negative_set.get(to_positive(bit_index))
        }
    }
}
