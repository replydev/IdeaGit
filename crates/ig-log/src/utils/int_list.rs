//! A read-only list of `i32`.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntList.java`. Nothing
//! in the graph module uses it upstream (the `IntList`s there are fastutil's); it is kept for the
//! file-to-file mapping.

pub trait IntList {
    fn size(&self) -> i32;

    fn get(&self, index: i32) -> i32;
}
