//! The permanent graph: all commits of the log, created once per refresh.
//!
//! Ported from `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/PermanentGraph.kt`.
//!
//! Deviations: `create_visible_graph` returns `Result`, failing with [`UnsupportedGraphOptions`]
//! for options that are not ported yet (Plan 2b); `all_commits` returns a lazy [`AllCommits`]
//! view; `get_contained_in_branch_condition` returns a [`ContainedInBranchCondition`].

use std::collections::HashSet;
use std::fmt;
use std::hash::Hash;

use crate::graph_commit::GraphCommitImpl;
use crate::visible_graph::VisibleGraph;

/// Sorting mechanism for the commits in the visible graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SortType {
    /// "Off": sort commits topologically and by date.
    Normal,
    /// "Standard" (IntelliSort): in case of a merge, show incoming commits first (directly below
    /// the merge commit).
    Bek,
}

impl SortType {
    pub fn presentation(self) -> &'static str {
        match self {
            SortType::Normal => "Off",
            SortType::Bek => "Standard",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            SortType::Normal => "Sort commits topologically and by date",
            SortType::Bek => {
                "In case of merge show incoming commits first (directly below merge commit)"
            }
        }
    }
}

/// Options controlling how the visible graph is created.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Options {
    /// Show the graph as is, sorted as specified by the [`SortType`].
    Base(SortType),
    /// Show incoming commits on top of main branch commits as if they were rebased.
    LinearBek,
    /// Follow only the first parent commit upon seeing a merge commit.
    FirstParent,
}

impl Options {
    /// `Options.Default`.
    pub const DEFAULT: Options = Options::Base(SortType::Normal);
}

/// `create_visible_graph` was asked for something this port does not support yet.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0} is not yet supported by ig-log (Plan 2b)")]
pub struct UnsupportedGraphOptions(pub String);

/// Lazy list of all commits of a permanent graph, with parents read back from the graph.
pub struct AllCommits<'a, Id> {
    len: usize,
    get: Box<dyn Fn(usize) -> GraphCommitImpl<Id> + 'a>,
}

impl<'a, Id> AllCommits<'a, Id> {
    pub fn new(len: usize, get: impl Fn(usize) -> GraphCommitImpl<Id> + 'a) -> AllCommits<'a, Id> {
        AllCommits {
            len,
            get: Box::new(get),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// # Panics
    /// If `index >= len()` (out of range).
    pub fn get(&self, index: usize) -> GraphCommitImpl<Id> {
        assert!(index < self.len, "index {index} out of bounds {}", self.len);
        (self.get)(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = GraphCommitImpl<Id>> + '_ {
        (0..self.len).map(|i| (self.get)(i))
    }
}

impl<Id> fmt::Debug for AllCommits<'_, Id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AllCommits(len = {})", self.len)
    }
}

/// Tells whether a commit is reachable from some heads (upstream `Predicate<CommitId>`).
#[derive(Clone, Debug)]
pub struct ContainedInBranchCondition<Id> {
    branch_nodes: HashSet<Id>,
}

impl<Id: Eq + Hash> ContainedInBranchCondition<Id> {
    pub fn new(branch_nodes: HashSet<Id>) -> ContainedInBranchCondition<Id> {
        ContainedInBranchCondition { branch_nodes }
    }

    pub fn test(&self, commit_id: &Id) -> bool {
        self.branch_nodes.contains(commit_id)
    }
}

/// Created once per repository, kept until the log is refreshed. Contains all commits of the log
/// and may occupy a lot of memory.
pub trait PermanentGraph<Id> {
    type Visible: VisibleGraph<Id> + Send;

    /// Creates a visible graph. `heads_of_visible_branches == None` means all branches are visible;
    /// `matched_commits == None` means all commits are visible.
    fn create_visible_graph(
        &self,
        options: Options,
        heads_of_visible_branches: Option<&HashSet<Id>>,
        matched_commits: Option<&HashSet<Id>>,
    ) -> Result<Self::Visible, UnsupportedGraphOptions>;

    /// `createVisibleGraph(sortType, ...)`: same as `Options::Base(sort_type)`.
    fn create_visible_graph_sorted(
        &self,
        sort_type: SortType,
        heads_of_visible_branches: Option<&HashSet<Id>>,
        matched_commits: Option<&HashSet<Id>>,
    ) -> Result<Self::Visible, UnsupportedGraphOptions> {
        self.create_visible_graph(
            Options::Base(sort_type),
            heads_of_visible_branches,
            matched_commits,
        )
    }

    fn all_commits(&self) -> AllCommits<'_, Id>;

    fn get_children(&self, commit: &Id) -> Vec<Id>;

    fn get_containing_branches(&self, commit: &Id) -> HashSet<Id>;

    fn get_contained_in_branch_condition(
        &self,
        current_branch_head: &[Id],
    ) -> ContainedInBranchCondition<Id>;
}
