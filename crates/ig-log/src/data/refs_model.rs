//! References indexed by commit.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/RefsModel.kt`,
//! `VcsLogRefsOfSingleRootFactory.kt` (`RootRefsModel`), and the interfaces
//! `platform/vcs-log/api/src/com/intellij/vcs/log/{VcsLogAggregatedStoredRefs,VcsLogRootStoredRefs}.kt`.
//!
//! Deviations: `get_ref_for_head_commit` also caches "no ref" results (upstream recomputes them);
//! the reference managers are passed directly instead of through the providers map.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::data::provider::VcsLogRefManager;
use crate::data::root::VcsRoot;
use crate::data::storage::{VcsLogCommitStorageIndex, VcsLogStorage};
use crate::data::vcs_ref::{VcsRef, VcsRefsContainer};

/// The references of one root, by commit index (`RootRefsModel`).
#[derive(Debug, Default)]
pub struct RootRefsModel {
    branches_mapping: HashMap<VcsLogCommitStorageIndex, Vec<VcsRef>>,
    tags_mapping: HashMap<VcsLogCommitStorageIndex, Vec<VcsRef>>,
}

impl RootRefsModel {
    /// `RootRefsModel.create`: all `refs` must belong to the same root.
    pub fn create(
        refs: impl IntoIterator<Item = VcsRef>,
        storage: &dyn VcsLogStorage,
    ) -> RootRefsModel {
        let mut model = RootRefsModel::default();
        let mut root: Option<VcsRoot> = None;
        for vcs_ref in refs {
            debug_assert!(
                root.as_ref().is_none_or(|r| r == vcs_ref.root()),
                "All references are supposed to be from the single root"
            );
            root = Some(vcs_ref.root().clone());
            let index = storage.get_commit_index(vcs_ref.commit_hash(), vcs_ref.root());
            let mapping = if vcs_ref.ref_type().is_branch() {
                &mut model.branches_mapping
            } else {
                &mut model.tags_mapping
            };
            mapping.entry(index).or_default().push(vcs_ref);
        }
        model
    }

    pub fn contains(&self, index: VcsLogCommitStorageIndex) -> bool {
        self.branches_mapping.contains_key(&index) || self.tags_mapping.contains_key(&index)
    }

    /// Branches first, then tags, each in insertion order.
    pub fn refs_to_commit(&self, index: VcsLogCommitStorageIndex) -> Vec<VcsRef> {
        let mut result = Vec::new();
        if let Some(refs) = self.branches_mapping.get(&index) {
            result.extend(refs.iter().cloned());
        }
        if let Some(refs) = self.tags_mapping.get(&index) {
            result.extend(refs.iter().cloned());
        }
        result
    }

    /// The commits that have references.
    pub fn get_refs_indexes(&self) -> HashSet<VcsLogCommitStorageIndex> {
        self.branches_mapping
            .keys()
            .chain(self.tags_mapping.keys())
            .copied()
            .collect()
    }
}

impl VcsRefsContainer for RootRefsModel {
    fn branches(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.branches_mapping.values().flatten())
    }

    fn tags(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.tags_mapping.values().flatten())
    }
}

/// The references of all roots (`RefsModel`, a `VcsLogAggregatedStoredRefs`).
pub struct RefsModel {
    refs_by_root: BTreeMap<VcsRoot, Arc<RootRefsModel>>,
    storage: Arc<dyn VcsLogStorage>,
    ref_managers: BTreeMap<VcsRoot, Arc<dyn VcsLogRefManager>>,
    single_root: Option<VcsRoot>,
    best_ref_for_head: Mutex<HashMap<VcsLogCommitStorageIndex, Option<VcsRef>>>,
    root_for_head: Mutex<HashMap<VcsLogCommitStorageIndex, Option<VcsRoot>>>,
}

impl RefsModel {
    /// `RefsModel.create`.
    pub fn create(
        refs: BTreeMap<VcsRoot, Arc<RootRefsModel>>,
        storage: Arc<dyn VcsLogStorage>,
        ref_managers: BTreeMap<VcsRoot, Arc<dyn VcsLogRefManager>>,
    ) -> RefsModel {
        let single_root = if refs.len() == 1 {
            refs.keys().next().cloned()
        } else {
            None
        };
        RefsModel {
            refs_by_root: refs,
            storage,
            ref_managers,
            single_root,
            best_ref_for_head: Mutex::new(HashMap::new()),
            root_for_head: Mutex::new(HashMap::new()),
        }
    }

    /// `RefsModel.createEmptyInstance`.
    pub fn create_empty_instance(storage: Arc<dyn VcsLogStorage>) -> RefsModel {
        RefsModel::create(BTreeMap::new(), storage, BTreeMap::new())
    }

    pub fn refs_by_root(&self) -> &BTreeMap<VcsRoot, Arc<RootRefsModel>> {
        &self.refs_by_root
    }

    /// `getRefForHeadCommit`: the most important reference of a branch head, by the root's
    /// branch layout comparator.
    pub fn get_ref_for_head_commit(&self, head_index: VcsLogCommitStorageIndex) -> Option<VcsRef> {
        let mut cache = self
            .best_ref_for_head
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = cache.get(&head_index) {
            return cached.clone();
        }
        let best = self.storage.get_commit_id(head_index).and_then(|id| {
            let manager = self.ref_managers.get(&id.root)?;
            // `minWithOrNull`: the first of equal minimums
            self.refs_to_commit(head_index)
                .into_iter()
                .reduce(|best, r| {
                    if manager.branch_layout_compare(&best, &r).is_gt() {
                        r
                    } else {
                        best
                    }
                })
        });
        cache.insert(head_index, best.clone());
        best
    }

    /// `getRootForHeadCommit`.
    pub fn get_root_for_head_commit(
        &self,
        head_index: VcsLogCommitStorageIndex,
    ) -> Option<VcsRoot> {
        if let Some(root) = &self.single_root {
            return Some(root.clone());
        }
        let mut cache = self.root_for_head.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .entry(head_index)
            .or_insert_with(|| self.storage.get_commit_id(head_index).map(|id| id.root))
            .clone()
    }

    /// `refsToCommit`: the references of a commit, branches before tags.
    pub fn refs_to_commit(&self, index: VcsLogCommitStorageIndex) -> Vec<VcsRef> {
        if let Some(root) = &self.single_root {
            return self.refs_by_root[root].refs_to_commit(index);
        }
        if self.refs_by_root.len() <= 10 {
            return self
                .refs_by_root
                .values()
                .find(|refs| refs.contains(index))
                .map(|refs| refs.refs_to_commit(index))
                .unwrap_or_default();
        }
        match self.storage.get_commit_id(index) {
            Some(id) => self.refs_to_commit_in_root(&id.root, index),
            None => Vec::new(),
        }
    }

    /// `refsToCommit(root, index)`.
    pub fn refs_to_commit_in_root(
        &self,
        root: &VcsRoot,
        index: VcsLogCommitStorageIndex,
    ) -> Vec<VcsRef> {
        self.refs_by_root
            .get(root)
            .map(|refs| refs.refs_to_commit(index))
            .unwrap_or_default()
    }
}

impl VcsRefsContainer for RefsModel {
    fn branches(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.refs_by_root.values().flat_map(|r| r.branches()))
    }

    fn tags(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.refs_by_root.values().flat_map(|r| r.tags()))
    }
}
