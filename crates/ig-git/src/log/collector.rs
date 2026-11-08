//! Grouping the records of one commit, as `git log -m` prints one record per parent.
//!
//! Port of `plugins/git4idea/backend/src/history/GitLogRecordCollector.java`.
//!
//! `git log -m` omits the record for a parent whose tree equals the merge commit's; the collector
//! then reads the tree hashes and inserts empty records for such parents.
//!
//! Deviation: errors upstream only logs (reading tree hashes fails) drop the incomplete commit;
//! the error is kept for [`GitLogRecordCollector::take_error`]. A missing commit or parent tree
//! hash (upstream asserts, and a missing parent tree would then throw an NPE) is likewise recorded
//! there, and the parent is skipped (no empty record is inserted for it).
//!
//! Not ported: `GitLogUnorderedRecordCollector`. It is only used by compressed details, which are
//! not ported (see `log/util.rs`).
//!
//! Note: the linear lookup of a hash in `hash_to_record` assumes the records of one hash are
//! consecutive, as git emits them (so at most a couple of entries are pending).

use std::collections::HashMap;
use std::path::Path;

use crate::error::Result;
use crate::log::parser::GitLogOption;
use crate::log::record::{GitLogFullRecord, GitLogRecord};
use crate::log::util::{
    STDIN, create_default_parser, create_git_handler, get_no_walk_parameter, send_hashes_to_stdin,
};
use crate::runner::Git;

/// Access to the common record of plain and full records.
pub trait AsGitLogRecord {
    fn as_record(&self) -> &GitLogRecord;
}

impl AsGitLogRecord for GitLogRecord {
    fn as_record(&self) -> &GitLogRecord {
        self
    }
}

impl AsGitLogRecord for GitLogFullRecord {
    fn as_record(&self) -> &GitLogRecord {
        self.record()
    }
}

/// Collects the records of each commit and passes them together to a consumer
/// (`GitLogRecordCollector`; `create_empty_copy` is the abstract `createEmptyCopy`).
pub struct GitLogRecordCollector<'a, R, F, E> {
    git: &'a Git,
    root: &'a Path,
    consumer: F,
    create_empty_copy: E,
    /// `MultiMap.createLinked()`: hashes in insertion order.
    hash_to_record: Vec<(String, Vec<R>)>,
    last_hash: Option<String>,
    error: Option<crate::error::GitError>,
}

impl<'a, R, F, E> GitLogRecordCollector<'a, R, F, E>
where
    R: AsGitLogRecord,
    F: FnMut(Vec<R>),
    E: Fn(&R) -> R,
{
    pub fn new(git: &'a Git, root: &'a Path, consumer: F, create_empty_copy: E) -> Self {
        GitLogRecordCollector {
            git,
            root,
            consumer,
            create_empty_copy,
            hash_to_record: Vec::new(),
            last_hash: None,
            error: None,
        }
    }

    /// `accept`.
    pub fn accept(&mut self, record: R) {
        let hash = record.as_record().hash().to_string();
        if self.last_hash.as_deref() != Some(hash.as_str()) {
            self.process_collected_records();
        }
        self.last_hash = Some(hash.clone());

        let parents_count = record.as_record().parents_hashes().len();
        let records_count = match self.hash_to_record.iter_mut().find(|(h, _)| *h == hash) {
            Some((_, records)) => {
                records.push(record);
                records.len()
            }
            None => {
                self.hash_to_record.push((hash, vec![record]));
                1
            }
        };
        if parents_count == 0 || parents_count == records_count {
            self.process_collected_records();
        }
    }

    /// `finish`.
    pub fn finish(&mut self) {
        self.process_collected_records();
    }

    /// The first error met while completing records, if any.
    pub fn take_error(&mut self) -> Option<crate::error::GitError> {
        self.error.take()
    }

    fn process_collected_records(&mut self) {
        for (hash, records) in std::mem::take(&mut self.hash_to_record) {
            let parents = records[0].as_record().parents_hashes().len();
            if parents != 0 && records.len() != parents {
                self.process_incomplete_record(hash, records);
            } else {
                (self.consumer)(records);
            }
        }
    }

    fn process_incomplete_record(&mut self, hash: String, records: Vec<R>) {
        if let Err(e) = self.process_incomplete_records(vec![(hash, records)]) {
            self.error.get_or_insert(e);
        }
    }

    /// `processIncompleteRecords`.
    pub fn process_incomplete_records(
        &mut self,
        incomplete_records: Vec<(String, Vec<R>)>,
    ) -> Result<()> {
        let first_records: Vec<&GitLogRecord> = incomplete_records
            .iter()
            .map(|(_, records)| records[0].as_record())
            .collect();
        let hash_to_tree_map = get_hash_to_tree_map(self.git, self.root, &first_records)?;
        for (_, mut records) in incomplete_records {
            self.fill_with_empty_records(&mut records, &hash_to_tree_map);
            (self.consumer)(records);
        }
        Ok(())
    }

    fn record_error(&mut self, message: String) {
        self.error
            .get_or_insert(crate::error::GitError::Command(message));
    }

    /// `fillWithEmptyRecords`: an empty record for each parent with the commit's tree.
    fn fill_with_empty_records(
        &mut self,
        records: &mut Vec<R>,
        hash_to_tree_map: &HashMap<String, String>,
    ) {
        let first = records[0].as_record();
        let commit_hash = first.hash().to_string();
        let parents: Vec<String> = first
            .parents_hashes()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let commit_tree_hash = hash_to_tree_map.get(first.hash()).cloned();
        if commit_tree_hash.is_none() {
            self.record_error(format!("No tree hash for commit {commit_hash}"));
        }
        for (parent_index, parent) in parents.iter().enumerate() {
            // sometimes a merge commit is identical to all its parents: we get one empty record
            let parent_tree_hash = hash_to_tree_map.get(parent);
            if parent_tree_hash.is_none() {
                self.record_error(format!(
                    "No tree hash for parent {parent} of commit {commit_hash}"
                ));
                continue;
            }
            if commit_tree_hash.is_some()
                && parent_tree_hash == commit_tree_hash.as_ref()
                && records.len() < parents.len()
            {
                let empty = (self.create_empty_copy)(&records[0]);
                records.insert(parent_index, empty);
            }
        }
    }
}

/// `getHashToTreeMap`: tree hashes of the commits and their parents.
fn get_hash_to_tree_map(
    git: &Git,
    root: &Path,
    records: &[&GitLogRecord],
) -> Result<HashMap<String, String>> {
    let mut hashes: Vec<String> = Vec::new();
    for r in records {
        for h in std::iter::once(r.hash()).chain(r.parents_hashes()) {
            if !hashes.iter().any(|x| x == h) {
                hashes.push(h.to_string());
            }
        }
    }
    let mut handler = create_git_handler(git, root, &[]);
    let mut parser = create_default_parser(git, &[GitLogOption::Hash, GitLogOption::Tree]);
    handler.set_stdout_suppressed(true);
    handler.add_parameters([parser.get_pretty()]);
    handler.add_parameters([get_no_walk_parameter(&git.version())]);
    handler.add_parameters([STDIN]);
    handler.end_options();
    send_hashes_to_stdin(&hashes, &mut handler);

    let output = git.run_command(handler).output_or_error()?;
    Ok(parser
        .parse(&output)
        .into_iter()
        .map(|record| (record.hash().to_string(), record.tree_hash().to_string()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::NullConsole;
    use crate::log::parser::OptionValues;
    use std::sync::Arc;

    fn record(hash: &str, parents: &str) -> GitLogRecord {
        let mut options: OptionValues = Default::default();
        options[GitLogOption::Hash.index()] = Some(hash.to_string());
        options[GitLogOption::Parents.index()] = Some(parents.to_string());
        GitLogRecord::new(options, true)
    }

    #[test]
    fn missing_trees_are_reported_and_parents_skipped() {
        let git = Git::new(
            "git",
            crate::version::GitVersion::NULL,
            Arc::new(NullConsole),
        );
        let root = Path::new("/nonexistent");
        let mut out: Vec<Vec<GitLogRecord>> = Vec::new();
        let mut collector = GitLogRecordCollector::new(
            &git,
            root,
            |records| out.push(records),
            |r: &GitLogRecord| GitLogRecord::new(r.options().clone(), true),
        );

        // parent tree missing: reported, no empty record
        let trees = HashMap::from([("c".to_string(), "t".to_string())]);
        let mut records = vec![record("c", "p1 p2")];
        collector.fill_with_empty_records(&mut records, &trees);
        assert_eq!(records.len(), 1);
        let error = collector.take_error().expect("parent tree is missing");
        assert!(error.to_string().contains("parent p1"), "{error}");

        // commit tree missing: reported, no empty record
        let trees = HashMap::from([("p1".to_string(), "t".to_string())]);
        let mut records = vec![record("c", "p1 p2")];
        collector.fill_with_empty_records(&mut records, &trees);
        assert_eq!(records.len(), 1);
        let error = collector.take_error().expect("commit tree is missing");
        assert!(error.to_string().contains("commit c"), "{error}");

        // all trees present: the identical parent gets an empty record
        let trees = HashMap::from([
            ("c".to_string(), "t".to_string()),
            ("p1".to_string(), "t".to_string()),
            ("p2".to_string(), "other".to_string()),
        ]);
        let mut records = vec![record("c", "p1 p2")];
        collector.fill_with_empty_records(&mut records, &trees);
        assert_eq!(records.len(), 2);
        assert!(collector.take_error().is_none());
    }
}
