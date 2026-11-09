//! Reading commits, metadata and refs with `git log`.
//!
//! Port of the timed-commit, metadata and ref parts of
//! `plugins/git4idea/backend/src/history/GitLogUtil.java`, plus `GitLogProvider.DONT_CONSIDER_SHA`
//! and `GitBranchUtil.getAllTags` (`plugins/git4idea/backend/src/branch/GitBranchUtil.java`).
//!
//! Not ported here: full details with changes (`readFullDetails*`, `GitFullDetailsCollector`,
//! `GitCommitRequirements`), file paths for filters.

use std::collections::HashSet;
use std::path::Path;

use ig_log::data::{
    DetailedLogData, Hash, TimedVcsCommit, VcsCommitMetadata, VcsRef, VcsRefType, VcsRoot, VcsUser,
};

use crate::branch::ORIGIN_HEAD;
use crate::command::GitCommand;
use crate::error::{GitError, Result};
use crate::handler::GitHandler;
use crate::log::parser::{GitLogOption, GitLogParser, NameStatus};
use crate::log::record::{DefaultGitLogRecordBuilder, GitLogRecord};
use crate::log::ref_manager::get_ref_type;
use crate::log::splitter::run_and_split;
use crate::refs::strip_refs_prefix;
use crate::runner::Git;
use crate::version::{GitVersion, GitVersionSpecialty};

pub const GRAFTED: &str = "grafted";
pub const REPLACED: &str = "replaced";
/// Equivalent to `--all`, without the index and the stash (`LOG_ALL`).
pub const LOG_ALL: [&str; 4] = ["HEAD", "--branches", "--remotes", "--tags"];
pub const STDIN: &str = "--stdin";
pub const COMMIT_METADATA_OPTIONS: [GitLogOption; 11] = [
    GitLogOption::Hash,
    GitLogOption::Parents,
    GitLogOption::CommitTime,
    GitLogOption::CommitterName,
    GitLogOption::CommitterEmail,
    GitLogOption::AuthorName,
    GitLogOption::AuthorTime,
    GitLogOption::AuthorEmail,
    GitLogOption::Subject,
    GitLogOption::Body,
    GitLogOption::RawBody,
];

/// `GitVersionSpecialty.STARTED_USING_RAW_BODY_IN_FORMAT`.
pub fn supports_raw_body(version: &GitVersion) -> bool {
    GitVersionSpecialty::StartedUsingRawBodyInFormat.exists_in(version)
}

/// `createDefaultParser(project, options)`.
pub fn create_default_parser(
    git: &Git,
    options: &[GitLogOption],
) -> GitLogParser<DefaultGitLogRecordBuilder> {
    GitLogParser::new(
        DefaultGitLogRecordBuilder,
        supports_raw_body(&git.version()),
        NameStatus::None,
        options,
    )
}

/// `getNoWalkParameter`.
pub fn get_no_walk_parameter(version: &GitVersion) -> &'static str {
    if GitVersionSpecialty::NoWalkUnsorted.exists_in(version) {
        "--no-walk=unsorted"
    } else {
        "--no-walk"
    }
}

/// `createGitHandler`: a `git log` handler for `root`.
pub fn create_git_handler(git: &Git, root: &Path, config_parameters: &[&str]) -> GitHandler {
    git.handler_with_config(root, GitCommand::LOG, config_parameters)
}

/// `sendHashesToStdin`: one hash per line, then an empty line.
pub fn send_hashes_to_stdin(hashes: &[String], handler: &mut GitHandler) {
    let mut input = String::new();
    for hash in hashes {
        input.push_str(hash);
        input.push('\n');
    }
    input.push('\n');
    handler.set_input(input.into_bytes());
}

fn build_hash(s: &str) -> Result<Hash> {
    Hash::try_build(s).map_err(|e| GitError::Command(e.to_string()))
}

fn build_hashes(hashes: &[&str]) -> Result<Vec<Hash>> {
    hashes.iter().map(|h| build_hash(h)).collect()
}

/// `readTimedCommits`: runs `git log` with `parameters` (a `--` in them separates paths) and
/// reports each commit, plus its author and refs when the consumers are given.
pub fn read_timed_commits(
    git: &Git,
    root: &VcsRoot,
    config_parameters: &[&str],
    parameters: &[&str],
    mut user_consumer: Option<&mut dyn FnMut(VcsUser)>,
    mut ref_consumer: Option<&mut dyn FnMut(VcsRef)>,
    commit_consumer: &mut dyn FnMut(TimedVcsCommit),
) -> Result<()> {
    let mut handler = create_git_handler(git, root, config_parameters);
    let mut options = vec![
        GitLogOption::Hash,
        GitLogOption::Parents,
        GitLogOption::CommitTime,
    ];
    if user_consumer.is_some() {
        options.push(GitLogOption::AuthorName);
        options.push(GitLogOption::AuthorEmail);
    }
    if ref_consumer.is_some() {
        options.push(GitLogOption::RefNames);
    }
    let mut parser = create_default_parser(git, &options);
    handler.set_stdout_suppressed(true);
    handler.add_parameters([parser.get_pretty(), "--encoding=UTF-8"]);
    handler.add_parameters(["--decorate=full"]);
    match parameters.iter().position(|p| *p == "--") {
        Some(index) => {
            handler.add_parameters(parameters[..index].iter().copied());
            handler.end_options();
            handler.add_parameters(parameters[index + 1..].iter().copied());
        }
        None => {
            handler.add_parameters(parameters.iter().copied());
            handler.end_options();
        }
    }

    let mut error = None;
    run_and_split(git, handler, &mut parser, |record| {
        if error.is_some() {
            return;
        }
        let converted = (|| -> Result<()> {
            let hash = build_hash(record.hash())?;
            let parents = build_hashes(&record.parents_hashes())?;
            commit_consumer(TimedVcsCommit::new(
                hash.clone(),
                parents,
                record.commit_time(),
            ));
            if let Some(ref_consumer) = ref_consumer.as_mut() {
                for vcs_ref in parse_refs(&record.refs(), &hash, root) {
                    ref_consumer(vcs_ref);
                }
            }
            if let Some(user_consumer) = user_consumer.as_mut() {
                user_consumer(VcsUser::create_user(
                    record.author_name(),
                    record.author_email(),
                ));
            }
            Ok(())
        })();
        if let Err(e) = converted {
            error = Some(e);
        }
    })?;
    error.map_or(Ok(()), Err)
}

/// `collectMetadata(project, root, hashes, consumer)`: the metadata of the given commits, read
/// with `git log --no-walk --stdin`.
pub fn collect_metadata(
    git: &Git,
    root: &VcsRoot,
    hashes: &[String],
    consumer: &mut dyn FnMut(VcsCommitMetadata),
) -> Result<()> {
    if hashes.is_empty() {
        return Ok(());
    }
    let mut handler = create_git_handler(git, root, &[]);
    let mut parser = create_default_parser(git, &COMMIT_METADATA_OPTIONS);
    handler.set_silent(true);
    // git show can show either -p, or --name-status, or --name-only, but we need nothing, just
    // details => using git log --no-walk
    handler.add_parameters([get_no_walk_parameter(&git.version())]);
    handler.add_parameters([parser.get_pretty(), "--encoding=UTF-8"]);
    handler.add_parameters([STDIN]);
    handler.end_options();
    send_hashes_to_stdin(hashes, &mut handler);

    let mut error = None;
    run_and_split(git, handler, &mut parser, |record| {
        if error.is_none() {
            match create_metadata(root, &record) {
                Ok(metadata) => consumer(metadata),
                Err(e) => error = Some(e),
            }
        }
    })?;
    error.map_or(Ok(()), Err)
}

/// `collectMetadata(project, root, params...)`: commits with metadata and the refs on them, read
/// with `git log <params> --decorate=full`. Refs are unique by name and type. If the command
/// fails after some commits were read, those are returned (upstream logs a warning).
pub fn collect_metadata_with_params(
    git: &Git,
    root: &VcsRoot,
    params: &[&str],
) -> Result<DetailedLogData> {
    let mut refs = RefsByNameAndType::default();
    let mut commits = Vec::new();
    let result = (|| -> Result<()> {
        let mut handler = create_git_handler(git, root, &[]);
        let mut options = COMMIT_METADATA_OPTIONS.to_vec();
        options.push(GitLogOption::RefNames);
        let mut parser = create_default_parser(git, &options);
        handler.set_stdout_suppressed(true);
        handler.add_parameters(params.iter().copied());
        handler.add_parameters([parser.get_pretty(), "--encoding=UTF-8"]);
        handler.add_parameters(["--decorate=full"]);
        handler.end_options();
        let mut error = None;
        run_and_split(git, handler, &mut parser, |record| {
            if error.is_some() {
                return;
            }
            match create_metadata(root, &record) {
                Ok(commit) => {
                    for vcs_ref in parse_refs(&record.refs(), commit_id(&commit), root) {
                        // upstream logs "Adding duplicate element" and keeps the first
                        refs.add(vcs_ref);
                    }
                    commits.push(commit);
                }
                Err(e) => error = Some(e),
            }
        })?;
        error.map_or(Ok(()), Err)
    })();
    if let Err(e) = result
        && commits.is_empty()
    {
        return Err(e);
    }
    Ok(DetailedLogData {
        commits,
        refs: refs.into_vec(),
    })
}

fn commit_id(commit: &VcsCommitMetadata) -> &Hash {
    use ig_log::GraphCommit;
    commit.id()
}

/// `parseRefs`: refs of a `%d` decoration, without `grafted`, `replaced` and `origin/HEAD`.
pub fn parse_refs(refs: &[String], hash: &Hash, root: &VcsRoot) -> Vec<VcsRef> {
    refs.iter()
        .filter(|name| *name != GRAFTED && *name != REPLACED)
        .filter_map(|ref_name| {
            let ref_type: VcsRefType = get_ref_type(ref_name);
            let name = strip_refs_prefix(ref_name);
            (name != ORIGIN_HEAD).then(|| VcsRef::new(hash.clone(), name, ref_type, root.clone()))
        })
        .collect()
}

/// `createMetadata`.
pub fn create_metadata(root: &VcsRoot, record: &GitLogRecord) -> Result<VcsCommitMetadata> {
    Ok(VcsCommitMetadata::new(
        build_hash(record.hash())?,
        build_hashes(&record.parents_hashes())?,
        record.commit_time(),
        root.clone(),
        record.subject().to_string(),
        VcsUser::create_user(record.author_name(), record.author_email()),
        record.full_message(),
        VcsUser::create_user(record.committer_name(), record.committer_email()),
        record.author_time_stamp(),
    ))
}

/// `GitBranchUtil.getAllTags`: `git tag -l --no-column`.
pub fn get_all_tags(git: &Git, root: &Path) -> Result<Vec<String>> {
    let mut handler = git.handler(root, GitCommand::TAG);
    // `--no-column`: `column.ui=always` would print several tags per line.
    handler.add_parameters(["-l", "--no-column"]);
    handler.set_silent(true);
    let mut tags = Vec::new();
    let result = git.run_command_without_collecting_output(handler, &mut |line, kind| {
        if kind == crate::console::OutputKind::Stdout && !line.is_empty() {
            tags.push(line.to_string());
        }
    });
    result.throw_on_error()?;
    Ok(tags)
}

/// A set of refs unique by name and type, in insertion order (upstream
/// `ObjectOpenCustomHashSet(GitLogProvider.DONT_CONSIDER_SHA)`).
#[derive(Default, Debug, Clone)]
pub struct RefsByNameAndType {
    keys: HashSet<(String, VcsRefType)>,
    refs: Vec<VcsRef>,
}

impl RefsByNameAndType {
    pub fn from_refs(refs: impl IntoIterator<Item = VcsRef>) -> Self {
        let mut set = RefsByNameAndType::default();
        for r in refs {
            set.add(r);
        }
        set
    }

    /// Adds `vcs_ref` unless a ref with the same name and type is present; returns whether added.
    pub fn add(&mut self, vcs_ref: VcsRef) -> bool {
        if self
            .keys
            .insert((vcs_ref.name().to_string(), vcs_ref.ref_type()))
        {
            self.refs.push(vcs_ref);
            true
        } else {
            false
        }
    }

    pub fn contains(&self, vcs_ref: &VcsRef) -> bool {
        self.keys
            .contains(&(vcs_ref.name().to_string(), vcs_ref.ref_type()))
    }

    pub fn iter(&self) -> impl Iterator<Item = &VcsRef> {
        self.refs.iter()
    }

    pub fn into_vec(self) -> Vec<VcsRef> {
        self.refs
    }
}
