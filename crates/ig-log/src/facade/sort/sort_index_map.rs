//! A permutation of the permanent graph's rows: sorted index <-> usual index (node id).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/SortIndexMap.java`.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortIndexMap {
    map: Vec<i32>,
    reverse_map: Vec<i32>,
}

impl SortIndexMap {
    /// `list[sorted_index] == usual_index`; `list` must be a permutation of `0..list.len()`.
    ///
    /// # Panics
    /// If `list` contains an index out of `0..list.len()`.
    pub fn new(list: Vec<i32>) -> SortIndexMap {
        let mut reverse_map = vec![0; list.len()];
        for (i, &usual_index) in list.iter().enumerate() {
            reverse_map[usual_index as usize] = i as i32;
        }
        SortIndexMap {
            map: list,
            reverse_map,
        }
    }

    pub fn size(&self) -> i32 {
        self.map.len() as i32
    }

    /// `usual_index` is the node id.
    ///
    /// # Panics
    /// If `usual_index` is out of `0..size()`.
    pub fn get_sorted_index(&self, usual_index: i32) -> i32 {
        self.reverse_map[usual_index as usize]
    }

    /// # Panics
    /// If `sorted_index` is out of `0..size()`.
    pub fn get_usual_index(&self, sorted_index: i32) -> i32 {
        self.map[sorted_index as usize]
    }
}
