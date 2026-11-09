//! Fixed-size boolean flags.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/Flags.java` and
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/BitSetFlags.java`.

use std::fmt;

/// A fixed number of boolean flags indexed `0..size()`.
pub trait Flags {
    fn size(&self) -> i32;

    fn get(&self, index: i32) -> bool;

    fn set(&mut self, index: i32, value: bool);

    fn set_all(&mut self, value: bool);
}

/// [`Flags`] backed by a bit set. All flags start `false`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct BitSetFlags {
    size: i32,
    words: Vec<u64>,
}

impl BitSetFlags {
    /// # Panics
    /// If `size < 0` (upstream `NegativeArraySizeException`).
    pub fn new(size: i32) -> BitSetFlags {
        assert!(size >= 0, "size < 0: {size}");
        BitSetFlags {
            size,
            words: vec![0; (size as usize).div_ceil(64)],
        }
    }

    pub fn with_default(size: i32, default_value: bool) -> BitSetFlags {
        let mut flags = BitSetFlags::new(size);
        if default_value {
            flags.set_all(true);
        }
        flags
    }

    fn check_range(&self, index: i32) {
        assert!(index >= 0, "index is {index} which is less than zero");
        assert!(
            index < self.size,
            "index is {index} and set size is {}",
            self.size
        );
    }
}

impl Flags for BitSetFlags {
    fn size(&self) -> i32 {
        self.size
    }

    /// # Panics
    /// If `index` is out of `0..size()` (upstream `IndexOutOfBoundsException`).
    fn get(&self, index: i32) -> bool {
        self.check_range(index);
        let index = index as usize;
        self.words[index / 64] & (1 << (index % 64)) != 0
    }

    /// # Panics
    /// If `index` is out of `0..size()`.
    fn set(&mut self, index: i32, value: bool) {
        self.check_range(index);
        let index = index as usize;
        if value {
            self.words[index / 64] |= 1 << (index % 64);
        } else {
            self.words[index / 64] &= !(1 << (index % 64));
        }
    }

    fn set_all(&mut self, value: bool) {
        let fill = if value { u64::MAX } else { 0 };
        self.words.iter_mut().for_each(|word| *word = fill);
        let tail = self.size as usize % 64;
        if value
            && tail != 0
            && let Some(last) = self.words.last_mut()
        {
            *last &= (1 << tail) - 1;
        }
    }
}

impl fmt::Debug for BitSetFlags {
    /// Like upstream `toString`: `{set indexes}, size = N`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let set: Vec<i32> = (0..self.size).filter(|&i| self.get(i)).collect();
        let set: Vec<String> = set.iter().map(i32::to_string).collect();
        write!(f, "{{{}}}, size = {}", set.join(", "), self.size)
    }
}
