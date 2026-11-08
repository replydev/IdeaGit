//! Port of `platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogJoinerTest.kt`.
//!
//! Generated from the upstream DSL: each test lists the saved log, the recent commits, the old
//! and new refs, and the expected ids (and, for some, ids with parents) of the joined log.

mod common;

use std::collections::HashSet;

use common::timed_commit_parser::log;
use ig_log::GraphCommit;
use ig_log::data::joiner::{ILLEGAL_DATA_RELOAD_ALL, add_commits};
use ig_log::data::{Hash, JoinError};

const BIG_TIME: i32 = 100000000;

fn lines(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

struct TestRunner {
    full_log: Vec<String>,
    recent_commits: Vec<String>,
    old_refs: Vec<String>,
    new_refs: Vec<String>,
    expected: Option<Vec<String>>,
    expected_with_parents: Option<Vec<String>>,
}

impl TestRunner {
    fn run(self) -> Result<(), JoinError> {
        let vcs_full_log = log(&self.full_log);
        let vcs_recent_commits = log(&self.recent_commits);
        let vcs_old_refs: HashSet<Hash> = self.old_refs.iter().map(|h| Hash::build(h)).collect();
        let vcs_new_refs: HashSet<Hash> = self.new_refs.iter().map(|h| Hash::build(h)).collect();

        let (result, _) = add_commits(
            &vcs_full_log,
            &vcs_old_refs,
            &vcs_recent_commits,
            &vcs_new_refs,
        )?;
        let actual: Vec<String> = result.iter().map(|c| c.id().as_string()).collect();
        assert_eq!(
            self.expected.expect("expected is set").join("\n"),
            actual.join("\n")
        );

        if let Some(expected_parents) = self.expected_with_parents {
            let actual_with_parents: Vec<String> = result
                .iter()
                .map(|commit| {
                    let parents: Vec<String> =
                        commit.parents().iter().map(Hash::as_string).collect();
                    if parents.is_empty() {
                        commit.id().as_string()
                    } else {
                        format!("{}|-{}", commit.id(), parents.join(" "))
                    }
                })
                .collect();
            assert_eq!(expected_parents.join("\n"), actual_with_parents.join("\n"));
        }
        Ok(())
    }
}

#[test]
fn simple() {
    let runner = TestRunner {
        full_log: lines(&["4|-a2|-a1", "3|-b1|-a", "2|-a1|-a", "1|-a|-"]),
        recent_commits: lines(&["5|-f|-b1", "6|-e|-a2"]),
        old_refs: lines(&["a2", "b1"]),
        new_refs: lines(&["f", "e"]),
        expected: Some(lines(&["e", "f", "a2", "b1", "a1", "a"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn one_node() {
    let runner = TestRunner {
        full_log: lines(&["3|-a1|-"]),
        recent_commits: lines(&["3|-a1|-"]),
        old_refs: lines(&["a1"]),
        new_refs: lines(&["a1"]),
        expected: Some(lines(&["a1"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn one_node_reset() {
    let runner = TestRunner {
        full_log: lines(&["3|-a1|-a2", "2|-a2|-"]),
        recent_commits: lines(&["2|-a2|-"]),
        old_refs: lines(&["a2", "a1"]),
        new_refs: lines(&["a2"]),
        expected: Some(lines(&["a2"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn one_node_reset2() {
    let runner = TestRunner {
        full_log: lines(&["3|-a1|-a2", "2|-a2|-"]),
        recent_commits: lines(&["2|-a2|-"]),
        old_refs: lines(&["a1"]),
        new_refs: lines(&["a2"]),
        expected: Some(lines(&["a2"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn simple_remove_commits() {
    let runner = TestRunner {
        full_log: lines(&["4|-a2|-a1", "3|-b1|-a", "2|-a1|-a", "1|-a|-"]),
        recent_commits: lines(&["5|-f|-b1", "6|-e|-a1"]),
        old_refs: lines(&["a2"]),
        new_refs: lines(&["f", "e"]),
        expected: Some(lines(&["e", "f", "b1", "a1", "a"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_commits() {
    let runner = TestRunner {
        full_log: lines(&[
            "5|-a5|-a4",
            "4|-a4|-a2 a3",
            "3|-a3|-a1",
            "2|-a2|-a1",
            "1|-a1|-",
        ]),
        recent_commits: lines(&["6|-a6|-a3"]),
        old_refs: lines(&["a5"]),
        new_refs: lines(&["a6"]),
        expected: Some(lines(&["a6", "a3", "a1"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_commits2() {
    let runner = TestRunner {
        full_log: lines(&["2|-a2|-a1", "1|-a1|-"]),
        recent_commits: lines(&["5|-a5|-a4", "3|-a3|-a2", "4|-a4|-a3"]),
        old_refs: lines(&["a2"]),
        new_refs: lines(&["a5"]),
        expected: Some(lines(&["a5", "a4", "a3", "a2", "a1"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_commits3() {
    let runner = TestRunner {
        full_log: lines(&["3|-a3|-a2", "2|-a2|-a1", "1|-a1|-"]),
        recent_commits: lines(&["2|-a2|-a1"]),
        old_refs: lines(&["a3"]),
        new_refs: lines(&["a2"]),
        expected: Some(lines(&["a2", "a1"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_old_branch() {
    let runner = TestRunner {
        full_log: std::iter::once("100|-e1|-e10".to_string())
            .chain((10..=100000).map(|it| format!("{}|-e{}|-e{}", BIG_TIME - it, it, it + 1)))
            .chain(std::iter::once("5|-e100001|-a1".to_string()))
            .chain(std::iter::once("4|-b2|-b1".to_string()))
            .chain(std::iter::once("3|-b1|-a1".to_string()))
            .chain(std::iter::once("1|-a1|-".to_string()))
            .collect::<Vec<String>>(),
        recent_commits: lines(&["100|-e1|-e10"]),
        old_refs: lines(&["e1", "b2"]),
        new_refs: lines(&["e1"]),
        expected: Some(
            std::iter::once("e1".to_string())
                .chain((10..=100000).map(|it| format!("e{it}")))
                .chain(std::iter::once("e100001".to_string()))
                .chain(std::iter::once("a1".to_string()))
                .collect::<Vec<String>>(),
        ),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn add_to_old_branch() {
    let runner = TestRunner {
        full_log: std::iter::once("100|-e1|-e10".to_string())
            .chain((10..=100000).map(|it| format!("{}|-e{}|-e{}", BIG_TIME - it, it, it + 1)))
            .chain(std::iter::once("5|-e100001|-a1".to_string()))
            .chain(std::iter::once("4|-b2|-b1".to_string()))
            .chain(std::iter::once("3|-b1|-a1".to_string()))
            .chain(std::iter::once("1|-a1|-".to_string()))
            .collect::<Vec<String>>(),
        recent_commits: lines(&["50|-b4|-b3", "49|-b3|-b2"]),
        old_refs: lines(&["e1", "b2"]),
        new_refs: lines(&["e1", "b4"]),
        expected: Some(
            std::iter::once("e1".to_string())
                .chain((10..=100000).map(|it| format!("e{it}")))
                .chain(std::iter::once("b4".to_string()))
                .chain(std::iter::once("b3".to_string()))
                .chain(std::iter::once("e100001".to_string()))
                .chain(std::iter::once("b2".to_string()))
                .chain(std::iter::once("b1".to_string()))
                .chain(std::iter::once("a1".to_string()))
                .collect::<Vec<String>>(),
        ),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_long_branch() {
    let runner = TestRunner {
        full_log: std::iter::once("100|-e1|-e10".to_string())
            .chain((10..=100000).map(|it| format!("{}|-e{}|-e{}", BIG_TIME - it, it, it + 1)))
            .chain(std::iter::once("5|-e100001|-a1".to_string()))
            .chain(std::iter::once("4|-b2|-b1".to_string()))
            .chain(std::iter::once("3|-b1|-a1".to_string()))
            .chain(std::iter::once("1|-a1|-".to_string()))
            .collect::<Vec<String>>(),
        recent_commits: lines(&["50|-b4|-b3", "49|-b3|-b2"]),
        old_refs: lines(&["e1", "b2"]),
        new_refs: lines(&["b4"]),
        expected: Some(lines(&["b4", "b3", "b2", "b1", "a1"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn not_enough_data_exception_test() {
    let runner = TestRunner {
        full_log: lines(&["1|-a1|-"]),
        recent_commits: lines(&["3|-a3|-a2"]),
        old_refs: lines(&["a1"]),
        new_refs: lines(&["a3"]),
        expected: None,
        expected_with_parents: None,
    };
    assert_eq!(runner.run().unwrap_err(), JoinError::NotEnoughData);
}

#[test]
fn illegal_state_exception_test() {
    let runner = TestRunner {
        full_log: lines(&["1|-a1|-"]),
        recent_commits: lines(&["1|-a1|-"]),
        old_refs: lines(&["a1", "a2"]),
        new_refs: lines(&["a1"]),
        expected: None,
        expected_with_parents: None,
    };
    assert_eq!(
        runner.run().unwrap_err(),
        JoinError::IllegalState(ILLEGAL_DATA_RELOAD_ALL)
    );
}

#[test]
fn illegal_state_exception_test2() {
    let runner = TestRunner {
        full_log: lines(&["2|-a2|-a1", "1|-a1|-"]),
        recent_commits: lines(&["3|-a3|-a1", "1|-a1|-"]),
        old_refs: lines(&[]),
        new_refs: lines(&["a3"]),
        expected: None,
        expected_with_parents: None,
    };
    assert_eq!(
        runner.run().unwrap_err(),
        JoinError::IllegalState(ILLEGAL_DATA_RELOAD_ALL)
    );
}

#[test]
fn remove_parallel_branch() {
    let runner = TestRunner {
        full_log: lines(&["4|-a4|-a1", "3|-a3|-a2", "2|-a2|-", "1|-a1|-"]),
        recent_commits: lines(&[]),
        old_refs: lines(&["a4", "a3"]),
        new_refs: lines(&["a3"]),
        expected: Some(lines(&["a3", "a2"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_commit_with_changed_parents() {
    let runner = TestRunner {
        full_log: lines(&["4|-c|-b d", "3|-d|-a", "2|-b|-a", "1|-a|-"]),
        recent_commits: lines(&["4|-c|-b", "2|-b|-a", "1|-a|-"]),
        old_refs: lines(&["c", "d"]),
        new_refs: lines(&["c"]),
        expected: Some(lines(&["c", "b", "a"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_commit_with_changed_parents_no_ref() {
    let runner = TestRunner {
        full_log: lines(&["4|-c|-b d", "3|-d|-a", "2|-b|-a", "1|-a|-"]),
        recent_commits: lines(&["4|-c|-b"]),
        old_refs: lines(&["c"]),
        new_refs: lines(&["c"]),
        expected: Some(lines(&["c", "b", "a"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn keep_shared_parent_despite_one_branch_dropping_it() {
    let runner = TestRunner {
        full_log: lines(&["5|-c|-b d", "4|-f|-d", "3|-d|-a", "2|-b|-a", "1|-a|-"]),
        recent_commits: lines(&["5|-c|-b", "4|-f|-d"]),
        old_refs: lines(&["c"]),
        new_refs: lines(&["c"]),
        expected: Some(lines(&["c", "f", "d", "b", "a"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

/// Scenario from IJPL-248963: commit C's parents changed from [B, D] to [B].
/// D had a ref of its own which is now gone. D should be removed (red).
#[test]
fn stale_parent_propagation() {
    let runner = TestRunner {
        full_log: lines(&["4|-c|-b d", "3|-b|-a", "2|-d|-a", "1|-a|-"]),
        recent_commits: lines(&["4|-c|-b"]),
        old_refs: lines(&["c", "d"]),
        new_refs: lines(&["c"]),
        expected: Some(lines(&["c", "b", "a"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

/// Verify that the returned commit C has updated parents from firstBlock,
/// not stale parents from savedLog.
#[test]
fn stale_parent_propagation_parents_updated() {
    let runner = TestRunner {
        full_log: lines(&["4|-c|-b d", "3|-b|-a", "2|-d|-a", "1|-a|-"]),
        recent_commits: lines(&["4|-c|-b"]),
        old_refs: lines(&["c", "d"]),
        new_refs: lines(&["c"]),
        expected: Some(lines(&["c", "b", "a"])),
        expected_with_parents: Some(lines(&[
            // C should have parent [b] only, not [b, d]
            "c|-b", "b|-a", "a",
        ])),
    };
    runner.run().unwrap();
}

/// Multiple commits have changed parents. Verify both are updated.
#[test]
fn stale_parent_propagation_multiple_changed_parents() {
    let runner = TestRunner {
        full_log: lines(&["5|-e|-c d", "4|-c|-b d", "3|-b|-a", "2|-d|-a", "1|-a|-"]),
        recent_commits: lines(&[
            // E now only has parent C, C now only has parent B (D removed from both)
            "5|-e|-c", "4|-c|-b",
        ]),
        old_refs: lines(&["e", "d"]),
        new_refs: lines(&["e"]),
        expected: Some(lines(&["e", "c", "b", "a"])),
        expected_with_parents: Some(lines(&["e|-c", "c|-b", "b|-a", "a"])),
    };
    runner.run().unwrap();
}

/// A→B→C becomes A→B'→C: commit A keeps the same hash but its parent
/// changed from B to B'. B is gone, B' is new.
/// A should connect to B' in the result, old B should be removed.
#[test]
fn parent_replaced_same_commit_hash() {
    let runner = TestRunner {
        full_log: lines(&["3|-a|-b", "2|-b|-c", "1|-c|-"]),
        recent_commits: lines(&["3|-a|-b2", "2|-b2|-c"]),
        old_refs: lines(&["a"]),
        new_refs: lines(&["a"]),
        expected: Some(lines(&["a", "b2", "c"])),
        expected_with_parents: Some(lines(&["a|-b2", "b2|-c", "c"])),
    };
    runner.run().unwrap();
}

/// Refreshing with the exact same data should not change the log.
#[test]
fn same_data_refresh() {
    let runner = TestRunner {
        full_log: lines(&["3|-a|-b", "2|-b|-c", "1|-c|-"]),
        recent_commits: lines(&["3|-a|-b", "2|-b|-c", "1|-c|-"]),
        old_refs: lines(&["a"]),
        new_refs: lines(&["a"]),
        expected: Some(lines(&["a", "b", "c"])),
        expected_with_parents: Some(lines(&["a|-b", "b|-c", "c"])),
    };
    runner.run().unwrap();
}

/// Same-data refresh with disconnected islands should not change the log.
#[test]
fn same_data_refresh_with_islands() {
    let runner = TestRunner {
        full_log: lines(&["3|-a|-b", "2|-b|-", "2|-e|-f", "1|-f|-"]),
        recent_commits: lines(&["3|-a|-b", "2|-b|-", "2|-e|-f", "1|-f|-"]),
        old_refs: lines(&["a", "e"]),
        new_refs: lines(&["a", "e"]),
        expected: Some(lines(&["a", "b", "e", "f"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

/// A new commit is added on top of an existing chain.
#[test]
fn new_commit_on_existing_chain() {
    let runner = TestRunner {
        full_log: lines(&["2|-b|-c", "1|-c|-"]),
        recent_commits: lines(&["3|-a|-b", "2|-b|-c"]),
        old_refs: lines(&["b"]),
        new_refs: lines(&["a"]),
        expected: Some(lines(&["a", "b", "c"])),
        expected_with_parents: Some(lines(&["a|-b", "b|-c", "c"])),
    };
    runner.run().unwrap();
}

/// E's parent changes from A to B (no ref change — E is still the only ref).
/// A becomes unreachable and should be removed from the log.
#[test]
fn dropped_parent_removed_when_no_ref_change() {
    let runner = TestRunner {
        full_log: lines(&["4|-e|-a", "3|-a|-b", "2|-b|-c", "1|-c|-"]),
        recent_commits: lines(&["4|-e|-b", "2|-b|-c", "1|-c|-"]),
        old_refs: lines(&["e"]),
        new_refs: lines(&["e"]),
        expected: Some(lines(&["e", "b", "c"])),
        expected_with_parents: Some(lines(&["e|-b", "b|-c", "c"])),
    };
    runner.run().unwrap();
}

/// E's parent changes from A to C, but A is still reachable through another ref F.
/// A should NOT be removed.
#[test]
fn dropped_parent_survives_if_reachable_elsewhere() {
    let runner = TestRunner {
        full_log: lines(&["4|-e|-a", "3|-a|-c", "3|-f|-a", "1|-c|-"]),
        recent_commits: lines(&["4|-e|-c", "3|-f|-a", "3|-a|-c", "1|-c|-"]),
        old_refs: lines(&["e", "f"]),
        new_refs: lines(&["e", "f"]),
        expected: Some(lines(&["e", "a", "f", "c"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

/// E's parent changes from A to nothing (E is re-fetched as an orphan/root commit).
/// A, B, C become unreachable and should all be removed.
#[test]
fn commit_becomes_orphan_drops_unreachable_parent() {
    let runner = TestRunner {
        full_log: lines(&["4|-e|-a", "3|-a|-b", "2|-b|-c", "1|-c|-"]),
        recent_commits: lines(&["4|-e|-"]),
        old_refs: lines(&["e"]),
        new_refs: lines(&["e"]),
        expected: Some(lines(&["e"])),
        expected_with_parents: Some(lines(&["e"])),
    };
    runner.run().unwrap();
}

#[test]
fn remove_all() {
    let runner = TestRunner {
        full_log: lines(&["4|-a4|-a1", "3|-a3|-a2", "2|-a2|-", "1|-a1|-"]),
        recent_commits: lines(&[]),
        old_refs: lines(&["a4", "a3"]),
        new_refs: lines(&[]),
        expected: Some(lines(&[])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}
