//! Port of `platform/vcs-log/impl/test/com/intellij/vcs/log/impl/TestVcsLogProvider.java`.
//!
//! Without the semaphores that block reading: the refresher is synchronous.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

use ig_log::GraphCommit;
use ig_log::data::{
    DetailedLogData, LogData, Requirements, TimedVcsCommit, VcsCommitMetadata, VcsException,
    VcsLogProvider, VcsLogRefManager, VcsRef, VcsRefType, VcsRoot, VcsUser,
};

pub const BRANCH_TYPE: VcsRefType = VcsRefType::new("BRANCH", true);
const SAMPLE_SUBJECT: &str = "Sample subject";

pub fn default_user() -> VcsUser {
    VcsUser::create_user("John Smith", "John.Smith@mail.com")
}

/// `MockRefManager`: every pair of refs compares equal.
struct MockRefManager;

impl VcsLogRefManager for MockRefManager {
    fn branch_layout_compare(&self, _: &VcsRef, _: &VcsRef) -> Ordering {
        Ordering::Equal
    }
    fn labels_order_compare(&self, _: &VcsRef, _: &VcsRef) -> Ordering {
        Ordering::Equal
    }
    fn name(&self) -> String {
        "MockRefManager".to_string()
    }
}

pub struct TestVcsLogProvider {
    commits: Mutex<Vec<TimedVcsCommit>>,
    refs: Mutex<HashSet<VcsRef>>,
    ref_manager: Arc<dyn VcsLogRefManager>,
    read_first_block_counter: AtomicUsize,
}

impl Default for TestVcsLogProvider {
    fn default() -> Self {
        TestVcsLogProvider {
            commits: Mutex::new(Vec::new()),
            refs: Mutex::new(HashSet::new()),
            ref_manager: Arc::new(MockRefManager),
            read_first_block_counter: AtomicUsize::new(0),
        }
    }
}

impl TestVcsLogProvider {
    /// `appendHistory`: the commits go on top of the existing ones.
    pub fn append_history(&self, commits: Vec<TimedVcsCommit>) {
        let mut all = self.commits.lock().unwrap();
        all.splice(0..0, commits);
    }

    /// IdeaGit addition: replaces the whole history (newest first), e.g. after a rewrite.
    pub fn set_history(&self, commits: Vec<TimedVcsCommit>) {
        *self.commits.lock().unwrap() = commits;
    }

    pub fn add_ref(&self, vcs_ref: VcsRef) {
        self.refs.lock().unwrap().insert(vcs_ref);
    }

    pub fn reset_read_first_block_counter(&self) {
        self.read_first_block_counter
            .store(0, AtomicOrdering::SeqCst);
    }

    pub fn get_read_first_block_counter(&self) -> usize {
        self.read_first_block_counter.load(AtomicOrdering::SeqCst)
    }
}

impl VcsLogProvider for TestVcsLogProvider {
    fn read_first_block(
        &self,
        root: &VcsRoot,
        requirements: &Requirements<'_>,
    ) -> Result<DetailedLogData, VcsException> {
        self.read_first_block_counter
            .fetch_add(1, AtomicOrdering::SeqCst);
        let commits = self.commits.lock().unwrap();
        let metadatas = commits[..requirements.commit_count.min(commits.len())]
            .iter()
            .map(|commit| {
                VcsCommitMetadata::new(
                    commit.id().clone(),
                    commit.parents().to_vec(),
                    commit.timestamp(),
                    root.clone(),
                    SAMPLE_SUBJECT.to_string(),
                    default_user(),
                    SAMPLE_SUBJECT.to_string(),
                    default_user(),
                    commit.timestamp(),
                )
            })
            .collect();
        Ok(DetailedLogData {
            commits: metadatas,
            refs: Vec::new(),
        })
    }

    fn read_all_hashes(
        &self,
        _root: &VcsRoot,
        commit_consumer: &mut dyn FnMut(TimedVcsCommit),
    ) -> Result<LogData, VcsException> {
        for commit in self.commits.lock().unwrap().iter() {
            commit_consumer(commit.clone());
        }
        Ok(LogData {
            refs: self.refs.lock().unwrap().clone(),
            users: HashSet::new(),
        })
    }

    fn read_metadata(
        &self,
        _root: &VcsRoot,
        _hashes: &[String],
        _consumer: &mut dyn FnMut(VcsCommitMetadata),
    ) -> Result<(), VcsException> {
        unimplemented!("UnsupportedOperationException")
    }

    fn reference_manager(&self) -> Arc<dyn VcsLogRefManager> {
        self.ref_manager.clone()
    }

    fn get_current_user(&self, _root: &VcsRoot) -> Result<Option<VcsUser>, VcsException> {
        Ok(Some(default_user()))
    }

    fn get_current_branch(&self, _root: &VcsRoot) -> Option<String> {
        None
    }
}
