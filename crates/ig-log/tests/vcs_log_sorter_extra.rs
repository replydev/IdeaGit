//! IdeaGit tests for the sorter (not ported from upstream).

mod common;

use std::collections::HashSet;

use common::timed_commit_parser::log;
use ig_log::GraphCommit;
use ig_log::data::{TimedVcsCommit, sort_by_date_topo_order};

fn ids(commits: &[TimedVcsCommit]) -> Vec<String> {
    commits.iter().map(|c| c.id().as_string()).collect()
}

/// A is a child of B, but A is older than B; C is unrelated. Without monotone timestamps the
/// result follows the input order deterministically.
#[test]
fn skewed_timestamps_follow_input_order() {
    let c = "5|-c|-";
    let a = "1|-a|-b";
    let b = "10|-b|-";
    let sorted = sort_by_date_topo_order(log(&[c, a, b]));
    assert_eq!(ids(&sorted), ["a", "b", "c"]);
    let sorted = sort_by_date_topo_order(log(&[a, b, c]));
    assert_eq!(ids(&sorted), ["c", "a", "b"]);
}

#[test]
fn cyclic_graph_keeps_every_commit() {
    let input = log(&["4|-a4|-a3", "3|-a3|-a2", "2|-a2|-a1", "1|-a1|-a4"]);
    let n = input.len();
    let sorted = sort_by_date_topo_order(input);
    assert_eq!(sorted.len(), n);
    let unique: HashSet<_> = ids(&sorted).into_iter().collect();
    assert_eq!(unique.len(), n);
}
