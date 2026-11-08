//! Commits as read from a VCS.
//!
//! Ported from `platform/vcs-log/api/src/com/intellij/vcs/log/TimedVcsCommit.java`,
//! `VcsShortCommitDetails.java`, `VcsCommitMetadata.java`, `CommitId.java` (same package) and
//! `platform/vcs-log/impl/src/com/intellij/vcs/log/impl/{TimedVcsCommitImpl,
//! VcsShortCommitDetailsImpl, VcsCommitMetadataImpl}.java`.
//!
//! Like `TimedVcsCommitImpl`, commits are equal when their hashes are equal, whatever the other
//! fields. Timestamps are milliseconds since the epoch, as upstream.

use std::fmt;
use std::hash::{Hash as StdHash, Hasher};

use crate::data::hash::Hash;
use crate::data::root::VcsRoot;
use crate::data::vcs_user::VcsUser;
use crate::graph_commit::GraphCommit;

/// A commit with its parents and commit time (`TimedVcsCommitImpl`).
#[derive(Clone)]
pub struct TimedVcsCommit {
    id: Hash,
    parents: Vec<Hash>,
    timestamp: i64,
}

impl TimedVcsCommit {
    pub fn new(id: Hash, parents: Vec<Hash>, timestamp: i64) -> TimedVcsCommit {
        TimedVcsCommit {
            id,
            parents,
            timestamp,
        }
    }
}

impl GraphCommit<Hash> for TimedVcsCommit {
    fn id(&self) -> &Hash {
        &self.id
    }

    fn parents(&self) -> &[Hash] {
        &self.parents
    }

    fn timestamp(&self) -> i64 {
        self.timestamp
    }
}

impl PartialEq for TimedVcsCommit {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for TimedVcsCommit {}

impl StdHash for TimedVcsCommit {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl fmt::Debug for TimedVcsCommit {
    /// `TimedVcsCommitImpl.toString`: `short-hash|-short-parents:time`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let parents: Vec<String> = self.parents.iter().map(Hash::to_short_string).collect();
        write!(
            f,
            "{}|-{}:{}",
            self.id.to_short_string(),
            parents.join(","),
            self.timestamp
        )
    }
}

/// What the log table and the commit details show for a commit (`VcsCommitMetadataImpl`, which
/// includes `VcsShortCommitDetailsImpl`).
#[derive(Clone)]
pub struct VcsCommitMetadata {
    commit: TimedVcsCommit,
    root: VcsRoot,
    subject: String,
    author: VcsUser,
    committer: VcsUser,
    author_time: i64,
    full_message: String,
}

impl VcsCommitMetadata {
    #[allow(clippy::too_many_arguments)] // mirrors the upstream constructor
    pub fn new(
        hash: Hash,
        parents: Vec<Hash>,
        commit_time: i64,
        root: VcsRoot,
        subject: String,
        author: VcsUser,
        message: String,
        committer: VcsUser,
        author_time: i64,
    ) -> VcsCommitMetadata {
        VcsCommitMetadata {
            commit: TimedVcsCommit::new(hash, parents, commit_time),
            root,
            subject,
            author,
            committer,
            author_time,
            full_message: message,
        }
    }

    pub fn root(&self) -> &VcsRoot {
        &self.root
    }

    pub fn subject(&self) -> &str {
        &self.subject
    }

    pub fn author(&self) -> &VcsUser {
        &self.author
    }

    pub fn committer(&self) -> &VcsUser {
        &self.committer
    }

    pub fn author_time(&self) -> i64 {
        self.author_time
    }

    pub fn commit_time(&self) -> i64 {
        self.commit.timestamp
    }

    pub fn full_message(&self) -> &str {
        &self.full_message
    }

    /// The commit without its details.
    pub fn as_timed_commit(&self) -> &TimedVcsCommit {
        &self.commit
    }
}

impl GraphCommit<Hash> for VcsCommitMetadata {
    fn id(&self) -> &Hash {
        &self.commit.id
    }

    fn parents(&self) -> &[Hash] {
        &self.commit.parents
    }

    fn timestamp(&self) -> i64 {
        self.commit.timestamp
    }
}

impl PartialEq for VcsCommitMetadata {
    fn eq(&self, other: &Self) -> bool {
        self.commit == other.commit
    }
}

impl Eq for VcsCommitMetadata {}

impl StdHash for VcsCommitMetadata {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.commit.hash(state);
    }
}

impl fmt::Debug for VcsCommitMetadata {
    /// `VcsShortCommitDetailsImpl.toString`: `short-hash(subject)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.commit.id.to_short_string(), self.subject)
    }
}

/// A commit identified across repositories: its hash and root (`CommitId`).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct CommitId {
    pub hash: Hash,
    pub root: VcsRoot,
}
