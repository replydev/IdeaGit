//! IdeaGit tests for the joiner (not ported from upstream `VcsLogJoinerTest`).

mod common;

use std::collections::HashSet;

use common::timed_commit_parser::log;
use ig_log::GraphCommit;
use ig_log::data::joiner::add_commits;
use ig_log::data::{Hash, JoinError, TimedVcsCommit};

fn hashes(items: &[&str]) -> HashSet<Hash> {
    items.iter().map(|h| Hash::build(h)).collect()
}

/// Ids, or `id|-parents` for commits that have parents.
fn describe(commits: &[TimedVcsCommit]) -> Vec<String> {
    commits
        .iter()
        .map(|c| {
            let parents: Vec<String> = c.parents().iter().map(Hash::as_string).collect();
            if parents.is_empty() {
                c.id().as_string()
            } else {
                format!("{}|-{}", c.id(), parents.join(" "))
            }
        })
        .collect()
}

fn join(
    saved: &[TimedVcsCommit],
    old_refs: &[&str],
    first_block: &[TimedVcsCommit],
    new_refs: &[&str],
) -> Result<Vec<TimedVcsCommit>, JoinError> {
    add_commits(saved, &hashes(old_refs), first_block, &hashes(new_refs)).map(|(log, _)| log)
}

/// A linear history of `count` commits, newest first: commit `i` has the parent `i + 1`.
fn linear_log(count: usize) -> Vec<TimedVcsCommit> {
    (0..count)
        .map(|i| {
            let parents = if i + 1 < count {
                vec![Hash::build(&format!("{:x}", i + 1))]
            } else {
                Vec::new()
            };
            TimedVcsCommit::new(
                Hash::build(&format!("{:x}", i)),
                parents,
                (count - i) as i64,
            )
        })
        .collect()
}

#[test]
fn new_ref_on_deep_commit_without_new_commits() {
    let saved = linear_log(4000);
    let head = format!("{:x}", 0);
    let deep = format!("{:x}", 3000);
    // an empty first block, and a first block with only the (unchanged) head
    for first_block in [Vec::new(), saved[..1].to_vec()] {
        let result = join(&saved, &[&head], &first_block, &[&head, &deep]).unwrap();
        assert_eq!(describe(&result), describe(&saved));
    }
}

#[test]
fn empty_saved_log_needs_more_data() {
    let result = join(&[], &[], &log(&["1|-a|-"]), &["a"]);
    assert_eq!(result.unwrap_err(), JoinError::NotEnoughData);
}

#[test]
fn equal_timestamp_new_commits_keep_first_block_order() {
    let saved = log(&["1|-1|-"]);
    let a = "5|-a|-1";
    let b = "5|-b|-1";
    let result = join(&saved, &["1"], &log(&[a, b]), &["a", "b"]).unwrap();
    assert_eq!(describe(&result), ["a|-1", "b|-1", "1"]);
    let result = join(&saved, &["1"], &log(&[b, a]), &["a", "b"]).unwrap();
    assert_eq!(describe(&result), ["b|-1", "a|-1", "1"]);
}

#[test]
fn duplicate_ids_last_wins() {
    let saved = log(&["2|-c|-", "1|-d|-"]);
    let first_block = log(&["5|-a|-c", "5|-a|-d"]);
    let result = join(&saved, &["c", "d"], &first_block, &["a", "c", "d"]).unwrap();
    assert_eq!(describe(&result), ["a|-d", "c", "d"]);
}
