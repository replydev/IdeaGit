//! Shared infrastructure of the ported graph tests.
//!
//! `load_text` / `do_test` are ported from
//! `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/AbstractTestWithTextFile.java` and
//! `AbstractTestWithTwoTextFile.java`; [`CommitIdManager`] from
//! `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/CommitIdManager.java`.
//! Test data lives in `tests/fixtures/<dir>` (byte-identical copies of `platform/vcs-log/graph/testData`).
#![allow(dead_code)] // each test crate uses a different subset

pub mod parser;
pub mod str_utils;
pub mod test_graph_builder;
pub mod test_vcs_log_provider;
pub mod timed_commit_parser;

use std::hash::Hash;
use std::path::PathBuf;

use parser::{SimpleCommit, create_hash, parse_integer_commit_list, parse_string_commit_list};

pub const IN_POSTFIX: &str = "_in.txt";
pub const OUT_POSTFIX: &str = "_out.txt";

/// `loadText`: the file with line separators converted to `\n` (`FileUtil.loadFile(file, true)`).
pub fn load_text(directory: &str, filename: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(directory)
        .join(filename);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// `doTest`: runs `run_test(in, out)` on `<test_name>_in.txt` and `<test_name>_out.txt`.
pub fn do_test(directory: &str, test_name: &str, run_test: impl FnOnce(&str, &str)) {
    let input = load_text(directory, &format!("{test_name}{IN_POSTFIX}"));
    let output = load_text(directory, &format!("{test_name}{OUT_POSTFIX}"));
    run_test(&input, &output);
}

/// How the commit ids of a test are parsed and printed (`CommitIdManager`).
pub trait CommitIdManager {
    type Id: Clone + Eq + Hash + Send + Sync + std::fmt::Debug + 'static;

    fn parse_commit_list(input: &str) -> Vec<SimpleCommit<Self::Id>>;

    fn to_str(commit: &Self::Id) -> String;

    fn to_int(commit: &Self::Id) -> i32;
}

/// `STRING_COMMIT_ID_MANAGER`
pub struct StringCommitIdManager;

impl CommitIdManager for StringCommitIdManager {
    type Id = String;

    fn parse_commit_list(input: &str) -> Vec<SimpleCommit<String>> {
        parse_string_commit_list(input)
    }

    fn to_str(commit: &String) -> String {
        commit.clone()
    }

    fn to_int(commit: &String) -> i32 {
        create_hash(commit)
    }
}

/// `INTEGER_COMMIT_ID_MANAGER`
pub struct IntegerCommitIdManager;

impl CommitIdManager for IntegerCommitIdManager {
    type Id = i32;

    fn parse_commit_list(input: &str) -> Vec<SimpleCommit<i32>> {
        parse_integer_commit_list(input)
    }

    /// `Integer.toHexString`.
    fn to_str(commit: &i32) -> String {
        format!("{:x}", *commit as u32)
    }

    fn to_int(commit: &i32) -> i32 {
        *commit
    }
}
