//! A commit as seen by the graph: an id, its parents and a timestamp.
//!
//! Ported from `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/GraphCommit.java`,
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/AbstractGraphCommit.java`,
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/GraphCommitImpl.java` and
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/IntGraphCommit.java`.
//!
//! Deviation: the integer-specialised commit classes (`IntGraphCommit`) are not needed in Rust;
//! [`GraphCommitImpl`] is generic and stores parents in a `Vec`.

use std::fmt;
use std::hash::{Hash, Hasher};

/// A commit of the graph.
pub trait GraphCommit<Id> {
    fn id(&self) -> &Id;

    fn parents(&self) -> &[Id];

    /// When the commit was made. Displayed in the table by default; used for joining commits from
    /// different repositories and for ordering commits in a single repository (keeping the
    /// preference of the topological ordering).
    fn timestamp(&self) -> i64;
}

/// The default [`GraphCommit`]. Like `AbstractGraphCommit`, equality and hashing use the id only.
#[derive(Clone)]
pub struct GraphCommitImpl<Id> {
    id: Id,
    parents: Vec<Id>,
    timestamp: i64,
}

impl<Id> GraphCommitImpl<Id> {
    /// `GraphCommitImpl.createCommit`.
    pub fn create_commit(id: Id, parents: Vec<Id>, timestamp: i64) -> GraphCommitImpl<Id> {
        GraphCommitImpl {
            id,
            parents,
            timestamp,
        }
    }
}

impl<Id> GraphCommit<Id> for GraphCommitImpl<Id> {
    fn id(&self) -> &Id {
        &self.id
    }

    fn parents(&self) -> &[Id] {
        &self.parents
    }

    fn timestamp(&self) -> i64 {
        self.timestamp
    }
}

impl<Id: PartialEq> PartialEq for GraphCommitImpl<Id> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<Id: Eq> Eq for GraphCommitImpl<Id> {}

impl<Id: Hash> Hash for GraphCommitImpl<Id> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// Like upstream `toString`: just the id.
impl<Id: fmt::Debug> fmt::Debug for GraphCommitImpl<Id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.id.fmt(f)
    }
}
