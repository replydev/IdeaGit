//! IdeaGit tests for `GitLogParser` edge cases that the ported upstream tests don't cover.
//!
//! These live in their own test binary, hence their own process, on purpose: the parser's
//! markers carry a random tail whose length depends on a process-global counter that grows after
//! every format error. The ported `log_parser.rs` only passes because no format error happens in
//! its process, so tests that provoke format errors must not share a binary with it. Every
//! fixture here is built from the parser's marker accessors, never from the marker constants,
//! so the tests hold whatever the counter is.

use ig_git::log::parser::{GitLogOption, GitLogParser, NameStatus};
use ig_git::log::record::{
    ChangeType, DefaultGitLogFullRecordBuilder, DefaultGitLogRecordBuilder, GitLogFullRecord,
    GitLogRecord, VcsFileStatusInfo,
};

use GitLogOption::*;

fn plain_parser(options: &[GitLogOption]) -> GitLogParser<DefaultGitLogRecordBuilder> {
    GitLogParser::new(DefaultGitLogRecordBuilder, true, NameStatus::None, options)
}

fn status_parser(options: &[GitLogOption]) -> GitLogParser<DefaultGitLogFullRecordBuilder> {
    GitLogParser::new(
        DefaultGitLogFullRecordBuilder::default(),
        true,
        NameStatus::Status,
        options,
    )
}

/// One record of the given items, wrapped in the parser's markers.
fn record_text<B: ig_git::log::record::GitLogRecordBuilder>(
    parser: &GitLogParser<B>,
    items: &[&str],
) -> String {
    format!(
        "{}{}{}",
        parser.record_start(),
        items.join(parser.items_separator()),
        parser.record_end()
    )
}

fn hashes(records: &[GitLogRecord]) -> Vec<&str> {
    records.iter().map(|r| r.hash()).collect()
}

#[test]
fn record_end_inside_a_message_drops_only_that_commit() {
    let mut parser = plain_parser(&[Hash, Subject, Body, Parents]);
    let end = parser.record_end().to_string();
    let sep = parser.items_separator();
    let bad = format!(
        "{}{}{end}\nmore body{sep}p{end}",
        parser.record_start(),
        ["a", "subject", "body line"].join(sep),
    );
    let output = [
        record_text(&parser, &["g1", "s", "b", "p"]),
        bad,
        record_text(&parser, &["g2", "s", "b", "p"]),
    ]
    .join("\n");
    let records = parser.parse(&output);
    assert_eq!(hashes(&records), ["g1", "g2"]);
}

#[test]
fn items_separator_inside_a_message_drops_only_that_commit() {
    let mut parser = plain_parser(&[Hash, Subject, Body]);
    let sep = parser.items_separator().to_string();
    let output = [
        record_text(&parser, &["g1", "s", "b"]),
        record_text(&parser, &["a", "s", &format!("body {sep} inside")]),
        record_text(&parser, &["g2", "s", "b"]),
    ]
    .join("\n");
    let records = parser.parse(&output);
    assert_eq!(hashes(&records), ["g1", "g2"]);
}

fn info(change_type: ChangeType, first: &str, second: Option<&str>) -> VcsFileStatusInfo {
    VcsFileStatusInfo {
        change_type,
        first_path: first.to_string(),
        second_path: second.map(str::to_string),
    }
}

#[test]
fn name_status_statuses_and_quoted_paths() {
    let mut parser = status_parser(&[Hash]);
    let output = [
        record_text(&parser, &["h1"]),
        "R087\t\"\\303\\251.txt\"\tnew name.txt".to_string(),
        "C75\torig\tcopy".to_string(),
        "U\tconflicted".to_string(),
        "T\ttypechanged".to_string(),
        "A\tadded".to_string(),
        "D\tdeleted".to_string(),
        "R50\ta\tb".to_string(),
        "".to_string(),
    ]
    .join("\n");
    let records: Vec<GitLogFullRecord> = parser.parse(&output);
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].hash(), "h1");
    assert_eq!(
        records[0].status_infos(),
        [
            info(ChangeType::Moved, "\u{e9}.txt", Some("new name.txt")),
            info(ChangeType::Moved, "orig", Some("copy")),
            info(ChangeType::Modification, "conflicted", None),
            info(ChangeType::Modification, "typechanged", None),
            info(ChangeType::New, "added", None),
            info(ChangeType::Deleted, "deleted", None),
            info(ChangeType::Moved, "a", Some("b")),
        ]
    );
}

#[test]
fn fresh_parsers_work_after_format_errors_bumped_the_counter() {
    // The tail length cycles with the error count: provoke enough errors to cover every length.
    for round in 0..12 {
        let mut broken = plain_parser(&[Hash, Parents]);
        let incomplete = record_text(&broken, &["only one item"]);
        assert!(broken.parse_line(&incomplete).is_err(), "round {round}");

        let mut parser = plain_parser(&[Hash, Parents]);
        let text = record_text(&parser, &["h", "p1 p2"]);
        let records = parser.parse(&text);
        assert_eq!(hashes(&records), ["h"], "round {round}");
        assert_eq!(records[0].parents_hashes(), ["p1", "p2"], "round {round}");
    }
}

#[test]
fn malformed_record_start_discards_the_pending_record() {
    let mut parser = status_parser(&[Hash, Subject]);
    let output = [
        record_text(&parser, &["a", "s"]),
        "M\tfile".to_string(),
        // malformed: one item of two
        record_text(&parser, &["b"]),
        record_text(&parser, &["c", "s"]),
        "M\tfile".to_string(),
    ]
    .join("\n");
    let records = parser.parse(&output);
    // As upstream: the error on `b`'s line discards `a`, which was about to be returned.
    let hashes: Vec<&str> = records.iter().map(|r| r.hash()).collect();
    assert_eq!(hashes, ["c"]);
}
