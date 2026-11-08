//! Port of `plugins/git4idea/tests/git4idea/history/GitLogParserTest.kt`.
//!
//! Differences in the harness: record hashes are a deterministic 40-hex digest of the record data
//! (upstream: SHA-1 of the Kotlin map's `toString`; only uniqueness matters); parsers are created
//! with raw-body support (upstream asks the installed git, always >= 1.7.2); the `Change`
//! assertions check the `--name-status` entries through `GitChangesParser.parseChange`'s path
//! mapping (`change_of`), since `Change` objects are not ported.

use ig_git::log::collector::AsGitLogRecord;
use ig_git::log::parser::{
    GitLogOption, GitLogParser, ITEMS_SEPARATOR, NameStatus, RECORD_END, RECORD_START,
};
use ig_git::log::record::{
    ChangeType, DefaultGitLogFullRecordBuilder, DefaultGitLogRecordBuilder, GitLogFullRecord,
    GitLogRecordBuilder, VcsFileStatusInfo,
};

use GitLogOption::*;

const GIT_LOG_OPTIONS: [GitLogOption; 12] = [
    Hash,
    CommitTime,
    AuthorName,
    AuthorTime,
    AuthorEmail,
    CommitterName,
    CommitterEmail,
    Subject,
    Body,
    Parents,
    RawBody,
    RefNames,
];

#[derive(Clone)]
struct GitTestChange {
    change_type: ChangeType,
    before_path: Option<String>,
    after_path: Option<String>,
}

impl GitTestChange {
    fn to_output_string(&self) -> String {
        let status = match self.change_type {
            ChangeType::Moved => "R100",
            ChangeType::Modification => "M",
            ChangeType::Deleted => "D",
            ChangeType::New => "A",
        };
        let mut sb = format!("{status}\t");
        if let Some(before) = &self.before_path {
            sb.push_str(before);
            sb.push('\t');
        }
        if let Some(after) = &self.after_path {
            sb.push_str(after);
            sb.push('\t');
        }
        sb.push('\n');
        sb
    }
}

fn added(path: &str) -> GitTestChange {
    GitTestChange {
        change_type: ChangeType::New,
        before_path: None,
        after_path: Some(path.into()),
    }
}

fn deleted(path: &str) -> GitTestChange {
    GitTestChange {
        change_type: ChangeType::Deleted,
        before_path: Some(path.into()),
        after_path: None,
    }
}

fn modified(path: &str) -> GitTestChange {
    GitTestChange {
        change_type: ChangeType::Modification,
        before_path: Some(path.into()),
        after_path: Some(path.into()),
    }
}

fn moved(before: &str, after: &str) -> GitTestChange {
    GitTestChange {
        change_type: ChangeType::Moved,
        before_path: Some(before.into()),
        after_path: Some(after.into()),
    }
}

#[derive(Clone)]
struct GitTestLogRecord {
    hash: String,
    /// milliseconds
    commit_time: i64,
    author_time: i64,
    author_name: String,
    author_email: String,
    committer_name: String,
    committer_email: String,
    subject: String,
    body: String,
    parents: Vec<String>,
    refs: Vec<String>,
    changes: Vec<GitTestChange>,
    new_refs_format: bool,
}

impl GitTestLogRecord {
    fn raw_body(&self) -> String {
        format!("{}\n\n{}", self.subject, self.body)
    }

    fn refs_for_output(&self) -> String {
        if self.refs.is_empty() {
            return String::new();
        }
        let mut refs = self.refs.clone();
        if self.new_refs_format {
            let mut new_refs = Vec::new();
            let mut head_ref_met = false;
            for r in refs {
                if r == "HEAD" {
                    head_ref_met = true;
                } else if head_ref_met {
                    new_refs.push(format!("HEAD -> {r}"));
                    head_ref_met = false;
                } else {
                    new_refs.push(r);
                }
            }
            refs = new_refs;
        }
        format!("({})", refs.join(", "))
    }

    fn paths(&self) -> Vec<String> {
        let mut paths = Vec::new();
        for change in &self.changes {
            match change.change_type {
                ChangeType::Modification | ChangeType::New => {
                    paths.push(change.after_path.clone().unwrap())
                }
                ChangeType::Deleted => paths.push(change.before_path.clone().unwrap()),
                ChangeType::Moved => {
                    paths.push(change.before_path.clone().unwrap());
                    paths.push(change.after_path.clone().unwrap());
                }
            }
        }
        paths
    }

    fn prepare_output_line(&self, name_status: NameStatus) -> String {
        let mut sb = String::from(RECORD_START);
        let values: Vec<String> = GIT_LOG_OPTIONS
            .iter()
            .map(|o| self.option_to_value(*o))
            .collect();
        sb.push_str(&values.join(ITEMS_SEPARATOR));
        sb.push_str(RECORD_END);
        if name_status == NameStatus::Status {
            sb.push_str("\n\n");
            sb.push_str(
                &self
                    .changes
                    .iter()
                    .map(GitTestChange::to_output_string)
                    .collect::<String>(),
            );
        }
        sb
    }

    fn option_to_value(&self, option: GitLogOption) -> String {
        match option {
            RawBody => self.raw_body(),
            CommitTime => (self.commit_time / 1000).to_string(),
            AuthorTime => (self.author_time / 1000).to_string(),
            Parents => self.parents.join(" "),
            RefNames => self.refs_for_output(),
            Hash => self.hash.clone(),
            AuthorName => self.author_name.clone(),
            AuthorEmail => self.author_email.clone(),
            CommitterName => self.committer_name.clone(),
            CommitterEmail => self.committer_email.clone(),
            Subject => self.subject.clone(),
            Body => self.body.clone(),
            other => panic!("unexpected option {other:?}"),
        }
    }
}

/// A deterministic 40-hex digest standing in for `DigestUtil.sha1Hex`.
fn digest_hex(data: &str) -> String {
    (0u64..3)
        .map(|seed| {
            let mut h: u64 = 0xcbf29ce484222325 ^ seed;
            for b in data.bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            format!("{h:016x}")
        })
        .collect::<String>()[..40]
        .to_string()
}

/// `createTestRecord`: defaults overridden by `customize`; the hash is computed from the data.
fn create_test_record(customize: impl FnOnce(&mut GitTestLogRecord)) -> GitTestLogRecord {
    let mut record = GitTestLogRecord {
        hash: String::new(),
        commit_time: 1315471452 * 1000,
        author_time: 1317027817 * 1000,
        author_name: "John Doe".into(),
        author_email: "John.Doe@example.com".into(),
        committer_name: "John Doe".into(),
        committer_email: "John.Doe@example.com".into(),
        subject: "Subject".into(),
        body: "Body".into(),
        parents: Vec::new(),
        refs: Vec::new(),
        changes: Vec::new(),
        new_refs_format: false,
    };
    customize(&mut record);
    let data = format!(
        "{} {} {} {} {} {} {} {} {:?} {:?}",
        record.commit_time,
        record.author_time,
        record.author_name,
        record.author_email,
        record.committer_name,
        record.committer_email,
        record.subject,
        record.body,
        record.parents,
        record.refs
    );
    record.hash = digest_hex(&data);
    record
}

fn generate_record_with_subject(subject: &str) -> GitTestLogRecord {
    create_test_record(|r| {
        r.subject = subject.into();
        r.body = "Small description".into();
        r.parents = vec!["2c815939f45fbcfda9583f84b14fe9d393ada790".into()];
        r.changes = vec![
            modified("src/CClass.java"),
            added("src/OtherClass.java"),
            deleted("src/OldClass.java"),
        ];
    })
}

fn generate_records(new_refs_format: bool) -> Vec<GitTestLogRecord> {
    let mut records = Vec::new();
    records.push(create_test_record(|r| {
        r.committer_name = "Bob Smith".into();
        r.committer_email = "Bob@site.com".into();
        r.subject = "Commit message".into();
        r.body =
            "Description goes here\n\nThen comes a long long description.\nProbably multilined."
                .into();
        r.refs = [
            "HEAD",
            "refs/heads/master",
            "refs/heads/(ref1)",
            "refs/heads/ref2",
        ]
        .map(String::from)
        .to_vec();
        r.changes = vec![
            moved("file2", "file3"),
            added("readme.txt"),
            modified("src/CClass.java"),
            deleted("src/ChildAClass.java"),
        ];
        r.new_refs_format = new_refs_format;
    }));
    let first = records[0].hash.clone();
    records.push(create_test_record(|r| {
        r.subject = "Commit message".into();
        r.body = "Small description".into();
        r.parents = vec![first.clone()];
        r.changes = vec![modified("src/CClass.java")];
        r.new_refs_format = new_refs_format;
    }));
    let second = records[1].hash.clone();
    records.push(create_test_record(|r| {
        r.subject = "Commit message".into();
        r.body = "Small description".into();
        r.parents = vec![first, second];
        r.refs = [
            "refs/heads/sly->name",
            "refs/remotes/origin/master",
            "refs/tags/v1.0",
        ]
        .map(String::from)
        .to_vec();
        r.changes = vec![modified("src/CClass.java")];
        r.new_refs_format = new_refs_format;
    }));
    records
}

fn full_parser(name_status: NameStatus) -> GitLogParser<DefaultGitLogFullRecordBuilder> {
    GitLogParser::new(
        DefaultGitLogFullRecordBuilder::default(),
        true,
        name_status,
        &GIT_LOG_OPTIONS,
    )
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// `GitChangesParser.parseChange` reduced to (type of the resulting `Change`, before, after).
fn change_of(info: &VcsFileStatusInfo) -> (ChangeType, Option<String>, Option<String>) {
    let path = Some(info.first_path.clone());
    match info.change_type {
        ChangeType::New => (ChangeType::New, None, path),
        ChangeType::Modification => (ChangeType::Modification, path.clone(), path),
        ChangeType::Deleted => (ChangeType::Deleted, path, None),
        ChangeType::Moved => {
            let after = info.second_path.clone().or(path.clone());
            // `Change.getType`: MOVED only if the paths differ
            let t = if after == path {
                ChangeType::Modification
            } else {
                ChangeType::Moved
            };
            (t, path, after)
        }
    }
}

fn assert_record(
    actual: &impl AsGitLogRecord,
    statuses: Option<&GitLogFullRecord>,
    expected: &GitTestLogRecord,
    option: NameStatus,
) {
    let actual = actual.as_record();
    assert_eq!(actual.hash(), expected.hash);

    assert_eq!(actual.committer_name(), expected.committer_name);
    assert_eq!(actual.committer_email(), expected.committer_email);
    assert_eq!(actual.commit_time(), expected.commit_time);

    assert_eq!(actual.author_name(), expected.author_name);
    assert_eq!(actual.author_email(), expected.author_email);
    assert_eq!(actual.author_time_stamp(), expected.author_time);

    assert_eq!(actual.subject(), expected.subject);
    assert_eq!(actual.body(), expected.body);
    assert_eq!(actual.raw_body(), expected.raw_body());

    let parents: Vec<String> = actual
        .parents_hashes()
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(sorted(parents), sorted(expected.parents.clone()));
    assert_eq!(sorted(actual.refs()), sorted(expected.refs.clone()));

    if option == NameStatus::Status {
        let full = statuses.expect("is a GitLogFullRecord");
        let actual_paths: Vec<String> = full.paths().iter().map(|s| s.to_string()).collect();
        assert_eq!(actual_paths, expected.paths());

        let actual_changes: Vec<_> = full.status_infos().iter().map(change_of).collect();
        assert_eq!(actual_changes.len(), expected.changes.len());
        for (actual_change, expected_change) in actual_changes.iter().zip(&expected.changes) {
            assert_change(actual_change, expected_change);
        }
    }
}

fn assert_change(actual: &(ChangeType, Option<String>, Option<String>), expected: &GitTestChange) {
    assert_eq!(expected.change_type, actual.0);
    match actual.0 {
        ChangeType::Modification | ChangeType::Moved => {
            assert_eq!(expected.before_path, actual.1);
            assert_eq!(expected.after_path, actual.2);
        }
        ChangeType::New => assert_eq!(expected.after_path, actual.2),
        ChangeType::Deleted => assert_eq!(expected.before_path, actual.1),
    }
}

fn do_test_all_records(name_status: NameStatus, new_refs_format: bool) {
    let expected_records = generate_records(new_refs_format);
    let mut parser = full_parser(name_status);
    let output = expected_records
        .iter()
        .map(|r| r.prepare_output_line(name_status))
        .collect::<Vec<_>>()
        .join("\n");
    let actual_records = parser.parse(&output);
    assert_eq!(expected_records.len(), actual_records.len());
    for (actual, expected) in actual_records.iter().zip(&expected_records) {
        assert_record(actual, Some(actual), expected, name_status);
    }
}

fn do_test_one_record<B>(
    mut parser: GitLogParser<B>,
    option: NameStatus,
    statuses: impl Fn(&B::Record) -> Option<&GitLogFullRecord>,
) where
    B: GitLogRecordBuilder,
    B::Record: AsGitLogRecord,
{
    let expected_record = generate_record_with_subject("Subject");
    let s = expected_record.prepare_output_line(option);
    let actual_record = parser.parse_one_record(&s).unwrap();
    assert_record(
        &actual_record,
        statuses(&actual_record),
        &expected_record,
        option,
    );
}

fn do_test_custom_commit_message(subject: &str) {
    let record = generate_record_with_subject(subject);
    let mut parser = full_parser(NameStatus::Status);
    let s = record.prepare_output_line(NameStatus::None);
    let records = parser.parse(&s);
    assert_eq!(
        records.len(),
        1,
        "Incorrect amount of actual records: {records:?}"
    );
    assert_eq!(records[0].subject(), subject, "Commit subject is incorrect");
}

#[test]
fn test_parse_all_without_name_status() {
    do_test_all_records(NameStatus::None, false);
}

#[test]
fn test_parse_all_with_name_status() {
    do_test_all_records(NameStatus::Status, false);
}

#[test]
fn test_parse_one_record_without_name_status() {
    let parser = GitLogParser::new(
        DefaultGitLogRecordBuilder,
        true,
        NameStatus::None,
        &GIT_LOG_OPTIONS,
    );
    do_test_one_record(parser, NameStatus::None, |_| None);
}

#[test]
fn test_parse_one_record_with_name_status() {
    do_test_one_record(full_parser(NameStatus::Status), NameStatus::Status, |r| {
        Some(r)
    });
}

#[test]
fn test_char_0001_in_commit_message() {
    do_test_custom_commit_message("Commit \u{1}subject");
}

#[test]
fn test_double_char_0001_in_commit_message() {
    do_test_custom_commit_message("Commit \u{1}\u{1}subject");
}

#[test]
fn test_char_0003_in_commit_message() {
    do_test_custom_commit_message("Commit \u{3}subject");
}

#[test]
fn test_double_char_0003_in_commit_message() {
    do_test_custom_commit_message("Commit \u{3}\u{3}subject");
}

#[test]
fn test_both_chars_0001_and_0003_in_commit_message() {
    do_test_custom_commit_message("Subject \u{1}of the \u{3}# weirdmessage");
}

#[test]
fn test_both_double_chars_0001_and_0003_in_commit_message() {
    do_test_custom_commit_message("Subject \u{1}\u{1}of the \u{3}\u{3}# weirdmessage");
}

#[test]
fn test_char_0001_twice_in_commit_message() {
    do_test_custom_commit_message("Subject \u{1}of the \u{1}# weird message");
}

#[test]
fn test_double_char_0001_twice_in_commit_message() {
    do_test_custom_commit_message("Subject \u{1}\u{1}of the \u{1}\u{1}# weird message");
}

#[test]
fn test_old_refs_format() {
    do_test_all_records(NameStatus::None, false);
}

#[test]
fn test_new_refs_format() {
    do_test_all_records(NameStatus::Status, true);
}

#[test]
fn test_files_with_spaces() {
    let mut parser = full_parser(NameStatus::Status);
    let expected_record =
        create_test_record(|r| r.changes = vec![modified("file "), modified(" ")]);
    let actual_record = parser
        .parse_one_record(&expected_record.prepare_output_line(NameStatus::Status))
        .unwrap();
    assert_record(
        &actual_record,
        Some(&actual_record),
        &expected_record,
        NameStatus::Status,
    );
}
