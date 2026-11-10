//! Maps between a "long" index range (all nodes) and a "short" one (the visible nodes, in order).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntToIntMap.java`,
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/UpdatableIntToIntMap.java`,
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/AbstractIntToIntMap.java` and
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/IDIntToIntMap.java`.
//!
//! Deviation: `AbstractIntToIntMap` is a set of functions ([`abstract_get_short_index`] and the
//! `check_*` helpers) that implementations call. Upstream's exceptions are panics.

/// A monotonic map from short indexes to long indexes.
pub trait IntToIntMap {
    fn short_size(&self) -> i32;

    fn long_size(&self) -> i32;

    /// Must be very fast.
    fn get_long_index(&self, short_index: i32) -> i32;

    /// The max short index whose long index is `<= long_index`, or 0.
    fn get_short_index(&self, long_index: i32) -> i32;
}

/// An [`IntToIntMap`] whose visibility can change; [`update`](Self::update) recomputes a range.
pub trait UpdatableIntToIntMap: IntToIntMap {
    fn update(&mut self, start_long_index: i32, end_long_index: i32);
}

/// `AbstractIntToIntMap.getShortIndex`: binary search over [`IntToIntMap::get_long_index`].
///
/// # Panics
/// If `long_index` is out of `0..long_size()`.
pub fn abstract_get_short_index(map: &(impl IntToIntMap + ?Sized), long_index: i32) -> i32 {
    check_long_index(map, long_index);

    if map.short_size() == 0 || map.get_long_index(0) > long_index {
        return 0;
    }
    let mut a = 0;
    let mut b = map.short_size() - 1;
    while b > a + 1 {
        let middle = (a + b) / 2;
        if map.get_long_index(middle) <= long_index {
            a = middle;
        } else {
            b = middle;
        }
    }
    if map.get_long_index(b) <= long_index {
        b
    } else {
        a
    }
}

/// # Panics
/// If `long_index` is out of `0..long_size()` (upstream `IndexOutOfBoundsException`).
pub fn check_long_index(map: &(impl IntToIntMap + ?Sized), long_index: i32) {
    if long_index < 0 || long_index >= map.long_size() {
        panic!(
            "LongSize is: {}, but longIndex: {long_index}",
            map.long_size()
        );
    }
}

/// # Panics
/// If `short_index` is out of `0..short_size()` (upstream `IndexOutOfBoundsException`).
pub fn check_short_index(map: &(impl IntToIntMap + ?Sized), short_index: i32) {
    if short_index < 0 || short_index >= map.short_size() {
        panic!(
            "ShortSize is: {}, but shortIndex: {short_index}",
            map.short_size()
        );
    }
}

/// # Panics
/// Unless `0 <= start_long_index <= end_long_index < long_size()` (upstream
/// `IllegalArgumentException`).
pub fn check_update_parameters(
    map: &(impl IntToIntMap + ?Sized),
    start_long_index: i32,
    end_long_index: i32,
) {
    if start_long_index < 0
        || end_long_index < start_long_index
        || end_long_index >= map.long_size()
    {
        panic!(
            "ShortSize is: {}, but updateRequest is: ({start_long_index}, {end_long_index})",
            map.short_size()
        );
    }
}

/// The identity map of `size` indexes (`IDIntToIntMap`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdIntToIntMap {
    size: i32,
}

impl IdIntToIntMap {
    pub fn new(size: i32) -> IdIntToIntMap {
        IdIntToIntMap { size }
    }

    /// `IDIntToIntMap.EMPTY`: the empty map, whose `update` does nothing.
    pub const EMPTY: IdIntToIntMap = IdIntToIntMap { size: 0 };
}

impl IntToIntMap for IdIntToIntMap {
    fn short_size(&self) -> i32 {
        self.size
    }

    fn long_size(&self) -> i32 {
        self.size
    }

    fn get_long_index(&self, short_index: i32) -> i32 {
        short_index
    }

    fn get_short_index(&self, long_index: i32) -> i32 {
        long_index
    }
}

/// Only [`IdIntToIntMap::EMPTY`] is updatable upstream (`EmptyIDIntToIntMap`); updating does
/// nothing.
impl UpdatableIntToIntMap for IdIntToIntMap {
    fn update(&mut self, _start_long_index: i32, _end_long_index: i32) {
        debug_assert_eq!(self.size, 0, "only the empty IDIntToIntMap is updatable");
    }
}
