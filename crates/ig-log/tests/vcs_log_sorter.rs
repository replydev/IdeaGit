//! Port of `platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogSorterTest.java`.

mod common;

use common::timed_commit_parser::log;
use ig_log::GraphCommit;
use ig_log::data::{TimedVcsCommit, sort_by_date_topo_order};

fn to_str(commits: &[TimedVcsCommit]) -> String {
    commits
        .iter()
        .map(|c| c.id().as_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn do_test(started: Vec<TimedVcsCommit>, expected: Vec<TimedVcsCommit>) {
    let sorted = sort_by_date_topo_order(started);
    assert_eq!(to_str(&expected), to_str(&sorted));
}

#[test]
fn simple_test() {
    do_test(
        log(&["1|-a0|-", "3|-a1|-a0", "6|-a2|-a0"]),
        log(&["6|-a2|-a0", "3|-a1|-a0", "1|-a0|-"]),
    );
}

#[test]
fn several_heads() {
    do_test(
        log(&[
            "1|-a4|-",
            "2|-a3|-a4",
            "6|-b1|-b2",
            "4|-b2|-a3",
            "3|-a2|-a3",
            "5|-a1|-a2",
        ]),
        log(&[
            "6|-b1|-b2",
            "5|-a1|-a2",
            "4|-b2|-a3",
            "3|-a2|-a3",
            "2|-a3|-a4",
            "1|-a4|-",
        ]),
    );
}

#[test]
fn with_merge() {
    do_test(
        log(&[
            "6|-b1|-b2",
            "2|-a3|-a4",
            "3|-a2|-a3",
            "4|-b2|-a3",
            "5|-a1|-a2",
            "1|-a0|-b1 a1",
            "1|-a4|-",
        ]),
        log(&[
            "1|-a0|-b1 a1",
            "6|-b1|-b2",
            "5|-a1|-a2",
            "4|-b2|-a3",
            "3|-a2|-a3",
            "2|-a3|-a4",
            "1|-a4|-",
        ]),
    );
}

#[test]
fn several_branches() {
    do_test(
        log(&["1|-a1|-", "3|-a3|-a1", "2|-a2|-", "4|-a4|-a2"]),
        log(&["4|-a4|-a2", "3|-a3|-a1", "2|-a2|-", "1|-a1|-"]),
    );
}

/// Upstream: `@Test(timeout = 1000)`; the sort must terminate on a cycle.
#[test]
fn cyclic_graph() {
    let start = std::time::Instant::now();
    sort_by_date_topo_order(log(&["4|-a4|-a3", "3|-a3|-a2", "2|-a2|-a1", "1|-a1|-a4"]));
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
}
