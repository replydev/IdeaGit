//! Attaching a freshly read block of recent commits to the loaded log.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogJoiner.java` and
//! `VcsLogRefreshNotEnoughDataException.java`. `NewCommitIntegrator` lives in
//! [`sorter`](crate::data::sorter).
//!
//! Commits of the saved log are "red" when they are no longer reachable from any reference and
//! "green" when they are; the "unsafe" prefix of the saved log, which may contain red commits or
//! where new commits are inserted, is rebuilt and the rest is kept as is.
//!
//! Deviation: the new commits are integrated in first-block order (upstream: `HashSet` order).
//! When the first block repeats an id, the last copy wins everywhere (the rebuilt unsafe prefix
//! and the new commits), like upstream's `HashMap.put`.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::data::sorter::NewCommitIntegrator;
use crate::graph_commit::GraphCommit;

/// `VcsLogJoiner.ILLEGAL_DATA_RELOAD_ALL`.
pub const ILLEGAL_DATA_RELOAD_ALL: &str = "All data is illegal - request reload all";

/// Why the first block couldn't be joined; the caller reloads the whole log.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JoinError {
    /// `VcsLogRefreshNotEnoughDataException`: the first block doesn't reach the saved log.
    #[error("not enough data to join the recent commits")]
    NotEnoughData,
    /// `IllegalStateException(ILLEGAL_DATA_RELOAD_ALL)`.
    #[error("{0}")]
    IllegalState(&'static str),
}

/// `VcsLogJoiner.addCommits`.
///
/// - `saved_log`: the log as currently loaded.
/// - `previous_refs`: commits referenced by the refs of the previous refresh.
/// - `first_block`: the freshly read most recent commits, newest first.
/// - `new_refs`: commits referenced by all refs now.
///
/// Returns the joined log and the number of commits added (negative when commits were removed).
pub fn add_commits<Id, C>(
    saved_log: &[C],
    previous_refs: &HashSet<Id>,
    first_block: &[C],
    new_refs: &HashSet<Id>,
) -> Result<(Vec<C>, isize), JoinError>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id> + Clone,
{
    let (save_green_index, all_new_commits) =
        get_new_commits_and_saved_green_index(saved_log, previous_refs, first_block, new_refs)?;
    let (save_red_index, remove_commits) =
        get_red_commits_and_saved_red_index(saved_log, previous_refs, first_block, new_refs)?;

    let unsafe_block_size = save_red_index.max(save_green_index);
    let unsafe_part_saved_log =
        build_unsafe_part_saved_log(saved_log, unsafe_block_size, &remove_commits, first_block);
    let unsafe_part_saved_log =
        NewCommitIntegrator::new(unsafe_part_saved_log, all_new_commits).get_result_list();

    let added = unsafe_part_saved_log.len() as isize - unsafe_block_size as isize;
    let mut result = unsafe_part_saved_log;
    result.extend_from_slice(&saved_log[unsafe_block_size..]);
    Ok((result, added))
}

/// Drops removed (red) commits from the unsafe prefix and replaces re-fetched commits with their
/// fresh copies, so their current parents are used.
fn build_unsafe_part_saved_log<Id, C>(
    saved_log: &[C],
    unsafe_block_size: usize,
    remove_commits: &HashSet<Id>,
    first_block: &[C],
) -> Vec<C>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id> + Clone,
{
    let recent_commits_by_id: HashMap<&Id, &C> = first_block.iter().map(|c| (c.id(), c)).collect();
    saved_log[..unsafe_block_size]
        .iter()
        .filter(|commit| !remove_commits.contains(commit.id()))
        .map(|commit| (*recent_commits_by_id.get(commit.id()).unwrap_or(&commit)).clone())
        .collect()
}

fn get_new_commits_and_saved_green_index<Id, C>(
    saved_log: &[C],
    previous_refs: &HashSet<Id>,
    first_block: &[C],
    new_refs: &HashSet<Id>,
) -> Result<(usize, Vec<C>), JoinError>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id> + Clone,
{
    let mut all_unresolved_linked_hashes: HashSet<Id> =
        new_refs.difference(previous_refs).cloned().collect();
    // at this moment it contains only NEW refs
    for commit in first_block {
        all_unresolved_linked_hashes.insert(commit.id().clone());
        all_unresolved_linked_hashes.extend(commit.parents().iter().cloned());
    }
    for commit in first_block {
        if !commit.parents().is_empty() {
            all_unresolved_linked_hashes.remove(commit.id());
        }
    }
    let save_green_index = get_first_un_tracked_index(saved_log, all_unresolved_linked_hashes)?;
    Ok((
        save_green_index,
        get_all_new_commits(&saved_log[..save_green_index], first_block),
    ))
}

fn get_first_un_tracked_index<Id, C>(
    commits: &[C],
    mut search_hashes: HashSet<Id>,
) -> Result<usize, JoinError>
where
    Id: Eq + Hash,
    C: GraphCommit<Id>,
{
    for (last_index, commit) in commits.iter().enumerate() {
        if search_hashes.is_empty() {
            return Ok(last_index);
        }
        search_hashes.remove(commit.id());
    }
    if !search_hashes.is_empty() {
        return Err(JoinError::NotEnoughData);
    }
    Ok(commits.len())
}

fn get_all_new_commits<Id, C>(unsafe_green_part_saved_log: &[C], first_block: &[C]) -> Vec<C>
where
    Id: Eq + Hash,
    C: GraphCommit<Id> + Clone,
{
    let existed_commit_hashes: HashSet<&Id> =
        unsafe_green_part_saved_log.iter().map(|c| c.id()).collect();
    // Duplicated ids stay: `NewCommitIntegrator::new` keeps the last one, like the unsafe prefix.
    first_block
        .iter()
        .filter(|c| !existed_commit_hashes.contains(c.id()))
        .cloned()
        .collect()
}

fn get_red_commits_and_saved_red_index<Id, C>(
    saved_log: &[C],
    previous_refs: &HashSet<Id>,
    first_block: &[C],
    new_refs: &HashSet<Id>,
) -> Result<(usize, HashSet<Id>), JoinError>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id>,
{
    let start_red_commits: HashSet<Id> = previous_refs.difference(new_refs).cloned().collect();
    let mut start_green_nodes: HashSet<Id> = new_refs.clone();
    let mut recent_commit_parents: HashMap<Id, &[Id]> = HashMap::new();
    for commit in first_block {
        recent_commit_parents.insert(commit.id().clone(), commit.parents());
        start_green_nodes.insert(commit.id().clone());
        start_green_nodes.extend(commit.parents().iter().cloned());
    }
    let mut sorter = RedGreenSorter {
        current_red: start_red_commits,
        current_green: start_green_nodes,
        all_red_commit: HashSet::new(),
        saved_log,
        recent_commit_parents,
    };
    let save_red_index = sorter.get_first_save_index()?;
    Ok((save_red_index, sorter.all_red_commit))
}

struct RedGreenSorter<'a, Id, C> {
    current_red: HashSet<Id>,
    current_green: HashSet<Id>,
    all_red_commit: HashSet<Id>,
    saved_log: &'a [C],
    recent_commit_parents: HashMap<Id, &'a [Id]>,
}

impl<Id, C> RedGreenSorter<'_, Id, C>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id>,
{
    fn mark_real_red_node(&mut self, node: &Id) -> Result<(), JoinError> {
        if !self.current_red.remove(node) {
            // see VcsLogJoinerTest#illegalStateExceptionTest2
            return Err(JoinError::IllegalState(ILLEGAL_DATA_RELOAD_ALL));
        }
        self.all_red_commit.insert(node.clone());
        Ok(())
    }

    fn get_first_save_index(&mut self) -> Result<usize, JoinError> {
        for (last_index, commit) in self.saved_log.iter().enumerate() {
            let is_green = self.current_green.contains(commit.id());
            if is_green {
                self.current_red.remove(commit.id());
                // A re-fetched commit's updated parents are already green; propagating the saved
                // parents would resurrect dropped ones, which become (tentative) red instead.
                match self.recent_commit_parents.get(commit.id()) {
                    None => self.current_green.extend(commit.parents().iter().cloned()),
                    Some(updated_parents) => {
                        let updated: HashSet<&Id> = updated_parents.iter().collect();
                        let dropped = commit.parents().iter().filter(|p| !updated.contains(p));
                        self.current_red.extend(dropped.cloned());
                    }
                }
            } else {
                self.mark_real_red_node(commit.id())?;
                self.current_red.extend(commit.parents().iter().cloned());
            }

            if self.current_red.is_empty() {
                return Ok(last_index + 1);
            }
        }
        // see VcsLogJoinerTest#illegalStateExceptionTest
        Err(JoinError::IllegalState(ILLEGAL_DATA_RELOAD_ALL))
    }
}
