//! Commit id interning: (root, hash) <-> dense integer indexes.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogStorage.kt` and
//! `InMemoryStorage.java` (with `BiDirectionalEnumerator`: indexes start at 1).
//!
//! The indexes are the commit ids of the permanent graph, as in upstream's production graph.
//! Indexes are never reused, so an index stays valid across refreshes.
//!
//! Deviations: no persistent storage (`VcsLogStorageImpl`); [`VcsLogStorage::get_commit_indexes`]
//! interns a commit with its parents under one lock.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::data::commit::CommitId;
use crate::data::hash::Hash;
use crate::data::root::VcsRoot;
use crate::data::vcs_ref::VcsRef;

/// `VcsLogCommitStorageIndex`.
pub type VcsLogCommitStorageIndex = i32;

/// Maps commits and references to integers (`VcsLogStorage`).
pub trait VcsLogStorage: Send + Sync {
    /// The index of the commit, assigning a new one for an unknown commit.
    fn get_commit_index(&self, hash: &Hash, root: &VcsRoot) -> VcsLogCommitStorageIndex;

    /// The indexes of several commits of one root, in order.
    fn get_commit_indexes(
        &self,
        hashes: &[&Hash],
        root: &VcsRoot,
    ) -> Vec<VcsLogCommitStorageIndex> {
        hashes
            .iter()
            .map(|hash| self.get_commit_index(hash, root))
            .collect()
    }

    /// The commit of an index, or `None` for an unknown index.
    fn get_commit_id(&self, commit_index: VcsLogCommitStorageIndex) -> Option<CommitId>;

    fn contains_commit(&self, id: &CommitId) -> bool;

    /// The index of a reference, assigning a new one for an unknown reference.
    fn get_ref_index(&self, vcs_ref: &VcsRef) -> i32;

    fn get_vcs_ref(&self, ref_index: i32) -> Option<VcsRef>;
}

#[derive(Default)]
struct Commits {
    roots: Vec<VcsRoot>,
    /// Per root (same order as `roots`): hash -> index.
    indexes: Vec<HashMap<Hash, i32>>,
    /// Index - 1 -> (root position, hash).
    ids: Vec<(u32, Hash)>,
}

impl Commits {
    fn root_position(&mut self, root: &VcsRoot) -> usize {
        if let Some(pos) = self
            .roots
            .iter()
            .position(|r| std::sync::Arc::ptr_eq(r, root) || r == root)
        {
            return pos;
        }
        self.roots.push(root.clone());
        self.indexes.push(HashMap::new());
        self.roots.len() - 1
    }

    fn find(&self, hash: &Hash, root: &VcsRoot) -> Option<i32> {
        let pos = self
            .roots
            .iter()
            .position(|r| std::sync::Arc::ptr_eq(r, root) || r == root)?;
        self.indexes[pos].get(hash).copied()
    }

    fn enumerate(&mut self, pos: usize, hash: &Hash) -> i32 {
        if let Some(&index) = self.indexes[pos].get(hash) {
            return index;
        }
        self.ids.push((pos as u32, hash.clone()));
        let index = self.ids.len() as i32;
        self.indexes[pos].insert(hash.clone(), index);
        index
    }
}

#[derive(Default)]
struct Refs {
    indexes: HashMap<VcsRef, i32>,
    refs: Vec<VcsRef>,
}

/// The in-memory [`VcsLogStorage`] (`InMemoryStorage`). Thread-safe; share it in an `Arc`.
#[derive(Default)]
pub struct InMemoryStorage {
    commits: RwLock<Commits>,
    refs: RwLock<Refs>,
}

impl InMemoryStorage {
    pub fn new() -> InMemoryStorage {
        InMemoryStorage::default()
    }

    /// The number of interned commits.
    pub fn commits_count(&self) -> usize {
        self.commits
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .ids
            .len()
    }
}

impl VcsLogStorage for InMemoryStorage {
    fn get_commit_index(&self, hash: &Hash, root: &VcsRoot) -> VcsLogCommitStorageIndex {
        if let Some(index) = self
            .commits
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .find(hash, root)
        {
            return index;
        }
        let mut commits = self.commits.write().unwrap_or_else(|e| e.into_inner());
        let pos = commits.root_position(root);
        commits.enumerate(pos, hash)
    }

    fn get_commit_indexes(
        &self,
        hashes: &[&Hash],
        root: &VcsRoot,
    ) -> Vec<VcsLogCommitStorageIndex> {
        let mut commits = self.commits.write().unwrap_or_else(|e| e.into_inner());
        let pos = commits.root_position(root);
        hashes
            .iter()
            .map(|hash| commits.enumerate(pos, hash))
            .collect()
    }

    fn get_commit_id(&self, commit_index: VcsLogCommitStorageIndex) -> Option<CommitId> {
        let commits = self.commits.read().unwrap_or_else(|e| e.into_inner());
        let (pos, hash) = commits.ids.get((commit_index as usize).checked_sub(1)?)?;
        Some(CommitId {
            hash: hash.clone(),
            root: commits.roots[*pos as usize].clone(),
        })
    }

    fn contains_commit(&self, id: &CommitId) -> bool {
        self.commits
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .find(&id.hash, &id.root)
            .is_some()
    }

    fn get_ref_index(&self, vcs_ref: &VcsRef) -> i32 {
        let mut refs = self.refs.write().unwrap_or_else(|e| e.into_inner());
        if let Some(&index) = refs.indexes.get(vcs_ref) {
            return index;
        }
        refs.refs.push(vcs_ref.clone());
        let index = refs.refs.len() as i32;
        refs.indexes.insert(vcs_ref.clone(), index);
        index
    }

    fn get_vcs_ref(&self, ref_index: i32) -> Option<VcsRef> {
        let refs = self.refs.read().unwrap_or_else(|e| e.into_inner());
        refs.refs.get((ref_index as usize).checked_sub(1)?).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::root::vcs_root;

    #[test]
    fn indexes_are_dense_stable_and_per_root() {
        let storage = InMemoryStorage::new();
        let r1 = vcs_root("/r1");
        let r2 = vcs_root("/r2");
        let a = Hash::build("aa");
        assert_eq!(storage.get_commit_index(&a, &r1), 1);
        assert_eq!(storage.get_commit_index(&a, &r2), 2);
        assert_eq!(storage.get_commit_index(&a, &vcs_root("/r1")), 1);
        assert_eq!(
            storage.get_commit_indexes(&[&Hash::build("bb"), &a], &r1),
            [3, 1]
        );
        let id = storage.get_commit_id(2).unwrap();
        assert_eq!((id.hash, &*id.root), (a.clone(), &*r2));
        assert!(storage.get_commit_id(0).is_none());
        assert!(storage.get_commit_id(4).is_none());
        assert!(storage.contains_commit(&CommitId { hash: a, root: r1 }));
        assert_eq!(storage.commits_count(), 3);
    }
}
