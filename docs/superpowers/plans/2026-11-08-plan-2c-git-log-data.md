# IdeaGit Plan 2c — Git Log Data Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Load real Git history into the commit graph: hashes, parents, timestamps, metadata and refs, for one or several repositories, with an initial load (first block, then full log) and incremental refresh. It is ported from IntelliJ's `GitLogProvider` / `GitLogUtil` / `GitLogParser` / `GitRefManager` and the VCS Log data layer (`VcsLogRefresherImpl`, `VcsLogJoiner`, `VcsLogMultiRepoJoiner`, `VcsLogSorter`, `RefsModel`, `GraphColorManagerImpl`, `HeadCommitsComparator`).

**Architecture:**
- The VCS-agnostic data layer lives in `ig-log::data`: commit and ref models, in-memory storage interning (root, hash) to dense integer ids, sorter, joiners, the graph-data builder, and a synchronous refresher generic over `VcsLogProvider`.
- The Git-specific side lives in `ig-git::log`: the parser and records, the output splitter and collector, `GitLogUtil`, `GitRefManager`, a minimal repository manager, and `GitLogProvider`, which implements `VcsLogProvider`.
- `ig-git` now depends on `ig-log`, mirroring upstream, where `git4idea` depends on the vcs-log API. Only `ig_git::log` imports `ig-log`, so it could move to its own crate mechanically.
- Graph commit ids are storage indexes (`u32`), as in upstream production.

**Tech Stack:** Rust 2024, existing workspace dependencies only. Tests run against the real `git` (≥ 2.17), isolated by `Git::with_base_environment`.

**Spec:** `docs/superpowers/specs/2026-11-08-ideagit-rust-design.md` (Git Log section). **Previous plans:** Plan 1 (`ig-git` core) and Plan 2 (`ig-log` graph core).

> **Note:** The code blocks below are the verified prototype. Review fixes applied during execution live in the crates and in `reference/upstream-map.toml`, and those are the source of truth.

---

## Scope

- **In scope:**
  - Timed commits (hash, parents, timestamp) and metadata (author, committer, times, subject, full message).
  - Refs: local and remote branches, tags, HEAD.
  - Current user, multi-repo, first block, full log, and refresh through the joiner.
  - The Plan 2 performance follow-up: O(1) node-id lookups in the permanent graph.
- **Out of scope (later plans):**
  - Per-commit changes and full details (`GitFullDetailsCollector`, `GitChangesParser`). These come with the Log UI and diff.
  - Filters (`getCommitsMatchingFilter`, Plan 2b `FilteredController`), ref groups and favorites.
  - The experimental refs provider, file-system watching (callers trigger `refresh`), and cancellation and progress.

## Ported upstream tests

| Upstream test class | Ported |
|---|---|
| `VcsLogSorterTest` | 5/5 |
| `VcsLogMultiRepoJoinerTest` | 1/1 |
| `VcsLogJoinerTest` | 29/29 |
| `GitLogParserTest` | 15/15 |
| `GitBranchComparatorTest`, `GitLabelComparatorTest` | 11/11, 12/12 |
| `VcsLogRefresherTest` | 4/7. The three skipped cases test blocking, cancellation and request batching, which don't exist in the synchronous refresher. |
| `GitLogProviderTest` | 5. Eight filter tests and the full-details test are out of scope. |
| `GitReadRecentCommitsTest` | 6/6, with experimental refs collection off. |
| `GitLogUtilTest` | 2, adapted to read through metadata. The renames and merge-diff cases need full details. |

IdeaGit's own tests cover:
- multi-repo loading
- refreshes, each checked to go through the joiner and to equal a fresh full load: a new commit, a branch plus a merge, a tag on an old commit, moving a branch pointer, a tagged-only branch, a reset, and one root of a multi-repo log
- the full reload past the retry limit
- the collector, splitter, order check and node index

There is also an `#[ignore]`d performance smoke test on a synthetic repository of about 50k commits. In release mode, a full initial load took about 240 ms (mostly `git log` itself), and a refresh after one commit took about 40 ms.

## Deviations (also recorded in `reference/upstream-map.toml`)

- **Graph performance (upstream TODO):**
  - `get_node_id`, `convert_to_node_ids` and `contains_all` use hash indexes, O(1) per id. Results are unchanged.
- **Refresher:**
  - It is synchronous. There is no request queue or batching, progress, cancellation, overlay data or tracing. Callers run it on one background worker and coalesce repository events.
  - It validates unique ids and topological order before building a graph. A joined log that fails validation forces a full reload. Upstream builds silently.
  - Roots are processed in path order.
- **Determinism:** upstream relies on `HashMap`/`HashSet` iteration order in several places. Here:
  - the integrator picks the next commit in input order
  - the multi-repo joiner breaks timestamp ties by root order
  - branch refs are created in name order
  - new tags are loaded in name order
- **Parser:**
  - It works on bytes, which is safe because all markers are ASCII.
  - Format errors that upstream only logs are dropped.
  - `--name-status` entries are kept, but `Change` objects are not ported.
- **Git provider and refs:**
  - Repository state comes from a small `GitRepositoryManager`, updated explicitly rather than by VFS events.
  - Ref types carry no colors.
  - Not ported: ref groups, favorites, serialization, filters, full details, and the experimental provider.
- **Small differences:**
  - `VcsLogRefManager::name()` replaces `toString`.
  - Ref managers are compared by `Arc` identity.
  - User and ref names are not interned.
  - Octal escapes in paths decode as UTF-8.
  - The collector keeps errors that upstream only logs.
- **Known wart:** `ig_git::hash::Hash` and `ig_log::data::Hash` both exist and are converted in the provider. Unify them in a follow-up.

## Notes for the Log UI plan

- **Refresher API:**
  - Create it with `VcsLogRefresher::new(Arc<InMemoryStorage>, LogProviders, 1000)`, using one `Arc<GitLogProvider>` for all git roots.
  - Register `set_data_pack_handler`.
  - `initialize()` publishes the first block, then the full log.
  - `refresh(&roots)` returns an `Arc<VcsLogGraphData>`.
- **Commits and refs:**
  - Graph commit ids are storage indexes, and `storage.get_commit_id(i)` returns (hash, root).
  - Labels come from `refs_model.refs_to_commit(i)`, sorted with `labels_order_compare`.
  - Branch colors are Java string hash codes, which the UI maps onto a palette.
- **Threading:**
  - Run all refresher calls on one background worker. The provider, storage and refs model are `Send + Sync`.
  - Build the visible graph per data pack, then move it to the UI thread. It is `Send` but not `Sync`.
- **Still to build:**
  - Row metadata, via a mini-details/top-commits cache fed by `set_recent_details_consumer` plus batched `read_metadata`.
  - Commit details and changes.
  - Filters.
  - Ref groups.
  - Repository watching that triggers refreshes.
  - Cancellation in the runner.
  - Unifying the two `Hash` types.
- **Refs in the first block:** the initial first block only carries refs on its own commits, as upstream. The full log brings all refs.

## Why some tests follow their code

Within each crate, the modules of a task compile together: `ig-log::data/mod.rs` declares all submodules, and the git parser and record types import each other. Each source task lands with its unit tests, and the following tasks add the ported upstream test classes and run them. A failing upstream case means the port is wrong. Never weaken the expected values.

---
### Task 1: ig-log: O(1) node-id lookups

**Files:**
- Modify: `crates/ig-log/src/permanent/permanent_commits_info_impl.rs`

**Review focus:** `permanent_commits_info_impl.rs`: index maps vs upstream semantics (duplicate ids, not-loaded ids).

- [ ] **Step 1: Replace the file** with this content. It adds id → node-index maps and makes `get_node_id`, `convert_to_node_ids` and `contains_all` O(1) per id, with results unchanged:

`crates/ig-log/src/permanent/permanent_commits_info_impl.rs`:

```rust
//! Node id to commit id (and timestamp) mapping of the permanent graph.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentCommitsInfoImpl.kt`
//! and `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/RowsMapping.kt`.
//!
//! Deviations:
//! - No `Int`-specialised storage (`isIntegerCase`); `Vec<CommitId>` is already compact.
//! - `get_node_id`, `convert_to_node_ids` and `contains_all` use hash indexes (commit id -> row and
//!   commit id -> not-loaded id) instead of linear scans (upstream: "todo optimize with special
//!   map"), so they are O(1) per commit instead of O(rows). Results are unchanged, including for
//!   duplicate commit ids: `get_node_id` returns the first row, `convert_to_node_ids` every row.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::api::permanent::{PermanentCommitsInfo, VcsLogGraphNodeId};
use crate::graph_commit::GraphCommit;
use crate::utils::TimestampGetter;

/// Commit ids and timestamps by row (`RowsMapping`).
#[derive(Clone, Debug)]
pub struct RowsMapping<CommitId> {
    commit_id_mapping: Vec<CommitId>,
    timestamps_mapping: Vec<i64>,
}

impl<CommitId: Clone> RowsMapping<CommitId> {
    pub fn new(size: usize) -> RowsMapping<CommitId> {
        RowsMapping {
            commit_id_mapping: Vec::with_capacity(size),
            timestamps_mapping: Vec::with_capacity(size),
        }
    }

    pub fn add(&mut self, commit_id: CommitId, timestamp: i64) {
        self.commit_id_mapping.push(commit_id);
        self.timestamps_mapping.push(timestamp);
    }

    pub fn commit_id_mapping(&self) -> &[CommitId] {
        &self.commit_id_mapping
    }

    pub fn get_commit_id(&self, row: i32) -> CommitId {
        self.commit_id_mapping[row as usize].clone()
    }
}

impl<CommitId> TimestampGetter for RowsMapping<CommitId> {
    fn get_timestamp(&self, row: i32) -> i64 {
        self.timestamps_mapping[row as usize]
    }
}

#[derive(Clone, Debug)]
pub struct PermanentCommitsInfoImpl<CommitId> {
    rows_mapping: RowsMapping<CommitId>,
    not_loaded_commits: HashMap<i32, CommitId>,
    /// Commit id -> its first row.
    node_ids: HashMap<CommitId, i32>,
    /// Commit id -> the rows after the first, for commit ids listed more than once (normally empty).
    duplicate_node_ids: HashMap<CommitId, Vec<i32>>,
    /// Commit id -> its not-loaded id (each not-loaded commit gets exactly one id).
    not_loaded_node_ids: HashMap<CommitId, i32>,
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> PermanentCommitsInfoImpl<CommitId> {
    /// `newInstance`: rows in the order of `graph_commits`; `not_loaded_commits` maps synthetic
    /// negative ids to the commits they stand for.
    pub fn new_instance<G: GraphCommit<CommitId>>(
        graph_commits: &[G],
        not_loaded_commits: HashMap<i32, CommitId>,
    ) -> PermanentCommitsInfoImpl<CommitId> {
        let mut rows_mapping = RowsMapping::new(graph_commits.len());
        let mut node_ids = HashMap::with_capacity(graph_commits.len());
        let mut duplicate_node_ids: HashMap<CommitId, Vec<i32>> = HashMap::new();
        for (row, commit) in graph_commits.iter().enumerate() {
            rows_mapping.add(commit.id().clone(), commit.timestamp());
            if node_ids.contains_key(commit.id()) {
                duplicate_node_ids
                    .entry(commit.id().clone())
                    .or_default()
                    .push(row as i32);
            } else {
                node_ids.insert(commit.id().clone(), row as i32);
            }
        }
        let not_loaded_node_ids = not_loaded_commits
            .iter()
            .map(|(&node_id, commit_id)| (commit_id.clone(), node_id))
            .collect();
        PermanentCommitsInfoImpl {
            rows_mapping,
            not_loaded_commits,
            node_ids,
            duplicate_node_ids,
            not_loaded_node_ids,
        }
    }

    pub fn timestamp_getter(&self) -> &dyn TimestampGetter {
        &self.rows_mapping
    }

    fn get_not_load_node_id(&self, commit_id: &CommitId) -> i32 {
        self.not_loaded_node_ids
            .get(commit_id)
            .copied()
            .unwrap_or(-1)
    }

    pub fn convert_to_commit_id_list(
        &self,
        commit_indexes: impl IntoIterator<Item = VcsLogGraphNodeId>,
    ) -> Vec<CommitId> {
        commit_indexes
            .into_iter()
            .map(|node_id| self.get_commit_id(node_id))
            .collect()
    }

    pub fn convert_to_commit_id_set(
        &self,
        commit_indexes: impl IntoIterator<Item = VcsLogGraphNodeId>,
    ) -> HashSet<CommitId> {
        commit_indexes
            .into_iter()
            .map(|node_id| self.get_commit_id(node_id))
            .collect()
    }

    /// `convertToNodeIds(commitIds, skipNotLoadedCommits)`.
    pub fn convert_to_node_ids_skipping(
        &self,
        commit_ids: &HashSet<CommitId>,
        skip_not_loaded_commits: bool,
    ) -> HashSet<VcsLogGraphNodeId> {
        let mut result: HashSet<i32> = HashSet::with_capacity(commit_ids.len());
        for commit_id in commit_ids {
            if let Some(&row) = self.node_ids.get(commit_id) {
                result.insert(row);
                if let Some(rows) = self.duplicate_node_ids.get(commit_id) {
                    result.extend(rows.iter().copied());
                }
            }
            if !skip_not_loaded_commits {
                if let Some(&node_id) = self.not_loaded_node_ids.get(commit_id) {
                    result.insert(node_id);
                }
            }
        }
        result
    }

    pub fn contains_all<'a>(&self, commit_ids: impl IntoIterator<Item = &'a CommitId>) -> bool
    where
        CommitId: 'a,
    {
        commit_ids
            .into_iter()
            .all(|id| self.node_ids.contains_key(id))
    }
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> PermanentCommitsInfo<CommitId>
    for PermanentCommitsInfoImpl<CommitId>
{
    /// # Panics
    /// If `node_id` is neither a row nor a known not-loaded id.
    fn get_commit_id(&self, node_id: VcsLogGraphNodeId) -> CommitId {
        if node_id < 0 {
            return self
                .not_loaded_commits
                .get(&node_id)
                .cloned()
                .unwrap_or_else(|| panic!("Unknown not loaded node id: {node_id}"));
        }
        self.rows_mapping.get_commit_id(node_id)
    }

    fn try_get_commit_id(&self, node_id: VcsLogGraphNodeId) -> Option<CommitId> {
        if node_id < 0 {
            return self.not_loaded_commits.get(&node_id).cloned();
        }
        self.rows_mapping
            .commit_id_mapping()
            .get(node_id as usize)
            .cloned()
    }

    fn get_timestamp(&self, node_id: VcsLogGraphNodeId) -> i64 {
        if node_id < 0 {
            return 0;
        }
        self.rows_mapping.get_timestamp(node_id)
    }

    /// The node id assigned to `commit_id`:
    /// - `0..nodes_count`: the row of a loaded commit;
    /// - `<= -2`: the synthetic id of a commit referenced as a parent but not loaded;
    /// - `-1`: unknown to this permanent graph.
    fn get_node_id(&self, commit_id: &CommitId) -> VcsLogGraphNodeId {
        if let Some(&row) = self.node_ids.get(commit_id) {
            return row;
        }
        self.get_not_load_node_id(commit_id)
    }

    fn convert_to_node_ids(&self, commit_ids: &HashSet<CommitId>) -> HashSet<VcsLogGraphNodeId> {
        self.convert_to_node_ids_skipping(commit_ids, false)
    }
}
```

- [ ] **Step 2: Run the existing graph suite**

Run: `cargo test -p ig-log`
Expected: all existing targets pass, unchanged (145 tests).

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "perf(ig-log): O(1) node-id lookups in permanent commits info"
```

---

### Task 2: ig-log: the `data` module

**Files:**
- Create: `crates/ig-log/src/data/*.rs`, `crates/ig-log/tests/common/{timed_commit_parser,test_vcs_log_provider}.rs`
- Modify: `crates/ig-log/src/lib.rs`, `crates/ig-log/tests/common/mod.rs`

**Review focus:** crate-level design of `data`: types, storage interning, refresher API and threading.

- [ ] **Step 1: Create the data module files**

`crates/ig-log/src/data/mod.rs`:

```rust
//! VCS-agnostic log data: commits, references, commit-id interning, sorting and joining, the data
//! pack (permanent graph plus references) and the refresher that loads it.
//!
//! Ported from `platform/vcs-log/api` (`Hash`, `TimedVcsCommit`, `VcsCommitMetadata`, `VcsRef`,
//! `VcsRefType`, `VcsUser`, `VcsLogProvider`, `VcsLogRefManager`, ...) and
//! `platform/vcs-log/impl/src/com/intellij/vcs/log/{data,graph,impl}`. Each module names its
//! upstream files; see also `reference/upstream-map.toml`.
//!
//! A VCS plugs in by implementing [`VcsLogProvider`]; [`VcsLogRefresher`] turns what providers read
//! into a [`VcsLogGraphData`] whose permanent graph uses [`VcsLogStorage`] indexes as commit ids.

pub mod commit;
pub mod graph_color_manager_impl;
pub mod graph_data;
pub mod hash;
pub mod joiner;
pub mod multi_repo_joiner;
pub mod provider;
pub mod refresher;
pub mod refs_model;
pub mod root;
pub mod sorter;
pub mod storage;
pub mod vcs_ref;
pub mod vcs_user;

pub use commit::{CommitId, TimedVcsCommit, VcsCommitMetadata};
pub use graph_color_manager_impl::{GraphColorManagerImpl, HeadCommitsComparator};
pub use graph_data::{LogGraphCommit, LogProviders, VcsLogGraphData};
pub use hash::Hash;
pub use joiner::JoinError;
pub use provider::{
    DetailedLogData, LogData, Requirements, RequirementsEx, VcsException, VcsLogProvider,
    VcsLogRefManager,
};
pub use refresher::VcsLogRefresher;
pub use refs_model::{RefsModel, RootRefsModel};
pub use root::{VcsRoot, compare_roots, vcs_root};
pub use sorter::sort_by_date_topo_order;
pub use storage::{InMemoryStorage, VcsLogCommitStorageIndex, VcsLogStorage};
pub use vcs_ref::{VcsRef, VcsRefType, VcsRefsContainer, VcsRefsList};
pub use vcs_user::VcsUser;
```

`crates/ig-log/src/data/commit.rs`:

```rust
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
```

`crates/ig-log/src/data/graph_color_manager_impl.rs`:

```rust
//! Branch colors and head ordering for the log's permanent graph.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/graph/GraphColorManagerImpl.kt` and
//! `platform/vcs-log/impl/src/com/intellij/vcs/log/graph/HeadCommitsComparator.java`.
//!
//! Deviations: "no refs at head" is not logged; reference managers are told apart by `Arc`
//! identity (upstream `equals`) and ordered by [`VcsLogRefManager::name`] (upstream `toString`).

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::data::provider::VcsLogRefManager;
use crate::data::refs_model::RefsModel;
use crate::data::root::VcsRoot;
use crate::data::storage::VcsLogCommitStorageIndex;
use crate::graph_color_manager::GraphColorManager;

/// `GraphColorManagerImpl.DEFAULT_COLOR`.
pub const DEFAULT_COLOR: i32 = 0;

/// `String.hashCode()` of Java: the color id of a branch is the hash code of its name.
pub fn java_string_hash_code(s: &str) -> i32 {
    s.encode_utf16()
        .fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(c as i32))
}

/// Colors a branch by the name of its most important reference (`GraphColorManagerImpl`).
pub struct GraphColorManagerImpl {
    refs_model: Arc<RefsModel>,
}

impl GraphColorManagerImpl {
    pub fn new(refs_model: Arc<RefsModel>) -> GraphColorManagerImpl {
        GraphColorManagerImpl { refs_model }
    }
}

impl GraphColorManager<VcsLogCommitStorageIndex> for GraphColorManagerImpl {
    fn get_color(
        &self,
        head_commit: &VcsLogCommitStorageIndex,
        head_fragment_index: i32,
        fragment_index: i32,
    ) -> i32 {
        if head_fragment_index == fragment_index {
            return match self.refs_model.get_ref_for_head_commit(*head_commit) {
                Some(first_ref) => java_string_hash_code(first_ref.name()),
                None => DEFAULT_COLOR,
            };
        }
        fragment_index
    }
}

/// Orders branch heads by their most important references (`HeadCommitsComparator`): heads
/// without references go last.
pub struct HeadCommitsComparator {
    refs_model: Arc<RefsModel>,
    ref_managers: BTreeMap<VcsRoot, Arc<dyn VcsLogRefManager>>,
}

impl HeadCommitsComparator {
    pub fn new(
        refs_model: Arc<RefsModel>,
        ref_managers: BTreeMap<VcsRoot, Arc<dyn VcsLogRefManager>>,
    ) -> HeadCommitsComparator {
        HeadCommitsComparator {
            refs_model,
            ref_managers,
        }
    }

    pub fn compare(
        &self,
        head1: &VcsLogCommitStorageIndex,
        head2: &VcsLogCommitStorageIndex,
    ) -> Ordering {
        if head1 == head2 {
            return Ordering::Equal;
        }
        let ref1 = self.refs_model.get_ref_for_head_commit(*head1);
        let ref2 = self.refs_model.get_ref_for_head_commit(*head2);
        let (ref1, ref2) = match (ref1, ref2) {
            (None, None) => return head1.cmp(head2),
            (None, Some(_)) => return Ordering::Greater,
            (Some(_), None) => return Ordering::Less,
            (Some(r1), Some(r2)) => (r1, r2),
        };
        let manager1 = &self.ref_managers[ref1.root()];
        let manager2 = &self.ref_managers[ref2.root()];
        if !Arc::ptr_eq(manager1, manager2) {
            return manager1.name().cmp(&manager2.name());
        }
        manager1.branch_layout_compare(&ref1, &ref2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_hash_code_matches_known_values() {
        assert_eq!(java_string_hash_code(""), 0);
        assert_eq!(java_string_hash_code("master"), -1081267614);
        assert_eq!(java_string_hash_code("main"), 3343801);
        assert_eq!(java_string_hash_code("origin/main"), 1685401346);
    }
}
```

`crates/ig-log/src/data/graph_data.rs`:

```rust
//! A loaded log: references plus the permanent graph of commit indexes ("data pack").
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogGraphData.kt`,
//! `VcsLogGraphDataFactory.kt` and `DataPack.kt` (same package).
//!
//! Deviations: no overlay data (`OverlayData`, disabled in tests upstream); an empty or failed pack
//! has no permanent graph (upstream: `EmptyPermanentGraph`).

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::sync::Arc;

use crate::data::graph_color_manager_impl::{GraphColorManagerImpl, HeadCommitsComparator};
use crate::data::provider::{VcsException, VcsLogProvider, VcsLogRefManager};
use crate::data::refs_model::{RefsModel, RootRefsModel};
use crate::data::root::VcsRoot;
use crate::data::storage::{VcsLogCommitStorageIndex, VcsLogStorage};
use crate::data::vcs_ref::VcsRefsContainer;
use crate::facade::permanent_graph_impl::PermanentGraphImpl;
use crate::graph_commit::GraphCommitImpl;
use crate::print::graph_color_getter::GraphColorGetterByHeadFactory;

/// The providers of the log, by root.
pub type LogProviders = BTreeMap<VcsRoot, Arc<dyn VcsLogProvider>>;

/// A commit of the log graph: storage indexes for the commit and its parents.
pub type LogGraphCommit = GraphCommitImpl<VcsLogCommitStorageIndex>;

/// The graph data of the log regardless of filters (`VcsLogGraphData`, formerly `DataPack`).
pub struct VcsLogGraphData {
    refs_model: Arc<RefsModel>,
    permanent_graph: Option<Arc<PermanentGraphImpl<VcsLogCommitStorageIndex>>>,
    log_providers: LogProviders,
    is_full: bool,
    error: Option<VcsException>,
}

impl VcsLogGraphData {
    /// `VcsLogGraphData.Empty`.
    pub fn empty(storage: Arc<dyn VcsLogStorage>) -> VcsLogGraphData {
        VcsLogGraphData {
            refs_model: Arc::new(RefsModel::create_empty_instance(storage)),
            permanent_graph: None,
            log_providers: LogProviders::new(),
            is_full: false,
            error: None,
        }
    }

    /// `VcsLogGraphData.Error`.
    pub fn error(storage: Arc<dyn VcsLogStorage>, error: VcsException) -> VcsLogGraphData {
        VcsLogGraphData {
            error: Some(error),
            ..VcsLogGraphData::empty(storage)
        }
    }

    pub fn refs_model(&self) -> &Arc<RefsModel> {
        &self.refs_model
    }

    /// The permanent graph; `None` for an empty log.
    pub fn permanent_graph(&self) -> Option<&Arc<PermanentGraphImpl<VcsLogCommitStorageIndex>>> {
        self.permanent_graph.as_ref()
    }

    pub fn log_providers(&self) -> &LogProviders {
        &self.log_providers
    }

    /// Whether the whole history is loaded (rather than the first block).
    pub fn is_full(&self) -> bool {
        self.is_full
    }

    /// The error of a failed load (`VcsLogGraphData.Error`).
    pub fn load_error(&self) -> Option<&VcsException> {
        self.error.as_ref()
    }

    /// The number of loaded commits.
    pub fn commits_count(&self) -> usize {
        use crate::permanent_graph::PermanentGraph;
        self.permanent_graph
            .as_ref()
            .map_or(0, |graph| graph.all_commits().len())
    }
}

impl fmt::Debug for VcsLogGraphData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(error) = &self.error {
            return write!(f, "{{Error. {error}}}");
        }
        write!(
            f,
            "{{DataPack. {} commits in {} roots}}",
            self.commits_count(),
            self.log_providers.len()
        )
    }
}

/// `VcsLogGraphDataFactory.buildData`.
pub fn build_data(
    commits: &[LogGraphCommit],
    refs: BTreeMap<VcsRoot, Arc<RootRefsModel>>,
    providers: &LogProviders,
    storage: &Arc<dyn VcsLogStorage>,
    full: bool,
) -> VcsLogGraphData {
    let ref_managers: BTreeMap<VcsRoot, Arc<dyn VcsLogRefManager>> = providers
        .iter()
        .map(|(root, provider)| (root.clone(), provider.reference_manager()))
        .collect();
    let refs_model = Arc::new(RefsModel::create(
        refs,
        storage.clone(),
        ref_managers.clone(),
    ));
    let permanent_graph =
        build_permanent_graph(commits, &refs_model, ref_managers, storage.as_ref()).map(Arc::new);
    VcsLogGraphData {
        refs_model,
        permanent_graph,
        log_providers: providers.clone(),
        is_full: full,
        error: None,
    }
}

fn build_permanent_graph(
    commits: &[LogGraphCommit],
    refs_model: &Arc<RefsModel>,
    ref_managers: BTreeMap<VcsRoot, Arc<dyn VcsLogRefManager>>,
    storage: &dyn VcsLogStorage,
) -> Option<PermanentGraphImpl<VcsLogCommitStorageIndex>> {
    if commits.is_empty() {
        return None;
    }
    let head_commits_comparator = HeadCommitsComparator::new(refs_model.clone(), ref_managers);
    let branches: HashSet<VcsLogCommitStorageIndex> = refs_model
        .branches()
        .map(|r| storage.get_commit_index(r.commit_hash(), r.root()))
        .collect();
    let factory = GraphColorGetterByHeadFactory::new(Arc::new(GraphColorManagerImpl::new(
        refs_model.clone(),
    )));
    Some(PermanentGraphImpl::new_instance(
        commits,
        &factory,
        |head1, head2| head_commits_comparator.compare(head1, head2),
        &branches,
    ))
}
```

`crates/ig-log/src/data/hash.rs`:

```rust
//! Commit hashes.
//!
//! Ported from `platform/vcs-log/api/src/com/intellij/vcs/log/Hash.java` and
//! `platform/vcs-impl/shared/src/com/intellij/vcs/log/impl/HashImpl.java`.
//!
//! Like `HashImpl`, a hash is stored as bytes: a parity byte (1 for an odd number of hex digits),
//! then two hex digits per byte, so any non-empty hex string round-trips (`a`, `a0`, full SHA-1 or
//! SHA-256). `as_string` is lowercase.

use std::fmt;

/// `VcsUtil.SHORT_HASH_LENGTH`.
pub const SHORT_HASH_LENGTH: usize = 8;

#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Hash {
    data: Box<[u8]>,
}

/// The input of [`Hash::build`] was empty or contained a non-hex character.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("bad hash string: {0}")]
pub struct BadHashError(pub String);

impl Hash {
    /// `HashImpl.build`.
    ///
    /// # Panics
    /// If `input` is empty or not hexadecimal (upstream: `assert` / `IllegalArgumentException`).
    pub fn build(input: &str) -> Hash {
        Hash::try_build(input).unwrap_or_else(|e| panic!("{e}"))
    }

    /// `HashImpl.build` returning an error instead of panicking.
    pub fn try_build(input: &str) -> Result<Hash, BadHashError> {
        let bytes = input.as_bytes();
        if bytes.is_empty() {
            return Err(BadHashError(input.to_string()));
        }
        let digit = |c: u8| {
            (c as char)
                .to_digit(16)
                .map(|d| d as u8)
                .ok_or_else(|| BadHashError(input.to_string()))
        };
        let odd = bytes.len() % 2;
        let mut data = Vec::with_capacity(bytes.len() / 2 + 1 + odd);
        data.push(odd as u8);
        for pair in bytes.chunks(2) {
            if pair.len() == 2 {
                data.push(digit(pair[0])? * 16 + digit(pair[1])?);
            } else {
                data.push(digit(pair[0])?);
            }
        }
        Ok(Hash {
            data: data.into_boxed_slice(),
        })
    }

    /// `asString`: lowercase hex.
    pub fn as_string(&self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let odd = self.data[0] == 1;
        let mut s = String::with_capacity((self.data.len() - 1) * 2);
        for (i, &b) in self.data.iter().enumerate().skip(1) {
            if odd && i == self.data.len() - 1 {
                s.push(DIGITS[(b & 0xf) as usize] as char);
            } else {
                s.push(DIGITS[(b >> 4) as usize] as char);
                s.push(DIGITS[(b & 0xf) as usize] as char);
            }
        }
        s
    }

    /// `toShortString`: the first [`SHORT_HASH_LENGTH`] hex digits.
    pub fn to_short_string(&self) -> String {
        let mut s = self.as_string();
        s.truncate(SHORT_HASH_LENGTH);
        s
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.as_string())
    }
}

impl fmt::Debug for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hash({})", self.as_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_even_odd_and_full_hashes() {
        for s in [
            "a",
            "a0",
            "e100001",
            "0e1d130689bc52f140c5c374aa9cc2b8916c0ad7",
        ] {
            assert_eq!(Hash::build(s).as_string(), s);
        }
        assert_eq!(Hash::build("ABCDEF").as_string(), "abcdef");
        assert_eq!(Hash::build("ABCDEF"), Hash::build("abcdef"));
        assert_ne!(Hash::build("a"), Hash::build("0a"));
        assert_eq!(
            Hash::build("0e1d130689bc52f1").to_short_string(),
            "0e1d1306"
        );
    }

    #[test]
    fn rejects_empty_and_non_hex() {
        assert!(Hash::try_build("").is_err());
        assert!(Hash::try_build("xyz").is_err());
        assert!(Hash::try_build("ref: refs/heads/main").is_err());
    }
}
```

`crates/ig-log/src/data/joiner.rs`:

```rust
//! Attaching a freshly read block of recent commits to the loaded log.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogJoiner.java` and
//! `VcsLogRefreshNotEnoughDataException.java`. `NewCommitIntegrator` lives in
//! [`sorter`](crate::data::sorter).
//!
//! Commits of the saved log are "red" when they are no longer reachable from any reference and
//! "green" when they are; the "unsafe" prefix of the saved log, which may contain red commits or
//! where new commits are inserted, is rebuilt and the rest is kept as is.
//!
//! Deviation: the new commits are integrated in first-block order (upstream: `HashSet` order).

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::data::sorter::NewCommitIntegrator;
use crate::graph_commit::GraphCommit;

/// `VcsLogJoiner.ILLEGAL_DATA_RELOAD_ALL`.
pub const ILLEGAL_DATA_RELOAD_ALL: &str = "All data is illegal - request reload all";

/// Why the first block couldn't be joined; the caller reloads the whole log.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JoinError {
    /// `VcsLogRefreshNotEnoughDataException`: the first block doesn't reach the saved log.
    #[error("not enough data to join the recent commits")]
    NotEnoughData,
    /// `IllegalStateException(ILLEGAL_DATA_RELOAD_ALL)`.
    #[error("{0}")]
    IllegalState(&'static str),
}

/// `VcsLogJoiner.addCommits`.
///
/// - `saved_log`: the log as currently loaded.
/// - `previous_refs`: commits referenced by the refs of the previous refresh.
/// - `first_block`: the freshly read most recent commits, newest first.
/// - `new_refs`: commits referenced by all refs now.
///
/// Returns the joined log and the number of commits added (negative when commits were removed).
pub fn add_commits<Id, C>(
    saved_log: &[C],
    previous_refs: &HashSet<Id>,
    first_block: &[C],
    new_refs: &HashSet<Id>,
) -> Result<(Vec<C>, isize), JoinError>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id> + Clone,
{
    let (save_green_index, all_new_commits) =
        get_new_commits_and_saved_green_index(saved_log, previous_refs, first_block, new_refs)?;
    let (save_red_index, remove_commits) =
        get_red_commits_and_saved_red_index(saved_log, previous_refs, first_block, new_refs)?;

    let unsafe_block_size = save_red_index.max(save_green_index);
    let unsafe_part_saved_log =
        build_unsafe_part_saved_log(saved_log, unsafe_block_size, &remove_commits, first_block);
    let unsafe_part_saved_log =
        NewCommitIntegrator::new(unsafe_part_saved_log, all_new_commits).get_result_list();

    let added = unsafe_part_saved_log.len() as isize - unsafe_block_size as isize;
    let mut result = unsafe_part_saved_log;
    result.extend_from_slice(&saved_log[unsafe_block_size..]);
    Ok((result, added))
}

/// Drops removed (red) commits from the unsafe prefix and replaces re-fetched commits with their
/// fresh copies, so their current parents are used.
fn build_unsafe_part_saved_log<Id, C>(
    saved_log: &[C],
    unsafe_block_size: usize,
    remove_commits: &HashSet<Id>,
    first_block: &[C],
) -> Vec<C>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id> + Clone,
{
    let recent_commits_by_id: HashMap<&Id, &C> = first_block.iter().map(|c| (c.id(), c)).collect();
    saved_log[..unsafe_block_size]
        .iter()
        .filter(|commit| !remove_commits.contains(commit.id()))
        .map(|commit| (*recent_commits_by_id.get(commit.id()).unwrap_or(&commit)).clone())
        .collect()
}

fn get_new_commits_and_saved_green_index<Id, C>(
    saved_log: &[C],
    previous_refs: &HashSet<Id>,
    first_block: &[C],
    new_refs: &HashSet<Id>,
) -> Result<(usize, Vec<C>), JoinError>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id> + Clone,
{
    let mut all_unresolved_linked_hashes: HashSet<Id> =
        new_refs.difference(previous_refs).cloned().collect();
    // at this moment it contains only NEW refs
    for commit in first_block {
        all_unresolved_linked_hashes.insert(commit.id().clone());
        all_unresolved_linked_hashes.extend(commit.parents().iter().cloned());
    }
    for commit in first_block {
        if !commit.parents().is_empty() {
            all_unresolved_linked_hashes.remove(commit.id());
        }
    }
    let save_green_index = get_first_un_tracked_index(saved_log, all_unresolved_linked_hashes)?;
    Ok((
        save_green_index,
        get_all_new_commits(&saved_log[..save_green_index], first_block),
    ))
}

fn get_first_un_tracked_index<Id, C>(
    commits: &[C],
    mut search_hashes: HashSet<Id>,
) -> Result<usize, JoinError>
where
    Id: Eq + Hash,
    C: GraphCommit<Id>,
{
    for (last_index, commit) in commits.iter().enumerate() {
        if search_hashes.is_empty() {
            return Ok(last_index);
        }
        search_hashes.remove(commit.id());
    }
    if !search_hashes.is_empty() {
        return Err(JoinError::NotEnoughData);
    }
    Ok(commits.len())
}

fn get_all_new_commits<Id, C>(unsafe_green_part_saved_log: &[C], first_block: &[C]) -> Vec<C>
where
    Id: Eq + Hash,
    C: GraphCommit<Id> + Clone,
{
    let existed_commit_hashes: HashSet<&Id> =
        unsafe_green_part_saved_log.iter().map(|c| c.id()).collect();
    let mut seen = HashSet::new();
    first_block
        .iter()
        .filter(|c| !existed_commit_hashes.contains(c.id()) && seen.insert(c.id()))
        .cloned()
        .collect()
}

fn get_red_commits_and_saved_red_index<Id, C>(
    saved_log: &[C],
    previous_refs: &HashSet<Id>,
    first_block: &[C],
    new_refs: &HashSet<Id>,
) -> Result<(usize, HashSet<Id>), JoinError>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id>,
{
    let start_red_commits: HashSet<Id> = previous_refs.difference(new_refs).cloned().collect();
    let mut start_green_nodes: HashSet<Id> = new_refs.clone();
    let mut recent_commit_parents: HashMap<Id, &[Id]> = HashMap::new();
    for commit in first_block {
        recent_commit_parents.insert(commit.id().clone(), commit.parents());
        start_green_nodes.insert(commit.id().clone());
        start_green_nodes.extend(commit.parents().iter().cloned());
    }
    let mut sorter = RedGreenSorter {
        current_red: start_red_commits,
        current_green: start_green_nodes,
        all_red_commit: HashSet::new(),
        saved_log,
        recent_commit_parents,
    };
    let save_red_index = sorter.get_first_save_index()?;
    Ok((save_red_index, sorter.all_red_commit))
}

struct RedGreenSorter<'a, Id, C> {
    current_red: HashSet<Id>,
    current_green: HashSet<Id>,
    all_red_commit: HashSet<Id>,
    saved_log: &'a [C],
    recent_commit_parents: HashMap<Id, &'a [Id]>,
}

impl<Id, C> RedGreenSorter<'_, Id, C>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id>,
{
    fn mark_real_red_node(&mut self, node: &Id) -> Result<(), JoinError> {
        if !self.current_red.remove(node) {
            // see VcsLogJoinerTest#illegalStateExceptionTest2
            return Err(JoinError::IllegalState(ILLEGAL_DATA_RELOAD_ALL));
        }
        self.all_red_commit.insert(node.clone());
        Ok(())
    }

    fn get_first_save_index(&mut self) -> Result<usize, JoinError> {
        for (last_index, commit) in self.saved_log.iter().enumerate() {
            let is_green = self.current_green.contains(commit.id());
            if is_green {
                self.current_red.remove(commit.id());
                // A re-fetched commit's updated parents are already green; propagating the saved
                // parents would resurrect dropped ones, which become (tentative) red instead.
                match self.recent_commit_parents.get(commit.id()) {
                    None => self.current_green.extend(commit.parents().iter().cloned()),
                    Some(updated_parents) => {
                        let updated: HashSet<&Id> = updated_parents.iter().collect();
                        let dropped = commit.parents().iter().filter(|p| !updated.contains(p));
                        self.current_red.extend(dropped.cloned());
                    }
                }
            } else {
                self.mark_real_red_node(commit.id())?;
                self.current_red.extend(commit.parents().iter().cloned());
            }

            if self.current_red.is_empty() {
                return Ok(last_index + 1);
            }
        }
        // see VcsLogJoinerTest#illegalStateExceptionTest
        Err(JoinError::IllegalState(ILLEGAL_DATA_RELOAD_ALL))
    }
}
```

`crates/ig-log/src/data/multi_repo_joiner.rs`:

```rust
//! Merging the logs of several repositories into one list.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogMultiRepoJoiner.java`.
//!
//! Each log keeps its own (topological) order; at each step the newest of the next commits of all
//! logs is taken. Deviation: ties go to the last log with that timestamp in the given order
//! (upstream: the last in `HashMap` iteration order).

use crate::graph_commit::GraphCommit;

/// `VcsLogMultiRepoJoiner.join`.
pub fn join<Id, C: GraphCommit<Id>>(logs_from_repos: Vec<Vec<C>>) -> Vec<C> {
    if logs_from_repos.len() == 1 {
        return logs_from_repos.into_iter().next().expect("one log");
    }
    let size = logs_from_repos.iter().map(Vec::len).sum();
    let mut result = Vec::with_capacity(size);

    let mut iterators: Vec<_> = logs_from_repos
        .into_iter()
        .map(|log| log.into_iter().peekable())
        .collect();
    loop {
        // `findLatestCommit`: `>=`, so the last of equal timestamps wins
        let mut latest: Option<(usize, i64)> = None;
        for (i, it) in iterators.iter_mut().enumerate() {
            if let Some(commit) = it.peek() {
                let timestamp = commit.timestamp();
                if latest.is_none_or(|(_, max)| timestamp >= max) {
                    latest = Some((i, timestamp));
                }
            }
        }
        let Some((i, _)) = latest else { break };
        result.push(iterators[i].next().expect("peeked"));
    }
    result
}
```

`crates/ig-log/src/data/provider.rs`:

```rust
//! The interface a VCS implements to feed the log, and the reference manager it supplies.
//!
//! Ported from the data-reading parts of `platform/vcs-log/api/src/com/intellij/vcs/log/VcsLogProvider.kt`,
//! `VcsLogRefManager.java`, `VcsLogProviderRequirementsEx.java` (same package),
//! `platform/vcs-log/impl/src/com/intellij/vcs/log/impl/RequirementsImpl.java`,
//! `SimpleLogProviderRequirements.kt` and `LogDataImpl.java`.
//!
//! Deviations:
//! - Calls are synchronous (`readRecentCommits` is the `readFirstBlock` default; there is no
//!   experimental refs loading policy). The refresher runs them on the caller's thread.
//! - Not ported yet: filters (`getCommitsMatchingFilter`), full details (`readFullDetails`),
//!   containing branches, diff/file-history handlers, root refresh subscription (the UI calls
//!   [`VcsLogRefresher::refresh`](crate::data::VcsLogRefresher::refresh) after repository changes),
//!   ref grouping for the table and branch filter, favorites and ref type serialization.

use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::Arc;

use crate::data::commit::{TimedVcsCommit, VcsCommitMetadata};
use crate::data::root::VcsRoot;
use crate::data::vcs_ref::{VcsRef, VcsRefsContainer};
use crate::data::vcs_user::VcsUser;

/// A VCS operation failed (`VcsException`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct VcsException(pub String);

/// Orders references (`VcsLogRefManager`, comparators only).
pub trait VcsLogRefManager: Send + Sync {
    /// `getBranchLayoutComparator`: the most important branch is the smallest; it is laid out
    /// leftmost and gives its color to the commits below it.
    fn branch_layout_compare(&self, ref1: &VcsRef, ref2: &VcsRef) -> Ordering;

    /// `getLabelsOrderComparator`: the order of reference labels on a commit row.
    fn labels_order_compare(&self, ref1: &VcsRef, ref2: &VcsRef) -> Ordering;

    /// Upstream `toString()`, used to order heads of different VCSs.
    fn name(&self) -> String;
}

/// What a refresh may rely on (`VcsLogProviderRequirementsEx`).
pub struct RequirementsEx<'a> {
    /// Refresh rather than initialization.
    pub refresh: bool,
    /// Whether all refs must be reloaded.
    pub refresh_refs: bool,
    /// The refs that were in the log before the refresh.
    pub previous_refs: &'a dyn VcsRefsContainer,
}

/// `VcsLogProvider.Requirements`: how many recent commits to read, plus refresh information for
/// [`RequirementsImpl`](Requirements::refresh) (`None` for `SimpleLogProviderRequirements`).
pub struct Requirements<'a> {
    pub commit_count: usize,
    pub ex: Option<RequirementsEx<'a>>,
}

impl<'a> Requirements<'a> {
    /// `SimpleLogProviderRequirements`.
    pub fn simple(commit_count: usize) -> Requirements<'static> {
        Requirements {
            commit_count,
            ex: None,
        }
    }

    /// `RequirementsImpl(count, refresh, previousRefs)`: refs are reloaded.
    pub fn refresh(
        commit_count: usize,
        refresh: bool,
        previous_refs: &'a dyn VcsRefsContainer,
    ) -> Requirements<'a> {
        Requirements::refresh_with(commit_count, refresh, previous_refs, true)
    }

    /// `RequirementsImpl(count, refresh, previousRefs, isRefreshRefs)`.
    pub fn refresh_with(
        commit_count: usize,
        refresh: bool,
        previous_refs: &'a dyn VcsRefsContainer,
        refresh_refs: bool,
    ) -> Requirements<'a> {
        Requirements {
            commit_count,
            ex: Some(RequirementsEx {
                refresh,
                refresh_refs,
                previous_refs,
            }),
        }
    }
}

/// Recent commits with metadata and the references (`DetailedLogData` / `LogDataImpl`).
#[derive(Clone, Debug, Default)]
pub struct DetailedLogData {
    pub commits: Vec<VcsCommitMetadata>,
    pub refs: Vec<VcsRef>,
}

/// All references and users of a repository (`LogData` / `LogDataImpl`).
#[derive(Clone, Debug, Default)]
pub struct LogData {
    pub refs: HashSet<VcsRef>,
    pub users: HashSet<VcsUser>,
}

/// Supplies the log data of a VCS (`VcsLogProvider`, data-reading part).
pub trait VcsLogProvider: Send + Sync {
    /// `readFirstBlock` (and the default `readRecentCommits`): the most recent commits, at least
    /// topologically ordered, with all references (or, without
    /// [`RequirementsEx::refresh_refs`], those found on these commits).
    fn read_first_block(
        &self,
        root: &VcsRoot,
        requirements: &Requirements<'_>,
    ) -> Result<DetailedLogData, VcsException>;

    /// `readAllHashes`: reports every commit, in topological order, to `commit_consumer`; returns
    /// all references and users.
    fn read_all_hashes(
        &self,
        root: &VcsRoot,
        commit_consumer: &mut dyn FnMut(TimedVcsCommit),
    ) -> Result<LogData, VcsException>;

    /// `readMetadata`: the metadata of the given commits.
    fn read_metadata(
        &self,
        root: &VcsRoot,
        hashes: &[String],
        consumer: &mut dyn FnMut(VcsCommitMetadata),
    ) -> Result<(), VcsException>;

    /// `getReferenceManager`. Roots sharing a provider must return the same `Arc`.
    fn reference_manager(&self) -> Arc<dyn VcsLogRefManager>;

    /// `getCurrentUser`: the configured user, or `None` if no user name is configured.
    fn get_current_user(&self, root: &VcsRoot) -> Result<Option<VcsUser>, VcsException>;

    /// `getCurrentBranch`: the current branch, `HEAD` when detached, `None` without commits.
    fn get_current_branch(&self, root: &VcsRoot) -> Option<String>;

    /// `isFullHash`: whether `hash` is a full commit hash in `root` (SHA-1 by default).
    fn is_full_hash(&self, _root: &VcsRoot, hash: &str) -> bool {
        hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit())
    }
}
```

`crates/ig-log/src/data/refresher.rs`:

```rust
//! Loading the log and keeping it up to date.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogRefresherImpl.kt`
//! (with `RefreshSessionData`).
//!
//! Like upstream, initialization shows the first block of recent commits, then the whole history;
//! a refresh reads the recent commits again and joins them to the loaded log
//! ([`joiner`]), reading 5x more once if they don't reach it, and reloads
//! everything if that still fails.
//!
//! Deviations:
//! - Synchronous: each call runs on the caller's thread (the UI calls it on a background executor
//!   and serializes calls). There is no request queue, so concurrent requests are not batched;
//!   pass all changed roots to one [`VcsLogRefresher::refresh`]. No progress, cancellation,
//!   overlay data or tracing.
//! - Every commit list is checked before the graph is built: commit ids must be unique and every
//!   parent that is in the list must come after its child (upstream builds the graph silently,
//!   treating such parents as not loaded). A joined log failing the check is treated like a failed
//!   join (full reload); a first block or full log failing it gives an error data pack.
//! - Roots are processed in path order (upstream: `HashMap` order), so multi-repository joins are
//!   deterministic.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use crate::data::commit::VcsCommitMetadata;
use crate::data::graph_data::{LogGraphCommit, LogProviders, VcsLogGraphData, build_data};
use crate::data::hash::Hash;
use crate::data::joiner;
use crate::data::multi_repo_joiner;
use crate::data::provider::{Requirements, VcsException};
use crate::data::refs_model::RootRefsModel;
use crate::data::root::VcsRoot;
use crate::data::storage::{VcsLogCommitStorageIndex, VcsLogStorage};
use crate::data::vcs_ref::VcsRefsList;
use crate::graph_commit::{GraphCommit, GraphCommitImpl};
use crate::permanent_graph::PermanentGraph;

/// Upstream `EmptyRefs`.
static EMPTY_REFS: VcsRefsList = VcsRefsList(Vec::new());

type DataPackHandler = Box<dyn FnMut(&Arc<VcsLogGraphData>) + Send>;
type RecentDetailsConsumer = Box<dyn FnMut(&VcsRoot, &[VcsCommitMetadata]) + Send>;

/// Commits and refs read in one refresh, by root (`RefreshSessionData`).
#[derive(Default)]
struct RefreshSessionData {
    refs_by_root: BTreeMap<VcsRoot, Arc<RootRefsModel>>,
    commits_by_root: BTreeMap<VcsRoot, Vec<LogGraphCommit>>,
}

impl RefreshSessionData {
    fn put_all(&mut self, other: RefreshSessionData) {
        self.commits_by_root.extend(other.commits_by_root);
        self.refs_by_root.extend(other.refs_by_root);
    }

    fn put(&mut self, root: VcsRoot, commits: Vec<LogGraphCommit>, refs: RootRefsModel) {
        self.commits_by_root.insert(root.clone(), commits);
        self.refs_by_root.insert(root, Arc::new(refs));
    }
}

/// Loads and refreshes the log of a set of roots (`VcsLogRefresherImpl`, synchronous).
pub struct VcsLogRefresher {
    storage: Arc<dyn VcsLogStorage>,
    providers: LogProviders,
    recent_commit_count: usize,
    current_data_pack: Arc<VcsLogGraphData>,
    data_pack_handler: Option<DataPackHandler>,
    recent_details_consumer: Option<RecentDetailsConsumer>,
}

impl VcsLogRefresher {
    /// `recent_commit_count` is the size of the first block (upstream default: 1000, from the
    /// registry key `vcs.log.recent.commits.count`).
    pub fn new(
        storage: Arc<dyn VcsLogStorage>,
        providers: LogProviders,
        recent_commit_count: usize,
    ) -> VcsLogRefresher {
        VcsLogRefresher {
            current_data_pack: Arc::new(VcsLogGraphData::empty(storage.clone())),
            storage,
            providers,
            recent_commit_count,
            data_pack_handler: None,
            recent_details_consumer: None,
        }
    }

    /// Called with every new data pack (upstream `dataPackUpdateHandler`), on the refreshing thread.
    pub fn set_data_pack_handler(
        &mut self,
        handler: impl FnMut(&Arc<VcsLogGraphData>) + Send + 'static,
    ) {
        self.data_pack_handler = Some(Box::new(handler));
    }

    /// Receives the metadata read with each first block (upstream
    /// `VcsLogCommitDataConsumer.storeRecentDetails`), e.g. to cache the top rows of the table.
    pub fn set_recent_details_consumer(
        &mut self,
        consumer: impl FnMut(&VcsRoot, &[VcsCommitMetadata]) + Send + 'static,
    ) {
        self.recent_details_consumer = Some(Box::new(consumer));
    }

    pub fn storage(&self) -> &Arc<dyn VcsLogStorage> {
        &self.storage
    }

    pub fn current_data_pack(&self) -> &Arc<VcsLogGraphData> {
        &self.current_data_pack
    }

    fn publish(&mut self, data_pack: VcsLogGraphData) -> Arc<VcsLogGraphData> {
        let data_pack = Arc::new(data_pack);
        self.current_data_pack = data_pack.clone();
        if let Some(handler) = &mut self.data_pack_handler {
            handler(&data_pack);
        }
        data_pack
    }

    /// `initialize`: publishes the first block, then loads and publishes the full log, which is
    /// returned.
    pub fn initialize(&mut self) -> Arc<VcsLogGraphData> {
        let first = self
            .load_first_block()
            .unwrap_or_else(|e| VcsLogGraphData::error(self.storage.clone(), e));
        self.publish(first);
        let roots: Vec<VcsRoot> = self.providers.keys().cloned().collect();
        self.refresh(&roots)
    }

    /// `refresh` (one request): rereads the recent commits of `roots` and joins them to the log,
    /// or loads the full log if it is not loaded yet or can't be joined. Publishes and returns the
    /// new data pack (the current one when `roots` is empty).
    pub fn refresh(&mut self, roots: &[VcsRoot]) -> Arc<VcsLogGraphData> {
        if roots.is_empty() {
            return self.current_data_pack.clone();
        }
        let current = self.current_data_pack.clone();
        let result = (|| {
            let partially_loaded_log = if current.is_full() {
                self.load_updated_data_pack(&current, roots)?
            } else {
                None
            };
            match partially_loaded_log {
                Some(data_pack) => Ok(data_pack),
                None => self.load_full_log(),
            }
        })();
        let data_pack = result.unwrap_or_else(|e| VcsLogGraphData::error(self.storage.clone(), e));
        self.publish(data_pack)
    }

    fn load_first_block(&mut self) -> Result<VcsLogGraphData, VcsException> {
        let count = self.recent_commit_count;
        let requirements = self
            .providers
            .keys()
            .map(|root| (root.clone(), Requirements::simple(count)))
            .collect();
        let data = self.load_recent_data(requirements)?;
        let mut compound_list =
            multi_repo_joiner::join(data.commits_by_root.into_values().collect());
        compound_list.truncate(count);
        check_commits(&compound_list).map_err(VcsException)?;
        Ok(build_data(
            &compound_list,
            data.refs_by_root,
            &self.providers,
            &self.storage,
            false,
        ))
    }

    fn load_updated_data_pack(
        &mut self,
        data_pack: &VcsLogGraphData,
        roots: &[VcsRoot],
    ) -> Result<Option<VcsLogGraphData>, VcsException> {
        let full_log: Vec<LogGraphCommit> = data_pack
            .permanent_graph()
            .map(|graph| graph.all_commits().iter().collect())
            .unwrap_or_default();
        let current_refs = data_pack.refs_model().refs_by_root().clone();
        let mut session = RefreshSessionData::default();
        let mut commit_count = self.recent_commit_count;
        for _ in 0..2 {
            let requirements = prepare_requirements(roots, commit_count, &current_refs);
            let attempt = self.load_recent_data(requirements)?;
            session.put_all(attempt);
            let compound_log =
                multi_repo_joiner::join(session.commits_by_root.values().cloned().collect());
            let all_new_refs: BTreeMap<VcsRoot, Arc<RootRefsModel>> = current_refs
                .iter()
                .map(|(root, refs)| {
                    let refs = session.refs_by_root.get(root).unwrap_or(refs);
                    (root.clone(), refs.clone())
                })
                .collect();
            let joined = join(&full_log, compound_log, &current_refs, &all_new_refs)
                .filter(|joined| check_commits(joined).is_ok());
            if let Some(joined) = joined {
                return Ok(Some(build_data(
                    &joined,
                    all_new_refs,
                    &self.providers,
                    &self.storage,
                    true,
                )));
            }
            commit_count *= 5;
        }
        // couldn't join => need to reload everything
        Ok(None)
    }

    fn load_full_log(&mut self) -> Result<VcsLogGraphData, VcsException> {
        let data = self.read_full_log_from_vcs()?;
        let graph_commits = multi_repo_joiner::join(data.commits_by_root.into_values().collect());
        check_commits(&graph_commits).map_err(VcsException)?;
        Ok(build_data(
            &graph_commits,
            data.refs_by_root,
            &self.providers,
            &self.storage,
            true,
        ))
    }

    fn load_recent_data(
        &mut self,
        requirements: Vec<(VcsRoot, Requirements<'_>)>,
    ) -> Result<RefreshSessionData, VcsException> {
        let mut session = RefreshSessionData::default();
        for (root, requirements) in requirements {
            let provider = self
                .providers
                .get(&root)
                .ok_or_else(|| {
                    VcsException(format!("Cannot find provider for root {}", root.display()))
                })?
                .clone();
            let data = provider.read_first_block(&root, &requirements)?;
            let commits = data
                .commits
                .iter()
                .map(|c| compact_commit(self.storage.as_ref(), c, &root))
                .collect();
            let refs = RootRefsModel::create(data.refs, self.storage.as_ref());
            session.put(root.clone(), commits, refs);
            if let Some(consumer) = &mut self.recent_details_consumer {
                consumer(&root, &data.commits);
            }
        }
        Ok(session)
    }

    fn read_full_log_from_vcs(&mut self) -> Result<RefreshSessionData, VcsException> {
        let mut session = RefreshSessionData::default();
        for (root, provider) in &self.providers {
            let mut graph_commits = Vec::new();
            let storage = self.storage.as_ref();
            let data = provider.read_all_hashes(root, &mut |commit| {
                graph_commits.push(compact_commit(storage, &commit, root));
            })?;
            let refs = RootRefsModel::create(data.refs, storage);
            session.put(root.clone(), graph_commits, refs);
        }
        Ok(session)
    }
}

/// `prepareRequirements` with previous refs.
fn prepare_requirements<'a>(
    roots: &[VcsRoot],
    commit_count: usize,
    prev_refs: &'a BTreeMap<VcsRoot, Arc<RootRefsModel>>,
) -> Vec<(VcsRoot, Requirements<'a>)> {
    roots
        .iter()
        .map(|root| {
            let requirements = match prev_refs.get(root) {
                None => Requirements::refresh_with(commit_count, true, &EMPTY_REFS, false),
                Some(refs) => Requirements::refresh(commit_count, true, refs.as_ref()),
            };
            (root.clone(), requirements)
        })
        .collect()
}

/// `compactCommit`: the commit with storage indexes.
pub fn compact_commit(
    storage: &dyn VcsLogStorage,
    commit: &impl GraphCommit<Hash>,
    root: &VcsRoot,
) -> LogGraphCommit {
    let hashes: Vec<&Hash> = std::iter::once(commit.id())
        .chain(commit.parents())
        .collect();
    let mut indexes = storage.get_commit_indexes(&hashes, root);
    let id = indexes.remove(0);
    GraphCommitImpl::create_commit(id, indexes, commit.timestamp())
}

/// `join`: `None` when the recent commits can't be joined to `full_log`.
fn join(
    full_log: &[LogGraphCommit],
    recent_commits: Vec<LogGraphCommit>,
    previous_refs: &BTreeMap<VcsRoot, Arc<RootRefsModel>>,
    new_refs: &BTreeMap<VcsRoot, Arc<RootRefsModel>>,
) -> Option<Vec<LogGraphCommit>> {
    if full_log.is_empty() {
        return Some(recent_commits);
    }
    let prev_ref_indices: HashSet<VcsLogCommitStorageIndex> = previous_refs
        .values()
        .flat_map(|r| r.get_refs_indexes())
        .collect();
    let new_ref_indices: HashSet<VcsLogCommitStorageIndex> = new_refs
        .values()
        .flat_map(|r| r.get_refs_indexes())
        .collect();
    // NotEnoughData is a valid case (e.g. a long-developed branch was merged); IllegalState
    // "happens from time to time" upstream. Both lead to a full reload.
    joiner::add_commits(
        full_log,
        &prev_ref_indices,
        &recent_commits,
        &new_ref_indices,
    )
    .ok()
    .map(|(log, _)| log)
}

/// Checks that commit ids are unique and that no parent in the list precedes its child.
pub fn check_commits(commits: &[LogGraphCommit]) -> Result<(), String> {
    let max_id = commits.iter().map(|c| *c.id()).max().unwrap_or(0).max(0) as usize;
    let mut positions = vec![usize::MAX; max_id + 1];
    for (i, commit) in commits.iter().enumerate() {
        let id = *commit.id();
        if id < 0 {
            return Err(format!("Invalid commit id {id}"));
        }
        if positions[id as usize] != usize::MAX {
            return Err(format!(
                "Duplicate commit id {id} at rows {} and {i}",
                positions[id as usize]
            ));
        }
        positions[id as usize] = i;
    }
    for (i, commit) in commits.iter().enumerate() {
        for &parent in commit.parents() {
            let p = positions
                .get(parent as usize)
                .copied()
                .unwrap_or(usize::MAX);
            if p != usize::MAX && p < i {
                return Err(format!(
                    "Commits are not in topological order: parent {parent} (row {p}) of {} (row {i})",
                    commit.id()
                ));
            }
        }
    }
    Ok(())
}
```

`crates/ig-log/src/data/refs_model.rs`:

```rust
//! References indexed by commit.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/RefsModel.kt`,
//! `VcsLogRefsOfSingleRootFactory.kt` (`RootRefsModel`), and the interfaces
//! `platform/vcs-log/api/src/com/intellij/vcs/log/{VcsLogAggregatedStoredRefs,VcsLogRootStoredRefs}.kt`.
//!
//! Deviations: `get_ref_for_head_commit` also caches "no ref" results (upstream recomputes them);
//! the reference managers are passed directly instead of through the providers map.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::data::provider::VcsLogRefManager;
use crate::data::root::VcsRoot;
use crate::data::storage::{VcsLogCommitStorageIndex, VcsLogStorage};
use crate::data::vcs_ref::{VcsRef, VcsRefsContainer};

/// The references of one root, by commit index (`RootRefsModel`).
#[derive(Debug, Default)]
pub struct RootRefsModel {
    branches_mapping: HashMap<VcsLogCommitStorageIndex, Vec<VcsRef>>,
    tags_mapping: HashMap<VcsLogCommitStorageIndex, Vec<VcsRef>>,
}

impl RootRefsModel {
    /// `RootRefsModel.create`: all `refs` must belong to the same root.
    pub fn create(
        refs: impl IntoIterator<Item = VcsRef>,
        storage: &dyn VcsLogStorage,
    ) -> RootRefsModel {
        let mut model = RootRefsModel::default();
        let mut root: Option<VcsRoot> = None;
        for vcs_ref in refs {
            debug_assert!(
                root.as_ref().is_none_or(|r| r == vcs_ref.root()),
                "All references are supposed to be from the single root"
            );
            root = Some(vcs_ref.root().clone());
            let index = storage.get_commit_index(vcs_ref.commit_hash(), vcs_ref.root());
            let mapping = if vcs_ref.ref_type().is_branch() {
                &mut model.branches_mapping
            } else {
                &mut model.tags_mapping
            };
            mapping.entry(index).or_default().push(vcs_ref);
        }
        model
    }

    pub fn contains(&self, index: VcsLogCommitStorageIndex) -> bool {
        self.branches_mapping.contains_key(&index) || self.tags_mapping.contains_key(&index)
    }

    /// Branches first, then tags, each in insertion order.
    pub fn refs_to_commit(&self, index: VcsLogCommitStorageIndex) -> Vec<VcsRef> {
        let mut result = Vec::new();
        if let Some(refs) = self.branches_mapping.get(&index) {
            result.extend(refs.iter().cloned());
        }
        if let Some(refs) = self.tags_mapping.get(&index) {
            result.extend(refs.iter().cloned());
        }
        result
    }

    /// The commits that have references.
    pub fn get_refs_indexes(&self) -> HashSet<VcsLogCommitStorageIndex> {
        self.branches_mapping
            .keys()
            .chain(self.tags_mapping.keys())
            .copied()
            .collect()
    }
}

impl VcsRefsContainer for RootRefsModel {
    fn branches(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.branches_mapping.values().flatten())
    }

    fn tags(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.tags_mapping.values().flatten())
    }
}

/// The references of all roots (`RefsModel`, a `VcsLogAggregatedStoredRefs`).
pub struct RefsModel {
    refs_by_root: BTreeMap<VcsRoot, Arc<RootRefsModel>>,
    storage: Arc<dyn VcsLogStorage>,
    ref_managers: BTreeMap<VcsRoot, Arc<dyn VcsLogRefManager>>,
    single_root: Option<VcsRoot>,
    best_ref_for_head: Mutex<HashMap<VcsLogCommitStorageIndex, Option<VcsRef>>>,
    root_for_head: Mutex<HashMap<VcsLogCommitStorageIndex, Option<VcsRoot>>>,
}

impl RefsModel {
    /// `RefsModel.create`.
    pub fn create(
        refs: BTreeMap<VcsRoot, Arc<RootRefsModel>>,
        storage: Arc<dyn VcsLogStorage>,
        ref_managers: BTreeMap<VcsRoot, Arc<dyn VcsLogRefManager>>,
    ) -> RefsModel {
        let single_root = if refs.len() == 1 {
            refs.keys().next().cloned()
        } else {
            None
        };
        RefsModel {
            refs_by_root: refs,
            storage,
            ref_managers,
            single_root,
            best_ref_for_head: Mutex::new(HashMap::new()),
            root_for_head: Mutex::new(HashMap::new()),
        }
    }

    /// `RefsModel.createEmptyInstance`.
    pub fn create_empty_instance(storage: Arc<dyn VcsLogStorage>) -> RefsModel {
        RefsModel::create(BTreeMap::new(), storage, BTreeMap::new())
    }

    pub fn refs_by_root(&self) -> &BTreeMap<VcsRoot, Arc<RootRefsModel>> {
        &self.refs_by_root
    }

    /// `getRefForHeadCommit`: the most important reference of a branch head, by the root's
    /// branch layout comparator.
    pub fn get_ref_for_head_commit(&self, head_index: VcsLogCommitStorageIndex) -> Option<VcsRef> {
        let mut cache = self
            .best_ref_for_head
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(cached) = cache.get(&head_index) {
            return cached.clone();
        }
        let best = self.storage.get_commit_id(head_index).and_then(|id| {
            let manager = self.ref_managers.get(&id.root)?;
            // `minWithOrNull`: the first of equal minimums
            self.refs_to_commit(head_index)
                .into_iter()
                .reduce(|best, r| {
                    if manager.branch_layout_compare(&best, &r).is_gt() {
                        r
                    } else {
                        best
                    }
                })
        });
        cache.insert(head_index, best.clone());
        best
    }

    /// `getRootForHeadCommit`.
    pub fn get_root_for_head_commit(
        &self,
        head_index: VcsLogCommitStorageIndex,
    ) -> Option<VcsRoot> {
        if let Some(root) = &self.single_root {
            return Some(root.clone());
        }
        let mut cache = self.root_for_head.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .entry(head_index)
            .or_insert_with(|| self.storage.get_commit_id(head_index).map(|id| id.root))
            .clone()
    }

    /// `refsToCommit`: the references of a commit, branches before tags.
    pub fn refs_to_commit(&self, index: VcsLogCommitStorageIndex) -> Vec<VcsRef> {
        if let Some(root) = &self.single_root {
            return self.refs_by_root[root].refs_to_commit(index);
        }
        if self.refs_by_root.len() <= 10 {
            return self
                .refs_by_root
                .values()
                .find(|refs| refs.contains(index))
                .map(|refs| refs.refs_to_commit(index))
                .unwrap_or_default();
        }
        match self.storage.get_commit_id(index) {
            Some(id) => self.refs_to_commit_in_root(&id.root, index),
            None => Vec::new(),
        }
    }

    /// `refsToCommit(root, index)`.
    pub fn refs_to_commit_in_root(
        &self,
        root: &VcsRoot,
        index: VcsLogCommitStorageIndex,
    ) -> Vec<VcsRef> {
        self.refs_by_root
            .get(root)
            .map(|refs| refs.refs_to_commit(index))
            .unwrap_or_default()
    }
}

impl VcsRefsContainer for RefsModel {
    fn branches(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.refs_by_root.values().flat_map(|r| r.branches()))
    }

    fn tags(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.refs_by_root.values().flat_map(|r| r.tags()))
    }
}
```

`crates/ig-log/src/data/root.rs`:

```rust
//! Repository roots.
//!
//! Upstream identifies a repository by its root `VirtualFile`. Here a root is a shared path,
//! cheap to clone into every [`VcsRef`](crate::data::VcsRef) and
//! [`CommitId`](crate::data::CommitId).

use std::cmp::Ordering;
use std::path::Path;
use std::sync::Arc;

/// The root directory of a repository.
pub type VcsRoot = Arc<Path>;

/// Creates a [`VcsRoot`].
pub fn vcs_root(path: impl AsRef<Path>) -> VcsRoot {
    Arc::from(path.as_ref())
}

/// `VcsLogUtil.compareRoots`: compares the presentable paths as strings.
pub fn compare_roots(root1: &VcsRoot, root2: &VcsRoot) -> Ordering {
    root1.to_string_lossy().cmp(&root2.to_string_lossy())
}
```

`crates/ig-log/src/data/sorter.rs`:

```rust
//! Sorting a block of commits by date, keeping topological order.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogSorter.java` and
//! `VcsLogJoiner.NewCommitIntegrator` (`VcsLogJoiner.java`).
//!
//! Deviation: upstream picks the next commit to integrate from a `HashMap` (iteration order of the
//! hashes); here it is the first remaining commit in input order, which makes the result
//! deterministic.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

use crate::graph_commit::GraphCommit;

/// `VcsLogSorter.sortByDateTopoOrder`: newest first, children before parents.
pub fn sort_by_date_topo_order<Id, C>(commits: Vec<C>) -> Vec<C>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id>,
{
    NewCommitIntegrator::new(Vec::new(), commits).get_result_list()
}

/// Inserts new commits into a list, parents first, each one before its first parent already in
/// the list or before the first older commit (`NewCommitIntegrator`).
pub struct NewCommitIntegrator<Id, C> {
    list: Vec<C>,
    /// The new commits in input order; taken ones are `None`.
    new_commits: Vec<Option<C>>,
    new_commits_index: HashMap<Id, usize>,
    first_remaining: usize,
    commits_stack: Vec<C>,
}

impl<Id, C> NewCommitIntegrator<Id, C>
where
    Id: Clone + Eq + Hash,
    C: GraphCommit<Id>,
{
    pub fn new(list: Vec<C>, new_commits: Vec<C>) -> NewCommitIntegrator<Id, C> {
        let mut new_commits_index = HashMap::with_capacity(new_commits.len());
        for (i, commit) in new_commits.iter().enumerate() {
            // `HashMap.put`: a later commit with the same id replaces the earlier one
            new_commits_index.insert(commit.id().clone(), i);
        }
        let new_commits = new_commits
            .into_iter()
            .enumerate()
            .map(|(i, c)| (new_commits_index.get(c.id()) == Some(&i)).then_some(c))
            .collect();
        NewCommitIntegrator {
            list,
            new_commits,
            new_commits_index,
            first_remaining: 0,
            commits_stack: Vec::new(),
        }
    }

    fn take_new_commit(&mut self, id: &Id) -> Option<C> {
        let index = self.new_commits_index.remove(id)?;
        self.new_commits[index].take()
    }

    fn first_new_commit(&mut self) -> Option<C> {
        while self.first_remaining < self.new_commits.len() {
            if let Some(commit) = self.new_commits[self.first_remaining].take() {
                self.new_commits_index.remove(commit.id());
                return Some(commit);
            }
            self.first_remaining += 1;
        }
        None
    }

    fn insert_all_use_stack(&mut self) {
        while let Some(first) = self.first_new_commit() {
            self.commits_stack.push(first);
            while let Some(current) = self.commits_stack.last() {
                let next_parent = current
                    .parents()
                    .iter()
                    .find(|p| self.new_commits_index.contains_key(*p))
                    .cloned();
                if let Some(parent) = next_parent {
                    let parent_commit = self.take_new_commit(&parent).expect("indexed");
                    self.commits_stack.push(parent_commit);
                    continue;
                }

                let current = self.commits_stack.pop().expect("not empty");
                let parents: HashSet<&Id> = current.parents().iter().collect();
                let insert_index = self
                    .list
                    .iter()
                    .position(|some| {
                        parents.contains(some.id()) || some.timestamp() < current.timestamp()
                    })
                    .unwrap_or(self.list.len());
                self.list.insert(insert_index, current);
            }
        }
    }

    /// `getResultList`.
    pub fn get_result_list(mut self) -> Vec<C> {
        self.insert_all_use_stack();
        self.list
    }
}
```

`crates/ig-log/src/data/storage.rs`:

```rust
//! Commit id interning: (root, hash) <-> dense integer indexes.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogStorage.kt` and
//! `InMemoryStorage.java` (with `BiDirectionalEnumerator`: indexes start at 1).
//!
//! The indexes are the commit ids of the permanent graph, as in upstream's production graph.
//! Indexes are never reused, so an index stays valid across refreshes.
//!
//! Deviations: no persistent storage (`VcsLogStorageImpl`); [`VcsLogStorage::get_commit_indexes`]
//! interns a commit with its parents under one lock.

use std::collections::HashMap;
use std::sync::RwLock;

use crate::data::commit::CommitId;
use crate::data::hash::Hash;
use crate::data::root::VcsRoot;
use crate::data::vcs_ref::VcsRef;

/// `VcsLogCommitStorageIndex`.
pub type VcsLogCommitStorageIndex = i32;

/// Maps commits and references to integers (`VcsLogStorage`).
pub trait VcsLogStorage: Send + Sync {
    /// The index of the commit, assigning a new one for an unknown commit.
    fn get_commit_index(&self, hash: &Hash, root: &VcsRoot) -> VcsLogCommitStorageIndex;

    /// The indexes of several commits of one root, in order.
    fn get_commit_indexes(
        &self,
        hashes: &[&Hash],
        root: &VcsRoot,
    ) -> Vec<VcsLogCommitStorageIndex> {
        hashes
            .iter()
            .map(|hash| self.get_commit_index(hash, root))
            .collect()
    }

    /// The commit of an index, or `None` for an unknown index.
    fn get_commit_id(&self, commit_index: VcsLogCommitStorageIndex) -> Option<CommitId>;

    fn contains_commit(&self, id: &CommitId) -> bool;

    /// The index of a reference, assigning a new one for an unknown reference.
    fn get_ref_index(&self, vcs_ref: &VcsRef) -> i32;

    fn get_vcs_ref(&self, ref_index: i32) -> Option<VcsRef>;
}

#[derive(Default)]
struct Commits {
    roots: Vec<VcsRoot>,
    /// Per root (same order as `roots`): hash -> index.
    indexes: Vec<HashMap<Hash, i32>>,
    /// Index - 1 -> (root position, hash).
    ids: Vec<(u32, Hash)>,
}

impl Commits {
    fn root_position(&mut self, root: &VcsRoot) -> usize {
        if let Some(pos) = self
            .roots
            .iter()
            .position(|r| std::sync::Arc::ptr_eq(r, root) || r == root)
        {
            return pos;
        }
        self.roots.push(root.clone());
        self.indexes.push(HashMap::new());
        self.roots.len() - 1
    }

    fn find(&self, hash: &Hash, root: &VcsRoot) -> Option<i32> {
        let pos = self
            .roots
            .iter()
            .position(|r| std::sync::Arc::ptr_eq(r, root) || r == root)?;
        self.indexes[pos].get(hash).copied()
    }

    fn enumerate(&mut self, pos: usize, hash: &Hash) -> i32 {
        if let Some(&index) = self.indexes[pos].get(hash) {
            return index;
        }
        self.ids.push((pos as u32, hash.clone()));
        let index = self.ids.len() as i32;
        self.indexes[pos].insert(hash.clone(), index);
        index
    }
}

#[derive(Default)]
struct Refs {
    indexes: HashMap<VcsRef, i32>,
    refs: Vec<VcsRef>,
}

/// The in-memory [`VcsLogStorage`] (`InMemoryStorage`). Thread-safe; share it in an `Arc`.
#[derive(Default)]
pub struct InMemoryStorage {
    commits: RwLock<Commits>,
    refs: RwLock<Refs>,
}

impl InMemoryStorage {
    pub fn new() -> InMemoryStorage {
        InMemoryStorage::default()
    }

    /// The number of interned commits.
    pub fn commits_count(&self) -> usize {
        self.commits
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .ids
            .len()
    }
}

impl VcsLogStorage for InMemoryStorage {
    fn get_commit_index(&self, hash: &Hash, root: &VcsRoot) -> VcsLogCommitStorageIndex {
        if let Some(index) = self
            .commits
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .find(hash, root)
        {
            return index;
        }
        let mut commits = self.commits.write().unwrap_or_else(|e| e.into_inner());
        let pos = commits.root_position(root);
        commits.enumerate(pos, hash)
    }

    fn get_commit_indexes(
        &self,
        hashes: &[&Hash],
        root: &VcsRoot,
    ) -> Vec<VcsLogCommitStorageIndex> {
        let mut commits = self.commits.write().unwrap_or_else(|e| e.into_inner());
        let pos = commits.root_position(root);
        hashes
            .iter()
            .map(|hash| commits.enumerate(pos, hash))
            .collect()
    }

    fn get_commit_id(&self, commit_index: VcsLogCommitStorageIndex) -> Option<CommitId> {
        let commits = self.commits.read().unwrap_or_else(|e| e.into_inner());
        let (pos, hash) = commits.ids.get((commit_index as usize).checked_sub(1)?)?;
        Some(CommitId {
            hash: hash.clone(),
            root: commits.roots[*pos as usize].clone(),
        })
    }

    fn contains_commit(&self, id: &CommitId) -> bool {
        self.commits
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .find(&id.hash, &id.root)
            .is_some()
    }

    fn get_ref_index(&self, vcs_ref: &VcsRef) -> i32 {
        let mut refs = self.refs.write().unwrap_or_else(|e| e.into_inner());
        if let Some(&index) = refs.indexes.get(vcs_ref) {
            return index;
        }
        refs.refs.push(vcs_ref.clone());
        let index = refs.refs.len() as i32;
        refs.indexes.insert(vcs_ref.clone(), index);
        index
    }

    fn get_vcs_ref(&self, ref_index: i32) -> Option<VcsRef> {
        let refs = self.refs.read().unwrap_or_else(|e| e.into_inner());
        refs.refs.get((ref_index as usize).checked_sub(1)?).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::root::vcs_root;

    #[test]
    fn indexes_are_dense_stable_and_per_root() {
        let storage = InMemoryStorage::new();
        let r1 = vcs_root("/r1");
        let r2 = vcs_root("/r2");
        let a = Hash::build("aa");
        assert_eq!(storage.get_commit_index(&a, &r1), 1);
        assert_eq!(storage.get_commit_index(&a, &r2), 2);
        assert_eq!(storage.get_commit_index(&a, &vcs_root("/r1")), 1);
        assert_eq!(
            storage.get_commit_indexes(&[&Hash::build("bb"), &a], &r1),
            [3, 1]
        );
        let id = storage.get_commit_id(2).unwrap();
        assert_eq!((id.hash, &*id.root), (a.clone(), &*r2));
        assert!(storage.get_commit_id(0).is_none());
        assert!(storage.get_commit_id(4).is_none());
        assert!(storage.contains_commit(&CommitId { hash: a, root: r1 }));
        assert_eq!(storage.commits_count(), 3);
    }
}
```

`crates/ig-log/src/data/vcs_ref.rs`:

```rust
//! References (branches, tags, HEAD) shown in the log.
//!
//! Ported from `platform/vcs-log/api/src/com/intellij/vcs/log/VcsRef.java`,
//! `platform/vcs-log/api/src/com/intellij/vcs/log/VcsRefType.java`,
//! `platform/vcs-log/api/src/com/intellij/vcs/log/VcsRefsContainer.kt` and
//! `platform/vcs-log/impl/src/com/intellij/vcs/log/impl/VcsRefImpl.java`.
//!
//! Deviations: a ref type carries no background color (a UI concern, to be mapped by the Log UI
//! from [`VcsRefType::name`]); ref names are not interned (`VcsRefNamesInterner`).

use std::fmt;

use crate::data::hash::Hash;
use crate::data::root::VcsRoot;

/// The type of a reference (`VcsRefType`). Types are compared by name and branch flag; upstream
/// compares the singleton instances by identity.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VcsRefType {
    name: &'static str,
    is_branch: bool,
}

impl VcsRefType {
    pub const fn new(name: &'static str, is_branch: bool) -> VcsRefType {
        VcsRefType { name, is_branch }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Whether this is a branch-like reference that defines a branch layout and color.
    pub fn is_branch(&self) -> bool {
        self.is_branch
    }
}

impl fmt::Display for VcsRefType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

/// A reference to a commit (`VcsRefImpl`): equal when hash, name, type and root are equal.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct VcsRef {
    commit_hash: Hash,
    name: String,
    ref_type: VcsRefType,
    root: VcsRoot,
}

impl VcsRef {
    pub fn new(commit_hash: Hash, name: &str, ref_type: VcsRefType, root: VcsRoot) -> VcsRef {
        VcsRef {
            commit_hash,
            name: name.to_string(),
            ref_type,
            root,
        }
    }

    pub fn commit_hash(&self) -> &Hash {
        &self.commit_hash
    }

    /// The short name, e.g. `main`, `origin/main`, `v1.0` or `HEAD`.
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ref_type(&self) -> VcsRefType {
        self.ref_type
    }

    pub fn root(&self) -> &VcsRoot {
        &self.root
    }
}

impl fmt::Debug for VcsRef {
    /// `VcsRefImpl.toString`: `root-name:name(hash|type)`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let root_name = self
            .root
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        write!(
            f,
            "{root_name}:{}({}|{})",
            self.name, self.commit_hash, self.ref_type
        )
    }
}

/// A collection of references split into branches and tags (`VcsRefsContainer`).
pub trait VcsRefsContainer {
    fn branches(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_>;

    fn tags(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_>;

    fn all_refs(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.branches().chain(self.tags()))
    }
}

/// A [`VcsRefsContainer`] over a list of refs: branch-typed refs are branches, all others tags
/// (`TestVcsRefsSequences` in upstream tests, `EmptyRefs` when empty).
#[derive(Clone, Debug, Default)]
pub struct VcsRefsList(pub Vec<VcsRef>);

impl VcsRefsContainer for VcsRefsList {
    fn branches(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.0.iter().filter(|r| r.ref_type().is_branch()))
    }

    fn tags(&self) -> Box<dyn Iterator<Item = &VcsRef> + '_> {
        Box::new(self.0.iter().filter(|r| !r.ref_type().is_branch()))
    }
}
```

`crates/ig-log/src/data/vcs_user.rs`:

```rust
//! Commit authors and committers.
//!
//! Ported from `platform/vcs-log/api/src/com/intellij/vcs/log/VcsUser.java`,
//! `platform/vcs-impl/shared/src/com/intellij/vcs/log/impl/VcsUserImpl.kt` and the parts of
//! `platform/vcs-impl/shared/src/com/intellij/vcs/log/util/VcsUserUtil.java` used by the log data.
//!
//! Deviation: users are not interned (`VcsUserRegistryImpl`); they are small owned values.

use std::fmt;

/// A user; equal when name and email are equal (`VcsUserImpl` is a data class).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VcsUser {
    name: String,
    email: String,
}

impl VcsUser {
    /// `VcsUserUtil.createUser`: the email is lowercased.
    pub fn create_user(name: &str, email: &str) -> VcsUser {
        VcsUser {
            name: name.to_string(),
            email: email.to_lowercase(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    /// `VcsUserUtil.toExactString`: `name <email>`, or whichever part is non-empty.
    pub fn to_exact_string(&self) -> String {
        if self.name.is_empty() {
            return self.email.clone();
        }
        if self.email.is_empty() {
            return self.name.clone();
        }
        format!("{} <{}>", self.name, self.email)
    }
}

impl fmt::Display for VcsUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_exact_string())
    }
}

impl fmt::Debug for VcsUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VcsUser({})", self.to_exact_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_is_lowercased_and_exact_string_drops_empty_parts() {
        let u = VcsUser::create_user("John Doe", "John.Doe@Example.com");
        assert_eq!(u.email(), "john.doe@example.com");
        assert_eq!(u.to_exact_string(), "John Doe <john.doe@example.com>");
        assert_eq!(VcsUser::create_user("", "a@b").to_exact_string(), "a@b");
        assert_eq!(VcsUser::create_user("n", "").to_exact_string(), "n");
    }
}
```

- [ ] **Step 2: Replace `crates/ig-log/src/lib.rs`**

`crates/ig-log/src/lib.rs`:

```rust
//! Commit graph core for IdeaGit: a port of IntelliJ's VCS Log graph
//! (`platform/vcs-log/graph` and `platform/vcs-log/graph-api`).
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.
//!
//! Layout mirrors upstream packages: the top-level modules are the public `graph-api`
//! (`com.intellij.vcs.log.graph`), [`api`] is the internal graph API
//! (`com.intellij.vcs.log.graph.api`), and [`permanent`], [`mod@print`], [`facade`], [`collapsing`]
//! and [`utils`] are the `impl` packages. Upstream names are kept in snake_case.
//!
//! Conventions: node indexes, node ids and counts are `i32`, like upstream, because not-loaded
//! commits have negative ids. Upstream exceptions are panics; upstream `assert` statements (disabled
//! in production) are `debug_assert!`s.
//!
//! Scope (Plan 2): building the permanent graph and showing it unfiltered with `SortType::Normal`.
//! BEK sorting, linear BEK, first-parent, branch and text filters and collapsing are Plan 2b.
//!
//! # Example
//!
//! ```
//! use std::collections::HashSet;
//!
//! use ig_log::print::graph_color_getter::GraphColorGetterByNodeFactory;
//! use ig_log::{GraphCommitImpl, Options, PermanentGraph, PermanentGraphImpl, VisibleGraph};
//!
//! // c (newest) -> b -> a
//! let commits = vec![
//!     GraphCommitImpl::create_commit("c".to_string(), vec!["b".to_string()], 3),
//!     GraphCommitImpl::create_commit("b".to_string(), vec!["a".to_string()], 2),
//!     GraphCommitImpl::create_commit("a".to_string(), vec![], 1),
//! ];
//! let factory = GraphColorGetterByNodeFactory::new(|_: &String, layout_index| layout_index);
//! let permanent = PermanentGraphImpl::new_instance(
//!     &commits,
//!     &factory,
//!     |a: &String, b: &String| a.cmp(b),
//!     &HashSet::new(),
//! );
//! let visible = permanent
//!     .create_visible_graph(Options::DEFAULT, None, None)
//!     .unwrap();
//!
//! let mut ids = Vec::new();
//! for row in 0..visible.visible_commit_count() {
//!     let info = visible.get_row_info(row);
//!     assert!(info.print_elements().len() >= 1);
//!     ids.push(info.commit());
//! }
//! assert_eq!(ids, ["c", "b", "a"]);
//! ```

pub mod actions;
pub mod api;
pub mod collapsing;
pub mod data;
pub mod facade;
pub mod graph_color_manager;
pub mod graph_commit;
pub mod permanent;
pub mod permanent_graph;
pub mod print;
pub mod print_element;
pub mod utils;
pub mod visible_graph;

pub use actions::{ActionController, GraphAction, GraphActionType, GraphAnswer};
pub use api::printer::GraphPrintElement;
pub use facade::permanent_graph_impl::PermanentGraphImpl;
pub use facade::visible_graph_impl::VisibleGraphImpl;
pub use graph_color_manager::GraphColorManager;
pub use graph_commit::{GraphCommit, GraphCommitImpl};
pub use permanent_graph::{Options, PermanentGraph, SortType, UnsupportedGraphOptions};
pub use print_element::{EdgePrintElement, NodePrintElement, PrintElement};
pub use visible_graph::{RowInfo, RowType, VisibleGraph};

// Compile-time guard for the UI threading model: the permanent graph is built on a background
// thread and shared, while the visible graph is owned by the UI thread (moved there once built).
const _: fn() = || {
    fn is_send<T: Send>() {}
    fn is_send_sync<T: Send + Sync>() {}
    is_send_sync::<PermanentGraphImpl<String>>();
    is_send::<VisibleGraphImpl<String>>();
    is_send::<GraphAnswer<String>>();
    is_send::<GraphPrintElement>();
};
```

- [ ] **Step 3: Test helpers.** Replace `crates/ig-log/tests/common/mod.rs` and create the two helpers:

`crates/ig-log/tests/common/mod.rs`:

```rust
//! Shared infrastructure of the ported graph tests.
//!
//! `load_text` / `do_test` are ported from
//! `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/AbstractTestWithTextFile.java` and
//! `AbstractTestWithTwoTextFile.java`; [`CommitIdManager`] from
//! `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/CommitIdManager.java`.
//! Test data lives in `tests/fixtures/<dir>` (byte-identical copies of `platform/vcs-log/graph/testData`).
#![allow(dead_code)] // each test crate uses a different subset

pub mod parser;
pub mod str_utils;
pub mod test_graph_builder;
pub mod test_vcs_log_provider;
pub mod timed_commit_parser;

use std::hash::Hash;
use std::path::PathBuf;

use parser::{SimpleCommit, create_hash, parse_integer_commit_list, parse_string_commit_list};

pub const IN_POSTFIX: &str = "_in.txt";
pub const OUT_POSTFIX: &str = "_out.txt";

/// `loadText`: the file with line separators converted to `\n` (`FileUtil.loadFile(file, true)`).
pub fn load_text(directory: &str, filename: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(directory)
        .join(filename);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// `doTest`: runs `run_test(in, out)` on `<test_name>_in.txt` and `<test_name>_out.txt`.
pub fn do_test(directory: &str, test_name: &str, run_test: impl FnOnce(&str, &str)) {
    let input = load_text(directory, &format!("{test_name}{IN_POSTFIX}"));
    let output = load_text(directory, &format!("{test_name}{OUT_POSTFIX}"));
    run_test(&input, &output);
}

/// How the commit ids of a test are parsed and printed (`CommitIdManager`).
pub trait CommitIdManager {
    type Id: Clone + Eq + Hash + Send + Sync + std::fmt::Debug + 'static;

    fn parse_commit_list(input: &str) -> Vec<SimpleCommit<Self::Id>>;

    fn to_str(commit: &Self::Id) -> String;

    fn to_int(commit: &Self::Id) -> i32;
}

/// `STRING_COMMIT_ID_MANAGER`
pub struct StringCommitIdManager;

impl CommitIdManager for StringCommitIdManager {
    type Id = String;

    fn parse_commit_list(input: &str) -> Vec<SimpleCommit<String>> {
        parse_string_commit_list(input)
    }

    fn to_str(commit: &String) -> String {
        commit.clone()
    }

    fn to_int(commit: &String) -> i32 {
        create_hash(commit)
    }
}

/// `INTEGER_COMMIT_ID_MANAGER`
pub struct IntegerCommitIdManager;

impl CommitIdManager for IntegerCommitIdManager {
    type Id = i32;

    fn parse_commit_list(input: &str) -> Vec<SimpleCommit<i32>> {
        parse_integer_commit_list(input)
    }

    /// `Integer.toHexString`.
    fn to_str(commit: &i32) -> String {
        format!("{:x}", *commit as u32)
    }

    fn to_int(commit: &i32) -> i32 {
        *commit
    }
}
```

`crates/ig-log/tests/common/timed_commit_parser.rs`:

```rust
//! Port of `platform/vcs-log/impl/test/com/intellij/vcs/log/TimedCommitParser.java`.

use ig_log::data::{Hash, TimedVcsCommit};

use super::parser::{create_hash, next_separator_index, parse_commit_parents};

/// `intToHash`: `Integer.toHexString`.
fn int_to_hash(index: i32) -> Hash {
    Hash::build(&format!("{:x}", index))
}

/// `1231423|-adada|-193 adf45`: timestamp, commit, parents (hex integers).
fn parse_timestamp_parent_hashes(line: &str) -> TimedVcsCommit {
    let first_separator_index = next_separator_index(line, 0);
    let timestamp_str = &line[..first_separator_index];
    let timestamp = if timestamp_str.is_empty() {
        0
    } else {
        timestamp_str
            .parse::<i64>()
            .unwrap_or_else(|_| panic!("bad timestamp in line: {line}"))
    };
    let (id, parents) = parse_commit_parents(&line[first_separator_index + 2..]);
    let parents = parents
        .into_iter()
        .filter(|p| !p.is_empty())
        .map(|p| int_to_hash(create_hash(p)))
        .collect();
    TimedVcsCommit::new(int_to_hash(create_hash(id)), parents, timestamp)
}

/// `TimedCommitParser.log`.
pub fn log<S: AsRef<str>>(commits: &[S]) -> Vec<TimedVcsCommit> {
    commits
        .iter()
        .map(|c| parse_timestamp_parent_hashes(c.as_ref()))
        .collect()
}
```

`crates/ig-log/tests/common/test_vcs_log_provider.rs`:

```rust
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
        let metadatas = commits[..requirements.commit_count]
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
```

- [ ] **Step 4: Build and run the unit tests**

```bash
cargo clippy -q --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ig-log
cargo test -p ig-log
```
Expected: clean. The `data` unit tests (hash, user, storage, color manager) pass, and all earlier targets still pass.

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "feat(ig-log): VCS log data layer (storage, refs, sorter, joiners, graph data, refresher)"
```

---

### Task 3: ig-log tests: sorter and multi-repo joiner

**Files:**
- Create: `crates/ig-log/tests/vcs_log_sorter.rs`, `crates/ig-log/tests/vcs_log_multi_repo_joiner.rs`

**Review focus:** `data/{sorter,multi_repo_joiner,commit,hash,root}.rs`.

Ports `VcsLogSorterTest` and `VcsLogMultiRepoJoinerTest`.

- [ ] **Step 1: Create the test file(s)**

`crates/ig-log/tests/vcs_log_sorter.rs`:

```rust
//! Port of `platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogSorterTest.java`.

mod common;

use common::timed_commit_parser::log;
use ig_log::GraphCommit;
use ig_log::data::{TimedVcsCommit, sort_by_date_topo_order};

fn to_str(commits: &[TimedVcsCommit]) -> String {
    commits
        .iter()
        .map(|c| c.id().as_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn do_test(started: Vec<TimedVcsCommit>, expected: Vec<TimedVcsCommit>) {
    let sorted = sort_by_date_topo_order(started);
    assert_eq!(to_str(&expected), to_str(&sorted));
}

#[test]
fn simple_test() {
    do_test(
        log(&["1|-a0|-", "3|-a1|-a0", "6|-a2|-a0"]),
        log(&["6|-a2|-a0", "3|-a1|-a0", "1|-a0|-"]),
    );
}

#[test]
fn several_heads() {
    do_test(
        log(&[
            "1|-a4|-",
            "2|-a3|-a4",
            "6|-b1|-b2",
            "4|-b2|-a3",
            "3|-a2|-a3",
            "5|-a1|-a2",
        ]),
        log(&[
            "6|-b1|-b2",
            "5|-a1|-a2",
            "4|-b2|-a3",
            "3|-a2|-a3",
            "2|-a3|-a4",
            "1|-a4|-",
        ]),
    );
}

#[test]
fn with_merge() {
    do_test(
        log(&[
            "6|-b1|-b2",
            "2|-a3|-a4",
            "3|-a2|-a3",
            "4|-b2|-a3",
            "5|-a1|-a2",
            "1|-a0|-b1 a1",
            "1|-a4|-",
        ]),
        log(&[
            "1|-a0|-b1 a1",
            "6|-b1|-b2",
            "5|-a1|-a2",
            "4|-b2|-a3",
            "3|-a2|-a3",
            "2|-a3|-a4",
            "1|-a4|-",
        ]),
    );
}

#[test]
fn several_branches() {
    do_test(
        log(&["1|-a1|-", "3|-a3|-a1", "2|-a2|-", "4|-a4|-a2"]),
        log(&["4|-a4|-a2", "3|-a3|-a1", "2|-a2|-", "1|-a1|-"]),
    );
}

/// Upstream: `@Test(timeout = 1000)`; the sort must terminate on a cycle.
#[test]
fn cyclic_graph() {
    let start = std::time::Instant::now();
    sort_by_date_topo_order(log(&["4|-a4|-a3", "3|-a3|-a2", "2|-a2|-a1", "1|-a1|-a4"]));
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
}
```

`crates/ig-log/tests/vcs_log_multi_repo_joiner.rs`:

```rust
//! Port of `platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogMultiRepoJoinerTest.java`.

mod common;

use common::timed_commit_parser::log;
use ig_log::GraphCommit;
use ig_log::data::multi_repo_joiner::join;

#[test]
fn join_test() {
    let first = log(&["6|-a2|-a0", "3|-a1|-a0", "1|-a0|-"]);
    let second = log(&["4|-b1|-b0", "2|-b0|-"]);
    let third = log(&["7|-c1|-c0", "5|-c0|-"]);

    let expected = log(&[
        "7|-c1|-c0",
        "6|-a2|-a0",
        "5|-c0|-",
        "4|-b1|-b0",
        "3|-a1|-a0",
        "2|-b0|-",
        "1|-a0|-",
    ]);

    let joined = join(vec![first, second, third]);

    // `TimedVcsCommitImpl.equals` compares hashes only; also compare the timestamps and parents
    assert_eq!(expected, joined);
    let full =
        |c: &ig_log::data::TimedVcsCommit| (c.id().clone(), c.parents().to_vec(), c.timestamp());
    assert_eq!(
        expected.iter().map(full).collect::<Vec<_>>(),
        joined.iter().map(full).collect::<Vec<_>>()
    );
}
```

- [ ] **Step 2: Run them**

```bash
cargo test -p ig-log --test vcs_log_sorter
cargo test -p ig-log --test vcs_log_multi_repo_joiner
```
Expected: `vcs_log_sorter`: 5 passed, `vcs_log_multi_repo_joiner`: 1 passed. Fix the source, never the expectations.

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "test(ig-log): sorter and multi-repo joiner"
```

---

### Task 4: ig-log tests: refresh joiner

**Files:**
- Create: `crates/ig-log/tests/vcs_log_joiner.rs`

**Review focus:** `data/joiner.rs`.

Ports `VcsLogJoinerTest`.

- [ ] **Step 1: Create the test file(s)**

`crates/ig-log/tests/vcs_log_joiner.rs`:

```rust
//! Port of `platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogJoinerTest.kt`.
//!
//! Generated from the upstream DSL: each test lists the saved log, the recent commits, the old
//! and new refs, and the expected ids (and, for some, ids with parents) of the joined log.

mod common;

use std::collections::HashSet;

use common::timed_commit_parser::log;
use ig_log::GraphCommit;
use ig_log::data::joiner::{ILLEGAL_DATA_RELOAD_ALL, add_commits};
use ig_log::data::{Hash, JoinError};

const BIG_TIME: i32 = 100000000;

fn lines(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

struct TestRunner {
    full_log: Vec<String>,
    recent_commits: Vec<String>,
    old_refs: Vec<String>,
    new_refs: Vec<String>,
    expected: Option<Vec<String>>,
    expected_with_parents: Option<Vec<String>>,
}

impl TestRunner {
    fn run(self) -> Result<(), JoinError> {
        let vcs_full_log = log(&self.full_log);
        let vcs_recent_commits = log(&self.recent_commits);
        let vcs_old_refs: HashSet<Hash> = self.old_refs.iter().map(|h| Hash::build(h)).collect();
        let vcs_new_refs: HashSet<Hash> = self.new_refs.iter().map(|h| Hash::build(h)).collect();

        let (result, _) = add_commits(
            &vcs_full_log,
            &vcs_old_refs,
            &vcs_recent_commits,
            &vcs_new_refs,
        )?;
        let actual: Vec<String> = result.iter().map(|c| c.id().as_string()).collect();
        assert_eq!(
            self.expected.expect("expected is set").join("\n"),
            actual.join("\n")
        );

        if let Some(expected_parents) = self.expected_with_parents {
            let actual_with_parents: Vec<String> = result
                .iter()
                .map(|commit| {
                    let parents: Vec<String> =
                        commit.parents().iter().map(Hash::as_string).collect();
                    if parents.is_empty() {
                        commit.id().as_string()
                    } else {
                        format!("{}|-{}", commit.id(), parents.join(" "))
                    }
                })
                .collect();
            assert_eq!(expected_parents.join("\n"), actual_with_parents.join("\n"));
        }
        Ok(())
    }
}

#[test]
fn simple() {
    let runner = TestRunner {
        full_log: lines(&["4|-a2|-a1", "3|-b1|-a", "2|-a1|-a", "1|-a|-"]),
        recent_commits: lines(&["5|-f|-b1", "6|-e|-a2"]),
        old_refs: lines(&["a2", "b1"]),
        new_refs: lines(&["f", "e"]),
        expected: Some(lines(&["e", "f", "a2", "b1", "a1", "a"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn one_node() {
    let runner = TestRunner {
        full_log: lines(&["3|-a1|-"]),
        recent_commits: lines(&["3|-a1|-"]),
        old_refs: lines(&["a1"]),
        new_refs: lines(&["a1"]),
        expected: Some(lines(&["a1"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn one_node_reset() {
    let runner = TestRunner {
        full_log: lines(&["3|-a1|-a2", "2|-a2|-"]),
        recent_commits: lines(&["2|-a2|-"]),
        old_refs: lines(&["a2", "a1"]),
        new_refs: lines(&["a2"]),
        expected: Some(lines(&["a2"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn one_node_reset2() {
    let runner = TestRunner {
        full_log: lines(&["3|-a1|-a2", "2|-a2|-"]),
        recent_commits: lines(&["2|-a2|-"]),
        old_refs: lines(&["a1"]),
        new_refs: lines(&["a2"]),
        expected: Some(lines(&["a2"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn simple_remove_commits() {
    let runner = TestRunner {
        full_log: lines(&["4|-a2|-a1", "3|-b1|-a", "2|-a1|-a", "1|-a|-"]),
        recent_commits: lines(&["5|-f|-b1", "6|-e|-a1"]),
        old_refs: lines(&["a2"]),
        new_refs: lines(&["f", "e"]),
        expected: Some(lines(&["e", "f", "b1", "a1", "a"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_commits() {
    let runner = TestRunner {
        full_log: lines(&[
            "5|-a5|-a4",
            "4|-a4|-a2 a3",
            "3|-a3|-a1",
            "2|-a2|-a1",
            "1|-a1|-",
        ]),
        recent_commits: lines(&["6|-a6|-a3"]),
        old_refs: lines(&["a5"]),
        new_refs: lines(&["a6"]),
        expected: Some(lines(&["a6", "a3", "a1"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_commits2() {
    let runner = TestRunner {
        full_log: lines(&["2|-a2|-a1", "1|-a1|-"]),
        recent_commits: lines(&["5|-a5|-a4", "3|-a3|-a2", "4|-a4|-a3"]),
        old_refs: lines(&["a2"]),
        new_refs: lines(&["a5"]),
        expected: Some(lines(&["a5", "a4", "a3", "a2", "a1"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_commits3() {
    let runner = TestRunner {
        full_log: lines(&["3|-a3|-a2", "2|-a2|-a1", "1|-a1|-"]),
        recent_commits: lines(&["2|-a2|-a1"]),
        old_refs: lines(&["a3"]),
        new_refs: lines(&["a2"]),
        expected: Some(lines(&["a2", "a1"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_old_branch() {
    let runner = TestRunner {
        full_log: std::iter::once("100|-e1|-e10".to_string())
            .chain((10..=100000).map(|it| format!("{}|-e{}|-e{}", BIG_TIME - it, it, it + 1)))
            .chain(std::iter::once("5|-e100001|-a1".to_string()))
            .chain(std::iter::once("4|-b2|-b1".to_string()))
            .chain(std::iter::once("3|-b1|-a1".to_string()))
            .chain(std::iter::once("1|-a1|-".to_string()))
            .collect::<Vec<String>>(),
        recent_commits: lines(&["100|-e1|-e10"]),
        old_refs: lines(&["e1", "b2"]),
        new_refs: lines(&["e1"]),
        expected: Some(
            std::iter::once("e1".to_string())
                .chain((10..=100000).map(|it| format!("e{it}")))
                .chain(std::iter::once("e100001".to_string()))
                .chain(std::iter::once("a1".to_string()))
                .collect::<Vec<String>>(),
        ),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn add_to_old_branch() {
    let runner = TestRunner {
        full_log: std::iter::once("100|-e1|-e10".to_string())
            .chain((10..=100000).map(|it| format!("{}|-e{}|-e{}", BIG_TIME - it, it, it + 1)))
            .chain(std::iter::once("5|-e100001|-a1".to_string()))
            .chain(std::iter::once("4|-b2|-b1".to_string()))
            .chain(std::iter::once("3|-b1|-a1".to_string()))
            .chain(std::iter::once("1|-a1|-".to_string()))
            .collect::<Vec<String>>(),
        recent_commits: lines(&["50|-b4|-b3", "49|-b3|-b2"]),
        old_refs: lines(&["e1", "b2"]),
        new_refs: lines(&["e1", "b4"]),
        expected: Some(
            std::iter::once("e1".to_string())
                .chain((10..=100000).map(|it| format!("e{it}")))
                .chain(std::iter::once("b4".to_string()))
                .chain(std::iter::once("b3".to_string()))
                .chain(std::iter::once("e100001".to_string()))
                .chain(std::iter::once("b2".to_string()))
                .chain(std::iter::once("b1".to_string()))
                .chain(std::iter::once("a1".to_string()))
                .collect::<Vec<String>>(),
        ),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_long_branch() {
    let runner = TestRunner {
        full_log: std::iter::once("100|-e1|-e10".to_string())
            .chain((10..=100000).map(|it| format!("{}|-e{}|-e{}", BIG_TIME - it, it, it + 1)))
            .chain(std::iter::once("5|-e100001|-a1".to_string()))
            .chain(std::iter::once("4|-b2|-b1".to_string()))
            .chain(std::iter::once("3|-b1|-a1".to_string()))
            .chain(std::iter::once("1|-a1|-".to_string()))
            .collect::<Vec<String>>(),
        recent_commits: lines(&["50|-b4|-b3", "49|-b3|-b2"]),
        old_refs: lines(&["e1", "b2"]),
        new_refs: lines(&["b4"]),
        expected: Some(lines(&["b4", "b3", "b2", "b1", "a1"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn not_enough_data_exception_test() {
    let runner = TestRunner {
        full_log: lines(&["1|-a1|-"]),
        recent_commits: lines(&["3|-a3|-a2"]),
        old_refs: lines(&["a1"]),
        new_refs: lines(&["a3"]),
        expected: None,
        expected_with_parents: None,
    };
    assert_eq!(runner.run().unwrap_err(), JoinError::NotEnoughData);
}

#[test]
fn illegal_state_exception_test() {
    let runner = TestRunner {
        full_log: lines(&["1|-a1|-"]),
        recent_commits: lines(&["1|-a1|-"]),
        old_refs: lines(&["a1", "a2"]),
        new_refs: lines(&["a1"]),
        expected: None,
        expected_with_parents: None,
    };
    assert_eq!(
        runner.run().unwrap_err(),
        JoinError::IllegalState(ILLEGAL_DATA_RELOAD_ALL)
    );
}

#[test]
fn illegal_state_exception_test2() {
    let runner = TestRunner {
        full_log: lines(&["2|-a2|-a1", "1|-a1|-"]),
        recent_commits: lines(&["3|-a3|-a1", "1|-a1|-"]),
        old_refs: lines(&[]),
        new_refs: lines(&["a3"]),
        expected: None,
        expected_with_parents: None,
    };
    assert_eq!(
        runner.run().unwrap_err(),
        JoinError::IllegalState(ILLEGAL_DATA_RELOAD_ALL)
    );
}

#[test]
fn remove_parallel_branch() {
    let runner = TestRunner {
        full_log: lines(&["4|-a4|-a1", "3|-a3|-a2", "2|-a2|-", "1|-a1|-"]),
        recent_commits: lines(&[]),
        old_refs: lines(&["a4", "a3"]),
        new_refs: lines(&["a3"]),
        expected: Some(lines(&["a3", "a2"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_commit_with_changed_parents() {
    let runner = TestRunner {
        full_log: lines(&["4|-c|-b d", "3|-d|-a", "2|-b|-a", "1|-a|-"]),
        recent_commits: lines(&["4|-c|-b", "2|-b|-a", "1|-a|-"]),
        old_refs: lines(&["c", "d"]),
        new_refs: lines(&["c"]),
        expected: Some(lines(&["c", "b", "a"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn remove_commit_with_changed_parents_no_ref() {
    let runner = TestRunner {
        full_log: lines(&["4|-c|-b d", "3|-d|-a", "2|-b|-a", "1|-a|-"]),
        recent_commits: lines(&["4|-c|-b"]),
        old_refs: lines(&["c"]),
        new_refs: lines(&["c"]),
        expected: Some(lines(&["c", "b", "a"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

#[test]
fn keep_shared_parent_despite_one_branch_dropping_it() {
    let runner = TestRunner {
        full_log: lines(&["5|-c|-b d", "4|-f|-d", "3|-d|-a", "2|-b|-a", "1|-a|-"]),
        recent_commits: lines(&["5|-c|-b", "4|-f|-d"]),
        old_refs: lines(&["c"]),
        new_refs: lines(&["c"]),
        expected: Some(lines(&["c", "f", "d", "b", "a"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

/// Scenario from IJPL-248963: commit C's parents changed from [B, D] to [B].
/// D had a ref of its own which is now gone. D should be removed (red).
#[test]
fn stale_parent_propagation() {
    let runner = TestRunner {
        full_log: lines(&["4|-c|-b d", "3|-b|-a", "2|-d|-a", "1|-a|-"]),
        recent_commits: lines(&["4|-c|-b"]),
        old_refs: lines(&["c", "d"]),
        new_refs: lines(&["c"]),
        expected: Some(lines(&["c", "b", "a"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

/// Verify that the returned commit C has updated parents from firstBlock,
/// not stale parents from savedLog.
#[test]
fn stale_parent_propagation_parents_updated() {
    let runner = TestRunner {
        full_log: lines(&["4|-c|-b d", "3|-b|-a", "2|-d|-a", "1|-a|-"]),
        recent_commits: lines(&["4|-c|-b"]),
        old_refs: lines(&["c", "d"]),
        new_refs: lines(&["c"]),
        expected: Some(lines(&["c", "b", "a"])),
        expected_with_parents: Some(lines(&[
            // C should have parent [b] only, not [b, d]
            "c|-b", "b|-a", "a",
        ])),
    };
    runner.run().unwrap();
}

/// Multiple commits have changed parents. Verify both are updated.
#[test]
fn stale_parent_propagation_multiple_changed_parents() {
    let runner = TestRunner {
        full_log: lines(&["5|-e|-c d", "4|-c|-b d", "3|-b|-a", "2|-d|-a", "1|-a|-"]),
        recent_commits: lines(&[
            // E now only has parent C, C now only has parent B (D removed from both)
            "5|-e|-c", "4|-c|-b",
        ]),
        old_refs: lines(&["e", "d"]),
        new_refs: lines(&["e"]),
        expected: Some(lines(&["e", "c", "b", "a"])),
        expected_with_parents: Some(lines(&["e|-c", "c|-b", "b|-a", "a"])),
    };
    runner.run().unwrap();
}

/// A→B→C becomes A→B'→C: commit A keeps the same hash but its parent
/// changed from B to B'. B is gone, B' is new.
/// A should connect to B' in the result, old B should be removed.
#[test]
fn parent_replaced_same_commit_hash() {
    let runner = TestRunner {
        full_log: lines(&["3|-a|-b", "2|-b|-c", "1|-c|-"]),
        recent_commits: lines(&["3|-a|-b2", "2|-b2|-c"]),
        old_refs: lines(&["a"]),
        new_refs: lines(&["a"]),
        expected: Some(lines(&["a", "b2", "c"])),
        expected_with_parents: Some(lines(&["a|-b2", "b2|-c", "c"])),
    };
    runner.run().unwrap();
}

/// Refreshing with the exact same data should not change the log.
#[test]
fn same_data_refresh() {
    let runner = TestRunner {
        full_log: lines(&["3|-a|-b", "2|-b|-c", "1|-c|-"]),
        recent_commits: lines(&["3|-a|-b", "2|-b|-c", "1|-c|-"]),
        old_refs: lines(&["a"]),
        new_refs: lines(&["a"]),
        expected: Some(lines(&["a", "b", "c"])),
        expected_with_parents: Some(lines(&["a|-b", "b|-c", "c"])),
    };
    runner.run().unwrap();
}

/// Same-data refresh with disconnected islands should not change the log.
#[test]
fn same_data_refresh_with_islands() {
    let runner = TestRunner {
        full_log: lines(&["3|-a|-b", "2|-b|-", "2|-e|-f", "1|-f|-"]),
        recent_commits: lines(&["3|-a|-b", "2|-b|-", "2|-e|-f", "1|-f|-"]),
        old_refs: lines(&["a", "e"]),
        new_refs: lines(&["a", "e"]),
        expected: Some(lines(&["a", "b", "e", "f"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

/// A new commit is added on top of an existing chain.
#[test]
fn new_commit_on_existing_chain() {
    let runner = TestRunner {
        full_log: lines(&["2|-b|-c", "1|-c|-"]),
        recent_commits: lines(&["3|-a|-b", "2|-b|-c"]),
        old_refs: lines(&["b"]),
        new_refs: lines(&["a"]),
        expected: Some(lines(&["a", "b", "c"])),
        expected_with_parents: Some(lines(&["a|-b", "b|-c", "c"])),
    };
    runner.run().unwrap();
}

/// E's parent changes from A to B (no ref change — E is still the only ref).
/// A becomes unreachable and should be removed from the log.
#[test]
fn dropped_parent_removed_when_no_ref_change() {
    let runner = TestRunner {
        full_log: lines(&["4|-e|-a", "3|-a|-b", "2|-b|-c", "1|-c|-"]),
        recent_commits: lines(&["4|-e|-b", "2|-b|-c", "1|-c|-"]),
        old_refs: lines(&["e"]),
        new_refs: lines(&["e"]),
        expected: Some(lines(&["e", "b", "c"])),
        expected_with_parents: Some(lines(&["e|-b", "b|-c", "c"])),
    };
    runner.run().unwrap();
}

/// E's parent changes from A to C, but A is still reachable through another ref F.
/// A should NOT be removed.
#[test]
fn dropped_parent_survives_if_reachable_elsewhere() {
    let runner = TestRunner {
        full_log: lines(&["4|-e|-a", "3|-a|-c", "3|-f|-a", "1|-c|-"]),
        recent_commits: lines(&["4|-e|-c", "3|-f|-a", "3|-a|-c", "1|-c|-"]),
        old_refs: lines(&["e", "f"]),
        new_refs: lines(&["e", "f"]),
        expected: Some(lines(&["e", "a", "f", "c"])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}

/// E's parent changes from A to nothing (E is re-fetched as an orphan/root commit).
/// A, B, C become unreachable and should all be removed.
#[test]
fn commit_becomes_orphan_drops_unreachable_parent() {
    let runner = TestRunner {
        full_log: lines(&["4|-e|-a", "3|-a|-b", "2|-b|-c", "1|-c|-"]),
        recent_commits: lines(&["4|-e|-"]),
        old_refs: lines(&["e"]),
        new_refs: lines(&["e"]),
        expected: Some(lines(&["e"])),
        expected_with_parents: Some(lines(&["e"])),
    };
    runner.run().unwrap();
}

#[test]
fn remove_all() {
    let runner = TestRunner {
        full_log: lines(&["4|-a4|-a1", "3|-a3|-a2", "2|-a2|-", "1|-a1|-"]),
        recent_commits: lines(&[]),
        old_refs: lines(&["a4", "a3"]),
        new_refs: lines(&[]),
        expected: Some(lines(&[])),
        expected_with_parents: None,
    };
    runner.run().unwrap();
}
```

- [ ] **Step 2: Run them**

```bash
cargo test -p ig-log --test vcs_log_joiner
```
Expected: `vcs_log_joiner`: 29 passed. Fix the source, never the expectations.

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "test(ig-log): refresh joiner"
```

---

### Task 5: ig-log tests: graph data and refresher

**Files:**
- Create: `crates/ig-log/tests/vcs_log_graph_data.rs`, `crates/ig-log/tests/vcs_log_refresher.rs`

**Review focus:** `data/{storage,refs_model,graph_color_manager_impl,graph_data,refresher,provider}.rs`.

Ports `VcsLogRefresherTest` (4/7) plus IdeaGit graph-data tests.

- [ ] **Step 1: Create the test file(s)**

`crates/ig-log/tests/vcs_log_graph_data.rs`:

```rust
//! IdeaGit tests for building the data pack: storage-index commit ids, branch head order
//! (`HeadCommitsComparator`), branch colors (`GraphColorManagerImpl`), the commit order check of
//! the refresher, and the node-id index of `PermanentCommitsInfoImpl`.

mod common;

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use common::test_vcs_log_provider::TestVcsLogProvider;
use common::timed_commit_parser::log;
use ig_log::api::permanent::PermanentCommitsInfo;
use ig_log::data::graph_color_manager_impl::{DEFAULT_COLOR, java_string_hash_code};
use ig_log::data::graph_data::build_data;
use ig_log::data::refresher::{check_commits, compact_commit};
use ig_log::data::{
    Hash, HeadCommitsComparator, InMemoryStorage, LogProviders, RootRefsModel, VcsLogProvider,
    VcsLogRefManager, VcsLogRefresher, VcsLogStorage, VcsRef, VcsRefType, vcs_root,
};
use ig_log::graph_color_manager::GraphColorManager;
use ig_log::permanent::permanent_commits_info_impl::PermanentCommitsInfoImpl;
use ig_log::{GraphCommitImpl, PermanentGraph};

const BRANCH: VcsRefType = VcsRefType::new("BRANCH", true);
const TAG: VcsRefType = VcsRefType::new("TAG", false);

/// Orders refs by name.
struct ByName;

impl VcsLogRefManager for ByName {
    fn branch_layout_compare(&self, a: &VcsRef, b: &VcsRef) -> Ordering {
        a.name().cmp(b.name())
    }
    fn labels_order_compare(&self, a: &VcsRef, b: &VcsRef) -> Ordering {
        a.name().cmp(b.name())
    }
    fn name(&self) -> String {
        "ByName".into()
    }
}

#[test]
fn heads_are_ordered_by_their_best_ref_and_colored_by_its_name() {
    let root = vcs_root("/r");
    let storage: Arc<dyn VcsLogStorage> = Arc::new(InMemoryStorage::new());
    // two branches from a1: b2 (refs "zoo" and "alpha") and c2 (ref "main"), d2 without refs
    let commits = log(&["5|-b2|-a1", "4|-c2|-a1", "3|-d2|-a1", "1|-a1|-"]);
    let graph_commits: Vec<_> = commits
        .iter()
        .map(|c| compact_commit(storage.as_ref(), c, &root))
        .collect();
    let r = |hash: &str, name: &str, t: VcsRefType| {
        VcsRef::new(Hash::build(hash), name, t, root.clone())
    };
    let refs = RootRefsModel::create(
        [
            r("b2", "zoo", BRANCH),
            r("b2", "alpha", BRANCH),
            r("c2", "main", BRANCH),
            r("a1", "v1", TAG),
        ],
        storage.as_ref(),
    );
    // only the reference manager of the provider is used
    let provider = Arc::new(TestVcsLogProvider::default());
    let mut providers = LogProviders::new();
    providers.insert(root.clone(), provider as Arc<dyn VcsLogProvider>);
    let mut pack = build_data(
        &graph_commits,
        BTreeMap::from([(root.clone(), Arc::new(refs))]),
        &providers,
        &storage,
        true,
    );
    assert_eq!(pack.commits_count(), 4);

    // the test provider's manager compares all refs equal: use a by-name manager instead
    let refs_model = pack.refs_model().clone();
    let managers = BTreeMap::from([(root.clone(), Arc::new(ByName) as Arc<dyn VcsLogRefManager>)]);
    let refs_model = Arc::new(ig_log::data::RefsModel::create(
        refs_model.refs_by_root().clone(),
        storage.clone(),
        managers.clone(),
    ));
    let index = |h: &str| storage.get_commit_index(&Hash::build(h), &root);
    assert_eq!(
        refs_model
            .get_ref_for_head_commit(index("b2"))
            .unwrap()
            .name(),
        "alpha"
    );
    assert_eq!(refs_model.refs_to_commit(index("a1")).len(), 1);
    assert!(refs_model.get_ref_for_head_commit(index("d2")).is_none());

    let comparator = HeadCommitsComparator::new(refs_model.clone(), managers);
    assert_eq!(
        comparator.compare(&index("b2"), &index("c2")),
        Ordering::Less,
        "alpha < main"
    );
    assert_eq!(
        comparator.compare(&index("d2"), &index("c2")),
        Ordering::Greater,
        "no refs last"
    );
    assert_eq!(
        comparator.compare(&index("c2"), &index("c2")),
        Ordering::Equal
    );

    let colors = ig_log::data::GraphColorManagerImpl::new(refs_model);
    assert_eq!(
        colors.get_color(&index("b2"), 1, 1),
        java_string_hash_code("alpha")
    );
    assert_eq!(colors.get_color(&index("b2"), 1, 7), 7);
    assert_eq!(colors.get_color(&index("d2"), 2, 2), DEFAULT_COLOR);

    // branch heads are the branch refs' commits (tags excluded)
    let graph = pack.permanent_graph().unwrap();
    let branches = graph.get_containing_branches(&index("a1"));
    assert_eq!(branches, HashSet::from([index("b2"), index("c2")]));
    pack = build_data(&[], BTreeMap::new(), &providers, &storage, false);
    assert!(pack.permanent_graph().is_none() && !pack.is_full());
}

#[test]
fn commit_order_check() {
    let c = |id: i32, parents: &[i32]| GraphCommitImpl::create_commit(id, parents.to_vec(), 0);
    assert!(check_commits(&[c(3, &[2]), c(2, &[1]), c(1, &[])]).is_ok());
    assert!(
        check_commits(&[c(3, &[9]), c(2, &[])]).is_ok(),
        "parents outside the list are not loaded"
    );
    let parent_first = check_commits(&[c(1, &[]), c(2, &[1])]).unwrap_err();
    assert!(parent_first.contains("topological"), "{parent_first}");
    let duplicate = check_commits(&[c(2, &[1]), c(2, &[1]), c(1, &[])]).unwrap_err();
    assert!(duplicate.contains("Duplicate"), "{duplicate}");
}

/// A provider returning a parent before its child gives an error data pack, not a wrong graph.
#[test]
fn refresher_rejects_non_topological_full_log() {
    let root = vcs_root("/r");
    let provider = Arc::new(TestVcsLogProvider::default());
    provider.append_history(log(&["1|-a|-", "2|-b|-a"]));
    let mut providers = LogProviders::new();
    providers.insert(root.clone(), provider as Arc<dyn VcsLogProvider>);
    let mut refresher = VcsLogRefresher::new(Arc::new(InMemoryStorage::new()), providers, 1);
    let pack = refresher.initialize();
    let error = pack.load_error().expect("error data pack");
    assert!(error.0.contains("topological"), "{error}");
}

#[test]
fn node_ids_are_indexed_including_duplicates_and_not_loaded_commits() {
    let commits = vec![
        GraphCommitImpl::create_commit("c".to_string(), vec!["b".to_string()], 3),
        GraphCommitImpl::create_commit("b".to_string(), vec!["a".to_string()], 2),
        GraphCommitImpl::create_commit("c".to_string(), vec![], 1),
    ];
    let info = PermanentCommitsInfoImpl::new_instance(
        &commits,
        [(-2, "a".to_string())].into_iter().collect(),
    );
    assert_eq!(
        info.get_node_id(&"c".to_string()),
        0,
        "the first row of a duplicate id"
    );
    assert_eq!(info.get_node_id(&"b".to_string()), 1);
    assert_eq!(info.get_node_id(&"a".to_string()), -2);
    assert_eq!(info.get_node_id(&"x".to_string()), -1);
    let ids: HashSet<String> = ["c", "a", "x"].map(String::from).into_iter().collect();
    assert_eq!(info.convert_to_node_ids(&ids), HashSet::from([0, 2, -2]));
    assert_eq!(
        info.convert_to_node_ids_skipping(&ids, true),
        HashSet::from([0, 2])
    );
    assert!(info.contains_all(&["c".to_string(), "b".to_string()]));
    assert!(
        !info.contains_all(&["a".to_string()]),
        "not-loaded commits are not contained"
    );
}
```

`crates/ig-log/tests/vcs_log_refresher.rs`:

```rust
//! Port of `platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogRefresherTest.kt`.
//!
//! The refresher is synchronous: data packs are published in order on the calling thread, so the
//! upstream cases about blocking, cancellation and batching of queued requests
//! (`test_first_refresh_waits_for_full_log`, `test_reinitialize_makes_refresh_cancelled`,
//! `test_two_immediately_consecutive_refreshes_causes_only_one_data_pack_update`) don't apply.

mod common;

use std::sync::{Arc, Mutex};

use common::test_vcs_log_provider::{BRANCH_TYPE, TestVcsLogProvider};
use common::timed_commit_parser::log;
use ig_log::data::{
    Hash, InMemoryStorage, LogProviders, TimedVcsCommit, VcsLogGraphData, VcsLogProvider,
    VcsLogRefresher, VcsLogStorage, VcsRef, VcsRoot, vcs_root,
};
use ig_log::{GraphCommit, PermanentGraph};

const RECENT_COMMITS_COUNT: usize = 2;

struct Fixture {
    project_root: VcsRoot,
    log_provider: Arc<TestVcsLogProvider>,
    storage: Arc<InMemoryStorage>,
    loader: VcsLogRefresher,
    data_packs: Arc<Mutex<Vec<Arc<VcsLogGraphData>>>>,
    commits: Vec<&'static str>,
}

fn set_up() -> Fixture {
    let project_root = vcs_root("/project");
    let log_provider = Arc::new(TestVcsLogProvider::default());
    let commits = vec!["3|-a2|-a1", "2|-a1|-a", "1|-a|-"];
    log_provider.append_history(log(&commits));
    log_provider.add_ref(create_branch_ref(&project_root, "master", "a2"));
    let storage = Arc::new(InMemoryStorage::new());
    let mut providers = LogProviders::new();
    providers.insert(
        project_root.clone(),
        log_provider.clone() as Arc<dyn VcsLogProvider>,
    );
    let mut loader = VcsLogRefresher::new(storage.clone(), providers, RECENT_COMMITS_COUNT);
    let data_packs = Arc::new(Mutex::new(Vec::new()));
    let sink = data_packs.clone();
    loader.set_data_pack_handler(move |pack| sink.lock().unwrap().push(pack.clone()));
    Fixture {
        project_root,
        log_provider,
        storage,
        loader,
        data_packs,
        commits,
    }
}

fn create_branch_ref(root: &VcsRoot, name: &str, commit: &str) -> VcsRef {
    VcsRef::new(Hash::build(commit), name, BRANCH_TYPE, root.clone())
}

impl Fixture {
    /// `dataWaiter.get()`: the next published data pack.
    fn next_data_pack(&self, index: usize) -> Arc<VcsLogGraphData> {
        self.data_packs.lock().unwrap()[index].clone()
    }

    fn published_count(&self) -> usize {
        self.data_packs.lock().unwrap().len()
    }

    /// `initAndWaitForFirstRefresh`.
    fn init_and_wait_for_first_refresh(&mut self) {
        self.loader.initialize();
        assert_eq!(self.published_count(), 2);
        assert!(!self.next_data_pack(0).is_full());
        assert!(self.next_data_pack(1).is_full());
    }

    fn assert_data_pack(&self, expected_log: &[TimedVcsCommit], data_pack: &VcsLogGraphData) {
        let storage: &dyn VcsLogStorage = self.storage.as_ref();
        let get_hash = |index: &i32| storage.get_commit_id(*index).unwrap().hash;
        let actual: Vec<(Hash, Vec<Hash>, i64)> = data_pack
            .permanent_graph()
            .unwrap()
            .all_commits()
            .iter()
            .map(|c| {
                (
                    get_hash(c.id()),
                    c.parents().iter().map(get_hash).collect(),
                    c.timestamp(),
                )
            })
            .collect();
        let expected: Vec<(Hash, Vec<Hash>, i64)> = expected_log
            .iter()
            .map(|c| (c.id().clone(), c.parents().to_vec(), c.timestamp()))
            .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
fn test_initialize_shows_short_history() {
    let mut f = set_up();
    f.loader.initialize();
    let result = f.next_data_pack(0);
    f.assert_data_pack(&log(&f.commits[..RECENT_COMMITS_COUNT]), &result);
    assert!(!result.is_full());
}

#[test]
fn test_first_refresh_reports_full_history() {
    let mut f = set_up();
    f.loader.initialize();

    let first_data_pack = f.next_data_pack(0);
    f.assert_data_pack(&log(&f.commits[..RECENT_COMMITS_COUNT]), &first_data_pack);

    let full_data_pack = f.next_data_pack(1);
    f.assert_data_pack(&log(&f.commits), &full_data_pack);
    assert_eq!(f.published_count(), 2);
}

#[test]
fn test_refresh_captures_new_commits() {
    let mut f = set_up();
    f.init_and_wait_for_first_refresh();

    let new_commit = "4|-a3|-a2";
    f.log_provider.append_history(log(&[new_commit]));
    let root = f.project_root.clone();
    f.loader.refresh(&[root]);
    let result = f.next_data_pack(2);

    let mut all_commits = vec![new_commit];
    all_commits.extend(f.commits.iter().copied());
    f.assert_data_pack(&log(&all_commits), &result);
}

#[test]
fn test_single_refresh_causes_single_data_read() {
    let mut f = set_up();
    f.init_and_wait_for_first_refresh();

    f.log_provider.reset_read_first_block_counter();
    let root = f.project_root.clone();
    f.loader.refresh(&[root]);
    assert_eq!(f.published_count(), 3);
    assert_eq!(
        1,
        f.log_provider.get_read_first_block_counter(),
        "Unexpected first block read count"
    );
}

/// IdeaGit: a refresh with no roots publishes nothing; a refresh for an unknown root gives an
/// error data pack, and the next refresh reloads the full log.
#[test]
fn refresh_edge_cases() {
    let mut f = set_up();
    f.init_and_wait_for_first_refresh();
    f.loader.refresh(&[]);
    assert_eq!(f.published_count(), 2);

    let error = f.loader.refresh(&[vcs_root("/unknown")]);
    assert!(error.load_error().is_some());
    assert!(!error.is_full());
    let root = f.project_root.clone();
    let reloaded = f.loader.refresh(&[root]);
    f.assert_data_pack(&log(&f.commits), &reloaded);
    assert!(reloaded.is_full());
}
```

- [ ] **Step 2: Run them**

```bash
cargo test -p ig-log --test vcs_log_graph_data
cargo test -p ig-log --test vcs_log_refresher
```
Expected: `vcs_log_graph_data`: 4 passed, `vcs_log_refresher`: 5 passed. Fix the source, never the expectations.

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "test(ig-log): graph data and refresher"
```

---

### Task 6: ig-git: runner, handler and version additions; natural comparator

**Files:**
- Modify: `crates/ig-git/src/{runner,handler,version,lib}.rs`
- Create: `crates/ig-git/src/natural_comparator.rs`

**Review focus:** `runner.rs`/`handler.rs` additions (no behavior change for existing callers), `natural_comparator.rs` vs upstream `NaturalComparator`.

These are the runner capabilities the log reader needs: streaming output to a line listener without collecting it, stdin input, and config parameters on handlers. They also add two version specialties and upstream's `NaturalComparator`, the ref-name ordering.

- [ ] **Step 1: Replace or create these files**

`crates/ig-git/src/runner.rs`:

```rust
//! Port of the execution parts of `plugins/git4idea/backend/src/commands/GitImplBase.java`
//! (environment, write locking, console logging, output collection) and `GitCommandResult.java`.
//!
//! Commands run synchronously on the calling thread; callers (UI) run them on a background executor.
//!
//! Known gaps, scheduled with cancellation in the fetch/push plan: a grandchild process that keeps
//! a pipe open (ssh ControlPersist, backgrounded hooks) delays completion until it exits, and the
//! write lock cannot be interrupted while waiting.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use crate::command::{GitCommand, LockingPolicy};
use crate::console::{ConsoleEvent, ConsoleSink, OutputKind};
use crate::error::{GitError, Result};
use crate::handler::GitHandler;
use crate::version::{GitVersion, GitVersionSpecialty};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommandResult {
    pub start_failed: bool,
    pub exit_code: i32,
    /// Stdout lines, separators trimmed.
    pub output: Vec<String>,
    /// Stderr lines, separators trimmed, progress lines removed.
    pub error_output: Vec<String>,
}

impl GitCommandResult {
    pub fn success(&self) -> bool {
        self.success_ignoring(&[])
    }

    /// `ignored_error_codes` are non-zero exit codes that count as success.
    pub fn success_ignoring(&self, ignored_error_codes: &[i32]) -> bool {
        !self.start_failed && (self.exit_code == 0 || ignored_error_codes.contains(&self.exit_code))
    }

    pub fn output_as_joined_string(&self) -> String {
        self.output.join("\n")
    }

    /// Upstream `getErrorOrStdOutput`: stderr, else stdout on failure, else a generic exit message.
    fn error_or_std_output(&self) -> Vec<String> {
        if !self.error_output.is_empty() {
            return self.error_output.clone();
        }
        if self.success() {
            return Vec::new();
        }
        if !self.output.is_empty() {
            return self.output.clone();
        }
        vec![format!(
            "The Git process exited with the code {}",
            self.exit_code
        )]
    }

    /// Upstream `getPresentableErrorOutput`: `error_or_std_output` without `@...@` banner
    /// lines and without `fatal:` / `error:` prefixes.
    pub fn presentable_error_output(&self) -> Vec<String> {
        self.error_or_std_output()
            .into_iter()
            .filter(|line| !is_git_error_header_line(line))
            .map(|line| cleanup_error_prefixes(&line))
            .collect()
    }

    pub fn error_output_as_joined_string(&self) -> String {
        self.presentable_error_output().join("\n")
    }

    /// Upstream `getOutputOrThrow`.
    pub fn output_or_error(&self) -> Result<String> {
        self.throw_on_error()?;
        Ok(self.output_as_joined_string())
    }

    /// Upstream `throwOnError`.
    pub fn throw_on_error(&self) -> Result<()> {
        if self.success() {
            Ok(())
        } else {
            Err(GitError::Command(self.error_output_as_joined_string()))
        }
    }
}

/// Upstream `GitCommandResult.isGitErrorHeaderLine`: lines of ASCII-art banners (e.g. ssh warnings).
fn is_git_error_header_line(line: &str) -> bool {
    line.starts_with('@') && line.ends_with('@')
}

/// Upstream `GitUtil.cleanupErrorPrefixes`.
pub fn cleanup_error_prefixes(msg: &str) -> String {
    let mut msg = msg.trim();
    for prefix in ["fatal:", "error:"] {
        if let Some(rest) = msg.strip_prefix(prefix) {
            msg = rest.trim();
        }
    }
    msg.to_string()
}

/// Exit code as the JVM reports it: 128 + signal number for processes killed by a signal.
fn exit_code_of(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    status.code().unwrap_or(-1)
}

/// Receives each output line with its kind (upstream `GitLineHandlerListener.onLineAvailable`).
pub type LineListener<'a> = dyn FnMut(&str, OutputKind) + 'a;

/// Runs git commands for one project. Holds the per-project command lock (upstream
/// `GitVcs.getCommandLock`) that serializes write commands.
pub struct Git {
    executable: PathBuf,
    version: GitVersion,
    console: Arc<dyn ConsoleSink>,
    command_lock: Mutex<()>,
    next_id: AtomicU64,
    base_env: Vec<(String, String)>,
    base_env_removed: Vec<String>,
}

impl Git {
    pub fn new(
        executable: impl Into<PathBuf>,
        version: GitVersion,
        console: Arc<dyn ConsoleSink>,
    ) -> Self {
        Self {
            executable: executable.into(),
            version,
            console,
            command_lock: Mutex::new(()),
            next_id: AtomicU64::new(1),
            base_env: Vec::new(),
            base_env_removed: Vec::new(),
        }
    }

    /// Environment applied to every handler created by [`Git::handler`] (on top of the inherited
    /// process environment). Used by tests to isolate git from the developer's configuration.
    pub fn with_base_environment(mut self, set: &[(&str, &str)], remove: &[&str]) -> Self {
        self.base_env = set
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self.base_env_removed = remove.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Runs `git version` with `executable` and returns a runner configured with the detected version.
    pub fn detect(executable: impl Into<PathBuf>, console: Arc<dyn ConsoleSink>) -> Result<Git> {
        let executable = executable.into();
        let probe = Git::new(executable.clone(), GitVersion::NULL, console.clone());
        let cwd = std::env::temp_dir();
        let result = probe.run_command(probe.handler(&cwd, GitCommand::VERSION));
        let output = result.output_or_error()?;
        let version = GitVersion::parse(&output)?;
        Ok(Git::new(executable, version, console))
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn version(&self) -> GitVersion {
        self.version
    }

    /// Creates a handler for this executable.
    pub fn handler(&self, working_dir: &Path, command: GitCommand) -> GitHandler {
        self.handler_with_config(working_dir, command, &[])
    }

    /// Creates a handler with extra `-c` config parameters (upstream
    /// `GitLineHandler(project, root, command, configParameters)`).
    pub fn handler_with_config(
        &self,
        working_dir: &Path,
        command: GitCommand,
        config_parameters: &[&str],
    ) -> GitHandler {
        let mut handler =
            GitHandler::with_config(working_dir, &self.executable, command, config_parameters);
        for (k, v) in &self.base_env {
            handler.add_custom_environment_variable(k, v);
        }
        for name in &self.base_env_removed {
            handler.remove_inherited_environment_variable(name);
        }
        handler
    }

    /// Upstream `GitImplBase.doRun`.
    pub fn run_command(&self, handler: GitHandler) -> GitCommandResult {
        self.run_with(handler, None, true)
    }

    /// Upstream `runCommandWithoutCollectingOutput`: every stdout and stderr line goes to
    /// `listener` as it arrives (upstream `GitLineHandlerListener.onLineAvailable`); only stderr is
    /// collected in the result.
    pub fn run_command_without_collecting_output(
        &self,
        handler: GitHandler,
        listener: &mut dyn FnMut(&str, OutputKind),
    ) -> GitCommandResult {
        self.run_with(handler, Some(listener), false)
    }

    fn run_with(
        &self,
        mut handler: GitHandler,
        listener: Option<&mut LineListener<'_>>,
        collect_stdout: bool,
    ) -> GitCommandResult {
        for (k, v) in trace_environment(&self.version) {
            handler.add_custom_environment_variable(k, v);
        }
        let can_suppress_optional_locks =
            GitVersionSpecialty::EnvGitOptionalLocksAllowed.exists_in(&self.version);
        if can_suppress_optional_locks {
            handler.add_custom_environment_variable("GIT_OPTIONAL_LOCKS", "0");
        }
        let _guard = if should_take_write_lock(handler.command(), can_suppress_optional_locks) {
            Some(self.command_lock.lock().unwrap_or_else(|e| e.into_inner()))
        } else {
            None
        };
        self.execute(&handler, listener, collect_stdout)
    }

    fn execute(
        &self,
        handler: &GitHandler,
        mut listener: Option<&mut LineListener<'_>>,
        collect_stdout: bool,
    ) -> GitCommandResult {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let log_to_console = !handler.is_silent();

        let mut cmd = handler.build_command();
        let stdin = if handler.input().is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        };
        cmd.stdin(stdin)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                return GitCommandResult {
                    start_failed: true,
                    exit_code: -1,
                    output: Vec::new(),
                    error_output: vec![format!("Failed to start Git process: {e}")],
                };
            }
        };
        if log_to_console {
            self.console.event(ConsoleEvent::Started {
                id,
                working_dir: handler.working_dir().to_path_buf(),
                command_line: handler.printable_command_line(),
            });
        }

        // Upstream `GitHandlerInputProcessorUtil.writeLines`: the input is written, then stdin closed.
        let in_thread = match (child.stdin.take(), handler.input()) {
            (Some(mut stdin), Some(input)) => {
                let input = input.to_vec();
                Some(thread::spawn(move || {
                    use std::io::Write;
                    let _ = stdin.write_all(&input);
                }))
            }
            _ => None,
        };

        let (tx, rx) = mpsc::channel::<(OutputKind, String)>();
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let out_thread = spawn_reader(stdout, OutputKind::Stdout, tx.clone());
        let err_thread = spawn_reader(stderr, OutputKind::Stderr, tx);

        let mut output = Vec::new();
        let mut error_output = Vec::new();
        for (kind, line) in rx {
            if log_to_console && should_log_line(handler, kind, &line) {
                self.console.event(ConsoleEvent::Output {
                    id,
                    kind,
                    line: line.clone(),
                });
            }
            if let Some(listener) = listener.as_mut() {
                listener(&line, kind);
            }
            match kind {
                OutputKind::Stdout => {
                    if collect_stdout {
                        output.push(line);
                    }
                }
                OutputKind::Stderr => {
                    if !looks_like_progress(&line) {
                        error_output.push(line);
                    }
                }
            }
        }
        let _ = out_thread.join();
        let _ = err_thread.join();
        if let Some(in_thread) = in_thread {
            let _ = in_thread.join();
        }
        let exit_code = match child.wait() {
            Ok(status) => exit_code_of(status),
            Err(_) => -1,
        };
        if log_to_console {
            self.console.event(ConsoleEvent::Finished { id, exit_code });
        }
        GitCommandResult {
            start_failed: false,
            exit_code,
            output,
            error_output,
        }
    }
}

fn should_take_write_lock(command: GitCommand, can_suppress_optional_locks: bool) -> bool {
    let policy = command.locking_policy();
    policy != LockingPolicy::Read
        && (policy != LockingPolicy::ReadOptionalLocking || !can_suppress_optional_locks)
}

/// Upstream `getGitTraceEnvironmentVariables` with tracing disabled (registry `git.execution.trace` = 0).
fn trace_environment(version: &GitVersion) -> Vec<(&'static str, &'static str)> {
    let mut env = vec![("GIT_TRACE", "0")];
    if GitVersionSpecialty::EnvGitTracePackAccessAllowed.exists_in(version) {
        env.push(("GIT_TRACE_PACK_ACCESS", ""));
    }
    env.push(("GIT_TRACE_PACKET", ""));
    env.push(("GIT_TRACE_PERFORMANCE", "0"));
    env.push(("GIT_TRACE_SETUP", "0"));
    env
}

/// Upstream `GitCommandOutputLogger.onLineAvailable` filters.
fn should_log_line(handler: &GitHandler, kind: OutputKind, line: &str) -> bool {
    if line.trim().is_empty() {
        return false;
    }
    match kind {
        OutputKind::Stdout => !handler.is_stdout_suppressed(),
        OutputKind::Stderr => !handler.is_stderr_suppressed(),
    }
}

const REMOTE_PROGRESS_PREFIX: &str = "remote: ";
const SUPPRESSED_PROGRESS_INDICATORS: &[&str] = &[
    "Counting objects: ",
    "Enumerating objects: ",
    "Compressing objects: ",
    "Writing objects: ",
    "Receiving objects: ",
    "Resolving deltas: ",
    "Finding sources: ",
    "Updating files: ",
    "Checking out files: ",
    "Expanding reachable commits in commit graph: ",
    "Delta compression using up to ",
];

/// Port of `GitImplBase.looksLikeProgress` (`PROGRESS_PATTERN` = `.*:\s*\d{1,3}% \(\d+/\d+\).*`).
pub fn looks_like_progress(line: &str) -> bool {
    use std::sync::LazyLock;
    static PROGRESS: LazyLock<regex::Regex> = LazyLock::new(|| {
        regex::Regex::new(r"^.*:\s*\d{1,3}% \(\d+/\d+\).*$").expect("valid regex")
    });
    if PROGRESS.is_match(line) {
        return true;
    }
    SUPPRESSED_PROGRESS_INDICATORS.iter().any(|prefix| {
        line.starts_with(prefix)
            || line
                .strip_prefix(REMOTE_PROGRESS_PREFIX)
                .is_some_and(|rest| rest.starts_with(prefix))
    })
}

/// Splits a byte stream into lines on `\n`, `\r\n` and `\r` (upstream `BufferingTextSplitter`).
#[derive(Default)]
pub(crate) struct LineSplitter {
    current: Vec<u8>,
    pending_cr: bool,
}

impl LineSplitter {
    pub(crate) fn feed(&mut self, bytes: &[u8], emit: &mut dyn FnMut(String)) {
        for &b in bytes {
            if self.pending_cr {
                self.pending_cr = false;
                if b == b'\n' {
                    continue; // CRLF: the line was already emitted at CR
                }
            }
            match b {
                b'\n' => emit(self.take_line()),
                b'\r' => {
                    emit(self.take_line());
                    self.pending_cr = true;
                }
                _ => self.current.push(b),
            }
        }
    }

    pub(crate) fn finish(&mut self, emit: &mut dyn FnMut(String)) {
        if !self.current.is_empty() {
            emit(self.take_line());
        }
    }

    fn take_line(&mut self) -> String {
        let line = String::from_utf8_lossy(&self.current).into_owned();
        self.current.clear();
        line
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    kind: OutputKind,
    tx: mpsc::Sender<(OutputKind, String)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut splitter = LineSplitter::default();
        let mut buf = [0u8; 8192];
        let mut emit = |line: String| {
            let _ = tx.send((kind, line));
        };
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => splitter.feed(&buf[..n], &mut emit),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        splitter.finish(&mut emit);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(chunks: &[&[u8]]) -> Vec<String> {
        let mut s = LineSplitter::default();
        let mut lines = Vec::new();
        for c in chunks {
            s.feed(c, &mut |l| lines.push(l));
        }
        s.finish(&mut |l| lines.push(l));
        lines
    }

    #[test]
    fn splits_lf_crlf_cr_across_chunks() {
        assert_eq!(split(&[b"a\nb\r\nc\rd"]), ["a", "b", "c", "d"]);
        assert_eq!(split(&[b"a\r", b"\nb\n"]), ["a", "b"]);
        assert_eq!(split(&[b"\n\n"]), ["", ""]);
        assert_eq!(split(&[b"a\r", b"b"]), ["a", "b"]);
        assert_eq!(split(&[b"a\r"]), ["a"]);
        assert_eq!(split(&[b"a\r\rb"]), ["a", "", "b"]);
    }

    #[test]
    fn presentable_errors_are_cleaned_up() {
        let r = GitCommandResult {
            start_failed: false,
            exit_code: 128,
            output: vec![],
            error_output: vec![
                "@@@@@@@@@@@@".into(),
                "@    WARNING: UNPROTECTED PRIVATE KEY FILE!    @".into(),
                "fatal: not a git repository".into(),
                "  error: bad thing ".into(),
            ],
        };
        assert_eq!(
            r.presentable_error_output(),
            ["not a git repository", "bad thing"]
        );
        assert_eq!(cleanup_error_prefixes("fatal: error: x"), "x");
    }

    #[test]
    fn progress_detection() {
        assert!(looks_like_progress(
            "remote: Finding sources:   1% (575/57489)   "
        ));
        assert!(looks_like_progress("Receiving objects:  45% (450/1000)"));
        assert!(looks_like_progress(
            "remote: Counting objects: 198285, done"
        ));
        assert!(looks_like_progress(
            "Expanding reachable commits in commit graph: 95907"
        ));
        assert!(looks_like_progress(
            "Receiving objects: 100% (57489/57489), 50.03 MiB | 2.83 MiB/s, done."
        ));
        assert!(!looks_like_progress("fatal: not a git repository"));
    }

    #[test]
    fn write_lock_rules() {
        assert!(should_take_write_lock(GitCommand::COMMIT, true));
        assert!(!should_take_write_lock(GitCommand::LOG, false));
        assert!(!should_take_write_lock(GitCommand::STATUS, true));
        assert!(should_take_write_lock(GitCommand::STATUS, false));
    }

    #[test]
    fn result_error_presentation() {
        let failed_silently = GitCommandResult {
            start_failed: false,
            exit_code: 128,
            output: vec![],
            error_output: vec![],
        };
        assert_eq!(
            failed_silently.error_output_as_joined_string(),
            "The Git process exited with the code 128"
        );
        assert!(failed_silently.output_or_error().is_err());
        let ok = GitCommandResult {
            exit_code: 0,
            output: vec!["x".into()],
            ..failed_silently
        };
        assert_eq!(ok.output_or_error().unwrap(), "x");
        assert!(
            GitCommandResult {
                exit_code: 1,
                ..ok.clone()
            }
            .success_ignoring(&[1])
        );
    }
}
```

`crates/ig-git/src/handler.rs`:

```rust
//! Port of the command-line and environment parts of
//! `plugins/git4idea/backend/src/commands/GitHandler.java` and `GitLineHandler.java`.
//!
//! Not ported: WSL/Eel executables, Windows `.cmd` parameter escaping, `VcsEnvCustomizer` extensions.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::command::{GitCommand, LockingPolicy};

/// Marker env var that lets git hooks detect IdeaGit.
pub const IDEAGIT_HANDLER_MARKER_ENV: &str = "IDEAGIT_GIT_EXECUTABLE";
/// Upstream's marker (`GitCommand.IJ_HANDLER_MARKER_ENV`), also set so hooks written for IntelliJ
/// behave the same way under IdeaGit.
pub const IJ_HANDLER_MARKER_ENV: &str = "INTELLIJ_GIT_EXECUTABLE";

/// Port of `VcsLocaleHelper.getDefaultLocaleEnvironmentVars("git")`.
pub fn locale_environment() -> [(&'static str, &'static str); 2] {
    [("LANGUAGE", ""), ("LC_ALL", "en_US.UTF-8")]
}

#[derive(Debug, Clone)]
pub struct GitHandler {
    executable: PathBuf,
    working_dir: PathBuf,
    command: GitCommand,
    /// Everything after the executable: `-c` pairs, the command name, then its arguments.
    parameters: Vec<String>,
    custom_env: Vec<(String, String)>,
    removed_env: Vec<String>,
    silent: bool,
    stdout_suppressed: bool,
    stderr_suppressed: bool,
    input: Option<Vec<u8>>,
}

impl GitHandler {
    pub fn new(
        working_dir: impl Into<PathBuf>,
        executable: impl Into<PathBuf>,
        command: GitCommand,
    ) -> Self {
        Self::with_config(working_dir, executable, command, &[])
    }

    /// `config_parameters` are passed as `-c <param>` before the command name, after the defaults.
    pub fn with_config(
        working_dir: impl Into<PathBuf>,
        executable: impl Into<PathBuf>,
        command: GitCommand,
        config_parameters: &[&str],
    ) -> Self {
        let mut parameters = Vec::new();
        for p in default_config_parameters(config_parameters) {
            parameters.push("-c".to_string());
            parameters.push(p);
        }
        parameters.push(command.name().to_string());
        Self {
            executable: executable.into(),
            working_dir: working_dir.into(),
            command,
            parameters,
            custom_env: Vec::new(),
            removed_env: Vec::new(),
            // Upstream: non-write commands are not logged to the console by default.
            silent: command.locking_policy() != LockingPolicy::Write,
            stdout_suppressed: true,
            stderr_suppressed: false,
            input: None,
        }
    }

    pub fn command(&self) -> GitCommand {
        self.command
    }

    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    pub fn parameters(&self) -> &[String] {
        &self.parameters
    }

    pub fn add_parameters<I, S>(&mut self, params: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.parameters.extend(params.into_iter().map(Into::into));
    }

    /// Adds `--`, separating options from paths.
    pub fn end_options(&mut self) {
        self.parameters.push("--".to_string());
    }

    /// Adds paths relative to the working directory, with `/` separators.
    pub fn add_relative_paths<P: AsRef<Path>>(&mut self, paths: &[P]) {
        for p in paths {
            let rel = relative_path(&self.working_dir, p.as_ref());
            self.parameters.push(rel);
        }
    }

    /// Silent commands are not logged to the Git console. Like upstream `setSilent`, making a
    /// handler silent also suppresses both stdout and stderr copying.
    pub fn set_silent(&mut self, silent: bool) {
        self.silent = silent;
        if silent {
            self.stdout_suppressed = true;
            self.stderr_suppressed = true;
        }
    }
    pub fn is_silent(&self) -> bool {
        self.silent
    }
    pub fn set_stdout_suppressed(&mut self, suppressed: bool) {
        self.stdout_suppressed = suppressed;
    }
    pub fn is_stdout_suppressed(&self) -> bool {
        self.stdout_suppressed
    }
    pub fn set_stderr_suppressed(&mut self, suppressed: bool) {
        self.stderr_suppressed = suppressed;
    }
    pub fn is_stderr_suppressed(&self) -> bool {
        self.stderr_suppressed
    }

    /// Bytes written to the command's stdin, which is then closed (upstream `setInputProcessor`).
    pub fn set_input(&mut self, input: Vec<u8>) {
        self.input = Some(input);
    }

    pub fn input(&self) -> Option<&[u8]> {
        self.input.as_deref()
    }

    /// Adds or replaces an environment variable for this command.
    pub fn add_custom_environment_variable(&mut self, name: &str, value: &str) {
        if let Some(entry) = self.custom_env.iter_mut().find(|(k, _)| k == name) {
            entry.1 = value.to_string();
        } else {
            self.custom_env.push((name.to_string(), value.to_string()));
        }
    }

    /// Removes an inherited environment variable from the command's environment.
    pub fn remove_inherited_environment_variable(&mut self, name: &str) {
        if !self.removed_env.iter().any(|n| n == name) {
            self.removed_env.push(name.to_string());
        }
    }

    /// Upstream `containsCustomEnvironmentVariable`.
    pub fn contains_custom_environment_variable(&self, name: &str) -> bool {
        self.custom_env.iter().any(|(k, _)| k == name)
    }

    /// The environment applied on top of the inherited process environment, in application order
    /// (upstream `prepareEnvironment`: locale, then custom variables, then the marker).
    pub fn environment(&self) -> Vec<(String, String)> {
        let mut env: Vec<(String, String)> = locale_environment()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        env.extend(self.custom_env.iter().cloned());
        env.push((IJ_HANDLER_MARKER_ENV.to_string(), "true".to_string()));
        env.push((IDEAGIT_HANDLER_MARKER_ENV.to_string(), "true".to_string()));
        env
    }

    /// The command line as shown in the Git console, e.g. `git -c core.quotepath=false ... status`.
    pub fn printable_command_line(&self) -> String {
        std::iter::once("git".to_string())
            .chain(self.parameters.iter().map(|p| quote_if_needed(p)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn build_command(&self) -> Command {
        let mut cmd = Command::new(&self.executable);
        cmd.args(&self.parameters).current_dir(&self.working_dir);
        for name in &self.removed_env {
            cmd.env_remove(name);
        }
        for (k, v) in self.environment() {
            cmd.env(k, v);
        }
        cmd
    }
}

fn default_config_parameters(requested: &[&str]) -> Vec<String> {
    let mut params = vec![
        "core.quotepath=false".to_string(),
        "log.showSignature=false".to_string(),
    ];
    params.extend(requested.iter().map(|s| s.to_string()));
    params
}

fn quote_if_needed(p: &str) -> String {
    if p.is_empty() || p.chars().any(char::is_whitespace) {
        format!("\"{}\"", p.replace('"', "\\\""))
    } else {
        p.to_string()
    }
}

/// Port of `VcsFileUtil.relativePath`: `/`-separated path relative to `base`, `.` for `base` itself.
/// Paths outside `base` are passed through unchanged.
pub(crate) fn relative_path(base: &Path, path: &Path) -> String {
    match path.strip_prefix(base) {
        Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
        Ok(rel) => rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_default_parameters_and_silence() {
        let mut h = GitHandler::new("/repo", "git", GitCommand::STATUS);
        h.add_parameters(["--porcelain", "-z"]);
        assert_eq!(
            h.parameters(),
            [
                "-c",
                "core.quotepath=false",
                "-c",
                "log.showSignature=false",
                "status",
                "--porcelain",
                "-z"
            ]
        );
        assert!(h.is_silent(), "read commands are silent by default");
        assert!(!GitHandler::new("/repo", "git", GitCommand::COMMIT).is_silent());
        assert!(h.is_stdout_suppressed());
    }

    #[test]
    fn extra_config_and_printable_line() {
        let mut h = GitHandler::with_config("/repo", "git", GitCommand::COMMIT, &["user.name=A B"]);
        h.add_parameters(["-m", "two words"]);
        assert_eq!(
            h.printable_command_line(),
            "git -c core.quotepath=false -c log.showSignature=false -c \"user.name=A B\" commit -m \"two words\""
        );
    }

    #[test]
    fn environment_order_and_override() {
        let mut h = GitHandler::new("/repo", "git", GitCommand::STATUS);
        h.add_custom_environment_variable("GIT_OPTIONAL_LOCKS", "1");
        h.add_custom_environment_variable("GIT_OPTIONAL_LOCKS", "0");
        let env = h.environment();
        assert_eq!(env[0], ("LANGUAGE".into(), "".into()));
        assert_eq!(env[1], ("LC_ALL".into(), "en_US.UTF-8".into()));
        assert_eq!(
            env.iter()
                .filter(|(k, _)| k == "GIT_OPTIONAL_LOCKS")
                .count(),
            1
        );
        assert!(env.contains(&("GIT_OPTIONAL_LOCKS".into(), "0".into())));
        assert!(env.contains(&(IJ_HANDLER_MARKER_ENV.into(), "true".into())));
    }

    #[test]
    fn removed_inherited_variable_is_reported_by_command() {
        let mut h = GitHandler::new("/repo", "git", GitCommand::STATUS);
        h.remove_inherited_environment_variable("GIT_DIR");
        h.remove_inherited_environment_variable("GIT_DIR");
        let cmd = h.build_command();
        let removed: Vec<_> = cmd
            .get_envs()
            .filter(|(k, _)| *k == std::ffi::OsStr::new("GIT_DIR"))
            .collect();
        assert_eq!(removed, [(std::ffi::OsStr::new("GIT_DIR"), None)]);
    }

    #[test]
    fn set_silent_suppresses_output_and_custom_env_lookup() {
        let mut h = GitHandler::new("/repo", "git", GitCommand::COMMIT);
        assert!(!h.is_stderr_suppressed());
        h.set_silent(true);
        assert!(h.is_silent() && h.is_stdout_suppressed() && h.is_stderr_suppressed());
        assert!(!h.contains_custom_environment_variable("GIT_EDITOR"));
        h.add_custom_environment_variable("GIT_EDITOR", "true");
        assert!(h.contains_custom_environment_variable("GIT_EDITOR"));
    }

    #[test]
    fn relative_paths() {
        let base = Path::new("/repo");
        assert_eq!(relative_path(base, Path::new("/repo/a/b.txt")), "a/b.txt");
        assert_eq!(relative_path(base, Path::new("/repo")), ".");
        assert_eq!(relative_path(base, Path::new("/other/x")), "/other/x");
    }
}
```

`crates/ig-git/src/version.rs`:

```rust
//! Port of `plugins/git4idea/backend/src/config/GitVersion.java` and the version-based entries of
//! `GitVersionSpecialty.kt` that IdeaGit currently needs. WSL types are intentionally not ported.

use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

use crate::error::{GitError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitVersionType {
    Unix,
    Msys,
    Cygwin,
    /// The type doesn't matter or couldn't be detected.
    Undefined,
    /// Version information is unavailable (invalid executable or not yet detected).
    Null,
}

/// Note: like upstream, ordering (`compare`) ignores the type while equality takes it into account.
#[derive(Debug, Clone, Copy)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub revision: u32,
    pub patch_level: u32,
    pub ty: GitVersionType,
}

static FORMAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^git version (\d+)\.(\d+)(?:\.(\d+))?(?:\.(\d+))?(.*)$").expect("valid regex")
});

impl GitVersion {
    pub const NULL: GitVersion = GitVersion::with_type(0, 0, 0, 0, GitVersionType::Null);

    pub const fn new(major: u32, minor: u32, revision: u32, patch_level: u32) -> Self {
        Self::with_type(
            major,
            minor,
            revision,
            patch_level,
            GitVersionType::Undefined,
        )
    }

    pub const fn with_type(
        major: u32,
        minor: u32,
        revision: u32,
        patch_level: u32,
        ty: GitVersionType,
    ) -> Self {
        Self {
            major,
            minor,
            revision,
            patch_level,
            ty,
        }
    }

    /// The minimal supported version (upstream `GitVersion.MIN`).
    pub fn min() -> GitVersion {
        if cfg!(windows) {
            GitVersion::new(2, 19, 2, 0)
        } else {
            GitVersion::new(2, 17, 0, 0)
        }
    }

    /// Parses the output of `git version`.
    pub fn parse(output: &str) -> Result<GitVersion> {
        Self::parse_for_os(output, cfg!(windows))
    }

    /// Same as [`GitVersion::parse`] with an explicit OS, so Windows type detection is testable anywhere.
    pub fn parse_for_os(output: &str, is_windows: bool) -> Result<GitVersion> {
        let trimmed = output.trim();
        if trimmed.is_empty() {
            return Err(GitError::VersionParse(output.to_string()));
        }
        let caps = FORMAT
            .captures(trimmed)
            .ok_or_else(|| GitError::VersionParse(output.to_string()))?;
        let int_group = |i: usize| -> Result<u32> {
            match caps.get(i) {
                None => Ok(0),
                Some(m) => m
                    .as_str()
                    .parse::<u32>()
                    .map_err(|_| GitError::VersionParse(output.to_string())),
            }
        };
        let suffix = caps
            .get(5)
            .map(|m| m.as_str().trim().to_lowercase())
            .unwrap_or_default();
        let ty = if is_windows {
            if suffix.contains("msysgit") || suffix.contains("windows") {
                GitVersionType::Msys
            } else {
                GitVersionType::Cygwin
            }
        } else {
            GitVersionType::Unix
        };
        Ok(GitVersion::with_type(
            int_group(1)?,
            int_group(2)?,
            int_group(3)?,
            int_group(4)?,
            ty,
        ))
    }

    pub fn is_null(&self) -> bool {
        self.ty == GitVersionType::Null
    }

    /// True if the version is supported (not NULL and at least [`GitVersion::min`]).
    pub fn is_supported(&self) -> bool {
        !self.is_null() && self.compare(&GitVersion::min()) != Ordering::Less
    }

    /// Upstream `compareTo`: only numbers are compared. A NULL `other` makes any non-NULL version
    /// Greater; a NULL `self` compares as 0.0.0.0 (so it equals a non-NULL 0.0.0.0, as upstream).
    pub fn compare(&self, other: &GitVersion) -> Ordering {
        if other.is_null() {
            return if self.is_null() {
                Ordering::Equal
            } else {
                Ordering::Greater
            };
        }
        (self.major, self.minor, self.revision, self.patch_level).cmp(&(
            other.major,
            other.minor,
            other.revision,
            other.patch_level,
        ))
    }

    pub fn is_later_or_equal(&self, other: &GitVersion) -> bool {
        self.compare(other) != Ordering::Less
    }

    pub fn is_older_or_equal(&self, other: &GitVersion) -> bool {
        self.compare(other) != Ordering::Greater
    }

    /// e.g. `2.39.1`, or `2.7.3.3` when there is a patch level.
    pub fn presentation(&self) -> String {
        let mut s = format!("{}.{}.{}", self.major, self.minor, self.revision);
        if self.patch_level > 0 {
            s.push_str(&format!(".{}", self.patch_level));
        }
        s
    }
}

/// Upstream `equals`: numbers must match, and types must match unless either is `Undefined`.
/// Like upstream, this is not transitive (`Unix == Undefined == Msys`, but `Unix != Msys`), so
/// don't use `GitVersion` as a map/set key.
impl PartialEq for GitVersion {
    fn eq(&self, other: &Self) -> bool {
        if self.compare(other) != Ordering::Equal {
            return false;
        }
        if self.ty == GitVersionType::Undefined || other.ty == GitVersionType::Undefined {
            return true;
        }
        self.ty == other.ty
    }
}

/// Port of the simple, version-threshold entries of `GitVersionSpecialty.kt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitVersionSpecialty {
    EnvGitTracePackAccessAllowed,
    EnvGitOptionalLocksAllowed,
    StatusSupportsIgnoredModes,
    StatusSupportsNoRenames,
    StartedUsingRawBodyInFormat,
    NoWalkUnsorted,
}

impl GitVersionSpecialty {
    fn minimal_version(self) -> GitVersion {
        match self {
            Self::EnvGitTracePackAccessAllowed => GitVersion::new(2, 1, 0, 0),
            Self::EnvGitOptionalLocksAllowed => GitVersion::new(2, 15, 0, 0),
            Self::StatusSupportsIgnoredModes => GitVersion::new(2, 16, 0, 0),
            Self::StatusSupportsNoRenames => GitVersion::new(2, 18, 0, 0),
            Self::StartedUsingRawBodyInFormat => GitVersion::new(1, 7, 2, 0),
            Self::NoWalkUnsorted => GitVersion::new(1, 7, 12, 1),
        }
    }

    pub fn exists_in(self, version: &GitVersion) -> bool {
        version.is_later_or_equal(&self.minimal_version())
    }
}

#[cfg(test)]
mod tests {
    //! Ported from `plugins/git4idea/tests/git4idea/tests/GitVersionTest.java`.
    use super::*;

    const COMMON: &[(&str, u32, u32, u32, u32)] = &[
        ("git version 1.6.4", 1, 6, 4, 0),
        ("git version 1.7.3.3", 1, 7, 3, 3),
        ("git version 1.7.3.5.gb27be", 1, 7, 3, 5),
        ("git version 1.7.4-rc1", 1, 7, 4, 0),
        ("git version 1.7.7.5 (Apple Git-24)", 1, 7, 7, 5),
        ("git version 1.9.rc0.143.g6fd479e", 1, 9, 0, 0),
    ];

    const MSYS: &[(&str, u32, u32, u32, u32)] = &[
        ("git version 1.6.4.msysgit.0", 1, 6, 4, 0),
        ("git version 1.7.3.3.msysgit.1", 1, 7, 3, 3),
        ("git version 1.7.3.2.msysgit", 1, 7, 3, 2),
        ("git version 1.7.3.5.msysgit.gb27be", 1, 7, 3, 5),
        ("git version 2.19.1.windows.1", 2, 19, 1, 0),
    ];

    fn assert_version(v: GitVersion, expected: (&str, u32, u32, u32, u32), ty: GitVersionType) {
        assert_eq!(
            (v.major, v.minor, v.revision, v.patch_level, v.ty),
            (expected.1, expected.2, expected.3, expected.4, ty),
            "parsing {:?}",
            expected.0
        );
    }

    #[test]
    fn parses_unix_versions() {
        for t in COMMON {
            assert_version(
                GitVersion::parse_for_os(t.0, false).unwrap(),
                *t,
                GitVersionType::Unix,
            );
        }
    }

    #[test]
    fn parses_cygwin_and_msys_on_windows() {
        for t in COMMON {
            assert_version(
                GitVersion::parse_for_os(t.0, true).unwrap(),
                *t,
                GitVersionType::Cygwin,
            );
        }
        for t in MSYS {
            assert_version(
                GitVersion::parse_for_os(t.0, true).unwrap(),
                *t,
                GitVersionType::Msys,
            );
        }
    }

    #[test]
    fn equality_and_ordering() {
        let v1 = GitVersion::parse_for_os("git version 1.6.3", true).unwrap();
        let v2 = GitVersion::parse_for_os("git version 1.7.3", true).unwrap();
        let v3 = GitVersion::parse_for_os("git version 1.7.3", true).unwrap();
        let v4 = GitVersion::parse_for_os("git version 1.7.3.msysgit.0", true).unwrap();
        assert!(v1 != v2);
        assert!(v1 != v3);
        assert!(v2 == v3);
        assert!(v1 != v4);
        assert!(v2 != v4, "cygwin and msys of the same number are not equal");
        assert_eq!(v1.compare(&v2), Ordering::Less);
        assert_eq!(v2.compare(&v3), Ordering::Equal);
        assert_eq!(
            v2.compare(&v4),
            Ordering::Equal,
            "ordering ignores the type"
        );
    }

    #[test]
    fn rejects_garbage_and_handles_null() {
        assert!(GitVersion::parse("").is_err());
        assert!(GitVersion::parse("hello").is_err());
        assert_eq!(
            GitVersion::NULL.compare(&GitVersion::new(0, 0, 0, 1)),
            Ordering::Less
        );
        assert_eq!(
            GitVersion::new(2, 0, 0, 0).compare(&GitVersion::NULL),
            Ordering::Greater
        );
        assert!(!GitVersion::NULL.is_supported());
        assert!(GitVersion::new(2, 40, 0, 0).is_supported());
    }

    #[test]
    fn specialties() {
        let v = GitVersion::new(2, 15, 0, 0);
        assert!(GitVersionSpecialty::EnvGitOptionalLocksAllowed.exists_in(&v));
        assert!(!GitVersionSpecialty::StatusSupportsIgnoredModes.exists_in(&v));
        assert!(!GitVersionSpecialty::StatusSupportsNoRenames.exists_in(&GitVersion::NULL));
    }

    #[test]
    fn presentation() {
        assert_eq!(GitVersion::new(2, 39, 1, 0).presentation(), "2.39.1");
        assert_eq!(GitVersion::new(1, 7, 3, 3).presentation(), "1.7.3.3");
    }
}
```

`crates/ig-git/src/natural_comparator.rs`:

```rust
//! Natural sort order ("file2" < "file10"), used to compare reference names.
//!
//! Port of `platform/util/base/src/com/intellij/openapi/util/text/NaturalComparator.java` (with
//! `StringUtilRt.compare(char, char, boolean)`), which is `GitReference.REFS_NAMES_COMPARATOR`
//! (`plugins/git4idea/shared/src/git4idea/GitReference.kt`). Works on UTF-16 code units, like Java.

use std::cmp::Ordering;

/// `NaturalComparator.INSTANCE.compare`: case-insensitive natural order, then case-sensitive.
pub fn natural_compare(s1: &str, s2: &str) -> Ordering {
    let a: Vec<u16> = s1.encode_utf16().collect();
    let b: Vec<u16> = s2.encode_utf16().collect();
    natural_compare_units(&a, &b, true, false).cmp(&0)
}

/// `naturalCompare(s1, s2, ignoreCase, likeFileNames)`, returning Java's `int`.
pub fn natural_compare_with(s1: &str, s2: &str, ignore_case: bool, like_file_names: bool) -> i32 {
    let a: Vec<u16> = s1.encode_utf16().collect();
    let b: Vec<u16> = s2.encode_utf16().collect();
    natural_compare_units(&a, &b, ignore_case, like_file_names)
}

fn is_decimal_digit(c: u16) -> bool {
    (b'0' as u16..=b'9' as u16).contains(&c)
}

const SPACE: u16 = b' ' as u16;
const ZERO: u16 = b'0' as u16;

fn natural_compare_units(s1: &[u16], s2: &[u16], ignore_case: bool, like_file_names: bool) -> i32 {
    let (length1, length2) = (s1.len(), s2.len());
    let mut i = 0;
    let mut j = 0;
    while i < length1 && j < length2 {
        let ch1 = s1[i];
        let ch2 = s2[j];
        if (is_decimal_digit(ch1) || ch1 == SPACE) && (is_decimal_digit(ch2) || ch2 == SPACE) {
            let start1 = skip_char(s1, skip_char(s1, i, SPACE), ZERO);
            let start2 = skip_char(s2, skip_char(s2, j, SPACE), ZERO);

            let end1 = skip_digits(s1, start1);
            let end2 = skip_digits(s2, start2);

            // numbers with more digits are always greater than shorter numbers
            let length_diff = (end1 - start1) as i32 - (end2 - start2) as i32;
            if length_diff != 0 {
                return length_diff;
            }
            // compare numbers with equal digit count
            let number_diff = compare_char_range(s1, s2, start1, start2, end1);
            if number_diff != 0 {
                return number_diff;
            }
            // compare number length including leading spaces and zeroes
            let full_length_diff = (end1 - i) as i32 - (end2 - j) as i32;
            if full_length_diff != 0 {
                return full_length_diff;
            }
            // the numbers are the same; compare leading spaces and zeroes
            let leading_diff = compare_char_range(s1, s2, i, j, start1);
            if leading_diff != 0 {
                return leading_diff;
            }
            // `i = end1 - 1` before the loop increment; `end1 > i` here
            i = end1;
            j = end2;
            continue;
        } else if like_file_names {
            // for super natural comparison (IDEA-80435)
            if ch1 != ch2 {
                let dash = b'-' as u16;
                let underscore = b'_' as u16;
                let diff = if ch1 == dash && ch2 != underscore {
                    compare_chars(underscore, ch2, ignore_case)
                } else if ch2 == dash && ch1 != underscore {
                    compare_chars(ch1, underscore, ignore_case)
                } else {
                    compare_chars(ch1, ch2, ignore_case)
                };
                if diff != 0 {
                    return diff;
                }
            }
        } else {
            let diff = compare_chars(ch1, ch2, ignore_case);
            if diff != 0 {
                return diff;
            }
        }
        i += 1;
        j += 1;
    }
    // the end of one of the strings might not have been reached if the other string ends with a
    // number and the strings are equal until the end of that number
    if i < length1 {
        return 1;
    }
    if j < length2 {
        return -1;
    }
    if length1 != length2 {
        return length1 as i32 - length2 as i32;
    }
    // do case-sensitive compare if case-insensitive strings are equal
    if ignore_case {
        natural_compare_units(s1, s2, false, like_file_names)
    } else {
        0
    }
}

fn compare_char_range(s1: &[u16], s2: &[u16], offset1: usize, offset2: usize, end1: usize) -> i32 {
    for k in 0..end1.saturating_sub(offset1) {
        let diff = s1[offset1 + k] as i32 - s2[offset2 + k] as i32;
        if diff != 0 {
            return diff;
        }
    }
    0
}

fn compare_chars(ch1: u16, ch2: u16, ignore_case: bool) -> i32 {
    // transitivity fix, otherwise can fail when comparing strings with characters between ' ' and
    // '0' (e.g. '#')
    if ch1 == SPACE && ch2 > SPACE && ch2 < ZERO {
        return 1;
    }
    if ch2 == SPACE && ch1 > SPACE && ch1 < ZERO {
        return -1;
    }
    compare_chars_rt(ch1, ch2, ignore_case)
}

/// `StringUtilRt.compare(char, char, boolean)`.
fn compare_chars_rt(c1: u16, c2: u16, ignore_case: bool) -> i32 {
    let d = c1 as i32 - c2 as i32;
    if d == 0 || !ignore_case {
        return d;
    }
    let u1 = to_upper(c1);
    let u2 = to_upper(c2);
    let d = u1 as i32 - u2 as i32;
    if d != 0 {
        return to_lower(u1) as i32 - to_lower(u2) as i32;
    }
    d
}

/// `Character.toUpperCase(char)` for one UTF-16 unit (surrogates unchanged).
fn to_upper(c: u16) -> u16 {
    map_unit(c, |ch| ch.to_uppercase())
}

fn to_lower(c: u16) -> u16 {
    map_unit(c, |ch| ch.to_lowercase())
}

fn map_unit<I: Iterator<Item = char>>(c: u16, f: impl Fn(char) -> I) -> u16 {
    match char::from_u32(c as u32) {
        Some(ch) => {
            let mut mapped = f(ch);
            match (mapped.next(), mapped.next()) {
                // Java maps one char to one char; multi-char mappings keep the original
                (Some(m), None) if (m as u32) <= 0xFFFF => m as u16,
                _ => c,
            }
        }
        None => c,
    }
}

fn skip_digits(s: &[u16], mut start: usize) -> usize {
    while start < s.len() && is_decimal_digit(s[start]) {
        start += 1;
    }
    start
}

fn skip_char(s: &[u16], mut start: usize, c: u16) -> usize {
    while start < s.len() && s[start] == c {
        start += 1;
    }
    start
}

#[cfg(test)]
mod tests {
    use super::*;
    use Ordering::*;

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(natural_compare("file2", "file10"), Less);
        assert_eq!(natural_compare("v1.10", "v1.9"), Greater);
        assert_eq!(natural_compare("a01", "a1"), Greater);
        assert_eq!(natural_compare("abc", "abc"), Equal);
    }

    #[test]
    fn case_insensitive_first() {
        assert_eq!(natural_compare("Feature", "feature"), Less);
        assert_eq!(natural_compare("Zoo", "apple"), Greater);
        assert_eq!(natural_compare("feature", "release"), Less);
        assert_eq!(natural_compare("origin/aaa", "origin/feature"), Less);
        assert_eq!(natural_compare("a", "ab"), Less);
    }
}
```

- [ ] **Step 2: Register the comparator.** Replace `crates/ig-git/src/lib.rs` with this content. The `log` module is added in the next task.

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod branch;
pub mod command;
pub mod config;
pub mod console;
pub mod error;
pub mod handler;
pub mod hash;
pub mod natural_comparator;
pub mod reader;
pub mod refs;
pub mod repo_files;
pub mod roots;
pub mod runner;
pub mod status;
pub mod version;
```

- [ ] **Step 3: Run**

```bash
cargo clippy -q --all-targets -- -D warnings
cargo test -p ig-git
```
Expected: clean. All existing `ig-git` targets pass, plus the new `natural_comparator` unit tests.

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "feat(ig-git): runner line listener and stdin, version specialties, natural comparator"
```

---

### Task 7: ig-git: the `log` module

**Files:**
- Create: `crates/ig-git/src/log/*.rs`, `crates/ig-git/tests/common/log_helpers.rs`
- Modify: `crates/ig-git/Cargo.toml`, `crates/ig-git/src/lib.rs`, `crates/ig-git/tests/common/mod.rs`

**Review focus:** crate-level design of `ig_git::log` and the new ig-git → ig-log dependency.

- [ ] **Step 1: Depend on `ig-log`.** Replace `crates/ig-git/Cargo.toml`:

`crates/ig-git/Cargo.toml`:

```toml
[package]
name = "ig-git"
version = "0.1.0"
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[dependencies]
ig-log = { path = "../ig-log" }
regex.workspace = true
thiserror.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 2: Create the log module files**

`crates/ig-git/src/log/mod.rs`:

```rust
//! The git side of the log: `git log` parsing, reading commits, metadata and refs, ref ordering
//! and the [`GitLogProvider`](provider::GitLogProvider) that feeds `ig_log::data`.
//!
//! Ported from `plugins/git4idea/backend/src/history` (`GitLogParser`, `GitLogRecord`,
//! `GitLogOutputSplitter`, `GitLogRecordBuilder`, `GitLogRecordCollector`, `GitLogUtil`) and
//! `plugins/git4idea/backend/src/log` (`GitLogProvider`, `GitRefManager`). Each module names its
//! upstream files; see also `reference/upstream-map.toml`.
//!
//! This is the only part of `ig-git` that depends on `ig-log`, as upstream `git4idea` depends on
//! the `vcs-log` API: the provider produces `ig_log::data` types directly.

pub mod collector;
pub mod parser;
pub mod provider;
pub mod record;
pub mod ref_manager;
pub mod repository_manager;
pub mod splitter;
pub mod unescape;
pub mod util;
```

`crates/ig-git/src/log/collector.rs`:

```rust
//! Grouping the records of one commit, as `git log -m` prints one record per parent.
//!
//! Port of `plugins/git4idea/backend/src/history/GitLogRecordCollector.java`.
//!
//! `git log -m` omits the record for a parent whose tree equals the merge commit's; the collector
//! then reads the tree hashes and inserts empty records for such parents.
//!
//! Deviation: errors upstream only logs (reading tree hashes fails, a tree hash is missing) drop
//! the incomplete commit or treat the parent as different; the error is kept for
//! [`GitLogRecordCollector::take_error`].

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

    /// `fillWithEmptyRecords`: an empty record for each parent with the commit's tree.
    fn fill_with_empty_records(
        &self,
        records: &mut Vec<R>,
        hash_to_tree_map: &HashMap<String, String>,
    ) {
        let first = records[0].as_record();
        let parents: Vec<String> = first
            .parents_hashes()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let commit_tree_hash = hash_to_tree_map.get(first.hash()).cloned();
        for (parent_index, parent) in parents.iter().enumerate() {
            // sometimes a merge commit is identical to all its parents: we get one empty record
            let parent_tree_hash = hash_to_tree_map.get(parent);
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
```

`crates/ig-git/src/log/parser.rs`:

```rust
//! Parsing `git log` output produced with a custom `--pretty=format:`.
//!
//! Port of `plugins/git4idea/backend/src/history/GitLogParser.java` (and `GitFormatException`).
//!
//! A record is `RECORD_START item ITEMS_SEPARATOR item ... RECORD_END`, optionally followed by
//! `--name-status` lines. Each parser instance appends a random tail of control characters to the
//! three markers; its length grows with the number of format errors seen in the process, so that
//! a commit message containing a marker breaks at most a few parses.
//!
//! Lines are processed byte-wise: the markers are ASCII control characters, which never occur
//! inside a multi-byte UTF-8 sequence, so items are cut at character boundaries (upstream works
//! on UTF-16 chars).
//!
//! Deviation: errors upstream logs with `LOG.error` (format errors in `parse`) are dropped.

use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::log::record::{GitLogRecordBuilder, change_type_from_status};
use crate::log::unescape::unescape_git_path;

/// Records begin with `%x01%x01`.
pub const RECORD_START: &str = "\u{1}\u{1}";
/// Items of commit information (hash, committer, subject, ...) are separated by `%x02%x02`.
pub const ITEMS_SEPARATOR: &str = "\u{2}\u{2}";
/// Records end with `%x03%x03`.
pub const RECORD_END: &str = "\u{3}\u{3}";
const MAX_SEPARATOR_LENGTH: usize = 10;
const CONTROL_CHARS: [char; 3] = ['\u{1}', '\u{2}', '\u{3}'];
/// Limits the line quoted in an error to ~2mb.
const INPUT_ERROR_MESSAGE_HEAD_LIMIT: usize = 1_000_000;
const INPUT_ERROR_MESSAGE_TAIL_LIMIT: usize = 100;

static ERROR_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Output that doesn't match the format (`GitFormatException`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct GitFormatError(pub String);

/// `--name-status` or no flag (`GitLogParser.NameStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameStatus {
    /// No flag.
    None,
    /// `--name-status`.
    Status,
}

/// A piece of commit information requested with a `--pretty=format:` placeholder
/// (`GitLogParser.GitLogOption`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GitLogOption {
    Hash,
    Tree,
    CommitTime,
    AuthorName,
    AuthorTime,
    AuthorEmail,
    CommitterName,
    CommitterEmail,
    Subject,
    Body,
    Parents,
    RefNames,
    ShortRefLogSelector,
    RawBody,
}

impl GitLogOption {
    pub const COUNT: usize = 14;

    pub fn placeholder(self) -> &'static str {
        match self {
            GitLogOption::Hash => "H",
            GitLogOption::Tree => "T",
            GitLogOption::CommitTime => "ct",
            GitLogOption::AuthorName => "an",
            GitLogOption::AuthorTime => "at",
            GitLogOption::AuthorEmail => "ae",
            GitLogOption::CommitterName => "cn",
            GitLogOption::CommitterEmail => "ce",
            GitLogOption::Subject => "s",
            GitLogOption::Body => "b",
            GitLogOption::Parents => "P",
            GitLogOption::RefNames => "d",
            GitLogOption::ShortRefLogSelector => "gd",
            GitLogOption::RawBody => "B",
        }
    }

    /// Position in [`GitLogOption`] order, used to store option values in an array.
    pub fn index(self) -> usize {
        self as usize
    }
}

/// The parsed options of one record, by option.
pub type OptionValues = [Option<String>; GitLogOption::COUNT];

/// Parses `git log` output for a fixed list of options into records built by `B`.
pub struct GitLogParser<B: GitLogRecordBuilder> {
    supports_raw_body: bool,
    pretty: String,
    options_parser: OptionsParser,
    paths_parser: PathsParser,
    record_builder: B,
    is_in_body: bool,
}

impl<B: GitLogRecordBuilder> GitLogParser<B> {
    /// `supports_raw_body`: `GitVersionSpecialty.STARTED_USING_RAW_BODY_IN_FORMAT` (see
    /// [`crate::log::util::supports_raw_body`]).
    pub fn new(
        record_builder: B,
        supports_raw_body: bool,
        name_status: NameStatus,
        options: &[GitLogOption],
    ) -> GitLogParser<B> {
        let record_start = format!("{RECORD_START}{}", generate_random_sequence());
        let record_end = format!("{RECORD_END}{}", generate_random_sequence());
        let items_separator = format!("{ITEMS_SEPARATOR}{}", generate_random_sequence());
        let pretty = format!(
            "--pretty=format:{}{}{}",
            encode_for_git(&record_start),
            make_format_from_options(options, &encode_for_git(&items_separator)),
            encode_for_git(&record_end)
        );
        GitLogParser {
            supports_raw_body,
            pretty,
            options_parser: OptionsParser {
                options: options.to_vec(),
                result: PartialResult::default(),
                record_start: record_start.into_bytes(),
                record_end: record_end.into_bytes(),
                items_separator: items_separator.into_bytes(),
            },
            paths_parser: PathsParser {
                name_status,
                error_text: None,
            },
            record_builder,
            is_in_body: true,
        }
    }

    /// `parse`: all records of `output`. Records with format errors are skipped.
    pub fn parse(&mut self, output: &str) -> Vec<B::Record> {
        let mut result = Vec::new();
        for line in output.split('\n') {
            match self.parse_line(line) {
                Ok(Some(record)) => result.push(record),
                Ok(None) => {}
                Err(_) => self.clear(),
            }
        }
        if let Some(record) = self.finish() {
            result.push(record);
        }
        result
    }

    /// `parseOneRecord`: the first record of `output`.
    pub fn parse_one_record(&mut self, output: &str) -> Option<B::Record> {
        let records = self.parse(output);
        self.clear();
        records.into_iter().next()
    }

    /// `parseLine`: feeds one line without its separator; returns a record when one is complete.
    pub fn parse_line(&mut self, line: &str) -> Result<Option<B::Record>, GitFormatError> {
        if self.paths_parser.expects_paths() {
            return self.parse_line_with_paths(line);
        }
        self.parse_line_without_paths(line)
    }

    fn parse_line_with_paths(&mut self, line: &str) -> Result<Option<B::Record>, GitFormatError> {
        if self.is_in_body {
            self.is_in_body = !self.options_parser.parse_line(line)?;
        } else {
            if line
                .as_bytes()
                .starts_with(&self.options_parser.record_start)
            {
                let record = self.create_record();
                self.is_in_body = !self.options_parser.parse_line(line)?;
                return Ok(record);
            }
            self.paths_parser
                .parse_line(line, &mut self.record_builder)?;
        }
        Ok(None)
    }

    fn parse_line_without_paths(
        &mut self,
        line: &str,
    ) -> Result<Option<B::Record>, GitFormatError> {
        if self.options_parser.parse_line(line)? {
            return Ok(self.create_record());
        }
        Ok(None)
    }

    /// `finish`: the last record, at the end of the output.
    pub fn finish(&mut self) -> Option<B::Record> {
        if self.options_parser.is_empty() {
            return None;
        }
        self.create_record()
    }

    fn create_record(&mut self) -> Option<B::Record> {
        if self.paths_parser.error_text.is_some()
            || !self.options_parser.has_complete_options_list()
        {
            self.options_parser.clear();
            self.record_builder.clear();
            self.paths_parser.clear();
            return None;
        }
        let options = self.options_parser.get_result();
        self.options_parser.clear();
        let record = self.record_builder.build(options, self.supports_raw_body);
        self.record_builder.clear();
        self.paths_parser.clear();
        self.is_in_body = true;
        Some(record)
    }

    pub fn clear(&mut self) {
        self.options_parser.clear();
        self.record_builder.clear();
        self.is_in_body = true;
    }

    /// `getPretty`: the `--pretty=format:...` parameter to pass to `git log`.
    pub fn get_pretty(&self) -> &str {
        &self.pretty
    }
}

/// `makeFormatFromOptions(options, separator)`.
pub fn make_format_from_options(options: &[GitLogOption], separator: &str) -> String {
    options
        .iter()
        .map(|o| format!("%{}", o.placeholder()))
        .collect::<Vec<_>>()
        .join(separator)
}

fn encode_for_git(line: &str) -> String {
    line.chars()
        .map(|c| format!("%x{:02x}", c as u32))
        .collect()
}

fn generate_random_sequence() -> String {
    let length = ERROR_COUNT.load(Ordering::Relaxed) % (MAX_SEPARATOR_LENGTH - RECORD_START.len());
    (0..length)
        .map(|_| CONTROL_CHARS[(random_u64() % CONTROL_CHARS.len() as u64) as usize])
        .collect()
}

/// A random number without a dependency: `RandomState` is randomly seeded per instance.
fn random_u64() -> u64 {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u8(0);
    hasher.finish()
}

fn throw_gfe(message: &str, line: &str) -> GitFormatError {
    ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
    GitFormatError(format!(
        "{message} [{}]",
        get_truncated_escaped_output(line)
    ))
}

fn get_truncated_escaped_output(line: &str) -> String {
    let len = line.chars().count();
    let line = if len > INPUT_ERROR_MESSAGE_HEAD_LIMIT + INPUT_ERROR_MESSAGE_TAIL_LIMIT + 32 {
        let head: String = line.chars().take(INPUT_ERROR_MESSAGE_HEAD_LIMIT).collect();
        let tail: String = line
            .chars()
            .skip(len - INPUT_ERROR_MESSAGE_TAIL_LIMIT)
            .collect();
        let more = len - INPUT_ERROR_MESSAGE_HEAD_LIMIT - INPUT_ERROR_MESSAGE_TAIL_LIMIT;
        format!("{head}...({more} more characters)...{tail}")
    } else {
        line.to_string()
    };
    line.escape_default().to_string()
}

struct OptionsParser {
    options: Vec<GitLogOption>,
    result: PartialResult,
    record_start: Vec<u8>,
    record_end: Vec<u8>,
    items_separator: Vec<u8>,
}

impl OptionsParser {
    /// Returns `true` when the record ended on this line.
    fn parse_line(&mut self, line: &str) -> Result<bool, GitFormatError> {
        let bytes = line.as_bytes();
        let mut offset = 0;

        if self.result.is_empty() {
            if !bytes.starts_with(&self.record_start) {
                return Ok(false);
            }
            offset += self.record_start.len();
        }

        while offset < bytes.len() {
            if self.at_record_end(bytes, offset) {
                self.result.finish_item();
                if !self.has_complete_options_list() {
                    return Err(throw_gfe(
                        &format!(
                            "Parsed incomplete options {:?} for {:?}",
                            self.result.result, self.options
                        ),
                        line,
                    ));
                }
                return Ok(true);
            }
            if bytes[offset..].starts_with(&self.items_separator) {
                self.result.finish_item();
                offset += self.items_separator.len();
            } else {
                self.result.current_item.push(bytes[offset]);
                offset += 1;
            }
        }

        self.result.current_item.push(b'\n');
        Ok(false)
    }

    fn has_complete_options_list(&self) -> bool {
        self.result.result.len() == self.options.len()
    }

    fn at_record_end(&self, bytes: &[u8], offset: usize) -> bool {
        bytes.len() >= self.record_end.len()
            && offset == bytes.len() - self.record_end.len()
            && bytes[offset..].starts_with(&self.record_end)
    }

    fn get_result(&self) -> OptionValues {
        let mut values: OptionValues = Default::default();
        for (option, value) in self.options.iter().zip(&self.result.result) {
            values[option.index()] = Some(value.clone());
        }
        values
    }

    fn clear(&mut self) {
        self.result.clear();
    }

    fn is_empty(&self) -> bool {
        self.result.is_empty()
    }
}

/// `--name-status` lines (`PathsParser`).
struct PathsParser {
    name_status: NameStatus,
    error_text: Option<String>,
}

impl PathsParser {
    fn parse_line<B: GitLogRecordBuilder>(
        &mut self,
        line: &str,
        record_builder: &mut B,
    ) -> Result<(), GitFormatError> {
        if line.is_empty() {
            return Ok(());
        }
        let parts = parse_paths_line(line);
        if parts.is_empty() {
            return Ok(());
        }
        if self.name_status != NameStatus::Status {
            return Err(throw_gfe("Status list not expected", line));
        }
        if parts.len() < 2 {
            self.error_text = Some(format!("Could not parse status line [{line}]"));
            return Ok(());
        }
        let change_type = change_type_from_status(parts[0])?;
        let first_path = try_unescape_path(parts[1]);
        let second_path = parts.get(2).map(|p| try_unescape_path(p));
        record_builder.add_path(change_type, first_path, second_path);
        Ok(())
    }

    fn expects_paths(&self) -> bool {
        self.name_status == NameStatus::Status
    }

    fn clear(&mut self) {
        self.error_text = None;
    }
}

/// `tryUnescapePath`: the path itself if it can't be unescaped.
fn try_unescape_path(path: &str) -> String {
    unescape_git_path(path).unwrap_or_else(|_| path.to_string())
}

/// Tab-separated tokens; a trailing tab doesn't add an empty token.
fn parse_paths_line(line: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < line.len() {
        let token_end = line[offset..].find('\t').map_or(line.len(), |i| offset + i);
        result.push(&line[offset..token_end]);
        offset = token_end + 1;
    }
    result
}

#[derive(Default)]
struct PartialResult {
    result: Vec<String>,
    current_item: Vec<u8>,
}

impl PartialResult {
    fn finish_item(&mut self) {
        let item = std::mem::take(&mut self.current_item);
        self.result.push(
            String::from_utf8(item)
                .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()),
        );
    }

    fn clear(&mut self) {
        self.current_item.clear();
        self.result = Vec::new();
    }

    fn is_empty(&self) -> bool {
        self.result.is_empty() && self.current_item.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::record::DefaultGitLogRecordBuilder;

    #[test]
    fn pretty_format_encodes_markers() {
        let parser = GitLogParser::new(
            DefaultGitLogRecordBuilder,
            true,
            NameStatus::None,
            &[GitLogOption::Hash, GitLogOption::Parents],
        );
        let start = String::from_utf8(parser.options_parser.record_start.clone()).unwrap();
        let expected_start: String = start
            .chars()
            .map(|c| format!("%x{:02x}", c as u32))
            .collect();
        assert!(
            parser
                .get_pretty()
                .starts_with(&format!("--pretty=format:{expected_start}%H"))
        );
        assert!(parser.get_pretty().starts_with("--pretty=format:%x01%x01"));
        assert!(parser.get_pretty().contains("%x02%x02"));
        assert!(parser.get_pretty().contains("%P%x03%x03"));
    }

    #[test]
    fn incomplete_record_is_a_format_error_and_bumps_the_separator_length() {
        let mut parser = GitLogParser::new(
            DefaultGitLogRecordBuilder,
            true,
            NameStatus::None,
            &[GitLogOption::Hash, GitLogOption::Parents],
        );
        let start = String::from_utf8(parser.options_parser.record_start.clone()).unwrap();
        let end = String::from_utf8(parser.options_parser.record_end.clone()).unwrap();
        let before = ERROR_COUNT.load(Ordering::Relaxed);
        let error = parser.parse_line(&format!("{start}abc{end}")).unwrap_err();
        assert!(
            error.0.starts_with("Parsed incomplete options [\"abc\"]"),
            "{error}"
        );
        assert!(ERROR_COUNT.load(Ordering::Relaxed) > before);
        parser.clear();
        // a status line without --name-status
        let mut parser = GitLogParser::new(
            DefaultGitLogRecordBuilder,
            true,
            NameStatus::None,
            &[GitLogOption::Hash],
        );
        assert!(
            parser
                .paths_parser
                .parse_line("M\ta", &mut DefaultGitLogRecordBuilder)
                .is_err()
        );
    }

    #[test]
    fn paths_line_tokens() {
        assert_eq!(parse_paths_line("M\tfile \t"), ["M", "file "]);
        assert_eq!(parse_paths_line("R100\ta\tb"), ["R100", "a", "b"]);
        assert_eq!(parse_paths_line("\t"), [""]);
    }
}
```

`crates/ig-git/src/log/provider.rs`:

```rust
//! The git log provider: recent commits with refs, all commits, metadata and the current user.
//!
//! Port of the data-reading parts of `plugins/git4idea/backend/src/log/GitLogProvider.kt`,
//! `plugins/git4idea/backend/src/log/VcsLogObjectsFactoryExt.kt` (`createBranchesRefs`) and
//! `plugins/git4idea/backend/src/GitUserRegistry.java` with `GitConfigUtil.getValue`.
//!
//! Deviations:
//! - Repository state comes from [`GitRepositoryManager`] (reread when refs are refreshed), not
//!   from IDE-wide repository objects updated by file-system events.
//! - Branch refs are created in name order (upstream: `HashSet` order); refs and commits gathered
//!   in sets upstream keep their first-seen order.
//! - Not ported: the experimental refs collection (`GitLogExperimentalProvider`), filters
//!   (`getCommitsMatchingFilter`), full details, containing branches, refresh subscriptions,
//!   `resolveReference`, submodule root correction and the debug-only data validation.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ig_log::data::{
    DetailedLogData, Hash, LogData, Requirements, TimedVcsCommit, VcsCommitMetadata, VcsException,
    VcsLogProvider, VcsLogRefManager, VcsRef, VcsRefsContainer, VcsRoot, VcsUser,
    sort_by_date_topo_order,
};

use crate::command::GitCommand;
use crate::error::GitError;
use crate::log::ref_manager::{GitRefManager, HEAD, LOCAL_BRANCH, REMOTE_BRANCH, TAG};
use crate::log::repository_manager::{GitRepoInfo, GitRepositoryManager};
use crate::log::util::{
    LOG_ALL, RefsByNameAndType, collect_metadata, collect_metadata_with_params, get_all_tags,
    read_timed_commits,
};
use crate::runner::Git;

/// `GitConfigUtil.USER_NAME` / `USER_EMAIL`.
const USER_NAME: &str = "user.name";
const USER_EMAIL: &str = "user.email";

fn vcs_error(e: GitError) -> VcsException {
    VcsException(e.to_string())
}

/// The git [`VcsLogProvider`] (`GitLogProvider`). One provider serves all git roots.
pub struct GitLogProvider {
    git: Arc<Git>,
    repository_manager: Arc<GitRepositoryManager>,
    ref_manager: Arc<dyn VcsLogRefManager>,
    users: Mutex<std::collections::HashMap<PathBuf, VcsUser>>,
}

impl GitLogProvider {
    pub fn new(git: Arc<Git>) -> GitLogProvider {
        GitLogProvider::with_repository_manager(Arc::new(GitRepositoryManager::new(git)))
    }

    pub fn with_repository_manager(
        repository_manager: Arc<GitRepositoryManager>,
    ) -> GitLogProvider {
        GitLogProvider {
            git: repository_manager.git().clone(),
            ref_manager: Arc::new(GitRefManager::new(repository_manager.clone())),
            repository_manager,
            users: Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn repository_manager(&self) -> &Arc<GitRepositoryManager> {
        &self.repository_manager
    }

    /// `readCurrentTagNames`.
    fn read_current_tag_names(&self, root: &Path) -> Result<HashSet<String>, VcsException> {
        Ok(get_all_tags(&self.git, root)
            .map_err(vcs_error)?
            .into_iter()
            .collect())
    }

    /// `loadSomeCommitsOnTaggedBranches`: up to `commit_count` commits from each tag.
    fn load_some_commits_on_tagged_branches(
        &self,
        root: &VcsRoot,
        commit_count: usize,
        unmatched_tags: &[String],
    ) -> Result<DetailedLogData, VcsException> {
        let max_count = format!("--max-count={commit_count}");
        let mut refs: Vec<VcsRef> = Vec::new();
        let mut commits: Vec<VcsCommitMetadata> = Vec::new();
        let mut seen_commits = HashSet::new();
        for tag in unmatched_tags {
            let log_data = collect_metadata_with_params(&self.git, root, &[&max_count, tag])
                .map_err(vcs_error)?;
            for r in log_data.refs {
                if !refs.contains(&r) {
                    refs.push(r);
                }
            }
            for c in log_data.commits {
                if seen_commits.insert(c.clone()) {
                    commits.push(c);
                }
            }
        }
        Ok(DetailedLogData { commits, refs })
    }

    /// `GitUserRegistry.readCurrentUser`.
    fn read_current_user(&self, root: &Path) -> Option<VcsUser> {
        let user_name = self
            .get_config_value(root, USER_NAME)
            .filter(|n| !n.is_empty())?;
        let user_email = self.get_config_value(root, USER_EMAIL).unwrap_or_default();
        Some(VcsUser::create_user(&user_name, &user_email))
    }

    /// `GitConfigUtil.getValue`: `git config --null --get <key>`; `None` if unset.
    fn get_config_value(&self, root: &Path, key: &str) -> Option<String> {
        let mut handler = self.git.handler(root, GitCommand::CONFIG);
        handler.set_silent(true);
        handler.add_parameters(["--null", "--get", key]);
        let result = self.git.run_command(handler);
        if !result.success() {
            return None;
        }
        let output = result.output_as_joined_string();
        output.find('\0').map(|pos| output[..pos].to_string())
    }
}

/// `createBranchesRefs`: local and remote branches and HEAD of the repository state.
fn create_branches_refs(info: &GitRepoInfo, root: &VcsRoot) -> Vec<VcsRef> {
    let mut locals: Vec<_> = info.state.local_branches.iter().collect();
    locals.sort_by(|a, b| a.0.name().cmp(b.0.name()));
    let mut remotes: Vec<_> = info
        .state
        .remote_branches
        .iter()
        .map(|(b, h)| (b.name(), h))
        .collect();
    remotes.sort();
    let to_hash = |h: &crate::hash::Hash| Hash::build(h.as_str());
    let mut refs: Vec<VcsRef> = locals
        .into_iter()
        .map(|(branch, hash)| VcsRef::new(to_hash(hash), branch.name(), LOCAL_BRANCH, root.clone()))
        .collect();
    refs.extend(
        remotes
            .into_iter()
            .map(|(name, hash)| VcsRef::new(to_hash(hash), &name, REMOTE_BRANCH, root.clone())),
    );
    if let Some(revision) = &info.state.current_revision {
        refs.push(VcsRef::new(
            Hash::build(revision),
            "HEAD",
            HEAD,
            root.clone(),
        ));
    }
    refs
}

impl VcsLogProvider for GitLogProvider {
    fn read_first_block(
        &self,
        root: &VcsRoot,
        requirements: &Requirements<'_>,
    ) -> Result<DetailedLogData, VcsException> {
        if self
            .repository_manager
            .get_repository_unless_fresh(root)
            .is_none()
        {
            return Ok(DetailedLogData::default());
        }

        // need to query more to sort them manually; this doesn't affect performance: it is equal
        // for -1000 and -2000
        let commit_count = requirements.commit_count * 2;
        let max_count = format!("--max-count={commit_count}");
        // NB: not specifying --tags, because it introduces great slowdown if there are many tags,
        // but makes sense only if there are heads without branch or HEAD labels (rare case).
        // Such cases are partially handled below.
        let params = ["HEAD", "--branches", "--remotes", &max_count];

        let (refresh, is_refresh_refs) = requirements
            .ex
            .as_ref()
            .map_or((false, false), |ex| (ex.refresh, ex.refresh_refs));

        let data = collect_metadata_with_params(&self.git, root, &params).map_err(vcs_error)?;

        let safe_refs = data.refs;
        let mut all_refs = RefsByNameAndType::from_refs(safe_refs.iter().cloned());
        if is_refresh_refs {
            let repository = self.repository_manager.update(root);
            for branch in create_branches_refs(&repository, root) {
                all_refs.add(branch);
            }
        }

        let mut all_details: Vec<VcsCommitMetadata> = data.commits;
        if let (true, true, Some(ex)) = (refresh, is_refresh_refs, requirements.ex.as_ref()) {
            // on refresh: get new tags, which point to commits not from the first block; then get
            // history, walking down just from these tags. On init: just ignore such tagged-only
            // branches. The price for speed-up.
            let current_tag_names = self.read_current_tag_names(root)?;
            add_old_still_existing_tags(&mut all_refs, &current_tag_names, ex.previous_refs);

            let mut seen: HashSet<VcsCommitMetadata> = HashSet::new();
            all_details.retain(|c| seen.insert(c.clone()));

            let previous_tags: HashSet<&str> = ex.previous_refs.tags().map(|r| r.name()).collect();
            let safe_tags: HashSet<&str> = safe_refs
                .iter()
                .filter(|r| r.ref_type() == TAG)
                .map(|r| r.name())
                .collect();
            let mut new_unmatched_tags: Vec<String> = current_tag_names
                .iter()
                .filter(|t| !previous_tags.contains(t.as_str()) && !safe_tags.contains(t.as_str()))
                .cloned()
                .collect();
            new_unmatched_tags.sort();

            if !new_unmatched_tags.is_empty() {
                let commits_from_tags = self.load_some_commits_on_tagged_branches(
                    root,
                    commit_count,
                    &new_unmatched_tags,
                )?;
                for commit in commits_from_tags.commits {
                    if seen.insert(commit.clone()) {
                        all_details.push(commit);
                    }
                }
                for r in commits_from_tags.refs {
                    all_refs.add(r);
                }
            }
        }

        let mut sorted_commits = sort_by_date_topo_order(all_details);
        sorted_commits.truncate(requirements.commit_count);
        Ok(DetailedLogData {
            commits: sorted_commits,
            refs: all_refs.into_vec(),
        })
    }

    fn read_all_hashes(
        &self,
        root: &VcsRoot,
        commit_consumer: &mut dyn FnMut(TimedVcsCommit),
    ) -> Result<LogData, VcsException> {
        if self
            .repository_manager
            .get_repository_unless_fresh(root)
            .is_none()
        {
            return Ok(LogData::default());
        }
        let mut parameters: Vec<&str> = LOG_ALL.to_vec();
        parameters.push("--date-order");

        let mut users = HashSet::new();
        let mut refs = HashSet::new();
        read_timed_commits(
            &self.git,
            root,
            &[],
            &parameters,
            Some(&mut |user| {
                users.insert(user);
            }),
            Some(&mut |vcs_ref| {
                refs.insert(vcs_ref);
            }),
            commit_consumer,
        )
        .map_err(vcs_error)?;
        Ok(LogData { refs, users })
    }

    fn read_metadata(
        &self,
        root: &VcsRoot,
        hashes: &[String],
        consumer: &mut dyn FnMut(VcsCommitMetadata),
    ) -> Result<(), VcsException> {
        collect_metadata(&self.git, root, hashes, consumer).map_err(vcs_error)
    }

    fn reference_manager(&self) -> Arc<dyn VcsLogRefManager> {
        self.ref_manager.clone()
    }

    /// `GitUserRegistry.getOrReadUser`: cached once read.
    fn get_current_user(&self, root: &VcsRoot) -> Result<Option<VcsUser>, VcsException> {
        let mut users = self.users.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(user) = users.get(&**root) {
            return Ok(Some(user.clone()));
        }
        let user = self.read_current_user(root);
        if let Some(user) = &user {
            users.insert(root.to_path_buf(), user.clone());
        }
        Ok(user)
    }

    fn get_current_branch(&self, root: &VcsRoot) -> Option<String> {
        let repository = self
            .repository_manager
            .get_repository_for_root_quick(root)?;
        match repository.current_branch_name() {
            None if repository.state.current_revision.is_some() => Some("HEAD".to_string()),
            name => name.map(str::to_string),
        }
    }

    fn is_full_hash(&self, root: &VcsRoot, hash: &str) -> bool {
        let Some(repository) = self.repository_manager.get_repository_for_root_quick(root) else {
            return false;
        };
        hash.len() == repository.full_hash_length && hash.bytes().all(|b| b.is_ascii_hexdigit())
    }
}

/// `addOldStillExistingTags`.
fn add_old_still_existing_tags(
    all_refs: &mut RefsByNameAndType,
    current_tags: &HashSet<String>,
    previous_refs: &dyn VcsRefsContainer,
) {
    for vcs_ref in previous_refs.all_refs() {
        if !all_refs.contains(vcs_ref) && current_tags.contains(vcs_ref.name()) {
            all_refs.add(vcs_ref.clone());
        }
    }
}
```

`crates/ig-git/src/log/record.rs`:

```rust
//! One commit record of `git log` output, and the builders that create records.
//!
//! Port of `plugins/git4idea/backend/src/history/GitLogRecord.java`, `GitLogFullRecord.java`,
//! `GitLogRecordBuilder.kt`, `GitChangeType.java` and `GitChangesParser.getChangeType`.
//!
//! Like upstream, accessors return an empty string (or 0) for a value that wasn't requested.
//! Deviations: no `setUsedHandler` (it only enriches log messages); a full record keeps its
//! `--name-status` entries but doesn't build `Change` objects (`parseChanges`, out of scope until
//! commit details are ported); path interning (`InternedGitLogRecordBuilder`) is not needed.

use crate::log::parser::{GitFormatError, GitLogOption, OptionValues};

/// The kind of change of a file (`Change.Type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeType {
    Modification,
    New,
    Deleted,
    Moved,
}

/// `GitChangesParser.getChangeType(GitChangeType.fromString(status))`: the type of a
/// `--name-status` status (`M`, `A`, `C..`, `D`, `R..`, `U`, `T`).
pub fn change_type_from_status(status: &str) -> Result<ChangeType, GitFormatError> {
    match status.chars().next() {
        Some('A') => Ok(ChangeType::New),
        Some('T') | Some('M') => Ok(ChangeType::Modification),
        Some('D') => Ok(ChangeType::Deleted),
        Some('C') | Some('R') => Ok(ChangeType::Moved),
        // upstream logs an error for UNRESOLVED and treats it as a modification
        Some('U') => Ok(ChangeType::Modification),
        _ => Err(GitFormatError(format!("Unexpected status [{status}]"))),
    }
}

/// A `--name-status` entry (`VcsFileStatusInfo`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VcsFileStatusInfo {
    pub change_type: ChangeType,
    pub first_path: String,
    pub second_path: Option<String>,
}

/// Creates records from the parsed options (`GitLogRecordBuilder`).
pub trait GitLogRecordBuilder {
    type Record;

    fn add_path(
        &mut self,
        change_type: ChangeType,
        first_path: String,
        second_path: Option<String>,
    );

    fn build(&mut self, options: OptionValues, supports_raw_body: bool) -> Self::Record;

    fn clear(&mut self);
}

/// Builds [`GitLogRecord`]s (`DefaultGitLogRecordBuilder`).
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultGitLogRecordBuilder;

impl GitLogRecordBuilder for DefaultGitLogRecordBuilder {
    type Record = GitLogRecord;

    /// # Panics
    /// Always: records without `--name-status` have no paths (upstream `UnsupportedOperationException`).
    fn add_path(&mut self, _: ChangeType, _: String, _: Option<String>) {
        panic!("Can not add paths to GitLogRecord");
    }

    fn build(&mut self, options: OptionValues, supports_raw_body: bool) -> GitLogRecord {
        GitLogRecord {
            options,
            supports_raw_body,
        }
    }

    fn clear(&mut self) {}
}

/// Builds [`GitLogFullRecord`]s with their `--name-status` entries (`DefaultGitLogFullRecordBuilder`).
#[derive(Debug, Default, Clone)]
pub struct DefaultGitLogFullRecordBuilder {
    statuses: Vec<VcsFileStatusInfo>,
}

impl GitLogRecordBuilder for DefaultGitLogFullRecordBuilder {
    type Record = GitLogFullRecord;

    fn add_path(
        &mut self,
        change_type: ChangeType,
        first_path: String,
        second_path: Option<String>,
    ) {
        self.statuses.push(VcsFileStatusInfo {
            change_type,
            first_path,
            second_path,
        });
    }

    fn build(&mut self, options: OptionValues, supports_raw_body: bool) -> GitLogFullRecord {
        GitLogFullRecord {
            record: GitLogRecord {
                options,
                supports_raw_body,
            },
            statuses: std::mem::take(&mut self.statuses),
        }
    }

    fn clear(&mut self) {
        self.statuses = Vec::new();
    }
}

/// The information of one commit (`GitLogRecord`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLogRecord {
    options: OptionValues,
    supports_raw_body: bool,
}

impl GitLogRecord {
    pub fn new(options: OptionValues, supports_raw_body: bool) -> GitLogRecord {
        GitLogRecord {
            options,
            supports_raw_body,
        }
    }

    /// The value of an option, `None` if it wasn't requested.
    pub fn option(&self, option: GitLogOption) -> Option<&str> {
        self.options[option.index()].as_deref()
    }

    fn lookup(&self, option: GitLogOption) -> &str {
        self.option(option).unwrap_or("")
    }

    pub fn hash(&self) -> &str {
        self.lookup(GitLogOption::Hash)
    }

    pub fn tree_hash(&self) -> &str {
        self.lookup(GitLogOption::Tree)
    }

    pub fn author_name(&self) -> &str {
        self.lookup(GitLogOption::AuthorName)
    }

    pub fn author_email(&self) -> &str {
        self.lookup(GitLogOption::AuthorEmail)
    }

    pub fn committer_name(&self) -> &str {
        self.lookup(GitLogOption::CommitterName)
    }

    pub fn committer_email(&self) -> &str {
        self.lookup(GitLogOption::CommitterEmail)
    }

    pub fn subject(&self) -> &str {
        self.lookup(GitLogOption::Subject)
    }

    pub fn body(&self) -> &str {
        self.lookup(GitLogOption::Body)
    }

    pub fn raw_body(&self) -> &str {
        self.lookup(GitLogOption::RawBody)
    }

    pub fn shortened_ref_log(&self) -> &str {
        self.lookup(GitLogOption::ShortRefLogSelector)
    }

    /// `getCommitTime` (`getDate`): milliseconds, 0 if missing or malformed.
    pub fn commit_time(&self) -> i64 {
        parse_time(self.option(GitLogOption::CommitTime)).unwrap_or(0)
    }

    /// `getAuthorTimeStamp`: milliseconds, 0 if missing or malformed.
    pub fn author_time_stamp(&self) -> i64 {
        parse_time(self.option(GitLogOption::AuthorTime)).unwrap_or(0)
    }

    /// `getFullMessage`: the raw body, or subject and body for git without `%B`, trailing
    /// whitespace stripped.
    pub fn full_message(&self) -> String {
        let message = if self.supports_raw_body {
            self.raw_body().to_string()
        } else {
            format!("{}\n\n{}", self.subject(), self.body())
        };
        message.trim_end().to_string()
    }

    /// `getParentsHashes`.
    pub fn parents_hashes(&self) -> Vec<&str> {
        let parents = self.lookup(GitLogOption::Parents);
        if parents.trim().is_empty() {
            return Vec::new();
        }
        java_split(parents, " ")
    }

    /// `getRefs`: the ref names of `%d` (with `--decorate=full`: `HEAD`, `refs/heads/...`, ...).
    pub fn refs(&self) -> Vec<String> {
        parse_ref_names(self.option(GitLogOption::RefNames))
    }

    pub fn options(&self) -> &OptionValues {
        &self.options
    }

    pub fn is_supports_raw_body(&self) -> bool {
        self.supports_raw_body
    }
}

/// A record with its `--name-status` entries (`GitLogFullRecord`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitLogFullRecord {
    record: GitLogRecord,
    statuses: Vec<VcsFileStatusInfo>,
}

impl GitLogFullRecord {
    pub fn new(
        options: OptionValues,
        statuses: Vec<VcsFileStatusInfo>,
        supports_raw_body: bool,
    ) -> GitLogFullRecord {
        GitLogFullRecord {
            record: GitLogRecord::new(options, supports_raw_body),
            statuses,
        }
    }

    pub fn record(&self) -> &GitLogRecord {
        &self.record
    }

    pub fn status_infos(&self) -> &[VcsFileStatusInfo] {
        &self.statuses
    }

    /// `getPaths`: the distinct paths, in order.
    pub fn paths(&self) -> Vec<&str> {
        let mut result: Vec<&str> = Vec::new();
        for info in &self.statuses {
            for path in std::iter::once(info.first_path.as_str()).chain(info.second_path.as_deref())
            {
                if !result.contains(&path) {
                    result.push(path);
                }
            }
        }
        result
    }
}

impl std::ops::Deref for GitLogFullRecord {
    type Target = GitLogRecord;

    fn deref(&self) -> &GitLogRecord {
        &self.record
    }
}

/// `GitLogUtil.parseTime`: seconds to milliseconds.
pub fn parse_time(time: Option<&str>) -> Option<i64> {
    time?.trim().parse::<i64>().ok().map(|t| t * 1000)
}

/// Java `String.split(regex)` for a literal separator: trailing empty strings are dropped.
fn java_split<'a>(s: &'a str, separator: &str) -> Vec<&'a str> {
    let mut parts: Vec<&str> = s.split(separator).collect();
    while parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    if parts == [""] && !s.is_empty() {
        parts.clear();
    }
    parts
}

/// `parseRefNames`.
fn parse_ref_names(decoration: Option<&str>) -> Vec<String> {
    let Some(decoration) = decoration else {
        return Vec::new();
    };
    let (Some(start), Some(end)) = (decoration.find('('), decoration.rfind(')')) else {
        return Vec::new();
    };
    if end < start + 1 {
        return Vec::new();
    }
    let refs = &decoration[start + 1..end];
    let mut result = Vec::new();
    for item in java_split(refs, ", ") {
        const POINTER: &str = " -> "; // HEAD -> refs/heads/master in Git 2.4.3+
        if item.contains(POINTER) {
            result.extend(
                item.split(POINTER)
                    .filter(|part| !part.is_empty())
                    .map(|part| part.trim().to_string()),
            );
        } else {
            // tags have the "tag:" prefix
            let raw = match item.find(':') {
                Some(colon) if colon > 0 => item[colon + 1..].trim(),
                _ => item,
            };
            result.push(raw.to_string());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record_with(option: GitLogOption, value: &str) -> GitLogRecord {
        let mut options: OptionValues = Default::default();
        options[option.index()] = Some(value.to_string());
        GitLogRecord::new(options, true)
    }

    #[test]
    fn ref_names_in_old_and_new_formats() {
        let r = record_with(
            GitLogOption::RefNames,
            " (HEAD -> refs/heads/master, tag: refs/tags/v1, refs/remotes/origin/HEAD)",
        );
        assert_eq!(
            r.refs(),
            [
                "HEAD",
                "refs/heads/master",
                "refs/tags/v1",
                "refs/remotes/origin/HEAD"
            ]
        );
        assert!(record_with(GitLogOption::RefNames, "").refs().is_empty());
        assert!(record_with(GitLogOption::Hash, "a").refs().is_empty());
    }

    #[test]
    fn parents_times_and_missing_values() {
        let r = record_with(GitLogOption::Parents, "a b");
        assert_eq!(r.parents_hashes(), ["a", "b"]);
        assert!(
            record_with(GitLogOption::Parents, " ")
                .parents_hashes()
                .is_empty()
        );
        assert_eq!(
            record_with(GitLogOption::CommitTime, " 12 ").commit_time(),
            12_000
        );
        assert_eq!(record_with(GitLogOption::CommitTime, "x").commit_time(), 0);
        assert_eq!(r.subject(), "");
        assert_eq!(r.option(GitLogOption::Subject), None);
    }

    #[test]
    fn full_message_without_raw_body() {
        let mut options: OptionValues = Default::default();
        options[GitLogOption::Subject.index()] = Some("s".into());
        options[GitLogOption::Body.index()] = Some("b\n\n".into());
        assert_eq!(GitLogRecord::new(options, false).full_message(), "s\n\nb");
    }

    #[test]
    fn status_types() {
        assert_eq!(change_type_from_status("R100").unwrap(), ChangeType::Moved);
        assert_eq!(
            change_type_from_status("T").unwrap(),
            ChangeType::Modification
        );
        assert!(change_type_from_status("X").is_err());
    }
}
```

`crates/ig-git/src/log/ref_manager.rs`:

```rust
//! Reference types and ordering for git.
//!
//! Port of `plugins/git4idea/backend/src/log/GitRefManager.kt`: ref types, `getRefType`, the label
//! order comparator and the branch layout comparator.
//!
//! Not ported (UI or other plans): ref groups for the table and the branch filter
//! (`groupForTable`, `groupForBranchFilter`, tracked groups), favorites, ref type serialization
//! and the label colors.

use std::cmp::Ordering;
use std::sync::Arc;

use ig_log::data::{VcsLogRefManager, VcsRef, VcsRefType, compare_roots};

use crate::log::repository_manager::GitRepositoryManager;
use crate::natural_comparator::natural_compare;
use crate::refs::{REFS_HEADS_PREFIX, REFS_REMOTES_PREFIX, REFS_TAGS_PREFIX};

pub const HEAD: VcsRefType = VcsRefType::new("HEAD", true);
pub const LOCAL_BRANCH: VcsRefType = VcsRefType::new("LOCAL_BRANCH", true);
pub const REMOTE_BRANCH: VcsRefType = VcsRefType::new("REMOTE_BRANCH", true);
pub const TAG: VcsRefType = VcsRefType::new("TAG", false);
pub const OTHER: VcsRefType = VcsRefType::new("OTHER", false);

pub const MASTER: &str = "master";
pub const MAIN: &str = "main";
pub const ORIGIN_MASTER: &str = "origin/master";
pub const ORIGIN_MAIN: &str = "origin/main";

/// `getRefType`: the type of a full ref name (`refs/heads/...`, `refs/remotes/...`,
/// `refs/tags/...`, `HEAD...`).
pub fn get_ref_type(ref_name: &str) -> VcsRefType {
    if ref_name.starts_with(REFS_HEADS_PREFIX) {
        return LOCAL_BRANCH;
    }
    if ref_name.starts_with(REFS_REMOTES_PREFIX) {
        return REMOTE_BRANCH;
    }
    if ref_name.starts_with(REFS_TAGS_PREFIX) {
        return TAG;
    }
    if ref_name.starts_with("HEAD") {
        return HEAD;
    }
    OTHER
}

/// `GitRefManager.RefType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RefType {
    Other,
    Head,
    CurrentBranch,
    Tag,
    LocalBranch,
    Master,
    RemoteBranch,
    OriginMaster,
}

/// `GitLabelComparator.orderedTypes`.
const LABEL_ORDER: [RefType; 8] = [
    RefType::Head,
    RefType::CurrentBranch,
    RefType::Master,
    RefType::OriginMaster,
    RefType::LocalBranch,
    RefType::RemoteBranch,
    RefType::Tag,
    RefType::Other,
];

/// `GitBranchLayoutComparator.orderedTypes`.
const BRANCH_LAYOUT_ORDER: [RefType; 8] = [
    RefType::OriginMaster,
    RefType::RemoteBranch,
    RefType::Master,
    RefType::LocalBranch,
    RefType::Tag,
    RefType::CurrentBranch,
    RefType::Head,
    RefType::Other,
];

/// `GitRefComparator.getType`.
fn base_ref_type(vcs_ref: &VcsRef) -> RefType {
    let ref_type = vcs_ref.ref_type();
    let name = vcs_ref.name();
    if ref_type == HEAD {
        RefType::Head
    } else if ref_type == TAG {
        RefType::Tag
    } else if ref_type == LOCAL_BRANCH {
        if name == MASTER || name == MAIN {
            RefType::Master
        } else {
            RefType::LocalBranch
        }
    } else if ref_type == REMOTE_BRANCH {
        if name == ORIGIN_MASTER || name == ORIGIN_MAIN {
            RefType::OriginMaster
        } else {
            RefType::RemoteBranch
        }
    } else {
        RefType::Other
    }
}

/// `GitRefComparator.compare`: by type power, then by name (natural order), then by root.
fn compare_refs(
    ordered_types: &[RefType; 8],
    get_type: impl Fn(&VcsRef) -> RefType,
    ref1: &VcsRef,
    ref2: &VcsRef,
) -> Ordering {
    let power = |r: &VcsRef| ordered_types.iter().position(|t| *t == get_type(r));
    let (power1, power2) = (power(ref1), power(ref2));
    if power1 != power2 {
        return power1.cmp(&power2);
    }
    let names_comparison = natural_compare(ref1.name(), ref2.name());
    if names_comparison != Ordering::Equal {
        return names_comparison;
    }
    compare_roots(ref1.root(), ref2.root())
}

/// The git [`VcsLogRefManager`] (`GitRefManager`).
pub struct GitRefManager {
    repository_manager: Arc<GitRepositoryManager>,
}

impl GitRefManager {
    pub fn new(repository_manager: Arc<GitRepositoryManager>) -> GitRefManager {
        GitRefManager { repository_manager }
    }

    /// `GitLabelComparator.isCurrentBranch`.
    fn is_current_branch(&self, vcs_ref: &VcsRef) -> bool {
        let Some(repo) = self
            .repository_manager
            .get_repository_for_root_quick(vcs_ref.root())
        else {
            return false;
        };
        repo.current_branch_name() == Some(vcs_ref.name())
    }

    /// `GitLabelComparator.getType`: local branches (including master) that are current.
    fn label_ref_type(&self, vcs_ref: &VcsRef) -> RefType {
        let ref_type = base_ref_type(vcs_ref);
        if (ref_type == RefType::LocalBranch || ref_type == RefType::Master)
            && self.is_current_branch(vcs_ref)
        {
            return RefType::CurrentBranch;
        }
        ref_type
    }
}

impl VcsLogRefManager for GitRefManager {
    fn branch_layout_compare(&self, ref1: &VcsRef, ref2: &VcsRef) -> Ordering {
        compare_refs(&BRANCH_LAYOUT_ORDER, base_ref_type, ref1, ref2)
    }

    fn labels_order_compare(&self, ref1: &VcsRef, ref2: &VcsRef) -> Ordering {
        compare_refs(&LABEL_ORDER, |r| self.label_ref_type(r), ref1, ref2)
    }

    fn name(&self) -> String {
        "GitRefManager".to_string()
    }
}
```

`crates/ig-git/src/log/repository_manager.rs`:

```rust
//! The repository state the log provider and reference manager rely on.
//!
//! A minimal stand-in for `GitRepositoryManager` / `GitRepository.update()`
//! (`plugins/git4idea/backend/src/repo/GitRepositoryManager.kt`, `GitRepositoryImpl.kt`): per
//! root, the branch state read by [`GitRepositoryReader`] plus the remotes and the hash length.
//! Upstream updates repositories on file-system events; here [`GitRepositoryManager::update`] is
//! called by the log provider when it reads the first block with refs, and by the UI as needed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use crate::config::{get_values, remotes_from_values};
use crate::reader::{GitBranchState, GitRepositoryReader};
use crate::repo_files::{GitRepositoryFiles, find_git_dir};
use crate::runner::Git;

/// The state of one repository (`GitRepoInfo`, partial).
#[derive(Debug, Clone)]
pub struct GitRepoInfo {
    pub root: PathBuf,
    pub state: GitBranchState,
    pub remotes: Vec<crate::config::GitRemote>,
    /// 40 for SHA-1, 64 for SHA-256 repositories (`GitRepository.fullHashLength`).
    pub full_hash_length: usize,
}

impl GitRepoInfo {
    /// `Repository.isFresh`: no commits yet.
    pub fn is_fresh(&self) -> bool {
        self.state.current_revision.is_none()
    }

    /// `getCurrentBranchName`.
    pub fn current_branch_name(&self) -> Option<&str> {
        self.state.current_branch.as_ref().map(|b| b.name())
    }

    /// Reads the state of the repository at `root`.
    pub fn read(git: &Git, root: &Path) -> GitRepoInfo {
        let git_dir = find_git_dir(root).unwrap_or_else(|| root.to_path_buf());
        let files = GitRepositoryFiles::create(root, &git_dir);
        let config = get_values(git, root).ok();
        let remotes = config.as_ref().map(remotes_from_values).unwrap_or_default();
        let object_format = config
            .as_ref()
            .and_then(|c| c.get("extensions.objectformat"))
            .and_then(|values| values.last().cloned());
        let full_hash_length = match object_format.as_deref() {
            Some(format) if format.eq_ignore_ascii_case("sha256") => 64,
            _ => 40,
        };
        let state = GitRepositoryReader::new(&files, git).read_state(&remotes);
        GitRepoInfo {
            root: root.to_path_buf(),
            state,
            remotes,
            full_hash_length,
        }
    }
}

/// Caches [`GitRepoInfo`] per root.
pub struct GitRepositoryManager {
    git: Arc<Git>,
    repositories: RwLock<HashMap<PathBuf, Arc<GitRepoInfo>>>,
}

impl GitRepositoryManager {
    pub fn new(git: Arc<Git>) -> GitRepositoryManager {
        GitRepositoryManager {
            git,
            repositories: RwLock::new(HashMap::new()),
        }
    }

    pub fn git(&self) -> &Arc<Git> {
        &self.git
    }

    /// `getRepositoryForRoot`: the cached state, read on first use.
    pub fn get_repository(&self, root: &Path) -> Arc<GitRepoInfo> {
        if let Some(info) = self.get_repository_for_root_quick(root) {
            return info;
        }
        self.update(root)
    }

    /// `getRepositoryForRootQuick`: the cached state only.
    pub fn get_repository_for_root_quick(&self, root: &Path) -> Option<Arc<GitRepoInfo>> {
        self.repositories
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(root)
            .cloned()
    }

    /// `getRepositoryUnlessFresh`: `None` for a repository without commits.
    pub fn get_repository_unless_fresh(&self, root: &Path) -> Option<Arc<GitRepoInfo>> {
        let info = self.get_repository(root);
        (!info.is_fresh()).then_some(info)
    }

    /// `GitRepository.update()`: rereads the state.
    pub fn update(&self, root: &Path) -> Arc<GitRepoInfo> {
        let info = Arc::new(GitRepoInfo::read(&self.git, root));
        self.repositories
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(root.to_path_buf(), info.clone());
        info
    }
}
```

`crates/ig-git/src/log/splitter.rs`:

```rust
//! Feeding `git log` output line by line to a [`GitLogParser`], without storing it.
//!
//! Port of `plugins/git4idea/backend/src/history/GitLogOutputSplitter.java`.

use crate::console::OutputKind;
use crate::error::{GitError, Result};
use crate::handler::GitHandler;
use crate::log::parser::GitLogParser;
use crate::log::record::GitLogRecordBuilder;
use crate::runner::{Git, cleanup_error_prefixes};

/// Parses stdout lines into records passed to a consumer; collects stderr for the error message
/// (`GitLogOutputSplitter`, a `GitLineHandlerListener`).
pub struct GitLogOutputSplitter<'p, B: GitLogRecordBuilder, F: FnMut(B::Record)> {
    printable_command_line: String,
    parser: &'p mut GitLogParser<B>,
    record_consumer: F,
    errors: String,
    exception: Option<GitError>,
}

impl<'p, B: GitLogRecordBuilder, F: FnMut(B::Record)> GitLogOutputSplitter<'p, B, F> {
    pub fn new(handler: &GitHandler, parser: &'p mut GitLogParser<B>, record_consumer: F) -> Self {
        GitLogOutputSplitter {
            printable_command_line: handler.printable_command_line(),
            parser,
            record_consumer,
            errors: String::new(),
            exception: None,
        }
    }

    /// `onLineAvailable`.
    pub fn on_line_available(&mut self, line: &str, kind: OutputKind) {
        match kind {
            OutputKind::Stderr => {
                self.errors.push_str(&cleanup_error_prefixes(line));
                self.errors.push('\n');
            }
            OutputKind::Stdout => {
                if let Err(e) = self.process_output_line(line) {
                    self.exception = Some(e);
                }
            }
        }
    }

    fn process_output_line(&mut self, line: &str) -> Result<()> {
        match self.parser.parse_line(line) {
            Ok(Some(record)) => {
                (self.record_consumer)(record);
                Ok(())
            }
            Ok(None) => Ok(()),
            Err(e) => {
                self.parser.clear();
                Err(GitError::Command(format!(
                    "Error while parsing line \"{}\": {e}",
                    line.escape_default()
                )))
            }
        }
    }

    /// `processTerminated`.
    pub fn process_terminated(&mut self, exit_code: i32) {
        if exit_code != 0 {
            let mut error_message = self.errors.clone();
            if error_message.is_empty() {
                error_message = format!("The Git process exited with the code {exit_code}");
            }
            self.exception = Some(GitError::Command(format!(
                "{error_message}\nCommand line: [{}]",
                self.printable_command_line
            )));
        } else if let Some(record) = self.parser.finish() {
            (self.record_consumer)(record);
        }
    }

    /// `startFailed`.
    pub fn start_failed(&mut self, error: String) {
        self.exception = Some(GitError::Command(error));
    }

    pub fn has_errors(&self) -> bool {
        self.exception.is_some()
    }

    /// `reportErrors`.
    pub fn report_errors(self) -> Result<()> {
        match self.exception {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// Runs `handler` with a splitter: `runCommandWithoutCollectingOutput(handler).throwOnError()`
/// followed by `reportErrors()`.
pub fn run_and_split<B: GitLogRecordBuilder>(
    git: &Git,
    handler: GitHandler,
    parser: &mut GitLogParser<B>,
    record_consumer: impl FnMut(B::Record),
) -> Result<()> {
    let mut splitter = GitLogOutputSplitter::new(&handler, parser, record_consumer);
    let result = git.run_command_without_collecting_output(handler, &mut |line, kind| {
        splitter.on_line_available(line, kind)
    });
    if result.start_failed {
        splitter.start_failed(result.error_output_as_joined_string());
    } else {
        splitter.process_terminated(result.exit_code);
    }
    result.throw_on_error()?;
    splitter.report_errors()
}
```

`crates/ig-git/src/log/unescape.rs`:

```rust
//! Unquoting paths printed by git.
//!
//! Port of `platform/vcs-impl/src/com/intellij/vcsUtil/VcsFileUtil.java#unescapeGitPath` (used by
//! `GitUtil.unescapePath`). Octal escapes are UTF-8 bytes (upstream: the default charset).

/// An escape sequence is unterminated or unknown (upstream `IllegalArgumentException`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct UnescapeError(pub String);

/// Strips surrounding quotes and decodes `\\`, `\t`, `\n`, `\r`, `\a`, `\b`, `\f`, `\"` and octal
/// byte sequences.
pub fn unescape_git_path(path: &str) -> Result<String, UnescapeError> {
    let original = path;
    let path = if path.len() >= 2 && path.starts_with('"') && path.ends_with('"') {
        &path[1..path.len() - 1]
    } else if path == "\"" {
        // upstream: `startsWith` and `endsWith` both hold for a lone quote; substring(1, 0) throws
        return Err(UnescapeError(format!("Invalid quoted path: {original}")));
    } else {
        path
    };
    if !path.contains('\\') {
        return Ok(path.to_string());
    }

    let chars: Vec<char> = path.chars().collect();
    let l = chars.len();
    let is_octal = |c: char| ('0'..='7').contains(&c);
    let mut rc = String::with_capacity(path.len());
    let mut i = 0;
    while i < l {
        let c = chars[i];
        if c != '\\' {
            rc.push(c);
            i += 1;
            continue;
        }
        i += 1;
        if i >= l {
            return Err(UnescapeError(format!(
                "Unterminated escape sequence in the path: {path}"
            )));
        }
        let e = chars[i];
        match e {
            '\\' => rc.push('\\'),
            't' => rc.push('\t'),
            'n' => rc.push('\n'),
            'r' => rc.push('\r'),
            'a' => rc.push('\u{7}'),
            'b' => rc.push('\u{8}'),
            'f' => rc.push('\u{c}'),
            '"' => rc.push('"'),
            _ if is_octal(e) => {
                // a sequence of octal escapes is one byte array
                let mut bytes = Vec::new();
                while i < l {
                    if is_octal(chars[i]) {
                        let mut code: u32 = 0;
                        let mut k = 0;
                        while k < 3 && i < l && is_octal(chars[i]) {
                            code = code * 8 + (chars[i] as u32 - '0' as u32);
                            i += 1;
                            k += 1;
                        }
                        bytes.push(code as u8);
                    }
                    if i + 1 >= l || chars[i] != '\\' || !is_octal(chars[i + 1]) {
                        break;
                    }
                    i += 1;
                }
                rc.push_str(&String::from_utf8_lossy(&bytes));
                continue;
            }
            other => {
                return Err(UnescapeError(format!(
                    "Unknown escape sequence '\\{other}' in the path: {path}"
                )));
            }
        }
        i += 1;
    }
    Ok(rc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescapes_quoted_paths() {
        assert_eq!(unescape_git_path("plain.txt").unwrap(), "plain.txt");
        assert_eq!(
            unescape_git_path("\"a\\tb\\\\c\\\"d\"").unwrap(),
            "a\tb\\c\"d"
        );
        assert_eq!(
            unescape_git_path("\"\\320\\264\\320\\260.txt\"").unwrap(),
            "да.txt"
        );
        assert_eq!(unescape_git_path("\"x\\303\\251y\"").unwrap(), "xéy");
        assert!(unescape_git_path("\"a\\\"").is_err());
        assert!(unescape_git_path("a\\q").is_err());
    }
}
```

`crates/ig-git/src/log/util.rs`:

```rust
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
    if let Err(e) = result {
        if commits.is_empty() {
            return Err(e);
        }
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

/// `GitBranchUtil.getAllTags`: `git tag -l`.
pub fn get_all_tags(git: &Git, root: &Path) -> Result<Vec<String>> {
    let mut handler = git.handler(root, GitCommand::TAG);
    handler.add_parameters(["-l"]);
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
```

- [ ] **Step 3: Replace `crates/ig-git/src/lib.rs`**

```rust
//! Git core for IdeaGit: a port of IntelliJ's `git4idea` command and repository layers.
//! See `reference/upstream-map.toml` for the upstream files each module is ported from.

pub mod branch;
pub mod command;
pub mod config;
pub mod console;
pub mod error;
pub mod handler;
pub mod hash;
pub mod log;
pub mod natural_comparator;
pub mod reader;
pub mod refs;
pub mod repo_files;
pub mod roots;
pub mod runner;
pub mod status;
pub mod version;
```

- [ ] **Step 4: Test helpers.** Replace `crates/ig-git/tests/common/mod.rs` and create `log_helpers.rs`:

`crates/ig-git/tests/common/mod.rs`:

```rust
//! Helpers for integration tests that run the real `git` executable.
#![allow(dead_code)] // each test crate uses a different subset

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use ig_git::console::{ConsoleSink, NullConsole};
use ig_git::handler::GitHandler;
use ig_git::runner::Git;

pub mod log_helpers;

/// An empty file used as the global git config, so the developer's config can't affect tests.
fn empty_global_config() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("empty-gitconfig");
    if !path.exists() {
        std::fs::write(&path, "").unwrap();
    }
    path
}

/// Inherited variables that would redirect git away from the test repository.
pub const SCRUBBED_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
];

/// Runs git in `dir` with a fixed identity and no global/system config; returns the raw output.
pub fn git_output(dir: &Path, args: &[&str]) -> std::process::Output {
    git_output_with_env(dir, args, &[])
}

/// Like [`git_output`], with extra environment variables (e.g. `GIT_COMMITTER_DATE`).
pub fn git_output_with_env(
    dir: &Path,
    args: &[&str],
    env: &[(&str, &str)],
) -> std::process::Output {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", empty_global_config())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");
    for v in SCRUBBED_ENV {
        cmd.env_remove(v);
    }
    cmd.envs(env.iter().copied());
    cmd.output().expect("git is installed")
}

/// Applies the same isolation as [`git_output`] (no global/system config, fixed identity) to a
/// handler run through the library runner.
pub fn isolate(h: &mut GitHandler) {
    let config = empty_global_config();
    h.add_custom_environment_variable("GIT_CONFIG_GLOBAL", config.to_str().expect("utf-8 path"));
    h.add_custom_environment_variable("GIT_CONFIG_NOSYSTEM", "1");
    h.add_custom_environment_variable("GIT_AUTHOR_NAME", "Test");
    h.add_custom_environment_variable("GIT_AUTHOR_EMAIL", "test@example.com");
    h.add_custom_environment_variable("GIT_COMMITTER_NAME", "Test");
    h.add_custom_environment_variable("GIT_COMMITTER_EMAIL", "test@example.com");
    for v in SCRUBBED_ENV {
        h.remove_inherited_environment_variable(v);
    }
}

/// Like [`git_output`], panicking on failure; returns stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = git_output(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `git init -b main` with one commit of `a.txt`.
pub fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("a.txt"), "one\n").unwrap();
    git(dir, &["add", "a.txt"]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

pub fn runner() -> Git {
    runner_with(Arc::new(NullConsole))
}

pub fn runner_with(console: Arc<dyn ConsoleSink>) -> Git {
    let config = empty_global_config();
    Git::detect("git", console)
        .expect("git is installed and detectable")
        .with_base_environment(
            &[
                ("GIT_CONFIG_GLOBAL", config.to_str().expect("utf-8 path")),
                ("GIT_CONFIG_NOSYSTEM", "1"),
                ("GIT_AUTHOR_NAME", "Test"),
                ("GIT_AUTHOR_EMAIL", "test@example.com"),
                ("GIT_COMMITTER_NAME", "Test"),
                ("GIT_COMMITTER_EMAIL", "test@example.com"),
            ],
            SCRUBBED_ENV,
        )
}
```

`crates/ig-git/tests/common/log_helpers.rs`:

```rust
//! Helpers for the log tests: the upstream `GitExecutor`/`GitTestUtil` functions they use
//! (`tac`, `last`, `readAllRefs` with `RefParser`).

use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;

use ig_git::log::provider::GitLogProvider;
use ig_git::log::ref_manager::get_ref_type;
use ig_git::refs::strip_refs_prefix;
use ig_log::data::{Hash, VcsRef, VcsRoot, VcsUser, vcs_root};

use super::{git, git_output_with_env, runner};

/// The identity of the commits made by [`git`].
pub fn default_user() -> VcsUser {
    VcsUser::create_user("Test", "test@example.com")
}

/// `git init -b master` with one commit (the upstream test repositories use `master`).
pub fn init_master_repo(dir: &Path) -> VcsRoot {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "master"]);
    tac(dir, "initial.txt");
    vcs_root(dir)
}

/// `last`: the hash of HEAD.
pub fn last(dir: &Path) -> String {
    git(dir, &["rev-parse", "HEAD"]).trim().to_string()
}

/// `tac`: touch (with random content), add and commit; returns the new HEAD.
pub fn tac(dir: &Path, file: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let content = format!("content{}", COUNTER.fetch_add(1, Ordering::Relaxed));
    std::fs::write(dir.join(file), content).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", &format!("Touched {file}")]);
    last(dir)
}

/// A provider for the test repositories (isolated git).
pub fn provider() -> GitLogProvider {
    GitLogProvider::new(Arc::new(runner()))
}

/// `readAllRefs`: refs from `git log --branches --tags --no-walk --format=%H%d --decorate=full`,
/// parsed like `RefParser.parseCommitRefs`.
pub fn read_all_refs(dir: &Path, root: &VcsRoot) -> HashSet<VcsRef> {
    let output = git(
        dir,
        &[
            "log",
            "--branches",
            "--tags",
            "--no-walk",
            "--format=%H%d",
            "--decorate=full",
        ],
    );
    let mut result = HashSet::new();
    for line in output.lines() {
        let Some(first_space_index) = line.find(' ') else {
            continue;
        };
        let hash = Hash::build(&line[..first_space_index]);
        let ref_paths = &line[first_space_index + 2..line.len() - 1];
        for long_ref_path in ref_paths.split(", ").flat_map(|s| s.split(" -> ")) {
            let name = long_ref_path.strip_prefix("tag: ").unwrap_or(long_ref_path);
            let ref_type = get_ref_type(name);
            result.insert(VcsRef::new(
                hash.clone(),
                strip_refs_prefix(name),
                ref_type,
                root.clone(),
            ));
        }
    }
    result
}

/// Commits a change of `file` with author and committer date `epoch_seconds`; returns the hash.
pub fn commit_at(dir: &Path, file: &str, epoch_seconds: i64) -> String {
    std::fs::write(dir.join(file), format!("{file} at {epoch_seconds}")).unwrap();
    git(dir, &["add", file]);
    let date = format!("@{epoch_seconds} +0000");
    let out = git_output_with_env(
        dir,
        &["commit", "-q", "-m", &format!("{file} at {epoch_seconds}")],
        &[("GIT_AUTHOR_DATE", &date), ("GIT_COMMITTER_DATE", &date)],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    last(dir)
}

/// `git init -b master` with a first commit at `epoch_seconds`.
pub fn init_repo_at(dir: &Path, epoch_seconds: i64) -> VcsRoot {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "master"]);
    commit_at(dir, "initial.txt", epoch_seconds);
    vcs_root(dir)
}
```

- [ ] **Step 5: Build and run the unit tests**

```bash
cargo clippy -q --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all
cargo test -p ig-git
```
Expected: clean. The parser, record and unescape unit tests pass, and all earlier targets still pass.

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "feat(ig-git): git log provider, parser, ref manager"
```

---

### Task 8: ig-git tests: log parser

**Files:**
- Create: `crates/ig-git/tests/log_parser.rs`

**Review focus:** `log/{parser,record,unescape}.rs`.

Ports `GitLogParserTest`.

- [ ] **Step 1: Create the test file(s)**

`crates/ig-git/tests/log_parser.rs`:

```rust
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
```

- [ ] **Step 2: Run them**

```bash
cargo test -p ig-git --test log_parser
```
Expected: `log_parser`: 15 passed. Fix the source, never the expectations.

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "test(ig-git): log parser"
```

---

### Task 9: ig-git tests: ref manager

**Files:**
- Create: `crates/ig-git/tests/log_ref_manager.rs`

**Review focus:** `log/{ref_manager,repository_manager}.rs`, `natural_comparator.rs`.

Ports `GitBranchComparatorTest` and `GitLabelComparatorTest`.

- [ ] **Step 1: Create the test file(s)**

`crates/ig-git/tests/log_ref_manager.rs`:

```rust
//! Ports of `plugins/git4idea/tests/git4idea/log/GitBranchComparatorTest.kt` and
//! `GitLabelComparatorTest.kt`, with the `given`/`expect` helpers of `GitRefManagerTestUtil.kt`.

mod common;

use std::path::Path;

use std::sync::Arc;

use common::log_helpers::{init_master_repo, last, read_all_refs};
use common::{git, runner};
use ig_git::log::ref_manager::{GitRefManager, HEAD, LOCAL_BRANCH, REMOTE_BRANCH, TAG};
use ig_git::log::repository_manager::GitRepositoryManager;
use ig_git::refs::strip_refs_prefix;
use ig_log::data::{Hash, VcsLogRefManager, VcsRef, VcsRoot};

struct Context {
    _tmp: tempfile::TempDir,
    root: VcsRoot,
    repository_manager: Arc<GitRepositoryManager>,
    ref_manager: GitRefManager,
}

impl Context {
    fn new() -> Context {
        let tmp = tempfile::tempdir().unwrap();
        let root = init_master_repo(tmp.path());
        let repository_manager = Arc::new(GitRepositoryManager::new(Arc::new(runner())));
        Context {
            _tmp: tmp,
            root,
            ref_manager: GitRefManager::new(repository_manager.clone()),
            repository_manager,
        }
    }

    fn dir(&self) -> &Path {
        &self.root
    }

    /// `given`: creates the refs at HEAD (`HEAD`, `origin/...` remote branches, `refs/tags/...`
    /// tags, local branches otherwise); local branches with a matching remote become tracking.
    fn given(&self, refs: &[&str]) -> Vec<VcsRef> {
        let dir = self.dir();
        let hash = Hash::build(&last(dir));
        let mut result = Vec::new();
        for ref_name in refs {
            if *ref_name == "HEAD" {
                result.push(self.vcs_ref(&hash, "HEAD", HEAD));
            } else if ref_name.starts_with("origin/") {
                git(
                    dir,
                    &[
                        "update-ref",
                        &format!("refs/remotes/{ref_name}"),
                        &hash.as_string(),
                    ],
                );
                result.push(self.vcs_ref(&hash, ref_name, REMOTE_BRANCH));
            } else if ref_name.starts_with("refs/tags/") {
                git(dir, &["update-ref", ref_name, &hash.as_string()]);
                result.push(self.vcs_ref(&hash, strip_refs_prefix(ref_name), TAG));
            } else {
                git(
                    dir,
                    &[
                        "update-ref",
                        &format!("refs/heads/{ref_name}"),
                        &hash.as_string(),
                    ],
                );
                result.push(self.vcs_ref(&hash, ref_name, LOCAL_BRANCH));
            }
        }
        self.set_up_tracking(&result);
        self.repository_manager.update(&self.root);
        result
    }

    fn vcs_ref(&self, hash: &Hash, name: &str, ref_type: ig_log::data::VcsRefType) -> VcsRef {
        VcsRef::new(hash.clone(), name, ref_type, self.root.clone())
    }

    fn set_up_tracking(&self, refs: &[VcsRef]) {
        for r in refs.iter().filter(|r| r.ref_type() == LOCAL_BRANCH) {
            let local_branch = r.name();
            let has_matching_remote = refs.iter().any(|it| {
                it.ref_type() == REMOTE_BRANCH && it.name().replace("origin/", "") == local_branch
            });
            if has_matching_remote {
                git(
                    self.dir(),
                    &["config", &format!("branch.{local_branch}.remote"), "origin"],
                );
                git(
                    self.dir(),
                    &[
                        "config",
                        &format!("branch.{local_branch}.merge"),
                        &format!("refs/heads/{local_branch}"),
                    ],
                );
            }
        }
    }

    /// `expect`: the refs actually in the repository, in the order of `ref_names`.
    fn expect(&self, ref_names: &[&str]) -> Vec<VcsRef> {
        let refs = read_all_refs(self.dir(), &self.root);
        ref_names
            .iter()
            .map(|ref_name| {
                refs.iter()
                    .find(|it| it.name() == strip_refs_prefix(ref_name))
                    .unwrap_or_else(|| panic!("Ref {ref_name} not found among {refs:?}"))
                    .clone()
            })
            .collect()
    }

    // GitBranchComparatorTest

    fn get_the_most_powerful_ref(&self, given_branches: &[VcsRef]) -> VcsRef {
        let mut sorted = given_branches.to_vec();
        sorted.sort_by(|a, b| self.ref_manager.branch_layout_compare(a, b));
        sorted[0].clone()
    }

    fn check_best(&self, expected_best: &str, given_branches: &[VcsRef]) {
        assert_eq!(
            self.get_the_most_powerful_ref(given_branches),
            self.expect(&[expected_best])[0]
        );
    }

    // GitLabelComparatorTest

    fn sort(&self, refs: &[VcsRef]) -> Vec<VcsRef> {
        let mut sorted = refs.to_vec();
        sorted.sort_by(|a, b| self.ref_manager.labels_order_compare(a, b));
        sorted
    }

    /// Only the names are compared, as upstream.
    fn check_labels(&self, unsorted: &[VcsRef], expected: &[VcsRef]) {
        let sorted = self.sort(unsorted);
        let actual: Vec<&str> = sorted.iter().map(|r| r.name()).collect();
        let expected: Vec<&str> = expected.iter().map(|r| r.name()).collect();
        assert_eq!(actual, expected, "Sorted refs don't match");
    }
}

mod branch_comparator {
    use super::Context;

    #[test]
    fn tracked_remote_branch_is_more_important_than_its_local_branch() {
        let c = Context::new();
        c.check_best("origin/139", &c.given(&["origin/139", "139"]));
    }

    #[test]
    fn tracked_remote_branch_is_more_important_than_any_local_branch() {
        let c = Context::new();
        c.check_best("origin/139", &c.given(&["feature", "origin/139", "139"]));
    }

    #[test]
    fn untracked_remote_branch_is_more_important_than_any_local_branch() {
        let c = Context::new();
        c.check_best("origin/139", &c.given(&["feature", "origin/139"]));
    }

    #[test]
    fn master_is_more_important_than_other_local_branches() {
        let c = Context::new();
        c.check_best("master", &c.given(&["feature", "master"]));
    }

    #[test]
    fn origin_master_is_more_important_than_other_remote_branches() {
        let c = Context::new();
        c.check_best(
            "origin/master",
            &c.given(&["origin/139", "master", "139", "origin/master", "origin/135"]),
        );
    }

    #[test]
    fn local_branch_is_more_important_than_tag() {
        let c = Context::new();
        c.check_best(
            "feature",
            &c.given(&["refs/tags/v1", "feature", "refs/tags/v2"]),
        );
    }

    #[test]
    fn two_local_non_tracking_branches_are_compared_lexicographically() {
        let c = Context::new();
        c.check_best("feature", &c.given(&["zoo", "feature"]));
    }

    #[test]
    fn tracked_local_branch_is_not_special() {
        let c = Context::new();
        let refs: Vec<_> = c
            .given(&["zoo", "origin/zoo", "feature"])
            .into_iter()
            .filter(|it| it.name() != "origin/zoo")
            .collect();
        assert_eq!(c.get_the_most_powerful_ref(&refs).name(), "feature");
    }

    #[test]
    fn remote_branches_are_compared_lexicographically() {
        let c = Context::new();
        c.check_best(
            "origin/feature",
            &c.given(&["origin/feature", "origin/zoo", "zoo"]),
        );
    }

    #[test]
    fn local_branch_is_more_important_than_head() {
        let c = Context::new();
        c.check_best("feature", &c.given(&["feature", "HEAD"]));
    }

    #[test]
    fn tag_is_more_important_than_head() {
        let c = Context::new();
        c.check_best("refs/tags/v1", &c.given(&["refs/tags/v1", "HEAD"]));
    }
}

mod label_comparator {
    use super::Context;
    use crate::common::git;

    #[test]
    fn empty() {
        let c = Context::new();
        c.check_labels(&[], &[]);
    }

    #[test]
    fn single() {
        let c = Context::new();
        c.check_labels(&c.given(&["HEAD"]), &c.expect(&["HEAD"]));
    }

    #[test]
    fn head_is_at_left_from_branch() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["master", "HEAD"]),
            &c.expect(&["HEAD", "master"]),
        );
    }

    #[test]
    fn local_branches_are_compared_as_strings() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["release", "feature"]),
            &c.expect(&["feature", "release"]),
        );
    }

    #[test]
    fn tag_is_to_the_right_of_remote_branch() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["refs/tags/v1", "origin/master"]),
            &c.expect(&["origin/master", "refs/tags/v1"]),
        );
    }

    #[test]
    fn master_is_to_the_left_of_other_local_branches() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["feature", "master"]),
            &c.expect(&["master", "feature"]),
        );
    }

    #[test]
    fn origin_master_is_to_the_left_of_other_remote_branches() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["origin/master", "origin/aaa"]),
            &c.expect(&["origin/master", "origin/aaa"]),
        );
    }

    #[test]
    fn remote_tracking_branch_is_not_special() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["feature", "origin/aaa", "origin/feature"]),
            &c.expect(&["feature", "origin/aaa", "origin/feature"]),
        );
    }

    #[test]
    fn complex_1() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["refs/tags/v1", "feature", "HEAD", "master"]),
            &c.expect(&["HEAD", "master", "feature", "refs/tags/v1"]),
        );
    }

    #[test]
    fn complex_2() {
        let c = Context::new();
        c.check_labels(
            &c.given(&[
                "origin/master",
                "origin/great_feature",
                "refs/tags/v1",
                "release",
                "HEAD",
                "master",
            ]),
            &c.expect(&[
                "HEAD",
                "master",
                "origin/master",
                "release",
                "origin/great_feature",
                "refs/tags/v1",
            ]),
        );
    }

    /// May happen e.g. in the multi-repo case.
    #[test]
    fn two_masters() {
        let c = Context::new();
        c.check_labels(
            &c.given(&["master", "master"]),
            &c.expect(&["master", "master"]),
        );
    }

    #[test]
    fn current_branch_first() {
        let c = Context::new();
        git(c.dir(), &["checkout", "-q", "-b", "zzz"]);
        c.check_labels(
            &c.given(&["xxx", "zzz", "yyy"]),
            &c.expect(&["zzz", "xxx", "yyy"]),
        );
    }
}
```

- [ ] **Step 2: Run them**

```bash
cargo test -p ig-git --test log_ref_manager
```
Expected: `log_ref_manager`: 23 passed. Fix the source, never the expectations.

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "test(ig-git): ref manager"
```

---

### Task 10: ig-git tests: output splitter and collector

**Files:**
- Create: `crates/ig-git/tests/log_collector.rs`

**Review focus:** `log/{splitter,collector}.rs`.

Ports IdeaGit tests for `GitLogOutputSplitter` and `GitLogRecordCollector`.

- [ ] **Step 1: Create the test file(s)**

`crates/ig-git/tests/log_collector.rs`:

```rust
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
```

- [ ] **Step 2: Run them**

```bash
cargo test -p ig-git --test log_collector
```
Expected: `log_collector`: 3 passed. Fix the source, never the expectations.

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "test(ig-git): output splitter and collector"
```

---

### Task 11: ig-git tests: log provider

**Files:**
- Create: `crates/ig-git/tests/log_provider.rs`

**Review focus:** `log/{provider,util}.rs`.

Ports `GitLogProviderTest` (5), `GitReadRecentCommitsTest` (6) and `GitLogUtilTest` (2, adapted).

- [ ] **Step 1: Create the test file(s)**

`crates/ig-git/tests/log_provider.rs`:

```rust
//! Ports of `plugins/git4idea/tests/git4idea/log/GitLogProviderTest.kt` (`GitLogProviderTest` and
//! `GitReadRecentCommitsTest`) and `plugins/git4idea/tests/git4idea/history/GitLogUtilTest.kt`,
//! against real repositories.
//!
//! Not ported (out of scope, see `reference/upstream-map.toml`): the filter tests (`test filter by
//! branch`, `... and user`, `test by range`, `... and branch`, `test filter by text`, `... no
//! regex`, `... with regex and user`, `... simple text and user`), `test full details`, the
//! readFullDetails cases of `GitLogUtilTest` (renames, merge diffs), and the
//! `experimental refs collection = true` parameter of `GitReadRecentCommitsTest`.
//! `GitLogUtilTest`'s two message/hash cases read through metadata instead of full details.

mod common;

use std::path::Path;

use common::log_helpers::{default_user, init_master_repo, last, provider, read_all_refs, tac};
use common::{git, runner};
use ig_git::log::provider::GitLogProvider;
use ig_git::log::ref_manager::{LOCAL_BRANCH, TAG};
use ig_git::log::util::collect_metadata_with_params;
use ig_log::GraphCommit;
use ig_log::data::{
    DetailedLogData, Hash, Requirements, TimedVcsCommit, VcsCommitMetadata, VcsLogProvider,
    VcsRefsList, VcsRoot, vcs_root,
};

/// `readCommitsFromGit`: `git log --all --date-order --full-history --sparse`.
fn read_commits_from_git(dir: &Path, root: &VcsRoot) -> Vec<VcsCommitMetadata> {
    let output = git(
        dir,
        &[
            "log",
            "--all",
            "--date-order",
            "--full-history",
            "--sparse",
            "--pretty=%H|%P|%ct|%s|%B",
        ],
    );
    output
        .lines()
        .filter(|l| !l.is_empty())
        .map(|record| {
            let items: Vec<&str> = record.split('|').collect();
            let time = items[2].parse::<i64>().unwrap() * 1000;
            VcsCommitMetadata::new(
                Hash::build(items[0]),
                items[1]
                    .split(' ')
                    .filter(|p| !p.is_empty())
                    .map(Hash::build)
                    .collect(),
                time,
                root.clone(),
                items[3].to_string(),
                default_user(),
                items[4].to_string(),
                default_user(),
                time,
            )
        })
        .collect()
}

fn ids<C: GraphCommit<Hash>>(commits: &[C]) -> Vec<Hash> {
    commits.iter().map(|c| c.id().clone()).collect()
}

fn sorted_ids<C: GraphCommit<Hash>>(commits: &[C]) -> Vec<Hash> {
    let mut v = ids(commits);
    v.sort();
    v
}

fn prepare_some_history(dir: &Path) {
    tac(dir, "a.txt");
    git(dir, &["tag", "ATAG"]);
    tac(dir, "b.txt");
}

fn create_tagged_branch(dir: &Path) {
    let hash = last(dir);
    tac(dir, "c.txt");
    tac(dir, "d.txt");
    tac(dir, "e.txt");
    git(dir, &["tag", "poor-tag"]);
    git(dir, &["reset", "-q", "--hard", &hash]);
}

/// `readRecentCommits` of the test: `repo.update()`, then `readFirstBlock`.
fn read_recent_commits(
    provider: &GitLogProvider,
    root: &VcsRoot,
    requirements: &Requirements<'_>,
) -> DetailedLogData {
    provider.repository_manager().update(root);
    provider.read_first_block(root, requirements).unwrap()
}

fn short_presentation(c: &VcsCommitMetadata) -> String {
    format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        c.id().to_short_string(),
        c.author_time(),
        c.author(),
        c.commit_time(),
        c.committer(),
        c.subject()
    )
}

// ---- GitLogProviderTest

#[test]
fn test_all_log_with_tagged_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    create_tagged_branch(dir);
    let expected_log = read_commits_from_git(dir, &root);
    let mut collector: Vec<TimedVcsCommit> = Vec::new();
    provider()
        .read_all_hashes(&root, &mut |c| collector.push(c))
        .unwrap();
    assert_eq!(ids(&collector), ids(&expected_log));
}

fn check_full_hash(object_format: &str, hex_size: usize, prefix_len: usize) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(format!("{object_format}-repo"));
    std::fs::create_dir_all(&dir).unwrap();
    git(
        &dir,
        &[
            "init",
            "-q",
            "-b",
            "master",
            &format!("--object-format={object_format}"),
        ],
    );
    tac(&dir, "initial.txt");
    let root = vcs_root(&dir);
    let provider = provider();
    provider.repository_manager().update(&root);
    let head_hash = last(&dir);
    assert_eq!(head_hash.len(), hex_size);

    assert!(provider.is_full_hash(&root, &head_hash));
    assert!(!provider.is_full_hash(&root, &head_hash[..prefix_len]));
    assert!(!provider.is_full_hash(&root, &"z".repeat(hex_size)));
}

#[test]
fn test_full_hash_check_sha1_repository() {
    check_full_hash("sha1", 40, 7);
}

#[test]
fn test_full_hash_check_sha256_repository() {
    check_full_hash("sha256", 64, 40);
}

#[test]
fn test_get_current_user() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    git(dir, &["config", "user.name", "John Doe"]);
    git(dir, &["config", "user.email", "John.Doe@example.com"]);
    let user = provider().get_current_user(&root).unwrap();
    let user = user.expect("User is not defined");
    assert_eq!(user.name(), "John Doe", "User name is incorrect");
    assert_eq!(
        user.email(),
        "john.doe@example.com",
        "User email is incorrect"
    );
}

/// IdeaGit: without `user.name` there is no current user.
#[test]
fn current_user_without_config_is_none() {
    let tmp = tempfile::tempdir().unwrap();
    let root = init_master_repo(tmp.path());
    assert_eq!(provider().get_current_user(&root).unwrap(), None);
}

#[test]
fn test_short_details() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    // prepareLongHistory
    for i in 0..15 {
        let file = format!("a{}.txt", i % 10);
        if i < 10 {
            tac(dir, &file);
        } else {
            std::fs::write(dir.join(&file), format!("modified {i}")).unwrap();
            git(dir, &["commit", "-q", "-am", &format!("Modified {file}")]);
        }
    }
    let log = read_commits_from_git(dir, &root);

    let provider = provider();
    let mut hashes = Vec::new();
    provider
        .read_all_hashes(&root, &mut |c| hashes.push(c.id().as_string()))
        .unwrap();

    let mut result = Vec::new();
    provider
        .read_metadata(&root, &hashes, &mut |m| result.push(m))
        .unwrap();

    let actual: Vec<String> = result.iter().map(short_presentation).collect();
    let expected: Vec<String> = log.iter().map(short_presentation).collect();
    assert_eq!(actual, expected);
}

// ---- GitReadRecentCommitsTest (experimental refs collection = false)

#[test]
fn test_init_with_tagged_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    let expected_log_without_tagged_branch = read_commits_from_git(dir, &root);
    create_tagged_branch(dir);

    let block = read_recent_commits(&provider(), &root, &Requirements::simple(1000));
    assert_eq!(
        ids(&block.commits),
        ids(&expected_log_without_tagged_branch)
    );
}

#[test]
fn test_refresh_with_new_tagged_branch() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    let prev_refs = VcsRefsList(read_all_refs(dir, &root).into_iter().collect());
    create_tagged_branch(dir);

    let expected_log = read_commits_from_git(dir, &root);
    let block = read_recent_commits(
        &provider(),
        &root,
        &Requirements::refresh(1000, true, &prev_refs),
    );
    assert_eq!(sorted_ids(&block.commits), sorted_ids(&expected_log));
}

#[test]
fn test_refresh_when_new_tag_moved() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    let prev_refs = VcsRefsList(read_all_refs(dir, &root).into_iter().collect());
    git(dir, &["tag", "-f", "ATAG"]);

    let expected_log = read_commits_from_git(dir, &root);
    let refs = read_all_refs(dir, &root);
    let block = read_recent_commits(
        &provider(),
        &root,
        &Requirements::refresh(1000, true, &prev_refs),
    );
    assert_eq!(sorted_ids(&block.commits), sorted_ids(&expected_log));
    assert_eq!(block.refs.len(), refs.len(), "{:?} vs {refs:?}", block.refs);
    assert_eq!(
        block
            .refs
            .into_iter()
            .collect::<std::collections::HashSet<_>>(),
        refs
    );
}

#[test]
fn test_new_tag_on_old_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    let prev_refs = VcsRefsList(read_all_refs(dir, &root).into_iter().collect());
    let commits = read_commits_from_git(dir, &root);
    let first_commit = commits[commits.len() - 1].id().as_string();
    git(dir, &["tag", "NEW_TAG", &first_commit]);

    let refs = read_all_refs(dir, &root);
    let block = read_recent_commits(
        &provider(),
        &root,
        &Requirements::refresh(1000, true, &prev_refs),
    );
    assert_eq!(block.refs.len(), refs.len(), "{:?} vs {refs:?}", block.refs);
    assert_eq!(
        block
            .refs
            .into_iter()
            .collect::<std::collections::HashSet<_>>(),
        refs
    );
}

#[test]
fn test_dont_report_origin_head() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    git(dir, &["update-ref", "refs/remotes/origin/HEAD", "master"]);

    let block = read_recent_commits(&provider(), &root, &Requirements::simple(1000));
    assert!(
        block.refs.iter().all(|r| r.name() != "origin/HEAD"),
        "origin/HEAD should be ignored"
    );
}

#[test]
fn test_support_equally_named_branch_and_tag() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    prepare_some_history(dir);
    git(dir, &["branch", "build"]);
    git(dir, &["tag", "build"]);

    let empty = VcsRefsList::default();
    let data = read_recent_commits(
        &provider(),
        &root,
        &Requirements::refresh(1000, true, &empty),
    );
    let expected_log = read_commits_from_git(dir, &root);
    assert_eq!(ids(&data.commits), ids(&expected_log));
    assert!(
        data.refs
            .iter()
            .any(|r| r.name() == "build" && r.ref_type() == LOCAL_BRANCH)
    );
    assert!(
        data.refs
            .iter()
            .any(|r| r.name() == "build" && r.ref_type() == TAG)
    );
}

// ---- GitLogUtilTest

#[test]
fn test_loading_details_with_u0001_character() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    let message =
        "subject containing \u{1} symbol in it\n\ncommit body containing \u{1} symbol in it";
    std::fs::write(dir.join("file.txt"), "content").unwrap();
    let msg_file = dir.join(".git").join("TEST_MSG");
    std::fs::write(&msg_file, message).unwrap();
    git(dir, &["add", "file.txt"]);
    git(dir, &["commit", "-q", "-F", msg_file.to_str().unwrap()]);

    let mut details = Vec::new();
    provider()
        .read_metadata(&root, &[last(dir)], &mut |m| details.push(m))
        .unwrap();
    let last_commit = details.first().expect("commit");
    assert_eq!(last_commit.full_message(), message);
}

#[test]
fn test_loading_details_without_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    let root = init_master_repo(dir);
    let message_file = dir.join("message.txt");
    std::fs::write(&message_file, "").unwrap();

    let commit_count = 20;
    let mut expected = Vec::new();
    for i in 0..commit_count {
        std::fs::write(dir.join("file.txt"), format!("content number {i}")).unwrap();
        git(dir, &["add", "."]);
        git(
            dir,
            &[
                "commit",
                "-q",
                "--allow-empty-message",
                "-F",
                message_file.to_str().unwrap(),
            ],
        );
        expected.push(last(dir));
    }
    expected.reverse();

    let max_count = format!("--max-count={commit_count}");
    let data = collect_metadata_with_params(&runner(), &root, &[&max_count]).unwrap();
    let actual_hashes: Vec<String> = data.commits.iter().map(|c| c.id().as_string()).collect();
    assert_eq!(actual_hashes, expected);
    assert!(
        data.commits
            .iter()
            .all(|c| c.subject().is_empty() && c.full_message().is_empty())
    );
}

/// IdeaGit: a fresh repository (no commits) has no log data.
#[test]
fn fresh_repository_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    git(dir, &["init", "-q", "-b", "master"]);
    let root = vcs_root(dir);
    let provider = provider();
    let block = provider
        .read_first_block(&root, &Requirements::simple(10))
        .unwrap();
    assert!(block.commits.is_empty() && block.refs.is_empty());
    let mut count = 0;
    let data = provider
        .read_all_hashes(&root, &mut |_| count += 1)
        .unwrap();
    assert_eq!(count, 0);
    assert!(data.refs.is_empty());
    // the branch of a fresh repository is known (upstream `getCurrentBranch`)
    assert_eq!(
        provider.get_current_branch(&root).as_deref(),
        Some("master")
    );
}
```

- [ ] **Step 2: Run them**

```bash
cargo test -p ig-git --test log_provider
```
Expected: `log_provider`: 15 passed. Fix the source, never the expectations.

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "test(ig-git): log provider"
```

---

### Task 12: ig-git tests: refresh and performance

**Files:**
- Create: `crates/ig-git/tests/log_refresh.rs`, `crates/ig-git/tests/log_perf.rs`

**Review focus:** end-to-end: `log_refresh.rs` against `ig-log::data::refresher` and the git provider.

Ports IdeaGit refresh tests and the 50k-commit performance smoke test.

- [ ] **Step 1: Create the test file(s)**

`crates/ig-git/tests/log_refresh.rs`:

```rust
//! IdeaGit integration tests: loading one or several repositories into the log data pack and
//! refreshing it after new commits, branches, tags and resets.
//!
//! A refresh must give the same log as a fresh full load, through the joiner (no second
//! `readAllHashes`).

mod common;

use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use common::git;
use common::log_helpers::{commit_at, init_repo_at, provider};
use ig_git::log::provider::GitLogProvider;
use ig_git::log::ref_manager::{LOCAL_BRANCH, TAG};
use ig_log::data::{
    DetailedLogData, InMemoryStorage, LogData, LogProviders, Requirements, TimedVcsCommit,
    VcsCommitMetadata, VcsException, VcsLogGraphData, VcsLogProvider, VcsLogRefManager,
    VcsLogRefresher, VcsLogStorage, VcsRefsContainer, VcsRoot, VcsUser,
};
use ig_log::{GraphCommit, Options, PermanentGraph, VisibleGraph};

/// Counts the full-log reads of a provider.
struct CountingProvider {
    inner: GitLogProvider,
    read_all_hashes_count: AtomicUsize,
    read_first_block_count: AtomicUsize,
}

impl CountingProvider {
    fn new() -> Arc<CountingProvider> {
        Arc::new(CountingProvider {
            inner: provider(),
            read_all_hashes_count: AtomicUsize::new(0),
            read_first_block_count: AtomicUsize::new(0),
        })
    }
}

impl VcsLogProvider for CountingProvider {
    fn read_first_block(
        &self,
        root: &VcsRoot,
        requirements: &Requirements<'_>,
    ) -> Result<DetailedLogData, VcsException> {
        self.read_first_block_count.fetch_add(1, Ordering::SeqCst);
        self.inner.read_first_block(root, requirements)
    }
    fn read_all_hashes(
        &self,
        root: &VcsRoot,
        consumer: &mut dyn FnMut(TimedVcsCommit),
    ) -> Result<LogData, VcsException> {
        self.read_all_hashes_count.fetch_add(1, Ordering::SeqCst);
        self.inner.read_all_hashes(root, consumer)
    }
    fn read_metadata(
        &self,
        root: &VcsRoot,
        hashes: &[String],
        consumer: &mut dyn FnMut(VcsCommitMetadata),
    ) -> Result<(), VcsException> {
        self.inner.read_metadata(root, hashes, consumer)
    }
    fn reference_manager(&self) -> Arc<dyn VcsLogRefManager> {
        self.inner.reference_manager()
    }
    fn get_current_user(&self, root: &VcsRoot) -> Result<Option<VcsUser>, VcsException> {
        self.inner.get_current_user(root)
    }
    fn get_current_branch(&self, root: &VcsRoot) -> Option<String> {
        self.inner.get_current_branch(root)
    }
}

/// Commits as (root, hash, parent hashes, timestamp) and refs as (root, name, type, hash).
type Snapshot = (
    Vec<(String, String, Vec<String>, i64)>,
    BTreeSet<(String, String, String, String)>,
);

fn snapshot(pack: &VcsLogGraphData, storage: &dyn VcsLogStorage) -> Snapshot {
    assert!(pack.load_error().is_none(), "{pack:?}");
    let id = |index: &i32| storage.get_commit_id(*index).unwrap();
    let commits = pack
        .permanent_graph()
        .map(|graph| {
            graph
                .all_commits()
                .iter()
                .map(|c| {
                    let commit = id(c.id());
                    let parents = c.parents().iter().map(|p| id(p).hash.as_string()).collect();
                    (
                        commit.root.display().to_string(),
                        commit.hash.as_string(),
                        parents,
                        c.timestamp(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let refs = pack
        .refs_model()
        .all_refs()
        .map(|r| {
            (
                r.root().display().to_string(),
                r.name().to_string(),
                r.ref_type().name().to_string(),
                r.commit_hash().as_string(),
            )
        })
        .collect();
    (commits, refs)
}

struct Log {
    storage: Arc<InMemoryStorage>,
    refresher: VcsLogRefresher,
    provider: Arc<CountingProvider>,
    published: Arc<Mutex<Vec<bool>>>,
}

fn new_log(roots: &[&VcsRoot], recent_commit_count: usize) -> Log {
    let provider = CountingProvider::new();
    let storage = Arc::new(InMemoryStorage::new());
    let mut providers = LogProviders::new();
    for root in roots {
        providers.insert((*root).clone(), provider.clone() as Arc<dyn VcsLogProvider>);
    }
    let mut refresher = VcsLogRefresher::new(storage.clone(), providers, recent_commit_count);
    let published = Arc::new(Mutex::new(Vec::new()));
    let sink = published.clone();
    refresher.set_data_pack_handler(move |pack| sink.lock().unwrap().push(pack.is_full()));
    Log {
        storage,
        refresher,
        provider,
        published,
    }
}

/// The log as a fresh refresher loads it.
fn full_load_snapshot(roots: &[&VcsRoot]) -> Snapshot {
    let mut log = new_log(roots, 1000);
    let pack = log.refresher.initialize();
    snapshot(&pack, log.storage.as_ref())
}

fn assert_refresh_matches_full_load(log: &mut Log, roots: &[&VcsRoot], refreshed: &[&VcsRoot]) {
    let full_reads = log.provider.read_all_hashes_count.load(Ordering::SeqCst);
    let refreshed: Vec<VcsRoot> = refreshed.iter().map(|r| (*r).clone()).collect();
    let pack = log.refresher.refresh(&refreshed);
    assert!(pack.is_full());
    assert_eq!(
        log.provider.read_all_hashes_count.load(Ordering::SeqCst),
        full_reads,
        "the refresh was joined, not reloaded"
    );
    assert_eq!(
        snapshot(&pack, log.storage.as_ref()),
        full_load_snapshot(roots)
    );
}

/// A history of `count` commits on master, one per 10 seconds from `start`.
fn linear_history(dir: &Path, start: i64, count: usize) -> i64 {
    let mut t = start;
    for i in 0..count {
        t += 10;
        commit_at(dir, &format!("f{i}.txt"), t);
    }
    t
}

#[test]
fn multi_repo_initial_load() {
    let tmp = tempfile::tempdir().unwrap();
    let a = init_repo_at(&tmp.path().join("a"), 1000);
    let b = init_repo_at(&tmp.path().join("b"), 1005);
    // interleaved timestamps
    for i in 0..5 {
        commit_at(&a, &format!("a{i}.txt"), 1010 + 20 * i);
        commit_at(&b, &format!("b{i}.txt"), 1020 + 20 * i);
    }
    git(&a, &["tag", "v1", "HEAD~2"]);
    git(&b, &["branch", "feature", "HEAD~1"]);

    let mut log = new_log(&[&a, &b], 3);
    let pack = log.refresher.initialize();
    assert_eq!(
        *log.published.lock().unwrap(),
        [false, true],
        "first block, then the full log"
    );

    let (commits, refs) = snapshot(&pack, log.storage.as_ref());
    assert_eq!(commits.len(), 12);
    let timestamps: Vec<i64> = commits.iter().map(|c| c.3).collect();
    let mut sorted = timestamps.clone();
    sorted.sort_by(|x, y| y.cmp(x));
    assert_eq!(timestamps, sorted, "repositories are joined by commit time");
    let roots: HashSet<&str> = commits.iter().map(|c| c.0.as_str()).collect();
    assert_eq!(roots.len(), 2);
    assert!(refs.contains(&(
        a.display().to_string(),
        "v1".into(),
        TAG.name().into(),
        commits_of(&commits, &a)[2].clone()
    )));
    assert!(
        refs.iter().any(|r| r.0 == b.display().to_string()
            && r.1 == "feature"
            && r.2 == LOCAL_BRANCH.name())
    );
    assert_eq!(pack.refs_model().refs_by_root().len(), 2);

    // the head of each repository is a branch head of the graph with its best ref
    let graph = pack.permanent_graph().unwrap();
    let a_head = log
        .storage
        .get_commit_index(&ig_log::data::Hash::build(&commits_of(&commits, &a)[0]), &a);
    let best = pack.refs_model().get_ref_for_head_commit(a_head).unwrap();
    assert_eq!(best.name(), "master");
    assert!(graph.get_containing_branches(&a_head).contains(&a_head));

    let visible = graph
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap();
    assert_eq!(visible.visible_commit_count(), 12);
}

fn commits_of(commits: &[(String, String, Vec<String>, i64)], root: &VcsRoot) -> Vec<String> {
    commits
        .iter()
        .filter(|c| c.0 == root.display().to_string())
        .map(|c| c.1.clone())
        .collect()
}

#[test]
fn refresh_after_new_commit_is_joined() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    let root = init_repo_at(&dir, 1000);
    let t = linear_history(&dir, 1000, 10);

    let mut log = new_log(&[&root], 3);
    log.refresher.initialize();
    assert_eq!(log.provider.read_all_hashes_count.load(Ordering::SeqCst), 1);

    commit_at(&dir, "new.txt", t + 10);
    assert_refresh_matches_full_load(&mut log, &[&root], &[&root]);
    assert_eq!(
        log.provider.read_first_block_count.load(Ordering::SeqCst),
        2
    );
}

#[test]
fn refresh_after_new_branch_and_tags_is_joined() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    let root = init_repo_at(&dir, 1000);
    let mut t = linear_history(&dir, 1000, 10);

    let mut log = new_log(&[&root], 3);
    log.refresher.initialize();

    // a new branch with its own commits, merged back
    git(&dir, &["checkout", "-q", "-b", "feature", "HEAD~3"]);
    t += 10;
    commit_at(&dir, "feature1.txt", t);
    t += 10;
    commit_at(&dir, "feature2.txt", t);
    git(&dir, &["checkout", "-q", "master"]);
    let date = format!("@{} +0000", t + 10);
    let out = common::git_output_with_env(
        &dir,
        &["merge", "-q", "--no-ff", "feature", "-m", "merge feature"],
        &[("GIT_AUTHOR_DATE", &date), ("GIT_COMMITTER_DATE", &date)],
    );
    assert!(out.status.success());
    assert_refresh_matches_full_load(&mut log, &[&root], &[&root]);

    // a tag on an old commit, outside the first block
    git(&dir, &["tag", "old-tag", "HEAD~8"]);
    assert_refresh_matches_full_load(&mut log, &[&root], &[&root]);

    // a new branch pointing at an existing commit
    git(&dir, &["branch", "pointer", "HEAD~5"]);
    assert_refresh_matches_full_load(&mut log, &[&root], &[&root]);
}

#[test]
fn refresh_with_tagged_only_branch_is_joined() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    let root = init_repo_at(&dir, 1000);
    let t = linear_history(&dir, 1000, 6);

    let mut log = new_log(&[&root], 3);
    log.refresher.initialize();

    // commits reachable only from a tag (`createTaggedBranch`)
    let head = common::log_helpers::last(&dir);
    commit_at(&dir, "c.txt", t + 10);
    commit_at(&dir, "d.txt", t + 20);
    git(&dir, &["tag", "poor-tag"]);
    git(&dir, &["reset", "-q", "--hard", &head]);
    assert_refresh_matches_full_load(&mut log, &[&root], &[&root]);
}

#[test]
fn refresh_after_reset_removes_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    let root = init_repo_at(&dir, 1000);
    linear_history(&dir, 1000, 10);

    let mut log = new_log(&[&root], 3);
    log.refresher.initialize();

    git(&dir, &["reset", "-q", "--hard", "HEAD~2"]);
    assert_refresh_matches_full_load(&mut log, &[&root], &[&root]);
}

#[test]
fn refresh_of_one_root_in_multi_repo_log() {
    let tmp = tempfile::tempdir().unwrap();
    let a = init_repo_at(&tmp.path().join("a"), 1000);
    let b = init_repo_at(&tmp.path().join("b"), 1001);
    let t = linear_history(&a, 1000, 5);
    linear_history(&b, 1001, 5);

    let mut log = new_log(&[&a, &b], 3);
    log.refresher.initialize();

    commit_at(&a, "new.txt", t + 100);
    git(&a, &["tag", "v2"]);
    assert_refresh_matches_full_load(&mut log, &[&a, &b], &[&a]);
}

#[test]
fn refresh_beyond_the_recent_block_reloads_everything() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("repo");
    let root = init_repo_at(&dir, 1000);
    let t = linear_history(&dir, 1000, 3);

    let mut log = new_log(&[&root], 2);
    log.refresher.initialize();

    // 30 new commits: more than 2 and 5 * 2 recent commits, so the join fails twice
    linear_history(&dir, t, 30);
    let pack = log.refresher.refresh(std::slice::from_ref(&root));
    assert_eq!(
        log.provider.read_all_hashes_count.load(Ordering::SeqCst),
        2,
        "reloaded"
    );
    assert_eq!(
        snapshot(&pack, log.storage.as_ref()),
        full_load_snapshot(&[&root])
    );
}
```

`crates/ig-git/tests/log_perf.rs`:

```rust
//! IdeaGit performance smoke test: a synthetic repository of ~50,000 commits with feature
//! branches, merges and tags (generated with `git fast-import`), loaded into the log.
//!
//! Ignored by default. Run in release mode to see the timings:
//! `cargo test --release -p ig-git --test log_perf -- --ignored --nocapture`

mod common;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::log_helpers::{commit_at, provider};
use common::{SCRUBBED_ENV, git};
use ig_log::data::graph_data::build_data;
use ig_log::data::refresher::compact_commit;
use ig_log::data::{
    InMemoryStorage, LogProviders, RootRefsModel, VcsLogProvider, VcsLogRefresher, VcsLogStorage,
    vcs_root,
};
use ig_log::{Options, PermanentGraph, VisibleGraph};

const TOTAL_COMMITS: usize = 50_000;

struct Stream {
    s: String,
    mark: usize,
    time: i64,
}

impl Stream {
    fn commit(&mut self, r: &str, from: Option<usize>, merge: Option<usize>, msg: &str) -> usize {
        self.mark += 1;
        self.time += 1;
        let (mark, time) = (self.mark, self.time);
        self.s.push_str(&format!(
            "commit {r}\nmark :{mark}\ncommitter Perf <perf@example.com> {time} +0000\ndata {}\n{msg}\n",
            msg.len()
        ));
        if let Some(from) = from {
            self.s.push_str(&format!("from :{from}\n"));
        }
        if let Some(merge) = merge {
            self.s.push_str(&format!("merge :{merge}\n"));
        }
        self.s.push_str(&format!(
            "M 644 inline f{}\ndata 2\n{}\n\n",
            mark % 97,
            mark % 10
        ));
        mark
    }
}

/// Writes a fast-import stream: a main line where every 40th commit forks a feature branch of
/// 3..20 commits; two of three features are merged back, the others stay as branches. A tag every
/// 1000 main commits. Returns the stream, the number of commits and of branch refs.
fn fast_import_stream() -> (String, usize, usize) {
    let mut st = Stream {
        s: String::with_capacity(TOTAL_COMMITS * 120),
        mark: 0,
        time: 1_500_000_000,
    };
    let mut main_head: Option<usize> = None;
    let mut branches = 0;
    let mut main_count = 0usize;
    while st.mark < TOTAL_COMMITS {
        main_count += 1;
        main_head = Some(st.commit(
            "refs/heads/master",
            main_head,
            None,
            &format!("main {main_count}"),
        ));
        if main_count % 40 == 0 {
            let feature = main_count / 40;
            let name = format!("refs/heads/feature/{feature}");
            let len = 3 + feature % 18;
            let mut head = main_head;
            for k in 0..len {
                head = Some(st.commit(&name, head, None, &format!("feature {feature} commit {k}")));
            }
            if feature % 3 != 0 {
                main_head = Some(st.commit(
                    "refs/heads/master",
                    main_head,
                    head,
                    &format!("merge feature {feature}"),
                ));
                st.s.push_str(&format!(
                    "reset {name}\nfrom 0000000000000000000000000000000000000000\n\n"
                ));
            } else {
                branches += 1;
            }
        }
        if main_count % 1000 == 0 {
            st.s.push_str(&format!(
                "reset refs/tags/v{}\nfrom :{}\n\n",
                main_count / 1000,
                main_head.unwrap()
            ));
        }
    }
    (st.s, st.mark, branches + 1)
}

fn generate_repository(dir: &Path) -> (usize, usize) {
    git(dir, &["init", "-q", "-b", "master"]);
    let (stream, commits, branches) = fast_import_stream();
    let mut cmd = Command::new("git");
    cmd.args(["fast-import", "--quiet"])
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped());
    for v in SCRUBBED_ENV {
        cmd.env_remove(v);
    }
    let mut child = cmd.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stream.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
    git(dir, &["reset", "-q", "--hard", "master"]);
    (commits, branches)
}

fn ms(d: Duration) -> String {
    format!("{:>8.1} ms", d.as_secs_f64() * 1000.0)
}

#[test]
#[ignore = "performance smoke test; run with --release -- --ignored --nocapture"]
fn load_50k_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("big");
    std::fs::create_dir_all(&dir).unwrap();
    let start = Instant::now();
    let (commits, branches) = generate_repository(&dir);
    eprintln!(
        "generated {commits} commits, {branches} branches with fast-import in {}",
        ms(start.elapsed())
    );
    let root = vcs_root(&dir);
    let provider = Arc::new(provider());

    // git alone, same arguments as read_all_hashes, output discarded (warms the caches too)
    let start = Instant::now();
    let status = Command::new("git")
        .args(["log", "HEAD", "--branches", "--remotes", "--tags", "--date-order", "--decorate=full"])
        .arg("--pretty=format:%x01%x01%H%x02%x02%P%x02%x02%ct%x02%x02%an%x02%x02%ae%x02%x02%d%x03%x03")
        .current_dir(&dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let t_git = start.elapsed();

    // step by step
    let start = Instant::now();
    let mut timed = Vec::with_capacity(commits);
    let data = provider
        .read_all_hashes(&root, &mut |c| timed.push(c))
        .unwrap();
    let t_read = start.elapsed();
    assert_eq!(timed.len(), commits);

    let storage: Arc<dyn VcsLogStorage> = Arc::new(InMemoryStorage::new());
    let start = Instant::now();
    let graph_commits: Vec<_> = timed
        .iter()
        .map(|c| compact_commit(storage.as_ref(), c, &root))
        .collect();
    let refs = RootRefsModel::create(data.refs.iter().cloned(), storage.as_ref());
    let t_compact = start.elapsed();

    let mut providers = LogProviders::new();
    providers.insert(root.clone(), provider.clone() as Arc<dyn VcsLogProvider>);
    let start = Instant::now();
    let pack = build_data(
        &graph_commits,
        BTreeMap::from([(root.clone(), Arc::new(refs))]),
        &providers,
        &storage,
        true,
    );
    let t_build = start.elapsed();

    let graph = pack.permanent_graph().unwrap();
    let start = Instant::now();
    let visible = graph
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap();
    let t_visible = start.elapsed();
    assert_eq!(visible.visible_commit_count() as usize, commits);

    let start = Instant::now();
    let mut elements = 0;
    for row in 0..1000 {
        elements += visible.get_row_info(row).print_elements().len();
    }
    let t_rows = start.elapsed();
    assert!(elements > 1000);

    let start = Instant::now();
    let width = visible.recommended_width();
    let t_width = start.elapsed();

    // end to end, as the UI does it
    let storage = Arc::new(InMemoryStorage::new());
    let mut refresher = VcsLogRefresher::new(storage.clone(), providers.clone(), 1000);
    let start = Instant::now();
    let full = refresher.initialize();
    let t_initialize = start.elapsed();
    assert_eq!(full.commits_count(), commits);

    commit_at(&dir, "new.txt", 2_000_000_000);
    let start = Instant::now();
    let refreshed = refresher.refresh(std::slice::from_ref(&root));
    let t_refresh = start.elapsed();
    assert_eq!(refreshed.commits_count(), commits + 1);
    assert!(refreshed.is_full() && refreshed.load_error().is_none());

    eprintln!("git log alone (output discarded)      {}", ms(t_git));
    eprintln!("read_all_hashes (git log + parse)     {}", ms(t_read));
    eprintln!("intern ids + refs model               {}", ms(t_compact));
    eprintln!("build data pack (permanent graph)     {}", ms(t_build));
    eprintln!("create visible graph                  {}", ms(t_visible));
    eprintln!("print elements of 1000 rows           {}", ms(t_rows));
    eprintln!(
        "recommended width (width {width:>3})        {}",
        ms(t_width)
    );
    eprintln!("refresher.initialize (block + full)   {}", ms(t_initialize));
    eprintln!("refresher.refresh (1 new commit)      {}", ms(t_refresh));
}
```

- [ ] **Step 2: Run them**

```bash
cargo test -p ig-git --test log_refresh
cargo test -p ig-git --test log_perf
```
Expected: `log_refresh`: 7 passed, `log_perf`: 0 passed (plus 1 ignored). Fix the source, never the expectations.

- [ ] **Step 2b: Run the performance smoke test once (release)**

```bash
cargo test -p ig-git --release --test log_perf -- --ignored --nocapture
```
Expected: it passes. Record the printed timings in the commit message body. The reference machine (macOS arm64, 50k commits) took about 240 ms for `initialize` and about 40 ms for a one-commit `refresh`.

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "test(ig-git): refresh and performance"
```

---

### Task 13: Module map and full verification

**Files:**
- Modify: `reference/upstream-map.toml`

- [ ] **Step 1: Replace `reference/upstream-map.toml`** with this content. It adds the new `ig-log::data::*` and `ig-git::log::*` entries, and two deviation lines on existing entries (runner and commits-info index):

```toml
# Rust module -> intellij-community files it was ported from.
# pinned:     upstream commit this module last matched (see reference/UPSTREAM)
# tests:      upstream tests ported alongside
# deviations: intentional differences from upstream

[modules."ig-git::command"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/commands/GitCommand.java"]

[modules."ig-git::handler"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/commands/GitHandler.java",
  "plugins/git4idea/backend/src/commands/GitLineHandler.java",
  "platform/vcs-impl/src/com/intellij/vcs/VcsLocaleHelper.kt",
]
deviations = [
  "No WSL/Eel executables, no Windows .cmd escaping, no VcsEnvCustomizer extensions",
  "Also sets IDEAGIT_GIT_EXECUTABLE=true next to INTELLIJ_GIT_EXECUTABLE=true",
]

[modules."ig-git::runner"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/commands/GitImplBase.java", "plugins/git4idea/backend/src/commands/GitCommandResult.java"]
deviations = [
  "Synchronous std::process execution on the calling thread; callers use a background executor",
  "No root-name prefix in error messages yet (multi-root presentation comes with the UI)",
  "runCommandWithoutCollectingOutput takes a line-listener closure (GitLineHandlerListener); stdin input is a byte buffer written then closed (setInputProcessor / writeLines)",
]

[modules."ig-git::console"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/commands/GitImplBase.java#GitCommandOutputLogger"]

[modules."ig-git::version"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/config/GitVersion.java", "plugins/git4idea/backend/src/config/GitVersionSpecialty.kt"]
tests = ["plugins/git4idea/tests/git4idea/tests/GitVersionTest.java"]
deviations = ["WSL1/WSL2 types not ported", "Only the specialties used so far are ported"]

[modules."ig-git::status"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/index/GitIndexStatusUtil.kt"]
deviations = ["Long path lists are not split into chunks (GitUtil.createHandlerWithPaths)"]

[modules."ig-git::hash"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-impl/shared/src/com/intellij/vcs/log/impl/HashImpl.java"]

[modules."ig-git::refs"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/repo/GitRefUtil.java",
  "plugins/git4idea/backend/src/validators/GitRefNameValidator.java",
  "plugins/git4idea/shared/src/com/intellij/vcs/git/ref/GitRefUtil.kt",
]

[modules."ig-git::repo_files"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/repo/GitRepositoryFiles.java",
  "plugins/git4idea/backend/src/GitUtil.java#findGitDir",
  "platform/dvcs-impl/src/com/intellij/dvcs/DvcsUtil.java#tryLoadFile",
]

[modules."ig-git::config"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/config/GitConfigUtil.java#getValues",
  "plugins/git4idea/backend/src/repo/GitConfig.kt#read",
]
deviations = [
  "Remotes only; no url.insteadOf substitution, branch tracking, hooks or core settings yet",
  "Valueless keys (`key\\0`) parse as `true`; upstream's parser merges them into the next record",
]

[modules."ig-git::branch"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/GitLocalBranch.kt",
  "plugins/git4idea/shared/src/git4idea/GitBranch.kt",
  "plugins/git4idea/shared/src/git4idea/GitStandardRemoteBranch.kt",
  "plugins/git4idea/shared/src/git4idea/GitSvnRemoteBranch.kt",
  "plugins/git4idea/backend/src/GitUtil.java#parseRemoteBranch",
]
deviations = ["Branch names compare case-sensitively (upstream: case-insensitive on case-insensitive file systems)", "Remote branches compare by remote name and branch name (upstream also compares urls/refspecs); no ordering defined yet"]

[modules."ig-git::reader"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/repo/GitRepositoryReader.kt"]
tests = ["plugins/git4idea/tests/git4idea/repo/GitRepositoryReaderTest.kt", "plugins/git4idea/testData/repo"]
deviations = [
  "Hooks info and shallow detection not yet ported",
  "Reftable: CHERRY_PICK_HEAD/REVERT_HEAD also checked via rev-parse (upstream only checks files)",
  "Reftable: current branch asked from git whenever no ref is HEAD (upstream only without local branches)",
  "Sequencer todo read as bytes; current-branch case fix uses Unicode lowercase",
]

[modules."ig-git::roots"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-impl/src/com/intellij/openapi/vcs/roots/VcsRootDetectorImpl.java",
  "plugins/git4idea/backend/src/roots/GitRootChecker.java",
]
deviations = ["Simplified: enclosing root below the user home + bounded downward scan; skips a fixed list of tool/cache dirs instead of .gitignore'd dirs"]

[modules."ig-log::graph_commit"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/GraphCommit.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/AbstractGraphCommit.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/GraphCommitImpl.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/IntGraphCommit.java",
]
deviations = ["Int-specialised commit classes not needed; GraphCommitImpl is generic over the id"]

[modules."ig-log::permanent_graph"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/PermanentGraph.kt"]
deviations = [
  "create_visible_graph returns Result and fails with UnsupportedGraphOptions for Bek, LinearBek, FirstParent and branch/text filters (Plan 2b)",
  "all_commits returns a lazy AllCommits view; the in-branch predicate is a ContainedInBranchCondition",
]

[modules."ig-log::visible_graph"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/VisibleGraph.kt",
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/RowInfo.java",
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/RowType.java",
]
deviations = ["get_row_info returns a boxed RowInfo borrowing the visible graph"]

[modules."ig-log::print_element"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/PrintElement.kt",
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/NodePrintElement.java",
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/EdgePrintElement.kt",
]

[modules."ig-log::graph_color_manager"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/GraphColorManager.java"]

[modules."ig-log::actions"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/actions/ActionController.java",
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/actions/GraphAction.kt",
  "platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/actions/GraphAnswer.java",
]
deviations = [
  "AWT Cursor becomes Cursor::{Default, Hand}",
  "The affected element is always a GraphPrintElement (no lookup of foreign PrintElements by equality)",
  "Runnable graph updater becomes GraphUpdater, run through VisibleGraphImpl::run_graph_updater",
]

[modules."ig-log::api"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/EdgeFilter.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/GraphLayout.java",
]

[modules."ig-log::api::elements"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/elements/GraphElement.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/elements/GraphNode.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/elements/GraphNodeType.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/elements/GraphEdge.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/elements/GraphEdgeType.java",
]
deviations = ["GraphElement is an enum over GraphNode and GraphEdge"]

[modules."ig-log::api::linear_graph"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/LinearGraph.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/LiteLinearGraph.java",
]

[modules."ig-log::api::permanent"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/permanent/PermanentCommitsInfo.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/permanent/PermanentGraphInfo.java",
]
deviations = ["PermanentGraphInfo.linear_graph returns an Arc handle"]

[modules."ig-log::api::printer"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/printer/GraphColorGetter.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/printer/GraphPrintElement.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/printer/PrintElementGenerator.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/printer/PrintElementPresentationManager.java",
]
deviations = ["GraphPrintElement is an enum over the two concrete print element types"]

[modules."ig-log::utils"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/TimestampGetter.java"]

[modules."ig-log::utils::flags"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/Flags.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/BitSetFlags.java",
]
tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/BitSetFlagsTest.java"]

[modules."ig-log::utils::normal_edge"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/NormalEdge.java"]

[modules."ig-log::utils::linear_graph_utils"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/LinearGraphUtils.java"]
deviations = ["DEFAULT_GRAPH_ANSWER is the function default_graph_answer()"]

[modules."ig-log::utils::dfs_util"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/DfsUtil.kt"]

[modules."ig-log::utils::int_int_multi_map"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntIntMultiMap.java"]
tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/IntIntMultiMapTest.kt"]

[modules."ig-log::utils::java_sort"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = []
deviations = [
  "IdeaGit helper replacing java.util.TimSort (Collections.sort, print element generator) / fastutil stableSort (IntArrayList.sort, layout builder): exact TimSort small-array path below 32 elements, stable merge sort above; never panics on inconsistent comparators",
  "Consistent comparators (strict weak orders): result identical to any stable sort, including TimSort and fastutil stableSort",
  "Inconsistent comparators: matches TimSort exactly below 32 elements; at 32 and above it can differ from Java, and Java may throw 'Comparison method violates its general contract'",
]

[modules."ig-log::utils::bounded_cache"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = []
deviations = ["IdeaGit helper replacing com.intellij.util.containers.SLRUMap with FIFO eviction; only memoises pure computations"]

[modules."ig-log::permanent::duplicate_parent_fixer"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/DuplicateParentFixer.java"]
deviations = ["Each commit is fixed once (Cow) instead of on every access of a lazy list"]

[modules."ig-log::permanent::permanent_linear_graph_builder"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentLinearGraphBuilder.java"]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphBuilderTest.kt",
  "platform/vcs-log/graph/testData/graphBuilder",
]
deviations = [
  "Bug fix: an edge to a not-loaded parent is written to that parent's slot, not the first free slot (upstream swaps parents when several are not loaded and processed out of order)",
  "Not-loaded parents sharing a smallest child are ordered by first appearance; Rust HashMap order is random per run (Java's is arbitrary but reproducible), so this keeps synthetic ids deterministic",
]

[modules."ig-log::permanent::permanent_linear_graph_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentLinearGraphImpl.java"]
tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphBuilderTest.kt"]

[modules."ig-log::permanent::graph_layout_builder"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilder.kt"]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilderTest.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilderHeadOrderTest.kt",
  "platform/vcs-log/graph/testData/layoutBuilder",
]
deviations = [
  "branches is an ordered slice (upstream: Set in hash order; matters only for comparator ties)",
  "No sortCatching: Rust comparators cannot throw",
]

[modules."ig-log::permanent::graph_layout_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutImpl.kt"]

[modules."ig-log::permanent::permanent_commits_info_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentCommitsInfoImpl.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/RowsMapping.kt",
]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphBuilderHashIndexTest.java",
  "platform/vcs-log/graph/testData/graphHashIndex",
]
deviations = [
  "No Int-specialised storage",
  "get_commit_id panics for an unknown negative id (upstream returns null)",
  "Plan 2c: get_node_id, convert_to_node_ids and contains_all use hash indexes (commit id -> first row, extra rows of duplicate ids, not-loaded id) instead of linear scans (upstream TODO 'optimize with special map'); results unchanged",
]

[modules."ig-log::print::edges_in_row_generator"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/EdgesInRowGenerator.java"]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/EdgesInRowTest.java",
  "platform/vcs-log/graph/testData/edgesInRow",
]
deviations = [
  "BTreeSet instead of HashSet (deterministic order); the print generator's edge comparator is not a strict weak order, so ties can resolve differently from IntelliJ in rare rows (mostly rows with not-loaded commit edges) and positions there can differ",
  "BoundedCache instead of SLRUMap",
  "Upstream limitation kept: an edge spanning more than about 2 * walk_size rows is not reported for rows far from both of its ends; get_edges_in_row is not an exact edge index",
]

[modules."ig-log::print::elements"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/elements/PrintElementBase.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/elements/SimplePrintElementImpl.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/elements/EdgePrintElementImpl.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/elements/TerminalEdgePrintElement.java",
]
deviations = [
  "color_id and is_selected are computed when the element is created, not on every access",
  "TerminalEdgePrintElement is the constructor EdgePrintElementImpl::terminal",
]

[modules."ig-log::print::graph_element_comparator_by_layout_index"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphElementComparatorByLayoutIndex.java"]

[modules."ig-log::print::graph_color_getter"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphColorGetterByHead.kt",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphColorGetterByNode.kt",
]

[modules."ig-log::print::print_element_generator_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/PrintElementGeneratorImpl.kt"]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/PrintElementGeneratorTest.kt",
  "platform/vcs-log/graph/testData/elementGenerator",
]
deviations = [
  "Owns its presentation manager (upstream shares it with VisibleGraphImpl)",
  "BoundedCache instead of SLRUMap",
  "Upstream bug fixed: recommended_width clamps the variance with max(0.0) before sqrt; upstream returns 0 for linear histories (negative variance, sqrt is NaN, NaN rounds to 0)",
  "In-row edges arrive in BTreeSet order instead of Java HashSet order; the edge comparator is not a strict weak order, so ties can resolve differently from IntelliJ in rare rows (mostly rows with not-loaded commit edges) and positions there can differ",
]

[modules."ig-log::collapsing::edge_storage"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/EdgeStorage.java"]
tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/EdgeStorageTest.kt"]

[modules."ig-log::collapsing::edge_storage_wrapper"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/EdgeStorageWrapper.java"]
tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/EdgeStorageTest.kt"]
deviations = [
  "Storage shared through Arc<RwLock<EdgeStorage>>; mutating methods take &self",
  "get_edges skips ids without a node index (upstream: NullPointerException)",
  "get_edges returns a BTreeSet (deterministic order; Java's HashSet order is deterministic, Rust's is not)",
]

[modules."ig-log::facade::base_controller"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/BaseController.java"]

[modules."ig-log::facade::graph_changes"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/GraphChanges.java",
  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/GraphChangesUtil.java",
]
deviations = ["Interfaces and *Impl classes collapse into structs; edges_replaced returns a Vec (upstream HashSet with identity equality)"]

[modules."ig-log::facade::linear_graph_controller"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphController.kt"]
deviations = ["compiled_graph returns an Arc handle; LinearGraphAction is GraphAction"]

[modules."ig-log::facade::linear_graph_wrapper"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphWrapper.kt"]

[modules."ig-log::facade::reachable_nodes"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/ReachableNodes.kt"]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/ContainingBranchesTest.java",
  "platform/vcs-log/graph/testData/containingBranches",
]
deviations = ["Mutex instead of synchronized"]

[modules."ig-log::facade::print_element_presentation_manager_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/PrintElementPresentationManagerImpl.kt"]

[modules."ig-log::facade::permanent_graph_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/PermanentGraphImpl.kt"]
deviations = [
  "Only Options.Base(Normal) without filters; others return UnsupportedGraphOptions (Plan 2b)",
  "Unfiltered Normal uses BaseController directly instead of CollapsedController over an all-visible CollapsedGraph (same compiled graph; no collapse/expand/fragment hover)",
  "Branch node ids sorted ascending before the layout builder",
  "No bekIntMap / SortedBaseController yet",
  "get_children returns an empty list for unknown or not-loaded commits (upstream throws)",
]

[modules."ig-log::facade::visible_graph_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/VisibleGraphImpl.kt"]
deviations = [
  "Implements ActionController itself (upstream inner class)",
  "BUTTON_COLLAPSE/BUTTON_EXPAND reported unsupported until CollapsedController (Plan 2b)",
  "buildSimpleGraphInfo / SimpleGraphInfo not ported",
  "Graph updater run via run_graph_updater",
  "Arrow actions on stale print elements (indices out of range / unknown not-loaded id) are ignored instead of throwing",
]

[modules."ig-log::tests::common"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/AbstractTestWithTextFile.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/AbstractTestWithTwoTextFile.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/GraphStrUtils.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/StrUtils.kt",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/TestGraphBuilder.kt",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/CommitIdManager.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/CommitParser.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/SimpleCommit.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/EdgeNodeCharConverter.java",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/LinearGraphParser.java",
]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/TestGraphBuilderTest.kt",
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/TestGraphBuilderTest.kt",
  "platform/vcs-log/graph/testData/testGraphBuilder",
  "platform/vcs-log/graph/testData/testParser",
]
deviations = ["Test support code; asTestGraphString and TestPermanentGraphInfo not ported yet (needed by Plan 2b tests)"]

# ---- Plan 2c: git log data

[modules."ig-git::natural_comparator"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/util/base/src/com/intellij/openapi/util/text/NaturalComparator.java",
  "plugins/git4idea/shared/src/git4idea/GitReference.kt#REFS_NAMES_COMPARATOR",
]
deviations = ["Character case mapping uses Rust's char::to_uppercase/to_lowercase for single-unit results (Java Character.toUpperCase/toLowerCase)"]

[modules."ig-git::log::parser"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/history/GitLogParser.java", "plugins/git4idea/backend/src/GitFormatException.java"]
tests = ["plugins/git4idea/tests/git4idea/history/GitLogParserTest.kt"]
deviations = [
  "Lines are processed byte-wise (markers are ASCII control characters), not as UTF-16 chars",
  "Format errors that upstream logs with LOG.error in parse() are dropped",
  "Random marker tails use std RandomState instead of ThreadLocalRandom",
  "Test harness: record hashes are a deterministic 40-hex digest instead of SHA-1 of the Kotlin map; Change assertions go through GitChangesParser.parseChange's path mapping (Change objects not ported)",
]

[modules."ig-git::log::record"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/history/GitLogRecord.java",
  "plugins/git4idea/backend/src/history/GitLogFullRecord.java",
  "plugins/git4idea/backend/src/history/GitLogRecordBuilder.kt",
  "plugins/git4idea/backend/src/history/GitChangeType.java",
  "plugins/git4idea/backend/src/history/GitChangesParser.java#getChangeType",
]
deviations = [
  "Option values are a fixed array indexed by GitLogOption (upstream Map)",
  "No setUsedHandler (only used in log messages)",
  "GitLogFullRecord keeps --name-status entries; getFilePaths(root)/parseChanges (Change objects) are out of scope until commit details",
  "InternedGitLogRecordBuilder not ported (no path interning)",
]

[modules."ig-git::log::unescape"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-impl/src/com/intellij/vcsUtil/VcsFileUtil.java#unescapeGitPath", "plugins/git4idea/backend/src/GitUtil.java#unescapePath"]
deviations = ["Octal escapes decode as UTF-8 (upstream: default charset)"]

[modules."ig-git::log::splitter"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/history/GitLogOutputSplitter.java"]
deviations = ["run_and_split bundles runCommandWithoutCollectingOutput(...).throwOnError() and reportErrors()"]

[modules."ig-git::log::collector"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/history/GitLogRecordCollector.java"]
deviations = [
  "createEmptyCopy is a closure; errors upstream only logs are kept for take_error, and a missing tree hash counts as different",
]

[modules."ig-git::log::util"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/history/GitLogUtil.java",
  "plugins/git4idea/backend/src/log/GitLogProvider.kt#DONT_CONSIDER_SHA",
  "plugins/git4idea/backend/src/branch/GitBranchUtil.java#getAllTags",
]
tests = ["plugins/git4idea/tests/git4idea/history/GitLogUtilTest.kt"]
deviations = [
  "Only timed commits, metadata and refs; readFullDetails*/GitFullDetailsCollector/GitCommitRequirements (changes) are out of scope",
  "read_timed_commits takes no file paths (filters are out of scope)",
  "GitLogUtilTest: the two in-scope cases read through metadata instead of full details; the renames and merge-diff cases are out of scope",
]

[modules."ig-git::log::ref_manager"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/log/GitRefManager.kt"]
tests = [
  "plugins/git4idea/tests/git4idea/log/GitBranchComparatorTest.kt",
  "plugins/git4idea/tests/git4idea/log/GitLabelComparatorTest.kt",
  "plugins/git4idea/tests/git4idea/log/GitRefManagerTestUtil.kt",
]
deviations = [
  "Ref types carry no colors (UI); compared by name and branch flag instead of identity",
  "Not ported: groupForTable, groupForBranchFilter (GitRefGroupsTest), favorites, serialize/deserialize",
  "The current branch comes from ig_git::log::repository_manager",
]

[modules."ig-git::log::repository_manager"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["plugins/git4idea/backend/src/repo/GitRepositoryManager.kt", "plugins/git4idea/backend/src/repo/GitRepositoryImpl.kt#update"]
deviations = [
  "Minimal stand-in: per-root GitBranchState, remotes and hash length (extensions.objectformat), updated explicitly (no VFS events)",
]

[modules."ig-git::log::provider"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "plugins/git4idea/backend/src/log/GitLogProvider.kt",
  "plugins/git4idea/backend/src/log/VcsLogObjectsFactoryExt.kt",
  "plugins/git4idea/backend/src/GitUserRegistry.java",
  "plugins/git4idea/backend/src/config/GitConfigUtil.java#getValue",
]
tests = [
  "plugins/git4idea/tests/git4idea/log/GitLogProviderTest.kt",
  "plugins/git4idea/testFramework/testSrc/git4idea/test/RefParser.java",
  "plugins/git4idea/tests/git4idea/log/TestVcsRefsSequences.kt",
]
deviations = [
  "Data reading only: readFirstBlock, readAllHashes, readMetadata, getCurrentUser, getCurrentBranch, isFullHash, getReferenceManager",
  "Not ported: GitLogExperimentalProvider (experimental refs collection), getCommitsMatchingFilter (filter tests), readFullDetails (test full details), getContainingBranches, subscribeToRootRefreshEvents, resolveReference, getVcsRoot, debug data validation",
  "Branch refs in name order and set-like collections in first-seen order (upstream HashSet order); new unmatched tags are loaded in name order",
]

[modules."ig-log::data::hash"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/api/src/com/intellij/vcs/log/Hash.java", "platform/vcs-impl/shared/src/com/intellij/vcs/log/impl/HashImpl.java"]
deviations = ["Separate from ig_git::hash::Hash (converted at the provider boundary); try_build returns an error instead of asserting"]

[modules."ig-log::data::commit"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/api/src/com/intellij/vcs/log/TimedVcsCommit.java",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsShortCommitDetails.java",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsCommitMetadata.java",
  "platform/vcs-log/api/src/com/intellij/vcs/log/CommitId.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/TimedVcsCommitImpl.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/VcsShortCommitDetailsImpl.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/VcsCommitMetadataImpl.java",
]

[modules."ig-log::data::vcs_user"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsUser.java",
  "platform/vcs-impl/shared/src/com/intellij/vcs/log/impl/VcsUserImpl.kt",
  "platform/vcs-impl/shared/src/com/intellij/vcs/log/util/VcsUserUtil.java",
]
deviations = ["Users are not interned (VcsUserRegistryImpl)"]

[modules."ig-log::data::vcs_ref"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsRef.java",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsRefType.java",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsRefsContainer.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/VcsRefImpl.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/EmptyRefs.kt",
]
deviations = ["VcsRefType has a name instead of a background color; names are not interned"]

[modules."ig-log::data::root"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/impl/src/com/intellij/vcs/log/util/VcsLogUtil.java#compareRoots"]
deviations = ["Roots are Arc<Path> instead of VirtualFile"]

[modules."ig-log::data::provider"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsLogProvider.kt",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsLogRefManager.java",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsLogProviderRequirementsEx.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/RequirementsImpl.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/SimpleLogProviderRequirements.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/impl/LogDataImpl.java",
]
deviations = [
  "Synchronous; readRecentCommits/RefsLoadingPolicy not modelled (readFirstBlock is the default)",
  "VcsLogRefManager has the two comparators plus name() (upstream toString); grouping, favorites and serialization not ported",
  "Filters, full details, containing branches and refresh subscriptions not ported",
]

[modules."ig-log::data::storage"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogStorage.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/InMemoryStorage.java",
]
deviations = ["In memory only (no VcsLogStorageImpl); get_commit_indexes interns a commit and its parents under one lock"]

[modules."ig-log::data::refs_model"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/RefsModel.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogRefsOfSingleRootFactory.kt",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsLogAggregatedStoredRefs.kt",
  "platform/vcs-log/api/src/com/intellij/vcs/log/VcsLogRootStoredRefs.kt",
]
deviations = ["get_ref_for_head_commit also caches 'no ref'; takes ref managers instead of providers"]

[modules."ig-log::data::sorter"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogSorter.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogJoiner.java#NewCommitIntegrator",
]
tests = ["platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogSorterTest.java", "platform/vcs-log/impl/test/com/intellij/vcs/log/TimedCommitParser.java"]
deviations = ["The next commit to integrate is the first remaining in input order (upstream: HashMap iteration order)"]

[modules."ig-log::data::joiner"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogJoiner.java",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogRefreshNotEnoughDataException.java",
]
tests = ["platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogJoinerTest.kt"]
deviations = ["Exceptions are JoinError values; new commits are integrated in first-block order (upstream HashSet order)"]

[modules."ig-log::data::multi_repo_joiner"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogMultiRepoJoiner.java"]
tests = ["platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogMultiRepoJoinerTest.java"]
deviations = ["Equal timestamps go to the last log in the given (root path) order (upstream: HashMap order)"]

[modules."ig-log::data::graph_color_manager_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/graph/GraphColorManagerImpl.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/graph/HeadCommitsComparator.java",
]
deviations = ["Ref managers told apart by Arc identity and ordered by name() (upstream equals/toString); 'no refs at head' not logged"]

[modules."ig-log::data::graph_data"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = [
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogGraphData.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogGraphDataFactory.kt",
  "platform/vcs-log/impl/src/com/intellij/vcs/log/data/DataPack.kt",
]
deviations = ["No OverlayData; an empty or failed pack has no permanent graph (upstream EmptyPermanentGraph)"]

[modules."ig-log::data::refresher"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogRefresherImpl.kt"]
tests = [
  "platform/vcs-log/impl/test/com/intellij/vcs/log/data/VcsLogRefresherTest.kt",
  "platform/vcs-log/impl/test/com/intellij/vcs/log/impl/TestVcsLogProvider.java",
]
deviations = [
  "Synchronous: no request channel/batching, progress, cancellation, overlay data or tracing; the caller serializes calls on a background thread",
  "VcsLogRefresherTest: the blocking/cancellation/batching cases (test_first_refresh_waits_for_full_log, test_reinitialize_makes_refresh_cancelled, test_two_immediately_consecutive_refreshes...) don't apply",
  "Every commit list is checked for unique ids and topological order before building the graph (upstream builds silently); a failing joined log forces a full reload, a failing first block or full log gives an error data pack",
  "Roots are processed in path order (upstream HashMap order)",
  "Recent first-block metadata goes to an optional consumer (upstream VcsLogCommitDataConsumer.storeRecentDetails)",
]
```

- [ ] **Step 2: Verify everything**

```bash
cargo fmt --all --check
cargo clippy -q --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all
cargo test --all
```
Expected: all clean, and 334 tests pass with 1 ignored (the performance smoke test).

- [ ] **Final step: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add -A crates reference Cargo.lock
git commit -m "docs: upstream map for the git log data layer"
```

---

