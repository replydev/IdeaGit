//! Branch colors and head ordering for the log's permanent graph.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/graph/GraphColorManagerImpl.kt` and
//! `platform/vcs-log/impl/src/com/intellij/vcs/log/graph/HeadCommitsComparator.java`.
//!
//! Deviations: "no refs at head" is not logged; reference managers are told apart by `Arc`
//! identity (upstream `equals`) and ordered by [`VcsLogRefManager::name`] (upstream `toString`).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::data::provider::VcsLogRefManager;
use crate::data::refs_model::RefsModel;
use crate::data::root::VcsRoot;
use crate::data::storage::VcsLogCommitStorageIndex;
use crate::graph_color_manager::GraphColorManager;

/// `GraphColorManagerImpl.DEFAULT_COLOR`.
pub const DEFAULT_COLOR: i32 = 0;

/// `String.hashCode()` of Java: the color id of a branch is the hash code of its name.
pub fn java_string_hash_code(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

/// Colors a branch by the name of its most important reference (`GraphColorManagerImpl`).
pub struct GraphColorManagerImpl {
    refs_model: Arc<RefsModel>,
}

impl GraphColorManagerImpl {
    pub fn new(refs_model: Arc<RefsModel>) -> GraphColorManagerImpl {
        GraphColorManagerImpl { refs_model }
    }
}

impl GraphColorManager<VcsLogCommitStorageIndex> for GraphColorManagerImpl {
    fn get_color(
        &self,
        head_commit: &VcsLogCommitStorageIndex,
        head_fragment_index: i32,
        fragment_index: i32,
    ) -> i32 {
        if head_fragment_index == fragment_index {
            return match self.refs_model.get_ref_for_head_commit(*head_commit) {
                Some(first_ref) => java_string_hash_code(first_ref.name()),
                None => DEFAULT_COLOR,
            };
        }
        fragment_index
    }
}

/// Orders branch heads by their most important references (`HeadCommitsComparator`): heads
/// without references go last.
pub struct HeadCommitsComparator {
    refs_model: Arc<RefsModel>,
    ref_managers: BTreeMap<VcsRoot, Arc<dyn VcsLogRefManager>>,
}

impl HeadCommitsComparator {
    pub fn new(
        refs_model: Arc<RefsModel>,
        ref_managers: BTreeMap<VcsRoot, Arc<dyn VcsLogRefManager>>,
    ) -> HeadCommitsComparator {
        HeadCommitsComparator {
            refs_model,
            ref_managers,
        }
    }

    pub fn compare(
        &self,
        head1: &VcsLogCommitStorageIndex,
        head2: &VcsLogCommitStorageIndex,
    ) -> Ordering {
        if head1 == head2 {
            return Ordering::Equal;
        }
        let ref1 = self.refs_model.get_ref_for_head_commit(*head1);
        let ref2 = self.refs_model.get_ref_for_head_commit(*head2);
        let (ref1, ref2) = match (ref1, ref2) {
            (None, None) => return head1.cmp(head2),
            (None, Some(_)) => return Ordering::Greater,
            (Some(_), None) => return Ordering::Less,
            (Some(r1), Some(r2)) => (r1, r2),
        };
        let manager1 = &self.ref_managers[ref1.root()];
        let manager2 = &self.ref_managers[ref2.root()];
        if !Arc::ptr_eq(manager1, manager2) {
            return manager1.name().cmp(&manager2.name());
        }
        manager1.branch_layout_compare(&ref1, &ref2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_hash_code_matches_known_values() {
        assert_eq!(java_string_hash_code(""), 0);
        assert_eq!(java_string_hash_code("master"), -1081267614);
        assert_eq!(java_string_hash_code("main"), 3343801);
        assert_eq!(java_string_hash_code("origin/main"), 1685401346);
    }
}
