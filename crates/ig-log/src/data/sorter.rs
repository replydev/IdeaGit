//! Sorting a block of commits by date, keeping topological order.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogSorter.java` and
//! `VcsLogJoiner.NewCommitIntegrator` (`VcsLogJoiner.java`).
//!
//! Deviation: upstream picks the next commit to integrate from a `HashMap` (iteration order of the
//! hashes); here it is the first remaining commit in input order, which makes the result
//! deterministic.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::graph_commit::GraphCommit;

/// `VcsLogSorter.sortByDateTopoOrder`: newest first, children before parents.
pub fn sort_by_date_topo_order<Id, C>(commits: Vec<C>) -> Vec<C>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id>,
{
    NewCommitIntegrator::new(Vec::new(), commits).get_result_list()
}

/// Inserts new commits into a list, parents first, each one before its first parent already in
/// the list or before the first older commit (`NewCommitIntegrator`).
pub struct NewCommitIntegrator<Id, C> {
    list: Vec<C>,
    /// The new commits in input order; taken ones are `None`.
    new_commits: Vec<Option<C>>,
    new_commits_index: HashMap<Id, usize>,
    first_remaining: usize,
    commits_stack: Vec<C>,
}

impl<Id, C> NewCommitIntegrator<Id, C>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id>,
{
    pub fn new(list: Vec<C>, new_commits: Vec<C>) -> NewCommitIntegrator<Id, C> {
        let mut new_commits_index = HashMap::with_capacity(new_commits.len());
        for (i, commit) in new_commits.iter().enumerate() {
            // `HashMap.put`: a later commit with the same id replaces the earlier one
            new_commits_index.insert(commit.id().clone(), i);
        }
        let new_commits = new_commits
            .into_iter()
            .enumerate()
            .map(|(i, c)| (new_commits_index.get(c.id()) == Some(&i)).then_some(c))
            .collect();
        NewCommitIntegrator {
            list,
            new_commits,
            new_commits_index,
            first_remaining: 0,
            commits_stack: Vec::new(),
        }
    }

    fn take_new_commit(&mut self, id: &Id) -> Option<C> {
        let index = self.new_commits_index.remove(id)?;
        self.new_commits[index].take()
    }

    fn first_new_commit(&mut self) -> Option<C> {
        while self.first_remaining < self.new_commits.len() {
            if let Some(commit) = self.new_commits[self.first_remaining].take() {
                self.new_commits_index.remove(commit.id());
                return Some(commit);
            }
            self.first_remaining += 1;
        }
        None
    }

    fn insert_all_use_stack(&mut self) {
        while let Some(first) = self.first_new_commit() {
            self.commits_stack.push(first);
            while let Some(current) = self.commits_stack.last() {
                let next_parent = current
                    .parents()
                    .iter()
                    .find(|p| self.new_commits_index.contains_key(*p))
                    .cloned();
                if let Some(parent) = next_parent {
                    let parent_commit = self.take_new_commit(&parent).expect("indexed");
                    self.commits_stack.push(parent_commit);
                    continue;
                }

                let current = self.commits_stack.pop().expect("not empty");
                let parents: HashSet<&Id> = current.parents().iter().collect();
                let insert_index = self
                    .list
                    .iter()
                    .position(|some| {
                        parents.contains(some.id()) || some.timestamp() < current.timestamp()
                    })
                    .unwrap_or(self.list.len());
                self.list.insert(insert_index, current);
            }
        }
    }

    /// `getResultList`.
    pub fn get_result_list(mut self) -> Vec<C> {
        self.insert_all_use_stack();
        self.list
    }
}
