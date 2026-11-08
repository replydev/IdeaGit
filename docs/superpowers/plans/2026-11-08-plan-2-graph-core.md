# IdeaGit Plan 2 — Commit Graph Core (`ig-log`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `ig-log` crate: a faithful port of IntelliJ's VCS Log commit graph (`platform/vcs-log/graph` + `graph-api`). It builds the permanent graph, lays it out, and produces per-row print elements for the default `Normal` sort without filters, with the upstream graph tests ported and passing.

**Architecture:** Plain Rust library, no UI dependency, generic over the commit id type. Module layout mirrors upstream packages: the top-level modules are the public `graph-api`, `api` is the internal graph API, and `permanent`, `print`, `facade`, `collapsing` and `utils` are the `impl` packages. Upstream names are kept in snake_case so upstream diffs map file-to-file. Node indexes and ids are `i32` (not-loaded commits have negative ids). Upstream exceptions are panics, and upstream `assert`s are `debug_assert!`s.

**Tech Stack:** Rust 2024, `thiserror`. Tests use upstream text test data, copied byte-identically.

**Spec:** `docs/superpowers/specs/2026-11-08-ideagit-rust-design.md` (Git Log section).

> **Note:** the code blocks below reflect the state at Task 1. Review fixes applied during execution (updater API, stale-element guard, edge-storage locking, width clamp, `get_row_info` assert, re-exports, …) live in the crate and in `reference/upstream-map.toml`; the crate is the source of truth.

---

## Where this plan fits

Slice-1 log work is split into three plans:
- **Plan 2 (this):** graph core for `Options.Base(SortType.Normal)`, the IntelliJ default ("Off"), unfiltered.
- **Plan 2b:** BEK and linear BEK sorts, first-parent, branch and text filtering, collapsing and fragment hover. This means `CollapsedController`, `CollapsedGraph`, `DottedFilterEdgesGenerator`, `FragmentGenerator` and `LinearBek*`, plus the not-yet-ported upstream tests: `BfsTests`, `GraphUtilTests`, `UnsignedBitSetTest`, `*IntToIntMapTest`, `DottedFilterEdgesGeneratorTest`, `FragmentGeneratorTest`, `InplaceFilterEdgesGeneratorTest`, `FirstParentTest` and `Bek*Test`.
- **Plan 2c:** git log data: `GitLogProvider`, `GitLogUtil`, `GitLogParser`, refs, and loading commits into the graph.

## Why the tests come after the code here

The crate's modules depend on each other in cycles, as upstream's do. For example, `api::linear_graph` → `visible_graph` → `actions` → `facade::visible_graph_impl`, and `utils::linear_graph_utils` → `facade::linear_graph_controller`. The shared test parser also uses most of the crate. The crate therefore compiles only as a whole, and a module-by-module red/green order would require throwaway stubs.

So Task 1 adds the complete crate source, which was verified in a prototype. Each later task then ports one upstream test class with its byte-identical test data, runs it, and has the module group it covers reviewed. These are acceptance tests that prove fidelity to upstream. No expected output may be weakened, and a failing case means the port is wrong.

## Prerequisites

- The upstream reference clone from Plan 1 must exist at `IJ=~/Code/intellij-community`, with `platform/vcs-log` in its sparse checkout (`git -C "$IJ" sparse-checkout add platform/vcs-log`). Test data comes from `$IJ/platform/vcs-log/graph/testData/`.

## Deviations from upstream (also recorded in `reference/upstream-map.toml`)

- **Upstream bugs fixed:**
  - `PermanentLinearGraphBuilder` put the edge to a not-loaded parent into the child's first free slot, which could swap parents. It now uses the parent's position, covered by the `not_loaded_parents_keep_their_order` test.
  - The ids of not-loaded parents depended on `HashMap` iteration order. They are now assigned in first-seen order.
- **Scope:**
  - `create_visible_graph` returns `Err(UnsupportedGraphOptions)` for `Bek`, `LinearBek`, `FirstParent`, visible-branch heads and matched commits.
  - Unfiltered `Normal` uses `BaseController` directly. Upstream wraps it in a `CollapsedController` over an all-visible graph, which yields the same compiled graph but also gives fragment hover. Plan 2b restores that.
  - Collapse and expand buttons are unsupported. `SimpleGraphInfo` is not ported.
- **Rust adaptations:**
  - `utils::java_sort` replaces Java's `TimSort`, because upstream comparators are not total orders and Rust's sort may panic. It is exact below 32 elements and a stable merge sort above.
  - Edges-in-row use `BTreeSet`s, and `BoundedCache` (FIFO) replaces `SLRUMap`.
  - Branch node ids are sorted before layout, which only matters when the heads comparator ties.
  - Element colors and selection are computed when an element is created.
  - Some upstream types map to Rust types: `GraphUpdater` for `Runnable`, `Cursor` for the AWT cursor, enums for `GraphElement`/`GraphPrintElement`, and `Arc<RwLock<_>>` for the shared `EdgeStorage`.

## Notes for later plans

- `VisibleGraphImpl` is `Send` but not `Sync` (its caches use `RefCell`), so the UI must own it on one thread. A `RowInfo` borrows the visible graph. Print elements come back with nodes last, so they are drawn over edges. After an action, run any returned `GraphUpdater`.
- `PermanentCommitsInfoImpl::get_node_id` is an O(n) lookup, as upstream (marked with a TODO there). Plan 2c should add an index and record it as a deviation.
- `get_children` returns an empty list for unknown or not-loaded commits (deviation: upstream throws). `ReachableNodes` serializes queries through a `Mutex`.

### From the Task 1 design review
- **Plan 2b: controller introspection.** Add default methods to `LinearGraphController`, e.g. `delegate()`, `as_any_mut()` and `supports_collapse()`. These replace upstream's downcasts: `isActionSupported` checking `!is FilteredController`, `performActionRecursively` and `GraphModificationUtil`.
- **Plan 2b: mutation model.** Updaters are `FnOnce + Send + 'static`, and `LinearGraph` is `&self + Send + Sync`. Collapsing and linear-BEK state must therefore live behind `Arc<RwLock<_>>` inside the same `Arc<dyn LinearGraph>`, as `EdgeStorageWrapper` does. Never swap the `Arc` from inside an updater.
- **UI threading:**
  - Call `recommended_width()` on the background thread, since it is lazy and samples up to 20k rows.
  - Run `get_containing_branches` / `get_contained_in_branch_condition` off the UI thread. They do a full DFS under a `Mutex`.
  - Drop hovered or selected print elements when the visible graph is replaced, because stale indexes can panic.
- **UI ergonomics:** `action_controller(&mut self)` requires `&mut` even for read-only queries, as upstream. Wrap it in UI helpers. Consider marking internal modules `#[doc(hidden)]`.
- **Plan 2c:** guarantee unique commit ids (duplicates are accepted silently, as upstream does), and add the node-id index.
- **Plan 2c (Task 4 review):**
  - Feed commits in topological order (`--topo-order`/`--date-order`), or check the order. A parent listed before its child is silently treated as not loaded, as upstream does.
  - Optimise for 100k+ histories: `SmallVec` for pending up-nodes and adjacent edges, a faster hasher, and pre-sized maps. `contains_all` and `convert_to_node_ids_skipping` are O(n·m)/O(n), alongside `get_node_id`.

## File structure

```
crates/ig-log/
  Cargo.toml
  src/lib.rs, graph_commit.rs, permanent_graph.rs, visible_graph.rs,
      print_element.rs, graph_color_manager.rs, actions.rs        public graph API (graph-api)
  src/api/          internal graph API: elements, linear_graph, permanent, printer
  src/utils/        flags, normal_edge, int_int_multi_map, java_sort, bounded_cache,
                    linear_graph_utils, dfs_util
  src/permanent/    duplicate_parent_fixer, permanent_linear_graph_builder/_impl,
                    permanent_commits_info_impl, graph_layout_builder/_impl
  src/print/        elements, comparator, color getters, edges_in_row_generator,
                    print_element_generator_impl
  src/collapsing/   edge_storage, edge_storage_wrapper (shared with Plan 2b)
  src/facade/       graph_changes, controllers, wrapper, reachable_nodes,
                    presentation manager, permanent_graph_impl, visible_graph_impl
  tests/common/     ported test infrastructure (parser, str utils, TestGraphBuilder)
  tests/*.rs        one target per ported upstream test class
  tests/fixtures/   byte-identical copies of upstream graph/testData/*
```

---

### Task 1: The `ig-log` crate

**Files:**
- Modify: `Cargo.toml` (workspace members), `.gitattributes`, `reference/upstream-map.toml`
- Create: `crates/ig-log/Cargo.toml`, every file under `crates/ig-log/src/`

- [ ] **Step 1: Register the crate.** In the root `Cargo.toml`, change `members = ["crates/ig-git"]` to:

```toml
members = ["crates/ig-git", "crates/ig-log"]
```

Append to `.gitattributes`:
```
crates/ig-log/tests/fixtures/** -text
```

- [ ] **Step 2: Create `crates/ig-log/Cargo.toml`**

```toml
[package]
name = "ig-log"
version = "0.1.0"
edition.workspace = true
license.workspace = true
rust-version.workspace = true

[dependencies]
thiserror.workspace = true
```

- [ ] **Step 3: Crate root and public graph API.** Create these files with exactly this content.

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

pub mod actions;
pub mod api;
pub mod collapsing;
pub mod facade;
pub mod graph_color_manager;
pub mod graph_commit;
pub mod permanent;
pub mod permanent_graph;
pub mod print;
pub mod print_element;
pub mod utils;
pub mod visible_graph;
```

`crates/ig-log/src/graph_commit.rs`:

```rust
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
```

`crates/ig-log/src/permanent_graph.rs`:

```rust
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
    /// If `index >= len()`.
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
    type Visible: VisibleGraph<Id>;

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
```

`crates/ig-log/src/visible_graph.rs`:

```rust
//! The part of the permanent graph that is drawn on screen, and its rows.
//!
//! Ported from `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/VisibleGraph.kt`,
//! `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/RowInfo.java` and
//! `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/RowType.java`.

use crate::actions::ActionController;
use crate::api::printer::GraphPrintElement;

/// Row index used in [`VisibleGraph`]; see [`VisibleGraph::get_visible_row_index`].
pub type VcsLogVisibleGraphIndex = i32;

/// Visible commit graph row type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowType {
    Normal,
    /// The commit on this row doesn't match current filters.
    Unmatched,
}

/// Visible commit graph row data.
pub trait RowInfo<Id> {
    fn commit(&self) -> Id;

    fn one_of_heads(&self) -> Id;

    /// Edges first, nodes last (nodes are drawn over the edges).
    fn print_elements(&self) -> Vec<GraphPrintElement>;

    fn row_type(&self) -> RowType;

    /// Rows of parents (`parent == true`) or children.
    fn adjacent_rows(&self, parent: bool) -> Vec<VcsLogVisibleGraphIndex>;
}

/// A part of the permanent graph which is drawn on screen (e.g. with applied filters). One per
/// client; all access must be synchronized. It refers to the permanent graph and occupies little
/// memory on its own.
pub trait VisibleGraph<Id> {
    fn visible_commit_count(&self) -> i32;

    fn recommended_width(&self) -> i32;

    fn action_controller(&mut self) -> &mut dyn ActionController<Id>;

    fn get_row_info(&self, visible_row: VcsLogVisibleGraphIndex) -> Box<dyn RowInfo<Id> + '_>;

    fn get_visible_row_index(&self, id: &Id) -> Option<VcsLogVisibleGraphIndex>;
}
```

`crates/ig-log/src/print_element.rs`:

```rust
//! Elements of the visible commit graph, as drawn by the UI.
//!
//! Ported from `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/PrintElement.kt`,
//! `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/NodePrintElement.java` and
//! `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/EdgePrintElement.kt`.

use crate::visible_graph::VcsLogVisibleGraphIndex;

/// Element in the visible commit graph.
pub trait PrintElement {
    fn row_index(&self) -> VcsLogVisibleGraphIndex;
    fn position_in_current_row(&self) -> i32;
    fn color_id(&self) -> i32;
    fn is_selected(&self) -> bool;
}

/// How a node is painted (`NodePrintElement.Type`, experimental upstream).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum NodePrintElementType {
    #[default]
    Fill,
    Outline,
    OutlineAndFill,
}

/// A commit node.
pub trait NodePrintElement: PrintElement {
    fn node_type(&self) -> NodePrintElementType {
        NodePrintElementType::Fill
    }
}

/// Direction of an edge part, relative to the row it is drawn in (`EdgePrintElement.Type`).
/// Ordinals matter for the test printer: `Up` = 0, `Down` = 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EdgePrintElementType {
    Up,
    Down,
}

impl EdgePrintElementType {
    pub fn ordinal(self) -> i32 {
        self as i32
    }

    /// Upstream enum constant name.
    pub fn name(self) -> &'static str {
        match self {
            EdgePrintElementType::Up => "UP",
            EdgePrintElementType::Down => "DOWN",
        }
    }
}

/// `EdgePrintElement.LineStyle`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
}

impl LineStyle {
    /// Upstream enum constant name.
    pub fn name(self) -> &'static str {
        match self {
            LineStyle::Solid => "SOLID",
            LineStyle::Dashed => "DASHED",
            LineStyle::Dotted => "DOTTED",
        }
    }
}

/// Half of an edge drawn in one row, going up or down to `position_in_other_row`.
pub trait EdgePrintElement: PrintElement {
    fn position_in_other_row(&self) -> i32;
    fn edge_print_type(&self) -> EdgePrintElementType;
    fn line_style(&self) -> LineStyle;
    fn has_arrow(&self) -> bool;
}
```

`crates/ig-log/src/graph_color_manager.rs`:

```rust
//! Branch colors.
//!
//! Ported from `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/GraphColorManager.java`.

/// Supplies color ids for fragments of branches.
pub trait GraphColorManager<CommitId>: Send + Sync {
    /// Color for drawing a fragment of a branch, based on the branch head, the index of the head
    /// commit's fragment and the index of the target fragment. This allows coloring non-main
    /// fragments separately (e.g. a feature branch merged into the main branch), even though they
    /// belong to the same branch in terms of VCS.
    fn get_color(
        &self,
        head_commit: &CommitId,
        head_fragment_index: i32,
        fragment_index: i32,
    ) -> i32;
}
```

`crates/ig-log/src/actions.rs`:

```rust
//! Mouse and button actions on the visible graph, and the graph's answers to them.
//!
//! Ported from `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/actions/`
//! (`ActionController.java`, `GraphAction.kt`, `GraphAnswer.java`).
//!
//! Deviations: the AWT `Cursor` becomes [`Cursor`]; the affected element is always a
//! [`GraphPrintElement`] (the UI passes back what `RowInfo::print_elements` gave it); the
//! `Runnable` graph updater becomes a [`GraphUpdater`] that the caller hands back to
//! [`VisibleGraphImpl::run_graph_updater`](crate::facade::visible_graph_impl::VisibleGraphImpl::run_graph_updater).

use std::fmt;

use crate::api::printer::GraphPrintElement;

/// Mouse cursor the UI should show (`Cursor.getDefaultCursor()` / `Cursor.HAND_CURSOR`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cursor {
    Default,
    Hand,
}

/// `GraphAction.Type`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GraphActionType {
    MouseClick,
    MouseOver,
    ButtonCollapse,
    ButtonExpand,
}

/// An action on the graph (`GraphAction.GraphActionImpl`).
#[derive(Clone, Debug, PartialEq)]
pub struct GraphAction {
    pub affected_element: Option<GraphPrintElement>,
    pub action_type: GraphActionType,
}

impl GraphAction {
    pub fn new(
        affected_element: Option<GraphPrintElement>,
        action_type: GraphActionType,
    ) -> GraphAction {
        GraphAction {
            affected_element,
            action_type,
        }
    }
}

/// Deferred graph modification returned by an action (upstream `Runnable`).
pub struct GraphUpdater(Box<dyn FnOnce() + Send>);

impl GraphUpdater {
    pub fn new(update: impl FnOnce() + Send + 'static) -> GraphUpdater {
        GraphUpdater(Box::new(update))
    }

    pub fn run(self) {
        (self.0)()
    }
}

impl fmt::Debug for GraphUpdater {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("GraphUpdater")
    }
}

/// The graph's reaction to [`ActionController::perform_action`].
#[derive(Debug)]
pub struct GraphAnswer<Id> {
    pub cursor_to_set: Option<Cursor>,
    pub commit_to_jump: Option<Id>,
    pub graph_updater: Option<GraphUpdater>,
    pub do_jump: bool,
    pub is_repaint_required: bool,
}

/// Handles user actions on a visible graph.
pub trait ActionController<Id> {
    fn perform_action(&mut self, graph_action: GraphAction) -> GraphAnswer<Id>;

    fn are_long_edges_hidden(&self) -> bool;

    fn set_long_edges_hidden(&mut self, long_edges_hidden: bool);

    fn is_action_supported(&self, _action: &GraphAction) -> bool {
        true
    }
}
```

- [ ] **Step 4: Internal graph API (`api`).** Create these files with exactly this content.

`crates/ig-log/src/api/mod.rs`:

```rust
//! Internal graph API: edge filters and the graph layout, plus the submodules for elements, linear
//! graphs, permanent graph info and printing.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/EdgeFilter.java` and
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/GraphLayout.java`.

pub mod elements;
pub mod linear_graph;
pub mod permanent;
pub mod printer;

/// Which adjacent edges [`LinearGraph::get_adjacent_edges`](linear_graph::LinearGraph::get_adjacent_edges)
/// returns: normal edges going up and/or down, and/or special (non-normal) edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EdgeFilter {
    All,
    NormalDown,
    NormalUp,
    NormalAll,
    Special,
}

impl EdgeFilter {
    pub fn up_normal(self) -> bool {
        matches!(
            self,
            EdgeFilter::All | EdgeFilter::NormalUp | EdgeFilter::NormalAll
        )
    }

    pub fn down_normal(self) -> bool {
        matches!(
            self,
            EdgeFilter::All | EdgeFilter::NormalDown | EdgeFilter::NormalAll
        )
    }

    pub fn special(self) -> bool {
        matches!(self, EdgeFilter::All | EdgeFilter::Special)
    }
}

/// Layout of the permanent graph: every node belongs to a "layout index" (a fragment started by one
/// of the head nodes, in head order). Node indexes must be in `0..graph.nodes_count()`.
pub trait GraphLayout: Send + Sync {
    fn get_layout_index(&self, node_index: i32) -> i32;

    fn get_one_of_head_node_index(&self, node_index: i32) -> i32;

    /// Head nodes that start a layout fragment, sorted by layout index.
    fn get_head_node_index(&self) -> &[i32];
}
```

`crates/ig-log/src/api/elements.rs`:

```rust
//! Graph elements: nodes and edges of a [`LinearGraph`](crate::api::linear_graph::LinearGraph).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/elements/`
//! (`GraphElement.java`, `GraphNode.java`, `GraphNodeType.java`, `GraphEdge.java`, `GraphEdgeType.java`).

/// Kind of a [`GraphEdge`]. Ordinals match upstream (they are packed into
/// [`EdgeStorage`](crate::collapsing::edge_storage::EdgeStorage)).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GraphEdgeType {
    /// Between visible delegate nodes.
    Usual,
    /// Collapsed fragment.
    Dotted,
    /// Edge to a commit that is not loaded.
    NotLoadCommit,
    DottedArrowUp,
    DottedArrowDown,
}

impl GraphEdgeType {
    /// All values in ordinal order (`GraphEdgeType.values()`).
    pub const VALUES: [GraphEdgeType; 5] = [
        GraphEdgeType::Usual,
        GraphEdgeType::Dotted,
        GraphEdgeType::NotLoadCommit,
        GraphEdgeType::DottedArrowUp,
        GraphEdgeType::DottedArrowDown,
    ];

    /// Normal edges connect two nodes; the others have one node and a target id.
    pub fn is_normal_edge(self) -> bool {
        matches!(self, GraphEdgeType::Usual | GraphEdgeType::Dotted)
    }

    /// Java `Enum.ordinal()`.
    pub fn ordinal(self) -> i32 {
        self as i32
    }
}

/// Kind of a [`GraphNode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GraphNodeType {
    Usual,
    NotLoadCommit,
    Unmatched,
}

/// A node of a linear graph, identified by its index (row) in that graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GraphNode {
    pub node_index: i32,
    pub node_type: GraphNodeType,
}

impl GraphNode {
    /// A [`GraphNodeType::Usual`] node.
    pub fn new(node_index: i32) -> GraphNode {
        GraphNode::with_type(node_index, GraphNodeType::Usual)
    }

    pub fn with_type(node_index: i32, node_type: GraphNodeType) -> GraphNode {
        GraphNode {
            node_index,
            node_type,
        }
    }
}

/// An edge of a linear graph. Normal edges have both node indexes; special edges have one node index
/// and an optional target id. Equality covers all four fields, like upstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GraphEdge {
    pub up_node_index: Option<i32>,
    pub down_node_index: Option<i32>,
    pub target_id: Option<i32>,
    pub edge_type: GraphEdgeType,
}

impl GraphEdge {
    pub fn new(
        up_node_index: Option<i32>,
        down_node_index: Option<i32>,
        target_id: Option<i32>,
        edge_type: GraphEdgeType,
    ) -> GraphEdge {
        GraphEdge {
            up_node_index,
            down_node_index,
            target_id,
            edge_type,
        }
    }

    /// `GraphEdge.createNormalEdge`: orders the two node indexes so the smaller one is up.
    pub fn create_normal_edge(
        node_index1: i32,
        node_index2: i32,
        edge_type: GraphEdgeType,
    ) -> GraphEdge {
        debug_assert!(
            edge_type.is_normal_edge(),
            "Unexpected edge type: {edge_type:?}"
        );
        GraphEdge::new(
            Some(node_index1.min(node_index2)),
            Some(node_index1.max(node_index2)),
            None,
            edge_type,
        )
    }

    /// `GraphEdge.createEdgeWithTargetId`.
    ///
    /// # Panics
    /// If `edge_type` is a normal edge type (upstream throws `AssertionError`).
    pub fn create_edge_with_target_id(
        node_index: i32,
        target_id: Option<i32>,
        edge_type: GraphEdgeType,
    ) -> GraphEdge {
        match edge_type {
            GraphEdgeType::DottedArrowUp => {
                GraphEdge::new(None, Some(node_index), target_id, edge_type)
            }
            GraphEdgeType::NotLoadCommit | GraphEdgeType::DottedArrowDown => {
                GraphEdge::new(Some(node_index), None, target_id, edge_type)
            }
            _ => panic!("Unexpected edge type: {edge_type:?}"),
        }
    }
}

/// A node or an edge (`GraphElement` marker interface).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GraphElement {
    Node(GraphNode),
    Edge(GraphEdge),
}

impl From<GraphNode> for GraphElement {
    fn from(node: GraphNode) -> Self {
        GraphElement::Node(node)
    }
}

impl From<GraphEdge> for GraphElement {
    fn from(edge: GraphEdge) -> Self {
        GraphElement::Edge(edge)
    }
}
```

`crates/ig-log/src/api/linear_graph.rs`:

```rust
//! The linear graph: commits as rows (node indexes) connected by edges.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/LinearGraph.kt` and
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/LiteLinearGraph.java`.

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphNode};
use crate::api::permanent::VcsLogGraphNodeId;
use crate::visible_graph::VcsLogVisibleGraphIndex;

/// A graph whose nodes are rows `0..nodes_count()`. Each node also has a stable node id
/// (the node index in the permanent graph).
pub trait LinearGraph: Send + Sync {
    fn nodes_count(&self) -> i32;

    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge>;

    fn get_graph_node(&self, node_index: i32) -> GraphNode;

    fn get_node_id(&self, node_index: VcsLogVisibleGraphIndex) -> VcsLogGraphNodeId;

    /// `None` if the node doesn't exist in this graph.
    fn get_node_index(&self, node_id: VcsLogGraphNodeId) -> Option<VcsLogVisibleGraphIndex>;
}

/// Which neighbours [`LiteLinearGraph::get_nodes`] returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeFilter {
    Up,
    Down,
    All,
}

impl NodeFilter {
    pub fn up(self) -> bool {
        matches!(self, NodeFilter::Up | NodeFilter::All)
    }

    pub fn down(self) -> bool {
        matches!(self, NodeFilter::Down | NodeFilter::All)
    }

    pub fn edge_filter(self) -> EdgeFilter {
        match self {
            NodeFilter::Up => EdgeFilter::NormalUp,
            NodeFilter::Down => EdgeFilter::NormalDown,
            NodeFilter::All => EdgeFilter::NormalAll,
        }
    }

    pub fn filter(is_up: bool) -> NodeFilter {
        if is_up {
            NodeFilter::Up
        } else {
            NodeFilter::Down
        }
    }
}

/// A graph view that only knows adjacent node indexes (normal edges only).
pub trait LiteLinearGraph {
    fn nodes_count(&self) -> i32;

    fn get_nodes(&self, node_index: i32, filter: NodeFilter) -> Vec<i32>;
}
```

`crates/ig-log/src/api/permanent.rs`:

```rust
//! Access to the permanent graph's commits, linear graph and layout.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/permanent/PermanentCommitsInfo.kt`
//! and `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/permanent/PermanentGraphInfo.java`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::api::GraphLayout;
use crate::api::linear_graph::LinearGraph;

/// Commit node id: the row of a loaded commit in the permanent graph (`>= 0`), or a synthetic
/// negative id (`<= -2`) for a parent that is not loaded. `-1` means "unknown".
pub type VcsLogGraphNodeId = i32;

/// Maps node ids of the permanent graph to commit ids and timestamps.
pub trait PermanentCommitsInfo<CommitId>: Send + Sync {
    fn get_commit_id(&self, node_id: VcsLogGraphNodeId) -> CommitId;

    fn get_timestamp(&self, node_id: VcsLogGraphNodeId) -> i64;

    /// See [`PermanentCommitsInfoImpl::get_node_id`](crate::permanent::permanent_commits_info_impl::PermanentCommitsInfoImpl::get_node_id)
    /// for the possible values.
    fn get_node_id(&self, commit_id: &CommitId) -> VcsLogGraphNodeId;

    fn convert_to_node_ids(&self, commit_ids: &HashSet<CommitId>) -> HashSet<VcsLogGraphNodeId>;
}

/// Everything a visible graph needs to know about the permanent graph it was created from.
pub trait PermanentGraphInfo<CommitId>: Send + Sync {
    fn permanent_commits_info(&self) -> &dyn PermanentCommitsInfo<CommitId>;

    /// Returns a shared handle (upstream returns the object itself).
    fn linear_graph(&self) -> Arc<dyn LinearGraph>;

    fn permanent_graph_layout(&self) -> &dyn GraphLayout;

    fn branch_node_ids(&self) -> &HashSet<VcsLogGraphNodeId>;
}
```

`crates/ig-log/src/api/printer.rs`:

```rust
//! Printing API: color getters, the print element generator and the presentation manager.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/printer/`
//! (`GraphColorGetter.kt`, `GraphPrintElement.kt`, `PrintElementGenerator.java`,
//! `PrintElementPresentationManager.java`).
//!
//! Deviation: `GraphPrintElement` is an enum over the two concrete print element types instead of
//! an interface; UI code matches on it.

use std::sync::Arc;

use crate::api::elements::GraphElement;
use crate::api::permanent::{PermanentGraphInfo, VcsLogGraphNodeId};
use crate::print::elements::{EdgePrintElementImpl, SimplePrintElementImpl};
use crate::print_element::PrintElement;
use crate::visible_graph::VcsLogVisibleGraphIndex;

/// Generates color ids for graph nodes.
pub trait GraphColorGetter: Send + Sync {
    /// Color id for `node_id` (a [`PermanentCommitsInfo`](crate::api::permanent::PermanentCommitsInfo)
    /// node id) and `layout_index` (the fragment the node belongs to, see
    /// [`GraphLayout::get_layout_index`](crate::api::GraphLayout::get_layout_index)).
    fn get_node_color(&self, node_id: VcsLogGraphNodeId, layout_index: i32) -> i32;
}

/// A factory for [`GraphColorGetter`].
pub trait GraphColorGetterFactory<CommitId> {
    /// `permanent_graph_info` is used for node id to commit id conversion, layout indexes, etc.
    fn create_color_getter(
        &self,
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    ) -> Arc<dyn GraphColorGetter>;
}

/// A print element together with the graph element it was created for.
#[derive(Clone, Debug)]
pub enum GraphPrintElement {
    Node(SimplePrintElementImpl),
    Edge(EdgePrintElementImpl),
}

impl GraphPrintElement {
    pub fn graph_element(&self) -> GraphElement {
        match self {
            GraphPrintElement::Node(node) => GraphElement::Node(node.graph_node()),
            GraphPrintElement::Edge(edge) => GraphElement::Edge(edge.graph_edge()),
        }
    }

    fn as_print_element(&self) -> &dyn PrintElement {
        match self {
            GraphPrintElement::Node(node) => node,
            GraphPrintElement::Edge(edge) => edge,
        }
    }
}

impl PrintElement for GraphPrintElement {
    fn row_index(&self) -> VcsLogVisibleGraphIndex {
        self.as_print_element().row_index()
    }
    fn position_in_current_row(&self) -> i32 {
        self.as_print_element().position_in_current_row()
    }
    fn color_id(&self) -> i32 {
        self.as_print_element().color_id()
    }
    fn is_selected(&self) -> bool {
        self.as_print_element().is_selected()
    }
}

/// Upstream `equals`: nodes equal nodes at the same place, edges equal edges with the same
/// positions, direction and arrow; a node never equals an edge.
impl PartialEq for GraphPrintElement {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (GraphPrintElement::Node(a), GraphPrintElement::Node(b)) => a == b,
            (GraphPrintElement::Edge(a), GraphPrintElement::Edge(b)) => a == b,
            _ => false,
        }
    }
}

/// Produces the print elements of a visible row.
pub trait PrintElementGenerator {
    fn get_print_elements(&self, visible_row: VcsLogVisibleGraphIndex) -> Vec<GraphPrintElement>;
}

/// Decides colors and selection of print elements.
pub trait PrintElementPresentationManager {
    fn is_selected(&self, print_element: &GraphPrintElement) -> bool;

    fn get_color_id(&self, element: &GraphElement) -> i32;
}
```

- [ ] **Step 5: Utilities (`utils`).** Create these files with exactly this content.

`crates/ig-log/src/utils/mod.rs`:

```rust
//! Graph utilities, plus the [`TimestampGetter`] interface.
//!
//! `TimestampGetter` is ported from
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/TimestampGetter.java`.
//!
//! Not ported (not needed by the `Normal` graph): `BfsUtil.kt`, `GraphUtil.kt`, `IntHashSetFlags.kt`,
//! `UnsignedBitSet.java`, `IntList.java` and the `IntToIntMap` family (`utils/impl/*IntToIntMap.java`,
//! `PredicateFlags.java`); they arrive with collapsing, BEK and filters in Plan 2b.

pub mod bounded_cache;
pub mod dfs_util;
pub mod flags;
pub mod int_int_multi_map;
pub mod java_sort;
pub mod linear_graph_utils;
pub mod normal_edge;

/// Timestamp of the commit in a row (`TimestampGetter`).
pub trait TimestampGetter {
    fn get_timestamp(&self, index: i32) -> i64;
}
```

`crates/ig-log/src/utils/flags.rs`:

```rust
//! Fixed-size boolean flags.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/Flags.java` and
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/BitSetFlags.java`.

use std::fmt;

/// A fixed number of boolean flags indexed `0..size()`.
pub trait Flags {
    fn size(&self) -> i32;

    fn get(&self, index: i32) -> bool;

    fn set(&mut self, index: i32, value: bool);

    fn set_all(&mut self, value: bool);
}

/// [`Flags`] backed by a bit set. All flags start `false`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct BitSetFlags {
    size: i32,
    words: Vec<u64>,
}

impl BitSetFlags {
    /// # Panics
    /// If `size < 0` (upstream `NegativeArraySizeException`).
    pub fn new(size: i32) -> BitSetFlags {
        assert!(size >= 0, "size < 0: {size}");
        BitSetFlags {
            size,
            words: vec![0; (size as usize).div_ceil(64)],
        }
    }

    pub fn with_default(size: i32, default_value: bool) -> BitSetFlags {
        let mut flags = BitSetFlags::new(size);
        if default_value {
            flags.set_all(true);
        }
        flags
    }

    fn check_range(&self, index: i32) {
        assert!(index >= 0, "index is {index} which is less than zero");
        assert!(
            index < self.size,
            "index is {index} and set size is {}",
            self.size
        );
    }
}

impl Flags for BitSetFlags {
    fn size(&self) -> i32 {
        self.size
    }

    /// # Panics
    /// If `index` is out of `0..size()` (upstream `IndexOutOfBoundsException`).
    fn get(&self, index: i32) -> bool {
        self.check_range(index);
        let index = index as usize;
        self.words[index / 64] & (1 << (index % 64)) != 0
    }

    /// # Panics
    /// If `index` is out of `0..size()`.
    fn set(&mut self, index: i32, value: bool) {
        self.check_range(index);
        let index = index as usize;
        if value {
            self.words[index / 64] |= 1 << (index % 64);
        } else {
            self.words[index / 64] &= !(1 << (index % 64));
        }
    }

    fn set_all(&mut self, value: bool) {
        let fill = if value { u64::MAX } else { 0 };
        self.words.iter_mut().for_each(|word| *word = fill);
        let tail = self.size as usize % 64;
        if value && tail != 0 {
            if let Some(last) = self.words.last_mut() {
                *last &= (1 << tail) - 1;
            }
        }
    }
}

impl fmt::Debug for BitSetFlags {
    /// Like upstream `toString`: `{set indexes}, size = N`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let set: Vec<i32> = (0..self.size).filter(|&i| self.get(i)).collect();
        let set: Vec<String> = set.iter().map(i32::to_string).collect();
        write!(f, "{{{}}}, size = {}", set.join(", "), self.size)
    }
}
```

`crates/ig-log/src/utils/normal_edge.rs`:

```rust
//! A normal edge as a pair of node indexes.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/NormalEdge.java`.

/// Up and down node indexes of a normal edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NormalEdge {
    pub up: i32,
    pub down: i32,
}

impl NormalEdge {
    pub fn create(up: i32, down: i32) -> NormalEdge {
        NormalEdge { up, down }
    }
}
```

`crates/ig-log/src/utils/int_int_multi_map.rs`:

```rust
//! A multimap from `i32` to sets of `i32`, keeping values in insertion order.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntIntMultiMap.java`.

use std::collections::HashMap;

/// Each key maps to a small array of distinct values (duplicates are ignored on insert).
#[derive(Clone, Debug, Default)]
pub struct IntIntMultiMap {
    key_to_array_map: HashMap<i32, Vec<i32>>,
}

impl IntIntMultiMap {
    pub fn new() -> IntIntMultiMap {
        IntIntMultiMap::default()
    }

    pub fn put_value(&mut self, key: i32, value: i32) {
        let values = self.key_to_array_map.entry(key).or_default();
        if !values.contains(&value) {
            values.push(value);
        }
    }

    /// Removes `value` from `key`; drops the key when no values are left.
    pub fn remove(&mut self, key: i32, value: i32) {
        let Some(values) = self.key_to_array_map.get_mut(&key) else {
            return;
        };
        let Some(remove_index) = values.iter().position(|&v| v == value) else {
            return;
        };
        if values.len() == 1 {
            self.key_to_array_map.remove(&key);
            return;
        }
        values.remove(remove_index);
    }

    /// Values of `key` in insertion order (empty if none).
    pub fn get(&self, key: i32) -> &[i32] {
        self.get_as_array(key)
    }

    pub fn get_as_array(&self, key: i32) -> &[i32] {
        self.key_to_array_map
            .get(&key)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn is_empty(&self) -> bool {
        self.key_to_array_map.is_empty()
    }

    pub fn contains_key(&self, key: i32) -> bool {
        self.key_to_array_map.contains_key(&key)
    }

    /// Keys in unspecified order.
    pub fn keys(&self) -> Vec<i32> {
        self.key_to_array_map.keys().copied().collect()
    }

    pub fn clear(&mut self) {
        self.key_to_array_map.clear();
    }
}
```

`crates/ig-log/src/utils/java_sort.rs`:

```rust
//! A stable sort that tolerates inconsistent comparators, like Java's `Collections.sort`.
//!
//! IdeaGit helper, no direct upstream file. Upstream sorts with `java.util.TimSort` (and fastutil's
//! stable merge sort for `IntArrayList`). Some comparators of the graph (e.g.
//! `GraphElementComparatorByLayoutIndex`) are not total orders, and Rust's `slice::sort_by` may panic
//! on those. For fewer than 32 elements this is an exact port of TimSort's small-array path
//! (`countRunAndMakeAscending` + `binarySort`), so ties and inconsistencies resolve like upstream;
//! longer slices use a stable merge sort, which matches TimSort for every consistent comparator.

use std::cmp::Ordering;

/// `TimSort.MIN_MERGE`.
const MIN_MERGE: usize = 32;

/// Sorts `v` stably with `compare`; never panics because of the comparator.
pub fn sort_by<T: Clone>(v: &mut [T], mut compare: impl FnMut(&T, &T) -> Ordering) {
    sort_range(v, &mut compare);
}

fn sort_range<T: Clone>(v: &mut [T], compare: &mut impl FnMut(&T, &T) -> Ordering) {
    let n = v.len();
    if n < 2 {
        return;
    }
    if n < MIN_MERGE {
        let init_run_len = count_run_and_make_ascending(v, compare);
        binary_sort(v, init_run_len, compare);
        return;
    }
    let mid = n / 2;
    sort_range(&mut v[..mid], compare);
    sort_range(&mut v[mid..], compare);
    merge(v, mid, compare);
}

/// `TimSort.binarySort`: `v[..start]` is already sorted.
fn binary_sort<T: Clone>(
    v: &mut [T],
    mut start: usize,
    compare: &mut impl FnMut(&T, &T) -> Ordering,
) {
    if start == 0 {
        start = 1;
    }
    while start < v.len() {
        let pivot = v[start].clone();
        let mut left = 0;
        let mut right = start;
        while left < right {
            let mid = (left + right) / 2;
            if compare(&pivot, &v[mid]) == Ordering::Less {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        v[left..=start].rotate_right(1);
        start += 1;
    }
}

/// `TimSort.countRunAndMakeAscending`: length of the run at the start; a strictly descending run is
/// reversed in place.
fn count_run_and_make_ascending<T>(
    v: &mut [T],
    compare: &mut impl FnMut(&T, &T) -> Ordering,
) -> usize {
    let hi = v.len();
    let mut run_hi = 1;
    if run_hi == hi {
        return 1;
    }
    let descending = compare(&v[run_hi], &v[0]) == Ordering::Less;
    run_hi += 1;
    if descending {
        while run_hi < hi && compare(&v[run_hi], &v[run_hi - 1]) == Ordering::Less {
            run_hi += 1;
        }
        v[..run_hi].reverse();
    } else {
        while run_hi < hi && compare(&v[run_hi], &v[run_hi - 1]) != Ordering::Less {
            run_hi += 1;
        }
    }
    run_hi
}

/// Stable merge of the sorted halves `v[..mid]` and `v[mid..]`.
fn merge<T: Clone>(v: &mut [T], mid: usize, compare: &mut impl FnMut(&T, &T) -> Ordering) {
    let left: Vec<T> = v[..mid].to_vec();
    let right: Vec<T> = v[mid..].to_vec();
    let (mut i, mut j) = (0, 0);
    for slot in v.iter_mut() {
        let take_right =
            i >= left.len() || (j < right.len() && compare(&right[j], &left[i]) == Ordering::Less);
        if take_right {
            *slot = right[j].clone();
            j += 1;
        } else {
            *slot = left[i].clone();
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_stably_small_and_large() {
        for n in [0usize, 1, 2, 5, 31, 32, 33, 100, 1000] {
            let mut v: Vec<(i32, usize)> = (0..n).map(|i| (((i * 7919) % 13) as i32, i)).collect();
            let mut expected = v.clone();
            expected.sort_by_key(|&(k, _)| k);
            sort_by(&mut v, |a, b| a.0.cmp(&b.0));
            assert_eq!(v, expected, "n = {n}");
        }
    }

    #[test]
    fn descending_run_is_reversed() {
        let mut v = vec![5, 4, 3, 2, 1];
        sort_by(&mut v, |a: &i32, b| a.cmp(b));
        assert_eq!(v, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn inconsistent_comparator_does_not_panic() {
        let mut v: Vec<i32> = (0..200).collect();
        sort_by(&mut v, |a, b| {
            if (a + b) % 3 == 0 {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        });
        assert_eq!(v.len(), 200);
    }
}
```

`crates/ig-log/src/utils/bounded_cache.rs`:

```rust
//! A small bounded cache replacing IntelliJ's `SLRUMap` in the print generators.
//!
//! IdeaGit helper standing in for `com.intellij.util.containers.SLRUMap` as used by
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/EdgesInRowGenerator.java` and
//! `PrintElementGeneratorImpl.kt`. Deviation: eviction is first-in-first-out instead of segmented
//! LRU. The caches only memoise pure computations, so results are unaffected.

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

#[derive(Clone, Debug)]
pub struct BoundedCache<K, V> {
    capacity: usize,
    map: HashMap<K, V>,
    order: VecDeque<K>,
}

impl<K: Eq + Hash + Clone, V> BoundedCache<K, V> {
    /// `SLRUMap(protectedQueueSize, probationalQueueSize)` holds up to the sum of both.
    pub fn new(protected_size: usize, probational_size: usize) -> BoundedCache<K, V> {
        BoundedCache {
            capacity: (protected_size + probational_size).max(1),
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.map.get(key)
    }

    pub fn put(&mut self, key: K, value: V) {
        if self.map.insert(key.clone(), value).is_none() {
            self.order.push_back(key);
            while self.order.len() > self.capacity {
                if let Some(oldest) = self.order.pop_front() {
                    self.map.remove(&oldest);
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.order.clear();
    }
}
```

`crates/ig-log/src/utils/linear_graph_utils.rs`:

```rust
//! Helpers over [`LinearGraph`] and [`GraphEdge`].
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/LinearGraphUtils.java`.

use std::collections::HashSet;
use std::ops::Deref;

use crate::actions::Cursor;
use crate::api::EdgeFilter;
use crate::api::elements::GraphEdge;
use crate::api::linear_graph::{LinearGraph, LiteLinearGraph, NodeFilter};
use crate::facade::linear_graph_controller::LinearGraphAnswer;
use crate::utils::normal_edge::NormalEdge;

/// `DEFAULT_GRAPH_ANSWER`: default cursor, no selection.
pub fn default_graph_answer() -> LinearGraphAnswer {
    LinearGraphAnswer::with_cursor(Some(Cursor::Default), None)
}

pub fn int_equal(value: Option<i32>, number: i32) -> bool {
    value == Some(number)
}

/// The edge goes up from `node_index` (the node is its down end).
pub fn is_edge_up(edge: &GraphEdge, node_index: i32) -> bool {
    int_equal(edge.down_node_index, node_index)
}

/// The edge goes down from `node_index` (the node is its up end).
pub fn is_edge_down(edge: &GraphEdge, node_index: i32) -> bool {
    int_equal(edge.up_node_index, node_index)
}

pub fn is_normal_edge(edge: Option<&GraphEdge>) -> bool {
    match edge {
        Some(edge) if edge.edge_type.is_normal_edge() => {
            debug_assert!(edge.up_node_index.is_some() && edge.down_node_index.is_some());
            true
        }
        _ => false,
    }
}

pub fn as_normal_edge(edge: &GraphEdge) -> Option<NormalEdge> {
    if is_normal_edge(Some(edge)) {
        Some(NormalEdge::create(
            edge.up_node_index?,
            edge.down_node_index?,
        ))
    } else {
        None
    }
}

/// # Panics
/// If the edge has neither node index.
pub fn get_not_null_node_index(edge: &GraphEdge) -> i32 {
    edge.up_node_index
        .or(edge.down_node_index)
        .expect("edge without node indexes")
}

pub fn get_up_nodes(graph: &(impl LinearGraph + ?Sized), node_index: i32) -> Vec<i32> {
    graph
        .get_adjacent_edges(node_index, EdgeFilter::NormalUp)
        .iter()
        .filter_map(|edge| edge.up_node_index)
        .collect()
}

pub fn get_down_nodes(graph: &(impl LinearGraph + ?Sized), node_index: i32) -> Vec<i32> {
    graph
        .get_adjacent_edges(node_index, EdgeFilter::NormalDown)
        .iter()
        .filter_map(|edge| edge.down_node_index)
        .collect()
}

/// [`LiteLinearGraph`] view of a [`LinearGraph`] (`asLiteLinearGraph`). `G` is any pointer to a
/// graph: `&G`, `Arc<G>`, `&dyn LinearGraph`, ...
#[derive(Clone, Debug)]
pub struct LiteLinearGraphAdapter<G>(pub G);

pub fn as_lite_linear_graph<G>(graph: G) -> LiteLinearGraphAdapter<G>
where
    G: Deref,
    G::Target: LinearGraph,
{
    LiteLinearGraphAdapter(graph)
}

impl<G> LiteLinearGraph for LiteLinearGraphAdapter<G>
where
    G: Deref,
    G::Target: LinearGraph,
{
    fn nodes_count(&self) -> i32 {
        self.0.nodes_count()
    }

    fn get_nodes(&self, node_index: i32, filter: NodeFilter) -> Vec<i32> {
        self.0
            .get_adjacent_edges(node_index, filter.edge_filter())
            .iter()
            .filter_map(|edge| {
                if is_edge_up(edge, node_index) {
                    return edge.up_node_index;
                }
                if is_edge_down(edge, node_index) {
                    return edge.down_node_index;
                }
                None
            })
            .collect()
    }
}

pub fn get_cursor(hand: bool) -> Cursor {
    if hand { Cursor::Hand } else { Cursor::Default }
}

/// Hand cursor and the node ids of `selected_node_indexes` as selection.
pub fn create_selected_answer(
    linear_graph: &(impl LinearGraph + ?Sized),
    selected_node_indexes: impl IntoIterator<Item = i32>,
) -> LinearGraphAnswer {
    let selected_ids: HashSet<i32> = selected_node_indexes
        .into_iter()
        .map(|node_index| linear_graph.get_node_id(node_index))
        .collect();
    LinearGraphAnswer::with_cursor(Some(get_cursor(true)), Some(selected_ids))
}

pub fn get_edge(graph: &(impl LinearGraph + ?Sized), up: i32, down: i32) -> Option<GraphEdge> {
    graph
        .get_adjacent_edges(up, EdgeFilter::NormalDown)
        .into_iter()
        .find(|edge| int_equal(edge.down_node_index, down))
}

pub fn convert_node_indexes_to_ids(
    graph: &(impl LinearGraph + ?Sized),
    node_indexes: impl IntoIterator<Item = i32>,
) -> HashSet<i32> {
    node_indexes
        .into_iter()
        .map(|node_index| graph.get_node_id(node_index))
        .collect()
}

pub fn convert_ids_to_node_indexes(
    graph: &(impl LinearGraph + ?Sized),
    ids: impl IntoIterator<Item = i32>,
) -> HashSet<i32> {
    ids.into_iter()
        .filter_map(|id| graph.get_node_index(id))
        .collect()
}
```

`crates/ig-log/src/utils/dfs_util.rs`:

```rust
//! Depth-first walks.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/DfsUtil.kt`.
//!
//! Upstream quirk kept: when the consumer of [`DfsWalk::walk`] returns `false`, only the walk from
//! the current start node stops; the remaining start nodes are still walked, and the stack keeps the
//! nodes left over from the interrupted walk.

use crate::api::linear_graph::{LiteLinearGraph, NodeFilter};
use crate::utils::flags::Flags;

/// `Dfs.NextNode.NODE_NOT_FOUND`: no next node, go back up the stack.
pub const NODE_NOT_FOUND: i32 = -1;
/// `Dfs.NextNode.EXIT`: stop the walk.
pub const EXIT: i32 = -10;

fn walk_with_stack(start: i32, stack: &mut Vec<i32>, mut next_node_fun: impl FnMut(i32) -> i32) {
    stack.push(start);

    while let Some(&top) = stack.last() {
        let next_node = next_node_fun(top);
        if next_node == EXIT {
            return;
        }
        if next_node != NODE_NOT_FOUND {
            stack.push(next_node);
        } else {
            stack.pop();
        }
    }
    stack.clear();
}

/// Walks from `start`: `next_node_fun(top)` returns the node to push, [`NODE_NOT_FOUND`] to pop, or
/// [`EXIT`] to stop.
pub fn walk(start: i32, next_node_fun: impl FnMut(i32) -> i32) {
    walk_with_stack(start, &mut Vec::new(), next_node_fun);
}

/// Depth-first walk from several start nodes, marking visited nodes in `visited`.
pub struct DfsWalk<'a, G: LiteLinearGraph + ?Sized> {
    start_nodes: Vec<i32>,
    graph: &'a G,
    visited: &'a mut dyn Flags,
    stack: Vec<i32>,
}

impl<'a, G: LiteLinearGraph + ?Sized> DfsWalk<'a, G> {
    pub fn new(
        start_nodes: impl IntoIterator<Item = i32>,
        graph: &'a G,
        visited: &'a mut dyn Flags,
    ) -> DfsWalk<'a, G> {
        DfsWalk {
            start_nodes: start_nodes.into_iter().collect(),
            graph,
            visited,
            stack: Vec::new(),
        }
    }

    /// Calls `consumer` for every reachable node (negative start nodes are skipped); `consumer`
    /// returns `false` to stop (see the module docs for the quirk).
    pub fn walk(&mut self, go_down: bool, mut consumer: impl FnMut(i32) -> bool) {
        let filter = if go_down {
            NodeFilter::Down
        } else {
            NodeFilter::Up
        };
        let graph = self.graph;
        for &start in &self.start_nodes {
            if start < 0 {
                continue;
            }
            if self.visited.get(start) {
                continue;
            }
            self.visited.set(start, true);
            if !consumer(start) {
                return;
            }

            let visited = &mut *self.visited;
            walk_with_stack(start, &mut self.stack, |current_node| {
                for down_node in graph.get_nodes(current_node, filter) {
                    if !visited.get(down_node) {
                        visited.set(down_node, true);
                        if !consumer(down_node) {
                            return EXIT;
                        }
                        return down_node;
                    }
                }
                NODE_NOT_FOUND
            });
        }
    }
}
```

- [ ] **Step 6: Permanent graph (`permanent`).** Create these files with exactly this content.

`crates/ig-log/src/permanent/mod.rs`:

```rust
//! Building the permanent graph: the linear graph of all commits, its layout and commit mapping.
//!
//! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/`; see each submodule.

pub mod duplicate_parent_fixer;
pub mod graph_layout_builder;
pub mod graph_layout_impl;
pub mod permanent_commits_info_impl;
pub mod permanent_linear_graph_builder;
pub mod permanent_linear_graph_impl;
```

`crates/ig-log/src/permanent/duplicate_parent_fixer.rs`:

```rust
//! Removes duplicate parents from commits before the graph is built.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/DuplicateParentFixer.java`.
//!
//! Deviation: upstream wraps the list lazily and re-fixes a commit on every access; here each commit
//! is fixed once, borrowing the original parents when nothing changes.

use std::borrow::Cow;
use std::collections::HashSet;
use std::hash::Hash;

use crate::graph_commit::GraphCommit;

/// A commit with duplicate parents removed (`DelegateGraphCommit`, or the original commit).
#[derive(Clone, Debug)]
pub struct FixedGraphCommit<'a, CommitId: Clone> {
    id: &'a CommitId,
    parents: Cow<'a, [CommitId]>,
    timestamp: i64,
}

impl<'a, CommitId: Clone> FixedGraphCommit<'a, CommitId> {
    /// The id, borrowed from the original commit list.
    pub fn commit_id(&self) -> &'a CommitId {
        self.id
    }

    /// The fixed parents (cheap unless duplicates were removed).
    pub fn parents_cow(&self) -> Cow<'a, [CommitId]> {
        self.parents.clone()
    }
}

impl<CommitId: Clone> GraphCommit<CommitId> for FixedGraphCommit<'_, CommitId> {
    fn id(&self) -> &CommitId {
        self.id
    }

    fn parents(&self) -> &[CommitId] {
        &self.parents
    }

    fn timestamp(&self) -> i64 {
        self.timestamp
    }
}

/// `fixDuplicateParentCommits`.
pub fn fix_duplicate_parent_commits<CommitId, G>(
    commits: &[G],
) -> Vec<FixedGraphCommit<'_, CommitId>>
where
    CommitId: Eq + Hash + Clone,
    G: GraphCommit<CommitId>,
{
    commits
        .iter()
        .map(|commit| FixedGraphCommit {
            id: commit.id(),
            parents: fix_parents_duplicate(commit.parents()),
            timestamp: commit.timestamp(),
        })
        .collect()
}

/// Keeps the first occurrence of every parent, in order.
pub fn fix_parents_duplicate<CommitId: Eq + Hash + Clone>(
    parents: &[CommitId],
) -> Cow<'_, [CommitId]> {
    if parents.len() <= 1 {
        return Cow::Borrowed(parents);
    }

    if parents.len() == 2 {
        return if parents[0] != parents[1] {
            Cow::Borrowed(parents)
        } else {
            Cow::Owned(vec![parents[0].clone()])
        };
    }

    let mut all_parents: HashSet<&CommitId> = parents.iter().collect();
    if parents.len() == all_parents.len() {
        return Cow::Borrowed(parents);
    }

    let correct_parents = parents
        .iter()
        .filter(|commit_id| all_parents.remove(commit_id))
        .cloned()
        .collect();
    Cow::Owned(correct_parents)
}
```

`crates/ig-log/src/permanent/permanent_linear_graph_builder.rs`:

```rust
//! Builds the [`PermanentLinearGraphImpl`] from a topologically sorted commit list.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentLinearGraphBuilder.java`.
//!
//! Deviations:
//! - Upstream bug fixed: for an edge to a commit that is not loaded, upstream fills the *first*
//!   unset edge slot of the child node, regardless of which parent the slot belongs to. When a
//!   commit has several unloaded parents and they are processed out of parent order, the edges (and
//!   therefore `PermanentGraph.allCommits` parents) get the wrong targets. Here the slot is found
//!   by parent position, like for loaded parents.
//! - Unloaded parents with the same smallest child node are processed in the order they were first
//!   seen (upstream: `HashMap` iteration order), so their synthetic ids are deterministic.

use std::collections::HashMap;
use std::hash::Hash;

use crate::graph_commit::GraphCommit;
use crate::permanent::duplicate_parent_fixer::{FixedGraphCommit, fix_duplicate_parent_commits};
use crate::permanent::permanent_linear_graph_impl::PermanentLinearGraphImpl;
use crate::utils::flags::{BitSetFlags, Flags};

/// Child nodes waiting for a parent commit, plus the order in which the parent was first seen.
struct UpNodes {
    first_seen: usize,
    up_nodes: Vec<i32>,
}

pub struct PermanentLinearGraphBuilder<'a, CommitId: Clone> {
    commits: Vec<FixedGraphCommit<'a, CommitId>>,
    simple_nodes: BitSetFlags,
    nodes_count: i32,
    node_to_edge_index: Vec<i32>,
    long_edges: Vec<i32>,
    /// downCommitId -> up node indexes
    up_adjacent_nodes: HashMap<CommitId, UpNodes>,
    seen_counter: usize,
}

impl<'a, CommitId: Eq + Hash + Clone> PermanentLinearGraphBuilder<'a, CommitId> {
    /// `newInstance`: fixes duplicate parents and finds the "simple" nodes (whose only parent is the
    /// next commit).
    pub fn new_instance<G: GraphCommit<CommitId>>(
        graph_commits: &'a [G],
    ) -> PermanentLinearGraphBuilder<'a, CommitId> {
        let graph_commits = fix_duplicate_parent_commits(graph_commits);
        let mut simple_nodes = BitSetFlags::new(graph_commits.len() as i32);

        let mut long_edges_count = 0;
        for (node_index, commit) in graph_commits.iter().enumerate() {
            let next_commit_hash_index = graph_commits.get(node_index + 1).map(|next| next.id());

            let parents = commit.parents();
            if parents.len() == 1 && Some(&parents[0]) == next_commit_hash_index {
                simple_nodes.set(node_index as i32, true);
            } else {
                long_edges_count += parents.len();
            }
        }

        let nodes_count = simple_nodes.size();
        PermanentLinearGraphBuilder {
            commits: graph_commits,
            simple_nodes,
            nodes_count,
            node_to_edge_index: vec![0; nodes_count as usize + 1],
            long_edges: vec![0; 2 * long_edges_count],
            up_adjacent_nodes: HashMap::new(),
            seen_counter: 0,
        }
    }

    fn add_underdone_edge(&mut self, up_node_index: i32, down_commit_id: &CommitId) {
        if let Some(entry) = self.up_adjacent_nodes.get_mut(down_commit_id) {
            entry.up_nodes.push(up_node_index);
            return;
        }
        self.seen_counter += 1;
        self.up_adjacent_nodes.insert(
            down_commit_id.clone(),
            UpNodes {
                first_seen: self.seen_counter,
                up_nodes: vec![up_node_index],
            },
        );
    }

    /// Index in `long_edges` of the down edge from `up_node_index` to its parent `down_commit_id`.
    fn find_underdone_edge(&self, up_node_index: i32, down_commit_id: &CommitId) -> Option<usize> {
        let end = self.node_to_edge_index[up_node_index as usize + 1] as usize;
        let parent_hash_indices = self.commits[up_node_index as usize].parents();
        parent_hash_indices
            .iter()
            .position(|parent| parent == down_commit_id)
            .map(|i| end - (parent_hash_indices.len() - i))
    }

    fn fix_underdone_edge(
        &mut self,
        up_node_index: i32,
        down_node_index: i32,
        down_commit_id: &CommitId,
    ) {
        let Some(edge_index) = self.find_underdone_edge(up_node_index, down_commit_id) else {
            panic!(
                "Not found underdone edges for node: {up_node_index}. Adjacent down node: {down_node_index}"
            );
        };
        if self.long_edges[edge_index] == -1 {
            self.long_edges[edge_index] = down_node_index;
        } else {
            panic!("Edge was set early!. Up node: {up_node_index}, down node: {down_node_index}");
        }
    }

    fn do_step(&mut self, node_index: i32) {
        let commit_id = self.commits[node_index as usize].commit_id();

        let up_nodes = self
            .up_adjacent_nodes
            .remove(commit_id)
            .map(|entry| entry.up_nodes)
            .unwrap_or_default();

        let mut edge_index = self.node_to_edge_index[node_index as usize] as usize;
        for up_node_index in up_nodes {
            self.fix_underdone_edge(up_node_index, node_index, commit_id);
            self.long_edges[edge_index] = up_node_index;
            edge_index += 1;
        }

        // down nodes
        if !self.simple_nodes.get(node_index) {
            let parents = self.commits[node_index as usize].parents_cow();
            for down_commit_id in parents.iter() {
                self.add_underdone_edge(node_index, down_commit_id);
                self.long_edges[edge_index] = -1;
                edge_index += 1;
            }
        }

        self.node_to_edge_index[node_index as usize + 1] = edge_index as i32;
    }

    /// `fixUnderdoneEdgeForNotLoadCommit`, with the slot chosen by parent position (see module docs).
    fn fix_underdone_edge_for_not_load_commit(
        &mut self,
        up_node_index: i32,
        not_load_id: i32,
        not_load_commit: &CommitId,
    ) {
        match self.find_underdone_edge(up_node_index, not_load_commit) {
            Some(edge_index) if self.long_edges[edge_index] == -1 => {
                self.long_edges[edge_index] = not_load_id;
            }
            _ => panic!("Not found underdone edge to not load commit for node: {up_node_index}"),
        }
    }

    fn fix_underdone_edges(&mut self, mut not_loaded_commit_to_id: impl FnMut(&CommitId) -> i32) {
        let up_adjacent_nodes = std::mem::take(&mut self.up_adjacent_nodes);
        let mut commit_ids: Vec<(&CommitId, &UpNodes)> = up_adjacent_nodes.iter().collect();
        commit_ids.sort_by_key(|(_, entry)| {
            (
                entry.up_nodes.iter().min().copied().unwrap_or(i32::MAX),
                entry.first_seen,
            )
        });
        for (not_load_commit, entry) in commit_ids {
            let not_load_id = not_loaded_commit_to_id(not_load_commit);
            for &up_node_index in &entry.up_nodes {
                self.fix_underdone_edge_for_not_load_commit(
                    up_node_index,
                    not_load_id,
                    not_load_commit,
                );
            }
        }
    }

    /// Builds the graph; `not_loaded_commit_to_id` assigns ids (`<= -2`) to parents that are not in
    /// the commit list, in order of their first child node.
    pub fn build_with(
        mut self,
        not_loaded_commit_to_id: impl FnMut(&CommitId) -> i32,
    ) -> PermanentLinearGraphImpl {
        for node_index in 0..self.nodes_count {
            self.do_step(node_index);
        }

        self.fix_underdone_edges(not_loaded_commit_to_id);

        PermanentLinearGraphImpl::new(self.simple_nodes, self.node_to_edge_index, self.long_edges)
    }

    /// `build()`: every unloaded parent gets id `i32::MIN`.
    pub fn build(self) -> PermanentLinearGraphImpl {
        self.build_with(|_| i32::MIN)
    }
}
```

`crates/ig-log/src/permanent/permanent_linear_graph_impl.rs`:

```rust
//! The compact linear graph of all loaded commits.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentLinearGraphImpl.java`.
//!
//! A "simple" node has exactly one parent, the next row; its down edge is implicit. All other
//! edges are stored in `long_edges`, indexed per node by `node_to_edge_index`: the up edges first,
//! then the down edges in parent order. Negative entries are ids of commits that are not loaded.

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphEdgeType, GraphNode};
use crate::api::linear_graph::LinearGraph;
use crate::utils::flags::{BitSetFlags, Flags};

#[derive(Clone, Debug)]
pub struct PermanentLinearGraphImpl {
    simple_nodes: BitSetFlags,
    node_to_edge_index: Vec<i32>,
    long_edges: Vec<i32>,
}

impl PermanentLinearGraphImpl {
    pub(crate) fn new(
        simple_nodes: BitSetFlags,
        node_to_edge_index: Vec<i32>,
        long_edges: Vec<i32>,
    ) -> PermanentLinearGraphImpl {
        PermanentLinearGraphImpl {
            simple_nodes,
            node_to_edge_index,
            long_edges,
        }
    }

    fn edge_range(&self, node_index: i32) -> std::ops::Range<usize> {
        self.node_to_edge_index[node_index as usize] as usize
            ..self.node_to_edge_index[node_index as usize + 1] as usize
    }

    /// Calls `handler` for every node that can be considered a parent of `node_index`, including
    /// the (negative) ids of [`GraphEdgeType::NotLoadCommit`] targets, in parent order.
    pub fn for_each_parent_node(&self, node_index: i32, mut handler: impl FnMut(i32)) {
        for i in self.edge_range(node_index) {
            let adjacent_node = self.long_edges[i];
            if adjacent_node < 0 || node_index < adjacent_node {
                handler(adjacent_node);
            }
        }
        if self.simple_nodes.get(node_index) {
            handler(node_index + 1);
        }
    }
}

impl LinearGraph for PermanentLinearGraphImpl {
    fn nodes_count(&self) -> i32 {
        self.simple_nodes.size()
    }

    /// # Panics
    /// If `node_index` is not a node of the graph.
    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        let mut result = Vec::new();

        let has_up_simple_edge = node_index != 0 && self.simple_nodes.get(node_index - 1);
        if has_up_simple_edge && filter.up_normal() {
            result.push(GraphEdge::new(
                Some(node_index - 1),
                Some(node_index),
                None,
                GraphEdgeType::Usual,
            ));
        }

        for i in self.edge_range(node_index) {
            let adjacent_node = self.long_edges[i];

            if adjacent_node < 0 && filter.special() {
                result.push(GraphEdge::create_edge_with_target_id(
                    node_index,
                    Some(adjacent_node),
                    GraphEdgeType::NotLoadCommit,
                ));
            }
            if adjacent_node < 0 {
                continue;
            }

            if node_index > adjacent_node && filter.up_normal() {
                result.push(GraphEdge::new(
                    Some(adjacent_node),
                    Some(node_index),
                    None,
                    GraphEdgeType::Usual,
                ));
            }
            if node_index < adjacent_node && filter.down_normal() {
                result.push(GraphEdge::new(
                    Some(node_index),
                    Some(adjacent_node),
                    None,
                    GraphEdgeType::Usual,
                ));
            }
        }

        if self.simple_nodes.get(node_index) && filter.down_normal() {
            result.push(GraphEdge::new(
                Some(node_index),
                Some(node_index + 1),
                None,
                GraphEdgeType::Usual,
            ));
        }

        result
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        GraphNode::new(node_index)
    }

    fn get_node_id(&self, node_index: i32) -> i32 {
        debug_assert!(
            node_index >= 0 && node_index < self.nodes_count(),
            "Bad nodeIndex: {node_index}"
        );
        node_index
    }

    fn get_node_index(&self, node_id: i32) -> Option<i32> {
        if node_id >= 0 && node_id < self.nodes_count() {
            Some(node_id)
        } else {
            None
        }
    }
}
```

`crates/ig-log/src/permanent/permanent_commits_info_impl.rs`:

```rust
//! Node id to commit id (and timestamp) mapping of the permanent graph.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/PermanentCommitsInfoImpl.kt`
//! and `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/RowsMapping.kt`.
//!
//! Deviation: no `Int`-specialised storage (`isIntegerCase`); `Vec<CommitId>` is already compact.
//! `get_node_id` stays a linear search, like upstream ("todo optimize with special map").

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
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> PermanentCommitsInfoImpl<CommitId> {
    /// `newInstance`: rows in the order of `graph_commits`; `not_loaded_commits` maps synthetic
    /// negative ids to the commits they stand for.
    pub fn new_instance<G: GraphCommit<CommitId>>(
        graph_commits: &[G],
        not_loaded_commits: HashMap<i32, CommitId>,
    ) -> PermanentCommitsInfoImpl<CommitId> {
        let mut rows_mapping = RowsMapping::new(graph_commits.len());
        for commit in graph_commits {
            rows_mapping.add(commit.id().clone(), commit.timestamp());
        }
        PermanentCommitsInfoImpl {
            rows_mapping,
            not_loaded_commits,
        }
    }

    pub fn timestamp_getter(&self) -> &dyn TimestampGetter {
        &self.rows_mapping
    }

    fn get_not_load_node_id(&self, commit_id: &CommitId) -> i32 {
        self.not_loaded_commits
            .iter()
            .find(|(_, value)| *value == commit_id)
            .map(|(&key, _)| key)
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
        let mut result: HashSet<i32> = self
            .rows_mapping
            .commit_id_mapping()
            .iter()
            .enumerate()
            .filter(|(_, commit_id)| commit_ids.contains(commit_id))
            .map(|(i, _)| i as i32)
            .collect();
        if !skip_not_loaded_commits {
            for (&node_id, commit_id) in &self.not_loaded_commits {
                if commit_ids.contains(commit_id) {
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
        let mapping = self.rows_mapping.commit_id_mapping();
        commit_ids.into_iter().all(|id| mapping.contains(id))
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
        if let Some(index) = self
            .rows_mapping
            .commit_id_mapping()
            .iter()
            .position(|id| id == commit_id)
        {
            return index as i32;
        }
        self.get_not_load_node_id(commit_id)
    }

    fn convert_to_node_ids(&self, commit_ids: &HashSet<CommitId>) -> HashSet<VcsLogGraphNodeId> {
        self.convert_to_node_ids_skipping(commit_ids, false)
    }
}
```

`crates/ig-log/src/permanent/graph_layout_builder.rs`:

```rust
//! Assigns layout indexes: heads are sorted by importance, then a depth-first walk from every head
//! numbers the fragments it reaches first.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilder.kt`.
//!
//! Deviations: `branches` is an ordered slice (upstream: a `Set`, iterated in hash order before the
//! stable sort, so it only matters for heads the comparator considers equal). The comparator cannot
//! throw, so upstream's `sortCatching` is a plain (Java-compatible) stable sort.

use std::cmp::Ordering;
use std::collections::HashSet;

use crate::api::linear_graph::LinearGraph;
use crate::permanent::graph_layout_impl::GraphLayoutImpl;
use crate::utils::dfs_util::{NODE_NOT_FOUND, walk};
use crate::utils::java_sort;
use crate::utils::linear_graph_utils::{get_down_nodes, get_up_nodes};

/// `build(graph, comparator)`: no extra branch heads.
pub fn build(
    graph: &(impl LinearGraph + ?Sized),
    comparator: impl FnMut(i32, i32) -> Ordering,
) -> GraphLayoutImpl {
    build_with_branches(graph, &[], comparator)
}

/// `build(graph, branches, comparator)`: `branches` (node indexes of branch heads) come first,
/// followed by the graph's own heads; all are sorted with `comparator` (most important first).
pub fn build_with_branches(
    graph: &(impl LinearGraph + ?Sized),
    branches: &[i32],
    mut comparator: impl FnMut(i32, i32) -> Ordering,
) -> GraphLayoutImpl {
    let mut seen = HashSet::new();
    let mut sorted_heads: Vec<i32> = branches
        .iter()
        .copied()
        .chain(get_heads(graph))
        .filter(|head| seen.insert(*head))
        .collect();
    java_sort::sort_by(&mut sorted_heads, |a, b| comparator(*a, *b));

    build_sorted(graph, &sorted_heads)
}

/// Nodes without up (child) edges.
pub(crate) fn get_heads(graph: &(impl LinearGraph + ?Sized)) -> Vec<i32> {
    (0..graph.nodes_count())
        .filter(|&i| get_up_nodes(graph, i).is_empty())
        .collect()
}

fn build_sorted(graph: &(impl LinearGraph + ?Sized), sorted_heads: &[i32]) -> GraphLayoutImpl {
    let mut layout_index = vec![0; graph.nodes_count() as usize];
    let mut important_heads = Vec::new();

    let mut current_layout_index = 1;
    for &head in sorted_heads {
        if layout_index[head as usize] != 0 {
            continue;
        }

        important_heads.push(head);

        walk(head, |current_node| {
            let first_visit = layout_index[current_node as usize] == 0;
            if first_visit {
                layout_index[current_node as usize] = current_layout_index;
            }

            let child_without_layout_index = get_down_nodes(graph, current_node)
                .into_iter()
                .find(|&it| layout_index[it as usize] == 0);
            match child_without_layout_index {
                None => {
                    if first_visit {
                        current_layout_index += 1;
                    }
                    NODE_NOT_FOUND
                }
                Some(child) => child,
            }
        });
    }

    GraphLayoutImpl::new(layout_index, important_heads)
}
```

`crates/ig-log/src/permanent/graph_layout_impl.rs`:

```rust
//! The permanent graph layout: a layout index per node and the heads that start each fragment.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutImpl.kt`.

use crate::api::GraphLayout;

#[derive(Clone, Debug)]
pub struct GraphLayoutImpl {
    layout_index: Vec<i32>,
    head_nodes: Vec<i32>,
    layout_index_for_head_nodes: Vec<i32>,
}

impl GraphLayoutImpl {
    /// `layout_index` per node; `head_nodes` in increasing layout index order.
    pub fn new(layout_index: Vec<i32>, head_nodes: Vec<i32>) -> GraphLayoutImpl {
        let layout_index_for_head_nodes = head_nodes
            .iter()
            .map(|&head| layout_index[head as usize])
            .collect();
        GraphLayoutImpl {
            layout_index,
            head_nodes,
            layout_index_for_head_nodes,
        }
    }

    fn get_head_node_index_for_layout(&self, layout_index: i32) -> i32 {
        self.head_nodes[self.get_head_order(layout_index)]
    }

    /// Position of the last head whose layout index is `<= layout_index` (0 if none), like
    /// upstream's `Arrays.binarySearch` arithmetic.
    fn get_head_order(&self, layout_index: i32) -> usize {
        match self
            .layout_index_for_head_nodes
            .binary_search(&layout_index)
        {
            Ok(i) => i,
            Err(insertion_point) => insertion_point.saturating_sub(1),
        }
    }
}

impl GraphLayout for GraphLayoutImpl {
    fn get_layout_index(&self, node_index: i32) -> i32 {
        self.layout_index[node_index as usize]
    }

    fn get_one_of_head_node_index(&self, node_index: i32) -> i32 {
        self.get_head_node_index_for_layout(self.get_layout_index(node_index))
    }

    fn get_head_node_index(&self) -> &[i32] {
        &self.head_nodes
    }
}
```

- [ ] **Step 7: Printing (`print`).** Create these files with exactly this content.

`crates/ig-log/src/print/mod.rs`:

```rust
//! Printing the visible graph: edges per row, element ordering, print elements and colors.
//!
//! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/`; see each submodule.

pub mod edges_in_row_generator;
pub mod elements;
pub mod graph_color_getter;
pub mod graph_element_comparator_by_layout_index;
pub mod print_element_generator_impl;
```

`crates/ig-log/src/print/elements.rs`:

```rust
//! Concrete print elements produced by [`PrintElementGeneratorImpl`](super::print_element_generator_impl::PrintElementGeneratorImpl).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/elements/`
//! (`PrintElementBase.kt`, `SimplePrintElementImpl.kt`, `EdgePrintElementImpl.kt`,
//! `TerminalEdgePrintElement.java`).
//!
//! Deviation: upstream elements ask the presentation manager for `colorId` / `isSelected` on every
//! access; here both are computed once, when the generator creates the element. Elements are
//! created afresh on every `get_print_elements` call, so they reflect the selection at that time.

use crate::api::elements::{GraphEdge, GraphEdgeType, GraphElement, GraphNode};
use crate::api::printer::PrintElementPresentationManager;
use crate::print_element::{
    EdgePrintElement, EdgePrintElementType, LineStyle, NodePrintElement, PrintElement,
};
use crate::visible_graph::VcsLogVisibleGraphIndex;

/// A node print element (`SimplePrintElementImpl`).
#[derive(Clone, Debug)]
pub struct SimplePrintElementImpl {
    row_index: VcsLogVisibleGraphIndex,
    position_in_current_row: i32,
    graph_node: GraphNode,
    color_id: i32,
    is_selected: bool,
}

impl SimplePrintElementImpl {
    /// Creates the element; color and selection come from `presentation_manager`.
    pub fn new(
        row_index: VcsLogVisibleGraphIndex,
        position_in_current_row: i32,
        graph_node: GraphNode,
        presentation_manager: &dyn PrintElementPresentationManager,
    ) -> SimplePrintElementImpl {
        let mut element = SimplePrintElementImpl {
            row_index,
            position_in_current_row,
            graph_node,
            color_id: presentation_manager.get_color_id(&GraphElement::Node(graph_node)),
            is_selected: false,
        };
        element.is_selected = presentation_manager.is_selected(
            &crate::api::printer::GraphPrintElement::Node(element.clone()),
        );
        element
    }

    pub fn graph_node(&self) -> GraphNode {
        self.graph_node
    }
}

impl PrintElement for SimplePrintElementImpl {
    fn row_index(&self) -> VcsLogVisibleGraphIndex {
        self.row_index
    }
    fn position_in_current_row(&self) -> i32 {
        self.position_in_current_row
    }
    fn color_id(&self) -> i32 {
        self.color_id
    }
    fn is_selected(&self) -> bool {
        self.is_selected
    }
}

impl NodePrintElement for SimplePrintElementImpl {}

/// Upstream equality: same row and position (graph element, color and selection are ignored).
impl PartialEq for SimplePrintElementImpl {
    fn eq(&self, other: &Self) -> bool {
        self.position_in_current_row == other.position_in_current_row
            && self.row_index == other.row_index
    }
}

/// An edge print element (`EdgePrintElementImpl`); a `TerminalEdgePrintElement` is one created by
/// [`EdgePrintElementImpl::terminal`].
#[derive(Clone, Debug)]
pub struct EdgePrintElementImpl {
    row_index: VcsLogVisibleGraphIndex,
    position_in_current_row: i32,
    position_in_other_row: i32,
    edge_print_type: EdgePrintElementType,
    graph_edge: GraphEdge,
    has_arrow: bool,
    line_style: LineStyle,
    color_id: i32,
    is_selected: bool,
}

impl EdgePrintElementImpl {
    /// Creates the element; color and selection come from `presentation_manager`.
    pub fn new(
        row_index: VcsLogVisibleGraphIndex,
        position_in_current_row: i32,
        position_in_other_row: i32,
        edge_print_type: EdgePrintElementType,
        graph_edge: GraphEdge,
        has_arrow: bool,
        presentation_manager: &dyn PrintElementPresentationManager,
    ) -> EdgePrintElementImpl {
        let mut element = EdgePrintElementImpl {
            row_index,
            position_in_current_row,
            position_in_other_row,
            edge_print_type,
            graph_edge,
            has_arrow,
            line_style: Self::convert_to_line_style(graph_edge.edge_type),
            color_id: presentation_manager.get_color_id(&GraphElement::Edge(graph_edge)),
            is_selected: false,
        };
        element.is_selected = presentation_manager.is_selected(
            &crate::api::printer::GraphPrintElement::Edge(element.clone()),
        );
        element
    }

    /// `TerminalEdgePrintElement`: an arrow that ends in this row (other position = own position).
    pub fn terminal(
        row_index: VcsLogVisibleGraphIndex,
        position: i32,
        edge_print_type: EdgePrintElementType,
        graph_edge: GraphEdge,
        presentation_manager: &dyn PrintElementPresentationManager,
    ) -> EdgePrintElementImpl {
        EdgePrintElementImpl::new(
            row_index,
            position,
            position,
            edge_print_type,
            graph_edge,
            true,
            presentation_manager,
        )
    }

    pub fn graph_edge(&self) -> GraphEdge {
        self.graph_edge
    }

    pub fn convert_to_line_style(edge_type: GraphEdgeType) -> LineStyle {
        match edge_type {
            GraphEdgeType::Usual | GraphEdgeType::NotLoadCommit => LineStyle::Solid,
            GraphEdgeType::Dotted
            | GraphEdgeType::DottedArrowUp
            | GraphEdgeType::DottedArrowDown => LineStyle::Dashed,
        }
    }
}

impl PrintElement for EdgePrintElementImpl {
    fn row_index(&self) -> VcsLogVisibleGraphIndex {
        self.row_index
    }
    fn position_in_current_row(&self) -> i32 {
        self.position_in_current_row
    }
    fn color_id(&self) -> i32 {
        self.color_id
    }
    fn is_selected(&self) -> bool {
        self.is_selected
    }
}

impl EdgePrintElement for EdgePrintElementImpl {
    fn position_in_other_row(&self) -> i32 {
        self.position_in_other_row
    }
    fn edge_print_type(&self) -> EdgePrintElementType {
        self.edge_print_type
    }
    fn line_style(&self) -> LineStyle {
        self.line_style
    }
    fn has_arrow(&self) -> bool {
        self.has_arrow
    }
}

/// Upstream equality: positions, row, type and arrow (graph edge, style, color and selection are ignored).
impl PartialEq for EdgePrintElementImpl {
    fn eq(&self, other: &Self) -> bool {
        self.position_in_current_row == other.position_in_current_row
            && self.position_in_other_row == other.position_in_other_row
            && self.row_index == other.row_index
            && self.edge_print_type == other.edge_print_type
            && self.has_arrow == other.has_arrow
    }
}
```

`crates/ig-log/src/print/graph_element_comparator_by_layout_index.rs`:

```rust
//! Orders the elements of a row from left to right by layout index.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphElementComparatorByLayoutIndex.java`.
//!
//! This is not a total order (special edges tie with everything at the same layout index), so sort
//! with [`java_sort`](crate::utils::java_sort), not `slice::sort_by`. Integer arithmetic wraps like
//! Java's.

use std::cmp::Ordering;

use crate::api::elements::{GraphEdge, GraphElement, GraphNode};
use crate::utils::linear_graph_utils::{as_normal_edge, get_not_null_node_index};

pub struct GraphElementComparatorByLayoutIndex {
    layout_index_getter: Box<dyn Fn(i32) -> i32 + Send + Sync>,
}

impl GraphElementComparatorByLayoutIndex {
    /// `layout_index_getter` maps a node index to its layout index.
    pub fn new(
        layout_index_getter: impl Fn(i32) -> i32 + Send + Sync + 'static,
    ) -> GraphElementComparatorByLayoutIndex {
        GraphElementComparatorByLayoutIndex {
            layout_index_getter: Box::new(layout_index_getter),
        }
    }

    /// Upstream `compare`: negative, zero or positive.
    pub fn compare(&self, o1: &GraphElement, o2: &GraphElement) -> i32 {
        match (o1, o2) {
            (GraphElement::Edge(edge1), GraphElement::Edge(edge2)) => {
                let Some(normal_edge1) = as_normal_edge(edge1) else {
                    return self
                        .compare2(edge2, GraphNode::new(get_not_null_node_index(edge1)))
                        .wrapping_neg();
                };
                let Some(normal_edge2) = as_normal_edge(edge2) else {
                    return self.compare2(edge1, GraphNode::new(get_not_null_node_index(edge2)));
                };

                if normal_edge1.up == normal_edge2.up {
                    return if normal_edge1.down < normal_edge2.down {
                        self.compare2(edge2, GraphNode::new(normal_edge1.down))
                            .wrapping_neg()
                    } else {
                        self.compare2(edge1, GraphNode::new(normal_edge2.down))
                    };
                }

                if normal_edge1.up < normal_edge2.up {
                    self.compare2(edge1, GraphNode::new(normal_edge2.up))
                } else {
                    self.compare2(edge2, GraphNode::new(normal_edge1.up))
                        .wrapping_neg()
                }
            }
            (GraphElement::Edge(edge), GraphElement::Node(node)) => self.compare2(edge, *node),
            (GraphElement::Node(node), GraphElement::Edge(edge)) => {
                self.compare2(edge, *node).wrapping_neg()
            }
            (GraphElement::Node(_), GraphElement::Node(_)) => {
                // upstream: `assert false; return 0;`
                if cfg!(debug_assertions) {
                    panic!("Comparing two nodes: {o1:?} and {o2:?}");
                }
                0
            }
        }
    }

    /// [`compare`](Self::compare) as an [`Ordering`].
    pub fn ordering(&self, o1: &GraphElement, o2: &GraphElement) -> Ordering {
        self.compare(o1, o2).cmp(&0)
    }

    fn compare2(&self, edge: &GraphEdge, node: GraphNode) -> i32 {
        let Some(normal_edge) = as_normal_edge(edge) else {
            return self
                .get_layout_index(get_not_null_node_index(edge))
                .wrapping_sub(self.get_layout_index(node.node_index));
        };

        let up_edge_li = self.get_layout_index(normal_edge.up);
        let down_edge_li = self.get_layout_index(normal_edge.down);

        let node_li = self.get_layout_index(node.node_index);
        let max_li = up_edge_li.max(down_edge_li);
        if max_li != node_li {
            max_li.wrapping_sub(node_li)
        } else {
            normal_edge.up.wrapping_sub(node.node_index)
        }
    }

    fn get_layout_index(&self, node_index: i32) -> i32 {
        (self.layout_index_getter)(node_index)
    }
}
```

`crates/ig-log/src/print/graph_color_getter.rs`:

```rust
//! The two color getter factories: by branch head (main branches and their fragments) and by node.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphColorGetterByHead.kt`
//! and `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphColorGetterByNode.kt`.

use std::sync::Arc;

use crate::api::permanent::{PermanentGraphInfo, VcsLogGraphNodeId};
use crate::api::printer::{GraphColorGetter, GraphColorGetterFactory};
use crate::graph_color_manager::GraphColorManager;

struct GraphColorGetterByHead<CommitId> {
    permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    color_manager: Arc<dyn GraphColorManager<CommitId>>,
}

impl<CommitId> GraphColorGetter for GraphColorGetterByHead<CommitId> {
    fn get_node_color(&self, node_id: VcsLogGraphNodeId, layout_index: i32) -> i32 {
        let layout = self.permanent_graph_info.permanent_graph_layout();
        let head_node_id = if node_id < 0 {
            0
        } else {
            layout.get_one_of_head_node_index(node_id)
        };
        let head_commit_id = self
            .permanent_graph_info
            .permanent_commits_info()
            .get_commit_id(head_node_id);
        let head_layout_index = layout.get_layout_index(head_node_id);
        self.color_manager
            .get_color(&head_commit_id, head_layout_index, layout_index)
    }
}

/// Colors main branches and graph fragments rather than individual nodes, using a
/// [`GraphColorManager`] keyed by the head commit of the main branch and the fragment's layout index.
pub struct GraphColorGetterByHeadFactory<CommitId> {
    color_manager: Arc<dyn GraphColorManager<CommitId>>,
}

impl<CommitId> GraphColorGetterByHeadFactory<CommitId> {
    pub fn new(color_manager: Arc<dyn GraphColorManager<CommitId>>) -> Self {
        GraphColorGetterByHeadFactory { color_manager }
    }
}

impl<CommitId: 'static> GraphColorGetterFactory<CommitId>
    for GraphColorGetterByHeadFactory<CommitId>
{
    fn create_color_getter(
        &self,
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    ) -> Arc<dyn GraphColorGetter> {
        Arc::new(GraphColorGetterByHead {
            permanent_graph_info,
            color_manager: self.color_manager.clone(),
        })
    }
}

type NodeColorFn<CommitId> = Arc<dyn Fn(&CommitId, i32) -> i32 + Send + Sync>;

struct GraphColorGetterByNode<CommitId> {
    permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    color_getter: NodeColorFn<CommitId>,
}

impl<CommitId> GraphColorGetter for GraphColorGetterByNode<CommitId> {
    fn get_node_color(&self, node_id: VcsLogGraphNodeId, layout_index: i32) -> i32 {
        let commit_id = self
            .permanent_graph_info
            .permanent_commits_info()
            .get_commit_id(node_id);
        (self.color_getter)(&commit_id, layout_index)
    }
}

/// A simple color getter factory defining the color of each node individually, from its commit id
/// and layout index.
pub struct GraphColorGetterByNodeFactory<CommitId> {
    color_getter: NodeColorFn<CommitId>,
}

impl<CommitId> GraphColorGetterByNodeFactory<CommitId> {
    pub fn new(color_getter: impl Fn(&CommitId, i32) -> i32 + Send + Sync + 'static) -> Self {
        GraphColorGetterByNodeFactory {
            color_getter: Arc::new(color_getter),
        }
    }
}

impl<CommitId: 'static> GraphColorGetterFactory<CommitId>
    for GraphColorGetterByNodeFactory<CommitId>
{
    fn create_color_getter(
        &self,
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    ) -> Arc<dyn GraphColorGetter> {
        Arc::new(GraphColorGetterByNode {
            permanent_graph_info,
            color_getter: self.color_getter.clone(),
        })
    }
}
```

`crates/ig-log/src/print/edges_in_row_generator.rs`:

```rust
//! Finds the normal edges passing through a row (neither starting nor ending in it).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/EdgesInRowGenerator.java`.
//!
//! Rows are grouped in blocks of 40; the edges at block boundaries are computed by walking up to
//! `walk_size` rows and cached. Deviations: sets are `BTreeSet`s (deterministic iteration; upstream
//! `HashSet` order depends on identity hash codes), and the cache is a [`BoundedCache`].

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::Arc;

use crate::api::EdgeFilter;
use crate::api::elements::GraphEdge;
use crate::api::linear_graph::LinearGraph;
use crate::utils::bounded_cache::BoundedCache;

const CACHE_SIZE: usize = 10;
const BLOCK_SIZE: i32 = 40;

/// Edges crossing `row`.
#[derive(Clone, Debug)]
struct GraphEdges {
    edges: BTreeSet<GraphEdge>,
    row: i32,
}

impl GraphEdges {
    fn empty(row: i32) -> GraphEdges {
        GraphEdges {
            edges: BTreeSet::new(),
            row,
        }
    }
}

pub struct EdgesInRowGenerator {
    walk_size: i32,
    graph: Arc<dyn LinearGraph>,
    cache_nu: RefCell<BoundedCache<i32, GraphEdges>>,
    cache_nd: RefCell<BoundedCache<i32, GraphEdges>>,
}

impl EdgesInRowGenerator {
    pub fn new(graph: Arc<dyn LinearGraph>) -> EdgesInRowGenerator {
        EdgesInRowGenerator::with_walk_size(graph, 1000)
    }

    pub fn with_walk_size(graph: Arc<dyn LinearGraph>, walk_size: i32) -> EdgesInRowGenerator {
        EdgesInRowGenerator {
            walk_size,
            graph,
            cache_nu: RefCell::new(BoundedCache::new(CACHE_SIZE, CACHE_SIZE * 2)),
            cache_nd: RefCell::new(BoundedCache::new(CACHE_SIZE, CACHE_SIZE * 2)),
        }
    }

    pub fn get_edges_in_row(&self, row_index: i32) -> BTreeSet<GraphEdge> {
        let mut neighbor_u = self.get_neighbor_u(row_index);
        while neighbor_u.row < row_index {
            neighbor_u = self.one_down_step(neighbor_u);
        }

        let mut neighbor_d = self.get_neighbor_d(row_index);
        while neighbor_d.row > row_index {
            neighbor_d = self.one_up_step(neighbor_d);
        }

        let mut result = neighbor_u.edges;
        result.extend(neighbor_d.edges);
        result
    }

    pub fn invalidate(&self) {
        self.cache_nu.borrow_mut().clear();
        self.cache_nd.borrow_mut().clear();
    }

    fn get_neighbor_u(&self, row_index: i32) -> GraphEdges {
        let up_neighbor_index = get_up_neighbor_index(row_index);
        if let Some(graph_edges) = self.cache_nu.borrow().get(&up_neighbor_index) {
            return graph_edges.clone();
        }
        let graph_edges = self.get_u_correct_edges(up_neighbor_index);
        self.cache_nu
            .borrow_mut()
            .put(up_neighbor_index, graph_edges.clone());
        graph_edges
    }

    fn get_neighbor_d(&self, row_index: i32) -> GraphEdges {
        let down_neighbor_index = get_up_neighbor_index(row_index) + BLOCK_SIZE;

        if down_neighbor_index >= self.graph.nodes_count() {
            return GraphEdges::empty(self.graph.nodes_count() - 1);
        }

        if let Some(graph_edges) = self.cache_nd.borrow().get(&down_neighbor_index) {
            return graph_edges.clone();
        }
        let graph_edges = self.get_d_correct_edges(down_neighbor_index);
        self.cache_nd
            .borrow_mut()
            .put(down_neighbor_index, graph_edges.clone());
        graph_edges
    }

    fn get_u_correct_edges(&self, row_index: i32) -> GraphEdges {
        let start_calculate_index = (row_index - self.walk_size).max(0);
        let mut graph_edges = GraphEdges::empty(start_calculate_index);

        for _ in start_calculate_index..row_index {
            graph_edges = self.one_down_step(graph_edges);
        }
        graph_edges
    }

    fn get_d_correct_edges(&self, row_index: i32) -> GraphEdges {
        let end_calculate_index = (row_index + self.walk_size).min(self.graph.nodes_count() - 1);
        let mut graph_edges = GraphEdges::empty(end_calculate_index);

        for _ in (row_index + 1..=end_calculate_index).rev() {
            graph_edges = self.one_up_step(graph_edges);
        }
        graph_edges
    }

    fn one_down_step(&self, graph_edges: GraphEdges) -> GraphEdges {
        let GraphEdges {
            mut edges,
            row: current_row,
        } = graph_edges;

        edges.extend(
            self.graph
                .get_adjacent_edges(current_row, EdgeFilter::NormalDown),
        );
        for edge in self
            .graph
            .get_adjacent_edges(current_row + 1, EdgeFilter::NormalUp)
        {
            edges.remove(&edge);
        }

        GraphEdges {
            edges,
            row: current_row + 1,
        }
    }

    fn one_up_step(&self, graph_edges: GraphEdges) -> GraphEdges {
        let GraphEdges {
            mut edges,
            row: current_row,
        } = graph_edges;

        edges.extend(
            self.graph
                .get_adjacent_edges(current_row, EdgeFilter::NormalUp),
        );
        for edge in self
            .graph
            .get_adjacent_edges(current_row - 1, EdgeFilter::NormalDown)
        {
            edges.remove(&edge);
        }

        GraphEdges {
            edges,
            row: current_row - 1,
        }
    }
}

fn get_up_neighbor_index(row_index: i32) -> i32 {
    (row_index / BLOCK_SIZE) * BLOCK_SIZE
}
```

`crates/ig-log/src/print/print_element_generator_impl.rs`:

```rust
//! Turns a row of the linear graph into print elements: the node, edge halves going up and down,
//! arrows for long edges, and terminal arrows.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/PrintElementGeneratorImpl.kt`.
//!
//! Long edges (at least `long_edge_size` rows) are only drawn within `visible_part_size` rows of
//! their ends, with arrows at the cut. Deviations: the generator owns its presentation manager
//! (upstream shares it with `VisibleGraphImpl`; reach it with
//! [`presentation_manager_mut`](PrintElementGeneratorImpl::presentation_manager_mut)), and the
//! row cache is a [`BoundedCache`].

use std::cell::{OnceCell, RefCell};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphEdgeType, GraphElement};
use crate::api::linear_graph::LinearGraph;
use crate::api::printer::{
    GraphPrintElement, PrintElementGenerator, PrintElementPresentationManager,
};
use crate::print::elements::{EdgePrintElementImpl, SimplePrintElementImpl};
use crate::print_element::EdgePrintElementType;
use crate::utils::bounded_cache::BoundedCache;
use crate::utils::java_sort;
use crate::utils::linear_graph_utils::{as_normal_edge, int_equal, is_edge_down, is_edge_up};
use crate::utils::normal_edge::NormalEdge;
use crate::visible_graph::VcsLogVisibleGraphIndex;

use super::edges_in_row_generator::EdgesInRowGenerator;

const VERY_LONG_EDGE_SIZE: i32 = 1000;
pub const LONG_EDGE_SIZE: i32 = 30;
const VERY_LONG_EDGE_PART_SIZE: i32 = 250;
const LONG_EDGE_PART_SIZE: i32 = 1;

const CACHE_SIZE: usize = 100;
const SAMPLE_SIZE: i32 = 20000;
const K: f64 = 0.1;

type ElementComparator = Box<dyn Fn(&GraphElement, &GraphElement) -> Ordering + Send + Sync>;

pub struct PrintElementGeneratorImpl<P> {
    linear_graph: Arc<dyn LinearGraph>,
    presentation_manager: P,
    element_comparator: ElementComparator,
    long_edge_size: i32,
    visible_part_size: i32,
    edge_with_arrow_size: i32,
    cache: RefCell<BoundedCache<i32, Vec<GraphElement>>>,
    edges_in_row_generator: EdgesInRowGenerator,
    recommended_width: OnceCell<i32>,
}

/// Positions of the elements of a neighbour row (`createEndPositionFunction`).
struct EndPositions {
    to_position: Option<HashMap<GraphElement, i32>>,
    up: bool,
}

impl EndPositions {
    fn get(&self, edge: &GraphEdge, linear_graph: &dyn LinearGraph) -> Option<i32> {
        let to_position = self.to_position.as_ref()?;
        if let Some(&position) = to_position.get(&GraphElement::Edge(*edge)) {
            return Some(position);
        }
        let node_index = if self.up {
            edge.up_node_index
        } else {
            edge.down_node_index
        }?;
        to_position
            .get(&GraphElement::Node(linear_graph.get_graph_node(node_index)))
            .copied()
    }
}

impl<P: PrintElementPresentationManager> PrintElementGeneratorImpl<P> {
    /// Production sizes: long edges are cut unless `show_long_edges`.
    pub fn new(
        graph: Arc<dyn LinearGraph>,
        presentation_manager: P,
        show_long_edges: bool,
        element_comparator: impl Fn(&GraphElement, &GraphElement) -> Ordering + Send + Sync + 'static,
    ) -> PrintElementGeneratorImpl<P> {
        let (long_edge_size, visible_part_size, edge_with_arrow_size) = if show_long_edges {
            (
                VERY_LONG_EDGE_SIZE,
                VERY_LONG_EDGE_PART_SIZE,
                LONG_EDGE_SIZE,
            )
        } else {
            (LONG_EDGE_SIZE, LONG_EDGE_PART_SIZE, i32::MAX)
        };
        PrintElementGeneratorImpl::with_sizes(
            graph,
            presentation_manager,
            element_comparator,
            long_edge_size,
            visible_part_size,
            edge_with_arrow_size,
        )
    }

    /// The `@VisibleForTesting` constructor with explicit sizes.
    pub fn with_sizes(
        graph: Arc<dyn LinearGraph>,
        presentation_manager: P,
        element_comparator: impl Fn(&GraphElement, &GraphElement) -> Ordering + Send + Sync + 'static,
        long_edge_size: i32,
        visible_part_size: i32,
        edge_with_arrow_size: i32,
    ) -> PrintElementGeneratorImpl<P> {
        PrintElementGeneratorImpl {
            edges_in_row_generator: EdgesInRowGenerator::new(graph.clone()),
            linear_graph: graph,
            presentation_manager,
            element_comparator: Box::new(element_comparator),
            long_edge_size,
            visible_part_size,
            edge_with_arrow_size,
            cache: RefCell::new(BoundedCache::new(CACHE_SIZE, CACHE_SIZE * 2)),
            recommended_width: OnceCell::new(),
        }
    }

    pub fn presentation_manager(&self) -> &P {
        &self.presentation_manager
    }

    pub fn presentation_manager_mut(&mut self) -> &mut P {
        &mut self.presentation_manager
    }

    /// Weighted mean plus standard deviation of the row widths of the first rows (computed once).
    pub fn recommended_width(&self) -> i32 {
        *self
            .recommended_width
            .get_or_init(|| self.calculate_recommended_width())
    }

    fn calculate_recommended_width(&self) -> i32 {
        let nodes_count = self.linear_graph.nodes_count();
        if nodes_count == 0 {
            return 0;
        }
        if nodes_count == 1 {
            return 1;
        }

        let n = SAMPLE_SIZE.min(nodes_count);

        let mut sum = 0.0;
        let mut sum_squares = 0.0;
        let mut edges_count = 0;
        let mut current_normal_edges: HashSet<NormalEdge> = HashSet::new();

        for i in 0..n {
            let adjacent_edges = self.linear_graph.get_adjacent_edges(i, EdgeFilter::All);
            let mut up_arrows = 0;
            let mut down_arrows = 0;
            for e in &adjacent_edges {
                match as_normal_edge(e) {
                    Some(normal_edge) => {
                        if is_edge_up(e, i) {
                            current_normal_edges.remove(&normal_edge);
                        } else {
                            current_normal_edges.insert(normal_edge);
                        }
                    }
                    None => {
                        if e.edge_type == GraphEdgeType::DottedArrowUp {
                            up_arrows += 1;
                        } else {
                            down_arrows += 1;
                        }
                    }
                }
            }

            let mut new_edges_count = 0;
            for e in &current_normal_edges {
                if self.is_normal_edge_visible_in_row(e, i) {
                    new_edges_count += 1;
                } else {
                    match self.get_normal_arrow_type(e, i) {
                        Some(EdgePrintElementType::Down) => down_arrows += 1,
                        Some(EdgePrintElementType::Up) => up_arrows += 1,
                        None => {}
                    }
                }
            }

            // 0 <= K < 1; weight is an arithmetic progression, starting at 2 / (n * (k + 1)) and
            // ending at k * 2 / (n * (k + 1)); this ensures that the sum of all weights is 1
            let width = (edges_count + up_arrows).max(new_edges_count + down_arrows);
            let weight = 2.0 / (f64::from(n) * (K + 1.0))
                * (1.0 + (K - 1.0) * f64::from(i) / f64::from(n - 1));
            sum += f64::from(width) * weight;
            sum_squares += f64::from(width) * f64::from(width) * weight;

            edges_count = new_edges_count;
        }

        // weighted variance, see
        // http://stackoverflow.com/questions/30383270/how-do-i-calculate-the-standard-deviation-between-weighted-measurements
        let average = sum;
        let deviation = (sum_squares - average * average).sqrt();
        // `Math.round`; NaN becomes 0 in both languages
        (average + deviation).round() as i32
    }

    fn create_end_position_function(
        &self,
        visible_row_index: VcsLogVisibleGraphIndex,
        up: bool,
    ) -> EndPositions {
        if visible_row_index < 0 || visible_row_index >= self.linear_graph.nodes_count() {
            return EndPositions {
                to_position: None,
                up,
            };
        }

        let visible_elements_in_next_row =
            self.get_sorted_visible_elements_in_row(visible_row_index);
        let to_position = visible_elements_in_next_row
            .into_iter()
            .enumerate()
            .map(|(position, element)| (element, position as i32))
            .collect();
        EndPositions {
            to_position: Some(to_position),
            up,
        }
    }

    fn get_arrow_type(
        &self,
        edge: &GraphEdge,
        row_index: VcsLogVisibleGraphIndex,
    ) -> Option<EdgePrintElementType> {
        if let Some(normal_edge) = as_normal_edge(edge) {
            return self.get_normal_arrow_type(&normal_edge, row_index);
        }
        // special edges
        match edge.edge_type {
            GraphEdgeType::DottedArrowDown | GraphEdgeType::NotLoadCommit => {
                if int_equal(edge.up_node_index, row_index - 1) {
                    return Some(EdgePrintElementType::Down);
                }
            }
            GraphEdgeType::DottedArrowUp => {
                // todo case 0-row arrow
                if int_equal(edge.down_node_index, row_index + 1) {
                    return Some(EdgePrintElementType::Up);
                }
            }
            // upstream logs "Unknown special edge type"; unreachable for normal edge types
            GraphEdgeType::Usual | GraphEdgeType::Dotted => {}
        }
        None
    }

    fn get_normal_arrow_type(
        &self,
        normal_edge: &NormalEdge,
        row_index: VcsLogVisibleGraphIndex,
    ) -> Option<EdgePrintElementType> {
        let edge_size = normal_edge.down - normal_edge.up;
        let up_offset = row_index - normal_edge.up;
        let down_offset = normal_edge.down - row_index;

        if edge_size >= self.long_edge_size {
            if up_offset == self.visible_part_size {
                // can not happen due to how constants are picked out, but just in case
                debug_assert!(
                    down_offset != self.visible_part_size,
                    "Both up and down arrow at row {row_index}"
                );
                return Some(EdgePrintElementType::Down);
            }
            if down_offset == self.visible_part_size {
                return Some(EdgePrintElementType::Up);
            }
        }
        if edge_size >= self.edge_with_arrow_size {
            if up_offset == 1 {
                debug_assert!(
                    down_offset != 1,
                    "Both up and down arrow at row {row_index}"
                );
                return Some(EdgePrintElementType::Down);
            }
            if down_offset == 1 {
                return Some(EdgePrintElementType::Up);
            }
        }
        None
    }

    /// Special edges are never "visible in row" (see `get_sorted_visible_elements_in_row`).
    fn is_edge_visible_in_row(&self, edge: &GraphEdge, visible_row_index: i32) -> bool {
        match as_normal_edge(edge) {
            Some(normal_edge) => {
                self.is_normal_edge_visible_in_row(&normal_edge, visible_row_index)
            }
            None => false,
        }
    }

    fn is_normal_edge_visible_in_row(
        &self,
        normal_edge: &NormalEdge,
        visible_row_index: i32,
    ) -> bool {
        normal_edge.down - normal_edge.up < self.long_edge_size
            || get_attachment_distance(normal_edge, visible_row_index) <= self.visible_part_size
    }

    fn get_sorted_visible_elements_in_row(
        &self,
        row_index: VcsLogVisibleGraphIndex,
    ) -> Vec<GraphElement> {
        if let Some(graph_elements) = self.cache.borrow().get(&row_index) {
            return graph_elements.clone();
        }

        let mut result = vec![GraphElement::Node(
            self.linear_graph.get_graph_node(row_index),
        )];

        result.extend(
            self.edges_in_row_generator
                .get_edges_in_row(row_index)
                .into_iter()
                .filter(|edge| self.is_edge_visible_in_row(edge, row_index))
                .map(GraphElement::Edge),
        );
        if row_index > 0 {
            result.extend(
                self.linear_graph
                    .get_adjacent_edges(row_index - 1, EdgeFilter::Special)
                    .into_iter()
                    .filter(|edge| is_edge_down(edge, row_index - 1))
                    .map(GraphElement::Edge),
            );
        }
        if row_index < self.linear_graph.nodes_count() - 1 {
            result.extend(
                self.linear_graph
                    .get_adjacent_edges(row_index + 1, EdgeFilter::Special)
                    .into_iter()
                    .filter(|edge| is_edge_up(edge, row_index + 1))
                    .map(GraphElement::Edge),
            );
        }

        java_sort::sort_by(&mut result, |a, b| (self.element_comparator)(a, b));
        self.cache.borrow_mut().put(row_index, result.clone());
        result
    }

    fn edge_element(
        &self,
        row_index: i32,
        position: i32,
        other_position: i32,
        edge_type: EdgePrintElementType,
        edge: GraphEdge,
        arrow_type: Option<EdgePrintElementType>,
    ) -> GraphPrintElement {
        GraphPrintElement::Edge(EdgePrintElementImpl::new(
            row_index,
            position,
            other_position,
            edge_type,
            edge,
            arrow_type == Some(edge_type),
            &self.presentation_manager,
        ))
    }
}

impl<P: PrintElementPresentationManager> PrintElementGenerator for PrintElementGeneratorImpl<P> {
    fn get_print_elements(&self, row_index: VcsLogVisibleGraphIndex) -> Vec<GraphPrintElement> {
        let mut result = Vec::new();
        // nodes at the end, to be drawn over the edges
        let mut nodes = Vec::new();

        let visible_elements = self.get_sorted_visible_elements_in_row(row_index);
        let up_position = self.create_end_position_function(row_index - 1, true);
        let down_position = self.create_end_position_function(row_index + 1, false);
        let graph = &*self.linear_graph;

        for (position, element) in visible_elements.into_iter().enumerate() {
            let position = position as i32;
            match element {
                GraphElement::Node(node) => {
                    nodes.push(GraphPrintElement::Node(SimplePrintElementImpl::new(
                        row_index,
                        position,
                        node,
                        &self.presentation_manager,
                    )));
                    for edge in graph.get_adjacent_edges(node.node_index, EdgeFilter::All) {
                        let arrow_type = self.get_arrow_type(&edge, row_index);
                        let down = down_position.get(&edge, graph);
                        let up = up_position.get(&edge, graph);
                        if let Some(down) = down {
                            let down_type = EdgePrintElementType::Down;
                            result.push(self.edge_element(
                                row_index, position, down, down_type, edge, arrow_type,
                            ));
                        }
                        if let Some(up) = up {
                            let up_type = EdgePrintElementType::Up;
                            result.push(
                                self.edge_element(
                                    row_index, position, up, up_type, edge, arrow_type,
                                ),
                            );
                        }
                    }
                }
                GraphElement::Edge(edge) => {
                    let down = down_position.get(&edge, graph);
                    let up = up_position.get(&edge, graph);
                    let arrow_type = self.get_arrow_type(&edge, row_index);

                    if let Some(down) = down {
                        let down_type = EdgePrintElementType::Down;
                        result.push(
                            self.edge_element(
                                row_index, position, down, down_type, edge, arrow_type,
                            ),
                        );
                    } else if arrow_type == Some(EdgePrintElementType::Down) {
                        result.push(GraphPrintElement::Edge(EdgePrintElementImpl::terminal(
                            row_index,
                            position,
                            EdgePrintElementType::Down,
                            edge,
                            &self.presentation_manager,
                        )));
                    }
                    if let Some(up) = up {
                        let up_type = EdgePrintElementType::Up;
                        result.push(
                            self.edge_element(row_index, position, up, up_type, edge, arrow_type),
                        );
                    } else if arrow_type == Some(EdgePrintElementType::Up) {
                        result.push(GraphPrintElement::Edge(EdgePrintElementImpl::terminal(
                            row_index,
                            position,
                            EdgePrintElementType::Up,
                            edge,
                            &self.presentation_manager,
                        )));
                    }
                }
            }
        }
        result.extend(nodes);
        result
    }
}

fn get_attachment_distance(e1: &NormalEdge, row_index: VcsLogVisibleGraphIndex) -> i32 {
    (row_index - e1.up).min(e1.down - row_index)
}
```

- [ ] **Step 8: Edge storage (`collapsing`).** Create these files with exactly this content.

`crates/ig-log/src/collapsing/mod.rs`:

```rust
//! Edge storage used by [`LinearGraphWrapper`](crate::facade::linear_graph_wrapper::LinearGraphWrapper).
//!
//! Only `EdgeStorage.java` and `EdgeStorageWrapper.java` of
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/` are ported; the collapsing
//! machinery (`CollapsedGraph`, `CollapsedController`, fragment generators, filters) is Plan 2b.

pub mod edge_storage;
pub mod edge_storage_wrapper;
```

`crates/ig-log/src/collapsing/edge_storage.rs`:

```rust
//! Additional edges stored by node id, each packed with its type into one `i32`.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/EdgeStorage.java`.

use crate::api::elements::GraphEdgeType;
use crate::utils::int_int_multi_map::IntIntMultiMap;

const EDGE_TYPE_BITS: u32 = 4;
const EDGE_BITS_OFFSET: u32 = i32::BITS - EDGE_TYPE_BITS;
const COMPRESSED_NODE_ID_MASK: i32 = (u32::MAX >> EDGE_TYPE_BITS) as i32;
const MAX_EDGE_TYPE_COUNT: usize = 1 << EDGE_TYPE_BITS;
const MAX_NODE_ID: i32 = i32::MAX >> EDGE_TYPE_BITS;
const MIN_NODE_ID: i32 = i32::MIN >> EDGE_TYPE_BITS;

const _: () = assert!(GraphEdgeType::VALUES.len() <= MAX_EDGE_TYPE_COUNT);

/// Normal edges are stored under both node ids; special edges only under their node.
#[derive(Clone, Debug, Default)]
pub struct EdgeStorage {
    edges: IntIntMultiMap,
}

impl EdgeStorage {
    /// Stands for a missing target id.
    pub const NULL_ID: i32 = MIN_NODE_ID;

    pub fn new() -> EdgeStorage {
        EdgeStorage::default()
    }

    pub fn create_edge(&mut self, main_node_id: i32, addition_id: i32, edge_type: GraphEdgeType) {
        self.edges
            .put_value(main_node_id, compress_edge(addition_id, edge_type));
        if edge_type.is_normal_edge() {
            self.edges
                .put_value(addition_id, compress_edge(main_node_id, edge_type));
        }
    }

    pub fn remove_edge(&mut self, main_node_id: i32, addition_id: i32, edge_type: GraphEdgeType) {
        self.edges
            .remove(main_node_id, compress_edge(addition_id, edge_type));
        if edge_type.is_normal_edge() {
            self.edges
                .remove(addition_id, compress_edge(main_node_id, edge_type));
        }
    }

    /// Edges of `node_id` as (other node id or target id, type); `None` for [`Self::NULL_ID`].
    pub fn get_edges(&self, node_id: i32) -> Vec<(Option<i32>, GraphEdgeType)> {
        self.edges
            .get(node_id)
            .iter()
            .map(|&compressed| {
                (
                    convert_to_integer(retrieved_node_id(compressed)),
                    retrieved_type(compressed),
                )
            })
            .collect()
    }

    pub fn get_known_ids(&self) -> Vec<i32> {
        self.edges.keys()
    }

    pub fn remove_all(&mut self) {
        self.edges.clear();
    }
}

fn convert_to_integer(value: i32) -> Option<i32> {
    if value == EdgeStorage::NULL_ID {
        None
    } else {
        Some(value)
    }
}

fn compress_edge(node_id: i32, edge_type: GraphEdgeType) -> i32 {
    debug_assert!(
        node_id == EdgeStorage::NULL_ID || (node_id < MAX_NODE_ID && node_id > MIN_NODE_ID)
    );
    (edge_type.ordinal() << EDGE_BITS_OFFSET) | (COMPRESSED_NODE_ID_MASK & node_id)
}

fn retrieved_type(compress_edge: i32) -> GraphEdgeType {
    let edge_type = (compress_edge as u32) >> EDGE_BITS_OFFSET;
    GraphEdgeType::VALUES[edge_type as usize]
}

fn retrieved_node_id(compress_edge: i32) -> i32 {
    (compress_edge << EDGE_TYPE_BITS) >> EDGE_TYPE_BITS
}
```

`crates/ig-log/src/collapsing/edge_storage_wrapper.rs`:

```rust
//! Node-index view of an [`EdgeStorage`] (which is keyed by node id).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/EdgeStorageWrapper.java`.
//!
//! Deviation: the storage is shared through `Arc<RwLock<_>>` (upstream shares the mutable object
//! between wrappers and `CollapsedGraph`), so mutating methods take `&self`.

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphEdgeType};
use crate::api::linear_graph::LinearGraph;
use crate::collapsing::edge_storage::EdgeStorage;
use crate::utils::linear_graph_utils::int_equal;

type IndexById = Arc<dyn Fn(i32) -> Option<i32> + Send + Sync>;
type IdByIndex = Arc<dyn Fn(i32) -> i32 + Send + Sync>;

#[derive(Clone)]
pub struct EdgeStorageWrapper {
    edge_storage: Arc<RwLock<EdgeStorage>>,
    get_node_index_by_id: IndexById,
    get_node_id_by_index: IdByIndex,
}

impl EdgeStorageWrapper {
    pub fn new(
        edge_storage: Arc<RwLock<EdgeStorage>>,
        get_node_index_by_id: impl Fn(i32) -> Option<i32> + Send + Sync + 'static,
        get_node_id_by_index: impl Fn(i32) -> i32 + Send + Sync + 'static,
    ) -> EdgeStorageWrapper {
        EdgeStorageWrapper {
            edge_storage,
            get_node_index_by_id: Arc::new(get_node_index_by_id),
            get_node_id_by_index: Arc::new(get_node_id_by_index),
        }
    }

    /// Converts with the node ids / indexes of `graph`.
    pub fn for_graph(
        edge_storage: Arc<RwLock<EdgeStorage>>,
        graph: Arc<dyn LinearGraph>,
    ) -> EdgeStorageWrapper {
        let graph2 = graph.clone();
        EdgeStorageWrapper::new(
            edge_storage,
            move |node_id| graph.get_node_index(node_id),
            move |node_index| graph2.get_node_id(node_index),
        )
    }

    /// `createSimpleEdgeStorage`: a fresh storage where node ids are node indexes.
    pub fn create_simple_edge_storage() -> EdgeStorageWrapper {
        EdgeStorageWrapper::new(Arc::new(RwLock::new(EdgeStorage::new())), Some, |index| {
            index
        })
    }

    fn storage(&self) -> std::sync::RwLockReadGuard<'_, EdgeStorage> {
        self.edge_storage
            .read()
            .expect("edge storage lock poisoned")
    }

    fn storage_mut(&self) -> std::sync::RwLockWriteGuard<'_, EdgeStorage> {
        self.edge_storage
            .write()
            .expect("edge storage lock poisoned")
    }

    pub fn remove_edge(&self, graph_edge: &GraphEdge) {
        let (main_id, addition_id) = self.get_node_ids(graph_edge);
        self.storage_mut()
            .remove_edge(main_id, addition_id, graph_edge.edge_type);
    }

    pub fn create_edge(&self, graph_edge: &GraphEdge) {
        let (main_id, addition_id) = self.get_node_ids(graph_edge);
        self.storage_mut()
            .create_edge(main_id, addition_id, graph_edge.edge_type);
    }

    /// Whether a normal edge between the two node indexes is stored.
    pub fn has_edge(&self, from_index: i32, to_index: i32) -> bool {
        let to_id = (self.get_node_id_by_index)(to_index);
        self.storage()
            .get_edges((self.get_node_id_by_index)(from_index))
            .into_iter()
            .any(|(other, edge_type)| edge_type.is_normal_edge() && int_equal(other, to_id))
    }

    pub fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        let retrieved = self
            .storage()
            .get_edges((self.get_node_id_by_index)(node_index));
        retrieved
            .into_iter()
            .filter_map(|(target_id, edge_type)| {
                self.decompress_edge(node_index, target_id, edge_type)
            })
            .filter(|edge| matched_edge(node_index, edge, filter))
            .collect()
    }

    pub fn get_edges(&self) -> HashSet<GraphEdge> {
        let known_ids = self.storage().get_known_ids();
        let mut result = HashSet::new();
        for id in known_ids {
            // upstream passes a possibly null index on; an unknown id has no edges to report
            if let Some(index) = (self.get_node_index_by_id)(id) {
                result.extend(self.get_adjacent_edges(index, EdgeFilter::All));
            }
        }
        result
    }

    fn get_node_ids(&self, graph_edge: &GraphEdge) -> (i32, i32) {
        match (graph_edge.up_node_index, graph_edge.down_node_index) {
            (Some(up), Some(down)) => (
                (self.get_node_id_by_index)(up),
                (self.get_node_id_by_index)(down),
            ),
            (Some(up), None) => (
                (self.get_node_id_by_index)(up),
                convert_to_int(graph_edge.target_id),
            ),
            (None, down) => {
                let down = down.expect("edge without node indexes");
                (
                    (self.get_node_id_by_index)(down),
                    convert_to_int(graph_edge.target_id),
                )
            }
        }
    }

    fn decompress_edge(
        &self,
        node_index: i32,
        target_id: Option<i32>,
        edge_type: GraphEdgeType,
    ) -> Option<GraphEdge> {
        if edge_type.is_normal_edge() {
            let target_id = target_id.expect("normal edge without target");
            // todo edge to hide node
            let another_node_index = (self.get_node_index_by_id)(target_id)?;
            Some(GraphEdge::create_normal_edge(
                node_index,
                another_node_index,
                edge_type,
            ))
        } else {
            Some(GraphEdge::create_edge_with_target_id(
                node_index, target_id, edge_type,
            ))
        }
    }

    pub fn remove_all(&self) {
        self.storage_mut().remove_all();
    }
}

fn matched_edge(start_node_index: i32, edge: &GraphEdge, filter: EdgeFilter) -> bool {
    if edge.edge_type.is_normal_edge() {
        (start_node_index == convert_to_int(edge.down_node_index) && filter.up_normal())
            || (start_node_index == convert_to_int(edge.up_node_index) && filter.down_normal())
    } else {
        filter.special()
    }
}

fn convert_to_int(value: Option<i32>) -> i32 {
    value.unwrap_or(EdgeStorage::NULL_ID)
}
```

- [ ] **Step 9: Facade (`facade`).** Create these files with exactly this content.

`crates/ig-log/src/facade/mod.rs`:

```rust
//! The graph facade: the permanent graph, visible graphs and the controller chain behind them.
//!
//! Ports the `Normal`-sort subset of `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/`;
//! see each submodule. Not ported yet (Plan 2b): `CascadeController.kt`, `FilteredController.kt`,
//! `FirstParentController.kt`, `GraphModificationUtil.kt`, `SimpleGraphInfo.java` and `sort/**`.

pub mod base_controller;
pub mod graph_changes;
pub mod linear_graph_controller;
pub mod linear_graph_wrapper;
pub mod permanent_graph_impl;
pub mod print_element_presentation_manager_impl;
pub mod reachable_nodes;
pub mod visible_graph_impl;
```

`crates/ig-log/src/facade/graph_changes.rs`:

```rust
//! Changes of a linear graph reported by controllers, by node id.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/GraphChanges.java`
//! and `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/GraphChangesUtil.java`.
//!
//! Deviation: the interfaces and their `*Impl` classes collapse into plain structs. Upstream's
//! `edgesReplaced` collects into a `HashSet` of `EdgeImpl`s, which has identity equality, so it is
//! effectively a list; here it is a `Vec`.

use crate::api::elements::GraphEdge;
use crate::api::linear_graph::LinearGraph;

/// A node that appeared or was removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node<NodeId> {
    pub node_id: NodeId,
    pub removed: bool,
}

/// An edge that appeared or was removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edge<NodeId> {
    pub up_node_id: Option<NodeId>,
    pub down_node_id: Option<NodeId>,
    pub target_id: Option<NodeId>,
    pub removed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphChanges<NodeId> {
    pub changed_nodes: Vec<Node<NodeId>>,
    pub changed_edges: Vec<Edge<NodeId>>,
}

impl<NodeId> GraphChanges<NodeId> {
    pub fn new(changed_nodes: Vec<Node<NodeId>>, changed_edges: Vec<Edge<NodeId>>) -> Self {
        GraphChanges {
            changed_nodes,
            changed_edges,
        }
    }

    /// `GraphChangesUtil.SOME_CHANGES`: "something changed", without details.
    pub fn some_changes() -> Self {
        GraphChanges::new(Vec::new(), Vec::new())
    }
}

/// `GraphChangesUtil.edgeChanged`: the edge with node indexes of `graph` converted to node ids.
pub fn edge_changed(edge: &GraphEdge, graph: &dyn LinearGraph, removed: bool) -> Edge<i32> {
    Edge {
        up_node_id: edge.up_node_index.map(|index| graph.get_node_id(index)),
        down_node_id: edge.down_node_index.map(|index| graph.get_node_id(index)),
        target_id: edge.target_id,
        removed,
    }
}

/// `GraphChangesUtil.edgesReplaced`.
pub fn edges_replaced(
    removed_edges: &[GraphEdge],
    added_edges: &[GraphEdge],
    delegate_graph: &dyn LinearGraph,
) -> GraphChanges<i32> {
    let edge_changes = removed_edges
        .iter()
        .map(|edge| edge_changed(edge, delegate_graph, true))
        .chain(
            added_edges
                .iter()
                .map(|edge| edge_changed(edge, delegate_graph, false)),
        )
        .collect();
    GraphChanges::new(Vec::new(), edge_changes)
}
```

`crates/ig-log/src/facade/linear_graph_controller.rs`:

```rust
//! A controller owns the graph shown by a visible graph and reacts to actions on it.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphController.kt`.
//!
//! Deviation: `compiled_graph` returns a shared handle; controllers that modify their graph (Plan 2b)
//! hand out a new handle after each change, and the visible graph re-reads it.

use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use crate::actions::{Cursor, GraphAction, GraphUpdater};
use crate::api::linear_graph::LinearGraph;
use crate::facade::graph_changes::GraphChanges;

/// A [`GraphAction`] whose affected element belongs to this controller's graph
/// (`LinearGraphAction` / `VisibleGraphImpl.LinearGraphActionImpl`).
pub type LinearGraphAction = GraphAction;

/// Answer of a controller; ids are node ids.
pub struct LinearGraphAnswer {
    pub graph_changes: Option<GraphChanges<i32>>,
    pub cursor_to_set: Option<Cursor>,
    pub selected_node_ids: Option<HashSet<i32>>,
    pub graph_updater: Option<GraphUpdater>,
}

impl LinearGraphAnswer {
    pub fn new(
        graph_changes: Option<GraphChanges<i32>>,
        cursor_to_set: Option<Cursor>,
        selected_node_ids: Option<HashSet<i32>>,
        graph_updater: Option<GraphUpdater>,
    ) -> LinearGraphAnswer {
        LinearGraphAnswer {
            graph_changes,
            cursor_to_set,
            selected_node_ids,
            graph_updater,
        }
    }

    /// `LinearGraphAnswer(cursor, selectedNodeIds)`.
    pub fn with_cursor(cursor: Option<Cursor>, selected_node_ids: Option<HashSet<i32>>) -> Self {
        LinearGraphAnswer::new(None, cursor, selected_node_ids, None)
    }

    /// `LinearGraphAnswer(graphChanges, graphUpdater = null)`.
    pub fn with_changes(
        graph_changes: Option<GraphChanges<i32>>,
        graph_updater: Option<GraphUpdater>,
    ) -> Self {
        LinearGraphAnswer::new(graph_changes, None, None, graph_updater)
    }
}

impl fmt::Debug for LinearGraphAnswer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinearGraphAnswer")
            .field("graph_changes", &self.graph_changes)
            .field("cursor_to_set", &self.cursor_to_set)
            .field("selected_node_ids", &self.selected_node_ids)
            .field("graph_updater", &self.graph_updater.is_some())
            .finish()
    }
}

pub trait LinearGraphController: Send {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph>;

    fn perform_linear_graph_action(&mut self, action: &LinearGraphAction) -> LinearGraphAnswer;
}
```

`crates/ig-log/src/facade/linear_graph_wrapper.rs`:

```rust
//! A linear graph with extra (dotted) edges added and some edges hidden.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphWrapper.kt`.
//! Used by the filter and first-parent controllers (Plan 2b); not on the unfiltered `Normal` path.

use std::sync::Arc;

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphNode};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::VcsLogGraphNodeId;
use crate::collapsing::edge_storage_wrapper::EdgeStorageWrapper;
use crate::visible_graph::VcsLogVisibleGraphIndex;

pub struct LinearGraphWrapper {
    pub graph: Arc<dyn LinearGraph>,
    pub hidden_edges: EdgeStorageWrapper,
    pub dotted_edges: EdgeStorageWrapper,
}

impl LinearGraphWrapper {
    /// With fresh, empty hidden and dotted edge storages.
    pub fn new(graph: Arc<dyn LinearGraph>) -> LinearGraphWrapper {
        LinearGraphWrapper::with_edges(
            graph,
            EdgeStorageWrapper::create_simple_edge_storage(),
            EdgeStorageWrapper::create_simple_edge_storage(),
        )
    }

    pub fn with_edges(
        graph: Arc<dyn LinearGraph>,
        hidden_edges: EdgeStorageWrapper,
        dotted_edges: EdgeStorageWrapper,
    ) -> LinearGraphWrapper {
        LinearGraphWrapper {
            graph,
            hidden_edges,
            dotted_edges,
        }
    }
}

impl LinearGraph for LinearGraphWrapper {
    /// Dotted edges first, then the graph's own edges, minus every edge equal to a hidden one.
    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        let mut result = self.dotted_edges.get_adjacent_edges(node_index, filter);
        result.extend(self.graph.get_adjacent_edges(node_index, filter));
        let hidden = self.hidden_edges.get_adjacent_edges(node_index, filter);
        result.retain(|edge| !hidden.contains(edge));
        result
    }

    fn nodes_count(&self) -> i32 {
        self.graph.nodes_count()
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        self.graph.get_graph_node(node_index)
    }

    fn get_node_id(&self, node_index: VcsLogVisibleGraphIndex) -> VcsLogGraphNodeId {
        self.graph.get_node_id(node_index)
    }

    fn get_node_index(&self, node_id: VcsLogGraphNodeId) -> Option<VcsLogVisibleGraphIndex> {
        self.graph.get_node_index(node_id)
    }
}
```

`crates/ig-log/src/facade/base_controller.rs`:

```rust
//! The bottom of every controller chain: shows the permanent linear graph as is.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/BaseController.java`.

use std::sync::Arc;

use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};
use crate::utils::linear_graph_utils::default_graph_answer;

pub struct BaseController<CommitId> {
    permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
}

impl<CommitId> BaseController<CommitId> {
    pub fn new(permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>) -> Self {
        BaseController {
            permanent_graph_info,
        }
    }
}

impl<CommitId> LinearGraphController for BaseController<CommitId> {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.permanent_graph_info.linear_graph()
    }

    /// Always the default answer: default cursor, nothing selected.
    fn perform_linear_graph_action(&mut self, _action: &LinearGraphAction) -> LinearGraphAnswer {
        default_graph_answer()
    }
}
```

`crates/ig-log/src/facade/reachable_nodes.rs`:

```rust
//! Reachability queries on the permanent graph: containing branches and "is in branch" sets.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/ReachableNodes.kt`.
//! The shared `visited` flags are guarded by a `Mutex` (upstream: `synchronized`).

use std::collections::HashSet;
use std::sync::Mutex;

use crate::api::linear_graph::LiteLinearGraph;
use crate::api::permanent::VcsLogGraphNodeId;
use crate::utils::dfs_util::DfsWalk;
use crate::utils::flags::{BitSetFlags, Flags};

pub struct ReachableNodes<G> {
    graph: G,
    visited: Mutex<BitSetFlags>,
}

impl<G: LiteLinearGraph> ReachableNodes<G> {
    pub fn new(graph: G) -> ReachableNodes<G> {
        let visited = Mutex::new(BitSetFlags::new(graph.nodes_count()));
        ReachableNodes { graph, visited }
    }

    /// Branch nodes (from `branch_node_indexes`) from which `node_id` can be reached going up.
    pub fn get_containing_branches(
        &self,
        node_id: VcsLogGraphNodeId,
        branch_node_indexes: &HashSet<VcsLogGraphNodeId>,
    ) -> HashSet<VcsLogGraphNodeId> {
        let mut result = HashSet::new();

        self.walk([node_id], false, |node| {
            if branch_node_indexes.contains(&node) {
                result.insert(node);
            }
            true
        });

        result
    }

    /// Calls `consumer` for every node reachable down from `head_ids` (negative ids are skipped).
    pub fn walk_down(
        &self,
        head_ids: impl IntoIterator<Item = VcsLogGraphNodeId>,
        mut consumer: impl FnMut(VcsLogGraphNodeId),
    ) {
        self.walk(head_ids, true, |node| {
            consumer(node);
            true
        });
    }

    pub fn walk(
        &self,
        start_nodes: impl IntoIterator<Item = VcsLogGraphNodeId>,
        go_down: bool,
        consumer: impl FnMut(VcsLogGraphNodeId) -> bool,
    ) {
        let mut visited = self.visited.lock().unwrap_or_else(|e| e.into_inner());
        visited.set_all(false);
        DfsWalk::new(start_nodes, &self.graph, &mut *visited).walk(go_down, consumer);
    }
}
```

`crates/ig-log/src/facade/print_element_presentation_manager_impl.rs`:

```rust
//! Colors and selection of the visible graph's print elements.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/PrintElementPresentationManagerImpl.kt`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::api::elements::GraphElement;
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::{PermanentGraphInfo, VcsLogGraphNodeId};
use crate::api::printer::{GraphColorGetter, GraphPrintElement, PrintElementPresentationManager};
use crate::utils::linear_graph_utils::{as_normal_edge, get_not_null_node_index};

/// What is selected: one hovered print element, or a set of node ids.
#[derive(Clone, Debug)]
pub enum Selection {
    FromPrintElement(GraphPrintElement),
    FromNodeIds(HashSet<VcsLogGraphNodeId>),
}

impl PartialEq for Selection {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Selection::FromPrintElement(a), Selection::FromPrintElement(b)) => a == b,
            (Selection::FromNodeIds(a), Selection::FromNodeIds(b)) => a == b,
            _ => false,
        }
    }
}

impl Selection {
    fn is_selected(
        &self,
        print_element: &GraphPrintElement,
        linear_graph: &dyn LinearGraph,
    ) -> bool {
        match self {
            Selection::FromPrintElement(selected) => print_element == selected,
            Selection::FromNodeIds(selected_node_ids) => match print_element.graph_element() {
                GraphElement::Node(node) => {
                    selected_node_ids.contains(&linear_graph.get_node_id(node.node_index))
                }
                GraphElement::Edge(edge) => {
                    let contains_index = |index: Option<i32>| {
                        index.is_none_or(|index| {
                            selected_node_ids.contains(&linear_graph.get_node_id(index))
                        })
                    };
                    edge.target_id
                        .is_none_or(|target| selected_node_ids.contains(&target))
                        && contains_index(edge.up_node_index)
                        && contains_index(edge.down_node_index)
                }
            },
        }
    }
}

pub struct PrintElementPresentationManagerImpl<CommitId> {
    permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    linear_graph: Arc<dyn LinearGraph>,
    color_getter: Arc<dyn GraphColorGetter>,
    selection: Selection,
}

impl<CommitId> PrintElementPresentationManagerImpl<CommitId> {
    pub fn new(
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
        linear_graph: Arc<dyn LinearGraph>,
        color_getter: Arc<dyn GraphColorGetter>,
    ) -> Self {
        PrintElementPresentationManagerImpl {
            permanent_graph_info,
            linear_graph,
            color_getter,
            selection: Selection::FromNodeIds(HashSet::new()),
        }
    }

    /// Returns whether the selection changed.
    pub fn set_selected_element(&mut self, print_element: GraphPrintElement) -> bool {
        self.set_selection(Selection::FromPrintElement(print_element))
    }

    /// Returns whether the selection changed.
    pub fn set_selected_elements(&mut self, selected_node_id: HashSet<VcsLogGraphNodeId>) -> bool {
        self.set_selection(Selection::FromNodeIds(selected_node_id))
    }

    fn set_selection(&mut self, new_selection: Selection) -> bool {
        if new_selection == self.selection {
            return false;
        }
        self.selection = new_selection;
        true
    }

    fn get_layout_index(&self, node_id: VcsLogGraphNodeId) -> i32 {
        if node_id < 0 {
            node_id
        } else {
            self.permanent_graph_info
                .permanent_graph_layout()
                .get_layout_index(node_id)
        }
    }

    fn node_color(&self, node_id: VcsLogGraphNodeId) -> i32 {
        self.color_getter
            .get_node_color(node_id, self.get_layout_index(node_id))
    }
}

impl<CommitId> PrintElementPresentationManager for PrintElementPresentationManagerImpl<CommitId> {
    fn is_selected(&self, print_element: &GraphPrintElement) -> bool {
        self.selection
            .is_selected(print_element, &*self.linear_graph)
    }

    /// Nodes use their own color; a normal edge uses the color of its end with the larger layout
    /// index (the up end on ties); a special edge uses the color of its node.
    fn get_color_id(&self, element: &GraphElement) -> i32 {
        let edge = match element {
            GraphElement::Node(node) => {
                return self.node_color(self.linear_graph.get_node_id(node.node_index));
            }
            GraphElement::Edge(edge) => edge,
        };
        let Some(normal_edge) = as_normal_edge(edge) else {
            return self.node_color(self.linear_graph.get_node_id(get_not_null_node_index(edge)));
        };
        let up_node_id = self.linear_graph.get_node_id(normal_edge.up);
        let down_node_id = self.linear_graph.get_node_id(normal_edge.down);
        let up_layout_index = self.get_layout_index(up_node_id);
        let down_layout_index = self.get_layout_index(down_node_id);
        if up_layout_index >= down_layout_index {
            self.color_getter
                .get_node_color(up_node_id, up_layout_index)
        } else {
            self.color_getter
                .get_node_color(down_node_id, down_layout_index)
        }
    }
}
```

`crates/ig-log/src/facade/permanent_graph_impl.rs`:

```rust
//! The permanent graph: built once from the topologically sorted commit list.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/PermanentGraphImpl.kt`.
//!
//! Deviations:
//! - Only `Options::Base(SortType::Normal)` without branch or text filters is supported; other
//!   options fail with [`UnsupportedGraphOptions`] (Plan 2b ports BEK, linear BEK, first-parent,
//!   `CollapsedController` and `FilteredController`).
//! - The unfiltered `Normal` graph uses [`BaseController`] directly. Upstream wraps it in a
//!   `CollapsedController` with every node visible, whose compiled graph is identical; only the
//!   collapse/expand and fragment hover actions are missing (see `VisibleGraphImpl`).
//! - Branch node ids are passed to the layout builder sorted ascending (upstream: hash-set order,
//!   which only matters for heads that `head_commits_comparator` considers equal).
//! - The `Int`-specialised commit and condition classes are not needed.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;

use crate::api::GraphLayout;
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::{PermanentCommitsInfo, PermanentGraphInfo, VcsLogGraphNodeId};
use crate::api::printer::{GraphColorGetter, GraphColorGetterFactory};
use crate::facade::base_controller::BaseController;
use crate::facade::linear_graph_controller::LinearGraphController;
use crate::facade::reachable_nodes::ReachableNodes;
use crate::facade::visible_graph_impl::VisibleGraphImpl;
use crate::graph_commit::{GraphCommit, GraphCommitImpl};
use crate::permanent::graph_layout_builder;
use crate::permanent::graph_layout_impl::GraphLayoutImpl;
use crate::permanent::permanent_commits_info_impl::PermanentCommitsInfoImpl;
use crate::permanent::permanent_linear_graph_builder::PermanentLinearGraphBuilder;
use crate::permanent::permanent_linear_graph_impl::PermanentLinearGraphImpl;
use crate::permanent_graph::{
    AllCommits, ContainedInBranchCondition, Options, PermanentGraph, SortType,
    UnsupportedGraphOptions,
};
use crate::utils::linear_graph_utils::{
    LiteLinearGraphAdapter, as_lite_linear_graph, get_up_nodes,
};

/// Bounds every commit id type must satisfy.
pub trait CommitIdBounds: Clone + Eq + Hash + Send + Sync + 'static {}
impl<T: Clone + Eq + Hash + Send + Sync + 'static> CommitIdBounds for T {}

/// The parts shared with visible graphs and color getters (upstream: `this` as `PermanentGraphInfo`).
struct PermanentGraphParts<CommitId> {
    permanent_linear_graph: Arc<PermanentLinearGraphImpl>,
    permanent_graph_layout: GraphLayoutImpl,
    permanent_commits_info: PermanentCommitsInfoImpl<CommitId>,
    branch_node_ids: HashSet<VcsLogGraphNodeId>,
}

impl<CommitId: CommitIdBounds> PermanentGraphInfo<CommitId> for PermanentGraphParts<CommitId> {
    fn permanent_commits_info(&self) -> &dyn PermanentCommitsInfo<CommitId> {
        &self.permanent_commits_info
    }

    fn linear_graph(&self) -> Arc<dyn LinearGraph> {
        self.permanent_linear_graph.clone()
    }

    fn permanent_graph_layout(&self) -> &dyn GraphLayout {
        &self.permanent_graph_layout
    }

    fn branch_node_ids(&self) -> &HashSet<VcsLogGraphNodeId> {
        &self.branch_node_ids
    }
}

/// Assigns ids `-2, -3, ...` to commits that are referenced as parents but not loaded.
struct NotLoadedCommitsIdsGenerator<CommitId> {
    not_loaded_commits: HashMap<i32, CommitId>,
}

impl<CommitId: Clone> NotLoadedCommitsIdsGenerator<CommitId> {
    fn apply(&mut self, dom: &CommitId) -> VcsLogGraphNodeId {
        let node_id = -(self.not_loaded_commits.len() as i32 + 2);
        self.not_loaded_commits.insert(node_id, dom.clone());
        node_id
    }
}

pub struct PermanentGraphImpl<CommitId> {
    parts: Arc<PermanentGraphParts<CommitId>>,
    graph_color_getter: Arc<dyn GraphColorGetter>,
    reachable_nodes: ReachableNodes<LiteLinearGraphAdapter<Arc<PermanentLinearGraphImpl>>>,
}

impl<CommitId: CommitIdBounds> PermanentGraphImpl<CommitId> {
    /// Creates the permanent graph.
    ///
    /// - `graph_commits`: topologically sorted commits.
    /// - `head_commits_comparator`: orders branch heads by importance (`Less` = more important).
    ///   More important branches are laid out further to the left, and their color is reused by
    ///   the subgraph below the point where the branches diverged.
    /// - `branches_commit_id`: commit ids of all branch heads.
    pub fn new_instance<G: GraphCommit<CommitId>>(
        graph_commits: &[G],
        color_getter_factory: &dyn GraphColorGetterFactory<CommitId>,
        mut head_commits_comparator: impl FnMut(&CommitId, &CommitId) -> Ordering,
        branches_commit_id: &HashSet<CommitId>,
    ) -> PermanentGraphImpl<CommitId> {
        let mut ids_generator = NotLoadedCommitsIdsGenerator {
            not_loaded_commits: HashMap::new(),
        };
        let linear_graph = PermanentLinearGraphBuilder::new_instance(graph_commits)
            .build_with(|commit| ids_generator.apply(commit));
        let permanent_commits_info =
            PermanentCommitsInfoImpl::new_instance(graph_commits, ids_generator.not_loaded_commits);
        let mut branch_indexes: Vec<i32> = permanent_commits_info
            .convert_to_node_ids_skipping(branches_commit_id, true)
            .into_iter()
            .collect();
        branch_indexes.sort_unstable();
        let permanent_graph_layout = graph_layout_builder::build_with_branches(
            &linear_graph,
            &branch_indexes,
            |node1, node2| {
                let commit_id1 = permanent_commits_info.get_commit_id(node1);
                let commit_id2 = permanent_commits_info.get_commit_id(node2);
                head_commits_comparator(&commit_id1, &commit_id2)
            },
        );

        let branch_node_ids = permanent_commits_info.convert_to_node_ids(branches_commit_id);
        let permanent_linear_graph = Arc::new(linear_graph);
        let parts = Arc::new(PermanentGraphParts {
            permanent_linear_graph: permanent_linear_graph.clone(),
            permanent_graph_layout,
            permanent_commits_info,
            branch_node_ids,
        });
        let graph_color_getter = color_getter_factory.create_color_getter(parts.clone());
        let reachable_nodes = ReachableNodes::new(as_lite_linear_graph(permanent_linear_graph));

        PermanentGraphImpl {
            parts,
            graph_color_getter,
            reachable_nodes,
        }
    }

    /// This graph as the `PermanentGraphInfo` handed to controllers and color getters.
    pub fn permanent_graph_info(&self) -> Arc<dyn PermanentGraphInfo<CommitId>> {
        self.parts.clone()
    }

    pub fn permanent_commits_info_impl(&self) -> &PermanentCommitsInfoImpl<CommitId> {
        &self.parts.permanent_commits_info
    }

    pub fn permanent_linear_graph(&self) -> &PermanentLinearGraphImpl {
        &self.parts.permanent_linear_graph
    }

    pub fn permanent_graph_layout_impl(&self) -> &GraphLayoutImpl {
        &self.parts.permanent_graph_layout
    }

    fn create_filtered_controller(
        &self,
        options: Options,
        visible_heads: Option<&HashSet<CommitId>>,
        matching_commits: Option<&HashSet<CommitId>>,
    ) -> Result<Box<dyn LinearGraphController>, UnsupportedGraphOptions> {
        match options {
            Options::Base(SortType::Normal) => {
                if matching_commits.is_some() {
                    return Err(UnsupportedGraphOptions(
                        "Filtering by matched commits (FilteredController)".to_string(),
                    ));
                }
                if visible_heads.is_some() {
                    return Err(UnsupportedGraphOptions(
                        "Filtering by visible branches (CollapsedController)".to_string(),
                    ));
                }
                Ok(Box::new(BaseController::new(self.permanent_graph_info())))
            }
            Options::Base(SortType::Bek) => Err(UnsupportedGraphOptions(
                "SortType.Bek (SortedBaseController)".to_string(),
            )),
            Options::FirstParent => Err(UnsupportedGraphOptions(
                "Options.FirstParent (FirstParentController)".to_string(),
            )),
            Options::LinearBek => Err(UnsupportedGraphOptions(
                "Options.LinearBek (LinearBekController)".to_string(),
            )),
        }
    }

    /// `createVisibleGraph` with a `preprocessor` that may modify the controller before the visible
    /// graph is built.
    pub fn create_visible_graph_with(
        &self,
        options: Options,
        visible_heads: Option<&HashSet<CommitId>>,
        matching_commits: Option<&HashSet<CommitId>>,
        preprocessor: impl FnOnce(&mut dyn LinearGraphController, &dyn PermanentGraphInfo<CommitId>),
    ) -> Result<VisibleGraphImpl<CommitId>, UnsupportedGraphOptions> {
        let mut controller =
            self.create_filtered_controller(options, visible_heads, matching_commits)?;
        preprocessor(controller.as_mut(), &*self.parts);
        Ok(VisibleGraphImpl::new(
            controller,
            self.permanent_graph_info(),
            self.graph_color_getter.clone(),
        ))
    }
}

impl<CommitId: CommitIdBounds> PermanentGraph<CommitId> for PermanentGraphImpl<CommitId> {
    type Visible = VisibleGraphImpl<CommitId>;

    fn create_visible_graph(
        &self,
        options: Options,
        heads_of_visible_branches: Option<&HashSet<CommitId>>,
        matched_commits: Option<&HashSet<CommitId>>,
    ) -> Result<VisibleGraphImpl<CommitId>, UnsupportedGraphOptions> {
        self.create_visible_graph_with(
            options,
            heads_of_visible_branches,
            matched_commits,
            |_, _| {},
        )
    }

    fn all_commits(&self) -> AllCommits<'_, CommitId> {
        let info = &self.parts.permanent_commits_info;
        let graph = &*self.parts.permanent_linear_graph;
        AllCommits::new(graph.nodes_count() as usize, move |index| {
            let index = index as i32;
            let mut parents = Vec::new();
            graph.for_each_parent_node(index, |parent_id| {
                parents.push(info.get_commit_id(parent_id))
            });
            GraphCommitImpl::create_commit(
                info.get_commit_id(index),
                parents,
                info.get_timestamp(index),
            )
        })
    }

    /// # Panics
    /// If `commit` is not a loaded commit (upstream throws too).
    fn get_children(&self, commit: &CommitId) -> Vec<CommitId> {
        let info = &self.parts.permanent_commits_info;
        let commit_index = info.get_node_id(commit);
        info.convert_to_commit_id_list(get_up_nodes(
            &*self.parts.permanent_linear_graph,
            commit_index,
        ))
    }

    fn get_containing_branches(&self, commit: &CommitId) -> HashSet<CommitId> {
        let info = &self.parts.permanent_commits_info;
        let node_id = info.get_node_id(commit);
        info.convert_to_commit_id_set(
            self.reachable_nodes
                .get_containing_branches(node_id, &self.parts.branch_node_ids),
        )
    }

    fn get_contained_in_branch_condition(
        &self,
        current_branch_head: &[CommitId],
    ) -> ContainedInBranchCondition<CommitId> {
        let info = &self.parts.permanent_commits_info;
        let head_ids: Vec<i32> = current_branch_head
            .iter()
            .map(|head| info.get_node_id(head))
            .collect();
        let mut branch_nodes = HashSet::new();
        self.reachable_nodes.walk_down(head_ids, |node| {
            branch_nodes.insert(info.get_commit_id(node));
        });
        ContainedInBranchCondition::new(branch_nodes)
    }
}
```

`crates/ig-log/src/facade/visible_graph_impl.rs`:

```rust
//! The visible graph: rows, print elements and actions over a controller's compiled graph.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/VisibleGraphImpl.kt`.
//!
//! Deviations:
//! - `VisibleGraphImpl` implements `ActionController` itself (upstream: an inner class).
//! - The graph updater of an answer is run with [`VisibleGraphImpl::run_graph_updater`], which then
//!   rebuilds the print element generator (upstream wraps the `Runnable`).
//! - `BUTTON_COLLAPSE` / `BUTTON_EXPAND` are reported unsupported until `CollapsedController`
//!   is ported (Plan 2b); upstream supports them on every non-filtered graph.
//! - `buildSimpleGraphInfo` (`SimpleGraphInfo`) is not ported yet.

use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use crate::actions::{ActionController, GraphAction, GraphActionType, GraphAnswer, GraphUpdater};
use crate::api::elements::{GraphEdgeType, GraphElement, GraphNodeType};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::{PermanentGraphInfo, VcsLogGraphNodeId};
use crate::api::printer::{GraphColorGetter, GraphPrintElement, PrintElementGenerator};
use crate::facade::linear_graph_controller::{LinearGraphAnswer, LinearGraphController};
use crate::facade::print_element_presentation_manager_impl::PrintElementPresentationManagerImpl;
use crate::print::graph_element_comparator_by_layout_index::GraphElementComparatorByLayoutIndex;
use crate::print::print_element_generator_impl::PrintElementGeneratorImpl;
use crate::print_element::{EdgePrintElement, EdgePrintElementType};
use crate::utils::linear_graph_utils::{get_cursor, get_down_nodes, get_up_nodes};
use crate::visible_graph::{RowInfo, RowType, VcsLogVisibleGraphIndex, VisibleGraph};

pub struct VisibleGraphImpl<CommitId> {
    graph_controller: Box<dyn LinearGraphController>,
    permanent_graph: Arc<dyn PermanentGraphInfo<CommitId>>,
    color_generator: Arc<dyn GraphColorGetter>,
    print_element_generator:
        PrintElementGeneratorImpl<PrintElementPresentationManagerImpl<CommitId>>,
    is_show_long_edges: bool,
}

impl<CommitId: 'static> VisibleGraphImpl<CommitId> {
    pub fn new(
        graph_controller: Box<dyn LinearGraphController>,
        permanent_graph: Arc<dyn PermanentGraphInfo<CommitId>>,
        color_generator: Arc<dyn GraphColorGetter>,
    ) -> VisibleGraphImpl<CommitId> {
        let print_element_generator = Self::create_print_element_generator(
            &*graph_controller,
            &permanent_graph,
            &color_generator,
            false,
        );
        VisibleGraphImpl {
            graph_controller,
            permanent_graph,
            color_generator,
            print_element_generator,
            is_show_long_edges: false,
        }
    }

    fn create_print_element_generator(
        graph_controller: &dyn LinearGraphController,
        permanent_graph: &Arc<dyn PermanentGraphInfo<CommitId>>,
        color_generator: &Arc<dyn GraphColorGetter>,
        is_show_long_edges: bool,
    ) -> PrintElementGeneratorImpl<PrintElementPresentationManagerImpl<CommitId>> {
        let linear_graph = graph_controller.compiled_graph();
        let presentation_manager = PrintElementPresentationManagerImpl::new(
            permanent_graph.clone(),
            linear_graph.clone(),
            color_generator.clone(),
        );
        let layout_graph = linear_graph.clone();
        let layout_info = permanent_graph.clone();
        let comparator = GraphElementComparatorByLayoutIndex::new(move |node_index| {
            let node_id = layout_graph.get_node_id(node_index);
            if node_id < 0 {
                return node_id;
            }
            layout_info
                .permanent_graph_layout()
                .get_layout_index(node_id)
        });
        PrintElementGeneratorImpl::new(
            linear_graph,
            presentation_manager,
            is_show_long_edges,
            move |a: &GraphElement, b: &GraphElement| comparator.ordering(a, b),
        )
    }

    /// `updatePrintElementGenerator`: rebuilds generator and presentation manager (the selection
    /// is reset).
    pub fn update_print_element_generator(&mut self) {
        self.print_element_generator = Self::create_print_element_generator(
            &*self.graph_controller,
            &self.permanent_graph,
            &self.color_generator,
            self.is_show_long_edges,
        );
    }

    /// Runs the updater from a [`GraphAnswer`], then rebuilds the print element generator.
    pub fn run_graph_updater(&mut self, updater: GraphUpdater) {
        updater.run();
        self.update_print_element_generator();
    }

    /// The controller's compiled graph (`linearGraph`).
    pub fn linear_graph(&self) -> Arc<dyn LinearGraph> {
        self.graph_controller.compiled_graph()
    }

    pub fn permanent_graph(&self) -> &Arc<dyn PermanentGraphInfo<CommitId>> {
        &self.permanent_graph
    }

    fn convert_to_node_id(
        &self,
        node_index: Option<VcsLogVisibleGraphIndex>,
    ) -> Option<VcsLogGraphNodeId> {
        node_index.map(|index| self.graph_controller.compiled_graph().get_node_id(index))
    }

    fn perform_arrow_action(&mut self, action: &GraphAction) -> Option<GraphAnswer<CommitId>> {
        let Some(GraphPrintElement::Edge(edge_print_element)) = &action.affected_element else {
            return None;
        };
        if !edge_print_element.has_arrow() {
            return None;
        }

        let edge = edge_print_element.graph_edge();

        let mut target_id = None;
        if edge.edge_type == GraphEdgeType::NotLoadCommit {
            debug_assert!(edge_print_element.edge_print_type() == EdgePrintElementType::Down);
            target_id = edge.target_id;
        }
        if edge.edge_type.is_normal_edge() {
            target_id = if edge_print_element.edge_print_type() == EdgePrintElementType::Down {
                self.convert_to_node_id(edge.down_node_index)
            } else {
                self.convert_to_node_id(edge.up_node_index)
            };
        }
        let target_id = target_id?;

        let commit_to_jump = || {
            self.permanent_graph
                .permanent_commits_info()
                .get_commit_id(target_id)
        };
        match action.action_type {
            GraphActionType::MouseOver => {
                let commit = commit_to_jump();
                let selection_changed = self
                    .print_element_generator
                    .presentation_manager_mut()
                    .set_selected_element(GraphPrintElement::Edge(edge_print_element.clone()));
                Some(GraphAnswer {
                    cursor_to_set: Some(get_cursor(true)),
                    commit_to_jump: Some(commit),
                    graph_updater: None,
                    do_jump: false,
                    is_repaint_required: selection_changed,
                })
            }
            GraphActionType::MouseClick => {
                let commit = commit_to_jump();
                let selection_changed = self
                    .print_element_generator
                    .presentation_manager_mut()
                    .set_selected_elements(HashSet::new());
                Some(GraphAnswer {
                    cursor_to_set: Some(get_cursor(false)),
                    commit_to_jump: Some(commit),
                    graph_updater: None,
                    do_jump: true,
                    is_repaint_required: selection_changed,
                })
            }
            _ => None,
        }
    }

    fn convert_answer(answer: LinearGraphAnswer, selection_changed: bool) -> GraphAnswer<CommitId> {
        GraphAnswer {
            cursor_to_set: answer.cursor_to_set,
            commit_to_jump: None,
            graph_updater: answer.graph_updater,
            do_jump: false,
            is_repaint_required: selection_changed,
        }
    }
}

impl<CommitId: 'static> ActionController<CommitId> for VisibleGraphImpl<CommitId> {
    fn perform_action(&mut self, graph_action: GraphAction) -> GraphAnswer<CommitId> {
        if let Some(graph_answer) = self.perform_arrow_action(&graph_action) {
            return graph_answer;
        }

        let answer = self
            .graph_controller
            .perform_linear_graph_action(&graph_action);
        let selected = answer.selected_node_ids.clone().unwrap_or_default();
        let selection_changed = self
            .print_element_generator
            .presentation_manager_mut()
            .set_selected_elements(selected);
        if answer.graph_changes.is_some() {
            self.update_print_element_generator();
        }
        Self::convert_answer(answer, selection_changed)
    }

    fn are_long_edges_hidden(&self) -> bool {
        !self.is_show_long_edges
    }

    fn set_long_edges_hidden(&mut self, long_edges_hidden: bool) {
        self.is_show_long_edges = !long_edges_hidden;
        self.update_print_element_generator();
    }

    fn is_action_supported(&self, action: &GraphAction) -> bool {
        !matches!(
            action.action_type,
            GraphActionType::ButtonCollapse | GraphActionType::ButtonExpand
        )
    }
}

struct RowInfoImpl<'a, CommitId> {
    visible_graph: &'a VisibleGraphImpl<CommitId>,
    node_id: VcsLogGraphNodeId,
    visible_row: VcsLogVisibleGraphIndex,
}

impl<CommitId: 'static> RowInfo<CommitId> for RowInfoImpl<'_, CommitId> {
    fn commit(&self) -> CommitId {
        self.visible_graph
            .permanent_graph
            .permanent_commits_info()
            .get_commit_id(self.node_id)
    }

    fn one_of_heads(&self) -> CommitId {
        let permanent_graph = &self.visible_graph.permanent_graph;
        let head_node_id = permanent_graph
            .permanent_graph_layout()
            .get_one_of_head_node_index(self.node_id);
        permanent_graph
            .permanent_commits_info()
            .get_commit_id(head_node_id)
    }

    fn print_elements(&self) -> Vec<GraphPrintElement> {
        self.visible_graph
            .print_element_generator
            .get_print_elements(self.visible_row)
    }

    /// # Panics
    /// For a not-loaded node (upstream throws `UnsupportedOperationException`).
    fn row_type(&self) -> RowType {
        let node_type = self
            .visible_graph
            .linear_graph()
            .get_graph_node(self.visible_row)
            .node_type;
        match node_type {
            GraphNodeType::Usual => RowType::Normal,
            GraphNodeType::Unmatched => RowType::Unmatched,
            GraphNodeType::NotLoadCommit => panic!("Unsupported node type: {node_type:?}"),
        }
    }

    fn adjacent_rows(&self, parent: bool) -> Vec<VcsLogVisibleGraphIndex> {
        let graph = self.visible_graph.linear_graph();
        if parent {
            get_down_nodes(&*graph, self.visible_row)
        } else {
            get_up_nodes(&*graph, self.visible_row)
        }
    }
}

impl<CommitId: 'static> VisibleGraph<CommitId> for VisibleGraphImpl<CommitId> {
    fn visible_commit_count(&self) -> i32 {
        self.graph_controller.compiled_graph().nodes_count()
    }

    fn recommended_width(&self) -> i32 {
        self.print_element_generator.recommended_width()
    }

    fn action_controller(&mut self) -> &mut dyn ActionController<CommitId> {
        self
    }

    fn get_row_info(
        &self,
        visible_row: VcsLogVisibleGraphIndex,
    ) -> Box<dyn RowInfo<CommitId> + '_> {
        let node_id = self
            .graph_controller
            .compiled_graph()
            .get_node_id(visible_row);
        debug_assert!(node_id >= 0); // todo remake for all id
        Box::new(RowInfoImpl {
            visible_graph: self,
            node_id,
            visible_row,
        })
    }

    fn get_visible_row_index(&self, commit_id: &CommitId) -> Option<VcsLogVisibleGraphIndex> {
        let node_id = self
            .permanent_graph
            .permanent_commits_info()
            .get_node_id(commit_id);
        self.graph_controller
            .compiled_graph()
            .get_node_index(node_id)
    }
}

/// Like upstream `toString`: `VisibleGraph[commit, commit, ...]`.
impl<CommitId: fmt::Debug + 'static> fmt::Debug for VisibleGraphImpl<CommitId> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let commits: Vec<String> = (0..self.visible_commit_count())
            .map(|i| format!("{:?}", self.get_row_info(i).commit()))
            .collect();
        write!(f, "VisibleGraph[{}]", commits.join(", "))
    }
}
```

- [ ] **Step 10: Append the module map.** Append this to `reference/upstream-map.toml`, after the last `ig-git` entry:

```toml
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
deviations = ["IdeaGit helper replacing java.util.TimSort / fastutil stable sort: exact TimSort small-array path below 32 elements, stable merge sort above (identical for consistent comparators); never panics on inconsistent comparators"]

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
  "Not-loaded parents with the same first child are processed in first-seen order (upstream: HashMap order)",
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
]

[modules."ig-log::print::edges_in_row_generator"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/EdgesInRowGenerator.java"]
tests = [
  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/EdgesInRowTest.java",
  "platform/vcs-log/graph/testData/edgesInRow",
]
deviations = [
  "BTreeSet instead of HashSet (deterministic order)",
  "BoundedCache instead of SLRUMap",
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
]

[modules."ig-log::facade::visible_graph_impl"]
pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/VisibleGraphImpl.kt"]
deviations = [
  "Implements ActionController itself (upstream inner class)",
  "BUTTON_COLLAPSE/BUTTON_EXPAND reported unsupported until CollapsedController (Plan 2b)",
  "buildSimpleGraphInfo / SimpleGraphInfo not ported",
  "Graph updater run via run_graph_updater",
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
```

- [ ] **Step 11: Build, lint and run the unit tests**

```bash
cargo build -p ig-log
cargo clippy -q --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ig-log
cargo test -p ig-log --lib
```
Expected: no errors or warnings, and `test result: ok. 3 passed` (the `java_sort` unit tests).

- [ ] **Step 12: Commit**

```bash
git add Cargo.toml Cargo.lock .gitattributes reference crates/ig-log
git commit -m "feat(ig-log): commit graph core ported from IntelliJ vcs-log graph"
```

---
### Task 2: Test infrastructure: text graph parser/printer and `TestGraphBuilder`

**Files:**
- Create: `crates/ig-log/tests/common/{mod,parser,str_utils,test_graph_builder}.rs`, `crates/ig-log/tests/test_graph_builder.rs`, `crates/ig-log/tests/fixtures/testGraphBuilder/**`, `crates/ig-log/tests/fixtures/testParser/**`

Ports `TestGraphBuilderTest` (graph and `impl.print` variants). **Review focus:** the test infrastructure in `tests/common/` and the graph element types in `src/api/elements.rs`.

- [ ] **Step 1: Copy the upstream test data, byte-identical**

```bash
mkdir -p crates/ig-log/tests/fixtures
cp -R "$IJ/platform/vcs-log/graph/testData/testGraphBuilder" crates/ig-log/tests/fixtures/
cp -R "$IJ/platform/vcs-log/graph/testData/testParser" crates/ig-log/tests/fixtures/
diff -r "$IJ/platform/vcs-log/graph/testData/testGraphBuilder" crates/ig-log/tests/fixtures/testGraphBuilder && echo "testGraphBuilder identical"
diff -r "$IJ/platform/vcs-log/graph/testData/testParser" crates/ig-log/tests/fixtures/testParser && echo "testParser identical"
```

- [ ] **Step 2: Create `crates/ig-log/tests/common/mod.rs`**

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

- [ ] **Step 3: Create `crates/ig-log/tests/common/parser.rs`**

```rust
//! Text formats of the upstream graph tests.
//!
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/`
//! (`CommitParser.java`, `SimpleCommit.java`, `EdgeNodeCharConverter.java`, `LinearGraphParser.java`).

use std::collections::HashMap;

use ig_log::api::EdgeFilter;
use ig_log::api::elements::{GraphEdge, GraphEdgeType, GraphNode, GraphNodeType};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::graph_commit::GraphCommit;

pub const SEPARATOR: &str = "|-";

/// A commit read from a test file (`SimpleCommit`).
#[derive(Clone, Debug)]
pub struct SimpleCommit<Id> {
    pub id: Id,
    pub parents: Vec<Id>,
    pub timestamp: i64,
}

impl<Id> GraphCommit<Id> for SimpleCommit<Id> {
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

impl SimpleCommit<i32> {
    pub fn as_integer_commit(commit_hash: &str, parents_hashes: &[&str]) -> SimpleCommit<i32> {
        let int_commit_hash = create_hash(commit_hash);
        let parents = parents_hashes
            .iter()
            .filter(|hash| !hash.is_empty())
            .map(|hash| create_hash(hash))
            .collect();
        SimpleCommit {
            id: int_commit_hash,
            parents,
            timestamp: i64::from(int_commit_hash),
        }
    }
}

impl SimpleCommit<String> {
    pub fn as_string_commit(commit_hash: &str, parents_hashes: &[&str]) -> SimpleCommit<String> {
        let timestamp = create_hash(commit_hash);
        let parents = parents_hashes
            .iter()
            .filter(|hash| !hash.is_empty())
            .map(|hash| hash.to_string())
            .collect();
        SimpleCommit {
            id: commit_hash.to_string(),
            parents,
            timestamp: i64::from(timestamp),
        }
    }
}

pub fn next_separator_index(line: &str, start_index: usize) -> usize {
    match line[start_index..].find(SEPARATOR) {
        Some(index) => start_index + index,
        None => panic!(
            "not found separator \"{SEPARATOR}\", with startIndex={start_index}, in line: {line}"
        ),
    }
}

/// `ab123|-adada 193 352` -> (`ab123`, [`adada`, `193`, `352`]); `123|-` has no parents.
pub fn parse_commit_parents(line: &str) -> (&str, Vec<&str>) {
    let separator_index = next_separator_index(line, 0);
    let commit_hash_str = &line[..separator_index];
    let parent_hash_str = &line[separator_index + 2..];
    (commit_hash_str, split_whitespace_java(parent_hash_str))
}

/// Java `String.split("\\s")`: empty strings between consecutive separators are kept, trailing
/// empty strings dropped.
pub fn split_whitespace_java(s: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = s.split([' ', '\t', '\n', '\x0B', '\x0C', '\r']).collect();
    while parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    parts
}

pub fn create_hash(s: &str) -> i32 {
    i32::from_str_radix(s, 16).unwrap_or_else(|e| panic!("bad hash {s:?}: {e}"))
}

/// Non-empty lines.
pub fn to_lines(input: &str) -> Vec<&str> {
    input.split('\n').filter(|line| !line.is_empty()).collect()
}

pub fn parse_string_commit_list(input: &str) -> Vec<SimpleCommit<String>> {
    to_lines(input)
        .into_iter()
        .map(|line| {
            let (hash, parents) = parse_commit_parents(line);
            SimpleCommit::as_string_commit(hash, &parents)
        })
        .collect()
}

pub fn parse_integer_commit_list(input: &str) -> Vec<SimpleCommit<i32>> {
    to_lines(input)
        .into_iter()
        .map(|line| {
            let (hash, parents) = parse_commit_parents(line);
            SimpleCommit::as_integer_commit(hash, &parents)
        })
        .collect()
}

// EdgeNodeCharConverter

pub fn parse_graph_node_type(c: char) -> GraphNodeType {
    match c {
        'U' => GraphNodeType::Usual,
        'G' => GraphNodeType::Unmatched,
        'N' => GraphNodeType::NotLoadCommit,
        _ => panic!("Illegal char for graph node type: {c}"),
    }
}

pub fn parse_graph_edge_type(c: char) -> GraphEdgeType {
    match c {
        'U' => GraphEdgeType::Usual,
        'D' => GraphEdgeType::Dotted,
        'N' => GraphEdgeType::NotLoadCommit,
        'P' => GraphEdgeType::DottedArrowUp,
        'O' => GraphEdgeType::DottedArrowDown,
        _ => panic!("Illegal char for graph edge type: {c}"),
    }
}

pub fn edge_type_to_char(edge_type: GraphEdgeType) -> char {
    match edge_type {
        GraphEdgeType::Usual => 'U',
        GraphEdgeType::Dotted => 'D',
        GraphEdgeType::NotLoadCommit => 'N',
        GraphEdgeType::DottedArrowUp => 'P',
        GraphEdgeType::DottedArrowDown => 'O',
    }
}

pub fn node_type_to_char(node_type: GraphNodeType) -> char {
    match node_type {
        GraphNodeType::Usual => 'U',
        GraphNodeType::Unmatched => 'G',
        GraphNodeType::NotLoadCommit => 'N',
    }
}

// LinearGraphParser

/// Java `Integer.decode`: optional sign, then `0x`/`0X`/`#` hex, leading-zero octal, or decimal.
fn java_integer_decode(s: &str) -> i32 {
    let (negative, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (radix, digits) = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
        .or_else(|| digits.strip_prefix('#'))
    {
        (16, hex)
    } else if digits.len() > 1 && digits.starts_with('0') {
        (8, &digits[1..])
    } else {
        (10, digits)
    };
    let value =
        i64::from_str_radix(digits, radix).unwrap_or_else(|e| panic!("bad number {s:?}: {e}"));
    (if negative { -value } else { value }) as i32
}

fn parse_number_with_char(input: &str) -> (i32, char) {
    let node_id = java_integer_decode(&input[..input.len() - 2]);
    (node_id, input.chars().last().expect("empty element"))
}

struct ParsedLine {
    node_id: i32,
    graph_node: GraphNode,
    normal_edges: Vec<String>,
}

/// Example input line: `0_U|-1_U 2_D`.
fn parse_line(line: &str, line_number: i32) -> ParsedLine {
    let separator_index = next_separator_index(line, 0);
    let (node_id, node_char) = parse_number_with_char(&line[..separator_index]);
    let graph_node = GraphNode::with_type(line_number, parse_graph_node_type(node_char));
    let normal_edges = split_whitespace_java(&line[separator_index + 2..])
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    ParsedLine {
        node_id,
        graph_node,
        normal_edges,
    }
}

/// `LinearGraphParser.parse`.
pub fn parse_linear_graph(input: &str) -> TestLinearGraphWithElementsInfo {
    let mut graph_nodes = Vec::new();
    let mut edges: Vec<Vec<String>> = Vec::new();
    let mut node_id_to_node_index = HashMap::new();

    for line in to_lines(input) {
        let parsed = parse_line(line, graph_nodes.len() as i32);
        edges.push(parsed.normal_edges);
        node_id_to_node_index.insert(parsed.node_id, graph_nodes.len() as i32);
        graph_nodes.push(parsed.graph_node);
    }

    let mut up_edges: HashMap<i32, Vec<GraphEdge>> = HashMap::new();
    let mut down_edges: HashMap<i32, Vec<GraphEdge>> = HashMap::new();
    for (graph_node, node_edges) in graph_nodes.iter().zip(&edges) {
        for str_edge in node_edges {
            let (node_id, edge_char) = parse_number_with_char(str_edge);
            let edge_type = parse_graph_edge_type(edge_char);

            let edge = match edge_type {
                GraphEdgeType::Usual | GraphEdgeType::Dotted => {
                    let down_node_index = *node_id_to_node_index
                        .get(&node_id)
                        .unwrap_or_else(|| panic!("unknown node {node_id}"));
                    GraphEdge::create_normal_edge(graph_node.node_index, down_node_index, edge_type)
                }
                GraphEdgeType::NotLoadCommit
                | GraphEdgeType::DottedArrowDown
                | GraphEdgeType::DottedArrowUp => GraphEdge::create_edge_with_target_id(
                    graph_node.node_index,
                    Some(node_id),
                    edge_type,
                ),
            };
            if let Some(up) = edge.up_node_index {
                down_edges.entry(up).or_default().push(edge);
            }
            if let Some(down) = edge.down_node_index {
                up_edges.entry(down).or_default().push(edge);
            }
        }
    }

    TestLinearGraphWithElementsInfo {
        graph_nodes,
        up_edges,
        down_edges,
    }
}

pub struct TestLinearGraphWithElementsInfo {
    graph_nodes: Vec<GraphNode>,
    up_edges: HashMap<i32, Vec<GraphEdge>>,
    down_edges: HashMap<i32, Vec<GraphEdge>>,
}

impl LinearGraph for TestLinearGraphWithElementsInfo {
    fn nodes_count(&self) -> i32 {
        self.graph_nodes.len() as i32
    }

    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        let mut result = Vec::new();

        for up_edge in self.up_edges.get(&node_index).into_iter().flatten() {
            if up_edge.edge_type.is_normal_edge() && filter.up_normal() {
                result.push(*up_edge);
            }
            if !up_edge.edge_type.is_normal_edge() && filter.special() {
                result.push(*up_edge);
            }
        }

        for down_edge in self.down_edges.get(&node_index).into_iter().flatten() {
            if down_edge.edge_type.is_normal_edge() && filter.down_normal() {
                result.push(*down_edge);
            }
            if !down_edge.edge_type.is_normal_edge() && filter.special() {
                result.push(*down_edge);
            }
        }

        result
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        self.graph_nodes[node_index as usize]
    }

    fn get_node_id(&self, node_index: i32) -> i32 {
        // upstream quirk kept: `nodeIndex > 0` rejects node 0 (only checked with assertions on)
        debug_assert!(
            node_index > 0 && node_index < self.nodes_count(),
            "Bad nodeIndex: {node_index}"
        );
        node_index
    }

    fn get_node_index(&self, node_id: i32) -> Option<i32> {
        (node_id >= 0 && node_id < self.nodes_count()).then_some(node_id)
    }
}
```

- [ ] **Step 4: Create `crates/ig-log/tests/common/str_utils.rs`**

```rust
//! String renderings of graphs, layouts and print elements used as expected test output.
//!
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/GraphStrUtils.java` and
//! `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/StrUtils.kt`.

use std::collections::{BTreeSet, HashSet};
use std::fmt::Display;

use ig_log::api::EdgeFilter;
use ig_log::api::GraphLayout;
use ig_log::api::elements::{GraphEdge, GraphElement, GraphNode};
use ig_log::api::linear_graph::{LinearGraph, LiteLinearGraph};
use ig_log::api::permanent::PermanentCommitsInfo;
use ig_log::api::printer::{GraphPrintElement, PrintElementGenerator};
use ig_log::facade::reachable_nodes::ReachableNodes;
use ig_log::print::edges_in_row_generator::EdgesInRowGenerator;
use ig_log::print::graph_element_comparator_by_layout_index::GraphElementComparatorByLayoutIndex;
use ig_log::print_element::{EdgePrintElement, PrintElement};
use ig_log::utils::java_sort;

use super::parser::{SEPARATOR, edge_type_to_char, node_type_to_char};

/// `GRAPH_ELEMENT_COMPARATOR`: by layout index, with every layout index 0.
pub fn graph_element_comparator() -> GraphElementComparatorByLayoutIndex {
    GraphElementComparatorByLayoutIndex::new(|_| 0)
}

pub fn commits_info_to_str<CommitId>(
    commits_info: &dyn PermanentCommitsInfo<CommitId>,
    size: usize,
    to_str: impl Fn(&CommitId) -> String,
) -> String {
    let mut s = String::new();
    for i in 0..size as i32 {
        if i != 0 {
            s.push('\n');
        }
        let commit_id = commits_info.get_commit_id(i);
        let commit_index = commits_info.get_node_id(&commit_id);
        let timestamp = commits_info.get_timestamp(i);
        s.push_str(&format!(
            "{commit_index}{SEPARATOR}{}{SEPARATOR}{timestamp}",
            to_str(&commit_id)
        ));
    }
    s
}

pub fn permanent_graph_layout_model_to_str(
    graph_layout: &dyn GraphLayout,
    nodes_count: i32,
) -> String {
    (0..nodes_count)
        .map(|node_index| {
            format!(
                "{}{SEPARATOR}{}",
                graph_layout.get_layout_index(node_index),
                graph_layout.get_one_of_head_node_index(node_index)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn containing_branches_getter_to_str<G: LiteLinearGraph>(
    reachable_nodes: &ReachableNodes<G>,
    branches: &HashSet<i32>,
    nodes_count: i32,
) -> String {
    (0..nodes_count)
        .map(|node_index| {
            let mut branch_node_indexes: Vec<i32> = reachable_nodes
                .get_containing_branches(node_index, branches)
                .into_iter()
                .collect();
            if branch_node_indexes.is_empty() {
                return "none".to_string();
            }
            branch_node_indexes.sort();
            branch_node_indexes
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn edges_in_row_to_str(
    edges_in_row_generator: &EdgesInRowGenerator,
    nodes_count: i32,
) -> String {
    (0..nodes_count)
        .map(|i| edges_to_str(&edges_in_row_generator.get_edges_in_row(i)))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Java's `"" + Integer` for a nullable integer.
fn java_int(value: Option<i32>) -> String {
    value.map_or_else(|| "null".to_string(), |v| v.to_string())
}

pub fn edges_to_str(edges: &BTreeSet<GraphEdge>) -> String {
    if edges.is_empty() {
        return "none".to_string();
    }
    let comparator = graph_element_comparator();
    let mut sorted_edges: Vec<GraphElement> =
        edges.iter().map(|e| GraphElement::Edge(*e)).collect();
    java_sort::sort_by(&mut sorted_edges, |a, b| comparator.ordering(a, b));
    sorted_edges
        .iter()
        .map(|element| {
            let GraphElement::Edge(edge) = element else {
                unreachable!()
            };
            format!(
                "{}_{}_{}",
                java_int(edge.up_node_index),
                java_int(edge.down_node_index),
                edge_type_to_char(edge.edge_type)
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// StrUtils.kt

/// `Int?.asString()`.
fn opt_as_string(value: Option<i32>) -> String {
    value.map_or_else(|| "n".to_string(), |v| v.to_string())
}

pub fn node_as_string(node: &GraphNode) -> String {
    format!("{}_{}", node.node_index, node_type_to_char(node.node_type))
}

pub fn edge_as_string(edge: &GraphEdge) -> String {
    format!(
        "{}:{}:{}_{}",
        opt_as_string(edge.up_node_index),
        opt_as_string(edge.down_node_index),
        opt_as_string(edge.target_id),
        edge_type_to_char(edge.edge_type)
    )
}

pub fn element_as_string(element: &GraphElement) -> String {
    match element {
        GraphElement::Node(node) => node_as_string(node),
        GraphElement::Edge(edge) => edge_as_string(edge),
    }
}

/// `LinearGraph.asString(sorted)`.
pub fn graph_as_string(graph: &(impl LinearGraph + ?Sized), sorted: bool) -> String {
    let mut s = String::new();
    for node_index in 0..graph.nodes_count() {
        if node_index > 0 {
            s.push('\n');
        }
        let node = graph.get_graph_node(node_index);
        s.push_str(&node_as_string(&node));
        s.push_str(SEPARATOR);

        let mut adj_edges: Vec<GraphElement> = graph
            .get_adjacent_edges(node_index, EdgeFilter::All)
            .into_iter()
            .map(GraphElement::Edge)
            .collect();
        if sorted {
            let comparator = graph_element_comparator();
            java_sort::sort_by(&mut adj_edges, |a, b| comparator.ordering(a, b));
        }
        s.push_str(&join(adj_edges.iter().map(element_as_string), " "));
    }
    s
}

fn join(items: impl Iterator<Item = impl Display>, separator: &str) -> String {
    items
        .map(|item| item.to_string())
        .collect::<Vec<_>>()
        .join(separator)
}

/// `GraphPrintElement.asString()`.
pub fn print_element_as_string(print_element: &GraphPrintElement) -> String {
    let element = element_as_string(&print_element.graph_element());
    let row = print_element.row_index();
    let color = print_element.color_id();
    let pos = print_element.position_in_current_row();
    let sel = if print_element.is_selected() {
        "Select"
    } else {
        "Unselect"
    };
    match print_element {
        GraphPrintElement::Node(_) => format!("Node|-{row}:{pos}|-{color}:{sel}({element})"),
        GraphPrintElement::Edge(edge) => {
            let t = edge.edge_print_type().name();
            let ls = edge.line_style().name();
            let pos_o = edge.position_in_other_row();
            let arrow = if edge.has_arrow() { "_ARROW" } else { "" };
            format!("Edge:{t}{arrow}:{ls}|-{row}:{pos}:{pos_o}|-{color}:{sel}({element})")
        }
    }
}

/// `PrintElementGenerator.asString(size)`.
pub fn print_element_generator_as_string(
    generator: &dyn PrintElementGenerator,
    size: i32,
) -> String {
    let mut s = String::new();
    for row in 0..size {
        if row > 0 {
            s.push('\n');
        }
        let mut elements = generator.get_print_elements(row);
        // stable, like Kotlin `sortedBy`
        elements.sort_by_key(|element| {
            let pos = element.position_in_current_row();
            match element {
                GraphPrintElement::Node(_) => 1024 * pos,
                GraphPrintElement::Edge(edge) => {
                    1024 * pos
                        + (edge.edge_print_type().ordinal() + 1) * 64
                        + edge.position_in_other_row()
                }
            }
        });
        s.push_str(&join(elements.iter().map(print_element_as_string), "\n  "));
    }
    s
}
```

- [ ] **Step 5: Create `crates/ig-log/tests/common/test_graph_builder.rs`**

```rust
//! A small DSL for building linear graphs in tests.
//!
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/TestGraphBuilder.kt`
//! (`BaseTestGraphBuilder`, `TestGraphBuilder`, `graph { }`). Kotlin's `1(2, 3.dot)` becomes
//! `b.node(1, [u(2), dot(3)])`; `2.UNM(3)` becomes `b.node(unm(2), [u(3)])`.
//! `asTestGraphString` and `TestPermanentGraphInfo` are not needed by the ported tests yet.

use std::collections::HashMap;

use ig_log::api::EdgeFilter;
use ig_log::api::elements::{GraphEdge, GraphEdgeType, GraphNode, GraphNodeType};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::utils::linear_graph_utils::{is_edge_down, is_edge_up};

#[derive(Clone, Copy, Debug)]
pub struct SimpleEdge {
    pub to_node: Option<i32>,
    pub edge_type: GraphEdgeType,
}

#[derive(Clone, Copy, Debug)]
pub struct SimpleNode {
    pub node_id: i32,
    pub node_type: GraphNodeType,
}

impl From<i32> for SimpleNode {
    fn from(node_id: i32) -> Self {
        SimpleNode {
            node_id,
            node_type: GraphNodeType::Usual,
        }
    }
}

impl From<i32> for SimpleEdge {
    fn from(to_node: i32) -> Self {
        u(to_node)
    }
}

/// `Int.U`
pub fn usual(node_id: i32) -> SimpleNode {
    SimpleNode::from(node_id)
}
/// `Int.UNM`
pub fn unm(node_id: i32) -> SimpleNode {
    SimpleNode {
        node_id,
        node_type: GraphNodeType::Unmatched,
    }
}
/// `Int.NOT_LOAD`
pub fn not_load_node(node_id: i32) -> SimpleNode {
    SimpleNode {
        node_id,
        node_type: GraphNodeType::NotLoadCommit,
    }
}

fn edge(to_node: Option<i32>, edge_type: GraphEdgeType) -> SimpleEdge {
    SimpleEdge { to_node, edge_type }
}
/// `Int.u`
pub fn u(to_node: i32) -> SimpleEdge {
    edge(Some(to_node), GraphEdgeType::Usual)
}
/// `Int.dot`
pub fn dot(to_node: i32) -> SimpleEdge {
    edge(Some(to_node), GraphEdgeType::Dotted)
}
/// `Int?.up_dot`
pub fn up_dot(to_node: Option<i32>) -> SimpleEdge {
    edge(to_node, GraphEdgeType::DottedArrowUp)
}
/// `Int?.down_dot`
pub fn down_dot(to_node: Option<i32>) -> SimpleEdge {
    edge(to_node, GraphEdgeType::DottedArrowDown)
}
/// `Int?.not_load`
pub fn not_load(to_node: Option<i32>) -> SimpleEdge {
    edge(to_node, GraphEdgeType::NotLoadCommit)
}

struct NodeWithEdges {
    node_id: i32,
    edges: Vec<SimpleEdge>,
    node_type: GraphNodeType,
}

#[derive(Default)]
pub struct TestGraphBuilder {
    nodes: Vec<NodeWithEdges>,
}

impl TestGraphBuilder {
    /// `node(edges...)`
    pub fn node<E: Into<SimpleEdge>>(
        &mut self,
        node: impl Into<SimpleNode>,
        edges: impl IntoIterator<Item = E>,
    ) {
        let node = node.into();
        self.nodes.push(NodeWithEdges {
            node_id: node.node_id,
            edges: edges.into_iter().map(Into::into).collect(),
            node_type: node.node_type,
        });
    }

    pub fn done(self) -> TestLinearGraph {
        TestLinearGraph::new(self.nodes)
    }
}

/// `graph { ... }`
pub fn graph(f: impl FnOnce(&mut TestGraphBuilder)) -> TestLinearGraph {
    let mut builder = TestGraphBuilder::default();
    f(&mut builder);
    builder.done()
}

pub struct TestLinearGraph {
    nodes: Vec<GraphNode>,
    node_index_to_id: HashMap<i32, i32>,
    node_id_to_index: HashMap<i32, i32>,
    edges: HashMap<i32, Vec<GraphEdge>>,
}

impl TestLinearGraph {
    fn new(build_nodes: Vec<NodeWithEdges>) -> TestLinearGraph {
        let mut node_index_to_id = HashMap::new();
        let nodes = build_nodes
            .iter()
            .enumerate()
            .map(|(index, it)| {
                node_index_to_id.insert(index as i32, it.node_id);
                GraphNode::with_type(index as i32, it.node_type)
            })
            .collect();
        let node_id_to_index: HashMap<i32, i32> =
            node_index_to_id.iter().map(|(&k, &v)| (v, k)).collect();

        // create edges
        let mut edges: HashMap<i32, Vec<GraphEdge>> = HashMap::new();
        for node in &build_nodes {
            let node_index = node_id_to_index[&node.node_id];
            for simple_edge in &node.edges {
                let edge_type = simple_edge.edge_type;

                if edge_type.is_normal_edge() {
                    let another_node_index = simple_edge
                        .to_node
                        .and_then(|to| node_id_to_index.get(&to).copied())
                        .unwrap_or_else(|| {
                            panic!(
                                "Graph is incorrect. Node {} has {edge_type:?} edge to not existed node: {:?}",
                                node.node_id, simple_edge.to_node
                            )
                        });
                    let graph_edge =
                        GraphEdge::create_normal_edge(another_node_index, node_index, edge_type);
                    edges.entry(node_index).or_default().push(graph_edge);
                    edges
                        .entry(another_node_index)
                        .or_default()
                        .push(graph_edge);
                } else {
                    edges.entry(node_index).or_default().push(
                        GraphEdge::create_edge_with_target_id(
                            node_index,
                            simple_edge.to_node,
                            edge_type,
                        ),
                    );
                }
            }
        }

        TestLinearGraph {
            nodes,
            node_index_to_id,
            node_id_to_index,
            edges,
        }
    }
}

impl LinearGraph for TestLinearGraph {
    fn nodes_count(&self) -> i32 {
        self.nodes.len() as i32
    }

    fn get_node_id(&self, node_index: i32) -> i32 {
        self.node_index_to_id[&node_index]
    }

    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        self.edges
            .get(&node_index)
            .into_iter()
            .flatten()
            .filter(|it| {
                if it.edge_type.is_normal_edge() {
                    (is_edge_up(it, node_index) && filter.up_normal())
                        || (is_edge_down(it, node_index) && filter.down_normal())
                } else {
                    filter.special()
                }
            })
            .copied()
            .collect()
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        self.nodes[node_index as usize]
    }

    fn get_node_index(&self, node_id: i32) -> Option<i32> {
        self.node_id_to_index.get(&node_id).copied()
    }
}
```

- [ ] **Step 6: Create `crates/ig-log/tests/test_graph_builder.rs`**

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/TestGraphBuilderTest.kt`
//! (fixtures `testGraphBuilder/`) and
//! `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/TestGraphBuilderTest.kt`
//! (the `LinearGraphParser` test, fixtures `testParser/`).

mod common;

/// `com.intellij.vcs.log.graph.TestGraphBuilderTest`
mod test_graph_builder_test {
    use super::common::load_text;
    use super::common::str_utils::graph_as_string;
    use super::common::test_graph_builder::*;

    fn run_test(test_name: &str, builder: impl FnOnce(&mut TestGraphBuilder)) {
        let actual = graph_as_string(&graph(builder), false);
        assert_eq!(
            load_text("testGraphBuilder", &format!("{test_name}.txt")),
            actual
        );
    }

    #[test]
    fn simple() {
        run_test("simple", |b| {
            b.node(1, [2]);
            b.node(2, [3]);
            b.node(3, [0; 0]);
        });
    }

    #[test]
    fn simple_many_nodes() {
        run_test("simpleManyNodes", |b| {
            b.node(3, [4, 6, 8]);
            b.node(4, [5, 6, 7, 8]);
            b.node(7, [8, 6]);
            b.node(6, [9, 10, 11]);
            b.node(9, [10, 11]);
            b.node(10, [11]);
            b.node(5, [8, 11]);
            b.node(8, [11]);
            b.node(11, [0; 0]);
        });
    }

    #[test]
    fn special_edges() {
        run_test("specialEdges", |b| {
            b.node(
                1,
                [
                    u(2),
                    dot(3),
                    not_load(Some(100)),
                    down_dot(Some(42)),
                    down_dot(None),
                    not_load(None),
                    up_dot(Some(0)),
                    up_dot(None),
                ],
            );
            b.node(2, [up_dot(None)]);
            b.node(3, [up_dot(Some(0))]);
        });
    }

    #[test]
    fn special_nodes() {
        run_test("specialNodes", |b| {
            b.node(usual(2), [3, 4]);
            b.node(unm(3), [dot(4)]);
            b.node(usual(4), [0; 0]);
            b.node(not_load_node(100), [0; 0]);
        });
    }
}

/// `com.intellij.vcs.log.graph.impl.print.TestGraphBuilderTest` (`LinearGraphParser`)
mod test_parser_test {
    use super::common::do_test;
    use super::common::parser::parse_linear_graph;
    use super::common::str_utils::graph_as_string;

    fn run(test_name: &str) {
        do_test("testParser", test_name, |input, out| {
            let graph = parse_linear_graph(input);
            assert_eq!(out, graph_as_string(&graph, false));
        });
    }

    #[test]
    fn special_elements() {
        run("specialElements");
    }

    #[test]
    fn many_nodes() {
        run("manyNodes");
    }

    #[test]
    fn not_load_node() {
        run("notLoadNode");
    }
}
```

- [ ] **Step 7: Run it**

Run: `cargo test -p ig-log --test test_graph_builder`
Expected: `test result: ok. 7 passed; 0 failed`. A failure means the port deviates from upstream. Fix the source, never the expected data.

- [ ] **Step 8: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add crates/ig-log/tests
git commit -m "test(ig-log): port TestGraphBuilderTest with upstream test data"
```

---
### Task 3: Utilities

**Files:**
- Create: `crates/ig-log/tests/utils.rs`

Ports `BitSetFlagsTest`, `IntIntMultiMapTest`. **Review focus:** `src/utils/*`.

- [ ] **Step 1: Create `crates/ig-log/tests/utils.rs`**

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/BitSetFlagsTest.java`
//! and `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/IntIntMultiMapTest.kt`.

mod bit_set_flags_test {
    use ig_log::utils::flags::{BitSetFlags, Flags};

    fn bit_to_char(bit: bool) -> char {
        if bit { '1' } else { '0' }
    }

    fn to_str(flags: &dyn Flags) -> String {
        (0..flags.size())
            .map(|i| bit_to_char(flags.get(i)))
            .collect()
    }

    #[test]
    fn init_test() {
        let flags = BitSetFlags::new(5);
        assert_eq!("00000", to_str(&flags));
        assert_eq!(5, flags.size());
    }

    #[test]
    fn set_test() {
        let mut flags = BitSetFlags::new(6);
        assert_eq!("000000", to_str(&flags));

        flags.set(0, true);
        assert_eq!("100000", to_str(&flags));

        flags.set(0, false);
        assert_eq!("000000", to_str(&flags));

        flags.set(4, true);
        assert_eq!("000010", to_str(&flags));

        flags.set(2, true);
        assert_eq!("001010", to_str(&flags));
    }

    #[test]
    fn set_all_test() {
        let mut flags = BitSetFlags::new(6);
        assert_eq!("000000", to_str(&flags));

        flags.set_all(false);
        assert_eq!("000000", to_str(&flags));

        flags.set_all(true);
        assert_eq!("111111", to_str(&flags));
    }

    #[test]
    fn size1_test() {
        let mut flags = BitSetFlags::new(1);
        assert_eq!("0", to_str(&flags));
        assert_eq!(1, flags.size());

        flags.set(0, true);
        assert_eq!("1", to_str(&flags));

        flags.set(0, false);
        assert_eq!("0", to_str(&flags));

        flags.set_all(true);
        assert_eq!("1", to_str(&flags));
    }

    #[test]
    fn empty_flags_test() {
        let mut flags = BitSetFlags::new(0);
        assert_eq!(0, flags.size());
        flags.set_all(true);
        assert_eq!(0, flags.size());
    }
}

mod int_int_multi_map_test {
    use std::collections::BTreeMap;

    use ig_log::utils::int_int_multi_map::IntIntMultiMap;

    /// Compares `IntIntMultiMap` with a reference multimap (upstream: IntelliJ's `MultiMap`).
    #[derive(Default)]
    struct MapTester {
        correct_map: BTreeMap<i32, Vec<i32>>,
        tested_map: IntIntMultiMap,
    }

    fn sorted(values: &[i32]) -> Vec<i32> {
        let mut values = values.to_vec();
        values.sort();
        values
    }

    fn join_values(values: &[i32]) -> String {
        values
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }

    impl MapTester {
        fn put_value(&mut self, key: i32, value: i32) {
            self.correct_map.entry(key).or_default().push(value);
            self.tested_map.put_value(key, value);
            self.compare_maps();
        }

        fn remove(&mut self, key: i32, value: i32) {
            if let Some(values) = self.correct_map.get_mut(&key) {
                if let Some(i) = values.iter().position(|&v| v == value) {
                    values.remove(i);
                }
                if values.is_empty() {
                    self.correct_map.remove(&key);
                }
            }
            self.tested_map.remove(key, value);
            self.compare_maps();
        }

        fn compare_maps(&self) {
            let keys = self.compare_keys();
            let lines = |f: &dyn Fn(i32) -> String| {
                keys.iter().map(|&k| f(k)).collect::<Vec<_>>().join("\n")
            };
            let correct_str =
                lines(&|k| format!("{k}:{}", join_values(&sorted(&self.correct_map[&k]))));
            let tested_str =
                lines(&|k| format!("{k}:{}", join_values(&sorted(self.tested_map.get(k)))));
            let tested_array_str = lines(&|k| {
                format!(
                    "{k}:{}",
                    join_values(&sorted(self.tested_map.get_as_array(k)))
                )
            });
            assert_eq!(
                tested_array_str, tested_str,
                "get and getasArray have nonequal results"
            );
            assert_eq!(correct_str, tested_str, "maps non equals");
        }

        fn compare_keys(&self) -> Vec<i32> {
            let correct_keys: Vec<i32> = self.correct_map.keys().copied().collect();
            let tested_keys = sorted(&self.tested_map.keys());
            assert_eq!(join_values(&correct_keys), join_values(&tested_keys));
            correct_keys
        }
    }

    fn run_test(test: impl FnOnce(&mut MapTester)) {
        test(&mut MapTester::default());
    }

    #[test]
    fn simple() {
        run_test(|t| {
            t.put_value(2, 239);
            t.put_value(2, 17);
            t.put_value(2, 18);
            t.put_value(8, 18);
            t.put_value(8, 22);

            t.remove(2, 2);
            t.put_value(2, 2);
            t.remove(2, 17);
            t.remove(8, 18);
        });
    }

    #[test]
    fn remove_one_value() {
        run_test(|t| {
            t.put_value(1, 2);
            t.remove(1, 2);
        });
    }

    #[test]
    fn remove_two_value() {
        run_test(|t| {
            t.put_value(1, 2);
            t.put_value(1, 3);
            t.remove(1, 2);
            t.remove(1, 3);
        });
    }

    #[test]
    fn add_different_keys() {
        run_test(|t| {
            t.put_value(1, 1);
            t.put_value(2, 10);
            t.put_value(1, 3);
            t.put_value(2, 30);
            t.remove(1, 3);
        });
    }

    #[test]
    fn remove_for_non_existed_key() {
        run_test(|t| {
            t.remove(1, 3);

            t.put_value(2, 5);
            t.remove(3, 3);
        });
    }
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p ig-log --test utils`
Expected: `test result: ok. 10 passed; 0 failed`. A failure means the port deviates from upstream. Fix the source, never the expected data.

- [ ] **Step 3: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add crates/ig-log/tests
git commit -m "test(ig-log): port BitSetFlagsTest and IntIntMultiMapTest"
```

---
### Task 4: Permanent graph builder

**Files:**
- Create: `crates/ig-log/tests/graph_builder.rs`, `crates/ig-log/tests/fixtures/graphBuilder/**`, `crates/ig-log/tests/fixtures/graphHashIndex/**`

Ports `GraphBuilderTest`, `GraphBuilderHashIndexTest` (String and Integer ids). **Review focus:** `src/permanent/{duplicate_parent_fixer,permanent_linear_graph_builder,permanent_linear_graph_impl,permanent_commits_info_impl}.rs`, `src/graph_commit.rs`.

- [ ] **Step 1: Copy the upstream test data, byte-identical**

```bash
mkdir -p crates/ig-log/tests/fixtures
cp -R "$IJ/platform/vcs-log/graph/testData/graphBuilder" crates/ig-log/tests/fixtures/
cp -R "$IJ/platform/vcs-log/graph/testData/graphHashIndex" crates/ig-log/tests/fixtures/
diff -r "$IJ/platform/vcs-log/graph/testData/graphBuilder" crates/ig-log/tests/fixtures/graphBuilder && echo "graphBuilder identical"
diff -r "$IJ/platform/vcs-log/graph/testData/graphHashIndex" crates/ig-log/tests/fixtures/graphHashIndex && echo "graphHashIndex identical"
```

- [ ] **Step 2: Create `crates/ig-log/tests/graph_builder.rs`**

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphBuilderTest.kt`
//! (fixtures `graphBuilder/`) and
//! `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphBuilderHashIndexTest.java`
//! (fixtures `graphHashIndex/`), each for string and integer commit ids.

mod common;

use std::collections::HashMap;

use common::str_utils::{commits_info_to_str, graph_as_string};
use common::{CommitIdManager, IntegerCommitIdManager, StringCommitIdManager, do_test};
use ig_log::permanent::permanent_commits_info_impl::PermanentCommitsInfoImpl;
use ig_log::permanent::permanent_linear_graph_builder::PermanentLinearGraphBuilder;

fn graph_builder_test<M: CommitIdManager>(test_name: &str) {
    do_test("graphBuilder", test_name, |input, out| {
        let commits = M::parse_commit_list(input);

        let graph_builder = PermanentLinearGraphBuilder::new_instance(&commits);
        let graph = graph_builder.build_with(|dom| -M::to_int(dom));

        assert_eq!(out, graph_as_string(&graph, false));
    });
}

fn graph_builder_hash_index_test<M: CommitIdManager>(test_name: &str) {
    do_test("graphHashIndex", test_name, |input, out| {
        let commits = M::parse_commit_list(input);
        let commits_info = PermanentCommitsInfoImpl::new_instance(&commits, HashMap::new());

        assert_eq!(
            out,
            commits_info_to_str(&commits_info, commits.len(), M::to_str)
        );
    });
}

macro_rules! graph_builder_tests {
    ($module:ident, $manager:ty) => {
        mod $module {
            fn run(test_name: &str) {
                crate::graph_builder_test::<$manager>(test_name);
            }

            #[test]
            fn simple() {
                run("simple");
            }
            #[test]
            fn many_nodes() {
                run("manyNodes");
            }
            #[test]
            fn many_up_nodes() {
                run("manyUpNodes");
            }
            #[test]
            fn many_down_nodes() {
                run("manyDownNodes");
            }
            #[test]
            fn one_node() {
                run("oneNode");
            }
            #[test]
            fn one_node_not_full_graph() {
                run("oneNodeNotFullGraph");
            }
            #[test]
            fn not_full_graph() {
                run("notFullGraph");
            }
            #[test]
            fn parents_order() {
                run("parentsOrder");
            }
            #[test]
            fn duplicate_parents() {
                run("duplicateParents");
            }
        }
    };
}

macro_rules! graph_builder_hash_index_tests {
    ($module:ident, $manager:ty) => {
        mod $module {
            fn run(test_name: &str) {
                crate::graph_builder_hash_index_test::<$manager>(test_name);
            }

            #[test]
            fn simple() {
                run("simple");
            }
            #[test]
            fn many_nodes() {
                run("manyNodes");
            }
            #[test]
            fn not_full_graph() {
                run("notFullGraph");
            }
            #[test]
            fn one_node() {
                run("oneNode");
            }
            #[test]
            fn one_node_not_full_graph() {
                run("oneNodeNotFullGraph");
            }
        }
    };
}

/// `GraphBuilderTest.StringTest` / `IntegerTest`
mod graph_builder_test {
    graph_builder_tests!(string_test, crate::StringCommitIdManager);
    graph_builder_tests!(integer_test, crate::IntegerCommitIdManager);
}

/// `GraphBuilderHashIndexTest.StringTest` / `IntegerTest`
mod graph_builder_hash_index_test {
    graph_builder_hash_index_tests!(string_test, crate::StringCommitIdManager);
    graph_builder_hash_index_tests!(integer_test, crate::IntegerCommitIdManager);
}
```

- [ ] **Step 3: Run it**

Run: `cargo test -p ig-log --test graph_builder`
Expected: `test result: ok. 28 passed; 0 failed`. A failure means the port deviates from upstream. Fix the source, never the expected data.

- [ ] **Step 4: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add crates/ig-log/tests
git commit -m "test(ig-log): port GraphBuilderTest with upstream test data"
```

---
### Task 5: Graph layout

**Files:**
- Create: `crates/ig-log/tests/graph_layout_builder.rs`, `crates/ig-log/tests/fixtures/layoutBuilder/**`

Ports `GraphLayoutBuilderTest`, `GraphLayoutBuilderHeadOrderTest`. **Review focus:** `src/permanent/{graph_layout_builder,graph_layout_impl}.rs`, `src/utils/dfs_util.rs`.

- [ ] **Step 1: Copy the upstream test data, byte-identical**

```bash
mkdir -p crates/ig-log/tests/fixtures
cp -R "$IJ/platform/vcs-log/graph/testData/layoutBuilder" crates/ig-log/tests/fixtures/
diff -r "$IJ/platform/vcs-log/graph/testData/layoutBuilder" crates/ig-log/tests/fixtures/layoutBuilder && echo "layoutBuilder identical"
```

- [ ] **Step 2: Create `crates/ig-log/tests/graph_layout_builder.rs`**

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilderTest.java`
//! (fixtures `layoutBuilder/`, string and integer commit ids) and
//! `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilderHeadOrderTest.kt`.

mod common;

use common::str_utils::permanent_graph_layout_model_to_str;
use common::{CommitIdManager, IntegerCommitIdManager, StringCommitIdManager, do_test};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::graph_commit::GraphCommit;
use ig_log::permanent::graph_layout_builder;
use ig_log::permanent::permanent_linear_graph_builder::PermanentLinearGraphBuilder;

fn graph_layout_builder_test<M: CommitIdManager>(test_name: &str) {
    do_test("layoutBuilder", test_name, |input, out| {
        let commits = M::parse_commit_list(input);

        let graph = PermanentLinearGraphBuilder::new_instance(&commits).build();

        let graph_layout = graph_layout_builder::build(&graph, |o1, o2| {
            let id1 = commits[o1 as usize].id();
            let id2 = commits[o2 as usize].id();
            M::to_str(id1).cmp(&M::to_str(id2))
        });
        assert_eq!(
            out,
            permanent_graph_layout_model_to_str(&graph_layout, graph.nodes_count())
        );
    });
}

macro_rules! graph_layout_builder_tests {
    ($module:ident, $manager:ty) => {
        mod $module {
            fn run(test_name: &str) {
                crate::graph_layout_builder_test::<$manager>(test_name);
            }

            #[test]
            fn many_nodes() {
                run("manyNodes");
            }
            #[test]
            fn not_full_graph() {
                run("notFullGraph");
            }
            #[test]
            fn one_node() {
                run("oneNode");
            }
            #[test]
            fn one_node_not_full_graph() {
                run("oneNodeNotFullGraph");
            }
            #[test]
            fn heads_order() {
                run("headsOrder");
            }
        }
    };
}

/// `GraphLayoutBuilderTest.StringTest` / `IntegerTest`
mod graph_layout_builder_test {
    graph_layout_builder_tests!(string_test, crate::StringCommitIdManager);
    graph_layout_builder_tests!(integer_test, crate::IntegerCommitIdManager);
}

/// `GraphLayoutBuilderHeadOrderTest`
mod graph_layout_builder_head_order_test {
    use std::cmp::Ordering;

    use super::common::test_graph_builder::graph;
    use ig_log::api::GraphLayout;
    use ig_log::api::linear_graph::LinearGraph;
    use ig_log::permanent::graph_layout_builder;

    /// Asserts that the layout built for `graph` has the expected head commit for each commit.
    fn assert_heads_for_commits(
        graph: &impl LinearGraph,
        branch_heads: &[i32],
        head_comparator: impl FnMut(i32, i32) -> Ordering,
        expected_heads: &[i32],
    ) {
        let graph_layout =
            graph_layout_builder::build_with_branches(graph, branch_heads, head_comparator);
        let heads: Vec<i32> = (0..graph.nodes_count())
            .map(|i| graph_layout.get_one_of_head_node_index(i))
            .collect();
        assert_eq!(expected_heads, heads.as_slice());
    }

    /// `list.indexOf(i) - list.indexOf(j)` (`-1` for nodes not in the list).
    fn order(list: &[i32]) -> impl FnMut(i32, i32) -> Ordering + '_ {
        let index_of = |x: i32| list.iter().position(|&y| y == x).map_or(-1, |p| p as i32);
        move |i, j| (index_of(i) - index_of(j)).cmp(&0)
    }

    #[test]
    fn linear_graph() {
        let graph = graph(|b| {
            b.node(0, [1]);
            b.node(1, [2]);
            b.node(2, [3]);
            b.node(3, [0; 0]);
        });
        assert_heads_for_commits(&graph, &[0, 2], order(&[0, 2]), &[0, 0, 0, 0]);
        assert_heads_for_commits(&graph, &[0, 2], order(&[2, 0]), &[0, 0, 2, 2]);
    }

    #[test]
    fn branching_graph() {
        let graph = graph(|b| {
            b.node(0, [2]);
            b.node(1, [3]);
            b.node(2, [3]);
            b.node(3, [4]);
            b.node(4, [0; 0]);
        });
        assert_heads_for_commits(&graph, &[0, 2], order(&[0, 1, 2]), &[0, 1, 0, 0, 0]);
        assert_heads_for_commits(&graph, &[0, 2], order(&[0, 2, 1]), &[0, 1, 0, 0, 0]);
        assert_heads_for_commits(&graph, &[0, 2], order(&[1, 0, 2]), &[0, 1, 0, 1, 1]);
        assert_heads_for_commits(&graph, &[0, 2], order(&[1, 2, 0]), &[0, 1, 2, 1, 1]);
        assert_heads_for_commits(&graph, &[0, 2], order(&[2, 0, 1]), &[0, 1, 2, 2, 2]);
        assert_heads_for_commits(&graph, &[0, 2], order(&[2, 1, 0]), &[0, 1, 2, 2, 2]);
    }
}
```

- [ ] **Step 3: Run it**

Run: `cargo test -p ig-log --test graph_layout_builder`
Expected: `test result: ok. 12 passed; 0 failed`. A failure means the port deviates from upstream. Fix the source, never the expected data.

- [ ] **Step 4: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add crates/ig-log/tests
git commit -m "test(ig-log): port GraphLayoutBuilderTest with upstream test data"
```

---
### Task 6: Containing branches

**Files:**
- Create: `crates/ig-log/tests/containing_branches.rs`, `crates/ig-log/tests/fixtures/containingBranches/**`

Ports `ContainingBranchesTest`. **Review focus:** `src/facade/reachable_nodes.rs`.

- [ ] **Step 1: Copy the upstream test data, byte-identical**

```bash
mkdir -p crates/ig-log/tests/fixtures
cp -R "$IJ/platform/vcs-log/graph/testData/containingBranches" crates/ig-log/tests/fixtures/
diff -r "$IJ/platform/vcs-log/graph/testData/containingBranches" crates/ig-log/tests/fixtures/containingBranches && echo "containingBranches identical"
```

- [ ] **Step 2: Create `crates/ig-log/tests/containing_branches.rs`**

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/permanent/ContainingBranchesTest.java`
//! (fixtures `containingBranches/`, string and integer commit ids).

mod common;

use std::collections::HashSet;

use common::str_utils::containing_branches_getter_to_str;
use common::{CommitIdManager, IntegerCommitIdManager, StringCommitIdManager, do_test};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::facade::reachable_nodes::ReachableNodes;
use ig_log::permanent::permanent_linear_graph_builder::PermanentLinearGraphBuilder;
use ig_log::utils::linear_graph_utils::as_lite_linear_graph;

const SEPARATOR: &str = "\nBRANCH NODES:\n";

fn parse_branch_node_index(s: &str) -> HashSet<i32> {
    common::parser::split_whitespace_java(s)
        .into_iter()
        .map(|sub_str| {
            sub_str
                .parse()
                .unwrap_or_else(|e| panic!("{sub_str:?}: {e}"))
        })
        .collect()
}

fn containing_branches_test<M: CommitIdManager>(test_name: &str) {
    do_test("containingBranches/", test_name, |input, out| {
        let i = input.find(SEPARATOR).expect("no branch nodes separator");
        let commits = M::parse_commit_list(&input[..i]);

        let graph = PermanentLinearGraphBuilder::new_instance(&commits).build();
        let branches = parse_branch_node_index(&input[i + SEPARATOR.len()..]);
        let reachable_nodes = ReachableNodes::new(as_lite_linear_graph(&graph));

        assert_eq!(
            out,
            containing_branches_getter_to_str(&reachable_nodes, &branches, graph.nodes_count())
        );
    });
}

macro_rules! containing_branches_tests {
    ($module:ident, $manager:ty) => {
        mod $module {
            fn run(test_name: &str) {
                crate::containing_branches_test::<$manager>(test_name);
            }

            #[test]
            fn simple() {
                run("simple");
            }
            #[test]
            fn many_nodes() {
                run("manyNodes");
            }
            #[test]
            fn not_full_graph() {
                run("notFullGraph");
            }
            #[test]
            fn one_node() {
                run("oneNode");
            }
            #[test]
            fn one_node_not_full_graph() {
                run("oneNodeNotFullGraph");
            }
        }
    };
}

/// `ContainingBranchesTest.StringTest` / `IntegerTest`
mod containing_branches_test {
    containing_branches_tests!(string_test, crate::StringCommitIdManager);
    containing_branches_tests!(integer_test, crate::IntegerCommitIdManager);
}
```

- [ ] **Step 3: Run it**

Run: `cargo test -p ig-log --test containing_branches`
Expected: `test result: ok. 10 passed; 0 failed`. A failure means the port deviates from upstream. Fix the source, never the expected data.

- [ ] **Step 4: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add crates/ig-log/tests
git commit -m "test(ig-log): port ContainingBranchesTest with upstream test data"
```

---
### Task 7: Edges in row

**Files:**
- Create: `crates/ig-log/tests/edges_in_row.rs`, `crates/ig-log/tests/fixtures/edgesInRow/**`

Ports `EdgesInRowTest`. **Review focus:** `src/print/edges_in_row_generator.rs`, `src/utils/bounded_cache.rs`.

- [ ] **Step 1: Copy the upstream test data, byte-identical**

```bash
mkdir -p crates/ig-log/tests/fixtures
cp -R "$IJ/platform/vcs-log/graph/testData/edgesInRow" crates/ig-log/tests/fixtures/
diff -r "$IJ/platform/vcs-log/graph/testData/edgesInRow" crates/ig-log/tests/fixtures/edgesInRow && echo "edgesInRow identical"
```

- [ ] **Step 2: Create `crates/ig-log/tests/edges_in_row.rs`**

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/EdgesInRowTest.java`
//! (fixtures `edgesInRow/`).

mod common;

use std::sync::Arc;

use common::do_test;
use common::parser::parse_linear_graph;
use common::str_utils::edges_in_row_to_str;
use ig_log::api::linear_graph::LinearGraph;
use ig_log::print::edges_in_row_generator::EdgesInRowGenerator;

fn run(test_name: &str) {
    do_test("edgesInRow", test_name, |input, out| {
        let graph: Arc<dyn LinearGraph> = Arc::new(parse_linear_graph(input));
        let edges_in_row_generator = EdgesInRowGenerator::new(graph.clone());
        assert_eq!(
            out,
            edges_in_row_to_str(&edges_in_row_generator, graph.nodes_count())
        );
    });
}

#[test]
fn simple() {
    run("simple");
}

#[test]
fn many_nodes() {
    run("manyNodes");
}

#[test]
fn many_up_nodes() {
    run("manyUpNodes");
}

#[test]
fn many_down_nodes() {
    run("manyDownNodes");
}

#[test]
fn one_node() {
    run("oneNode");
}

#[test]
fn one_node_not_full_graph() {
    run("oneNodeNotFullGraph");
}

#[test]
fn not_full_graph() {
    run("notFullGraph");
}

#[test]
fn long_graph() {
    run("longGraph");
}

#[test]
fn not_load_node() {
    run("notLoadNode");
}
```

- [ ] **Step 3: Run it**

Run: `cargo test -p ig-log --test edges_in_row`
Expected: `test result: ok. 9 passed; 0 failed`. A failure means the port deviates from upstream. Fix the source, never the expected data.

- [ ] **Step 4: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add crates/ig-log/tests
git commit -m "test(ig-log): port EdgesInRowTest with upstream test data"
```

---
### Task 8: Print element generator

**Files:**
- Create: `crates/ig-log/tests/print_element_generator.rs`, `crates/ig-log/tests/fixtures/elementGenerator/**`

Ports `PrintElementGeneratorTest`. **Review focus:** `src/print/{print_element_generator_impl,elements,graph_element_comparator_by_layout_index}.rs`, `src/print_element.rs`, `src/api/printer.rs`, `src/utils/java_sort.rs`.

- [ ] **Step 1: Copy the upstream test data, byte-identical**

```bash
mkdir -p crates/ig-log/tests/fixtures
cp -R "$IJ/platform/vcs-log/graph/testData/elementGenerator" crates/ig-log/tests/fixtures/
diff -r "$IJ/platform/vcs-log/graph/testData/elementGenerator" crates/ig-log/tests/fixtures/elementGenerator && echo "elementGenerator identical"
```

- [ ] **Step 2: Create `crates/ig-log/tests/print_element_generator.rs`**

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/print/PrintElementGeneratorTest.kt`
//! (fixtures `elementGenerator/`).

mod common;

use std::sync::Arc;

use common::parser::parse_linear_graph;
use common::str_utils::print_element_generator_as_string;
use common::{IN_POSTFIX, OUT_POSTFIX, do_test, load_text};
use ig_log::api::GraphLayout;
use ig_log::api::elements::GraphElement;
use ig_log::api::linear_graph::LinearGraph;
use ig_log::api::printer::{GraphPrintElement, PrintElementPresentationManager};
use ig_log::permanent::graph_layout_builder;
use ig_log::print::graph_element_comparator_by_layout_index::GraphElementComparatorByLayoutIndex;
use ig_log::print::print_element_generator_impl::PrintElementGeneratorImpl;
use ig_log::utils::linear_graph_utils::{as_normal_edge, get_not_null_node_index};

const DIRECTORY: &str = "elementGenerator";

/// Nothing selected; a node's color is its index, a normal edge's is `up + down`, a special
/// edge's is its node index.
struct TestPrintElementPresentationManager;

impl PrintElementPresentationManager for TestPrintElementPresentationManager {
    fn is_selected(&self, _print_element: &GraphPrintElement) -> bool {
        false
    }

    fn get_color_id(&self, element: &GraphElement) -> i32 {
        match element {
            GraphElement::Node(node) => node.node_index,
            GraphElement::Edge(edge) => match as_normal_edge(edge) {
                Some(normal_edge) => normal_edge.up + normal_edge.down,
                None => get_not_null_node_index(edge),
            },
        }
    }
}

fn run_test_with_sizes(
    input: &str,
    out: &str,
    long_edge_size: i32,
    visible_part_size: i32,
    edge_with_arrow_size: i32,
) {
    let graph: Arc<dyn LinearGraph> = Arc::new(parse_linear_graph(input));
    let graph_layout = Arc::new(graph_layout_builder::build(&*graph, |o1, o2| o1.cmp(&o2)));
    let graph_element_comparator = GraphElementComparatorByLayoutIndex::new(move |node_index| {
        graph_layout.get_layout_index(node_index)
    });
    let print_element_generator = PrintElementGeneratorImpl::with_sizes(
        graph.clone(),
        TestPrintElementPresentationManager,
        move |a: &GraphElement, b: &GraphElement| graph_element_comparator.ordering(a, b),
        long_edge_size,
        visible_part_size,
        edge_with_arrow_size,
    );
    let actual = print_element_generator_as_string(&print_element_generator, graph.nodes_count());
    assert_eq!(out, actual);
}

fn run(test_name: &str) {
    do_test(DIRECTORY, test_name, |input, out| {
        run_test_with_sizes(input, out, 7, 2, 10)
    });
}

#[test]
fn one_node() {
    run("oneNode");
}

#[test]
fn many_nodes() {
    run("manyNodes");
}

#[test]
fn long_edges() {
    run("longEdges");
}

#[test]
fn special_elements() {
    run("specialElements");
}

// oneUpOneDown tests were created in order to investigate some arrow behavior in upsource

#[test]
fn one_up_one_down1() {
    let test_name = "oneUpOneDown1";
    run_test_with_sizes(
        &load_text(DIRECTORY, &format!("{test_name}{IN_POSTFIX}")),
        &load_text(DIRECTORY, &format!("{test_name}{OUT_POSTFIX}")),
        7,
        1,
        10,
    );
}

#[test]
fn one_up_one_down2() {
    let test_name = "oneUpOneDown2";
    run_test_with_sizes(
        &load_text(DIRECTORY, &format!("{test_name}{IN_POSTFIX}")),
        &load_text(DIRECTORY, &format!("{test_name}{OUT_POSTFIX}")),
        10,
        1,
        10,
    );
}
```

- [ ] **Step 3: Run it**

Run: `cargo test -p ig-log --test print_element_generator`
Expected: `test result: ok. 6 passed; 0 failed`. A failure means the port deviates from upstream. Fix the source, never the expected data.

- [ ] **Step 4: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add crates/ig-log/tests
git commit -m "test(ig-log): port PrintElementGeneratorTest with upstream test data"
```

---
### Task 9: Edge storage

**Files:**
- Create: `crates/ig-log/tests/edge_storage.rs`

Ports `EdgeStorageTest`. **Review focus:** `src/collapsing/*`, `src/facade/linear_graph_wrapper.rs`.

- [ ] **Step 1: Create `crates/ig-log/tests/edge_storage.rs`**

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/EdgeStorageTest.kt`.

mod common;

use std::sync::{Arc, RwLock};

use common::str_utils::edge_as_string;
use common::test_graph_builder::{SimpleEdge, down_dot, not_load, up_dot};
use ig_log::api::EdgeFilter;
use ig_log::api::elements::GraphEdgeType;
use ig_log::collapsing::edge_storage::EdgeStorage;
use ig_log::collapsing::edge_storage_wrapper::EdgeStorageWrapper;

fn node_id_by_index(index: i32) -> i32 {
    index - 10
}

fn node_index_by_id(id: i32) -> Option<i32> {
    Some(id + 10)
}

/// `Int to SimpleEdge`
struct FullEdge {
    main_id: i32,
    addition_id: Option<i32>,
    edge_type: GraphEdgeType,
}

fn to(main_id: i32, edge: impl Into<SimpleEdge>) -> FullEdge {
    let edge = edge.into();
    FullEdge {
        main_id,
        addition_id: edge.to_node,
        edge_type: edge.edge_type,
    }
}

fn dot(to_node: i32) -> SimpleEdge {
    common::test_graph_builder::dot(to_node)
}

struct Storage(Arc<RwLock<EdgeStorage>>);

fn create() -> Storage {
    Storage(Arc::new(RwLock::new(EdgeStorage::new())))
}

impl Storage {
    /// `EdgeStorage + FullEdge`
    fn plus(self, edge: FullEdge) -> Storage {
        self.0.write().unwrap().create_edge(
            edge.main_id,
            edge.addition_id.unwrap_or(EdgeStorage::NULL_ID),
            edge.edge_type,
        );
        self
    }

    fn adapter(&self) -> EdgeStorageWrapper {
        EdgeStorageWrapper::new(self.0.clone(), node_index_by_id, node_id_by_index)
    }

    fn as_string(&self) -> String {
        let mut known_ids = self.0.read().unwrap().get_known_ids();
        known_ids.sort();
        let adapter = self.adapter();
        known_ids
            .iter()
            .map(|&id| {
                adapter
                    .get_adjacent_edges(node_index_by_id(id).unwrap(), EdgeFilter::All)
                    .iter()
                    .map(edge_as_string)
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect::<Vec<_>>()
            .join("|-")
    }

    fn assert(self, s: &str) {
        assert_eq!(s, self.as_string());
    }
}

#[test]
fn simple() {
    create().plus(to(1, 2)).assert("11:12:n_U|-11:12:n_U");
}

#[test]
fn dotted() {
    create().plus(to(1, dot(3))).assert("11:13:n_D|-11:13:n_D");
}

#[test]
fn not_load_test() {
    create().plus(to(1, not_load(Some(-3)))).assert("11:n:-3_N");
}

#[test]
fn not_load_null() {
    create().plus(to(1, not_load(None))).assert("11:n:n_N");
}

#[test]
fn down_dot_test() {
    create().plus(to(1, down_dot(Some(3)))).assert("11:n:3_O");
}

#[test]
fn up_dot_test() {
    create().plus(to(1, up_dot(Some(3)))).assert("n:11:3_P");
}

#[test]
fn negative_id() {
    create().plus(to(-2, -3)).assert("7:8:n_U|-7:8:n_U");
}

#[test]
fn big_id() {
    create()
        .plus(to(0x7fffff0, -0x7fffff0))
        .assert("-134217702:134217722:n_U|--134217702:134217722:n_U");
}

#[test]
fn two_equal_edge() {
    create()
        .plus(to(1, 2))
        .plus(to(1, 2))
        .assert("11:12:n_U|-11:12:n_U");
}

#[test]
fn two_from_one_node() {
    create()
        .plus(to(1, 2))
        .plus(to(1, 3))
        .assert("11:12:n_U,11:13:n_U|-11:12:n_U|-11:13:n_U");
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p ig-log --test edge_storage`
Expected: `test result: ok. 10 passed; 0 failed`. A failure means the port deviates from upstream. Fix the source, never the expected data.

- [ ] **Step 3: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add crates/ig-log/tests
git commit -m "test(ig-log): port EdgeStorageTest"
```

---
### Task 10: Facade: permanent and visible graph

**Files:**
- Create: `crates/ig-log/tests/permanent_graph.rs`

Ports IdeaGit facade tests (upstream has no direct facade test for this path). **Review focus:** `src/facade/*`, `src/permanent_graph.rs`, `src/visible_graph.rs`, `src/actions.rs`, `src/print/graph_color_getter.rs`.

- [ ] **Step 1: Create `crates/ig-log/tests/permanent_graph.rs`**

```rust
//! IdeaGit tests for the facade (`PermanentGraphImpl`, `VisibleGraphImpl`) with `SortType::Normal`.
//! Upstream has no direct tests for this path; expected values follow from the upstream algorithms
//! (and from the ported `containingBranches` fixtures).

use std::collections::HashSet;
use std::sync::Arc;

use ig_log::actions::{Cursor, GraphAction, GraphActionType};
use ig_log::api::printer::GraphPrintElement;
use ig_log::facade::permanent_graph_impl::PermanentGraphImpl;
use ig_log::graph_color_manager::GraphColorManager;
use ig_log::graph_commit::{GraphCommit, GraphCommitImpl};
use ig_log::permanent_graph::{Options, PermanentGraph, SortType};
use ig_log::print::graph_color_getter::{
    GraphColorGetterByHeadFactory, GraphColorGetterByNodeFactory,
};
use ig_log::print_element::{EdgePrintElement, EdgePrintElementType, PrintElement};
use ig_log::visible_graph::{RowType, VisibleGraph};

/// `a|-b c` lines; timestamps are the line numbers counted from the end.
fn commits(text: &str) -> Vec<GraphCommitImpl<String>> {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let count = lines.len() as i64;
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let (id, parents) = line.split_once("|-").unwrap();
            let parents = parents.split_whitespace().map(str::to_string).collect();
            GraphCommitImpl::create_commit(id.to_string(), parents, count - i as i64)
        })
        .collect()
}

fn set(ids: &[&str]) -> HashSet<String> {
    ids.iter().map(|s| s.to_string()).collect()
}

fn permanent_graph(text: &str, branches: &[&str]) -> PermanentGraphImpl<String> {
    let factory = GraphColorGetterByNodeFactory::new(|_: &String, layout_index| layout_index);
    PermanentGraphImpl::new_instance(
        &commits(text),
        &factory,
        |a: &String, b: &String| a.cmp(b),
        &set(branches),
    )
}

/// The `containingBranches/notFullGraph` graph: `a6` and `a5` are not loaded.
const NOT_FULL_GRAPH: &str = "
a0|-a1 a6 a3
a1|-a6
a2|-a3
a3|-
a4|-a5
";

#[test]
fn all_commits_round_trip_including_not_loaded_parents() {
    let graph = permanent_graph(NOT_FULL_GRAPH, &[]);
    let original = commits(NOT_FULL_GRAPH);
    let all = graph.all_commits();
    assert_eq!(all.len(), original.len());
    for (actual, expected) in all.iter().zip(&original) {
        assert_eq!(actual.id(), expected.id());
        assert_eq!(actual.parents(), expected.parents());
        assert_eq!(actual.timestamp(), expected.timestamp());
    }
}

/// Upstream filled the first free edge slot with the not-loaded id, swapping parents `p` and `q`
/// of `b` here (`q` is processed first because its first child `a` comes earlier).
#[test]
fn not_loaded_parents_keep_their_order() {
    let text = "
a|-q
b|-p q
";
    let graph = permanent_graph(text, &[]);
    let all = graph.all_commits();
    assert_eq!(all.get(0).parents(), ["q".to_string()]);
    assert_eq!(all.get(1).parents(), ["p".to_string(), "q".to_string()]);
}

#[test]
fn children_and_containing_branches() {
    let graph = permanent_graph(NOT_FULL_GRAPH, &["a0", "a2", "a5"]);
    assert_eq!(
        graph.get_children(&"a3".to_string()),
        ["a2".to_string(), "a0".to_string()]
    );
    assert!(graph.get_children(&"a0".to_string()).is_empty());

    // same as the containingBranches/notFullGraph fixture (branch nodes 0 2 5 -> a0, a2)
    assert_eq!(
        graph.get_containing_branches(&"a1".to_string()),
        set(&["a0"])
    );
    assert_eq!(
        graph.get_containing_branches(&"a3".to_string()),
        set(&["a0", "a2"])
    );
    assert_eq!(graph.get_containing_branches(&"a4".to_string()), set(&[]));
    // unknown and not-loaded commits are skipped by the walk
    assert_eq!(graph.get_containing_branches(&"zz".to_string()), set(&[]));
    assert_eq!(graph.get_containing_branches(&"a6".to_string()), set(&[]));

    let condition = graph.get_contained_in_branch_condition(&["a2".to_string()]);
    assert!(condition.test(&"a2".to_string()));
    assert!(condition.test(&"a3".to_string()));
    assert!(!condition.test(&"a0".to_string()));
    assert!(!condition.test(&"a6".to_string()));
}

#[test]
fn visible_graph_normal_rows() {
    let graph = permanent_graph(NOT_FULL_GRAPH, &["a0", "a2"]);
    let visible = graph
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap();

    assert_eq!(visible.visible_commit_count(), 5);
    assert_eq!(
        format!("{visible:?}"),
        r#"VisibleGraph["a0", "a1", "a2", "a3", "a4"]"#
    );
    let row = visible.get_row_info(3);
    assert_eq!(row.commit(), "a3");
    assert_eq!(row.one_of_heads(), "a0");
    assert_eq!(row.row_type(), RowType::Normal);
    assert_eq!(row.adjacent_rows(false), [2, 0]);
    assert!(row.adjacent_rows(true).is_empty());
    assert_eq!(visible.get_row_info(0).adjacent_rows(true), [1, 3]);
    assert_eq!(visible.get_visible_row_index(&"a4".to_string()), Some(4));
    assert_eq!(visible.get_visible_row_index(&"a6".to_string()), None);
    assert_eq!(visible.get_visible_row_index(&"zz".to_string()), None);
}

#[test]
fn visible_graph_print_elements_and_colors() {
    struct ColorByHead;
    impl GraphColorManager<String> for ColorByHead {
        fn get_color(
            &self,
            head_commit: &String,
            head_fragment_index: i32,
            fragment_index: i32,
        ) -> i32 {
            let head = i32::from_str_radix(head_commit, 16).unwrap();
            head * 10000 + head_fragment_index * 100 + fragment_index
        }
    }
    let factory = GraphColorGetterByHeadFactory::new(Arc::new(ColorByHead));
    let graph = PermanentGraphImpl::new_instance(
        &commits(NOT_FULL_GRAPH),
        &factory,
        |a: &String, b: &String| a.cmp(b),
        &set(&["a0", "a2"]),
    );
    let visible = graph
        .create_visible_graph_sorted(SortType::Normal, None, None)
        .unwrap();

    // row 0: node a0 with its down edges to a1 and a3 and to the not-loaded a6
    let elements = visible.get_row_info(0).print_elements();
    let node = elements.last().unwrap();
    assert!(matches!(node, GraphPrintElement::Node(_)));
    assert_eq!(node.position_in_current_row(), 0);
    // a0 is the head of layout index 1
    assert_eq!(node.color_id(), 0xa0 * 10000 + 100 + 1);
    let down_edges = elements
        .iter()
        .filter(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.edge_print_type() == EdgePrintElementType::Down))
        .count();
    assert_eq!(down_edges, 3);
    assert!(elements.iter().all(|e| !e.is_selected()));
    assert!(visible.recommended_width() >= 1);
}

#[test]
fn long_edge_arrow_actions() {
    // n0 -> n1 -> ... -> n40 and a long edge n0 -> n40
    let mut text = String::from("n0|-n1 n40\n");
    for i in 1..40 {
        text.push_str(&format!("n{i}|-n{}\n", i + 1));
    }
    text.push_str("n40|-\n");
    let graph = permanent_graph(&text, &[]);
    let mut visible = graph
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap();

    // long edges are hidden by default: at row 1 the edge ends in a terminal down arrow
    assert!(visible.action_controller().are_long_edges_hidden());
    let arrow = visible
        .get_row_info(1)
        .print_elements()
        .into_iter()
        .find(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.has_arrow()))
        .expect("arrow in row 1");
    let GraphPrintElement::Edge(edge) = &arrow else {
        unreachable!()
    };
    assert_eq!(edge.edge_print_type(), EdgePrintElementType::Down);
    assert_eq!(edge.position_in_other_row(), edge.position_in_current_row());

    let hover = visible.action_controller().perform_action(GraphAction::new(
        Some(arrow.clone()),
        GraphActionType::MouseOver,
    ));
    assert_eq!(hover.commit_to_jump.as_deref(), Some("n40"));
    assert_eq!(hover.cursor_to_set, Some(Cursor::Hand));
    assert!(!hover.do_jump);
    assert!(hover.is_repaint_required);
    // the hovered arrow is now selected
    assert!(
        visible
            .get_row_info(1)
            .print_elements()
            .iter()
            .any(|e| *e == arrow && e.is_selected())
    );

    let click = visible
        .action_controller()
        .perform_action(GraphAction::new(Some(arrow), GraphActionType::MouseClick));
    assert_eq!(click.commit_to_jump.as_deref(), Some("n40"));
    assert_eq!(click.cursor_to_set, Some(Cursor::Default));
    assert!(click.do_jump);

    // a click on a node goes to the BaseController: default cursor, nothing to jump to
    let node = visible.get_row_info(5).print_elements().pop().unwrap();
    let answer = visible
        .action_controller()
        .perform_action(GraphAction::new(Some(node), GraphActionType::MouseClick));
    assert_eq!(answer.cursor_to_set, Some(Cursor::Default));
    assert!(answer.commit_to_jump.is_none());

    // hidden long edges are not drawn in the middle; shown ones are (with arrows next to the ends)
    let long_edge_parts = |visible: &ig_log::facade::visible_graph_impl::VisibleGraphImpl<
        String,
    >,
                           row| {
        visible
            .get_row_info(row)
            .print_elements()
            .iter()
            .filter(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.graph_edge().down_node_index == Some(40) && edge.graph_edge().up_node_index == Some(0)))
            .count()
    };
    assert_eq!(long_edge_parts(&visible, 20), 0);
    visible.action_controller().set_long_edges_hidden(false);
    assert_eq!(long_edge_parts(&visible, 20), 2);
    assert!(
        visible
            .get_row_info(1)
            .print_elements()
            .iter()
            .any(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.has_arrow()))
    );
}

#[test]
fn unsupported_options_are_errors() {
    let graph = permanent_graph(NOT_FULL_GRAPH, &[]);
    let heads = set(&["a0"]);
    assert!(
        graph
            .create_visible_graph(Options::Base(SortType::Bek), None, None)
            .is_err()
    );
    assert!(
        graph
            .create_visible_graph(Options::LinearBek, None, None)
            .is_err()
    );
    assert!(
        graph
            .create_visible_graph(Options::FirstParent, None, None)
            .is_err()
    );
    assert!(
        graph
            .create_visible_graph(Options::DEFAULT, Some(&heads), None)
            .is_err()
    );
    let error = graph
        .create_visible_graph(Options::DEFAULT, None, Some(&heads))
        .unwrap_err();
    assert!(error.to_string().contains("Plan 2b"), "{error}");
}

#[test]
fn empty_graph() {
    let graph = permanent_graph("", &[]);
    assert!(graph.all_commits().is_empty());
    let visible = graph
        .create_visible_graph(Options::DEFAULT, None, None)
        .unwrap();
    assert_eq!(visible.visible_commit_count(), 0);
    assert_eq!(visible.recommended_width(), 0);
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p ig-log --test permanent_graph`
Expected: `test result: ok. 8 passed; 0 failed`. A failure means the port deviates from upstream. Fix the source, never the expected data.

- [ ] **Step 3: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add crates/ig-log/tests
git commit -m "test(ig-log): facade tests for permanent and visible graph"
```

---
### Task 11: Full verification

- [ ] **Step 1: Run everything**

```bash
cargo fmt --all --check
cargo clippy -q --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ig-git -p ig-log
cargo test --all
for d in $(ls crates/ig-log/tests/fixtures); do diff -rq "$IJ/platform/vcs-log/graph/testData/$d" crates/ig-log/tests/fixtures/$d; done
```
Expected: all clean. `ig-log` runs 142 tests: 104 from this plan's tasks plus 38 added by review follow-ups (`graph_builder_deviations`, `edges_in_row_oracle`, `edge_storage_mutation`, `permanent_graph_behaviour`, `print_element_generator_extra`, `utils_extra` and extra unit tests). `ig-git` stays at 67, and the fixture diff prints nothing.

- [ ] **Step 2: Add the CI doc build for the new crate.** In `.github/workflows/ci.yml`, change `cargo doc --no-deps -p ig-git` to `cargo doc --no-deps -p ig-git -p ig-log`, then commit:

```bash
git add .github/workflows/ci.yml && git commit -m "ci: build ig-log docs"
```
