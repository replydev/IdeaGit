//! IdeaGit tests for `GitLogOutputSplitter` and `GitLogRecordCollector` against real `git log`
//! output (upstream tests them only through full details).

mod common;

use common::log_helpers::{init_master_repo, last, tac};
use common::{git, runner};
use ig_git::log::collector::GitLogRecordCollector;
use ig_git::log::parser::{GitLogOption, GitLogParser, NameStatus};
use ig_git::log::record::{ChangeType, DefaultGitLogFullRecordBuilder, GitLogFullRecord};
use ig_git::log::splitter::run_and_split;
use ig_git::log::util::{
    COMMIT_METADATA_OPTIONS, create_default_parser, create_git_handler, supports_raw_body,
};

#[test]
fn collector_adds_empty_record_for_parent_with_same_tree() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_master_repo(dir);
    git(dir, &["checkout", "-q", "-b", "feature"]);
    tac(dir, "f.txt");
    git(dir, &["checkout", "-q", "master"]);
    tac(dir, "b.txt");
    // the merge keeps master's tree: `git log -m` prints no record for the first parent
    git(
        dir,
        &["merge", "-q", "-s", "ours", "feature", "-m", "merge"],
    );
    let merge = last(dir);

    let g = runner();
    let mut options = vec![GitLogOption::Hash, GitLogOption::Parents];
    options.extend(COMMIT_METADATA_OPTIONS.iter().skip(2));
    let mut parser = GitLogParser::new(
        DefaultGitLogFullRecordBuilder::default(),
        supports_raw_body(&g.version()),
        NameStatus::Status,
        &options,
    );
    let mut handler = create_git_handler(&g, dir, &[]);
    handler.add_parameters(["-m", "--name-status", parser.get_pretty(), "-1", &merge]);
    handler.end_options();

    let mut grouped: Vec<Vec<GitLogFullRecord>> = Vec::new();
    let mut raw_count = 0;
    {
        let mut collector = GitLogRecordCollector::new(
            &g,
            dir,
            |records| grouped.push(records),
            |r: &GitLogFullRecord| {
                GitLogFullRecord::new(r.options().clone(), Vec::new(), r.is_supports_raw_body())
            },
        );
        run_and_split(&g, handler, &mut parser, |record| {
            raw_count += 1;
            collector.accept(record);
        })
        .unwrap();
        collector.finish();
        assert!(collector.take_error().is_none());
    }
    assert_eq!(raw_count, 1, "git omits the record of the identical parent");
    assert_eq!(grouped.len(), 1);
    let records = &grouped[0];
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].hash(), merge);
    assert!(
        records[0].status_infos().is_empty(),
        "empty record for the first parent"
    );
    let second: Vec<_> = records[1]
        .status_infos()
        .iter()
        .map(|s| (s.change_type, s.first_path.as_str()))
        .collect();
    assert_eq!(
        second,
        [(ChangeType::New, "b.txt"), (ChangeType::Deleted, "f.txt")]
    );
}

#[test]
fn splitter_reports_command_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_master_repo(dir);
    let g = runner();
    let mut parser = create_default_parser(&g, &[GitLogOption::Hash]);
    let mut handler = create_git_handler(&g, dir, &[]);
    handler.add_parameters([parser.get_pretty(), "no-such-revision"]);
    handler.end_options();
    let mut count = 0;
    let error = run_and_split(&g, handler, &mut parser, |_| count += 1).unwrap_err();
    assert!(error.to_string().contains("no-such-revision"), "{error}");
    assert_eq!(count, 0);
}

#[test]
fn splitter_streams_all_records_of_a_long_log() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_master_repo(dir);
    for i in 0..30 {
        tac(dir, &format!("f{i}.txt"));
    }
    let g = runner();
    let mut parser = create_default_parser(&g, &COMMIT_METADATA_OPTIONS);
    let mut handler = create_git_handler(&g, dir, &[]);
    handler.add_parameters([parser.get_pretty()]);
    handler.end_options();
    let mut subjects = Vec::new();
    run_and_split(&g, handler, &mut parser, |r| {
        subjects.push(r.subject().to_string())
    })
    .unwrap();
    assert_eq!(subjects.len(), 31);
    assert_eq!(subjects[0], "Touched f29.txt");
    assert_eq!(subjects[30], "Touched initial.txt");
}
