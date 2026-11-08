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

/// A failing git command surfaces as an error through `run_and_split`. Note that this exercises
/// `throw_on_error` with the raw stderr (it runs before `report_errors`); the splitter's own
/// "exited with the code" message is covered by the unit tests in `splitter.rs`.
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

fn full_parser(g: &ig_git::runner::Git) -> GitLogParser<DefaultGitLogFullRecordBuilder> {
    let mut options = vec![GitLogOption::Hash, GitLogOption::Parents];
    options.extend(COMMIT_METADATA_OPTIONS.iter().skip(2));
    GitLogParser::new(
        DefaultGitLogFullRecordBuilder::default(),
        supports_raw_body(&g.version()),
        NameStatus::Status,
        &options,
    )
}

fn empty_copy(r: &GitLogFullRecord) -> GitLogFullRecord {
    GitLogFullRecord::new(r.options().clone(), Vec::new(), r.is_supports_raw_body())
}

/// Runs `git log -m --name-status <args>` through splitter and collector.
fn collect_groups(
    g: &ig_git::runner::Git,
    dir: &std::path::Path,
    args: &[&str],
) -> (Vec<Vec<GitLogFullRecord>>, Option<ig_git::error::GitError>) {
    let mut parser = full_parser(g);
    let mut handler = create_git_handler(g, dir, &[]);
    handler.add_parameters(["-m", "--name-status", parser.get_pretty()]);
    handler.add_parameters(args.iter().copied());
    handler.end_options();
    let mut grouped = Vec::new();
    let error;
    {
        let mut collector =
            GitLogRecordCollector::new(g, dir, |records| grouped.push(records), empty_copy);
        run_and_split(g, handler, &mut parser, |record| collector.accept(record)).unwrap();
        collector.finish();
        error = collector.take_error();
    }
    (grouped, error)
}

#[test]
fn collector_groups_true_merge_in_parent_order_among_other_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_master_repo(dir);
    git(dir, &["checkout", "-q", "-b", "feature"]);
    tac(dir, "f.txt");
    git(dir, &["checkout", "-q", "master"]);
    tac(dir, "b.txt");
    // a real merge: the tree differs from both parents
    git(dir, &["merge", "-q", "--no-ff", "feature", "-m", "merge"]);
    let merge = last(dir);
    tac(dir, "c.txt");

    let g = runner();
    let (grouped, error) = collect_groups(&g, dir, &["--topo-order", "master"]);
    assert!(error.is_none());

    let expected: Vec<String> = git(dir, &["rev-list", "--topo-order", "master"])
        .lines()
        .map(str::to_string)
        .collect();
    let hashes: Vec<&str> = grouped.iter().map(|g| g[0].hash()).collect();
    assert_eq!(hashes, expected);
    let sizes: Vec<usize> = grouped.iter().map(Vec::len).collect();
    // c, merge, then b, f and initial in some topological order
    assert_eq!(sizes, [1, 2, 1, 1, 1]);

    let group = &grouped[1];
    assert_eq!(group[0].hash(), merge);
    assert_eq!(group[1].hash(), merge);
    let status = |r: &GitLogFullRecord| -> Vec<(ChangeType, String)> {
        r.status_infos()
            .iter()
            .map(|s| (s.change_type, s.first_path.clone()))
            .collect()
    };
    // against the first parent (master): f.txt arrives; against the second (feature): b.txt
    assert_eq!(status(&group[0]), [(ChangeType::New, "f.txt".to_string())]);
    assert_eq!(status(&group[1]), [(ChangeType::New, "b.txt".to_string())]);
}

#[test]
fn collector_accept_flushes_per_hash_and_on_parent_count() {
    use ig_git::log::parser::OptionValues;
    use ig_git::log::record::GitLogRecord;

    fn record(hash: &str, parents: &str) -> GitLogRecord {
        let mut options: OptionValues = Default::default();
        options[GitLogOption::Hash.index()] = Some(hash.to_string());
        options[GitLogOption::Parents.index()] = Some(parents.to_string());
        GitLogRecord::new(options, true)
    }

    // never reaches git: every group is complete
    let g = runner();
    let root = std::path::Path::new("/nonexistent");
    let mut out: Vec<Vec<String>> = Vec::new();
    let mut collector = GitLogRecordCollector::new(
        &g,
        root,
        |records: Vec<GitLogRecord>| {
            out.push(records.iter().map(|r| r.hash().to_string()).collect())
        },
        |r: &GitLogRecord| GitLogRecord::new(r.options().clone(), true),
    );
    // root commit: no parents, flushed at once
    collector.accept(record("a", ""));
    // merge: flushed when the second of two records arrives
    collector.accept(record("b", "x y"));
    collector.accept(record("b", "x y"));
    // single parent
    collector.accept(record("c", "z"));
    collector.finish();
    assert!(collector.take_error().is_none());
    drop(collector);
    assert_eq!(out, [vec!["a"], vec!["b", "b"], vec!["c"]]);

    // finish() leaves nothing behind: a second one adds nothing
    let mut out2: Vec<usize> = Vec::new();
    let mut collector = GitLogRecordCollector::new(
        &g,
        root,
        |records: Vec<GitLogRecord>| out2.push(records.len()),
        |r: &GitLogRecord| GitLogRecord::new(r.options().clone(), true),
    );
    collector.accept(record("d", "p q"));
    collector.accept(record("d", "p q"));
    collector.finish();
    collector.finish();
    drop(collector);
    assert_eq!(out2, [2]);
}

#[test]
fn empty_range_yields_no_records_and_no_error() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_master_repo(dir);
    let g = runner();
    let (grouped, error) = collect_groups(&g, dir, &["HEAD..HEAD"]);
    assert!(grouped.is_empty());
    assert!(error.is_none());
}

#[test]
fn collector_drops_commit_and_reports_error_when_tree_lookup_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    init_master_repo(dir);
    let g = runner();
    let bogus = |c: char| c.to_string().repeat(40);
    let mut grouped: Vec<Vec<ig_git::log::record::GitLogRecord>> = Vec::new();
    let mut collector = GitLogRecordCollector::new(
        &g,
        dir,
        |records| grouped.push(records),
        |r: &ig_git::log::record::GitLogRecord| {
            ig_git::log::record::GitLogRecord::new(r.options().clone(), true)
        },
    );
    let mut options: ig_git::log::parser::OptionValues = Default::default();
    options[GitLogOption::Hash.index()] = Some(bogus('a'));
    options[GitLogOption::Parents.index()] = Some(format!("{} {}", bogus('b'), bogus('c')));
    // one record for a two-parent commit: incomplete, so the trees are looked up and fail
    collector.accept(ig_git::log::record::GitLogRecord::new(options, true));
    collector.finish();
    assert!(collector.take_error().is_some());
    assert!(collector.take_error().is_none(), "taken once");
    drop(collector);
    assert!(grouped.is_empty(), "the commit is dropped");
}
