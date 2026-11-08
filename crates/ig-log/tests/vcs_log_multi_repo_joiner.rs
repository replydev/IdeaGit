//! Port of `platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogMultiRepoJoinerTest.java`.

mod common;

use common::timed_commit_parser::log;
use ig_log::GraphCommit;
use ig_log::data::multi_repo_joiner::join;

#[test]
fn join_test() {
    let first = log(&["6|-a2|-a0", "3|-a1|-a0", "1|-a0|-"]);
    let second = log(&["4|-b1|-b0", "2|-b0|-"]);
    let third = log(&["7|-c1|-c0", "5|-c0|-"]);

    let expected = log(&[
        "7|-c1|-c0",
        "6|-a2|-a0",
        "5|-c0|-",
        "4|-b1|-b0",
        "3|-a1|-a0",
        "2|-b0|-",
        "1|-a0|-",
    ]);

    let joined = join(vec![first, second, third]);

    // `TimedVcsCommitImpl.equals` compares hashes only; also compare the timestamps and parents
    assert_eq!(expected, joined);
    let full =
        |c: &ig_log::data::TimedVcsCommit| (c.id().clone(), c.parents().to_vec(), c.timestamp());
    assert_eq!(
        expected.iter().map(full).collect::<Vec<_>>(),
        joined.iter().map(full).collect::<Vec<_>>()
    );
}
