//! Read-only [`Flags`] backed by a predicate.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/PredicateFlags.java`.

use crate::utils::flags::Flags;

pub struct PredicateFlags<P> {
    visible: P,
    size: i32,
}

impl<P: Fn(i32) -> bool> PredicateFlags<P> {
    pub fn new(visible: P, size: i32) -> PredicateFlags<P> {
        PredicateFlags { visible, size }
    }
}

impl<P: Fn(i32) -> bool> Flags for PredicateFlags<P> {
    fn size(&self) -> i32 {
        self.size
    }

    fn get(&self, index: i32) -> bool {
        (self.visible)(index)
    }

    /// # Panics
    /// Always (upstream `UnsupportedOperationException`).
    fn set(&mut self, _index: i32, _value: bool) {
        panic!("Modification is not supported");
    }

    /// # Panics
    /// Always (upstream `UnsupportedOperationException`).
    fn set_all(&mut self, _value: bool) {
        panic!("Modification is not supported");
    }
}
