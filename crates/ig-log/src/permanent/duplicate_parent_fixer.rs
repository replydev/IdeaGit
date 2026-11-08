//! Removes duplicate parents from commits before the graph is built.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/DuplicateParentFixer.java`.
//!
//! Deviation: upstream wraps the list lazily and re-fixes a commit on every access; here each commit
//! is fixed once, borrowing the original parents when nothing changes.

use std::borrow::Cow;
use std::collections::HashSet;
use std::hash::Hash;

use crate::graph_commit::GraphCommit;

/// A commit with duplicate parents removed (`DelegateGraphCommit`, or the original commit).
#[derive(Clone, Debug)]
pub struct FixedGraphCommit<'a, CommitId: Clone> {
    id: &'a CommitId,
    parents: Cow<'a, [CommitId]>,
    timestamp: i64,
}

impl<'a, CommitId: Clone> FixedGraphCommit<'a, CommitId> {
    /// The id, borrowed from the original commit list.
    pub fn commit_id(&self) -> &'a CommitId {
        self.id
    }

    /// The fixed parents (cheap unless duplicates were removed).
    pub fn parents_cow(&self) -> Cow<'a, [CommitId]> {
        self.parents.clone()
    }
}

impl<CommitId: Clone> GraphCommit<CommitId> for FixedGraphCommit<'_, CommitId> {
    fn id(&self) -> &CommitId {
        self.id
    }

    fn parents(&self) -> &[CommitId] {
        &self.parents
    }

    fn timestamp(&self) -> i64 {
        self.timestamp
    }
}

/// `fixDuplicateParentCommits`.
pub fn fix_duplicate_parent_commits<CommitId, G>(
    commits: &[G],
) -> Vec<FixedGraphCommit<'_, CommitId>>
where
    CommitId: Eq + Hash + Clone,
    G: GraphCommit<CommitId>,
{
    commits
        .iter()
        .map(|commit| FixedGraphCommit {
            id: commit.id(),
            parents: fix_parents_duplicate(commit.parents()),
            timestamp: commit.timestamp(),
        })
        .collect()
}

/// Keeps the first occurrence of every parent, in order.
pub fn fix_parents_duplicate<CommitId: Eq + Hash + Clone>(
    parents: &[CommitId],
) -> Cow<'_, [CommitId]> {
    if parents.len() <= 1 {
        return Cow::Borrowed(parents);
    }

    if parents.len() == 2 {
        return if parents[0] != parents[1] {
            Cow::Borrowed(parents)
        } else {
            Cow::Owned(vec![parents[0].clone()])
        };
    }

    let mut all_parents: HashSet<&CommitId> = parents.iter().collect();
    if parents.len() == all_parents.len() {
        return Cow::Borrowed(parents);
    }

    let correct_parents = parents
        .iter()
        .filter(|commit_id| all_parents.remove(commit_id))
        .cloned()
        .collect();
    Cow::Owned(correct_parents)
}
