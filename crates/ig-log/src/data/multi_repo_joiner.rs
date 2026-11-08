//! Merging the logs of several repositories into one list.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogMultiRepoJoiner.java`.
//!
//! Each log keeps its own (topological) order; at each step the newest of the next commits of all
//! logs is taken.
//!
//! Deviations:
//! - Commits with the same hash in different logs are all kept. Upstream keys its iterator map by
//!   commit, whose equality is by hash, so it silently drops one log's iterator.
//! - Timestamp ties go to the later log in the caller's order (upstream: the last in `HashMap`
//!   iteration order). The refresher passes roots in `VcsRoot` (`Path`) order, which is not
//!   `compare_roots` string order.

use crate::graph_commit::GraphCommit;

/// `VcsLogMultiRepoJoiner.join`.
pub fn join<Id, C: GraphCommit<Id>>(logs_from_repos: Vec<Vec<C>>) -> Vec<C> {
    if logs_from_repos.len() == 1 {
        return logs_from_repos.into_iter().next().expect("one log");
    }
    let size = logs_from_repos.iter().map(Vec::len).sum();
    let mut result = Vec::with_capacity(size);

    let mut iterators: Vec<_> = logs_from_repos
        .into_iter()
        .map(|log| log.into_iter().peekable())
        .collect();
    loop {
        // `findLatestCommit`: `>=`, so the last of equal timestamps wins
        let mut latest: Option<(usize, i64)> = None;
        for (i, it) in iterators.iter_mut().enumerate() {
            if let Some(commit) = it.peek() {
                let timestamp = commit.timestamp();
                if latest.is_none_or(|(_, max)| timestamp >= max) {
                    latest = Some((i, timestamp));
                }
            }
        }
        let Some((i, _)) = latest else { break };
        result.push(iterators[i].next().expect("peeked"));
    }
    result
}
