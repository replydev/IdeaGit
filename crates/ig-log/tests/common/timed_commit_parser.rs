//! Port of `platform/vcs-log/impl/test/com/intellij/vcs/log/TimedCommitParser.java`.

use ig_log::data::{Hash, TimedVcsCommit};

use super::parser::{create_hash, next_separator_index, parse_commit_parents};

/// `intToHash`: `Integer.toHexString`.
fn int_to_hash(index: i32) -> Hash {
    Hash::build(&format!("{:x}", index))
}

/// `1231423|-adada|-193 adf45`: timestamp, commit, parents (hex integers).
fn parse_timestamp_parent_hashes(line: &str) -> TimedVcsCommit {
    let first_separator_index = next_separator_index(line, 0);
    let timestamp_str = &line[..first_separator_index];
    let timestamp = if timestamp_str.is_empty() {
        0
    } else {
        timestamp_str
            .parse::<i64>()
            .unwrap_or_else(|_| panic!("bad timestamp in line: {line}"))
    };
    let (id, parents) = parse_commit_parents(&line[first_separator_index + 2..]);
    let parents = parents
        .into_iter()
        .filter(|p| !p.is_empty())
        .map(|p| int_to_hash(create_hash(p)))
        .collect();
    TimedVcsCommit::new(int_to_hash(create_hash(id)), parents, timestamp)
}

/// `TimedCommitParser.log`.
pub fn log<S: AsRef<str>>(commits: &[S]) -> Vec<TimedVcsCommit> {
    commits
        .iter()
        .map(|c| parse_timestamp_parent_hashes(c.as_ref()))
        .collect()
}
