# IdeaGit Plan 2b — Graph Extras Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the port of upstream's `platform/vcs-log/graph`: BEK ("Standard") and linear BEK ("IntelliJ style") sorting, first-parent, graph-level branch and commit filtering, and collapsing. Every option combination of `create_visible_graph` then works as upstream, and the Log gets fragment hover and click-to-collapse.

**Architecture:**
- **`ig-log`:**
  - New modules: `utils` (BFS, `GraphUtil`, `UnsignedBitSet`, the `IntToIntMap` family, Java's `PriorityQueue`), `facade::sort` (the BEK sort), `collapsing` (the collapsed graph, fragment generators, dotted filter edges, the collapsed and branch-filter controllers), `facade` (the cascade, filtered and first-parent controllers, `GraphModificationUtil`, `SimpleGraphInfo`) and `linear_bek`.
  - `PermanentGraphImpl::create_visible_graph` builds upstream's `createFilteredController` chain and no longer returns `Result`.
- **State:** collapsed and linear-BEK state lives behind `Arc<RwLock<_>>` inside the compiled graph. Updaters mutate it in place and never swap the graph's `Arc`.
- **`ig-app`:** only the Log's graph-action handling changes.

**Tech Stack:** Rust 2024, MSRV 1.95. No new dependencies; `Cargo.lock` is unchanged.

**Spec:** `docs/superpowers/specs/2026-11-10-plan-2b-graph-extras-design.md`. **Previous plans:** 2 (graph core; its "Notes for later plans" decided the controller introspection and mutation model used here), 3 (app shell).

> **Note:** The code blocks below are the verified prototype. Review fixes applied during execution live in the crates and in `reference/upstream-map.toml`, and those are the source of truth.

## How this plan is executed

- **Source of every file:** the prototype branch `wip/graph-extras-prototype`, a local branch in the main repository. Its eight commits are this plan's eight tasks.
  - Each task copies its files from **its own commit** (`git checkout <commit> -- <paths>`), not from the branch tip. A file that several tasks touch is therefore copied in its intermediate form until its last task. The copy is checked with `git diff --cached --exit-code <commit>`.
  - New files are embedded below in full. Changed files are embedded as a diff against the previous task, which is easier to review.
- **Test first:** each task first copies its tests (ported upstream test classes, IdeaGit tests, test support) and runs them red. Then it copies the code and runs them green. Before the code, the red step is a compile error, because the tests reference modules that don't exist yet. Task 8 (docs) has no red step.
- **Simulated:** the task order was replayed on a fresh clone of `master` at the spec commit. At every task, the tree was identical to the prototype commit, and fmt, clippy (with and without the `screenshots` feature), docs and tests were green. The red output and the expected test counts in each task come from that run, on Rust 1.96 and macOS arm64.
- **Build rules:** always pass `--locked`, and never run `cargo update`. On this machine, set `CARGO_NET_GIT_FETCH_WITH_CLI=true`. Never glob-import `gpui` in test modules.
- **Ported tests are acceptance tests.** Their expected values never change. A failing case means the port is wrong.

## Scope

- **In scope:**
  - Every not-yet-ported file under `platform/vcs-log/graph/src`, except `TestGraphs.kt`, which no upstream test uses.
  - `RowsMapping` and the `GraphColorGetterBy*` files were already ported in Plan 2.
  - The app: fragment hover, and click to collapse or expand.
- **Out of scope (the Log UI plan):**
  - The View Options popup: sort type, first-parent, collapse/expand all, long edges.
  - `VcsLogFilterer`, which works out the commits matching a text, user, date or path filter.
  - Details, the changes tree, and file watching.
  - The default stays `Options.Base(Normal)`.

## Tests

| Upstream test class | Rust test file | Tests |
|---|---|---|
| `BfsTests` | `tests/bfs.rs` | 1 |
| `GraphUtilTests` | `tests/graph_util.rs` | 9 |
| `UnsignedBitSetTest` | `tests/unsigned_bit_set.rs` | 8 |
| `ListIntToIntMapTest`, `PermanentListIntToIntMapTest`, `TreeIntToIntMapTest` (each runs `UpdatableIntToIntMapTest`) | `tests/updatable_int_to_int_map.rs` | 33 |
| `BekTest` | `tests/bek.rs` | 4 |
| `DottedFilterEdgesGeneratorTest` | `tests/dotted_filter_edges_generator.rs` | 8, plus `anotherRoot` ignored as upstream |
| `InplaceFilterEdgesGeneratorTest` | `tests/dotted_filter_edges_generator.rs` | 4 |
| `FragmentGeneratorTest` | `tests/fragment_generator.rs` | 28 |
| `FirstParentTest` | `tests/first_parent.rs` | 5 |
| `LinearBekTest` | `tests/linear_bek.rs` | 20 |
| `LinearBekExpandTest` | `tests/linear_bek_expand.rs` | 4 |

IdeaGit-only tests:
- **`tests/graph_options.rs`:** every option × 5 visible-head sets × 4 matched-commit sets, including empty sets and unknown or not-loaded ids. Each visible graph also gets hover and click on every element, stale elements from a bigger graph, collapse/expand with their updaters, and an empty graph. Nothing may panic.
- **`tests/collapsing_controllers.rs`:** hover, click to collapse and expand, collapse/expand all, hidden commits, and stale elements.
- **ig-log facade tests:** the shared BEK map and `SimpleGraphInfo`.
- **UI tests:** fragment hover selects exactly the fragment, and a click collapses it while keeping the row selection.
- **Perf (ignored):** `crates/ig-git/tests/log_perf.rs` builds every option on 50,001 commits. Release timings from the prototype: Normal 0.1 ms; Bek 7.8 ms the first time (BEK map) and 2.4 ms after; LinearBek 6.4 ms; FirstParent 1.7 ms; one visible branch 2.1 ms (Normal) and 8.7 ms (LinearBek); every third commit matched 7.7 ms; fragment hover 0.1 ms; merge hover with LinearBek 1.5 ms.

## Deviations (also recorded in `reference/upstream-map.toml`)

- **Bug fix:** upstream throws when building an unfiltered Normal graph for an empty repository, because `getReachableNodes(null)` calls `set(0, -1)`. The port returns an empty set, and two tests cover it.
- **Mutation model:**
  - Collapsed state is shared behind `Arc<RwLock<_>>`.
  - Holding the write lock during `apply` replaces upstream's `assertNotUnderModification`.
  - A modification dropped without being applied releases the graph. Upstream would refuse every later one.
- **Stale input:** an action element whose node indexes are out of range is treated as "no element". Plan 2 did this only for arrows; upstream throws. Unknown head or matched ids are skipped.
- **Downcasts:**
  - Upstream's `is FilteredController` / `is CascadeController` checks become `LinearGraphController` default methods: `delegate`, `delegate_mut`, `delegate_graph_changed`, `as_any_mut` and `supports_collapse`.
  - `CascadeController` is a trait plus a template function.
- **Logging:** `SortChecker`'s `LOG.error` and BEK's `LOG.assertTrue` become `debug_assert!`s.
- **Deterministic iteration:** where upstream iterates Java or fastutil hash sets, the port uses ascending order. That covers fragment gray nodes, merge-fragment tails, `collectChildFragments`, `expand_nodes` and edge sets. The results don't depend on that order.
- **Comparator ties:** linear BEK's edge comparator breaks ties between edges with no down node with a deterministic hash. Upstream's includes an identity hash; such edges never reach the queue.
- **Java's `PriorityQueue`:** ported as `utils::java_priority_queue`, because the order in which tied edges come out changes the linear BEK result.
- **Shape changes:**
  - The `IntToIntMap` family always returns its own type, with an empty mode in place of upstream's special empty map.
  - The list map owns its flags.
  - BFS takes its visited flags per step.
  - `BekGraphLayout` precomputes its head nodes.
  - `FilteredController` takes a collapsed graph that is already built.
- **Quirks kept and documented:**
  - `hasDottedEdges` always returns false in the down direction.
  - `BfsSearch`'s limit never stops the search.
  - `CollapsedController.convertToDelegate` mixes compiled and delegate indexes.
  - `SimpleGraphInfo` converts branch ids against the source graph.
- **App:** after a click, the Log refreshes its rows as upstream's `handleGraphAnswer` does: a new generation, hover cleared, and the selection kept by commit.

## Task order

1. Utilities.
2. The BEK sort.
3. The collapsed graph and fragment generators. They come before the controllers because `FirstParentTest` needs them.
4. The wrapping controllers.
5. Linear BEK.
6. `PermanentGraphImpl`'s chain and the infallible API.
7. App hover and click.
8. The upstream map.

---

### Task 1: Graph utilities (BFS, GraphUtil, UnsignedBitSet, IntToIntMap family)

**Files:**
- Create: `crates/ig-log/tests/bfs.rs`, `crates/ig-log/tests/graph_util.rs`, `crates/ig-log/tests/unsigned_bit_set.rs`, `crates/ig-log/tests/updatable_int_to_int_map.rs`, `crates/ig-log/src/utils/bfs_util.rs`, `crates/ig-log/src/utils/graph_util.rs`, `crates/ig-log/src/utils/int_hash_set_flags.rs`, `crates/ig-log/src/utils/int_list.rs`, `crates/ig-log/src/utils/int_to_int_map.rs`, `crates/ig-log/src/utils/list_int_to_int_map.rs`, `crates/ig-log/src/utils/permanent_list_int_to_int_map.rs`, `crates/ig-log/src/utils/predicate_flags.rs`, `crates/ig-log/src/utils/tree_int_to_int_map.rs`, `crates/ig-log/src/utils/unsigned_bit_set.rs`
- Modify: `crates/ig-log/tests/common/test_graph_builder.rs`, `crates/ig-log/tests/utils_extra.rs`, `crates/ig-log/src/utils/mod.rs`

**Source commit:** `8da092b` on `wip/graph-extras-prototype`.

**Review focus:** `GraphUtil` / `BfsUtil` against `utils/GraphUtil.kt` and `utils/BfsUtil.kt` (walk order, the `BfsSearch` limit quirk), `UnsignedBitSet` negative indexes, and the `IntToIntMap` family's update semantics (`UpdatableIntToIntMap.update`).

The leaf utilities the rest of the plan builds on. The `test_graph!` macro in the shared test builder mirrors upstream's Kotlin `graph { 1(2, 3) ... }` DSL, so ported tests read like upstream.

- [ ] **Step 1: Copy the tests first**

```bash
git checkout 8da092b -- crates/ig-log/tests/bfs.rs crates/ig-log/tests/common/test_graph_builder.rs crates/ig-log/tests/graph_util.rs crates/ig-log/tests/unsigned_bit_set.rs crates/ig-log/tests/updatable_int_to_int_map.rs crates/ig-log/tests/utils_extra.rs
```

`crates/ig-log/tests/bfs.rs`:

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/BfsTests.kt`.

mod common;

use common::test_graph_builder::test_graph;
use ig_log::api::linear_graph::LinearGraph;
use ig_log::utils::bfs_util::BfsWalk;
use ig_log::utils::flags::{BitSetFlags, Flags};
use ig_log::utils::linear_graph_utils::as_lite_linear_graph;

fn set_all(mut flags: BitSetFlags, values: &[i32]) -> BitSetFlags {
    values.iter().for_each(|&it| flags.set(it, true));
    flags
}

#[test]
fn test_visited() {
    let graph = test_graph! {
        0(3)
        1(3)
        2(4)
        3(4)
        4(5, 6)
        5(7)
        6(8)
        7()
        8()
    };
    let nodes_count = graph.nodes_count();
    let mut visited = BitSetFlags::new(nodes_count);
    let lite = as_lite_linear_graph(&graph);

    BfsWalk::new(0, &lite, true).walk(&mut visited, |_| true);
    assert_eq!(
        set_all(BitSetFlags::new(nodes_count), &[0, 3, 4, 5, 6, 7, 8]),
        visited
    );
    visited.set_all(false);

    BfsWalk::new(3, &lite, true).walk(&mut visited, |_| true);
    assert_eq!(
        set_all(BitSetFlags::new(nodes_count), &[3, 4, 5, 6, 7, 8]),
        visited
    );
    visited.set_all(false);

    BfsWalk::new(5, &lite, true).walk(&mut visited, |_| true);
    assert_eq!(set_all(BitSetFlags::new(nodes_count), &[5, 7]), visited);
    visited.set_all(false);

    BfsWalk::new(8, &lite, true).walk(&mut visited, |_| true);
    assert_eq!(set_all(BitSetFlags::new(nodes_count), &[8]), visited);
    visited.set_all(false);
}
```

`crates/ig-log/tests/common/test_graph_builder.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/tests/common/test_graph_builder.rs b/crates/ig-log/tests/common/test_graph_builder.rs
index cae36df..24cd582 100644
--- a/crates/ig-log/tests/common/test_graph_builder.rs
+++ b/crates/ig-log/tests/common/test_graph_builder.rs
@@ -2,8 +2,9 @@
 //!
 //! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/TestGraphBuilder.kt`
 //! (`BaseTestGraphBuilder`, `TestGraphBuilder`, `graph { }`). Kotlin's `1(2, 3.dot)` becomes
-//! `b.node(1, [u(2), dot(3)])`; `2.UNM(3)` becomes `b.node(unm(2), [u(3)])`.
-//! `asTestGraphString` and `TestPermanentGraphInfo` are not needed by the ported tests yet.
+//! `b.node(1, [u(2), dot(3)])`; `2.UNM(3)` becomes `b.node(unm(2), [u(3)])`. The
+//! [`test_graph!`](crate::common::test_graph_builder::test_graph) macro accepts the Kotlin syntax
+//! itself (`test_graph! { 1(2, 3.dot) 2.UNM(3) 3() }`).
 
 use std::collections::HashMap;
 
@@ -113,6 +114,69 @@ impl TestGraphBuilder {
     }
 }
 
+/// `graph { ... }` with the Kotlin DSL's syntax: `0(1, 2.dot)`, `2.UNM(3)`, `4.NOT_LOAD()`, `3()`.
+/// Edges are `n` / `n.u` (usual) and `n.dot` (dotted).
+#[allow(unused_macros)]
+macro_rules! test_graph {
+    ($($nodes:tt)*) => {{
+        #[allow(unused_mut)]
+        let mut builder = $crate::common::test_graph_builder::TestGraphBuilder::default();
+        $crate::common::test_graph_builder::test_graph_nodes!(builder; $($nodes)*);
+        builder.done()
+    }};
+}
+#[allow(unused_imports)]
+pub(crate) use test_graph;
+
+#[allow(unused_macros)]
+macro_rules! test_graph_nodes {
+    ($b:ident;) => {};
+    ($b:ident; $id:literal . UNM ($($edges:tt)*) $($rest:tt)*) => {
+        $b.node(
+            $crate::common::test_graph_builder::unm($id),
+            $crate::common::test_graph_builder::test_graph_edges!($($edges)*),
+        );
+        $crate::common::test_graph_builder::test_graph_nodes!($b; $($rest)*);
+    };
+    ($b:ident; $id:literal . NOT_LOAD ($($edges:tt)*) $($rest:tt)*) => {
+        $b.node(
+            $crate::common::test_graph_builder::not_load_node($id),
+            $crate::common::test_graph_builder::test_graph_edges!($($edges)*),
+        );
+        $crate::common::test_graph_builder::test_graph_nodes!($b; $($rest)*);
+    };
+    ($b:ident; $id:literal ($($edges:tt)*) $($rest:tt)*) => {
+        $b.node(
+            $id,
+            $crate::common::test_graph_builder::test_graph_edges!($($edges)*),
+        );
+        $crate::common::test_graph_builder::test_graph_nodes!($b; $($rest)*);
+    };
+}
+#[allow(unused_imports)]
+pub(crate) use test_graph_nodes;
+
+#[allow(unused_macros)]
+macro_rules! test_graph_edges {
+    () => {
+        ::std::vec::Vec::<$crate::common::test_graph_builder::SimpleEdge>::new()
+    };
+    ($($id:literal $(. $kind:ident)?),+ $(,)?) => {
+        ::std::vec![$($crate::common::test_graph_builder::test_graph_edges!(@edge $id $(. $kind)?)),+]
+    };
+    (@edge $id:literal) => {
+        $crate::common::test_graph_builder::u($id)
+    };
+    (@edge $id:literal . u) => {
+        $crate::common::test_graph_builder::u($id)
+    };
+    (@edge $id:literal . dot) => {
+        $crate::common::test_graph_builder::dot($id)
+    };
+}
+#[allow(unused_imports)]
+pub(crate) use test_graph_edges;
+
 /// `graph { ... }`
 pub fn graph(f: impl FnOnce(&mut TestGraphBuilder)) -> TestLinearGraph {
     let mut builder = TestGraphBuilder::default();
```

`crates/ig-log/tests/graph_util.rs`:

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/GraphUtilTests.kt`.

mod common;

use std::collections::HashSet;

use common::str_utils::graph_as_string;
use common::test_graph_builder::{TestLinearGraph, test_graph};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::utils::flags::BitSetFlags;
use ig_log::utils::graph_util::{exclusive_nodes, get_corresponding_parent, subgraph_difference};
use ig_log::utils::linear_graph_utils::as_lite_linear_graph;

fn assert_corresponding_parent(
    start_node: i32,
    end_node: i32,
    expected_parent: i32,
    graph: TestLinearGraph,
) {
    let actual_parent = get_corresponding_parent(
        &as_lite_linear_graph(&graph),
        start_node,
        end_node,
        &mut BitSetFlags::new(graph.nodes_count()),
    );
    assert_eq!(
        actual_parent,
        expected_parent,
        "Incorrect parent found when walking from {start_node} to {end_node} in {}",
        graph_as_string(&graph, true)
    );
}

#[test]
fn one_parent() {
    assert_corresponding_parent(
        0,
        5,
        1,
        test_graph! {
            0(1)
            1(2)
            2(3, 4)
            3(5)
            4(5)
            5()
        },
    );
}

#[test]
fn target_parent() {
    assert_corresponding_parent(
        0,
        2,
        2,
        test_graph! {
            0(1, 2)
            1(3)
            2(3)
            3()
        },
    );
}

#[test]
fn merge_commit() {
    assert_corresponding_parent(
        0,
        4,
        2,
        test_graph! {
            0(1, 2)
            1(3)
            2(3, 4)
            3()
            4()
        },
    );
}

fn assert_subgraph_difference(
    node1: i32,
    node2: i32,
    expected_difference: HashSet<i32>,
    graph: TestLinearGraph,
) {
    let actual_difference = subgraph_difference(&graph, node1, node2);
    assert_eq!(
        actual_difference,
        expected_difference,
        "Incorrect subgraph difference calculated between {node1} to {node2} in {}",
        graph_as_string(&graph, true)
    );
}

fn int_set_of(elements: &[i32]) -> HashSet<i32> {
    elements.iter().copied().collect()
}

/*
0
| 1
2 |
| 3
4 |
| 5
6 |
|/
7
8
9
*/
#[test]
fn single_branches() {
    assert_subgraph_difference(
        0,
        1,
        int_set_of(&[0, 2, 4, 6]),
        test_graph! {
            0(2)
            1(3)
            2(4)
            3(5)
            4(6)
            5(7)
            6(7)
            7(8)
            8(9)
            9()
        },
    );
}

/*
0
|\
| 1
2 |\
| | 3
| 4 |
5 |/
| 6
|/
7
8
*/
#[test]
fn ancestor() {
    assert_subgraph_difference(
        0,
        3,
        int_set_of(&[0, 1, 2, 4, 5]),
        test_graph! {
            0(1, 2)
            1(3, 4)
            2(5)
            3(6)
            4(6)
            5(7)
            6(7)
            7(8)
            8()
        },
    );
}

/*
 0
 |\
 | 1
 2 |\
 | | 3
 | 4 |
 5 |/
 | 6
 |/
 7
 8
*/
#[test]
fn descendant() {
    assert_subgraph_difference(
        3,
        0,
        HashSet::new(),
        test_graph! {
            0(1, 2)
            1(3, 4)
            2(5)
            3(6)
            4(6)
            5(7)
            6(7)
            7(8)
            8()
        },
    );
}

/*
0
|  1
2  |
|\ 3
| \|\
|  4 |
|  | 5
|  |/
| /|
|/ |
6  |
|  7
8 /
|/
9
*/
#[test]
fn multiple_bases() {
    assert_subgraph_difference(
        0,
        1,
        int_set_of(&[0, 2]),
        test_graph! {
            0(2)
            1(3)
            2(4, 6)
            3(4, 5)
            4(7)
            5(6)
            6(8)
            7(9)
            8(9)
            9()
        },
    );
}

fn assert_exclusive_nodes(
    node: i32,
    expected_exclusive_nodes: HashSet<i32>,
    other_heads: HashSet<i32>,
    graph: &TestLinearGraph,
) {
    let actual_exclusive_nodes = exclusive_nodes(graph, node, |n| other_heads.contains(&n));
    assert_eq!(
        actual_exclusive_nodes,
        expected_exclusive_nodes,
        "Incorrect exclusive nodes for {node} {}",
        graph_as_string(graph, true)
    );
}

/*
0
| 1
2 |
| 3
4 |
| 5
6 |
|/
7
8
9
*/
#[test]
fn simple_branch() {
    let graph = test_graph! {
        0(2)
        1(3)
        2(4)
        3(5)
        4(6)
        5(7)
        6(7)
        7(8)
        8(9)
        9()
    };
    assert_exclusive_nodes(0, int_set_of(&[0, 2, 4, 6]), HashSet::new(), &graph);
    assert_exclusive_nodes(0, int_set_of(&[0, 2]), int_set_of(&[0, 1, 4]), &graph);
    assert_exclusive_nodes(1, int_set_of(&[1, 3, 5]), HashSet::new(), &graph);
    assert_exclusive_nodes(1, int_set_of(&[1, 3]), int_set_of(&[0, 1, 5]), &graph);
}

/*
0
|   1
2   |
|\  |
3 | |
| 4 |
|/  5
6  /
| /
7
8
*/
#[test]
fn branch_with_merge_commit() {
    let graph = test_graph! {
        0(2)
        1(5)
        2(3, 4)
        3(6)
        4(6)
        5(7)
        6(7)
        7(8)
        8()
    };
    assert_exclusive_nodes(0, int_set_of(&[0, 2, 3, 4, 6]), HashSet::new(), &graph);
    assert_exclusive_nodes(0, int_set_of(&[0, 2, 4]), int_set_of(&[0, 1, 3]), &graph);
    assert_exclusive_nodes(1, int_set_of(&[1, 5]), HashSet::new(), &graph);
    assert_exclusive_nodes(1, int_set_of(&[1]), int_set_of(&[0, 1, 5]), &graph);
}
```

`crates/ig-log/tests/unsigned_bit_set.rs`:

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/UnsignedBitSetTest.java`.

use ig_log::utils::unsigned_bit_set::UnsignedBitSet;

fn to_str(bit_set: &UnsignedBitSet) -> String {
    (-5..=5)
        .map(|i| if bit_set.get(i) { '1' } else { '0' })
        .collect()
}

#[test]
fn init_false() {
    let bit_set = UnsignedBitSet::new();
    assert_eq!("00000000000", to_str(&bit_set));
}

#[test]
fn set_zero() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set(0, true);
    assert_eq!("00000100000", to_str(&bit_set));
}

#[test]
fn set_one() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set(1, true);
    assert_eq!("00000010000", to_str(&bit_set));
}

#[test]
fn set_minus_one() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set(-1, true);
    assert_eq!("00001000000", to_str(&bit_set));
}

#[test]
fn set_several_times() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set(3, true);
    bit_set.set(-3, true);
    bit_set.set(-2, true);
    assert_eq!("00110000100", to_str(&bit_set));
}

#[test]
fn set_positive_range() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set_range(0, 4, true);
    assert_eq!("00000111110", to_str(&bit_set));
}

#[test]
fn set_negative_range() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set_range(-3, -1, true);
    assert_eq!("00111000000", to_str(&bit_set));
}

#[test]
fn set_range() {
    let mut bit_set = UnsignedBitSet::new();
    bit_set.set_range(-3, 1, true);
    assert_eq!("00111110000", to_str(&bit_set));
}
```

`crates/ig-log/tests/updatable_int_to_int_map.rs`:

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/UpdatableIntToIntMapTest.java`
//! and its subclasses `ListIntToIntMapTest.java`, `PermanentListIntToIntMapTest.java` and
//! `TreeIntToIntMapTest.java`: [`updatable_int_to_int_map_tests!`] expands the abstract test class
//! once per map.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use ig_log::utils::flags::Flags;
use ig_log::utils::int_to_int_map::{IntToIntMap, UpdatableIntToIntMap};
use ig_log::utils::list_int_to_int_map::ListIntToIntMap;
use ig_log::utils::permanent_list_int_to_int_map::PermanentListIntToIntMap;
use ig_log::utils::predicate_flags::PredicateFlags;
use ig_log::utils::tree_int_to_int_map::TreeIntToIntMap;

type Visible = Rc<RefCell<HashSet<i32>>>;

/// `Integer.decode` of `|`-separated numbers.
fn parse_set(visibility: &str) -> HashSet<i32> {
    if visibility.is_empty() {
        return HashSet::new();
    }
    visibility
        .split('|')
        .map(|number| number.parse().unwrap())
        .collect()
}

struct Tester {
    visible_nodes: Visible,
    updatable_int_to_int_map: Box<dyn UpdatableIntToIntMap>,
}

impl Tester {
    fn change_visibility(&mut self, update_from: i32, update_to: i32, new_visibility: &str) {
        *self.visible_nodes.borrow_mut() = parse_set(new_visibility);

        self.updatable_int_to_int_map.update(update_from, update_to);

        assert_eq!(new_visibility, self.map_to_string());
    }

    fn map_to_string(&self) -> String {
        (0..self.updatable_int_to_int_map.short_size())
            .map(|short_index| {
                self.updatable_int_to_int_map
                    .get_long_index(short_index)
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    fn reverse_map_to_string(&self) -> String {
        (0..self.updatable_int_to_int_map.long_size())
            .map(|long_index| {
                self.updatable_int_to_int_map
                    .get_short_index(long_index)
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    fn test_long_to_short(&self, expected: &str) {
        assert_eq!(expected, self.reverse_map_to_string());
    }
}

fn get_test(
    create_updatable_int_to_int_map: fn(Visible, i32) -> Box<dyn UpdatableIntToIntMap>,
    long_size: i32,
    init_visibility: &str,
) -> Tester {
    let visible_nodes = Rc::new(RefCell::new(parse_set(init_visibility)));
    let updatable_int_to_int_map =
        create_updatable_int_to_int_map(visible_nodes.clone(), long_size);
    let tester = Tester {
        visible_nodes,
        updatable_int_to_int_map,
    };

    assert_eq!(init_visibility, tester.map_to_string());
    tester
}

fn is_visible(visible_nodes: Visible) -> impl Fn(i32) -> bool + Clone {
    move |integer| visible_nodes.borrow().contains(&integer)
}

/// `UpdatableIntToIntMapTest`'s test methods for one `createUpdatableIntToIntMap`.
macro_rules! updatable_int_to_int_map_tests {
    ($create:expr) => {
        use super::*;

        fn get_test(long_size: i32, init_visibility: &str) -> Tester {
            super::get_test($create, long_size, init_visibility)
        }

        #[test]
        fn simple_test() {
            let mut tester = get_test(6, "0|1|2|3|4|5");

            tester.change_visibility(1, 3, "0|2|4|5");
            tester.change_visibility(2, 2, "0|4|5");
            tester.change_visibility(1, 1, "0|1|4|5");
        }

        #[test]
        fn test_one_node() {
            let mut tester = get_test(1, "0");

            tester.change_visibility(0, 0, "");
            tester.change_visibility(0, 0, "");
            tester.change_visibility(0, 0, "0");
            tester.change_visibility(0, 0, "0");
            tester.change_visibility(0, 0, "");
        }

        #[test]
        fn test_two_nodes() {
            let mut tester = get_test(2, "0");

            tester.change_visibility(1, 1, "0|1");
            tester.change_visibility(0, 1, "");
            tester.change_visibility(0, 0, "0");
        }

        #[test]
        fn test4_nodes() {
            let mut tester = get_test(4, "2|3");

            tester.change_visibility(1, 3, "1");
            tester.change_visibility(0, 2, "0|1|2");
            tester.change_visibility(0, 0, "1|2");
        }

        #[test]
        fn test5_nodes() {
            let mut tester = get_test(5, "0|1|2|3|4");

            tester.change_visibility(4, 4, "0|1|2|3");
            tester.change_visibility(3, 4, "0|1|2|4");
        }

        #[test]
        fn test_reverse_map() {
            let mut tester = get_test(7, "0|1|2|3|4|5|6");

            tester.test_long_to_short("0|1|2|3|4|5|6");

            tester.change_visibility(2, 4, "0|1|5|6");

            tester.test_long_to_short("0|1|1|1|1|2|3");
        }

        #[test]
        fn test_reverse_map2() {
            let mut tester = get_test(8, "0|4|7");

            tester.test_long_to_short("0|0|0|0|1|1|1|2");

            tester.change_visibility(0, 0, "4|7");

            tester.test_long_to_short("0|0|0|0|0|0|0|1");
        }

        #[test]
        fn test_reverse_with_min_nodes() {
            let mut tester = get_test(1, "");

            tester.test_long_to_short("0");

            tester.change_visibility(0, 0, "0");

            tester.test_long_to_short("0");
        }

        #[test]
        fn test_reverse_with_min_nodes2() {
            let mut tester = get_test(2, "");

            tester.test_long_to_short("0|0");

            tester.change_visibility(1, 1, "1");

            tester.test_long_to_short("0|0");

            tester.change_visibility(0, 0, "0|1");

            tester.test_long_to_short("0|1");
        }

        #[test]
        fn empty_test() {
            let tester = get_test(0, "");
            tester.test_long_to_short("");
        }

        #[test]
        fn block_size_test() {
            get_test(0, "").test_long_to_short("");
            get_test(1, "0").test_long_to_short("0");
            get_test(2, "0|1").test_long_to_short("0|1");
            get_test(3, "0|1|2").test_long_to_short("0|1|2");
            get_test(4, "0|1|2|3").test_long_to_short("0|1|2|3");
            get_test(5, "0|1|2|3|4").test_long_to_short("0|1|2|3|4");
            get_test(6, "0|1|2|3|4|5").test_long_to_short("0|1|2|3|4|5");
            get_test(7, "0|1|2|3|4|5|6").test_long_to_short("0|1|2|3|4|5|6");
        }
    };
}

/// `ListIntToIntMapTest`
mod list_int_to_int_map_test {
    updatable_int_to_int_map_tests!(|this_is_visible, long_size| {
        Box::new(ListIntToIntMap::with_block_size(
            PredicateFlags::new(is_visible(this_is_visible), long_size),
            3,
        ))
    });
}

/// `TreeIntToIntMapTest`
mod tree_int_to_int_map_test {
    updatable_int_to_int_map_tests!(|this_is_visible, long_size| {
        Box::new(TreeIntToIntMap::new_instance(
            is_visible(this_is_visible),
            long_size,
        ))
    });
}

/// `PermanentListIntToIntMapTest`
mod permanent_list_int_to_int_map_test {
    updatable_int_to_int_map_tests!(|this_is_visible, long_size| {
        Box::new(UpdatableIntToIntMapWrapper::new(TestFlags {
            long_size,
            this_is_visible: is_visible(this_is_visible),
        }))
    });
}

/// The anonymous `Flags` of `PermanentListIntToIntMapTest.createUpdatableIntToIntMap`.
#[derive(Clone)]
struct TestFlags<P> {
    long_size: i32,
    this_is_visible: P,
}

impl<P: Fn(i32) -> bool> Flags for TestFlags<P> {
    fn size(&self) -> i32 {
        self.long_size
    }

    fn get(&self, index: i32) -> bool {
        (self.this_is_visible)(index)
    }

    fn set(&mut self, _index: i32, _value: bool) {
        unimplemented!()
    }

    fn set_all(&mut self, _value: bool) {
        unimplemented!()
    }
}

/// `PermanentListIntToIntMapTest.UpdatableIntToIntMapWrapper`: rebuilds the map on every update.
struct UpdatableIntToIntMapWrapper<P> {
    flags: TestFlags<P>,
    int_to_int_map: PermanentListIntToIntMap<TestFlags<P>>,
}

impl<P: Fn(i32) -> bool + Clone> UpdatableIntToIntMapWrapper<P> {
    fn new(flags: TestFlags<P>) -> Self {
        let int_to_int_map = Self::create_int_to_int_map(&flags);
        UpdatableIntToIntMapWrapper {
            flags,
            int_to_int_map,
        }
    }

    fn create_int_to_int_map(flags: &TestFlags<P>) -> PermanentListIntToIntMap<TestFlags<P>> {
        let mut short_size = 0;
        for i in 0..flags.size() {
            if flags.get(i) {
                short_size += 1;
            }
        }
        PermanentListIntToIntMap::with_block_size(flags.clone(), short_size, 2)
    }
}

impl<P: Fn(i32) -> bool + Clone> UpdatableIntToIntMap for UpdatableIntToIntMapWrapper<P> {
    fn update(&mut self, _start_long_index: i32, _end_long_index: i32) {
        self.int_to_int_map = Self::create_int_to_int_map(&self.flags);
    }
}

impl<P: Fn(i32) -> bool + Clone> IntToIntMap for UpdatableIntToIntMapWrapper<P> {
    fn short_size(&self) -> i32 {
        self.int_to_int_map.short_size()
    }

    fn long_size(&self) -> i32 {
        self.int_to_int_map.long_size()
    }

    fn get_long_index(&self, short_index: i32) -> i32 {
        self.int_to_int_map.get_long_index(short_index)
    }

    fn get_short_index(&self, long_index: i32) -> i32 {
        self.int_to_int_map.get_short_index(long_index)
    }
}
```

`crates/ig-log/tests/utils_extra.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/tests/utils_extra.rs b/crates/ig-log/tests/utils_extra.rs
index bec788e..87c9726 100644
--- a/crates/ig-log/tests/utils_extra.rs
+++ b/crates/ig-log/tests/utils_extra.rs
@@ -1,4 +1,4 @@
-//! IdeaGit tests for `java_sort` and `DfsWalk`. Upstream has no tests for these helpers.
+//! IdeaGit tests for `java_sort`, `DfsWalk` and `graph_util`. Upstream has no tests for these cases.
 
 use std::cmp::Ordering;
 
@@ -96,3 +96,33 @@ fn dfs_false_on_start_node_ends_whole_walk() {
     });
     assert_eq!(seen, vec![0]);
 }
+
+/// Bug fix (IdeaGit): upstream's `getReachableNodes(null)` throws for an empty graph.
+#[test]
+fn reachable_nodes_of_an_empty_graph() {
+    use ig_log::api::EdgeFilter;
+    use ig_log::api::elements::{GraphEdge, GraphNode};
+    use ig_log::api::linear_graph::LinearGraph;
+    use ig_log::utils::graph_util::get_reachable_nodes;
+
+    struct Empty;
+    impl LinearGraph for Empty {
+        fn nodes_count(&self) -> i32 {
+            0
+        }
+        fn get_adjacent_edges(&self, _: i32, _: EdgeFilter) -> Vec<GraphEdge> {
+            unreachable!()
+        }
+        fn get_graph_node(&self, _: i32) -> GraphNode {
+            unreachable!()
+        }
+        fn get_node_id(&self, _: i32) -> i32 {
+            unreachable!()
+        }
+        fn get_node_index(&self, _: i32) -> Option<i32> {
+            unreachable!()
+        }
+    }
+    let visibility = get_reachable_nodes(&Empty, None);
+    assert!(!visibility.get(0));
+}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -q --locked -p ig-log --test bfs --test graph_util --test unsigned_bit_set --test updatable_int_to_int_map --test utils_extra
```

Expected: FAIL — the new modules don't exist yet, so the test crates fail to compile (`unresolved import ig_log::utils::...`). The simulation's first error:

```text
error[E0432]: unresolved import `ig_log::utils::bfs_util`
```

- [ ] **Step 3: Copy the code**

```bash
git checkout 8da092b -- crates/ig-log/src/utils/bfs_util.rs crates/ig-log/src/utils/graph_util.rs crates/ig-log/src/utils/int_hash_set_flags.rs crates/ig-log/src/utils/int_list.rs crates/ig-log/src/utils/int_to_int_map.rs crates/ig-log/src/utils/list_int_to_int_map.rs crates/ig-log/src/utils/mod.rs crates/ig-log/src/utils/permanent_list_int_to_int_map.rs crates/ig-log/src/utils/predicate_flags.rs crates/ig-log/src/utils/tree_int_to_int_map.rs crates/ig-log/src/utils/unsigned_bit_set.rs
```

`crates/ig-log/src/utils/bfs_util.rs`:

```rust
//! Breadth-first walks and searches.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/BfsUtil.kt`.
//!
//! Deviations:
//! - The visited [`Flags`] are passed to [`BfsWalk::step`] / [`BfsWalk::walk`] instead of being
//!   captured by the walk: `GraphUtil.subgraphDifference` uses visited flags that read the state of
//!   another walk, which Rust's borrows only allow when the flags are built per step. The upstream
//!   constructor without flags (a fresh `BitSetFlags`) is the caller's `BitSetFlags::new`.
//! - The overridable queue is a type parameter ([`BfsQueue`]): a FIFO [`VecDeque`] by default, or
//!   [`ReversePriorityQueue`] (`PriorityQueue(Collections.reverseOrder())`, used by linear BEK).
//! - [`BfsSearch`] is a struct holding a [`BfsWalk`] rather than a subclass.
//!
//! Upstream quirk kept: [`BfsSearch`]'s `count` only grows when a node is found, and finding a
//! node ends the search, so the `limit` never stops it.

use std::collections::{BinaryHeap, VecDeque};

use crate::api::linear_graph::{LiteLinearGraph, NodeFilter};
use crate::utils::flags::Flags;

/// The queue of a [`BfsWalk`] (upstream `Queue<Int>`).
pub trait BfsQueue {
    fn poll(&mut self) -> Option<i32>;

    fn add_all(&mut self, nodes: &[i32]);

    fn is_empty(&self) -> bool;

    fn contains(&self, node: i32) -> bool;

    fn min_node(&self) -> Option<i32>;

    fn max_node(&self) -> Option<i32>;
}

/// FIFO queue (`ContainerUtil.newLinkedList`).
impl BfsQueue for VecDeque<i32> {
    fn poll(&mut self) -> Option<i32> {
        self.pop_front()
    }

    fn add_all(&mut self, nodes: &[i32]) {
        self.extend(nodes.iter().copied());
    }

    fn is_empty(&self) -> bool {
        VecDeque::is_empty(self)
    }

    fn contains(&self, node: i32) -> bool {
        VecDeque::contains(self, &node)
    }

    fn min_node(&self) -> Option<i32> {
        self.iter().copied().min()
    }

    fn max_node(&self) -> Option<i32> {
        self.iter().copied().max()
    }
}

/// `PriorityQueue(Collections.reverseOrder())`: polls the greatest node first. Equal `i32`s are
/// indistinguishable, so the order matches Java's heap exactly.
#[derive(Clone, Debug, Default)]
pub struct ReversePriorityQueue(BinaryHeap<i32>);

impl ReversePriorityQueue {
    pub fn new(start: i32) -> ReversePriorityQueue {
        ReversePriorityQueue(BinaryHeap::from([start]))
    }
}

impl BfsQueue for ReversePriorityQueue {
    fn poll(&mut self) -> Option<i32> {
        self.0.pop()
    }

    fn add_all(&mut self, nodes: &[i32]) {
        self.0.extend(nodes.iter().copied());
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn contains(&self, node: i32) -> bool {
        self.0.iter().any(|&n| n == node)
    }

    fn min_node(&self) -> Option<i32> {
        self.0.iter().copied().min()
    }

    fn max_node(&self) -> Option<i32> {
        self.0.peek().copied()
    }
}

/// A breadth-first walk from `start`, down (to parents, smallest first) or up (to children,
/// greatest first).
pub struct BfsWalk<'g, G: LiteLinearGraph + ?Sized, Q: BfsQueue = VecDeque<i32>> {
    pub start: i32,
    graph: &'g G,
    down: bool,
    queue: Q,
}

impl<'g, G: LiteLinearGraph + ?Sized> BfsWalk<'g, G> {
    pub fn new(start: i32, graph: &'g G, down: bool) -> BfsWalk<'g, G> {
        BfsWalk::with_queue(start, graph, down, VecDeque::from([start]))
    }
}

impl<'g, G: LiteLinearGraph + ?Sized, Q: BfsQueue> BfsWalk<'g, G, Q> {
    /// A walk with a custom queue, which must contain `start`.
    pub fn with_queue(start: i32, graph: &'g G, down: bool, queue: Q) -> BfsWalk<'g, G, Q> {
        BfsWalk {
            start,
            graph,
            down,
            queue,
        }
    }

    pub fn is_finished(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn current_nodes(&self) -> &Q {
        &self.queue
    }

    /// Visits the next unvisited node and queues its neighbours, which it returns. Returns an empty
    /// list if the queue ran out or `consumer` returned `false` for the node.
    pub fn step(
        &mut self,
        visited: &mut dyn Flags,
        mut consumer: impl FnMut(i32) -> bool,
    ) -> Vec<i32> {
        while let Some(node) = self.queue.poll() {
            if !visited.get(node) {
                visited.set(node, true);
                if !consumer(node) {
                    return Vec::new();
                }
                let mut next = if self.down {
                    self.graph.get_nodes(node, NodeFilter::Down)
                } else {
                    self.graph.get_nodes(node, NodeFilter::Up)
                };
                if self.down {
                    next.sort_unstable();
                } else {
                    next.sort_unstable_by(|a, b| b.cmp(a));
                }
                self.queue.add_all(&next);
                return next;
            }
        }
        Vec::new()
    }

    pub fn walk(&mut self, visited: &mut dyn Flags, mut consumer: impl FnMut(i32) -> bool) {
        while !self.is_finished() {
            self.step(visited, &mut consumer);
        }
    }
}

/// A breadth-first search for the first node that `consumer` maps to a value.
pub struct BfsSearch<'g, G: LiteLinearGraph + ?Sized, Q: BfsQueue = VecDeque<i32>> {
    walk: BfsWalk<'g, G, Q>,
    limit: i32,
    pub count: i32,
}

impl<'g, G: LiteLinearGraph + ?Sized, Q: BfsQueue> BfsSearch<'g, G, Q> {
    /// `limit` defaults to `graph.nodes_count()` upstream.
    pub fn new(walk: BfsWalk<'g, G, Q>, limit: i32) -> BfsSearch<'g, G, Q> {
        BfsSearch {
            walk,
            limit,
            count: 0,
        }
    }

    pub fn find<T>(
        &mut self,
        visited: &mut dyn Flags,
        mut consumer: impl FnMut(i32) -> Option<T>,
    ) -> Option<T> {
        self.count = 0;
        let mut result = None;
        let mut count = 0;
        while !(result.is_some() || self.walk.is_finished() || count > self.limit) {
            self.walk.step(visited, |node| match consumer(node) {
                None => true,
                Some(value) => {
                    result = Some(value);
                    count += 1;
                    false
                }
            });
        }
        self.count = count;
        result
    }
}
```

`crates/ig-log/src/utils/graph_util.rs`:

```rust
//! Reachability and ancestry queries on graphs.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/GraphUtil.kt`.
//!
//! Deviations:
//! - Bug fix: [`get_reachable_matching_nodes`] of an empty graph without start or matched nodes
//!   returns an empty set. Upstream calls `UnsignedBitSet.set(0, -1, true)`, which throws, so an
//!   unfiltered `CollapsedController` over an empty graph (an empty repository) cannot be built.
//! - Kotlin's two `exclusiveNodes` overloads are [`lite_exclusive_nodes`] (`LiteLinearGraph`) and
//!   [`exclusive_nodes`] (`LinearGraph`); `IntSet`s are `HashSet<i32>`.

use std::collections::HashSet;

use crate::api::linear_graph::{LinearGraph, LiteLinearGraph, NodeFilter};
use crate::utils::bfs_util::{BfsQueue, BfsWalk};
use crate::utils::dfs_util::{DfsWalk, EXIT, NODE_NOT_FOUND, walk};
use crate::utils::flags::{BitSetFlags, Flags};
use crate::utils::int_hash_set_flags::IntHashSetFlags;
use crate::utils::linear_graph_utils::as_lite_linear_graph;
use crate::utils::unsigned_bit_set::UnsignedBitSet;

/// Nodes reachable from `start_nodes` by down edges, or all nodes if `start_nodes` is `None`.
pub fn get_reachable_nodes(
    graph: &(impl LinearGraph + ?Sized),
    start_nodes: Option<&HashSet<i32>>,
) -> UnsignedBitSet {
    get_reachable_matching_nodes(graph, start_nodes, None)
}

/// Matching nodes reachable from `start_nodes` by down edges, or all matching nodes if
/// `start_nodes` is `None`. `matched_nodes == None` means every node matches.
pub fn get_reachable_matching_nodes(
    graph: &(impl LinearGraph + ?Sized),
    start_nodes: Option<&HashSet<i32>>,
    matched_nodes: Option<&HashSet<i32>>,
) -> UnsignedBitSet {
    let mut visibility = UnsignedBitSet::new();
    let Some(start_nodes) = start_nodes else {
        match matched_nodes {
            // deviation: upstream throws for an empty graph (set(0, -1))
            None if graph.nodes_count() > 0 => {
                visibility.set_range(0, graph.nodes_count() - 1, true)
            }
            None => {}
            Some(matched_nodes) => {
                for &matched_id in matched_nodes {
                    visibility.set(matched_id, true);
                }
            }
        }
        return visibility;
    };

    let lite = as_lite_linear_graph(graph);
    let mut visited = BitSetFlags::new(graph.nodes_count());
    DfsWalk::new(start_nodes.iter().copied(), &lite, &mut visited).walk(true, |node| {
        if matched_nodes.is_none_or(|matched| matched.contains(&node)) {
            visibility.set(node, true);
        }
        true
    });
    visibility
}

/// Whether `lower_node` is an ancestor of `upper_node`.
pub fn is_ancestor(
    graph: &(impl LiteLinearGraph + ?Sized),
    lower_node: i32,
    upper_node: i32,
) -> bool {
    let mut visited = BitSetFlags::with_default(graph.nodes_count(), false);

    let mut result = false;
    walk(lower_node, |current_node| {
        visited.set(current_node, true);

        if current_node == upper_node {
            result = true;
            return EXIT;
        }
        if current_node > upper_node {
            for next_node in graph.get_nodes(current_node, NodeFilter::Up) {
                if !visited.get(next_node) {
                    return next_node;
                }
            }
        }

        NODE_NOT_FOUND
    });

    result
}

/// A parent of `start_node` on the path to `end_node`; the first parent if there is no path.
pub fn get_corresponding_parent(
    graph: &(impl LiteLinearGraph + ?Sized),
    start_node: i32,
    end_node: i32,
    visited: &mut dyn Flags,
) -> i32 {
    let candidates = graph.get_nodes(start_node, NodeFilter::Down);
    if candidates.len() == 1 {
        return candidates[0];
    }
    if candidates.contains(&end_node) {
        return end_node;
    }

    let mut bfs_walks: Vec<_> = candidates
        .iter()
        .map(|&candidate| BfsWalk::new(candidate, graph, true))
        .collect();

    visited.set_all(false);
    loop {
        for walk in &mut bfs_walks {
            if walk.step(visited, |_| true).contains(&end_node) {
                return walk.start;
            }
        }
        bfs_walks.retain(|walk| !walk.is_finished());
        if bfs_walks.is_empty() {
            break;
        }
    }

    candidates[0]
}

/// `visited1` of `subgraphDifference`: also counts nodes the second walk visited or queued.
struct Visited1<'a, Q: BfsQueue> {
    data: &'a mut IntHashSetFlags,
    visited2: &'a BitSetFlags,
    queue2: &'a Q,
}

impl<Q: BfsQueue> Flags for Visited1<'_, Q> {
    fn size(&self) -> i32 {
        self.data.size()
    }

    fn get(&self, index: i32) -> bool {
        self.data.get(index) || self.visited2.get(index) || self.queue2.contains(index)
    }

    fn set(&mut self, index: i32, value: bool) {
        self.data.set(index, value);
    }

    fn set_all(&mut self, value: bool) {
        self.data.set_all(value);
    }
}

/// Nodes reachable from `node1` but not from `node2`.
pub fn subgraph_difference(
    graph: &(impl LinearGraph + ?Sized),
    node1: i32,
    node2: i32,
) -> HashSet<i32> {
    let lite_linear_graph = as_lite_linear_graph(graph);

    let mut visited2 = BitSetFlags::new(graph.nodes_count());
    let mut bfs_walk2 = BfsWalk::new(node2, &lite_linear_graph, true);
    let mut visited1 = IntHashSetFlags::new(graph.nodes_count());
    let mut bfs_walk1 = BfsWalk::new(node1, &lite_linear_graph, true);

    let mut max1 = bfs_walk1.current_nodes().max_node().unwrap_or(i32::MIN);
    let mut min2 = bfs_walk2.current_nodes().min_node().unwrap_or(i32::MAX);
    while !bfs_walk1.is_finished() {
        if max1 < min2 {
            let mut flags = Visited1 {
                data: &mut visited1,
                visited2: &visited2,
                queue2: bfs_walk2.current_nodes(),
            };
            let next = bfs_walk1.step(&mut flags, |_| true);
            max1 = max1.max(next.into_iter().max().unwrap_or(i32::MIN));
        } else {
            bfs_walk2.step(&mut visited2, |_| true);
            min2 = bfs_walk2.current_nodes().min_node().unwrap_or(i32::MAX);
        }
    }

    visited1.data
}

/// Nodes reachable only from `head_node` and not from other heads (`isHead`), on a lite graph.
pub fn lite_exclusive_nodes(
    graph: &(impl LiteLinearGraph + ?Sized),
    head_node: i32,
    is_head: impl Fn(i32) -> bool,
) -> HashSet<i32> {
    let mut result = HashSet::new();
    let mut visited = BitSetFlags::new(graph.nodes_count());
    BfsWalk::new(head_node, graph, true).walk(&mut visited, |it| {
        let up_nodes = graph.get_nodes(it, NodeFilter::Up);
        if (up_nodes.is_empty() || up_nodes.iter().all(|up| result.contains(up)))
            && (it == head_node || !is_head(it))
        {
            result.insert(it);
            true
        } else {
            false
        }
    });
    result
}

/// Nodes reachable only from `head_node` and not from other heads (`isHead`).
pub fn exclusive_nodes(
    graph: &(impl LinearGraph + ?Sized),
    head_node: i32,
    is_head: impl Fn(i32) -> bool,
) -> HashSet<i32> {
    lite_exclusive_nodes(&as_lite_linear_graph(graph), head_node, is_head)
}
```

`crates/ig-log/src/utils/int_hash_set_flags.rs`:

```rust
//! [`Flags`] backed by a hash set of the set indexes (cheap for sparse flags over a big range).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntHashSetFlags.kt`.

use std::collections::HashSet;

use crate::utils::flags::Flags;

#[derive(Clone, Debug, Default)]
pub struct IntHashSetFlags {
    size: i32,
    pub data: HashSet<i32>,
}

impl IntHashSetFlags {
    pub fn new(size: i32) -> IntHashSetFlags {
        IntHashSetFlags {
            size,
            data: HashSet::new(),
        }
    }
}

impl Flags for IntHashSetFlags {
    fn size(&self) -> i32 {
        self.size
    }

    fn get(&self, index: i32) -> bool {
        self.data.contains(&index)
    }

    fn set(&mut self, index: i32, value: bool) {
        if value {
            self.data.insert(index);
        } else {
            self.data.remove(&index);
        }
    }

    fn set_all(&mut self, value: bool) {
        if value {
            self.data.extend(0..self.size);
        } else {
            self.data.clear();
        }
    }
}
```

`crates/ig-log/src/utils/int_list.rs`:

```rust
//! A read-only list of `i32`.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntList.java`. Nothing
//! in the graph module uses it upstream (the `IntList`s there are fastutil's); it is kept for the
//! file-to-file mapping.

pub trait IntList {
    fn size(&self) -> i32;

    fn get(&self, index: i32) -> i32;
}
```

`crates/ig-log/src/utils/int_to_int_map.rs`:

```rust
//! Maps between a "long" index range (all nodes) and a "short" one (the visible nodes, in order).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntToIntMap.java`,
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/UpdatableIntToIntMap.java`,
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/AbstractIntToIntMap.java` and
//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/IDIntToIntMap.java`.
//!
//! Deviation: `AbstractIntToIntMap` is a set of functions ([`abstract_get_short_index`] and the
//! `check_*` helpers) that implementations call. Upstream's exceptions are panics.

/// A monotonic map from short indexes to long indexes.
pub trait IntToIntMap {
    fn short_size(&self) -> i32;

    fn long_size(&self) -> i32;

    /// Must be very fast.
    fn get_long_index(&self, short_index: i32) -> i32;

    /// The max short index whose long index is `<= long_index`, or 0.
    fn get_short_index(&self, long_index: i32) -> i32;
}

/// An [`IntToIntMap`] whose visibility can change; [`update`](Self::update) recomputes a range.
pub trait UpdatableIntToIntMap: IntToIntMap {
    fn update(&mut self, start_long_index: i32, end_long_index: i32);
}

/// `AbstractIntToIntMap.getShortIndex`: binary search over [`IntToIntMap::get_long_index`].
///
/// # Panics
/// If `long_index` is out of `0..long_size()`.
pub fn abstract_get_short_index(map: &(impl IntToIntMap + ?Sized), long_index: i32) -> i32 {
    check_long_index(map, long_index);

    if map.short_size() == 0 || map.get_long_index(0) > long_index {
        return 0;
    }
    let mut a = 0;
    let mut b = map.short_size() - 1;
    while b > a + 1 {
        let middle = (a + b) / 2;
        if map.get_long_index(middle) <= long_index {
            a = middle;
        } else {
            b = middle;
        }
    }
    if map.get_long_index(b) <= long_index {
        b
    } else {
        a
    }
}

/// # Panics
/// If `long_index` is out of `0..long_size()` (upstream `IndexOutOfBoundsException`).
pub fn check_long_index(map: &(impl IntToIntMap + ?Sized), long_index: i32) {
    if long_index < 0 || long_index >= map.long_size() {
        panic!(
            "LongSize is: {}, but longIndex: {long_index}",
            map.long_size()
        );
    }
}

/// # Panics
/// If `short_index` is out of `0..short_size()` (upstream `IndexOutOfBoundsException`).
pub fn check_short_index(map: &(impl IntToIntMap + ?Sized), short_index: i32) {
    if short_index < 0 || short_index >= map.short_size() {
        panic!(
            "ShortSize is: {}, but shortIndex: {short_index}",
            map.short_size()
        );
    }
}

/// # Panics
/// Unless `0 <= start_long_index <= end_long_index < long_size()` (upstream
/// `IllegalArgumentException`).
pub fn check_update_parameters(
    map: &(impl IntToIntMap + ?Sized),
    start_long_index: i32,
    end_long_index: i32,
) {
    if start_long_index < 0
        || end_long_index < start_long_index
        || end_long_index >= map.long_size()
    {
        panic!(
            "ShortSize is: {}, but updateRequest is: ({start_long_index}, {end_long_index})",
            map.short_size()
        );
    }
}

/// The identity map of `size` indexes (`IDIntToIntMap`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdIntToIntMap {
    size: i32,
}

impl IdIntToIntMap {
    pub fn new(size: i32) -> IdIntToIntMap {
        IdIntToIntMap { size }
    }

    /// `IDIntToIntMap.EMPTY`: the empty map, whose `update` does nothing.
    pub const EMPTY: IdIntToIntMap = IdIntToIntMap { size: 0 };
}

impl IntToIntMap for IdIntToIntMap {
    fn short_size(&self) -> i32 {
        self.size
    }

    fn long_size(&self) -> i32 {
        self.size
    }

    fn get_long_index(&self, short_index: i32) -> i32 {
        short_index
    }

    fn get_short_index(&self, long_index: i32) -> i32 {
        long_index
    }
}

/// Only [`IdIntToIntMap::EMPTY`] is updatable upstream (`EmptyIDIntToIntMap`); updating does
/// nothing.
impl UpdatableIntToIntMap for IdIntToIntMap {
    fn update(&mut self, _start_long_index: i32, _end_long_index: i32) {
        debug_assert_eq!(self.size, 0, "only the empty IDIntToIntMap is updatable");
    }
}
```

`crates/ig-log/src/utils/list_int_to_int_map.rs`:

```rust
//! An updatable [`IntToIntMap`] over visibility [`Flags`], with cached visible counts per block.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/ListIntToIntMap.java`.
//!
//! Deviations:
//! - The map owns its flags and exposes them ([`ListIntToIntMap::flags`] /
//!   [`ListIntToIntMap::flags_mut`]): upstream keeps a reference to flags that its owner mutates.
//! - `new_instance` always returns a `ListIntToIntMap`. For `long_size == 0` it behaves like
//!   `IDIntToIntMap.EMPTY`, which upstream returns instead: no size checks, identity lookups and a
//!   no-op `update`.

use crate::utils::flags::Flags;
use crate::utils::int_to_int_map::{
    IntToIntMap, UpdatableIntToIntMap, check_long_index, check_short_index, check_update_parameters,
};
use crate::utils::predicate_flags::PredicateFlags;

/// `ListIntToIntMap.DEFAULT_BLOCK_SIZE`.
pub const DEFAULT_BLOCK_SIZE: i32 = 30;

pub struct ListIntToIntMap<F> {
    visibility: F,
    long_size: i32,
    block_size: i32,
    sub_sum_of_blocks: Vec<i32>,
}

impl<P: Fn(i32) -> bool> ListIntToIntMap<PredicateFlags<P>> {
    /// `newInstance(thisIsVisible, longSize)`.
    pub fn with_predicate(this_is_visible: P, long_size: i32) -> Self {
        ListIntToIntMap::new_instance(PredicateFlags::new(this_is_visible, long_size))
    }
}

impl<F: Flags> ListIntToIntMap<F> {
    /// `newInstance(visibleNodes)`, with [`DEFAULT_BLOCK_SIZE`].
    pub fn new_instance(visible_nodes: F) -> ListIntToIntMap<F> {
        ListIntToIntMap::with_block_size(visible_nodes, DEFAULT_BLOCK_SIZE)
    }

    /// `newInstance(visibility, blockSize)`. Memory usage is `long_size / block_size`;
    /// `get_long_index` needs `log(long_size) + block_size` steps, `get_short_index` `block_size`.
    ///
    /// # Panics
    /// If `visibility.size() < 0` (upstream `NegativeArraySizeException`).
    pub fn with_block_size(visibility: F, block_size: i32) -> ListIntToIntMap<F> {
        let long_size = visibility.size();
        assert!(long_size >= 0, "size < 0: {long_size}");

        if long_size == 0 {
            return ListIntToIntMap {
                visibility,
                long_size,
                block_size,
                sub_sum_of_blocks: Vec::new(),
            };
        }

        let sum_size = (long_size - 1) / block_size + 1;
        let mut list_int_to_int_map = ListIntToIntMap {
            visibility,
            long_size,
            block_size,
            sub_sum_of_blocks: vec![0; sum_size as usize],
        };
        list_int_to_int_map.update(0, long_size - 1);
        list_int_to_int_map
    }

    pub fn flags(&self) -> &F {
        &self.visibility
    }

    /// Changing the flags requires an [`update`](UpdatableIntToIntMap::update) of the changed
    /// range.
    pub fn flags_mut(&mut self) -> &mut F {
        &mut self.visibility
    }

    fn is_empty_map(&self) -> bool {
        self.sub_sum_of_blocks.is_empty()
    }

    fn get_relevant_sum_index(&self, long_index: i32) -> i32 {
        long_index / self.block_size
    }

    /// Sum of visible nodes before `block_index`, plus those of the block up to `last_long_index`.
    fn calculate_sum_for_block(&self, block_index: i32, last_long_index: i32) -> i32 {
        let mut sum = 0;
        if block_index > 0 {
            sum = self.sub_sum_of_blocks[(block_index - 1) as usize];
        }

        for long_index in block_index * self.block_size..=last_long_index {
            if self.visibility.get(long_index) {
                sum += 1;
            }
        }
        sum
    }
}

impl<F: Flags> IntToIntMap for ListIntToIntMap<F> {
    fn short_size(&self) -> i32 {
        self.sub_sum_of_blocks.last().copied().unwrap_or(0)
    }

    fn long_size(&self) -> i32 {
        self.long_size
    }

    fn get_long_index(&self, short_index: i32) -> i32 {
        if self.is_empty_map() {
            return short_index;
        }
        check_short_index(self, short_index);

        let block_index = last_insertion_point(&self.sub_sum_of_blocks, short_index);
        let mut pref_visible_count = 0;
        if block_index > 0 {
            pref_visible_count = self.sub_sum_of_blocks[block_index - 1];
        }

        for long_index in block_index as i32 * self.block_size..self.long_size {
            if self.visibility.get(long_index) {
                pref_visible_count += 1;
            }
            if pref_visible_count > short_index {
                return long_index;
            }
        }

        unreachable!("This should never happen!");
    }

    fn get_short_index(&self, long_index: i32) -> i32 {
        if self.is_empty_map() {
            return long_index;
        }
        check_long_index(self, long_index);

        let block_index = self.get_relevant_sum_index(long_index);
        let count_visible = self.calculate_sum_for_block(block_index, long_index);
        if count_visible > 0 {
            count_visible - 1
        } else {
            0
        }
    }
}

impl<F: Flags> UpdatableIntToIntMap for ListIntToIntMap<F> {
    fn update(&mut self, start_long_index: i32, end_long_index: i32) {
        if self.is_empty_map() {
            return;
        }
        check_update_parameters(self, start_long_index, end_long_index);
        let start_sum_index = self.get_relevant_sum_index(start_long_index);
        let end_sum_index = self.get_relevant_sum_index(end_long_index);
        let prev_end_sum = self.sub_sum_of_blocks[end_sum_index as usize];

        for block_index in start_sum_index..=end_sum_index {
            let end_index = self.long_size.min((block_index + 1) * self.block_size);
            self.sub_sum_of_blocks[block_index as usize] =
                self.calculate_sum_for_block(block_index, end_index - 1);
        }

        let sum_delta = self.sub_sum_of_blocks[end_sum_index as usize] - prev_end_sum;
        for block_index in (end_sum_index + 1) as usize..self.sub_sum_of_blocks.len() {
            self.sub_sum_of_blocks[block_index] += sum_delta;
        }
    }
}

/// The index of the first element greater than `key` in the sorted `array` (the last insertion
/// point; `Arrays.binarySearch` does not say which of several equal elements it finds).
fn last_insertion_point(array: &[i32], key: i32) -> usize {
    let mut l = 0;
    let mut u = array.len() - 1;
    while u > l {
        let middle = (l + u) / 2;
        if array[middle] <= key {
            l = middle + 1;
        } else {
            u = middle;
        }
    }
    debug_assert_eq!(l, u);
    l
}
```

`crates/ig-log/src/utils/mod.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/utils/mod.rs b/crates/ig-log/src/utils/mod.rs
index ae8b740..7e383fb 100644
--- a/crates/ig-log/src/utils/mod.rs
+++ b/crates/ig-log/src/utils/mod.rs
@@ -2,18 +2,24 @@
 //!
 //! `TimestampGetter` is ported from
 //! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/TimestampGetter.java`.
-//!
-//! Not ported (not needed by the `Normal` graph): `BfsUtil.kt`, `GraphUtil.kt`, `IntHashSetFlags.kt`,
-//! `UnsignedBitSet.java`, `IntList.java` and the `IntToIntMap` family (`utils/impl/*IntToIntMap.java`,
-//! `PredicateFlags.java`); they arrive with collapsing, BEK and filters in Plan 2b.
 
+pub mod bfs_util;
 pub mod bounded_cache;
 pub mod dfs_util;
 pub mod flags;
+pub mod graph_util;
+pub mod int_hash_set_flags;
 pub mod int_int_multi_map;
+pub mod int_list;
+pub mod int_to_int_map;
 pub mod java_sort;
 pub mod linear_graph_utils;
+pub mod list_int_to_int_map;
 pub mod normal_edge;
+pub mod permanent_list_int_to_int_map;
+pub mod predicate_flags;
+pub mod tree_int_to_int_map;
+pub mod unsigned_bit_set;
 
 /// Timestamp of the commit in a row (`TimestampGetter`).
 pub trait TimestampGetter {
```

`crates/ig-log/src/utils/permanent_list_int_to_int_map.rs`:

```rust
//! An immutable [`IntToIntMap`] over visibility [`Flags`], storing every `block_size`-th long index.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/PermanentListIntToIntMap.java`.
//!
//! Deviation: `new_instance` always returns a `PermanentListIntToIntMap`. For `short_size == 0` it
//! behaves like upstream's `createEmptyIntToIntMap` (no checks, every lookup returns 0).

use crate::utils::flags::Flags;
use crate::utils::int_to_int_map::{IntToIntMap, abstract_get_short_index, check_short_index};

/// `PermanentListIntToIntMap.DEFAULT_BLOCK_SIZE`.
pub const DEFAULT_BLOCK_SIZE: i32 = 30;

pub struct PermanentListIntToIntMap<F> {
    visible_indexes: F,
    long_size: i32,
    short_size: i32,
    block_size: i32,
    strong_short_indexes: Vec<i32>,
}

impl<F: Flags> PermanentListIntToIntMap<F> {
    /// `newInstance(visibleIndexes, shortSize)`, with [`DEFAULT_BLOCK_SIZE`].
    pub fn new_instance(visible_indexes: F, short_size: i32) -> PermanentListIntToIntMap<F> {
        PermanentListIntToIntMap::with_block_size(visible_indexes, short_size, DEFAULT_BLOCK_SIZE)
    }

    /// `newInstance(visibleIndexes, shortSize, blockSize)`; `short_size` must be the number of
    /// visible indexes.
    ///
    /// # Panics
    /// If `short_size < 0` (upstream `NegativeArraySizeException`).
    pub fn with_block_size(
        visible_indexes: F,
        short_size: i32,
        block_size: i32,
    ) -> PermanentListIntToIntMap<F> {
        assert!(short_size >= 0, "shortSize < 0: {short_size}");
        let long_size = visible_indexes.size();
        if short_size == 0 {
            return PermanentListIntToIntMap {
                visible_indexes,
                long_size,
                short_size,
                block_size,
                strong_short_indexes: Vec::new(),
            };
        }

        let mut strong_short_indexes = vec![0; ((short_size - 1) / block_size + 1) as usize];

        let mut current_short_index = -1;
        for long_index in 0..visible_indexes.size() {
            if visible_indexes.get(long_index) {
                current_short_index += 1;
                if current_short_index % block_size == 0 {
                    strong_short_indexes[(current_short_index / block_size) as usize] = long_index;
                }
            }
        }

        PermanentListIntToIntMap {
            visible_indexes,
            long_size,
            short_size,
            block_size,
            strong_short_indexes,
        }
    }
}

impl<F: Flags> IntToIntMap for PermanentListIntToIntMap<F> {
    fn short_size(&self) -> i32 {
        self.short_size
    }

    fn long_size(&self) -> i32 {
        self.long_size
    }

    fn get_long_index(&self, short_index: i32) -> i32 {
        if self.short_size == 0 {
            return 0;
        }
        check_short_index(self, short_index);

        let strong_index = short_index / self.block_size;

        let mut sub = short_index - strong_index * self.block_size;
        for long_index in self.strong_short_indexes[strong_index as usize]..self.long_size {
            if self.visible_indexes.get(long_index) {
                sub -= 1;
            }
            if sub == -1 {
                return long_index;
            }
        }
        panic!(
            "Not found long index for short index: {short_index}. Long & short size is: {}, {}.",
            self.long_size, self.short_size
        );
    }

    fn get_short_index(&self, long_index: i32) -> i32 {
        if self.short_size == 0 {
            return 0;
        }
        abstract_get_short_index(self, long_index)
    }
}
```

`crates/ig-log/src/utils/predicate_flags.rs`:

```rust
//! Read-only [`Flags`] backed by a predicate.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/PredicateFlags.java`.

use crate::utils::flags::Flags;

pub struct PredicateFlags<P> {
    visible: P,
    size: i32,
}

impl<P: Fn(i32) -> bool> PredicateFlags<P> {
    pub fn new(visible: P, size: i32) -> PredicateFlags<P> {
        PredicateFlags { visible, size }
    }
}

impl<P: Fn(i32) -> bool> Flags for PredicateFlags<P> {
    fn size(&self) -> i32 {
        self.size
    }

    fn get(&self, index: i32) -> bool {
        (self.visible)(index)
    }

    /// # Panics
    /// Always (upstream `UnsupportedOperationException`).
    fn set(&mut self, _index: i32, _value: bool) {
        panic!("Modification is not supported");
    }

    /// # Panics
    /// Always (upstream `UnsupportedOperationException`).
    fn set_all(&mut self, _value: bool) {
        panic!("Modification is not supported");
    }
}
```

`crates/ig-log/src/utils/tree_int_to_int_map.rs`:

```rust
//! An updatable [`IntToIntMap`] over a visibility predicate, backed by a segment tree of counts.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/TreeIntToIntMap.java`.
//!
//! Deviation: `new_instance` always returns a `TreeIntToIntMap`. For `long_size == 0` it behaves
//! like `IDIntToIntMap.EMPTY`, which upstream returns instead.

use crate::utils::int_to_int_map::{
    IntToIntMap, UpdatableIntToIntMap, abstract_get_short_index, check_short_index,
    check_update_parameters,
};

pub struct TreeIntToIntMap<P> {
    this_is_visible: P,
    long_size: i32,
    count_levels: i32,
    tree: Vec<i32>,
}

fn count_digits(mut long_size: i32) -> i32 {
    let mut count = 0;
    while long_size != 0 {
        count += 1;
        long_size >>= 1;
    }
    count
}

impl<P: Fn(i32) -> bool> TreeIntToIntMap<P> {
    /// # Panics
    /// If `long_size < 0` (upstream `NegativeArraySizeException`).
    pub fn new_instance(this_is_visible: P, long_size: i32) -> TreeIntToIntMap<P> {
        assert!(long_size >= 0, "size < 0: {long_size}");

        if long_size == 0 {
            return TreeIntToIntMap {
                this_is_visible,
                long_size,
                count_levels: 0,
                tree: Vec::new(),
            };
        }

        // longSize -> countLevels: 1..2 -> 2; 3..4 -> 3;  5..8 -> 4
        let count_levels = if long_size == 1 {
            2
        } else {
            count_digits(long_size - 1) + 1
        };

        let empty_tree = vec![0; 1 << (count_levels - 1)];
        let mut int_to_int_map = TreeIntToIntMap {
            this_is_visible,
            long_size,
            count_levels,
            tree: empty_tree,
        };
        int_to_int_map.update(0, long_size - 1);
        int_to_int_map
    }

    fn is_empty_map(&self) -> bool {
        self.tree.is_empty()
    }

    fn tree_len(&self) -> i32 {
        self.tree.len() as i32
    }

    fn is_last_level(&self, node: i32) -> bool {
        node >= self.tree_len()
    }

    fn update_node_count(&mut self, node: i32) -> i32 {
        if self.is_last_level(node) {
            return self.get_count_in_last_level(node);
        }

        let child = node << 1;
        let count = self.update_node_count(child) + self.update_node_count(child + 1);
        self.tree[node as usize] = count;
        count
    }

    fn get_count_in_last_level(&self, node: i32) -> i32 {
        let node = node - self.tree_len();
        if node < self.long_size && (self.this_is_visible)(node) {
            1
        } else {
            0
        }
    }

    fn get_count_in_node(&self, node: i32) -> i32 {
        if self.is_last_level(node) {
            self.get_count_in_last_level(node)
        } else {
            self.tree[node as usize]
        }
    }
}

impl<P: Fn(i32) -> bool> IntToIntMap for TreeIntToIntMap<P> {
    fn short_size(&self) -> i32 {
        if self.is_empty_map() {
            return 0;
        }
        self.tree[1]
    }

    fn long_size(&self) -> i32 {
        self.long_size
    }

    fn get_long_index(&self, mut short_index: i32) -> i32 {
        if self.is_empty_map() {
            return short_index;
        }
        check_short_index(self, short_index);

        let mut node = 1;
        for _ in 0..self.count_levels - 1 {
            let child = node << 1;
            let count_in_child_node = self.get_count_in_node(child);
            if count_in_child_node > short_index {
                node = child;
            } else {
                node = child + 1;
                short_index -= count_in_child_node;
            }
        }
        node - self.tree_len()
    }

    fn get_short_index(&self, long_index: i32) -> i32 {
        if self.is_empty_map() {
            return long_index;
        }
        abstract_get_short_index(self, long_index)
    }
}

impl<P: Fn(i32) -> bool> UpdatableIntToIntMap for TreeIntToIntMap<P> {
    fn update(&mut self, start_long_index: i32, end_long_index: i32) {
        if self.is_empty_map() {
            return;
        }
        check_update_parameters(self, start_long_index, end_long_index);

        let start_node = start_long_index + self.tree_len();
        let end_node = end_long_index + self.tree_len();
        let common_node = start_node >> count_digits(start_node ^ end_node);
        self.update_node_count(common_node);

        let mut parent = common_node >> 1;
        while parent != 0 {
            self.tree[parent as usize] =
                self.get_count_in_node(parent << 1) + self.get_count_in_node((parent << 1) + 1);
            parent >>= 1;
        }
    }
}
```

`crates/ig-log/src/utils/unsigned_bit_set.rs`:

```rust
//! A bit set indexed by any `i32`, negative indexes included (node ids of not-loaded commits are
//! negative).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/UnsignedBitSet.java`.
//! The two `java.util.BitSet`s become [`JavaBitSet`], a growable bit set with the same semantics
//! (bits beyond the end read `false`).

/// The subset of `java.util.BitSet` used here: a growable set of non-negative bit indexes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JavaBitSet {
    words: Vec<u64>,
}

impl JavaBitSet {
    pub fn new() -> JavaBitSet {
        JavaBitSet::default()
    }

    /// # Panics
    /// If `bit_index < 0` (upstream `IndexOutOfBoundsException`).
    pub fn get(&self, bit_index: i32) -> bool {
        assert!(bit_index >= 0, "bitIndex < 0: {bit_index}");
        let index = bit_index as usize;
        self.words
            .get(index / 64)
            .is_some_and(|word| word & (1 << (index % 64)) != 0)
    }

    /// # Panics
    /// If `bit_index < 0`.
    pub fn set(&mut self, bit_index: i32, value: bool) {
        assert!(bit_index >= 0, "bitIndex < 0: {bit_index}");
        let index = bit_index as usize;
        let word = index / 64;
        if value {
            if word >= self.words.len() {
                self.words.resize(word + 1, 0);
            }
            self.words[word] |= 1 << (index % 64);
        } else if let Some(w) = self.words.get_mut(word) {
            *w &= !(1 << (index % 64));
        }
    }

    /// Sets the bits `from_index..to_index` (exclusive end) to `value`.
    ///
    /// # Panics
    /// If `from_index < 0` or `from_index > to_index` (upstream `IndexOutOfBoundsException`).
    pub fn set_range(&mut self, from_index: i32, to_index: i32, value: bool) {
        assert!(from_index >= 0, "fromIndex < 0: {from_index}");
        assert!(
            from_index <= to_index,
            "fromIndex: {from_index} > toIndex: {to_index}"
        );
        if from_index == to_index {
            return;
        }
        let (from, to) = (from_index as usize, to_index as usize);
        if value {
            let words = to.div_ceil(64);
            if words > self.words.len() {
                self.words.resize(words, 0);
            }
        }
        for index in from..to.min(self.words.len() * 64) {
            let mask = 1 << (index % 64);
            if value {
                self.words[index / 64] |= mask;
            } else {
                self.words[index / 64] &= !mask;
            }
        }
    }
}

/// A bit set over all `i32` indexes: non-negative ones in one [`JavaBitSet`], negative ones
/// (negated) in another.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnsignedBitSet {
    positive_set: JavaBitSet,
    negative_set: JavaBitSet,
}

fn to_positive(bit_index: i32) -> i32 {
    -bit_index
}

impl UnsignedBitSet {
    pub fn new() -> UnsignedBitSet {
        UnsignedBitSet::default()
    }

    pub fn with_sets(positive_set: JavaBitSet, negative_set: JavaBitSet) -> UnsignedBitSet {
        UnsignedBitSet {
            positive_set,
            negative_set,
        }
    }

    /// # Panics
    /// For `i32::MIN`, whose negation overflows (upstream: `IndexOutOfBoundsException`).
    pub fn set(&mut self, bit_index: i32, value: bool) {
        if bit_index >= 0 {
            self.positive_set.set(bit_index, value);
        } else {
            self.negative_set.set(to_positive(bit_index), value);
        }
    }

    /// Sets `from_index..=to_index` (both boundaries included).
    ///
    /// # Panics
    /// If `from_index > to_index` (upstream `IndexOutOfBoundsException`).
    pub fn set_range(&mut self, from_index: i32, to_index: i32, value: bool) {
        assert!(
            from_index <= to_index,
            "fromIndex: {from_index} > toIndex: {to_index}"
        );
        if from_index >= 0 {
            self.positive_set.set_range(from_index, to_index + 1, value);
        } else if to_index < 0 {
            self.negative_set
                .set_range(to_positive(to_index), to_positive(from_index) + 1, value);
        } else {
            self.positive_set.set_range(0, to_index + 1, value);
            self.negative_set
                .set_range(to_positive(-1), to_positive(from_index) + 1, value);
        }
    }

    pub fn get(&self, bit_index: i32) -> bool {
        if bit_index >= 0 {
            self.positive_set.get(bit_index)
        } else {
            self.negative_set.get(to_positive(bit_index))
        }
    }
}
```

- [ ] **Step 4: Run the new tests and watch them pass**

```bash
cargo test -q --locked -p ig-log --test bfs --test graph_util --test unsigned_bit_set --test updatable_int_to_int_map --test utils_extra
```

Expected: PASS.

- [ ] **Step 5: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code 8da092b -- crates/ig-log/tests/bfs.rs crates/ig-log/tests/common/test_graph_builder.rs crates/ig-log/tests/graph_util.rs crates/ig-log/tests/unsigned_bit_set.rs crates/ig-log/tests/updatable_int_to_int_map.rs crates/ig-log/tests/utils_extra.rs crates/ig-log/src/utils/bfs_util.rs crates/ig-log/src/utils/graph_util.rs crates/ig-log/src/utils/int_hash_set_flags.rs crates/ig-log/src/utils/int_list.rs crates/ig-log/src/utils/int_to_int_map.rs crates/ig-log/src/utils/list_int_to_int_map.rs crates/ig-log/src/utils/mod.rs crates/ig-log/src/utils/permanent_list_int_to_int_map.rs crates/ig-log/src/utils/predicate_flags.rs crates/ig-log/src/utils/tree_int_to_int_map.rs crates/ig-log/src/utils/unsigned_bit_set.rs
```

Expected: no output.

- [ ] **Step 6: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 566 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m 'feat(ig-log): graph utilities (BFS, GraphUtil, UnsignedBitSet, IntToIntMap family)'
```

---

### Task 2: BEK sort (SortIndexMap, BekSorter, SortedBaseController)

**Files:**
- Create: `crates/ig-log/tests/bek.rs`, `crates/ig-log/tests/common/linear_bek.rs`, `crates/ig-log/src/facade/sort/bek/bek_branch.rs`, `crates/ig-log/src/facade/sort/bek/bek_branch_creator.rs`, `crates/ig-log/src/facade/sort/bek/bek_branch_merger.rs`, `crates/ig-log/src/facade/sort/bek/bek_edge_restrictions.rs`, `crates/ig-log/src/facade/sort/bek/bek_sorter.rs`, `crates/ig-log/src/facade/sort/bek/mod.rs`, `crates/ig-log/src/facade/sort/mod.rs`, `crates/ig-log/src/facade/sort/sort_checker.rs`, `crates/ig-log/src/facade/sort/sort_index_map.rs`, `crates/ig-log/src/facade/sort/sorted_base_controller.rs`
- Modify: `crates/ig-log/tests/common/mod.rs`, `crates/ig-log/tests/common/test_graph_builder.rs`, `crates/ig-log/src/facade/mod.rs`

**Source commit:** `8f9df63` on `wip/graph-extras-prototype`.

**Review focus:** `BekSorter`, `BekBranchCreator`, `BekBranchMerger` and `BekEdgeRestrictions` against `impl/facade/sort/bek/*`; comparator sorts go through `utils::java_sort`; `SortChecker`'s `LOG.error` became `debug_assert!`.

The BEK ("Standard") ordering: a permutation of the permanent graph's rows, applied by `SortedBaseController`.

- [ ] **Step 1: Copy the tests first**

```bash
git checkout 8f9df63 -- crates/ig-log/tests/bek.rs crates/ig-log/tests/common/linear_bek.rs crates/ig-log/tests/common/mod.rs crates/ig-log/tests/common/test_graph_builder.rs
```

`crates/ig-log/tests/bek.rs`:

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/BekTest.kt`.

mod common;

use common::linear_bek::{assert_graph_equals, run_bek};
use common::test_graph_builder::{TestLinearGraph, test_graph};

fn run_test(before_bek: TestLinearGraph, after_bek: TestLinearGraph) {
    assert_graph_equals(after_bek, &run_bek(before_bek));
}

#[test]
fn ea68497() {
    run_test(
        test_graph! {
            0(1, 3)
            1(4, 2)
            2(4, 3)
            3()
            4()
        },
        test_graph! {
            0(1, 3)
            1(4, 2)
            2(4, 3)
            3()
            4()
        },
    )
}

#[test]
fn simple_merge() {
    run_test(
        test_graph! {
            0(1, 2)
            1(3)
            2(4)
            3(5)
            4(5)
            5()
        },
        test_graph! {
            0(1, 2)
            2(4)
            4(5)
            1(3)
            3(5)
            5()
        },
    )
}

#[test]
fn two_heads() {
    run_test(
        test_graph! {
            0(2)
            1(2)
            2()
        },
        test_graph! {
            1(2)
            0(2)
            2()
        },
    )
}

#[test]
fn simple_merge_with_two_heads() {
    run_test(
        test_graph! {
            0(2, 3)
            1(4)
            2(4)
            3(5)
            4(6)
            5(6)
            6()
        },
        test_graph! {
            1(4)
            0(2, 3)
            3(5)
            5(6)
            2(4)
            4(6)
            6()
        },
    )
}
```

`crates/ig-log/tests/common/linear_bek.rs`:

```rust
//! Helpers of the BEK and linear BEK tests.
//!
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/TestUtils.kt`.

use std::sync::Arc;

use ig_log::api::linear_graph::LinearGraph;
use ig_log::facade::sort::bek::bek_sorter;
use ig_log::facade::sort::sort_checker;
use ig_log::facade::sort::sorted_base_controller::SortedLinearGraph;
use ig_log::permanent::graph_layout_builder;
use ig_log::permanent::graph_layout_impl::GraphLayoutImpl;
use ig_log::utils::TimestampGetter;

use super::test_graph_builder::{TestLinearGraph, as_test_graph_string};

pub struct DummyTimestampGetter {
    pub nodes_count: i32,
}

impl TimestampGetter for DummyTimestampGetter {
    fn get_timestamp(&self, _index: i32) -> i64 {
        0
    }
}

pub fn build_layout(graph: &TestLinearGraph) -> GraphLayoutImpl {
    graph_layout_builder::build(graph, |node_index1, node_index2| {
        node_index1.cmp(&node_index2)
    })
}

/// `runBek`: takes the graph built by the test's builder (Kotlin builds it twice).
pub fn run_bek(before_bek: TestLinearGraph) -> SortedLinearGraph {
    let before_layout = build_layout(&before_bek);

    let bek_map = bek_sorter::create_bek_map(
        &before_bek,
        &before_layout,
        &DummyTimestampGetter {
            nodes_count: before_bek.nodes_count(),
        },
    );
    let bek_map = Arc::new(bek_map);
    let after_bek = SortedLinearGraph::new(bek_map.clone(), Arc::new(before_bek));

    let edge = sort_checker::find_reversed_edge(&after_bek);
    if let Some(edge) = edge {
        panic!(
            "Found reversed edge: {:?}",
            (
                bek_map.get_usual_index(edge.0),
                bek_map.get_usual_index(edge.1)
            )
        );
    }

    after_bek
}

/// `assertEquals(expected: TestGraphBuilder.() -> Unit, actual: LinearGraph)`.
pub fn assert_graph_equals(expected: TestLinearGraph, actual: &(impl LinearGraph + ?Sized)) {
    assert_eq!(
        as_test_graph_string(&expected, false),
        as_test_graph_string(actual, false)
    );
}
```

`crates/ig-log/tests/common/mod.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/tests/common/mod.rs b/crates/ig-log/tests/common/mod.rs
index 80461a0..483a88c 100644
--- a/crates/ig-log/tests/common/mod.rs
+++ b/crates/ig-log/tests/common/mod.rs
@@ -7,6 +7,7 @@
 //! Test data lives in `tests/fixtures/<dir>` (byte-identical copies of `platform/vcs-log/graph/testData`).
 #![allow(dead_code)] // each test crate uses a different subset
 
+pub mod linear_bek;
 pub mod parser;
 pub mod str_utils;
 pub mod test_graph_builder;
```

`crates/ig-log/tests/common/test_graph_builder.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/tests/common/test_graph_builder.rs b/crates/ig-log/tests/common/test_graph_builder.rs
index 24cd582..8e9e860 100644
--- a/crates/ig-log/tests/common/test_graph_builder.rs
+++ b/crates/ig-log/tests/common/test_graph_builder.rs
@@ -4,15 +4,18 @@
 //! (`BaseTestGraphBuilder`, `TestGraphBuilder`, `graph { }`). Kotlin's `1(2, 3.dot)` becomes
 //! `b.node(1, [u(2), dot(3)])`; `2.UNM(3)` becomes `b.node(unm(2), [u(3)])`. The
 //! [`test_graph!`](crate::common::test_graph_builder::test_graph) macro accepts the Kotlin syntax
-//! itself (`test_graph! { 1(2, 3.dot) 2.UNM(3) 3() }`).
+//! itself (`test_graph! { 1(2, 3.dot) 2.UNM(3) 3() }`). Also ports `asTestGraphString`.
 
 use std::collections::HashMap;
 
 use ig_log::api::EdgeFilter;
-use ig_log::api::elements::{GraphEdge, GraphEdgeType, GraphNode, GraphNodeType};
+use ig_log::api::elements::{GraphEdge, GraphEdgeType, GraphElement, GraphNode, GraphNodeType};
 use ig_log::api::linear_graph::LinearGraph;
+use ig_log::utils::java_sort;
 use ig_log::utils::linear_graph_utils::{is_edge_down, is_edge_up};
 
+use super::str_utils::graph_element_comparator;
+
 #[derive(Clone, Copy, Debug)]
 pub struct SimpleEdge {
     pub to_node: Option<i32>,
@@ -284,3 +287,105 @@ impl LinearGraph for TestLinearGraph {
         self.node_id_to_index.get(&node_id).copied()
     }
 }
+
+/// `LinearGraph.assertEdge`: the edge is consistent with `node_index` and the other end's edges.
+fn assert_edge(graph: &(impl LinearGraph + ?Sized), node_index: i32, edge: &GraphEdge) {
+    if edge.edge_type.is_normal_edge() {
+        if Some(node_index) == edge.up_node_index {
+            assert!(
+                graph
+                    .get_adjacent_edges(edge.down_node_index.unwrap(), EdgeFilter::NormalUp)
+                    .contains(edge)
+            );
+        } else {
+            assert_eq!(Some(node_index), edge.down_node_index);
+            assert!(
+                graph
+                    .get_adjacent_edges(edge.up_node_index.unwrap(), EdgeFilter::NormalDown)
+                    .contains(edge)
+            );
+        }
+    } else {
+        match edge.edge_type {
+            GraphEdgeType::NotLoadCommit | GraphEdgeType::DottedArrowDown => {
+                assert_eq!(Some(node_index), edge.up_node_index);
+                assert_eq!(None, edge.down_node_index);
+            }
+            GraphEdgeType::DottedArrowUp => {
+                assert_eq!(Some(node_index), edge.down_node_index);
+                assert_eq!(None, edge.up_node_index);
+            }
+            _ => {}
+        }
+    }
+}
+
+/// `LinearGraph.asTestGraphString(sorted)`: the graph in the `graph { }` DSL's notation, one node
+/// per line (`id.UNM(edges)`), with the down edges of each node.
+pub fn as_test_graph_string(graph: &(impl LinearGraph + ?Sized), sorted: bool) -> String {
+    let mut s = String::new();
+    for node_index in 0..graph.nodes_count() {
+        let node = graph.get_graph_node(node_index);
+        s.push_str(&graph.get_node_id(node_index).to_string());
+        assert_eq!(
+            node_index,
+            node.node_index,
+            "nodeIndex: {node_index}, but for node with this index(nodeId: {}) nodeIndex: {}",
+            graph.get_node_id(node_index),
+            node.node_index
+        );
+        match node.node_type {
+            GraphNodeType::Unmatched => s.push_str(".UNM"),
+            GraphNodeType::NotLoadCommit => s.push_str(".NOT_LOAD"),
+            GraphNodeType::Usual => {}
+        }
+
+        // edges
+        let mut adjacent_edges = graph.get_adjacent_edges(node_index, EdgeFilter::All);
+        if sorted {
+            let comparator = graph_element_comparator();
+            let mut elements: Vec<GraphElement> =
+                adjacent_edges.into_iter().map(GraphElement::Edge).collect();
+            java_sort::sort_by(&mut elements, |a, b| comparator.ordering(a, b));
+            adjacent_edges = elements
+                .into_iter()
+                .map(|element| match element {
+                    GraphElement::Edge(edge) => edge,
+                    GraphElement::Node(_) => unreachable!(),
+                })
+                .collect();
+        }
+
+        s.push('(');
+        let edges: Vec<String> = adjacent_edges
+            .iter()
+            .filter_map(|it| {
+                assert_edge(graph, node_index, it);
+                if it.up_node_index == Some(node_index) {
+                    let start_id = if it.edge_type.is_normal_edge() {
+                        graph.get_node_id(it.down_node_index.unwrap()).to_string()
+                    } else if let Some(target_id) = it.target_id {
+                        target_id.to_string()
+                    } else {
+                        "null".to_string()
+                    };
+
+                    Some(match it.edge_type {
+                        GraphEdgeType::Usual => start_id,
+                        GraphEdgeType::Dotted => format!("{start_id}.dot"),
+                        GraphEdgeType::DottedArrowUp => format!("{start_id}.up_dot"),
+                        GraphEdgeType::DottedArrowDown => format!("{start_id}.down_dot"),
+                        GraphEdgeType::NotLoadCommit => format!("{start_id}.not_load"),
+                    })
+                } else {
+                    None
+                }
+            })
+            .collect();
+        s.push_str(&edges.join(", "));
+
+        s.push(')');
+        s.push('\n');
+    }
+    s
+}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -q --locked -p ig-log --test bek
```

Expected: FAIL — `ig_log::facade::sort` doesn't exist yet, so `tests/bek.rs` fails to compile. The simulation's first error:

```text
error[E0433]: cannot find `sort` in `facade`
```

- [ ] **Step 3: Copy the code**

```bash
git checkout 8f9df63 -- crates/ig-log/src/facade/mod.rs crates/ig-log/src/facade/sort/bek/bek_branch.rs crates/ig-log/src/facade/sort/bek/bek_branch_creator.rs crates/ig-log/src/facade/sort/bek/bek_branch_merger.rs crates/ig-log/src/facade/sort/bek/bek_edge_restrictions.rs crates/ig-log/src/facade/sort/bek/bek_sorter.rs crates/ig-log/src/facade/sort/bek/mod.rs crates/ig-log/src/facade/sort/mod.rs crates/ig-log/src/facade/sort/sort_checker.rs crates/ig-log/src/facade/sort/sort_index_map.rs crates/ig-log/src/facade/sort/sorted_base_controller.rs
```

`crates/ig-log/src/facade/mod.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/facade/mod.rs b/crates/ig-log/src/facade/mod.rs
index 753fd2f..8688943 100644
--- a/crates/ig-log/src/facade/mod.rs
+++ b/crates/ig-log/src/facade/mod.rs
@@ -2,7 +2,7 @@
 //!
 //! Ports the `Normal`-sort subset of `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/`;
 //! see each submodule. Not ported yet (Plan 2b): `CascadeController.kt`, `FilteredController.kt`,
-//! `FirstParentController.kt`, `GraphModificationUtil.kt`, `SimpleGraphInfo.java` and `sort/**`.
+//! `FirstParentController.kt`, `GraphModificationUtil.kt` and `SimpleGraphInfo.java`.
 
 pub mod base_controller;
 pub mod graph_changes;
@@ -11,4 +11,5 @@ pub mod linear_graph_wrapper;
 pub mod permanent_graph_impl;
 pub mod print_element_presentation_manager_impl;
 pub mod reachable_nodes;
+pub mod sort;
 pub mod visible_graph_impl;
```

`crates/ig-log/src/facade/sort/bek/bek_branch.rs`:

```rust
//! A branch of the BEK sort: a run of nodes inserted into the result in parts, bottom up.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekBranch.java`.

use crate::api::linear_graph::LinearGraph;
use crate::facade::sort::bek::bek_edge_restrictions::BekEdgeRestrictions;
use crate::utils::TimestampGetter;
use crate::utils::linear_graph_utils::get_down_nodes;

const MAX_BLOCK_SIZE: usize = 20;
const MAX_DELTA_TIME: i64 = 60 * 60 * 24 * 3 * 1000;
const SMALL_DELTA_TIME: i64 = 60 * 60 * 4 * 1000;

#[derive(Clone, Debug)]
pub struct BekBranch {
    node_indexes: Vec<i32>,
    no_insert_size: usize,
    /// Start of the prepared part; the part is `node_indexes[start..no_insert_size]`.
    prepare_for_insert_part: Option<usize>,
}

impl BekBranch {
    pub fn new(node_indexes: Vec<i32>) -> BekBranch {
        let no_insert_size = node_indexes.len();
        BekBranch {
            node_indexes,
            no_insert_size,
            prepare_for_insert_part: None,
        }
    }

    pub fn update_prepare_for_insert_part(
        &mut self,
        permanent_graph: &(impl LinearGraph + ?Sized),
        timestamp_getter: &(impl TimestampGetter + ?Sized),
        edge_restrictions: &BekEdgeRestrictions,
    ) {
        debug_assert!(self.prepare_for_insert_part.is_none());
        let current_node = self.node_indexes[self.no_insert_size - 1];

        if edge_restrictions.has_restriction(current_node) {
            return;
        }

        let mut prev_index = self.no_insert_size - 1;
        while prev_index > 0 {
            let up_node = self.node_indexes[prev_index - 1];
            let down_node = self.node_indexes[prev_index];

            // for correct topological order
            if edge_restrictions.has_restriction(up_node) {
                break;
            }

            // upNode is mergeCommit
            let down_nodes = get_down_nodes(permanent_graph, up_node);
            if down_nodes.len() > 1 && down_nodes.contains(&down_node) {
                prev_index -= 1;
                continue;
            }

            // division
            if !down_nodes.contains(&down_node) {
                break;
            }

            let delta = (timestamp_getter.get_timestamp(up_node)
                - timestamp_getter.get_timestamp(down_node))
            .abs();

            // long time between commits
            if delta > MAX_DELTA_TIME {
                break;
            }

            // if block so long
            if (prev_index as i64) < self.no_insert_size as i64 - MAX_BLOCK_SIZE as i64
                && delta > SMALL_DELTA_TIME
            {
                break;
            }
            prev_index -= 1;
        }

        self.prepare_for_insert_part = Some(prev_index);
    }

    pub fn get_prepare_for_insert_part(&self) -> Option<&[i32]> {
        self.prepare_for_insert_part
            .map(|start| &self.node_indexes[start..self.no_insert_size])
    }

    pub fn done_insert_prepared_part(&mut self) {
        let start = self
            .prepare_for_insert_part
            .take()
            .expect("no prepared part to insert");
        self.no_insert_size = start;
    }

    pub fn is_done(&self) -> bool {
        self.no_insert_size == 0
    }
}
```

`crates/ig-log/src/facade/sort/bek/bek_branch_creator.rs`:

```rust
//! Splits the permanent graph into BEK branches, one per head, walking down each head's layout.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekBranchCreator.java`.

use crate::api::GraphLayout;
use crate::api::linear_graph::LinearGraph;
use crate::facade::sort::bek::bek_branch::BekBranch;
use crate::facade::sort::bek::bek_edge_restrictions::BekEdgeRestrictions;
use crate::permanent::graph_layout_impl::GraphLayoutImpl;
use crate::utils::dfs_util::{NODE_NOT_FOUND, walk};
use crate::utils::flags::{BitSetFlags, Flags};
use crate::utils::linear_graph_utils::{get_down_nodes, get_up_nodes};

pub struct BekBranchCreator<'a, G: LinearGraph + ?Sized> {
    permanent_graph: &'a G,
    graph_layout: &'a GraphLayoutImpl,
    done_nodes: BitSetFlags,
    edge_restrictions: BekEdgeRestrictions,
}

impl<'a, G: LinearGraph + ?Sized> BekBranchCreator<'a, G> {
    pub fn new(permanent_graph: &'a G, graph_layout: &'a GraphLayoutImpl) -> Self {
        BekBranchCreator {
            permanent_graph,
            graph_layout,
            done_nodes: BitSetFlags::with_default(permanent_graph.nodes_count(), false),
            edge_restrictions: BekEdgeRestrictions::default(),
        }
    }

    pub fn get_result(mut self) -> (Vec<BekBranch>, BekEdgeRestrictions) {
        let mut bek_branches = Vec::new();

        for &head_node in self.graph_layout.get_head_node_index() {
            if self.done_nodes.get(head_node) {
                continue;
            }
            let next_branch = self.create_next_branch(head_node);
            bek_branches.push(BekBranch::new(next_branch));
        }
        (bek_branches, self.edge_restrictions)
    }

    pub fn create_next_branch(&mut self, head_node: i32) -> Vec<i32> {
        self.done_nodes.set(head_node, true);
        let mut node_indexes = vec![head_node];

        let graph = self.permanent_graph;
        let layout = self.graph_layout;
        let done_nodes = &mut self.done_nodes;
        let edge_restrictions = &mut self.edge_restrictions;
        let start_layout = layout.get_layout_index(head_node);

        walk(head_node, |current_node| {
            let current_layout = layout.get_layout_index(current_node);
            let down_nodes = get_down_nodes(graph, current_node);
            for &down_node in down_nodes.iter().rev() {
                if done_nodes.get(down_node) {
                    if layout.get_layout_index(down_node) < start_layout {
                        edge_restrictions.add_restriction(current_node, down_node);
                    }
                } else if current_layout <= layout.get_layout_index(down_node) {
                    // almost ok node, except (may be) up nodes
                    let mut has_undone_up_nodes = false;
                    for up_node in get_up_nodes(graph, down_node) {
                        if !done_nodes.get(up_node)
                            && layout.get_layout_index(up_node)
                                <= layout.get_layout_index(down_node)
                        {
                            has_undone_up_nodes = true;
                            break;
                        }
                    }

                    if !has_undone_up_nodes {
                        done_nodes.set(down_node, true);
                        node_indexes.push(down_node);
                        return down_node;
                    }
                }
            }
            NODE_NOT_FOUND
        });

        node_indexes
    }
}
```

`crates/ig-log/src/facade/sort/bek/bek_branch_merger.rs`:

```rust
//! Merges the BEK branches into one order: repeatedly inserts the prepared part with the oldest
//! timestamp, bottom up.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekBranchMerger.java`.

use crate::api::linear_graph::LinearGraph;
use crate::facade::sort::bek::bek_branch::BekBranch;
use crate::facade::sort::bek::bek_edge_restrictions::BekEdgeRestrictions;
use crate::utils::TimestampGetter;

pub struct BekBranchMerger<'a, G: ?Sized, T: ?Sized> {
    permanent_graph: &'a G,
    bek_branches: Vec<BekBranch>,
    edge_restrictions: BekEdgeRestrictions,
    timestamp_getter: &'a T,
    inverse_result_list: Vec<i32>,
}

impl<'a, G: LinearGraph + ?Sized, T: TimestampGetter + ?Sized> BekBranchMerger<'a, G, T> {
    pub fn new(
        permanent_graph: &'a G,
        bek_branches: Vec<BekBranch>,
        edge_restrictions: BekEdgeRestrictions,
        timestamp_getter: &'a T,
    ) -> Self {
        BekBranchMerger {
            permanent_graph,
            bek_branches,
            edge_restrictions,
            timestamp_getter,
            inverse_result_list: Vec::new(),
        }
    }

    /// Returns whether some branch is not done yet.
    fn prepare_last_parts_for_branches(&mut self) -> bool {
        let mut has_undone_branches = false;
        for bek_branch in &mut self.bek_branches {
            if !bek_branch.is_done() {
                has_undone_branches = true;
                if bek_branch.get_prepare_for_insert_part().is_none() {
                    bek_branch.update_prepare_for_insert_part(
                        self.permanent_graph,
                        self.timestamp_getter,
                        &self.edge_restrictions,
                    );
                }
            }
        }
        has_undone_branches
    }

    fn get_branch_last_part_timestamp(&self, bek_branch: &BekBranch) -> i64 {
        let Some(prepare_for_insert_part) = bek_branch.get_prepare_for_insert_part() else {
            return i64::MAX;
        };

        debug_assert!(!prepare_for_insert_part.is_empty());
        let node_index = prepare_for_insert_part[0];
        self.timestamp_getter.get_timestamp(node_index)
    }

    fn step(&mut self) {
        let mut select_branch = 0;
        for (i, bek_branch) in self.bek_branches.iter().enumerate() {
            if self.get_branch_last_part_timestamp(&self.bek_branches[select_branch])
                > self.get_branch_last_part_timestamp(bek_branch)
            {
                select_branch = i;
            }
        }

        // upstream: LOG.assertTrue, then a NullPointerException
        let prepare_for_insert_part = self.bek_branches[select_branch]
            .get_prepare_for_insert_part()
            .expect("BEK: no branch part can be inserted")
            .to_vec();
        for &inserted_node in &prepare_for_insert_part {
            self.edge_restrictions.remove_restriction(inserted_node);
        }

        self.inverse_result_list
            .extend(prepare_for_insert_part.iter().rev());
        self.bek_branches[select_branch].done_insert_prepared_part();
    }

    pub fn get_result(mut self) -> Vec<i32> {
        while self.prepare_last_parts_for_branches() {
            self.step();
        }

        self.inverse_result_list.reverse();
        self.inverse_result_list
    }
}
```

`crates/ig-log/src/facade/sort/bek/bek_edge_restrictions.rs`:

```rust
//! Edges whose up node must not be inserted before their down node.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekEdgeRestrictions.java`.

use crate::utils::int_int_multi_map::IntIntMultiMap;

#[derive(Clone, Debug, Default)]
pub struct BekEdgeRestrictions {
    up_to_edge: IntIntMultiMap,
    down_to_edge: IntIntMultiMap,
}

impl BekEdgeRestrictions {
    pub fn add_restriction(&mut self, up_node: i32, down_node: i32) {
        self.up_to_edge.put_value(up_node, down_node);
        self.down_to_edge.put_value(down_node, up_node);
    }

    pub fn remove_restriction(&mut self, down_node: i32) {
        for &up_node in self.down_to_edge.get(down_node) {
            self.up_to_edge.remove(up_node, down_node);
        }
    }

    pub fn has_restriction(&self, up_node: i32) -> bool {
        self.up_to_edge.contains_key(up_node)
    }
}
```

`crates/ig-log/src/facade/sort/bek/bek_sorter.rs`:

```rust
//! Computes the BEK order of the permanent graph.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekSorter.java`.

use crate::api::linear_graph::LinearGraph;
use crate::facade::sort::bek::bek_branch_creator::BekBranchCreator;
use crate::facade::sort::bek::bek_branch_merger::BekBranchMerger;
use crate::facade::sort::sort_index_map::SortIndexMap;
use crate::permanent::graph_layout_impl::GraphLayoutImpl;
use crate::utils::TimestampGetter;

pub fn create_bek_map(
    permanent_graph: &(impl LinearGraph + ?Sized),
    graph_layout: &GraphLayoutImpl,
    timestamp_getter: &(impl TimestampGetter + ?Sized),
) -> SortIndexMap {
    let bek_branch_creator = BekBranchCreator::new(permanent_graph, graph_layout);
    let (branches, edge_restrictions) = bek_branch_creator.get_result();

    let bek_branch_merger = BekBranchMerger::new(
        permanent_graph,
        branches,
        edge_restrictions,
        timestamp_getter,
    );
    let result = bek_branch_merger.get_result();

    debug_assert_eq!(result.len() as i32, permanent_graph.nodes_count());
    SortIndexMap::new(result)
}
```

`crates/ig-log/src/facade/sort/bek/mod.rs`:

```rust
//! BEK ("IntelliSort", the "Standard" sort): commits of a merged branch directly below the merge.
//!
//! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/`; see each
//! submodule. Upstream's `LOG.assertTrue` checks are `debug_assert!`s.

pub mod bek_branch;
pub mod bek_branch_creator;
pub mod bek_branch_merger;
pub mod bek_edge_restrictions;
pub mod bek_sorter;
```

`crates/ig-log/src/facade/sort/mod.rs`:

```rust
//! Sorted views of the permanent graph: the sort index map, the BEK ("IntelliSort") sorter and the
//! controller showing the graph in a sorted order.
//!
//! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/`; see each
//! submodule.

pub mod bek;
pub mod sort_checker;
pub mod sort_index_map;
pub mod sorted_base_controller;
```

`crates/ig-log/src/facade/sort/sort_checker.rs`:

```rust
//! Checks that a sorted graph is still topologically sorted (every edge goes down).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/SortChecker.java`.
//!
//! Deviation: upstream's `LOG.error` (which fails tests and only logs in production) is a
//! `debug_assert!`.

use crate::api::linear_graph::LinearGraph;
use crate::utils::linear_graph_utils::{get_down_nodes, get_up_nodes};

pub fn check_linear_graph(linear_graph: &(impl LinearGraph + ?Sized)) {
    let reversed_edge = find_reversed_edge(linear_graph);
    debug_assert!(
        reversed_edge.is_none(),
        "Illegal edge: up node {:?}, downNode {:?}",
        reversed_edge.map(|e| e.0),
        reversed_edge.map(|e| e.1)
    );
}

/// The first edge `(up node, down node)` whose up node is not above its down node.
pub fn find_reversed_edge(linear_graph: &(impl LinearGraph + ?Sized)) -> Option<(i32, i32)> {
    for i in 0..linear_graph.nodes_count() {
        for down_node in get_down_nodes(linear_graph, i) {
            if down_node <= i {
                return Some((i, down_node));
            }
        }

        for up_node in get_up_nodes(linear_graph, i) {
            if up_node >= i {
                return Some((up_node, i));
            }
        }
    }
    None
}
```

`crates/ig-log/src/facade/sort/sort_index_map.rs`:

```rust
//! A permutation of the permanent graph's rows: sorted index <-> usual index (node id).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/SortIndexMap.java`.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortIndexMap {
    map: Vec<i32>,
    reverse_map: Vec<i32>,
}

impl SortIndexMap {
    /// `list[sorted_index] == usual_index`; `list` must be a permutation of `0..list.len()`.
    ///
    /// # Panics
    /// If `list` contains an index out of `0..list.len()`.
    pub fn new(list: Vec<i32>) -> SortIndexMap {
        let mut reverse_map = vec![0; list.len()];
        for (i, &usual_index) in list.iter().enumerate() {
            reverse_map[usual_index as usize] = i as i32;
        }
        SortIndexMap {
            map: list,
            reverse_map,
        }
    }

    pub fn size(&self) -> i32 {
        self.map.len() as i32
    }

    /// `usual_index` is the node id.
    ///
    /// # Panics
    /// If `usual_index` is out of `0..size()`.
    pub fn get_sorted_index(&self, usual_index: i32) -> i32 {
        self.reverse_map[usual_index as usize]
    }

    /// # Panics
    /// If `sorted_index` is out of `0..size()`.
    pub fn get_usual_index(&self, sorted_index: i32) -> i32 {
        self.map[sorted_index as usize]
    }
}
```

`crates/ig-log/src/facade/sort/sorted_base_controller.rs`:

```rust
//! The bottom controller of the BEK chains: shows the permanent graph in a sorted order.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/SortedBaseController.java`.

use std::sync::Arc;

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphNode, GraphNodeType};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::{PermanentGraphInfo, VcsLogGraphNodeId};
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};
use crate::facade::sort::sort_checker;
use crate::facade::sort::sort_index_map::SortIndexMap;
use crate::utils::linear_graph_utils::default_graph_answer;
use crate::visible_graph::VcsLogVisibleGraphIndex;

pub struct SortedBaseController {
    sort_index_map: Arc<SortIndexMap>,
    sorted_graph: Arc<SortedLinearGraph>,
}

impl SortedBaseController {
    pub fn new<CommitId>(
        permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
        sort_index_map: Arc<SortIndexMap>,
    ) -> SortedBaseController {
        let sorted_graph = Arc::new(SortedLinearGraph::new(
            sort_index_map.clone(),
            permanent_graph_info.linear_graph(),
        ));

        sort_checker::check_linear_graph(&*sorted_graph);
        SortedBaseController {
            sort_index_map,
            sorted_graph,
        }
    }

    pub fn bek_int_map(&self) -> &Arc<SortIndexMap> {
        &self.sort_index_map
    }
}

impl LinearGraphController for SortedBaseController {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.sorted_graph.clone()
    }

    fn perform_linear_graph_action(&mut self, _action: &LinearGraphAction) -> LinearGraphAnswer {
        default_graph_answer()
    }
}

/// The permanent graph with rows reordered by a [`SortIndexMap`]; node ids stay the permanent
/// graph's rows.
pub struct SortedLinearGraph {
    linear_graph: Arc<dyn LinearGraph>,
    sort_index_map: Arc<SortIndexMap>,
}

impl SortedLinearGraph {
    pub fn new(
        sort_index_map: Arc<SortIndexMap>,
        linear_graph: Arc<dyn LinearGraph>,
    ) -> SortedLinearGraph {
        SortedLinearGraph {
            linear_graph,
            sort_index_map,
        }
    }

    fn sorted_node_index(&self, node_id: Option<i32>) -> Option<i32> {
        node_id.map(|node_id| self.sort_index_map.get_sorted_index(node_id))
    }

    fn in_ranges(&self, index: i32) -> bool {
        index >= 0 && index < self.nodes_count()
    }
}

impl LinearGraph for SortedLinearGraph {
    fn nodes_count(&self) -> i32 {
        self.linear_graph.nodes_count()
    }

    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        self.linear_graph
            .get_adjacent_edges(self.sort_index_map.get_usual_index(node_index), filter)
            .into_iter()
            .map(|edge| {
                GraphEdge::new(
                    self.sorted_node_index(edge.up_node_index),
                    self.sorted_node_index(edge.down_node_index),
                    edge.target_id,
                    edge.edge_type,
                )
            })
            .collect()
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        debug_assert!(self.in_ranges(node_index));

        GraphNode::with_type(node_index, GraphNodeType::Usual)
    }

    /// See [`PermanentLinearGraphImpl::get_node_id`](crate::permanent::permanent_linear_graph_impl::PermanentLinearGraphImpl).
    fn get_node_id(&self, node_index: VcsLogVisibleGraphIndex) -> VcsLogGraphNodeId {
        self.sort_index_map.get_usual_index(node_index)
    }

    fn get_node_index(&self, node_id: VcsLogGraphNodeId) -> Option<VcsLogVisibleGraphIndex> {
        if !self.in_ranges(node_id) {
            return None;
        }

        Some(self.sort_index_map.get_sorted_index(node_id))
    }
}
```

- [ ] **Step 4: Run the new tests and watch them pass**

```bash
cargo test -q --locked -p ig-log --test bek
```

Expected: PASS.

- [ ] **Step 5: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code 8f9df63 -- crates/ig-log/tests/bek.rs crates/ig-log/tests/common/linear_bek.rs crates/ig-log/tests/common/mod.rs crates/ig-log/tests/common/test_graph_builder.rs crates/ig-log/src/facade/mod.rs crates/ig-log/src/facade/sort/bek/bek_branch.rs crates/ig-log/src/facade/sort/bek/bek_branch_creator.rs crates/ig-log/src/facade/sort/bek/bek_branch_merger.rs crates/ig-log/src/facade/sort/bek/bek_edge_restrictions.rs crates/ig-log/src/facade/sort/bek/bek_sorter.rs crates/ig-log/src/facade/sort/bek/mod.rs crates/ig-log/src/facade/sort/mod.rs crates/ig-log/src/facade/sort/sort_checker.rs crates/ig-log/src/facade/sort/sort_index_map.rs crates/ig-log/src/facade/sort/sorted_base_controller.rs
```

Expected: no output.

- [ ] **Step 6: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 570 passed, 1 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m 'feat(ig-log): BEK sort (SortIndexMap, BekSorter, SortedBaseController)'
```

---

### Task 3: Collapsed graph, dotted filter edges and fragment generators

**Files:**
- Create: `crates/ig-log/tests/dotted_filter_edges_generator.rs`, `crates/ig-log/tests/fragment_generator.rs`, `crates/ig-log/src/collapsing/collapsed_graph.rs`, `crates/ig-log/src/collapsing/dotted_filter_edges_generator.rs`, `crates/ig-log/src/collapsing/fragment_generator.rs`, `crates/ig-log/src/collapsing/graph_nodes_visibility.rs`, `crates/ig-log/src/collapsing/linear_fragment_generator.rs`, `crates/ig-log/src/collapsing/tree_set_node_iterator.rs`
- Modify: `crates/ig-log/src/collapsing/mod.rs`

**Source commit:** `741cdf8` on `wip/graph-extras-prototype`.

**Review focus:** `CollapsedGraph`'s `Arc<RwLock<_>>` state and lock re-entrancy (the compiled graph's edge wrapper takes the read lock per call; no lock may be held while the same lock is taken again), and the ascending iteration that replaces upstream's hash-set order in `FragmentGenerator`.

The collapsing machinery the filtering and collapsing controllers sit on. It comes before the controllers because `FirstParentTest` needs `CollapsedGraph` and the dotted-edge generator.

- [ ] **Step 1: Copy the tests first**

```bash
git checkout 741cdf8 -- crates/ig-log/tests/dotted_filter_edges_generator.rs crates/ig-log/tests/fragment_generator.rs
```

`crates/ig-log/tests/dotted_filter_edges_generator.rs`:

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/DottedFilterEdgesGeneratorTest.kt`
//! and `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/InplaceFilterEdgesGeneratorTest.kt`
//! (which uses the former's top-level `LinearGraph.assert`).

mod common;

use std::sync::Arc;

use common::test_graph_builder::{TestLinearGraph, as_test_graph_string, test_graph};
use ig_log::api::elements::GraphNodeType;
use ig_log::api::linear_graph::LinearGraph;
use ig_log::collapsing::collapsed_graph::CollapsedGraph;
use ig_log::utils::unsigned_bit_set::UnsignedBitSet;

/// `LinearGraph.assert(process, result)`.
fn assert_collapsed(
    graph: TestLinearGraph,
    process: impl FnOnce(&CollapsedGraph),
    result: TestLinearGraph,
) {
    let mut nodes_visibility = UnsignedBitSet::new();
    for node_index in 0..graph.nodes_count() {
        let graph_node = graph.get_graph_node(node_index);
        nodes_visibility.set(
            graph.get_node_id(node_index),
            graph_node.node_type == GraphNodeType::Usual,
        );
    }
    let collapsed_graph = CollapsedGraph::new_instance(Arc::new(graph), nodes_visibility);
    process(&collapsed_graph);

    let expected_result_graph = result;
    let actual_result_graph = collapsed_graph.compiled_graph();

    assert_eq!(
        as_test_graph_string(&expected_result_graph, true),
        as_test_graph_string(&*actual_result_graph, true)
    );
}

mod dotted_filter_edges_generator_test {
    use super::*;
    use ig_log::collapsing::dotted_filter_edges_generator;

    /// `LinearGraph.assert(upIndex = 0, downIndex = nodesCount() - 1, result)`.
    fn assert_graph(graph: TestLinearGraph, result: TestLinearGraph) {
        let down_index = graph.nodes_count() - 1;
        assert_range(graph, 0, down_index, result);
    }

    fn assert_range(
        graph: TestLinearGraph,
        up_index: i32,
        down_index: i32,
        result: TestLinearGraph,
    ) {
        assert_collapsed(
            graph,
            |collapsed_graph| {
                dotted_filter_edges_generator::update(collapsed_graph, up_index, down_index)
            },
            result,
        )
    }

    #[test]
    fn simple() {
        assert_graph(
            test_graph! {
                1(2)
                2.UNM(3)
                3()
            },
            test_graph! {
                1(3.dot)
                3()
            },
        )
    }

    #[test]
    fn simple2_up() {
        assert_graph(
            test_graph! {
                1(3)
                2(3)
                3.UNM(4)
                4()
            },
            test_graph! {
                1(4.dot)
                2(4.dot)
                4()
            },
        )
    }

    #[test]
    fn simple2_down() {
        assert_graph(
            test_graph! {
                1(2)
                2.UNM(3, 4)
                3()
                4()
            },
            test_graph! {
                1(3.dot, 4.dot)
                3()
                4()
            },
        )
    }

    /*
     0
     |\
     1 2
     |\|\
     3 4 5
    */
    #[test]
    fn down_tree() {
        assert_graph(
            test_graph! {
                0(1, 2)
                1.UNM(3, 4)
                2.UNM(4, 5)
                3()
                4()
                5()
            },
            test_graph! {
                0(3.dot, 4.dot, 5.dot)
                3()
                4()
                5()
            },
        )
    }

    /*
     0 1 2
     \/\/
     3 4
     \/
     5
    */
    #[test]
    fn up_tree() {
        assert_graph(
            test_graph! {
                0(3)
                1(3, 4)
                2(4)
                3.UNM(5)
                4.UNM(5)
                5()
            },
            test_graph! {
                0(5.dot)
                1(5.dot)
                2(5.dot)
                5()
            },
        )
    }

    /*
     1
     |\
     2 |
     | 3
     4 |
     | 5
     |/
     6
    */
    #[test]
    fn simple_merge() {
        assert_graph(
            test_graph! {
                1(2, 3)
                2(4)
                3(5)
                4.UNM(6)
                5.UNM(6)
                6()
            },
            test_graph! {
                1(2, 3)
                2(6.dot)
                3(6.dot)
                6()
            },
        )
    }

    #[test]
    fn simple_merge2() {
        assert_graph(
            test_graph! {
                1(2, 3)
                2.UNM(4)
                3.UNM(5)
                4(6)
                5(6)
                6()
            },
            test_graph! {
                1(4.dot, 5.dot)
                4(6)
                5(6)
                6()
            },
        )
    }

    #[test]
    fn fork() {
        assert_graph(
            test_graph! {
                0(2)
                1(3)
                2(3)
                3.UNM(4)
                4.UNM(5)
                5()
            },
            test_graph! {
                0(2)
                1(5.dot)
                2(5.dot)
                5()
            },
        )
    }

    /*
    0
    |
    1
    |\
    | 2
    3
    | 4
    |/
    5
    |
    6
    */
    #[test]
    #[ignore = "This test fails, there has to be a 0-6 dotted edge, but there isn't"]
    fn another_root() {
        assert_graph(
            test_graph! {
                0(1)
                1.UNM(2, 3)
                2()
                3.UNM(5)
                4(5)
                5.UNM(6)
                6()
            },
            test_graph! {
                0(2.dot, 6.dot)
                2()
                4(6.dot)
                6()
            },
        )
    }
}

mod inplace_filter_edges_generator_test {
    use super::*;
    use ig_log::collapsing::dotted_filter_edges_generator;
    use std::collections::HashSet;

    /// `LinearGraph.assert(vararg toHide, result)`.
    fn assert_hidden(graph: TestLinearGraph, to_hide: &[i32], result: TestLinearGraph) {
        let nodes_count = graph.nodes_count();
        assert_collapsed(
            graph,
            |collapsed_graph| {
                dotted_filter_edges_generator::update(collapsed_graph, 0, nodes_count - 1);
                let to_hide: HashSet<i32> = to_hide
                    .iter()
                    .map(|&it| collapsed_graph.compiled_graph().get_node_index(it).unwrap())
                    .collect();
                dotted_filter_edges_generator::hide_inplace(collapsed_graph, &to_hide);
            },
            result,
        )
    }

    /*
    1
    |
    2
    |
    3
    |
    4
    */
    #[test]
    fn simple_line() {
        assert_hidden(
            test_graph! {
                1(2)
                2.UNM(3)
                3(4)
                4()
            },
            &[3],
            test_graph! {
                1(4.dot)
                4()
            },
        )
    }

    /*
     0
     |\
     | 1
     2 |
     |/
     3
    */
    #[test]
    fn simple_triangular_merge() {
        assert_hidden(
            test_graph! {
                0(1, 2)
                1(3)
                2.UNM(3)
                3()
            },
            &[0],
            test_graph! {
                1(3)
                3()
            },
        )
    }

    /*
    1
    |
    2
    |\
    | 3
    5  \
    |   4
    6
    */
    #[test]
    fn two_roots_merge() {
        assert_hidden(
            test_graph! {
                1(2)
                2(3, 5)
                3.UNM(4)
                4()
                5.UNM(6)
                6()
            },
            &[2],
            test_graph! {
                1(4.dot, 6.dot)
                4()
                6()
            },
        )
    }

    /*
      0
     /|
    1 |
    | 2
    3 |
    | 4
    |/|
    5 |
    | 6
    7 |
    | 8
    |/|
    9 |
    |/
    10
    */
    #[test]
    fn multiple_merges() {
        assert_hidden(
            test_graph! {
                0(1, 2)
                1(3)
                2.UNM(4)
                3.UNM(5)
                4(5, 6)
                5(7)
                6.UNM(8)
                7.UNM(9)
                8(9, 10)
                9(10)
                10.UNM()
            },
            &[0, 4, 8],
            test_graph! {
                1(5.dot)
                5(9.dot)
                9()
            },
        )
    }
}
```

`crates/ig-log/tests/fragment_generator.rs`:

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/FragmentGeneratorTest.kt`.

mod common;

use std::collections::HashSet;
use std::sync::LazyLock;

use common::test_graph_builder::{TestLinearGraph, test_graph};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::collapsing::fragment_generator::{FragmentGenerator, GreenFragment};
use ig_log::utils::linear_graph_utils::{LiteLinearGraphAdapter, as_lite_linear_graph};

type Lite = LiteLinearGraphAdapter<&'static TestLinearGraph>;

fn lite(graph: &'static TestLinearGraph) -> &'static Lite {
    Box::leak(Box::new(as_lite_linear_graph(graph)))
}

fn get_middle_nodes(graph: &'static TestLinearGraph, up_node: i32, down_node: i32) -> HashSet<i32> {
    FragmentGenerator::new(lite(graph), |_| false).get_middle_nodes(up_node, down_node, false)
}

fn assert_nodes(nodes: HashSet<i32>, s: &str) {
    let mut sorted: Vec<i32> = nodes.into_iter().collect();
    sorted.sort();
    let joined: Vec<String> = sorted.iter().map(i32::to_string).collect();
    assert_eq!(s, joined.join(","));
}

fn assert_node(actual: Option<i32>, i: Option<i32>) {
    assert_eq!(i, actual);
}

fn red_nodes(
    graph: &'static TestLinearGraph,
    red_node: &[i32],
) -> FragmentGenerator<'static, Lite, impl Fn(i32) -> bool> {
    let red_nodes: HashSet<i32> = red_node.iter().copied().collect();

    FragmentGenerator::new(lite(graph), move |it| {
        red_nodes.contains(&graph.get_node_id(it))
    })
}

fn s(value: Option<i32>) -> String {
    value.map_or_else(|| "n".to_string(), |v| v.to_string())
}

fn assert_fragment(fragment: GreenFragment, expected: &str) {
    let mut middle: Vec<i32> = fragment.middle_green_nodes().iter().copied().collect();
    middle.sort();
    let middle: Vec<String> = middle.iter().map(i32::to_string).collect();
    assert_eq!(
        expected,
        format!(
            "{}|{}|{}",
            s(fragment.up_red_node()),
            s(fragment.down_red_node()),
            middle.join(",")
        )
    );
}

static SIMPLE: LazyLock<TestLinearGraph> = LazyLock::new(|| {
    test_graph! {
        0(1, 2)
        1(2)
        2()
    }
});

static TWO_BRANCH: LazyLock<TestLinearGraph> = LazyLock::new(|| {
    test_graph! {
        0(2)
        1(2)
        2()
    }
});

static DOWN_TREE: LazyLock<TestLinearGraph> = LazyLock::new(|| {
    test_graph! {
        0(1, 2)
        1(3, 4)
        2(4, 5)
        3()
        4()
        5()
    }
});

static UP_TREE: LazyLock<TestLinearGraph> = LazyLock::new(|| {
    test_graph! {
        0(3)
        1(3, 4)
        2(4)
        3(5)
        4(5)
        5()
    }
});

static DIFFICULT: LazyLock<TestLinearGraph> = LazyLock::new(|| {
    test_graph! {
        0(1, 2)
        1(3, 4)
        2(6)
        3(5)
        4(6)
        5(7, 8)
        6(7, 8)
        7()
        8()
    }
});

mod middle_nodes_test {
    use super::*;

    #[test]
    fn simple() {
        assert_nodes(get_middle_nodes(&SIMPLE, 0, 2), "0,1,2")
    }
    #[test]
    fn empty() {
        assert_nodes(get_middle_nodes(&TWO_BRANCH, 0, 1), "")
    }
    #[test]
    fn with_down_redundant_branches() {
        assert_nodes(get_middle_nodes(&DOWN_TREE, 0, 4), "0,1,2,4")
    }
    #[test]
    fn with_up_redundant_branches() {
        assert_nodes(get_middle_nodes(&UP_TREE, 1, 5), "1,3,4,5")
    }

    #[test]
    fn difficult1() {
        assert_nodes(get_middle_nodes(&DIFFICULT, 1, 7), "1,3,4,5,6,7")
    }
    #[test]
    fn difficult2() {
        assert_nodes(get_middle_nodes(&DIFFICULT, 0, 5), "0,1,3,5")
    }
    #[test]
    fn difficult3() {
        assert_nodes(get_middle_nodes(&DIFFICULT, 1, 8), "1,3,4,5,6,8")
    }
}

mod near_red_node {
    use super::*;

    #[test]
    fn simple1() {
        assert_node(
            red_nodes(&SIMPLE, &[0]).get_near_red_node(2, 10, true),
            Some(0),
        )
    }
    #[test]
    fn simple2() {
        assert_node(
            red_nodes(&SIMPLE, &[2]).get_near_red_node(0, 10, false),
            Some(2),
        )
    }
    #[test]
    fn simple3() {
        assert_node(
            red_nodes(&SIMPLE, &[2, 1]).get_near_red_node(0, 10, false),
            Some(1),
        )
    }
    #[test]
    fn simple4() {
        assert_node(
            red_nodes(&SIMPLE, &[0, 1]).get_near_red_node(0, 10, false),
            Some(0),
        )
    }
    #[test]
    fn simple5() {
        assert_node(
            red_nodes(&SIMPLE, &[0, 1]).get_near_red_node(2, 10, true),
            Some(1),
        )
    }

    #[test]
    fn down_tree1() {
        assert_node(
            red_nodes(&DOWN_TREE, &[4, 5]).get_near_red_node(0, 10, false),
            Some(4),
        )
    }
    #[test]
    fn down_tree2() {
        assert_node(
            red_nodes(&DOWN_TREE, &[2, 3]).get_near_red_node(0, 10, false),
            Some(2),
        )
    }
    #[test]
    fn up_tree1() {
        assert_node(
            red_nodes(&UP_TREE, &[1, 2]).get_near_red_node(5, 10, true),
            Some(2),
        )
    }
    #[test]
    fn up_tree2() {
        assert_node(
            red_nodes(&UP_TREE, &[4, 0]).get_near_red_node(5, 10, true),
            Some(4),
        )
    }

    #[test]
    fn difficult1() {
        assert_node(
            red_nodes(&DIFFICULT, &[4, 5, 6]).get_near_red_node(0, 10, false),
            Some(4),
        )
    }
    #[test]
    fn difficult2() {
        assert_node(
            red_nodes(&DIFFICULT, &[2, 5, 6]).get_near_red_node(1, 10, false),
            Some(5),
        )
    }

    #[test]
    fn null_answer1() {
        assert_node(
            red_nodes(&DIFFICULT, &[8]).get_near_red_node(0, 6, false),
            None,
        )
    }
    #[test]
    fn null_answer2() {
        assert_node(
            red_nodes(&DIFFICULT, &[8]).get_near_red_node(0, 7, false),
            Some(8),
        )
    }
}

mod green_fragment {
    use super::*;

    #[test]
    fn simple1() {
        assert_fragment(
            red_nodes(&SIMPLE, &[0]).get_green_fragment_for_collapse(2, 10),
            "0|n|1,2",
        )
    }
    #[test]
    fn simple2() {
        assert_fragment(
            red_nodes(&SIMPLE, &[0, 2]).get_green_fragment_for_collapse(1, 10),
            "0|2|1",
        )
    }
    #[test]
    fn simple3() {
        assert_fragment(
            red_nodes(&SIMPLE, &[0, 2]).get_green_fragment_for_collapse(0, 10),
            "n|n|",
        )
    }

    #[test]
    fn down_tree1() {
        assert_fragment(
            red_nodes(&DOWN_TREE, &[4]).get_green_fragment_for_collapse(2, 10),
            "n|4|0,2",
        )
    }
    #[test]
    fn down_tree2() {
        assert_fragment(
            red_nodes(&DOWN_TREE, &[4]).get_green_fragment_for_collapse(1, 10),
            "n|4|0,1",
        )
    }

    #[test]
    fn difficult1() {
        assert_fragment(
            red_nodes(&DIFFICULT, &[1, 7]).get_green_fragment_for_collapse(3, 10),
            "1|7|3,5",
        )
    }
    #[test]
    fn difficult2() {
        assert_fragment(
            red_nodes(&DIFFICULT, &[1, 7]).get_green_fragment_for_collapse(8, 10),
            "1|n|3,4,5,6,8",
        )
    }
    #[test]
    fn difficult3() {
        assert_fragment(
            red_nodes(&DIFFICULT, &[1, 7]).get_green_fragment_for_collapse(2, 10),
            "n|7|0,2,6",
        )
    }
}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -q --locked -p ig-log --test dotted_filter_edges_generator --test fragment_generator
```

Expected: FAIL — the collapsing modules don't exist yet, so both test crates fail to compile. The simulation's first error:

```text
error[E0432]: unresolved import `ig_log::collapsing::fragment_generator`
```

- [ ] **Step 3: Copy the code**

```bash
git checkout 741cdf8 -- crates/ig-log/src/collapsing/collapsed_graph.rs crates/ig-log/src/collapsing/dotted_filter_edges_generator.rs crates/ig-log/src/collapsing/fragment_generator.rs crates/ig-log/src/collapsing/graph_nodes_visibility.rs crates/ig-log/src/collapsing/linear_fragment_generator.rs crates/ig-log/src/collapsing/mod.rs crates/ig-log/src/collapsing/tree_set_node_iterator.rs
```

`crates/ig-log/src/collapsing/collapsed_graph.rs`:

```rust
//! A graph with some nodes of a delegate graph hidden and extra (dotted) edges added: the compiled
//! graph of the filtering and collapsing controllers.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/CollapsedGraph.java`.
//!
//! Deviations (Plan 2's mutation model):
//! - The state (visibility, node map, edge storage) lives behind an `Arc<RwLock<_>>` shared by the
//!   [`CollapsedGraph`], its compiled graph and its [`Modification`]s, so a modification can be
//!   applied later by a `Send + 'static` graph updater, in place.
//! - `assertNotUnderModification` is replaced by the lock: [`Modification::apply`] holds the write
//!   lock, so the compiled graph is never read half-modified.
//! - Only one modification may be running at a time, as upstream; a [`Modification`] dropped
//!   without [`apply`](Modification::apply) (e.g. a graph updater that is never run) ends the
//!   modification, where upstream would refuse every later one.
//! - `updateInstance` copies the visibility (upstream shares it with the old graph, which is
//!   dropped).
//! - The compiled graph's edge storage wrapper resolves indexes through the shared state, taking
//!   the read lock per call, so no lock is held while another is taken.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphNode};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::VcsLogGraphNodeId;
use crate::collapsing::edge_storage::EdgeStorage;
use crate::collapsing::edge_storage_wrapper::EdgeStorageWrapper;
use crate::collapsing::graph_nodes_visibility::GraphNodesVisibility;
use crate::utils::int_to_int_map::{IntToIntMap, UpdatableIntToIntMap};
use crate::utils::list_int_to_int_map::ListIntToIntMap;
use crate::utils::unsigned_bit_set::UnsignedBitSet;
use crate::visible_graph::VcsLogVisibleGraphIndex;

struct CollapsedState {
    delegated_graph: Arc<dyn LinearGraph>,
    /// Visibility by delegate node index (`myDelegateNodesVisibility` / `myNodesMap`).
    nodes_map: ListIntToIntMap<GraphNodesVisibility>,
}

impl CollapsedState {
    fn is_visible(&self, delegate_node_index: i32) -> bool {
        self.nodes_map.flags().is_visible(delegate_node_index)
    }

    /// `CompiledGraph.compiledNodeIndex`: `-1` for a hidden node.
    fn compiled_node_index(&self, delegate_node_index: Option<i32>) -> Option<i32> {
        let delegate_node_index = delegate_node_index?;
        if self.is_visible(delegate_node_index) {
            Some(self.nodes_map.get_short_index(delegate_node_index))
        } else {
            Some(-1)
        }
    }

    fn compiled_node_id(&self, node_index: i32) -> VcsLogGraphNodeId {
        let delegate_index = self.nodes_map.get_long_index(node_index);
        self.delegated_graph.get_node_id(delegate_index)
    }

    fn compiled_index_of_id(&self, node_id: VcsLogGraphNodeId) -> Option<VcsLogVisibleGraphIndex> {
        let delegate_index = self.delegated_graph.get_node_index(node_id)?;
        if self.is_visible(delegate_index) {
            Some(self.nodes_map.get_short_index(delegate_index))
        } else {
            None
        }
    }
}

type SharedState = Arc<RwLock<CollapsedState>>;

fn read(state: &SharedState) -> RwLockReadGuard<'_, CollapsedState> {
    // the state is only replaced field by field under the lock; a poisoned guard is still usable
    state.read().unwrap_or_else(PoisonError::into_inner)
}

fn write(state: &SharedState) -> RwLockWriteGuard<'_, CollapsedState> {
    state.write().unwrap_or_else(PoisonError::into_inner)
}

pub struct CollapsedGraph {
    state: SharedState,
    matched_node_id: Arc<UnsignedBitSet>,
    edge_storage: Arc<RwLock<EdgeStorage>>,
    compiled_graph: Arc<CompiledGraph>,
    modification_running: Arc<AtomicBool>,
}

impl CollapsedGraph {
    /// `newInstance`: nodes whose id is set in `matched_node_id` are visible.
    pub fn new_instance(
        delegate_graph: Arc<dyn LinearGraph>,
        matched_node_id: UnsignedBitSet,
    ) -> CollapsedGraph {
        let visible_nodes_id = matched_node_id.clone();
        CollapsedGraph::new(
            delegate_graph,
            Arc::new(matched_node_id),
            visible_nodes_id,
            Arc::new(RwLock::new(EdgeStorage::new())),
        )
    }

    /// `updateInstance`: the same visibility and edges over a new delegate graph.
    pub fn update_instance(
        prev_collapsed_graph: &CollapsedGraph,
        new_delegate_graph: Arc<dyn LinearGraph>,
    ) -> CollapsedGraph {
        let visible_nodes_id = read(&prev_collapsed_graph.state)
            .nodes_map
            .flags()
            .get_node_visibility_by_id()
            .clone();
        CollapsedGraph::new(
            new_delegate_graph,
            prev_collapsed_graph.matched_node_id.clone(),
            visible_nodes_id,
            prev_collapsed_graph.edge_storage.clone(),
        )
    }

    fn new(
        delegated_graph: Arc<dyn LinearGraph>,
        matched_node_id: Arc<UnsignedBitSet>,
        visible_nodes_id: UnsignedBitSet,
        edge_storage: Arc<RwLock<EdgeStorage>>,
    ) -> CollapsedGraph {
        let delegate_nodes_visibility =
            GraphNodesVisibility::new(delegated_graph.clone(), visible_nodes_id);
        let nodes_map = ListIntToIntMap::new_instance(delegate_nodes_visibility);
        let state = Arc::new(RwLock::new(CollapsedState {
            delegated_graph,
            nodes_map,
        }));
        let compiled_graph = Arc::new(CompiledGraph::new(state.clone(), edge_storage.clone()));
        CollapsedGraph {
            state,
            matched_node_id,
            edge_storage,
            compiled_graph,
            modification_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn delegated_graph(&self) -> Arc<dyn LinearGraph> {
        read(&self.state).delegated_graph.clone()
    }

    pub fn is_node_visible(&self, delegate_node_index: i32) -> bool {
        read(&self.state).is_visible(delegate_node_index)
    }

    /// # Panics
    /// If another modification is running (upstream `RuntimeException`).
    pub fn start_modification(&self) -> Modification {
        if self
            .modification_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            panic!("Can not start a new modification while the other one is still running.");
        }
        Modification {
            state: self.state.clone(),
            edge_storage: self.edge_storage.clone(),
            matched_node_id: self.matched_node_id.clone(),
            modification_running: self.modification_running.clone(),
            edges_to_add: EdgeStorageWrapper::create_simple_edge_storage(),
            edges_to_remove: EdgeStorageWrapper::create_simple_edge_storage(),
            nodes_to_hide: HashSet::new(),
            nodes_to_show: HashSet::new(),
            clear_edges: false,
            clear_visibility: false,
            min_affected_node_index: i32::MAX,
            max_affected_node_index: i32::MIN,
        }
    }

    pub fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.compiled_graph.clone()
    }

    /// # Panics
    /// If `compiled_node_index` is out of `0..compiled_graph().nodes_count()`.
    pub fn convert_to_delegate_node_index(&self, compiled_node_index: i32) -> i32 {
        read(&self.state)
            .nodes_map
            .get_long_index(compiled_node_index)
    }

    pub fn matched_node_id(&self) -> &UnsignedBitSet {
        &self.matched_node_id
    }

    /// Whether a normal edge between the two delegate node indexes was added by this graph
    /// (upstream "todo proper name").
    pub fn is_my_collapsed_edge(&self, up_node_index: i32, down_node_index: i32) -> bool {
        EdgeStorageWrapper::for_graph(self.edge_storage.clone(), self.delegated_graph())
            .has_edge(up_node_index, down_node_index)
    }
}

/// A batch of changes to a [`CollapsedGraph`]: collected first, then applied at once. Everywhere
/// here "node indexes" are node indexes in the delegated graph.
pub struct Modification {
    state: SharedState,
    edge_storage: Arc<RwLock<EdgeStorage>>,
    matched_node_id: Arc<UnsignedBitSet>,
    modification_running: Arc<AtomicBool>,
    edges_to_add: EdgeStorageWrapper,
    edges_to_remove: EdgeStorageWrapper,
    nodes_to_hide: HashSet<i32>,
    nodes_to_show: HashSet<i32>,
    clear_edges: bool,
    clear_visibility: bool,
    min_affected_node_index: i32,
    max_affected_node_index: i32,
}

impl Modification {
    fn touch_index(&mut self, node_index: i32) {
        self.min_affected_node_index = self.min_affected_node_index.min(node_index);
        self.max_affected_node_index = self.max_affected_node_index.max(node_index);
    }

    fn touch_all(&mut self) {
        self.min_affected_node_index = 0;
        self.max_affected_node_index = read(&self.state).delegated_graph.nodes_count() - 1;
    }

    fn touch_edge(&mut self, edge: &GraphEdge) {
        if let Some(up) = edge.up_node_index {
            self.touch_index(up);
        }
        if let Some(down) = edge.down_node_index {
            self.touch_index(down);
        }
    }

    pub fn show_node(&mut self, node_index: i32) {
        self.nodes_to_show.insert(node_index);
        self.touch_index(node_index);
    }

    pub fn hide_node(&mut self, node_index: i32) {
        self.nodes_to_hide.insert(node_index);
        self.touch_index(node_index);
    }

    pub fn create_edge(&mut self, edge: &GraphEdge) {
        self.edges_to_add.create_edge(edge);
        self.touch_edge(edge);
    }

    /// Upstream "todo add support for removing edge from delegate graph".
    pub fn remove_edge(&mut self, edge: &GraphEdge) {
        self.edges_to_remove.create_edge(edge);
        self.touch_edge(edge);
    }

    pub fn remove_additional_edges(&mut self) {
        self.clear_edges = true;
        self.touch_all();
    }

    pub fn reset_nodes_visibility(&mut self) {
        self.clear_visibility = true;
        self.touch_all();
    }

    /// Upstream "package private": "I'm not entirely happy about this method".
    pub fn edges_to_add(&self) -> &EdgeStorageWrapper {
        &self.edges_to_add
    }

    pub fn is_node_hidden(&self, node_index: i32) -> bool {
        self.nodes_to_hide.contains(&node_index)
    }

    pub fn is_node_shown(&self, node_index: i32) -> bool {
        self.nodes_to_show.contains(&node_index)
    }

    /// # Panics
    /// If `node_index` is out of the compiled graph's range.
    pub fn convert_to_delegate_node_index(&self, node_index: i32) -> i32 {
        read(&self.state).nodes_map.get_long_index(node_index)
    }

    pub fn apply(self) {
        let mut state = write(&self.state);

        if self.clear_visibility {
            state
                .nodes_map
                .flags_mut()
                .set_node_visibility_by_id((*self.matched_node_id).clone());
        }
        if self.clear_edges {
            self.edge_storage
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .remove_all();
        }

        for &to_show in &self.nodes_to_show {
            state.nodes_map.flags_mut().show(to_show);
        }
        for &to_hide in &self.nodes_to_hide {
            state.nodes_map.flags_mut().hide(to_hide);
        }

        let edge_storage_wrapper =
            EdgeStorageWrapper::for_graph(self.edge_storage.clone(), state.delegated_graph.clone());
        for edge in self.edges_to_add.get_edges() {
            edge_storage_wrapper.create_edge(&edge);
        }
        for edge in self.edges_to_remove.get_edges() {
            edge_storage_wrapper.remove_edge(&edge);
        }

        if self.min_affected_node_index != i32::MAX && self.max_affected_node_index != i32::MIN {
            state
                .nodes_map
                .update(self.min_affected_node_index, self.max_affected_node_index);
        }
        // `Drop` ends the modification
    }
}

impl Drop for Modification {
    fn drop(&mut self) {
        self.modification_running.store(false, Ordering::Release);
    }
}

/// The collapsed graph as a [`LinearGraph`]: visible delegate nodes, their delegate edges between
/// visible nodes, and the added edges.
struct CompiledGraph {
    state: SharedState,
    edge_storage_wrapper: EdgeStorageWrapper,
}

impl CompiledGraph {
    fn new(state: SharedState, edge_storage: Arc<RwLock<EdgeStorage>>) -> CompiledGraph {
        let index_state = state.clone();
        let id_state = state.clone();
        let edge_storage_wrapper = EdgeStorageWrapper::new(
            edge_storage,
            move |node_id| read(&index_state).compiled_index_of_id(node_id),
            move |node_index| read(&id_state).compiled_node_id(node_index),
        );
        CompiledGraph {
            state,
            edge_storage_wrapper,
        }
    }
}

fn is_visible_edge(compiled_up_node: Option<i32>, compiled_down_node: Option<i32>) -> bool {
    compiled_up_node != Some(-1) && compiled_down_node != Some(-1)
}

impl LinearGraph for CompiledGraph {
    fn nodes_count(&self) -> i32 {
        read(&self.state).nodes_map.short_size()
    }

    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        let mut result = Vec::new();
        {
            let state = read(&self.state);
            let delegate_index = state.nodes_map.get_long_index(node_index);

            // add delegate edges
            for delegate_edge in state
                .delegated_graph
                .get_adjacent_edges(delegate_index, filter)
            {
                let compiled_up_index = state.compiled_node_index(delegate_edge.up_node_index);
                let compiled_down_index = state.compiled_node_index(delegate_edge.down_node_index);
                if is_visible_edge(compiled_up_index, compiled_down_index) {
                    result.push(GraphEdge::new(
                        compiled_up_index,
                        compiled_down_index,
                        delegate_edge.target_id,
                        delegate_edge.edge_type,
                    ));
                }
            }
        }

        // the wrapper takes the read lock itself
        result.extend(
            self.edge_storage_wrapper
                .get_adjacent_edges(node_index, filter),
        );

        result
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        let state = read(&self.state);
        let delegate_index = state.nodes_map.get_long_index(node_index);
        let graph_node = state.delegated_graph.get_graph_node(delegate_index);
        GraphNode::with_type(node_index, graph_node.node_type)
    }

    fn get_node_id(&self, node_index: VcsLogVisibleGraphIndex) -> VcsLogGraphNodeId {
        read(&self.state).compiled_node_id(node_index)
    }

    fn get_node_index(&self, node_id: VcsLogGraphNodeId) -> Option<VcsLogVisibleGraphIndex> {
        read(&self.state).compiled_index_of_id(node_id)
    }
}
```

`crates/ig-log/src/collapsing/dotted_filter_edges_generator.rs`:

```rust
//! Dotted edges (and dotted arrows) that connect visible nodes across hidden ones, so a filtered
//! graph keeps its shape.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/DottedFilterEdgesGenerator.kt`
//! (`DottedFilterEdgesGenerator`, `DottedFilterEdgesGeneratorImpl`, `InplaceFilterEdgesGenerator`,
//! `ShiftNumber`).
//!
//! Deviation: the abstract class becomes [`DottedFilterEdgesGenerator`] over a
//! [`DottedFilterEdgesCallbacks`] implementation.
//!
//! Upstream quirk kept: `hasDottedEdges` returns `false` for the down direction even when it finds
//! a down dotted edge, so no edge or arrow is added to nodes below the updated range. Every caller
//! updates the whole graph, so there are no such nodes.

use std::collections::HashSet;

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphEdgeType};
use crate::api::linear_graph::{LiteLinearGraph, NodeFilter};
use crate::collapsing::collapsed_graph::{CollapsedGraph, Modification};
use crate::utils::linear_graph_utils::{as_lite_linear_graph, is_edge_down, is_edge_up};
use crate::visible_graph::VcsLogVisibleGraphIndex;

/// The abstract methods of `DottedFilterEdgesGenerator`.
pub trait DottedFilterEdgesCallbacks {
    fn node_is_visible(&self, node_index: VcsLogVisibleGraphIndex) -> bool;

    fn add_dotted_edge(
        &mut self,
        node_index1: VcsLogVisibleGraphIndex,
        node_index2: VcsLogVisibleGraphIndex,
    );

    fn add_dotted_arrow(&mut self, node_index: VcsLogVisibleGraphIndex, is_up: bool);

    fn has_dotted_edges(&self, node_index: VcsLogVisibleGraphIndex, is_up: bool) -> bool;
}

pub struct DottedFilterEdgesGenerator<'g, G: ?Sized, C> {
    graph: &'g G,
    up_index: VcsLogVisibleGraphIndex,
    down_index: VcsLogVisibleGraphIndex,
    numbers: ShiftNumber,
    callbacks: C,
}

impl<'g, G: LiteLinearGraph + ?Sized, C: DottedFilterEdgesCallbacks>
    DottedFilterEdgesGenerator<'g, G, C>
{
    pub fn new(
        graph: &'g G,
        up_index: VcsLogVisibleGraphIndex,
        down_index: VcsLogVisibleGraphIndex,
        callbacks: C,
    ) -> Self {
        DottedFilterEdgesGenerator {
            graph,
            up_index,
            down_index,
            numbers: ShiftNumber::new(up_index, down_index),
            callbacks,
        }
    }

    /// Updates the specified range.
    pub fn update(&mut self) {
        self.down_walk();
        self.cleanup();
        self.up_walk();
    }

    fn cleanup(&mut self) {
        for current_node_index in self.up_index..=self.down_index {
            self.numbers.set_number(current_node_index, i32::MAX);
        }
    }

    fn add_edge_or_arrow(
        &mut self,
        current_node_index: VcsLogVisibleGraphIndex,
        another_node_index: VcsLogVisibleGraphIndex,
        is_up: bool,
    ) {
        if self.callbacks.has_dotted_edges(current_node_index, is_up) {
            if self.callbacks.node_is_visible(another_node_index) {
                self.callbacks
                    .add_dotted_edge(current_node_index, another_node_index);
            } else {
                self.callbacks.add_dotted_arrow(current_node_index, is_up);
            }
        }
    }

    fn down_walk(&mut self) {
        for current_node_index in self.up_index..=self.down_index {
            if self.callbacks.node_is_visible(current_node_index) {
                let mut nearly_up = i32::MIN;
                let mut max_adj_number = i32::MIN;
                for up_node in self.graph.get_nodes(current_node_index, NodeFilter::Up) {
                    if up_node < self.up_index {
                        self.add_edge_or_arrow(current_node_index, up_node, true);
                        continue;
                    }

                    if self.callbacks.node_is_visible(up_node) {
                        max_adj_number = max_adj_number.max(self.numbers.get_number(up_node));
                    } else {
                        nearly_up = nearly_up.max(self.numbers.get_number(up_node));
                    }
                }

                if nearly_up == max_adj_number || nearly_up == i32::MIN {
                    self.numbers.set_number(current_node_index, max_adj_number);
                } else {
                    self.callbacks
                        .add_dotted_edge(current_node_index, nearly_up);
                    self.numbers.set_number(current_node_index, nearly_up);
                }
            } else {
                // node currentNodeIndex invisible

                let mut nearly_up = i32::MIN;
                for up_node in self.graph.get_nodes(current_node_index, NodeFilter::Up) {
                    if self.callbacks.node_is_visible(up_node) {
                        nearly_up = nearly_up.max(up_node);
                    } else if up_node >= self.up_index {
                        nearly_up = nearly_up.max(self.numbers.get_number(up_node));
                    }
                }
                self.numbers.set_number(current_node_index, nearly_up);
            }
        }
    }

    fn up_walk(&mut self) {
        for current_node_index in (self.up_index..=self.down_index).rev() {
            if self.callbacks.node_is_visible(current_node_index) {
                let mut nearly_down = i32::MAX;
                let mut min_adj_number = i32::MAX;
                for down_node in self.graph.get_nodes(current_node_index, NodeFilter::Down) {
                    if down_node > self.down_index {
                        self.add_edge_or_arrow(current_node_index, down_node, false);
                        continue;
                    }

                    if self.callbacks.node_is_visible(down_node) {
                        min_adj_number = min_adj_number.min(self.numbers.get_number(down_node));
                    } else {
                        nearly_down = nearly_down.min(self.numbers.get_number(down_node));
                    }
                }

                if nearly_down == min_adj_number || nearly_down == i32::MAX {
                    self.numbers.set_number(current_node_index, min_adj_number);
                } else {
                    self.callbacks
                        .add_dotted_edge(current_node_index, nearly_down);
                    self.numbers.set_number(current_node_index, nearly_down);
                }
            } else {
                // node currentNodeIndex invisible

                let mut nearly_down = i32::MAX;
                for down_node in self.graph.get_nodes(current_node_index, NodeFilter::Down) {
                    if self.callbacks.node_is_visible(down_node) {
                        nearly_down = nearly_down.min(down_node);
                    } else if down_node <= self.down_index {
                        nearly_down = nearly_down.min(self.numbers.get_number(down_node));
                    }
                }
                self.numbers.set_number(current_node_index, nearly_down);
            }
        }
    }
}

/// `DottedFilterEdgesGenerator.update(collapsedGraph, up, down)`: adds the dotted edges of the
/// delegate node range `up_delegate_node_index..=down_delegate_node_index`.
pub fn update(
    collapsed_graph: &CollapsedGraph,
    up_delegate_node_index: VcsLogVisibleGraphIndex,
    down_delegate_node_index: VcsLogVisibleGraphIndex,
) {
    let mut modification = collapsed_graph.start_modification();
    let delegated_graph = collapsed_graph.delegated_graph();
    let lite = as_lite_linear_graph(delegated_graph);
    DottedFilterEdgesGenerator::new(
        &lite,
        up_delegate_node_index,
        down_delegate_node_index,
        DottedFilterEdgesGeneratorImpl {
            collapsed_graph,
            modification: &mut modification,
        },
    )
    .update();
    modification.apply();
}

/// `DottedFilterEdgesGenerator.hideInplace`: hides the compiled rows `to_hide` and connects the
/// remaining rows with dotted edges.
pub fn hide_inplace(collapsed_graph: &CollapsedGraph, to_hide: &HashSet<VcsLogVisibleGraphIndex>) {
    let mut modification = collapsed_graph.start_modification();
    let compiled_graph = collapsed_graph.compiled_graph();
    let lite = as_lite_linear_graph(compiled_graph.clone());
    DottedFilterEdgesGenerator::new(
        &lite,
        0,
        compiled_graph.nodes_count() - 1,
        InplaceFilterEdgesGenerator {
            collapsed_graph,
            modification: &mut modification,
            to_hide,
        },
    )
    .update();
    for &it in to_hide {
        modification.hide_node(collapsed_graph.convert_to_delegate_node_index(it));
    }
    modification.apply();
}

/// `DottedFilterEdgesGeneratorImpl`: works on the delegate graph of a collapsed graph.
pub struct DottedFilterEdgesGeneratorImpl<'a> {
    collapsed_graph: &'a CollapsedGraph,
    modification: &'a mut Modification,
}

impl DottedFilterEdgesCallbacks for DottedFilterEdgesGeneratorImpl<'_> {
    fn node_is_visible(&self, node_index: VcsLogVisibleGraphIndex) -> bool {
        self.collapsed_graph.is_node_visible(node_index)
    }

    fn add_dotted_edge(
        &mut self,
        node_index1: VcsLogVisibleGraphIndex,
        node_index2: VcsLogVisibleGraphIndex,
    ) {
        self.modification
            .create_edge(&GraphEdge::create_normal_edge(
                node_index1,
                node_index2,
                GraphEdgeType::Dotted,
            ));
    }

    fn add_dotted_arrow(&mut self, node_index: VcsLogVisibleGraphIndex, is_up: bool) {
        self.modification
            .create_edge(&GraphEdge::create_edge_with_target_id(
                node_index,
                None,
                dotted_arrow_type(is_up),
            ));
    }

    fn has_dotted_edges(&self, node_index: VcsLogVisibleGraphIndex, is_up: bool) -> bool {
        has_dotted_edges(self.modification, node_index, is_up)
    }
}

fn dotted_arrow_type(is_up: bool) -> GraphEdgeType {
    if is_up {
        GraphEdgeType::DottedArrowUp
    } else {
        GraphEdgeType::DottedArrowDown
    }
}

/// `CollapsedGraph.Modification.hasDottedEdges` (see the module docs for its quirk).
fn has_dotted_edges(
    modification: &Modification,
    node_index: VcsLogVisibleGraphIndex,
    is_up: bool,
) -> bool {
    for edge in modification
        .edges_to_add()
        .get_adjacent_edges(node_index, EdgeFilter::NormalAll)
    {
        if edge.edge_type == GraphEdgeType::Dotted {
            if is_up && is_edge_up(&edge, node_index) {
                return true;
            }
            if !is_up && is_edge_down(&edge, node_index) {
                return false;
            }
        }
    }
    false
}

/// `InplaceFilterEdgesGenerator`: works on the compiled graph, hiding `to_hide`.
pub struct InplaceFilterEdgesGenerator<'a> {
    collapsed_graph: &'a CollapsedGraph,
    modification: &'a mut Modification,
    to_hide: &'a HashSet<VcsLogVisibleGraphIndex>,
}

impl DottedFilterEdgesCallbacks for InplaceFilterEdgesGenerator<'_> {
    fn node_is_visible(&self, node_index: VcsLogVisibleGraphIndex) -> bool {
        !self.to_hide.contains(&node_index)
    }

    fn add_dotted_edge(
        &mut self,
        node_index1: VcsLogVisibleGraphIndex,
        node_index2: VcsLogVisibleGraphIndex,
    ) {
        let delegate_index1 = self
            .collapsed_graph
            .convert_to_delegate_node_index(node_index1);
        let delegate_index2 = self
            .collapsed_graph
            .convert_to_delegate_node_index(node_index2);
        self.modification
            .create_edge(&GraphEdge::create_normal_edge(
                delegate_index1,
                delegate_index2,
                GraphEdgeType::Dotted,
            ));
    }

    fn add_dotted_arrow(&mut self, node_index: VcsLogVisibleGraphIndex, is_up: bool) {
        let delegate_index = self
            .collapsed_graph
            .convert_to_delegate_node_index(node_index);
        self.modification
            .create_edge(&GraphEdge::create_edge_with_target_id(
                delegate_index,
                None,
                dotted_arrow_type(is_up),
            ));
    }

    fn has_dotted_edges(&self, node_index: VcsLogVisibleGraphIndex, is_up: bool) -> bool {
        let delegate_index = self
            .collapsed_graph
            .convert_to_delegate_node_index(node_index);
        has_dotted_edges(self.modification, delegate_index, is_up)
    }
}

/// Numbers of the nodes `start_index..=end_index`; `-1` outside.
struct ShiftNumber {
    start_index: i32,
    end_index: i32,
    numbers: Vec<i32>,
}

impl ShiftNumber {
    fn new(start_index: i32, end_index: i32) -> ShiftNumber {
        ShiftNumber {
            start_index,
            end_index,
            numbers: vec![0; (end_index - start_index + 1).max(0) as usize],
        }
    }

    fn in_range(&self, node_index: i32) -> bool {
        (self.start_index..=self.end_index).contains(&node_index)
    }

    fn get_number(&self, node_index: i32) -> i32 {
        if self.in_range(node_index) {
            self.numbers[(node_index - self.start_index) as usize]
        } else {
            -1
        }
    }

    fn set_number(&mut self, node_index: i32, value: i32) {
        if self.in_range(node_index) {
            self.numbers[(node_index - self.start_index) as usize] = value;
        }
    }
}
```

`crates/ig-log/src/collapsing/fragment_generator.rs`:

```rust
//! Fragments between "red" nodes: the nodes between two nodes, the nearest red node, and the
//! green (collapsible) fragment around a node.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/FragmentGenerator.java`.

use std::collections::HashSet;

use crate::api::linear_graph::{LiteLinearGraph, NodeFilter};
use crate::collapsing::tree_set_node_iterator::TreeSetNodeIterator;

/// A fragment of non-red nodes between up to two red nodes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreenFragment {
    up_red_node: Option<i32>,
    down_red_node: Option<i32>,
    middle_green_nodes: HashSet<i32>,
}

impl GreenFragment {
    pub fn up_red_node(&self) -> Option<i32> {
        self.up_red_node
    }

    pub fn down_red_node(&self) -> Option<i32> {
        self.down_red_node
    }

    pub fn middle_green_nodes(&self) -> &HashSet<i32> {
        &self.middle_green_nodes
    }
}

pub struct FragmentGenerator<'g, G: ?Sized, R> {
    graph: &'g G,
    red_nodes: R,
}

impl<'g, G: LiteLinearGraph + ?Sized, R: Fn(i32) -> bool> FragmentGenerator<'g, G, R> {
    pub fn new(graph: &'g G, red_nodes: R) -> FragmentGenerator<'g, G, R> {
        FragmentGenerator { graph, red_nodes }
    }

    /// Nodes on paths from `up_node` down to `down_node` (`strict`: without the two ends).
    pub fn get_middle_nodes(&self, up_node: i32, down_node: i32, strict: bool) -> HashSet<i32> {
        let mut down_walk = self.get_walk_nodes(up_node, false, |integer| integer > down_node);
        let up_walk = self.get_walk_nodes(down_node, true, |integer| integer < up_node);

        down_walk.retain(|node| up_walk.contains(node));
        if strict {
            down_walk.remove(&up_node);
            down_walk.remove(&down_node);
        }
        down_walk
    }

    pub fn get_near_red_node(
        &self,
        start_node: i32,
        mut max_walk_size: i32,
        is_up: bool,
    ) -> Option<i32> {
        if (self.red_nodes)(start_node) {
            return Some(start_node);
        }

        let mut walker = TreeSetNodeIterator::new(start_node, is_up);
        while walker.not_empty() {
            let next = walker.pop();

            if (self.red_nodes)(next) {
                return Some(next);
            }

            if max_walk_size < 0 {
                return None;
            }
            max_walk_size -= 1;

            walker.add_all(&self.get_nodes(next, is_up));
        }

        None
    }

    pub fn get_green_fragment_for_collapse(
        &self,
        start_node: i32,
        max_walk_size: i32,
    ) -> GreenFragment {
        if (self.red_nodes)(start_node) {
            return GreenFragment {
                up_red_node: None,
                down_red_node: None,
                middle_green_nodes: HashSet::new(),
            };
        }
        let up_red_node = self.get_near_red_node(start_node, max_walk_size, true);
        let down_red_node = self.get_near_red_node(start_node, max_walk_size, false);

        let up_part = match up_red_node {
            Some(up_red_node) => self.get_middle_nodes(up_red_node, start_node, false),
            None => self.get_walk_nodes(start_node, true, create_stop_function(max_walk_size)),
        };

        let down_part = match down_red_node {
            Some(down_red_node) => self.get_middle_nodes(start_node, down_red_node, false),
            None => self.get_walk_nodes(start_node, false, create_stop_function(max_walk_size)),
        };

        let middle_nodes = up_part
            .union(&down_part)
            .copied()
            .filter(|&i| Some(i) != up_red_node && Some(i) != down_red_node)
            .collect();
        GreenFragment {
            up_red_node,
            down_red_node,
            middle_green_nodes: middle_nodes,
        }
    }

    fn get_walk_nodes(
        &self,
        start_node: i32,
        is_up: bool,
        mut stop_function: impl FnMut(i32) -> bool,
    ) -> HashSet<i32> {
        let mut walk_nodes = HashSet::new();

        let mut walker = TreeSetNodeIterator::new(start_node, is_up);
        while walker.not_empty() {
            let next = walker.pop();
            if !stop_function(next) {
                walk_nodes.insert(next);
                walker.add_all(&self.get_nodes(next, is_up));
            }
        }

        walk_nodes
    }

    fn get_nodes(&self, node_index: i32, is_up: bool) -> Vec<i32> {
        self.graph.get_nodes(node_index, NodeFilter::filter(is_up))
    }
}

/// Stops after `max_node_count` nodes.
fn create_stop_function(max_node_count: i32) -> impl FnMut(i32) -> bool {
    let mut count = max_node_count;
    move |_| {
        count -= 1;
        count < 0
    }
}
```

`crates/ig-log/src/collapsing/graph_nodes_visibility.rs`:

```rust
//! Visibility of a graph's nodes, stored by node id.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/GraphNodesVisibility.java`.
//!
//! Deviation: `GraphNodesVisibility` implements [`Flags`] itself (upstream: the `asFlags()` view),
//! so the [`ListIntToIntMap`](crate::utils::list_int_to_int_map::ListIntToIntMap) of
//! [`CollapsedGraph`](crate::collapsing::collapsed_graph::CollapsedGraph) can own it.

use std::sync::Arc;

use crate::api::linear_graph::LinearGraph;
use crate::utils::flags::Flags;
use crate::utils::unsigned_bit_set::UnsignedBitSet;

pub struct GraphNodesVisibility {
    linear_graph: Arc<dyn LinearGraph>,
    node_visibility_by_id: UnsignedBitSet,
}

impl GraphNodesVisibility {
    pub fn new(
        linear_graph: Arc<dyn LinearGraph>,
        node_visibility_by_id: UnsignedBitSet,
    ) -> GraphNodesVisibility {
        GraphNodesVisibility {
            linear_graph,
            node_visibility_by_id,
        }
    }

    pub fn get_node_visibility_by_id(&self) -> &UnsignedBitSet {
        &self.node_visibility_by_id
    }

    pub fn set_node_visibility_by_id(&mut self, node_visibility_by_id: UnsignedBitSet) {
        self.node_visibility_by_id = node_visibility_by_id;
    }

    pub fn is_visible(&self, node_index: i32) -> bool {
        self.node_visibility_by_id.get(self.node_id(node_index))
    }

    pub fn show(&mut self, node_index: i32) {
        let node_id = self.node_id(node_index);
        self.node_visibility_by_id.set(node_id, true);
    }

    pub fn hide(&mut self, node_index: i32) {
        let node_id = self.node_id(node_index);
        self.node_visibility_by_id.set(node_id, false);
    }

    fn node_id(&self, node_index: i32) -> i32 {
        self.linear_graph.get_node_id(node_index)
    }
}

/// `asFlags()`: visibility by node index.
impl Flags for GraphNodesVisibility {
    fn size(&self) -> i32 {
        self.linear_graph.nodes_count()
    }

    fn get(&self, index: i32) -> bool {
        self.node_visibility_by_id.get(self.node_id(index))
    }

    fn set(&mut self, index: i32, value: bool) {
        let node_id = self.node_id(index);
        self.node_visibility_by_id.set(node_id, value);
    }

    fn set_all(&mut self, value: bool) {
        for index in 0..self.size() {
            self.set(index, value);
        }
    }
}
```

`crates/ig-log/src/collapsing/linear_fragment_generator.rs`:

```rust
//! Linear fragments: the part of the graph between a node and the next node all of whose paths
//! meet again (what a hover highlights and a click collapses).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/LinearFragmentGenerator.java`.
//!
//! Deviation: the gray nodes of `getFragment` are a `BTreeSet` (ascending order; Java iterates its
//! `HashSet<Integer>` in ascending order for small node indexes). The fragment found does not
//! depend on that order.

use std::collections::{BTreeSet, HashSet};

use crate::api::elements::GraphElement;
use crate::api::linear_graph::{LiteLinearGraph, NodeFilter};
use crate::utils::linear_graph_utils::as_normal_edge;

const SHORT_FRAGMENT_MAX_SIZE: usize = 10;
const MAX_SEARCH_SIZE: usize = 10;

/// A fragment between two node indexes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphFragment {
    pub up_node_index: i32,
    pub down_node_index: i32,
}

impl GraphFragment {
    pub fn new(up_node_index: i32, down_node_index: i32) -> GraphFragment {
        GraphFragment {
            up_node_index,
            down_node_index,
        }
    }

    pub fn create(start_node: i32, end_node: i32) -> GraphFragment {
        GraphFragment::new(start_node, end_node)
    }
}

pub struct LinearFragmentGenerator<'g, G: ?Sized> {
    linear_graph: &'g G,
    pinned_nodes: HashSet<i32>,
}

impl<'g, G: LiteLinearGraph + ?Sized> LinearFragmentGenerator<'g, G> {
    pub fn new(linear_graph: &'g G, pinned_nodes: HashSet<i32>) -> LinearFragmentGenerator<'g, G> {
        LinearFragmentGenerator {
            linear_graph,
            pinned_nodes,
        }
    }

    fn up_nodes(&self, node: i32) -> Vec<i32> {
        self.linear_graph.get_nodes(node, NodeFilter::Up)
    }

    fn down_nodes(&self, node: i32) -> Vec<i32> {
        self.linear_graph.get_nodes(node, NodeFilter::Down)
    }

    pub fn get_relative_fragment(&self, element: &GraphElement) -> Option<GraphFragment> {
        let (mut up_node_index, down_node_index) = match element {
            GraphElement::Node(node) => (node.node_index, node.node_index),
            GraphElement::Edge(edge) => {
                let graph_edge = as_normal_edge(edge)?;
                (graph_edge.up, graph_edge.down)
            }
        };

        for _ in 0..MAX_SEARCH_SIZE {
            let graph_fragment = self.get_down_fragment(up_node_index);
            if let Some(graph_fragment) = graph_fragment
                && graph_fragment.down_node_index >= down_node_index
            {
                return Some(graph_fragment);
            }

            let up_nodes = self.up_nodes(up_node_index);
            if up_nodes.len() != 1 {
                break;
            }
            up_node_index = up_nodes[0];
        }

        None
    }

    pub fn get_down_fragment(&self, upper_visible_node_index: i32) -> Option<GraphFragment> {
        self.get_fragment(upper_visible_node_index, true)
    }

    pub fn get_up_fragment(&self, lower_node_index: i32) -> Option<GraphFragment> {
        self.get_fragment(lower_node_index, false)
    }

    pub fn get_long_down_fragment(&self, row_index: i32) -> Option<GraphFragment> {
        self.get_long_fragment_bounded(self.get_down_fragment(row_index), i32::MAX)
    }

    pub fn get_long_fragment(&self, element: &GraphElement) -> Option<GraphFragment> {
        self.get_long_fragment_bounded(self.get_relative_fragment(element), i32::MAX)
    }

    /// For hover.
    pub fn get_part_long_fragment(&self, element: &GraphElement) -> Option<GraphFragment> {
        self.get_long_fragment_bounded(self.get_relative_fragment(element), 500)
    }

    fn get_long_fragment_bounded(
        &self,
        start_fragment: Option<GraphFragment>,
        bound: i32,
    ) -> Option<GraphFragment> {
        let start_fragment = start_fragment?;

        let mut max_down = start_fragment.down_node_index;
        while let Some(short_fragment) = self.get_down_fragment(max_down) {
            if self.pinned_nodes.contains(&max_down) {
                break;
            }
            max_down = short_fragment.down_node_index;
            if max_down - start_fragment.down_node_index > bound {
                break;
            }
        }

        let mut max_up = start_fragment.up_node_index;
        while let Some(short_fragment) = self.get_up_fragment(max_up) {
            if self.pinned_nodes.contains(&max_up) {
                break;
            }
            max_up = short_fragment.up_node_index;
            if start_fragment.up_node_index - max_up > bound {
                break;
            }
        }

        if max_up != start_fragment.up_node_index || max_down != start_fragment.down_node_index {
            return Some(GraphFragment::new(max_up, max_down));
        }
        // start fragment is Simple
        if self.down_nodes(start_fragment.up_node_index).len() != 1 {
            return Some(start_fragment);
        }
        None
    }

    fn get_fragment(&self, start_node: i32, is_down: bool) -> Option<GraphFragment> {
        let get_next_nodes = |node| {
            if is_down {
                self.down_nodes(node)
            } else {
                self.up_nodes(node)
            }
        };
        let get_prev_nodes = |node| {
            if is_down {
                self.up_nodes(node)
            } else {
                self.down_nodes(node)
            }
        };
        let this_node_cant_be_in_middle = &self.pinned_nodes;

        let mut black_nodes = HashSet::new();
        black_nodes.insert(start_node);

        let mut gray_nodes: BTreeSet<i32> = get_next_nodes(start_node).into_iter().collect();

        let mut end_node = -1;
        while black_nodes.len() < SHORT_FRAGMENT_MAX_SIZE {
            let mut next_black_node = -1;
            for &gray_node in &gray_nodes {
                if get_prev_nodes(gray_node)
                    .iter()
                    .all(|prev| black_nodes.contains(prev))
                {
                    next_black_node = gray_node;
                    break;
                }
            }

            if next_black_node == -1 {
                return None;
            }

            if gray_nodes.len() == 1 {
                end_node = next_black_node;
                break;
            }

            let next_gray_nodes = get_next_nodes(next_black_node);
            if next_gray_nodes.is_empty() || this_node_cant_be_in_middle.contains(&next_black_node)
            {
                return None;
            }

            black_nodes.insert(next_black_node);
            gray_nodes.remove(&next_black_node);
            gray_nodes.extend(next_gray_nodes);
        }

        if end_node != -1 {
            Some(if is_down {
                GraphFragment::create(start_node, end_node)
            } else {
                GraphFragment::create(end_node, start_node)
            })
        } else {
            None
        }
    }
}
```

`crates/ig-log/src/collapsing/mod.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/collapsing/mod.rs b/crates/ig-log/src/collapsing/mod.rs
index 307eb11..b28ee7c 100644
--- a/crates/ig-log/src/collapsing/mod.rs
+++ b/crates/ig-log/src/collapsing/mod.rs
@@ -1,8 +1,13 @@
-//! Edge storage used by [`LinearGraphWrapper`](crate::facade::linear_graph_wrapper::LinearGraphWrapper).
+//! Filtering and collapsing: a graph with hidden nodes and dotted edges, the controllers that
+//! show it, and the fragment generators behind hover and collapse.
 //!
-//! Only `EdgeStorage.java` and `EdgeStorageWrapper.java` of
-//! `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/` are ported; the collapsing
-//! machinery (`CollapsedGraph`, `CollapsedController`, fragment generators, filters) is Plan 2b.
+//! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/`; see each submodule.
 
+pub mod collapsed_graph;
+pub mod dotted_filter_edges_generator;
 pub mod edge_storage;
 pub mod edge_storage_wrapper;
+pub mod fragment_generator;
+pub mod graph_nodes_visibility;
+pub mod linear_fragment_generator;
+pub mod tree_set_node_iterator;
```

`crates/ig-log/src/collapsing/tree_set_node_iterator.rs`:

```rust
//! A sorted work list of nodes: going up pops the greatest node first, going down the smallest.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/TreeSetNodeIterator.java`.

use std::collections::BTreeSet;

pub struct TreeSetNodeIterator {
    walk_nodes: BTreeSet<i32>,
    is_up: bool,
}

impl TreeSetNodeIterator {
    pub fn new(start_node: i32, is_up: bool) -> TreeSetNodeIterator {
        TreeSetNodeIterator {
            walk_nodes: BTreeSet::from([start_node]),
            is_up,
        }
    }

    /// # Panics
    /// If empty (upstream `NoSuchElementException`).
    pub fn pop(&mut self) -> i32 {
        let next = if self.is_up {
            self.walk_nodes.pop_last()
        } else {
            self.walk_nodes.pop_first()
        };
        next.expect("TreeSetNodeIterator is empty")
    }

    pub fn not_empty(&self) -> bool {
        !self.walk_nodes.is_empty()
    }

    pub fn add_all(&mut self, nodes: &[i32]) {
        self.walk_nodes.extend(nodes.iter().copied());
    }
}
```

- [ ] **Step 4: Run the new tests and watch them pass**

```bash
cargo test -q --locked -p ig-log --test dotted_filter_edges_generator --test fragment_generator
```

Expected: PASS.

- [ ] **Step 5: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code 741cdf8 -- crates/ig-log/tests/dotted_filter_edges_generator.rs crates/ig-log/tests/fragment_generator.rs crates/ig-log/src/collapsing/collapsed_graph.rs crates/ig-log/src/collapsing/dotted_filter_edges_generator.rs crates/ig-log/src/collapsing/fragment_generator.rs crates/ig-log/src/collapsing/graph_nodes_visibility.rs crates/ig-log/src/collapsing/linear_fragment_generator.rs crates/ig-log/src/collapsing/mod.rs crates/ig-log/src/collapsing/tree_set_node_iterator.rs
```

Expected: no output.

- [ ] **Step 6: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 610 passed, 2 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m 'feat(ig-log): collapsed graph, dotted filter edges and fragment generators'
```

---

### Task 4: Cascade, filtered, first-parent, collapsed and branch filter controllers

**Files:**
- Create: `crates/ig-log/tests/collapsing_controllers.rs`, `crates/ig-log/tests/first_parent.rs`, `crates/ig-log/src/collapsing/branch_filter_controller.rs`, `crates/ig-log/src/collapsing/collapsed_action_manager.rs`, `crates/ig-log/src/collapsing/collapsed_controller.rs`, `crates/ig-log/src/facade/cascade_controller.rs`, `crates/ig-log/src/facade/filtered_controller.rs`, `crates/ig-log/src/facade/first_parent_controller.rs`, `crates/ig-log/src/facade/graph_modification_util.rs`
- Modify: `crates/ig-log/tests/common/test_graph_builder.rs`, `crates/ig-log/src/actions.rs`, `crates/ig-log/src/api/printer.rs`, `crates/ig-log/src/collapsing/mod.rs`, `crates/ig-log/src/facade/linear_graph_controller.rs`, `crates/ig-log/src/facade/mod.rs`, `crates/ig-log/src/facade/visible_graph_impl.rs`, `crates/ig-log/src/print/elements.rs`

**Source commit:** `005e40a` on `wip/graph-extras-prototype`.

**Review focus:** `CascadeController` as a trait plus template function, the new `LinearGraphController` default methods (`delegate`, `delegate_mut`, `delegate_graph_changed`, `as_any_mut`, `supports_collapse`) that replace upstream's downcasts, `GraphModificationUtil`, and the stale-element guard (out-of-range node indexes act as "no element").

The controller chain pieces. `TestPermanentGraphInfo` is ported into the test builder here.

- [ ] **Step 1: Copy the tests first**

```bash
git checkout 005e40a -- crates/ig-log/tests/collapsing_controllers.rs crates/ig-log/tests/common/test_graph_builder.rs crates/ig-log/tests/first_parent.rs
```

`crates/ig-log/tests/collapsing_controllers.rs`:

```rust
//! IdeaGit tests for `CollapsedController`, `FilteredController` and `GraphModificationUtil` behind
//! a `VisibleGraphImpl`: fragment hover, collapse / expand on click and with the buttons, and hiding
//! commits. Upstream has no tests for these; expected values follow from the upstream algorithms.

mod common;

use std::collections::HashSet;
use std::sync::Arc;

use common::test_graph_builder::{TestPermanentGraphInfo, as_test_graph_string, test_graph};
use ig_log::actions::{Cursor, GraphAction, GraphActionType};
use ig_log::api::elements::GraphElement;
use ig_log::api::permanent::PermanentGraphInfo;
use ig_log::api::printer::{GraphColorGetter, GraphPrintElement};
use ig_log::collapsing::collapsed_controller::CollapsedController;
use ig_log::facade::base_controller::BaseController;
use ig_log::facade::filtered_controller::FilteredController;
use ig_log::facade::graph_modification_util::hide_commits;
use ig_log::facade::linear_graph_controller::LinearGraphController;
use ig_log::facade::visible_graph_impl::VisibleGraphImpl;
use ig_log::visible_graph::VisibleGraph;

struct NoColors;

impl GraphColorGetter for NoColors {
    fn get_node_color(&self, _node_id: i32, _layout_index: i32) -> i32 {
        0
    }
}

/*
0
1
|\
2 |
3 |
4 |
| 5
|/
6
7
*/
fn info(branch_nodes: &[i32]) -> Arc<TestPermanentGraphInfo> {
    Arc::new(TestPermanentGraphInfo::new(
        test_graph! {
            0(1)
            1(2, 5)
            2(3)
            3(4)
            4(6)
            5(6)
            6(7)
            7()
        },
        &[],
        branch_nodes.iter().copied().collect(),
    ))
}

fn visible(
    info: &Arc<TestPermanentGraphInfo>,
    controller: impl FnOnce(Arc<dyn PermanentGraphInfo<i32>>) -> Box<dyn LinearGraphController>,
) -> VisibleGraphImpl<i32> {
    let info: Arc<dyn PermanentGraphInfo<i32>> = info.clone();
    VisibleGraphImpl::new(controller(info.clone()), info, Arc::new(NoColors))
}

fn collapsed(info: &Arc<TestPermanentGraphInfo>) -> VisibleGraphImpl<i32> {
    visible(info, |info| {
        Box::new(CollapsedController::new(
            Box::new(BaseController::new(info.clone())),
            info,
            None,
        ))
    })
}

fn node(visible: &VisibleGraphImpl<i32>, row: i32) -> GraphPrintElement {
    visible
        .get_row_info(row)
        .print_elements()
        .into_iter()
        .find(|e| matches!(e, GraphPrintElement::Node(_)))
        .unwrap()
}

fn act_node(
    visible: &mut VisibleGraphImpl<i32>,
    row: i32,
    kind: GraphActionType,
) -> ig_log::actions::GraphAnswer<i32> {
    let element = node(visible, row);
    act(visible, Some(element), kind)
}

fn act(
    visible: &mut VisibleGraphImpl<i32>,
    element: Option<GraphPrintElement>,
    kind: GraphActionType,
) -> ig_log::actions::GraphAnswer<i32> {
    visible
        .action_controller()
        .perform_action(GraphAction::new(element, kind))
}

/// Commits of the rows whose node is printed selected.
fn selected_rows(visible: &VisibleGraphImpl<i32>) -> Vec<i32> {
    use ig_log::print_element::PrintElement as _;
    (0..visible.visible_commit_count())
        .filter(|&row| {
            visible
                .get_row_info(row)
                .print_elements()
                .iter()
                .any(|e| matches!(e, GraphPrintElement::Node(_)) && e.is_selected())
        })
        .map(|row| visible.get_row_info(row).commit())
        .collect()
}

fn graph_string(visible: &VisibleGraphImpl<i32>) -> String {
    as_test_graph_string(&*visible.linear_graph(), true)
}

#[test]
fn hovering_a_node_selects_its_linear_fragment() {
    let info = info(&[0]);
    let mut visible = collapsed(&info);
    let answer = act_node(&mut visible, 3, GraphActionType::MouseOver);
    assert_eq!(answer.cursor_to_set, Some(Cursor::Hand));
    assert!(answer.is_repaint_required);
    assert!(answer.graph_updater.is_none());
    assert_eq!(selected_rows(&visible), [2, 3, 4]);

    // hovering nothing clears the selection
    let answer = act(&mut visible, None, GraphActionType::MouseOver);
    assert!(answer.is_repaint_required);
    assert!(selected_rows(&visible).is_empty());
}

#[test]
fn clicking_collapses_and_expands_a_fragment() {
    let info = info(&[0]);
    let mut visible = collapsed(&info);
    let before = graph_string(&visible);

    act_node(&mut visible, 3, GraphActionType::MouseClick);
    assert_eq!(
        graph_string(&visible),
        "0(1)\n1(5, 2)\n2(4.dot)\n4(6)\n5(6)\n6(7)\n7()\n"
    );

    // hovering the collapsed edge's upper node selects the ends of the collapsed fragment
    act_node(&mut visible, 2, GraphActionType::MouseOver);
    assert_eq!(selected_rows(&visible), [2, 4]);

    act_node(&mut visible, 2, GraphActionType::MouseClick);
    assert_eq!(graph_string(&visible), before);
}

#[test]
fn collapse_all_and_expand_all_run_through_the_updater() {
    let info = info(&[0]);
    let mut visible = collapsed(&info);
    let before = graph_string(&visible);

    let answer = act(&mut visible, None, GraphActionType::ButtonCollapse);
    // nothing changes before the updater runs
    assert_eq!(graph_string(&visible), before);
    visible.run_graph_updater(answer.graph_updater.expect("collapse all updater"));
    assert_eq!(graph_string(&visible), "0(7.dot)\n7()\n");

    let answer = act(&mut visible, None, GraphActionType::ButtonExpand);
    visible.run_graph_updater(answer.graph_updater.expect("expand all updater"));
    assert_eq!(graph_string(&visible), before);

    // an updater that is never run does not block later modifications
    drop(act(&mut visible, None, GraphActionType::ButtonCollapse));
    act_node(&mut visible, 3, GraphActionType::MouseClick);
    assert_eq!(visible.visible_commit_count(), 7);
}

#[test]
fn visible_branches_hide_the_other_branches() {
    // only what is reachable from 5
    let info = info(&[0]);
    let mut visible = visible(&info, |info| {
        Box::new(CollapsedController::new(
            Box::new(BaseController::new(info.clone())),
            info,
            Some(&HashSet::from([5])),
        ))
    });
    assert_eq!(graph_string(&visible), "5(6)\n6(7)\n7()\n");
    // unknown and not-loaded heads are skipped
    let skipped = visible_with_heads(&info, &[-2, 5]);
    assert_eq!(graph_string(&skipped), "5(6)\n6(7)\n7()\n");
    act_node(&mut visible, 0, GraphActionType::MouseOver);
}

fn visible_with_heads(info: &Arc<TestPermanentGraphInfo>, heads: &[i32]) -> VisibleGraphImpl<i32> {
    let heads: HashSet<i32> = heads.iter().copied().collect();
    visible(info, |info| {
        Box::new(CollapsedController::new(
            Box::new(BaseController::new(info.clone())),
            info,
            Some(&heads),
        ))
    })
}

#[test]
fn filtered_graph_ignores_actions_and_hides_commits() {
    let info = info(&[0]);
    let matched: HashSet<i32> = [0, 2, 4, 5, 7].into_iter().collect();
    let mut visible = visible(&info, |info| {
        Box::new(FilteredController::create(
            Box::new(BaseController::new(info.clone())),
            &*info,
            &matched,
            None,
        ))
    });
    assert_eq!(
        graph_string(&visible),
        "0(5.dot, 2.dot)\n2(4.dot)\n4(7.dot)\n5(7.dot)\n7()\n"
    );
    let answer = act_node(&mut visible, 1, GraphActionType::MouseClick);
    assert_eq!(answer.cursor_to_set, Some(Cursor::Default));

    let info_dyn: Arc<dyn PermanentGraphInfo<i32>> = info.clone();
    let mut controller = FilteredController::create(
        Box::new(BaseController::new(info_dyn.clone())),
        &*info_dyn,
        &matched,
        None,
    );
    assert!(hide_commits(
        &mut controller,
        &*info_dyn,
        &HashSet::from([2, 4])
    ));
    assert_eq!(
        as_test_graph_string(&*controller.compiled_graph(), true),
        "0(7.dot, 5.dot)\n5(7.dot)\n7()\n"
    );

    // no filtered controller in the chain
    let mut base = BaseController::new(info_dyn.clone());
    assert!(!hide_commits(&mut base, &*info_dyn, &HashSet::from([2])));
}

#[test]
fn stale_elements_out_of_range_act_as_no_element() {
    let info = info(&[0]);
    let big = collapsed(&info);
    let stale = node(&big, 7);
    let mut small = visible(&info, |info| {
        Box::new(CollapsedController::new(
            Box::new(BaseController::new(info.clone())),
            info,
            Some(&HashSet::from([6])),
        ))
    });
    assert_eq!(small.visible_commit_count(), 2);
    for kind in [GraphActionType::MouseOver, GraphActionType::MouseClick] {
        let answer = act(&mut small, Some(stale.clone()), kind);
        assert_eq!(answer.cursor_to_set, Some(Cursor::Default));
        assert!(answer.graph_updater.is_none());
    }
    assert!(matches!(stale.graph_element(), GraphElement::Node(_)));
}
```

`crates/ig-log/tests/common/test_graph_builder.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/tests/common/test_graph_builder.rs b/crates/ig-log/tests/common/test_graph_builder.rs
index 8e9e860..825e666 100644
--- a/crates/ig-log/tests/common/test_graph_builder.rs
+++ b/crates/ig-log/tests/common/test_graph_builder.rs
@@ -4,7 +4,8 @@
 //! (`BaseTestGraphBuilder`, `TestGraphBuilder`, `graph { }`). Kotlin's `1(2, 3.dot)` becomes
 //! `b.node(1, [u(2), dot(3)])`; `2.UNM(3)` becomes `b.node(unm(2), [u(3)])`. The
 //! [`test_graph!`](crate::common::test_graph_builder::test_graph) macro accepts the Kotlin syntax
-//! itself (`test_graph! { 1(2, 3.dot) 2.UNM(3) 3() }`). Also ports `asTestGraphString`.
+//! itself (`test_graph! { 1(2, 3.dot) 2.UNM(3) 3() }`). Also ports `asTestGraphString` and
+//! `TestPermanentGraphInfo`.
 
 use std::collections::HashMap;
 
@@ -389,3 +390,90 @@ pub fn as_test_graph_string(graph: &(impl LinearGraph + ?Sized), sorted: bool) -
     }
     s
 }
+
+/// `TestPermanentGraphInfo`: a permanent graph info over a test graph whose commit ids are node
+/// ids (and timestamps). Heads are laid out by `heads_order` (by node id if empty).
+pub struct TestPermanentGraphInfo {
+    pub graph: std::sync::Arc<TestLinearGraph>,
+    commit_info: TestCommitsInfo,
+    graph_layout: ig_log::permanent::graph_layout_impl::GraphLayoutImpl,
+    branch_nodes: std::collections::HashSet<i32>,
+}
+
+struct TestCommitsInfo;
+
+impl ig_log::api::permanent::PermanentCommitsInfo<i32> for TestCommitsInfo {
+    fn get_commit_id(&self, node_id: i32) -> i32 {
+        node_id
+    }
+
+    fn try_get_commit_id(&self, node_id: i32) -> Option<i32> {
+        Some(node_id)
+    }
+
+    fn get_timestamp(&self, node_id: i32) -> i64 {
+        node_id as i64
+    }
+
+    fn get_node_id(&self, commit_id: &i32) -> i32 {
+        *commit_id
+    }
+
+    fn convert_to_node_ids(
+        &self,
+        heads: &std::collections::HashSet<i32>,
+    ) -> std::collections::HashSet<i32> {
+        heads.clone()
+    }
+}
+
+impl TestPermanentGraphInfo {
+    pub fn new(
+        graph: TestLinearGraph,
+        heads_order: &[i32],
+        branch_nodes: std::collections::HashSet<i32>,
+    ) -> TestPermanentGraphInfo {
+        let graph_layout = ig_log::permanent::graph_layout_builder::build(&graph, |x, y| {
+            if heads_order.is_empty() {
+                (graph.get_node_id(x) - graph.get_node_id(y)).cmp(&0)
+            } else {
+                let index_of = |n: i32| heads_order.iter().position(|&h| h == n);
+                let t = if index_of(x).is_none() {
+                    x
+                } else if index_of(y).is_none() {
+                    y
+                } else {
+                    -1
+                };
+                if t != -1 {
+                    panic!("Not found headsOrder for {t} node by id");
+                }
+                index_of(x).cmp(&index_of(y))
+            }
+        });
+        TestPermanentGraphInfo {
+            graph: std::sync::Arc::new(graph),
+            commit_info: TestCommitsInfo,
+            graph_layout,
+            branch_nodes,
+        }
+    }
+}
+
+impl ig_log::api::permanent::PermanentGraphInfo<i32> for TestPermanentGraphInfo {
+    fn permanent_commits_info(&self) -> &dyn ig_log::api::permanent::PermanentCommitsInfo<i32> {
+        &self.commit_info
+    }
+
+    fn linear_graph(&self) -> std::sync::Arc<dyn LinearGraph> {
+        self.graph.clone()
+    }
+
+    fn permanent_graph_layout(&self) -> &dyn ig_log::api::GraphLayout {
+        &self.graph_layout
+    }
+
+    fn branch_node_ids(&self) -> &std::collections::HashSet<i32> {
+        &self.branch_nodes
+    }
+}
```

`crates/ig-log/tests/first_parent.rs`:

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/facade/FirstParentTest.kt`.

mod common;

use std::collections::HashSet;
use std::sync::Arc;

use common::linear_bek::assert_graph_equals;
use common::test_graph_builder::{TestLinearGraph, test_graph};
use ig_log::facade::first_parent_controller::build_collapsed_graph;

fn set_of(elements: &[i32]) -> HashSet<i32> {
    elements.iter().copied().collect()
}

fn assert_first_parent(
    graph: TestLinearGraph,
    start_nodes: HashSet<i32>,
    matched_nodes: Option<HashSet<i32>>,
    expected: TestLinearGraph,
) {
    let actual_graph = build_collapsed_graph(Arc::new(graph), &start_nodes, matched_nodes.as_ref())
        .compiled_graph();
    assert_graph_equals(expected, &*actual_graph);
}

#[test]
fn simple_merge() {
    assert_first_parent(
        test_graph! {
            0(1)
            1(2, 3)
            2(4)
            3(4)
            4()
        },
        set_of(&[0]),
        None,
        test_graph! {
            0(1)
            1(2)
            2(4)
            4()
        },
    );
}

#[test]
fn two_branches() {
    assert_first_parent(
        test_graph! {
            0(2)
            1(5)
            2(3, 4)
            3(6)
            4(5)
            5(6)
            6()
        },
        set_of(&[0, 1]),
        None,
        test_graph! {
            0(2)
            1(5)
            2(3)
            3(6)
            5(6)
            6()
        },
    );
}

#[test]
fn start_not_at_head() {
    assert_first_parent(
        test_graph! {
            0(1)
            1(2, 3)
            2(4)
            3(4)
            4()
        },
        set_of(&[3]),
        None,
        test_graph! {
            3(4)
            4()
        },
    );
}

#[test]
fn feature_branch_merges() {
    // main branch merged into feature, then feature merged into master
    assert_first_parent(
        test_graph! {
            0(2)
            1(3)
            2(5, 3)
            3(4)
            4(6, 5)
            5(7)
            6(7)
            7()
        },
        set_of(&[0, 1]),
        None,
        test_graph! {
            0(2)
            1(3)
            2(5)
            3(4)
            4(6)
            5(7)
            6(7)
            7()
        },
    );
}

#[test]
fn filtered_merge() {
    assert_first_parent(
        test_graph! {
            0(1)
            1(2, 3)
            2(4)
            3(4)
            4()
        },
        set_of(&[0]),
        Some(set_of(&[1, 3, 4])),
        test_graph! {
            1(4.dot)
            4()
        },
    );
}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -q --locked -p ig-log --test collapsing_controllers --test first_parent
```

Expected: FAIL — the controllers don't exist yet, so `tests/first_parent.rs` and `tests/collapsing_controllers.rs` fail to compile. The simulation's first error:

```text
error[E0432]: unresolved import `ig_log::collapsing::collapsed_controller`
```

- [ ] **Step 3: Copy the code**

```bash
git checkout 005e40a -- crates/ig-log/src/actions.rs crates/ig-log/src/api/printer.rs crates/ig-log/src/collapsing/branch_filter_controller.rs crates/ig-log/src/collapsing/collapsed_action_manager.rs crates/ig-log/src/collapsing/collapsed_controller.rs crates/ig-log/src/collapsing/mod.rs crates/ig-log/src/facade/cascade_controller.rs crates/ig-log/src/facade/filtered_controller.rs crates/ig-log/src/facade/first_parent_controller.rs crates/ig-log/src/facade/graph_modification_util.rs crates/ig-log/src/facade/linear_graph_controller.rs crates/ig-log/src/facade/mod.rs crates/ig-log/src/facade/visible_graph_impl.rs crates/ig-log/src/print/elements.rs
```

`crates/ig-log/src/actions.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/actions.rs b/crates/ig-log/src/actions.rs
index eba5214..11bc167 100644
--- a/crates/ig-log/src/actions.rs
+++ b/crates/ig-log/src/actions.rs
@@ -80,10 +80,10 @@ pub struct GraphAnswer<Id> {
 /// Handles user actions on a visible graph.
 pub trait ActionController<Id> {
     /// The element must come from this visible graph's current `print_elements`.
-    /// Arrow elements whose node indexes are out of range, or whose not-loaded target is unknown,
-    /// are ignored (deviation: upstream throws). A stale element that is still in range acts on
-    /// whatever node now has that index, so drop hovered/selected elements when the visible graph
-    /// is replaced.
+    /// Elements whose node indexes are out of range are treated as no element, and arrows whose
+    /// not-loaded target is unknown are ignored (deviation: upstream throws). A stale element that
+    /// is still in range acts on whatever node now has that index, so drop hovered/selected
+    /// elements when the visible graph is replaced.
     fn perform_action(&mut self, graph_action: GraphAction) -> GraphAnswer<Id>;
 
     fn are_long_edges_hidden(&self) -> bool;
```

`crates/ig-log/src/api/printer.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/api/printer.rs b/crates/ig-log/src/api/printer.rs
index 823b704..70a0879 100644
--- a/crates/ig-log/src/api/printer.rs
+++ b/crates/ig-log/src/api/printer.rs
@@ -47,6 +47,20 @@ impl GraphPrintElement {
         }
     }
 
+    /// `CascadeController.convertPrintElement`: this print element standing for `graph_element`
+    /// (a node for a node, an edge for an edge; `None` otherwise).
+    pub fn with_graph_element(&self, graph_element: GraphElement) -> Option<GraphPrintElement> {
+        match (self, graph_element) {
+            (GraphPrintElement::Node(node), GraphElement::Node(graph_node)) => {
+                Some(GraphPrintElement::Node(node.with_graph_node(graph_node)))
+            }
+            (GraphPrintElement::Edge(edge), GraphElement::Edge(graph_edge)) => {
+                Some(GraphPrintElement::Edge(edge.with_graph_edge(graph_edge)))
+            }
+            _ => None,
+        }
+    }
+
     fn as_print_element(&self) -> &dyn PrintElement {
         match self {
             GraphPrintElement::Node(node) => node,
```

`crates/ig-log/src/collapsing/branch_filter_controller.rs`:

```rust
//! Shows the nodes reachable from the visible branches over a delegate that may change its graph
//! (linear BEK); no collapsing.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/BranchFilterController.kt`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::api::elements::GraphElement;
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::collapsing::collapsed_controller;
use crate::collapsing::collapsed_graph::CollapsedGraph;
use crate::facade::cascade_controller::{self, CascadeController};
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};
use crate::utils::graph_util::get_reachable_nodes;
use crate::utils::unsigned_bit_set::UnsignedBitSet;

pub struct BranchFilterController {
    delegate_controller: Box<dyn LinearGraphController>,
    collapsed_graph: CollapsedGraph,
    visibility: UnsignedBitSet,
}

impl BranchFilterController {
    pub fn new<CommitId>(
        delegate_linear_graph_controller: Box<dyn LinearGraphController>,
        permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
        ids_of_visible_branches: Option<&HashSet<i32>>,
    ) -> BranchFilterController {
        let visibility = get_reachable_nodes(
            &*permanent_graph_info.linear_graph(),
            ids_of_visible_branches,
        );
        let collapsed_graph = Self::update(&*delegate_linear_graph_controller, &visibility);
        BranchFilterController {
            delegate_controller: delegate_linear_graph_controller,
            collapsed_graph,
            visibility,
        }
    }

    fn update(
        delegate_controller: &dyn LinearGraphController,
        visibility: &UnsignedBitSet,
    ) -> CollapsedGraph {
        CollapsedGraph::new_instance(delegate_controller.compiled_graph(), visibility.clone())
    }
}

impl LinearGraphController for BranchFilterController {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.collapsed_graph.compiled_graph()
    }

    fn perform_linear_graph_action(&mut self, action: &LinearGraphAction) -> LinearGraphAnswer {
        cascade_controller::perform_linear_graph_action(self, action)
    }

    fn delegate(&self) -> Option<&dyn LinearGraphController> {
        Some(&*self.delegate_controller)
    }

    fn delegate_mut(&mut self) -> Option<&mut dyn LinearGraphController> {
        Some(&mut *self.delegate_controller)
    }

    fn delegate_graph_changed(&mut self, delegate_answer: LinearGraphAnswer) -> LinearGraphAnswer {
        if delegate_answer.graph_changes.is_some() {
            self.collapsed_graph = Self::update(&*self.delegate_controller, &self.visibility);
        }
        delegate_answer
    }
}

impl CascadeController for BranchFilterController {
    fn perform_action(&mut self, _action: &LinearGraphAction) -> Option<LinearGraphAnswer> {
        None
    }

    fn convert_to_delegate(&self, graph_element: &GraphElement) -> Option<GraphElement> {
        collapsed_controller::convert_to_delegate(graph_element, &self.collapsed_graph)
    }
}
```

`crates/ig-log/src/collapsing/collapsed_action_manager.rs`:

```rust
//! The actions of [`CollapsedController`](crate::collapsing::collapsed_controller::CollapsedController):
//! collapse / expand all, collapse or expand a linear fragment on click, and highlight it on hover.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/CollapsedActionManager.java`.
//!
//! Deviations:
//! - The action cases are functions tried in upstream's order.
//! - `expand_nodes` takes an ordered set (upstream: a `HashSet`).

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use crate::actions::GraphActionType;
use crate::actions::GraphUpdater;
use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphEdgeType, GraphElement};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::collapsing::collapsed_graph::CollapsedGraph;
use crate::collapsing::fragment_generator::FragmentGenerator;
use crate::collapsing::linear_fragment_generator::LinearFragmentGenerator;
use crate::facade::graph_changes::GraphChanges;
use crate::facade::linear_graph_controller::{LinearGraphAction, LinearGraphAnswer};
use crate::utils::linear_graph_utils::{
    LiteLinearGraphAdapter, as_lite_linear_graph, convert_ids_to_node_indexes,
    create_selected_answer,
};
use crate::utils::unsigned_bit_set::UnsignedBitSet;

/// `CollapsedActionManager.performAction`: the answer of the first action case that handles
/// `action`, or `None` for the delegate.
pub fn perform_action<CommitId>(
    collapsed_graph: &CollapsedGraph,
    permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
    action: &LinearGraphAction,
) -> Option<LinearGraphAnswer> {
    let context = ActionContext::new(collapsed_graph, permanent_graph_info, action);

    type ActionCase = fn(&ActionContext) -> Option<LinearGraphAnswer>;
    let filter_action_cases: [(ActionCase, &[GraphActionType]); 4] = [
        (collapse_all, &[GraphActionType::ButtonCollapse]),
        (expand_all, &[GraphActionType::ButtonExpand]),
        (
            linear_expand_case,
            &[GraphActionType::MouseClick, GraphActionType::MouseOver],
        ),
        (
            linear_collapse_case,
            &[GraphActionType::MouseClick, GraphActionType::MouseOver],
        ),
    ];
    for (action_case, supported_action_types) in filter_action_cases {
        if supported_action_types.contains(&context.action_type())
            && let Some(graph_answer) = action_case(&context)
        {
            return Some(graph_answer);
        }
    }
    None
}

/// Shows the hidden fragments around `nodes_to_show` (delegate node indexes).
pub fn expand_nodes(collapsed_graph: &CollapsedGraph, nodes_to_show: &BTreeSet<i32>) {
    let delegated_graph = as_lite_linear_graph(collapsed_graph.delegated_graph());
    let generator = FragmentGenerator::new(&delegated_graph, |node_index| {
        collapsed_graph.is_node_visible(node_index)
    });

    let mut modification = collapsed_graph.start_modification();

    for &node_to_show in nodes_to_show {
        if modification.is_node_shown(node_to_show) {
            continue;
        }

        let fragment = generator.get_green_fragment_for_collapse(node_to_show, i32::MAX);
        let (Some(up_red_node), Some(down_red_node)) =
            (fragment.up_red_node(), fragment.down_red_node())
        else {
            continue;
        };
        if up_red_node == down_red_node {
            continue;
        }

        for &n in fragment.middle_green_nodes() {
            modification.show_node(n);
        }

        modification.remove_edge(&GraphEdge::create_normal_edge(
            up_red_node,
            down_red_node,
            GraphEdgeType::Dotted,
        ));
    }

    modification.apply();
}

type Lite = LiteLinearGraphAdapter<Arc<dyn LinearGraph>>;

/// `FragmentGenerators`: the parts the generators of one graph need (they borrow these).
struct FragmentGenerators {
    linear_graph: Arc<dyn LinearGraph>,
    lite: Lite,
    branch_node_indexes: HashSet<i32>,
}

impl FragmentGenerators {
    fn new<CommitId>(
        linear_graph: Arc<dyn LinearGraph>,
        permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
    ) -> FragmentGenerators {
        let branch_node_indexes = convert_ids_to_node_indexes(
            &*linear_graph,
            permanent_graph_info.branch_node_ids().iter().copied(),
        );
        FragmentGenerators {
            lite: as_lite_linear_graph(linear_graph.clone()),
            linear_graph,
            branch_node_indexes,
        }
    }

    fn fragment_generator<'a>(
        &'a self,
        matched_node_id: &'a UnsignedBitSet,
    ) -> FragmentGenerator<'a, Lite, impl Fn(i32) -> bool + 'a> {
        let linear_graph = &self.linear_graph;
        FragmentGenerator::new(&self.lite, move |node_index| {
            matched_node_id.get(linear_graph.get_node_id(node_index))
        })
    }

    fn linear_fragment_generator(&self) -> LinearFragmentGenerator<'_, Lite> {
        LinearFragmentGenerator::new(&self.lite, self.branch_node_indexes.clone())
    }
}

struct ActionContext<'a> {
    collapsed_graph: &'a CollapsedGraph,
    graph_action: &'a LinearGraphAction,
    delegated_fragment_generators: FragmentGenerators,
    compiled_fragment_generators: FragmentGenerators,
}

impl<'a> ActionContext<'a> {
    fn new<CommitId>(
        collapsed_graph: &'a CollapsedGraph,
        permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
        graph_action: &'a LinearGraphAction,
    ) -> ActionContext<'a> {
        ActionContext {
            collapsed_graph,
            graph_action,
            delegated_fragment_generators: FragmentGenerators::new(
                collapsed_graph.delegated_graph(),
                permanent_graph_info,
            ),
            compiled_fragment_generators: FragmentGenerators::new(
                collapsed_graph.compiled_graph(),
                permanent_graph_info,
            ),
        }
    }

    fn action_type(&self) -> GraphActionType {
        self.graph_action.action_type
    }

    fn affected_graph_element(&self) -> Option<GraphElement> {
        self.graph_action
            .affected_element
            .as_ref()
            .map(|element| element.graph_element())
    }

    fn delegated_graph(&self) -> &Arc<dyn LinearGraph> {
        &self.delegated_fragment_generators.linear_graph
    }

    fn compiled_graph(&self) -> &Arc<dyn LinearGraph> {
        &self.compiled_fragment_generators.linear_graph
    }

    fn convert_to_delegate_node_index(&self, compiled_node_index: i32) -> i32 {
        self.collapsed_graph
            .convert_to_delegate_node_index(compiled_node_index)
    }

    fn convert_to_delegate_node_indexes(
        &self,
        compiled_node_indexes: &HashSet<i32>,
    ) -> HashSet<i32> {
        compiled_node_indexes
            .iter()
            .map(|&node_index| self.convert_to_delegate_node_index(node_index))
            .collect()
    }

    fn convert_to_delegate_edge(&self, compiled_edge: &GraphEdge) -> GraphEdge {
        let up_node_index = compiled_edge
            .up_node_index
            .map(|up| self.convert_to_delegate_node_index(up));
        let down_node_index = compiled_edge
            .down_node_index
            .map(|down| self.convert_to_delegate_node_index(down));

        GraphEdge::new(
            up_node_index,
            down_node_index,
            compiled_edge.target_id,
            compiled_edge.edge_type,
        )
    }
}

/// `LINEAR_COLLAPSE_CASE`: hover highlights the linear fragment, click collapses it.
fn linear_collapse_case(context: &ActionContext) -> Option<LinearGraphAnswer> {
    if is_for_delegate_graph(context) {
        return None;
    }

    let affected_graph_element = context.affected_graph_element()?;

    let compiled_linear_fragment_generator = context
        .compiled_fragment_generators
        .linear_fragment_generator();
    let matched_node_id = context.collapsed_graph.matched_node_id();
    let compiled_fragment_generator = context
        .compiled_fragment_generators
        .fragment_generator(matched_node_id);

    if context.action_type() == GraphActionType::MouseOver {
        let fragment =
            compiled_linear_fragment_generator.get_part_long_fragment(&affected_graph_element)?;
        let middle_compiled_nodes = compiled_fragment_generator.get_middle_nodes(
            fragment.up_node_index,
            fragment.down_node_index,
            false,
        );
        return Some(create_selected_answer(
            &**context.compiled_graph(),
            middle_compiled_nodes,
        ));
    }

    let fragment = compiled_linear_fragment_generator.get_long_fragment(&affected_graph_element)?;

    let middle_compiled_nodes = compiled_fragment_generator.get_middle_nodes(
        fragment.up_node_index,
        fragment.down_node_index,
        true,
    );
    let mut dotted_compiled_edges = HashSet::new();
    for &middle_node_index in &middle_compiled_nodes {
        dotted_compiled_edges.extend(
            context
                .compiled_graph()
                .get_adjacent_edges(middle_node_index, EdgeFilter::NormalAll)
                .into_iter()
                .filter(|edge| edge.edge_type == GraphEdgeType::Dotted),
        );
    }

    let up_node_index = context.convert_to_delegate_node_index(fragment.up_node_index);
    let down_node_index = context.convert_to_delegate_node_index(fragment.down_node_index);
    let middle_nodes = context.convert_to_delegate_node_indexes(&middle_compiled_nodes);
    let dotted_edges: HashSet<GraphEdge> = dotted_compiled_edges
        .iter()
        .map(|edge| context.convert_to_delegate_edge(edge))
        .collect();

    let mut modification = context.collapsed_graph.start_modification();
    for edge in &dotted_edges {
        modification.remove_edge(edge);
    }
    for &middle_node in &middle_nodes {
        modification.hide_node(middle_node);
    }
    modification.create_edge(&GraphEdge::new(
        Some(up_node_index),
        Some(down_node_index),
        None,
        GraphEdgeType::Dotted,
    ));

    modification.apply();
    Some(LinearGraphAnswer::with_changes(
        Some(GraphChanges::some_changes()),
        None,
    ))
}

/// `EXPAND_ALL`: the reset is applied by the answer's graph updater.
fn expand_all(context: &ActionContext) -> Option<LinearGraphAnswer> {
    let mut modification = context.collapsed_graph.start_modification();
    modification.remove_additional_edges();
    modification.reset_nodes_visibility();
    Some(LinearGraphAnswer::with_changes(
        Some(GraphChanges::some_changes()),
        Some(GraphUpdater::new(move || modification.apply())),
    ))
}

/// `COLLAPSE_ALL`: collapses every long fragment; applied by the answer's graph updater.
fn collapse_all(context: &ActionContext) -> Option<LinearGraphAnswer> {
    let mut modification = context.collapsed_graph.start_modification();
    modification.remove_additional_edges();
    modification.reset_nodes_visibility();

    let delegate_graph = context.delegated_graph();
    let linear_fragment_generator = context
        .delegated_fragment_generators
        .linear_fragment_generator();
    let fragment_generator = context
        .delegated_fragment_generators
        .fragment_generator(context.collapsed_graph.matched_node_id());
    for node_index in 0..delegate_graph.nodes_count() {
        if modification.is_node_hidden(node_index) {
            continue;
        }

        if let Some(fragment) = linear_fragment_generator.get_long_down_fragment(node_index) {
            let middle_nodes = fragment_generator.get_middle_nodes(
                fragment.up_node_index,
                fragment.down_node_index,
                true,
            );

            for &node_index_for_hide in &middle_nodes {
                modification.hide_node(node_index_for_hide);
            }
            modification.create_edge(&GraphEdge::new(
                Some(fragment.up_node_index),
                Some(fragment.down_node_index),
                None,
                GraphEdgeType::Dotted,
            ));
        }
    }

    Some(LinearGraphAnswer::with_changes(
        Some(GraphChanges::some_changes()),
        Some(GraphUpdater::new(move || modification.apply())),
    ))
}

/// `LINEAR_EXPAND_CASE`: hover highlights the ends of a collapsed fragment, click expands it.
fn linear_expand_case(context: &ActionContext) -> Option<LinearGraphAnswer> {
    if is_for_delegate_graph(context) {
        return None;
    }

    let dotted_edge = get_dotted_edge(
        context.affected_graph_element().as_ref(),
        &**context.compiled_graph(),
    )?;

    let up_node_index =
        context.convert_to_delegate_node_index(assert_int(dotted_edge.up_node_index));
    let down_node_index =
        context.convert_to_delegate_node_index(assert_int(dotted_edge.down_node_index));

    if context.action_type() == GraphActionType::MouseOver {
        return Some(create_selected_answer(
            &**context.delegated_graph(),
            [up_node_index, down_node_index],
        ));
    }

    let middle_nodes = context
        .delegated_fragment_generators
        .fragment_generator(context.collapsed_graph.matched_node_id())
        .get_middle_nodes(up_node_index, down_node_index, true);

    let mut modification = context.collapsed_graph.start_modification();
    for &middle_node in &middle_nodes {
        modification.show_node(middle_node);
    }
    modification.remove_edge(&GraphEdge::new(
        Some(up_node_index),
        Some(down_node_index),
        None,
        GraphEdgeType::Dotted,
    ));

    modification.apply();
    Some(LinearGraphAnswer::with_changes(
        Some(GraphChanges::some_changes()),
        None,
    ))
}

/// A dotted edge the collapsed graph did not add (e.g. a filter's), which the delegate handles.
fn is_for_delegate_graph(context: &ActionContext) -> bool {
    let Some(affected_graph_element) = context.affected_graph_element() else {
        return false;
    };

    if let Some(dotted_edge) =
        get_dotted_edge(Some(&affected_graph_element), &**context.compiled_graph())
    {
        let up_node_index =
            context.convert_to_delegate_node_index(assert_int(dotted_edge.up_node_index));
        let down_node_index =
            context.convert_to_delegate_node_index(assert_int(dotted_edge.down_node_index));

        if !context
            .collapsed_graph
            .is_my_collapsed_edge(up_node_index, down_node_index)
        {
            return true;
        }
    }
    false
}

/// A dotted edge always has both ends.
fn assert_int(value: Option<i32>) -> i32 {
    value.expect("dotted edge without a node index")
}

/// The element if it is a dotted edge, or the first dotted edge of a node.
fn get_dotted_edge(
    graph_element: Option<&GraphElement>,
    graph: &dyn LinearGraph,
) -> Option<GraphEdge> {
    match graph_element? {
        GraphElement::Edge(edge) if edge.edge_type == GraphEdgeType::Dotted => Some(*edge),
        GraphElement::Edge(_) => None,
        GraphElement::Node(node) => graph
            .get_adjacent_edges(node.node_index, EdgeFilter::NormalAll)
            .into_iter()
            .find(|edge| edge.edge_type == GraphEdgeType::Dotted),
    }
}
```

`crates/ig-log/src/collapsing/collapsed_controller.rs`:

```rust
//! Shows the graph of visible branches and handles collapsing, expanding and fragment hover.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/CollapsedController.java`.
//!
//! Upstream quirk kept: `convert_to_delegate` checks whether an edge is "my collapsed edge" with
//! the edge's compiled node indexes, while the edge storage is keyed by delegate node indexes.
//! Delegate actions on edges do nothing in every chain that has this controller, so it does not
//! matter.

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use crate::api::elements::{GraphEdge, GraphElement, GraphNode};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::collapsing::collapsed_action_manager;
use crate::collapsing::collapsed_graph::CollapsedGraph;
use crate::facade::cascade_controller::{self, CascadeController};
use crate::facade::graph_changes::GraphChanges;
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};
use crate::utils::graph_util::get_reachable_nodes;

pub struct CollapsedController<CommitId> {
    delegate_controller: Box<dyn LinearGraphController>,
    permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    collapsed_graph: CollapsedGraph,
}

impl<CommitId: 'static> CollapsedController<CommitId> {
    /// Shows the nodes reachable from `ids_of_visible_branches` (all nodes if `None`).
    pub fn new(
        delegate_linear_graph_controller: Box<dyn LinearGraphController>,
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
        ids_of_visible_branches: Option<&HashSet<i32>>,
    ) -> CollapsedController<CommitId> {
        let init_visibility = get_reachable_nodes(
            &*permanent_graph_info.linear_graph(),
            ids_of_visible_branches,
        );
        let collapsed_graph = CollapsedGraph::new_instance(
            delegate_linear_graph_controller.compiled_graph(),
            init_visibility,
        );
        CollapsedController {
            delegate_controller: delegate_linear_graph_controller,
            permanent_graph_info,
            collapsed_graph,
        }
    }

    pub fn collapsed_graph(&self) -> &CollapsedGraph {
        &self.collapsed_graph
    }

    pub fn permanent_graph_info(&self) -> &dyn PermanentGraphInfo<CommitId> {
        &*self.permanent_graph_info
    }

    fn apply_delegate_changes(&self, graph: &dyn LinearGraph, changes: &GraphChanges<i32>) {
        // ordered for determinism (upstream: a HashSet)
        let mut nodes_to_show = BTreeSet::new();

        for e in &changes.changed_edges {
            if !e.removed {
                if let Some(up_index) = e.up_node_id.and_then(|up_id| graph.get_node_index(up_id)) {
                    nodes_to_show.insert(up_index);
                }
                if let Some(down_index) = e
                    .down_node_id
                    .and_then(|down_id| graph.get_node_index(down_id))
                {
                    nodes_to_show.insert(down_index);
                }
            }
        }

        for e in &changes.changed_nodes {
            if !e.removed
                && let Some(node_index) = graph.get_node_index(e.node_id)
            {
                nodes_to_show.insert(node_index);
            }
        }

        collapsed_action_manager::expand_nodes(&self.collapsed_graph, &nodes_to_show);
    }
}

/// `CollapsedController.convertToDelegate(graphElement, collapsedGraph)`.
pub fn convert_to_delegate(
    graph_element: &GraphElement,
    collapsed_graph: &CollapsedGraph,
) -> Option<GraphElement> {
    match graph_element {
        GraphElement::Edge(edge) => {
            let up_index = edge.up_node_index;
            let down_index = edge.down_node_index;
            if let (Some(up), Some(down)) = (up_index, down_index)
                && collapsed_graph.is_my_collapsed_edge(up, down)
            {
                return None;
            }

            let converted_up_index =
                up_index.map(|up| collapsed_graph.convert_to_delegate_node_index(up));
            let converted_down_index =
                down_index.map(|down| collapsed_graph.convert_to_delegate_node_index(down));

            Some(GraphElement::Edge(GraphEdge::new(
                converted_up_index,
                converted_down_index,
                edge.target_id,
                edge.edge_type,
            )))
        }
        GraphElement::Node(node) => Some(GraphElement::Node(GraphNode::with_type(
            collapsed_graph.convert_to_delegate_node_index(node.node_index),
            node.node_type,
        ))),
    }
}

impl<CommitId: 'static> LinearGraphController for CollapsedController<CommitId> {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.collapsed_graph.compiled_graph()
    }

    fn perform_linear_graph_action(&mut self, action: &LinearGraphAction) -> LinearGraphAnswer {
        cascade_controller::perform_linear_graph_action(self, action)
    }

    fn delegate(&self) -> Option<&dyn LinearGraphController> {
        Some(&*self.delegate_controller)
    }

    fn delegate_mut(&mut self) -> Option<&mut dyn LinearGraphController> {
        Some(&mut *self.delegate_controller)
    }

    /// Upstream: "if somebody outside actually uses changes we return here they are screwed".
    fn delegate_graph_changed(&mut self, delegate_answer: LinearGraphAnswer) -> LinearGraphAnswer {
        if let Some(graph_changes) = &delegate_answer.graph_changes {
            let delegate_graph = self.delegate_controller.compiled_graph();
            self.collapsed_graph =
                CollapsedGraph::update_instance(&self.collapsed_graph, delegate_graph.clone());

            // some new edges and node appeared, so we expand them
            self.apply_delegate_changes(&*delegate_graph, graph_changes);
        }
        delegate_answer
    }
}

impl<CommitId: 'static> CascadeController for CollapsedController<CommitId> {
    fn perform_action(&mut self, action: &LinearGraphAction) -> Option<LinearGraphAnswer> {
        collapsed_action_manager::perform_action(
            &self.collapsed_graph,
            &*self.permanent_graph_info,
            action,
        )
    }

    fn convert_to_delegate(&self, graph_element: &GraphElement) -> Option<GraphElement> {
        convert_to_delegate(graph_element, &self.collapsed_graph)
    }
}
```

`crates/ig-log/src/collapsing/mod.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/collapsing/mod.rs b/crates/ig-log/src/collapsing/mod.rs
index b28ee7c..a5ecd94 100644
--- a/crates/ig-log/src/collapsing/mod.rs
+++ b/crates/ig-log/src/collapsing/mod.rs
@@ -3,6 +3,9 @@
 //!
 //! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/`; see each submodule.
 
+pub mod branch_filter_controller;
+pub mod collapsed_action_manager;
+pub mod collapsed_controller;
 pub mod collapsed_graph;
 pub mod dotted_filter_edges_generator;
 pub mod edge_storage;
```

`crates/ig-log/src/facade/cascade_controller.rs`:

```rust
//! The base of the controllers that wrap another controller (filters, collapsing, linear BEK).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/CascadeController.kt`.
//!
//! Deviations:
//! - The abstract class becomes the [`CascadeController`] trait (`performAction`,
//!   `convertToDelegate`) plus [`LinearGraphController`]'s `delegate*` default methods, which every
//!   cascade controller implements; [`perform_linear_graph_action`] is the template method.
//! - `permanentGraphInfo` is kept only by the controllers that use it.
//! - `convertPrintElement` is [`GraphPrintElement::with_graph_element`].

use crate::actions::GraphAction;
use crate::api::elements::GraphElement;
use crate::api::printer::GraphPrintElement;
use crate::facade::graph_changes::GraphChanges;
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};

pub trait CascadeController: LinearGraphController {
    /// `None` means that this action must be performed by the delegate controller.
    fn perform_action(&mut self, action: &LinearGraphAction) -> Option<LinearGraphAnswer>;

    /// The element of the delegate graph for `graph_element` of this controller's graph; `None` if
    /// there is none.
    fn convert_to_delegate(&self, graph_element: &GraphElement) -> Option<GraphElement> {
        Some(*graph_element)
    }
}

/// `CascadeController.performLinearGraphAction`: this controller's action, or the delegate's
/// (with the affected element converted) followed by
/// [`delegate_graph_changed`](LinearGraphController::delegate_graph_changed).
pub fn perform_linear_graph_action<C: CascadeController + ?Sized>(
    controller: &mut C,
    action: &LinearGraphAction,
) -> LinearGraphAnswer {
    if let Some(answer) = controller.perform_action(action) {
        return answer;
    }
    let delegate_action = GraphAction::new(
        convert_print_element_to_delegate(controller, action.affected_element.as_ref()),
        action.action_type,
    );
    let delegate_answer = match controller.delegate_mut() {
        Some(delegate) => delegate.perform_linear_graph_action(&delegate_action),
        None => unreachable!("a CascadeController has a delegate"),
    };
    controller.delegate_graph_changed(delegate_answer)
}

fn convert_print_element_to_delegate<C: CascadeController + ?Sized>(
    controller: &C,
    element: Option<&GraphPrintElement>,
) -> Option<GraphPrintElement> {
    let element = element?;
    let converted_graph_element = controller.convert_to_delegate(&element.graph_element())?;
    element.with_graph_element(converted_graph_element)
}

/// `performActionRecursively`: runs `action` on `controller`, then on its delegates until one
/// returns changes, which every controller above it then sees through `delegate_graph_changed`.
pub fn perform_action_recursively(
    controller: &mut dyn LinearGraphController,
    action: &mut dyn FnMut(&mut dyn LinearGraphController) -> Option<GraphChanges<i32>>,
) -> Option<GraphChanges<i32>> {
    if let Some(graph_changes) = action(controller) {
        return Some(graph_changes);
    }

    let delegate = controller.delegate_mut()?;
    let result = perform_action_recursively(delegate, action)?;
    controller
        .delegate_graph_changed(LinearGraphAnswer::with_changes(Some(result), None))
        .graph_changes
}
```

`crates/ig-log/src/facade/filtered_controller.rs`:

```rust
//! Shows the commits that match a filter, connected by dotted edges; no actions are passed on.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/FilteredController.kt`.
//!
//! Deviations:
//! - The constructor takes the built [`CollapsedGraph`] (upstream: a builder lambda it calls
//!   right away).
//! - `delegateGraphChanged` compares the answer by identity with `DEFAULT_GRAPH_ANSWER` upstream;
//!   here an answer without graph changes or updater counts as the default answer. Upstream throws
//!   for any other answer; that cannot happen, since a filtered controller never passes actions on
//!   and is never below another controller.

use std::any::Any;
use std::collections::HashSet;
use std::sync::Arc;

use crate::api::elements::GraphElement;
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::collapsing::collapsed_graph::CollapsedGraph;
use crate::collapsing::dotted_filter_edges_generator;
use crate::facade::cascade_controller::CascadeController;
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};
use crate::utils::graph_util::get_reachable_matching_nodes;
use crate::utils::linear_graph_utils::default_graph_answer;

pub struct FilteredController {
    delegate_controller: Box<dyn LinearGraphController>,
    pub collapsed_graph: CollapsedGraph,
}

impl FilteredController {
    pub fn new(
        delegate_linear_graph_controller: Box<dyn LinearGraphController>,
        collapsed_graph: CollapsedGraph,
    ) -> FilteredController {
        FilteredController {
            delegate_controller: delegate_linear_graph_controller,
            collapsed_graph,
        }
    }

    /// `FilteredController.create`: shows the `matched_ids` reachable from `visible_heads_ids`
    /// (all matched ids if `None`).
    pub fn create<CommitId>(
        delegate_controller: Box<dyn LinearGraphController>,
        permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
        matched_ids: &HashSet<i32>,
        visible_heads_ids: Option<&HashSet<i32>>,
    ) -> FilteredController {
        let visibility = get_reachable_matching_nodes(
            &*permanent_graph_info.linear_graph(),
            visible_heads_ids,
            Some(matched_ids),
        );
        let collapsed_graph =
            CollapsedGraph::new_instance(delegate_controller.compiled_graph(), visibility);
        dotted_filter_edges_generator::update(
            &collapsed_graph,
            0,
            collapsed_graph.delegated_graph().nodes_count() - 1,
        );
        FilteredController::new(delegate_controller, collapsed_graph)
    }
}

impl LinearGraphController for FilteredController {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.collapsed_graph.compiled_graph()
    }

    /// Filter prohibits any actions on delegate graph for now.
    fn perform_linear_graph_action(&mut self, action: &LinearGraphAction) -> LinearGraphAnswer {
        self.perform_action(action)
            .unwrap_or_else(default_graph_answer)
    }

    fn delegate(&self) -> Option<&dyn LinearGraphController> {
        Some(&*self.delegate_controller)
    }

    fn delegate_mut(&mut self) -> Option<&mut dyn LinearGraphController> {
        Some(&mut *self.delegate_controller)
    }

    /// # Panics
    /// For an answer with graph changes or an updater (upstream `UnsupportedOperationException`,
    /// "todo fix later"); unreachable, see the module docs.
    fn delegate_graph_changed(&mut self, delegate_answer: LinearGraphAnswer) -> LinearGraphAnswer {
        if delegate_answer.graph_changes.is_none() && delegate_answer.graph_updater.is_none() {
            return delegate_answer;
        }
        panic!("FilteredController does not support changes of its delegate graph");
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }

    fn supports_collapse(&self) -> bool {
        false
    }
}

impl CascadeController for FilteredController {
    fn perform_action(&mut self, _action: &LinearGraphAction) -> Option<LinearGraphAnswer> {
        None
    }

    /// Filter prohibits any actions on delegate graph for now.
    fn convert_to_delegate(&self, _graph_element: &GraphElement) -> Option<GraphElement> {
        None
    }
}
```

`crates/ig-log/src/facade/first_parent_controller.rs`:

```rust
//! Shows only the first parent of every merge, walking down from the heads.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/FirstParentController.kt`.

use std::collections::HashSet;
use std::sync::Arc;

use crate::api::EdgeFilter;
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::collapsing::collapsed_graph::CollapsedGraph;
use crate::collapsing::dotted_filter_edges_generator;
use crate::collapsing::edge_storage_wrapper::EdgeStorageWrapper;
use crate::facade::filtered_controller::FilteredController;
use crate::facade::linear_graph_controller::LinearGraphController;
use crate::facade::linear_graph_wrapper::LinearGraphWrapper;
use crate::permanent::graph_layout_builder::get_heads;
use crate::utils::dfs_util::NODE_NOT_FOUND;
use crate::utils::flags::{BitSetFlags, Flags};
use crate::utils::unsigned_bit_set::UnsignedBitSet;

/// `FirstParentController.create`: walks from `visible_heads_ids`, or from the branch heads and
/// the graph's heads if `None`; shows the `matched_ids` among the walked nodes (all if `None`).
pub fn create<CommitId>(
    delegate_controller: Box<dyn LinearGraphController>,
    permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
    matched_ids: Option<&HashSet<i32>>,
    visible_heads_ids: Option<&HashSet<i32>>,
) -> FilteredController {
    let linear_graph = permanent_graph_info.linear_graph();
    let start_nodes = match visible_heads_ids {
        Some(visible_heads_ids) => visible_heads_ids.clone(),
        None => permanent_graph_info
            .branch_node_ids()
            .iter()
            .copied()
            .chain(get_heads(&*linear_graph))
            .collect(),
    };
    let collapsed_graph = build_collapsed_graph(linear_graph, &start_nodes, matched_ids);
    FilteredController::new(delegate_controller, collapsed_graph)
}

/// The first-parent graph (visible for testing upstream).
pub fn build_collapsed_graph(
    linear_graph: Arc<dyn LinearGraph>,
    start_nodes: &HashSet<i32>,
    matched_nodes: Option<&HashSet<i32>>,
) -> CollapsedGraph {
    let (visible_nodes, hidden_edges) =
        get_visible_nodes_and_hidden_edges(&*linear_graph, start_nodes, matched_nodes);

    let collapsed_graph = CollapsedGraph::new_instance(
        Arc::new(LinearGraphWrapper::with_edges(
            linear_graph,
            hidden_edges,
            EdgeStorageWrapper::create_simple_edge_storage(),
        )),
        visible_nodes,
    );
    if matched_nodes.is_some() {
        dotted_filter_edges_generator::update(
            &collapsed_graph,
            0,
            collapsed_graph.delegated_graph().nodes_count() - 1,
        );
    }
    collapsed_graph
}

fn get_visible_nodes_and_hidden_edges(
    graph: &dyn LinearGraph,
    start_nodes: &HashSet<i32>,
    matched_nodes: Option<&HashSet<i32>>,
) -> (UnsignedBitSet, EdgeStorageWrapper) {
    let mut visible_nodes = UnsignedBitSet::new();
    let hidden_edges = EdgeStorageWrapper::create_simple_edge_storage();

    let mut visited = BitSetFlags::new(graph.nodes_count());
    for &start in start_nodes {
        if start < 0 {
            continue;
        }

        let mut node = start;
        while node != NODE_NOT_FOUND {
            if visited.get(node) {
                break;
            }
            visited.set(node, true);

            if matched_nodes.is_none_or(|matched| matched.contains(&node)) {
                visible_nodes.set(node, true);
            }

            let down_edges = graph.get_adjacent_edges(node, EdgeFilter::NormalDown);
            down_edges
                .iter()
                .skip(1)
                .for_each(|it| hidden_edges.create_edge(it));
            node = down_edges
                .first()
                .and_then(|edge| edge.down_node_index)
                .unwrap_or(NODE_NOT_FOUND);
        }
    }
    (visible_nodes, hidden_edges)
}
```

`crates/ig-log/src/facade/graph_modification_util.rs`:

```rust
//! Hiding commits in, and modifying, the collapsed graph of a filtered controller chain.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/GraphModificationUtil.kt`.
//! The `CollapsedGraph` / `Modification` extension functions are free functions.

use std::collections::HashSet;

use crate::api::elements::{GraphEdge, GraphEdgeType};
use crate::api::permanent::PermanentGraphInfo;
use crate::collapsing::collapsed_graph::{CollapsedGraph, Modification};
use crate::collapsing::dotted_filter_edges_generator;
use crate::facade::cascade_controller::perform_action_recursively;
use crate::facade::filtered_controller::FilteredController;
use crate::facade::graph_changes::GraphChanges;
use crate::facade::linear_graph_controller::LinearGraphController;

/// Hides `commits_to_hide` in the first filtered graph of the chain; `false` if there is none.
pub fn hide_commits<CommitId>(
    graph_controller: &mut dyn LinearGraphController,
    permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
    commits_to_hide: &HashSet<CommitId>,
) -> bool {
    modify_graph(graph_controller, |graph| {
        let node_ids = permanent_graph_info
            .permanent_commits_info()
            .convert_to_node_ids(commits_to_hide);
        let compiled_graph = graph.compiled_graph();
        let rows_to_hide: HashSet<i32> = node_ids
            .into_iter()
            .filter_map(|it| compiled_graph.get_node_index(it))
            .collect();
        dotted_filter_edges_generator::hide_inplace(graph, &rows_to_hide);
    })
}

/// Runs `consumer` on the collapsed graph of the first [`FilteredController`] of the chain;
/// `false` if there is none.
pub fn modify_graph(
    graph_controller: &mut dyn LinearGraphController,
    mut consumer: impl FnMut(&CollapsedGraph),
) -> bool {
    let result = perform_action_recursively(graph_controller, &mut |cc| {
        let filtered = cc
            .as_any_mut()
            .and_then(|any| any.downcast_mut::<FilteredController>())?;
        consumer(&filtered.collapsed_graph);
        Some(GraphChanges::some_changes())
    });
    result.is_some()
}

/// `CollapsedGraph.modify`: starts a modification, lets `modifier` fill it, and applies it.
pub fn modify(collapsed_graph: &CollapsedGraph, modifier: impl FnOnce(&mut Modification)) {
    let mut modification = collapsed_graph.start_modification();
    modifier(&mut modification);
    modification.apply();
}

/// `Modification.hideRow`: hides the compiled row `row`.
pub fn hide_row(modification: &mut Modification, row: i32) {
    let node_index = modification.convert_to_delegate_node_index(row);
    modification.hide_node(node_index);
}

/// `Modification.connectRows`: a dotted edge between the compiled rows.
pub fn connect_rows(modification: &mut Modification, up_row: i32, down_row: i32) {
    let up = modification.convert_to_delegate_node_index(up_row);
    let down = modification.convert_to_delegate_node_index(down_row);
    modification.create_edge(&GraphEdge::new(
        Some(up),
        Some(down),
        None,
        GraphEdgeType::Dotted,
    ));
}
```

`crates/ig-log/src/facade/linear_graph_controller.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/facade/linear_graph_controller.rs b/crates/ig-log/src/facade/linear_graph_controller.rs
index 5981793..9917a45 100644
--- a/crates/ig-log/src/facade/linear_graph_controller.rs
+++ b/crates/ig-log/src/facade/linear_graph_controller.rs
@@ -2,9 +2,18 @@
 //!
 //! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphController.kt`.
 //!
-//! Deviation: `compiled_graph` returns a shared handle; controllers that modify their graph (Plan 2b)
-//! hand out a new handle after each change, and the visible graph re-reads it.
+//! Deviations:
+//! - `compiled_graph` returns a shared handle. Controllers that change their graph change it in
+//!   place or hand out a new handle, and the visible graph re-reads it.
+//! - Upstream's downcasts are default methods: [`LinearGraphController::delegate`] /
+//!   [`delegate_mut`](LinearGraphController::delegate_mut) /
+//!   [`delegate_graph_changed`](LinearGraphController::delegate_graph_changed) for `is
+//!   CascadeController` (`performActionRecursively`), [`as_any_mut`](LinearGraphController::as_any_mut)
+//!   for `is FilteredController` (`GraphModificationUtil`) and
+//!   [`supports_collapse`](LinearGraphController::supports_collapse) for `VisibleGraphImpl`'s
+//!   `isActionSupported`.
 
+use std::any::Any;
 use std::collections::HashSet;
 use std::fmt;
 use std::sync::Arc;
@@ -69,4 +78,31 @@ pub trait LinearGraphController: Send {
     fn compiled_graph(&self) -> Arc<dyn LinearGraph>;
 
     fn perform_linear_graph_action(&mut self, action: &LinearGraphAction) -> LinearGraphAnswer;
+
+    /// The delegate of a `CascadeController`; `None` for other controllers.
+    fn delegate(&self) -> Option<&dyn LinearGraphController> {
+        None
+    }
+
+    /// See [`delegate`](Self::delegate).
+    fn delegate_mut(&mut self) -> Option<&mut dyn LinearGraphController> {
+        None
+    }
+
+    /// `CascadeController.delegateGraphChanged`: called after the delegate answered with
+    /// `delegate_answer`. Only called on controllers with a [`delegate`](Self::delegate).
+    fn delegate_graph_changed(&mut self, delegate_answer: LinearGraphAnswer) -> LinearGraphAnswer {
+        delegate_answer
+    }
+
+    /// For downcasts to a concrete controller; `None` unless the controller opts in.
+    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
+        None
+    }
+
+    /// Whether `BUTTON_COLLAPSE` / `BUTTON_EXPAND` are supported (upstream: `!is
+    /// FilteredController`).
+    fn supports_collapse(&self) -> bool {
+        true
+    }
 }
```

`crates/ig-log/src/facade/mod.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/facade/mod.rs b/crates/ig-log/src/facade/mod.rs
index 8688943..674997b 100644
--- a/crates/ig-log/src/facade/mod.rs
+++ b/crates/ig-log/src/facade/mod.rs
@@ -5,7 +5,11 @@
 //! `FirstParentController.kt`, `GraphModificationUtil.kt` and `SimpleGraphInfo.java`.
 
 pub mod base_controller;
+pub mod cascade_controller;
+pub mod filtered_controller;
+pub mod first_parent_controller;
 pub mod graph_changes;
+pub mod graph_modification_util;
 pub mod linear_graph_controller;
 pub mod linear_graph_wrapper;
 pub mod permanent_graph_impl;
```

`crates/ig-log/src/facade/visible_graph_impl.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/facade/visible_graph_impl.rs b/crates/ig-log/src/facade/visible_graph_impl.rs
index ece9286..1b38efe 100644
--- a/crates/ig-log/src/facade/visible_graph_impl.rs
+++ b/crates/ig-log/src/facade/visible_graph_impl.rs
@@ -8,9 +8,8 @@
 //!   rebuilds the print element generator (upstream wraps the `Runnable`).
 //! - `BUTTON_COLLAPSE` / `BUTTON_EXPAND` are reported unsupported until `CollapsedController`
 //!   is ported (Plan 2b); upstream supports them on every non-filtered graph.
-//! - Arrow actions on a stale print element (one that does not belong to this graph's current
-//!   `print_elements`, e.g. a hover racing a log refresh) are ignored and fall through to the
-//!   controller's default answer; upstream throws.
+//! - An action on a stale print element whose node indexes are out of this graph's range (e.g. a
+//!   hover racing a log refresh) is performed without an affected element; upstream throws.
 //! - `buildSimpleGraphInfo` (`SimpleGraphInfo`) is not ported yet.
 
 use std::collections::HashSet;
@@ -128,16 +127,6 @@ impl<CommitId: 'static> VisibleGraphImpl<CommitId> {
 
         let edge = edge_print_element.graph_edge();
 
-        // A stale element (from an older visible graph) may refer to nodes this graph does not
-        // have: ignore it (deviation: upstream throws).
-        let nodes_count = self.graph_controller.compiled_graph().nodes_count();
-        let in_range = |index: Option<VcsLogVisibleGraphIndex>| {
-            index.is_none_or(|index| (0..nodes_count).contains(&index))
-        };
-        if !in_range(edge.up_node_index) || !in_range(edge.down_node_index) {
-            return None;
-        }
-
         let mut target_id = None;
         if edge.edge_type == GraphEdgeType::NotLoadCommit {
             debug_assert!(edge_print_element.edge_print_type() == EdgePrintElementType::Down);
@@ -188,6 +177,26 @@ impl<CommitId: 'static> VisibleGraphImpl<CommitId> {
         }
     }
 
+    /// Deviation: drops an affected element that refers to nodes this graph does not have (a stale
+    /// element from an older visible graph); upstream throws.
+    fn without_stale_element(&self, mut action: GraphAction) -> GraphAction {
+        let nodes_count = self.graph_controller.compiled_graph().nodes_count();
+        let in_range = |index: Option<VcsLogVisibleGraphIndex>| {
+            index.is_none_or(|index| (0..nodes_count).contains(&index))
+        };
+        let stale = match action.affected_element.as_ref().map(|e| e.graph_element()) {
+            Some(GraphElement::Node(node)) => !in_range(Some(node.node_index)),
+            Some(GraphElement::Edge(edge)) => {
+                !in_range(edge.up_node_index) || !in_range(edge.down_node_index)
+            }
+            None => false,
+        };
+        if stale {
+            action.affected_element = None;
+        }
+        action
+    }
+
     fn convert_answer(answer: LinearGraphAnswer, selection_changed: bool) -> GraphAnswer<CommitId> {
         GraphAnswer {
             cursor_to_set: answer.cursor_to_set,
@@ -201,6 +210,7 @@ impl<CommitId: 'static> VisibleGraphImpl<CommitId> {
 
 impl<CommitId: 'static> ActionController<CommitId> for VisibleGraphImpl<CommitId> {
     fn perform_action(&mut self, graph_action: GraphAction) -> GraphAnswer<CommitId> {
+        let graph_action = self.without_stale_element(graph_action);
         if let Some(graph_answer) = self.perform_arrow_action(&graph_action) {
             return graph_answer;
         }
```

`crates/ig-log/src/print/elements.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/print/elements.rs b/crates/ig-log/src/print/elements.rs
index 06c56f2..bc5d377 100644
--- a/crates/ig-log/src/print/elements.rs
+++ b/crates/ig-log/src/print/elements.rs
@@ -49,6 +49,14 @@ impl SimplePrintElementImpl {
     pub fn graph_node(&self) -> GraphNode {
         self.graph_node
     }
+
+    /// The same element for another graph node (`CascadeController.convertPrintElement`).
+    pub(crate) fn with_graph_node(&self, graph_node: GraphNode) -> SimplePrintElementImpl {
+        SimplePrintElementImpl {
+            graph_node,
+            ..self.clone()
+        }
+    }
 }
 
 impl PrintElement for SimplePrintElementImpl {
@@ -154,6 +162,14 @@ impl EdgePrintElementImpl {
         self.graph_edge
     }
 
+    /// The same element for another graph edge (`CascadeController.convertPrintElement`).
+    pub(crate) fn with_graph_edge(&self, graph_edge: GraphEdge) -> EdgePrintElementImpl {
+        EdgePrintElementImpl {
+            graph_edge,
+            ..self.clone()
+        }
+    }
+
     pub fn convert_to_line_style(edge_type: GraphEdgeType) -> LineStyle {
         match edge_type {
             GraphEdgeType::Usual | GraphEdgeType::NotLoadCommit => LineStyle::Solid,
```

- [ ] **Step 4: Run the new tests and watch them pass**

```bash
cargo test -q --locked -p ig-log --test collapsing_controllers --test first_parent
```

Expected: PASS.

- [ ] **Step 5: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code 005e40a -- crates/ig-log/tests/collapsing_controllers.rs crates/ig-log/tests/common/test_graph_builder.rs crates/ig-log/tests/first_parent.rs crates/ig-log/src/actions.rs crates/ig-log/src/api/printer.rs crates/ig-log/src/collapsing/branch_filter_controller.rs crates/ig-log/src/collapsing/collapsed_action_manager.rs crates/ig-log/src/collapsing/collapsed_controller.rs crates/ig-log/src/collapsing/mod.rs crates/ig-log/src/facade/cascade_controller.rs crates/ig-log/src/facade/filtered_controller.rs crates/ig-log/src/facade/first_parent_controller.rs crates/ig-log/src/facade/graph_modification_util.rs crates/ig-log/src/facade/linear_graph_controller.rs crates/ig-log/src/facade/mod.rs crates/ig-log/src/facade/visible_graph_impl.rs crates/ig-log/src/print/elements.rs
```

Expected: no output.

- [ ] **Step 6: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 621 passed, 2 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m 'feat(ig-log): cascade, filtered, first-parent, collapsed and branch filter controllers'
```

---

### Task 5: Linear BEK (LinearBekGraph, LinearBekGraphBuilder, LinearBekController)

**Files:**
- Create: `crates/ig-log/tests/linear_bek.rs`, `crates/ig-log/tests/linear_bek_expand.rs`, `crates/ig-log/src/linear_bek/linear_bek_controller.rs`, `crates/ig-log/src/linear_bek/linear_bek_graph.rs`, `crates/ig-log/src/linear_bek/linear_bek_graph_builder.rs`, `crates/ig-log/src/linear_bek/mod.rs`, `crates/ig-log/src/utils/java_priority_queue.rs`
- Modify: `crates/ig-log/tests/common/linear_bek.rs`, `crates/ig-log/src/lib.rs`, `crates/ig-log/src/utils/mod.rs`

**Source commit:** `6ac6c8a` on `wip/graph-extras-prototype`.

**Review focus:** `LinearBekGraphBuilder` against `linearBek/LinearBekGraphBuilder.java`, and `utils::java_priority_queue`: the order in which tied edges leave Java's `PriorityQueue` changes the result, so it must match `java.util.PriorityQueue` exactly.

The "IntelliJ style" sort. `LinearBekTest` was converted from Kotlin by script; its graph data was checked digit for digit against upstream.

- [ ] **Step 1: Copy the tests first**

```bash
git checkout 6ac6c8a -- crates/ig-log/tests/common/linear_bek.rs crates/ig-log/tests/linear_bek.rs crates/ig-log/tests/linear_bek_expand.rs
```

`crates/ig-log/tests/common/linear_bek.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/tests/common/linear_bek.rs b/crates/ig-log/tests/common/linear_bek.rs
index 0749bf9..c26c6cb 100644
--- a/crates/ig-log/tests/common/linear_bek.rs
+++ b/crates/ig-log/tests/common/linear_bek.rs
@@ -8,6 +8,8 @@ use ig_log::api::linear_graph::LinearGraph;
 use ig_log::facade::sort::bek::bek_sorter;
 use ig_log::facade::sort::sort_checker;
 use ig_log::facade::sort::sorted_base_controller::SortedLinearGraph;
+use ig_log::linear_bek::linear_bek_graph::LinearBekGraph;
+use ig_log::linear_bek::linear_bek_graph_builder::LinearBekGraphBuilder;
 use ig_log::permanent::graph_layout_builder;
 use ig_log::permanent::graph_layout_impl::GraphLayoutImpl;
 use ig_log::utils::TimestampGetter;
@@ -65,3 +67,12 @@ pub fn assert_graph_equals(expected: TestLinearGraph, actual: &(impl LinearGraph
         as_test_graph_string(actual, false)
     );
 }
+
+/// `runLinearBek`: takes the graph built by the test's builder (Kotlin builds it twice).
+pub fn run_linear_bek(before_linear_bek: TestLinearGraph) -> LinearBekGraph {
+    let before_linear_bek_layout = build_layout(&before_linear_bek);
+
+    let after_linear_bek = LinearBekGraph::new(Arc::new(before_linear_bek));
+    LinearBekGraphBuilder::new(&after_linear_bek, &before_linear_bek_layout).collapse_all();
+    after_linear_bek
+}
```

`crates/ig-log/tests/linear_bek.rs`:

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/LinearBekTest.kt`.

mod common;

use common::linear_bek::{assert_graph_equals, run_linear_bek};
use common::test_graph_builder::{TestLinearGraph, test_graph};

fn run_test(before_linear_bek: TestLinearGraph, after_linear_bek: TestLinearGraph) {
    assert_graph_equals(after_linear_bek, &run_linear_bek(before_linear_bek));
}

/*
0         0
| \       |
| 1       1
| 2       2
| 3       3
| |       :
4 |  ->   4
5 |       5
6 |       6
|/        |
7         7
*/
#[test]
fn simple_graph_test() {
    run_test(
        test_graph! {
            0(4, 1)
            1(2)
            2(3)
            3(7)
            4(5)
            5(6)
            6(7)
            7()
        },
        test_graph! {
            0(1)
            1(2)
            2(3)
            3(4.dot)
            4(5)
            5(6)
            6(7)
            7()
        },
    )
}

/*
0           0
|\          |
| 1         1
| |         :
2 |         2
|/          |
3    ->     3
|\          |
| 4         4
| |         :
5 |         5
|/          |
6           6
 */
#[test]
fn two_sections_graph_test() {
    run_test(
        test_graph! {
            0(2, 1)
            1(3)
            2(3)
            3(5, 4)
            4(6)
            5(6)
            6()
        },
        test_graph! {
            0(1)
            1(2.dot)
            2(3)
            3(4)
            4(5.dot)
            5(6)
            6()
        },
    )
}

/*
0              0
|\             |
| 1            1
| |\           |
| | 2          2
| | |          :
| 3 |   ->     3
| |/           |
| 4            4
| |            :
5 |            5
|/             |
6              6
 */
#[test]
fn recursive_sections_test() {
    run_test(
        test_graph! {
            0(5, 1)
            1(3, 2)
            2(4)
            3(4)
            4(6)
            5(6)
            6()
        },
        test_graph! {
            0(1)
            1(2)
            2(3.dot)
            3(4)
            4(5.dot)
            5(6)
            6()
        },
    )
}

/*
0             0
|\            |
| 1           1
| |           :
2 |           2
|\|           |
| 3     ->    3
| |           :
4 |           4
|/            |
5             5
 */
#[test]
fn diagonal_test() {
    run_test(
        test_graph! {
            0(2, 1)
            1(3)
            2(4, 3)
            3(5)
            4(5)
            5()
        },
        test_graph! {
            0(1)
            1(2.dot)
            2(3)
            3(4.dot)
            4(5)
            5()
        },
    )
}

/*
0             0
|\            |
| 1           1
|/|    ->     :
2 |           2
\|            :
3             3
 */
#[test]
fn different_diagonal_test() {
    run_test(
        test_graph! {
            0(2, 1)
            1(2, 3)
            2(3)
            3()
        },
        test_graph! {
            0(1)
            1(2.dot)
            2(3.dot)
            3()
        },
    )
}

/*
0                0
|\               |
| 1              1
| |\             |
| 2 \            2
| |\|            :
| | 3            3
| \|             :
| 4              4
| \      ->      :
5  \             5
|\  \            |
6 \ |            6
|\| |            :
| 7 |            7
|/  |            :
8  /             8
|/               |
9                9
 */
#[test]
fn complicated_branches_test() {
    run_test(
        test_graph! {
            0(5, 1)
            1(2, 3)
            2(4, 3)
            3(4)
            4(9)
            5(6, 7)
            6(8, 7)
            7(8)
            8(9)
            9()
        },
        test_graph! {
            0(1)
            1(2)
            2(3.dot)
            3(4.dot)
            4(5.dot)
            5(6)
            6(7.dot)
            7(8.dot)
            8(9)
            9()
        },
    )
}

/*
0                0
|\               |
| 1              1
| |\             |
| | 2            2
| | 3            3
| |/|            :
| 4 |    ->      4
| 5 |            5
| | |            :
6 | |            6
|/ /             |
7 /              7
|/               :
8                8
 */
#[test]
fn interesting_test() {
    run_test(
        test_graph! {
            0(6, 1)
            1(4, 2)
            2(3)
            3(4, 8)
            4(5)
            5(7)
            6(7)
            7(8)
            8()
        },
        test_graph! {
            0(1)
            1(2)
            2(3)
            3(4.dot)
            4(5)
            5(6.dot)
            6(7)
            7(8.dot)
            8()
        },
    )
}

/*
0                0
|\               |
| 1              1
| |\             |
| | 2            2
| | |\           |
| | | 3          3
| | | |          :
| | 4 |          4
| |/  |          :
| 5   |    ->    5
| 6   |          6
| |  /           :
7 | /            7
|/ /             |
8 /              8
|/               |
9                9
 */
#[test]
fn interesting_test2() {
    run_test(
        test_graph! {
            0(7, 1)
            1(5, 2)
            2(4, 3)
            3(9)
            4(5)
            5(6)
            6(8)
            7(8)
            8(9)
            9()
        },
        test_graph! {
            0(1)
            1(2)
            2(3)
            3(4.dot)
            4(5.dot)
            5(6)
            6(7.dot)
            7(8)
            8(9)
            9()
        },
    )
}

/*
0                0
|\               |
| 1              1
| |\             |
| | 2            2
| | |            :
3 | |    ->      3
4 | |            4
|/ /             |
5 /              5
\/               |
6                6
 */
#[test]
fn small_recursive_branches_test() {
    run_test(
        test_graph! {
            0(3, 1)
            1(5, 2)
            2(6)
            3(4)
            4(5)
            5(6)
            6()
        },
        test_graph! {
            0(1)
            1(2)
            2(3.dot)
            3(4)
            4(5)
            5(6)
            6()
        },
    )
}

/*
0                0
|\               |
| 1              1
| 2              2
| |\             |\
| | 3            | 3
| | 4            | 4
| | |\           | 5
| | | \          | :
| | | 5    ->    6 :
| 6 | |          7 :
| 7 | |          :'
8 |/  |          8
|/|   |          |
9 |  /           9
|/  /            |
10 /             10
11               11
 */
#[test]
fn two_tails_branch_test() {
    run_test(
        test_graph! {
            0(8, 1)
            1(2)
            2(6, 3)
            3(4)
            4(9, 5)
            5(11)
            6(7)
            7(10)
            8(9)
            9(10)
            10(11)
            11()
        },
        test_graph! {
            0(1)
            1(2)
            2(6, 3)
            3(4)
            4(5)
            5(8.dot)
            6(7)
            7(8.dot)
            8(9)
            9(10)
            10(11)
            11()
        },
    )
}

/*
0                0
|\   1           |  1
| 2 |            2 /
| |/             |/
| 3       ->     3
| |              :
4 |              4
|/               |
5                5
 */
#[test]
fn incoming_edge_test() {
    run_test(
        test_graph! {
            0(4, 2)
            1(3)
            2(3)
            3(5)
            4(5)
            5()
        },
        test_graph! {
            0(2)
            1(3)
            2(3)
            3(4.dot)
            4(5)
            5()
        },
    )
}

/*
0                0
|\               |
| 1              1
| |\             :
2 ||             2
|/ |             |
3  |     ->      3
|\ |             :
| \|             :
| 4              4
| |              :
5 |              5
|/               |
6                6
 */
#[test]
fn hidden_incoming_edges() {
    run_test(
        test_graph! {
            0(2, 1)
            1(3, 4)
            2(3)
            3(5, 4)
            4(6)
            5(6)
            6()
        },
        test_graph! {
            0(1)
            1(2.dot)
            2(3)
            3(4.dot)
            4(5.dot)
            5(6)
            6()
        },
    )
}

/*
0                0
|\               |
| 1              1
| |\             |
| | 2            2
| | 3            3
| | 4            4
| |/             :
|/               :
5                5
6                6
7                7
 */
#[test]
fn test_merge_with_old_commit() {
    run_test(
        test_graph! {
            0(5, 1)
            1(5, 2)
            2(3)
            3(4)
            4(5)
            5(6)
            6(7)
            7()
        },
        test_graph! {
            0(1)
            1(2)
            2(3)
            3(4)
            4(5.dot)
            5(6)
            6(7)
            7()
        },
    )
}

/*
0                0
|   1            | 1
2   |            2 |
|\  |            | |
| 3 |     ->     3 |
| |/             |/
| 4              4
|/               :
5                5
*/
#[test]
fn test_heads_order1() {
    run_test(
        test_graph! {
            0(2)
            1(4)
            2(5, 3)
            3(4)
            4(5)
            5()
        },
        test_graph! {
            0(2)
            1(4)
            2(3)
            3(4)
            4(5.dot)
            5()
        },
    )
}

/*
    0               0
1   |           1   |
2   |           2   |
|\  |           |   |
| 3 |     ->    3  /
| |/            |/
| 4             4
|/              :
5               5
*/
#[test]
fn test_heads_order2() {
    run_test(
        test_graph! {
            0(4)
            1(2)
            2(5, 3)
            3(4)
            4(5)
            5()
        },
        test_graph! {
            0(4)
            1(2)
            2(3)
            3(4)
            4(5.dot)
            5()
        },
    )
}

/*
0                0
|\               |
| 1              1
| 2      ->      2
|/               :
3                3
*/
#[test]
fn test_katisha() {
    run_test(
        test_graph! {
            0(1, 3)
            1(2)
            2(3)
            3()
        },
        test_graph! {
            0(1)
            1(2)
            2(3.dot)
            3()
        },
    )
}

/*
0                0
|\               |\
| 1      ->      | 1
| 2              | 2
3                3
 */
#[test]
fn test_initial_import() {
    run_test(
        test_graph! {
            0(3, 1)
            1(2)
            2()
            3()
        },
        test_graph! {
            0(3, 1)
            1(2)
            2()
            3()
        },
    )
}

/*
0              0
|\             |
| 1    ->      1
|/             :
2              2
 */
#[test]
fn test_triangle() {
    run_test(
        test_graph! {
            0(2, 1)
            1(2)
            2()
        },
        test_graph! {
            0(1)
            1(2.dot)
            2()
        },
    )
}

/*
0
|\
1 \
2 \
|\ \
3 \|
|\ 4
| \|
| 5
|/
6
 */
#[test]
fn reversed_parents_test() {
    run_test(
        test_graph! {
            0(1, 4)
            1(2)
            2(3, 4)
            3(5, 6)
            4(5)
            5(6)
            6()
        },
        test_graph! {
            0(1)
            1(2)
            2(3)
            3(4.dot)
            4(5)
            5(6.dot)
            6()
        },
    )
}

/*
0               0
|\              |
| 1             1
| |\            |\
| | 2           | 2
| 3 |    -->    3 :
| \'            : :
|/|             :'
4 |             4
|/              |
5               5
 */
#[test]
fn crossed_tails_test() {
    run_test(
        test_graph! {
            0(4, 1)
            1(3, 2)
            2(4)
            3(5)
            4(5)
            5()
        },
        test_graph! {
            0(1)
            1(3, 2)
            2(4.dot)
            3(4.dot)
            4(5)
            5()
        },
    )
}
```

`crates/ig-log/tests/linear_bek_expand.rs`:

```rust
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/LinearBekExpandTest.kt`.

mod common;

use common::linear_bek::{assert_graph_equals, build_layout, run_linear_bek};
use common::test_graph_builder::{TestLinearGraph, test_graph};
use ig_log::api::elements::{GraphEdge, GraphEdgeType};
use ig_log::linear_bek::linear_bek_graph_builder::LinearBekGraphBuilder;

fn run_test(
    before_linear_bek: TestLinearGraph,
    after_expansion: TestLinearGraph,
    from_node_to_expand: i32,
    to_node_to_expand: i32,
) {
    let after_linear_bek = run_linear_bek(before_linear_bek);
    after_linear_bek.expand_edge(&GraphEdge::create_normal_edge(
        from_node_to_expand,
        to_node_to_expand,
        GraphEdgeType::Dotted,
    ));
    assert_graph_equals(after_expansion, &after_linear_bek);
}

/*
0              0
|\             |
| 1            1
| |\           |
| | 2          2
| | |          :
| 3 |   ->     3
| |/           |
| 4            4
| |            :
5 |            5
|/             |
6              6
 */
#[test]
fn recursive_expansion_test() {
    run_test(
        test_graph! {
            0(5, 1)
            1(3, 2)
            2(4)
            3(4)
            4(6)
            5(6)
            6()
        },
        test_graph! {
            0(5, 1)
            1(2)
            2(3.dot)
            3(4)
            4(6)
            5(6)
            6()
        },
        4,
        5,
    )
}

/*
0                0
|\               |
| 1              1
| |\             |
| | 2            2
| | |            :
3 | |    ->      3
4 | |            4
|/ /             |
5 /              5
\/               |
6                6
 */
#[test]
fn small_recursive_branches_test() {
    run_test(
        test_graph! {
            0(3, 1)
            1(5, 2)
            2(6)
            3(4)
            4(5)
            5(6)
            6()
        },
        test_graph! {
            0(3, 1)
            1(5, 2)
            2(6)
            3(4)
            4(5)
            5(6)
            6()
        },
        2,
        3,
    )
}

/*
0
|\
| 1
|  \
|  2
|  |\
|  | 3
|  | |
4  | |
|\ | |
|5 | |
|\/  |
|/\  |
6 | /
|/ /
8 /
|/
9
*/
#[test]
fn several_collapses_test() {
    let builder = || {
        test_graph! {
            0(4, 1)
            1(2)
            2(6, 3)
            3(9)
            4(6, 5)
            5(8)
            6(8)
            7(8)
            8(9)
            9()
        }
    };
    let layout = build_layout(&builder());
    let after_linear_bek = run_linear_bek(builder());

    after_linear_bek.expand_edge(&GraphEdge::create_normal_edge(5, 6, GraphEdgeType::Dotted));
    LinearBekGraphBuilder::new(&after_linear_bek, &layout).collapse_fragment(4);
    after_linear_bek.expand_edge(&GraphEdge::create_normal_edge(3, 4, GraphEdgeType::Dotted));

    assert_graph_equals(
        test_graph! {
            0(4, 1)
            1(2)
            2(6, 3)
            3(9)
            4(5)
            5(6.dot)
            6(8)
            7(8)
            8(9)
            9()
        },
        &after_linear_bek,
    );
}

/*
0
|\
| 1
| |\
|/ 2
3 /
|/
4
*/
#[test]
fn replace_dotted_edge_test() {
    let builder = || {
        test_graph! {
            0(3, 1)
            1(3, 2)
            2(4)
            3(4)
            4()
        }
    };
    let after_linear_bek = run_linear_bek(builder());

    after_linear_bek.expand_edge(&GraphEdge::create_normal_edge(2, 3, GraphEdgeType::Dotted));

    assert_graph_equals(builder(), &after_linear_bek);
}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -q --locked -p ig-log --test linear_bek --test linear_bek_expand
```

Expected: FAIL — `ig_log::linear_bek` doesn't exist yet, so the test crates fail to compile. The simulation's first error:

```text
error[E0433]: cannot find `linear_bek` in `ig_log`
```

- [ ] **Step 3: Copy the code**

```bash
git checkout 6ac6c8a -- crates/ig-log/src/lib.rs crates/ig-log/src/linear_bek/linear_bek_controller.rs crates/ig-log/src/linear_bek/linear_bek_graph.rs crates/ig-log/src/linear_bek/linear_bek_graph_builder.rs crates/ig-log/src/linear_bek/mod.rs crates/ig-log/src/utils/java_priority_queue.rs crates/ig-log/src/utils/mod.rs
```

`crates/ig-log/src/lib.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/lib.rs b/crates/ig-log/src/lib.rs
index 9334a64..f327f3b 100644
--- a/crates/ig-log/src/lib.rs
+++ b/crates/ig-log/src/lib.rs
@@ -55,6 +55,7 @@ pub mod data;
 pub mod facade;
 pub mod graph_color_manager;
 pub mod graph_commit;
+pub mod linear_bek;
 pub mod permanent;
 pub mod permanent_graph;
 pub mod print;
```

`crates/ig-log/src/linear_bek/linear_bek_controller.rs`:

```rust
//! The linear BEK controller: collapses all merges on creation; a click on a merge's commits
//! collapses it again, a click on a dotted edge expands it, and hovering highlights them.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/linearBek/LinearBekController.kt`.
//!
//! Deviations:
//! - The graph builder is created per use (upstream keeps one that references the compiled graph).
//! - `collectChildFragments`' work list is ordered by insertion with tails and body added in
//!   ascending order (upstream: a `LinkedHashSet` fed from a fastutil hash set). The fragments are
//!   a list (upstream: a set of objects without `equals`, i.e. also a list).
//! - The creation time is not logged.
//!
//! Upstream quirk kept: `iterateMerges`' search limit never stops it (see
//! [`BfsSearch`]), so it walks up to the first merge that
//! answers.

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::sync::Arc;

use crate::actions::{GraphActionType, GraphUpdater};
use crate::api::EdgeFilter;
use crate::api::GraphLayout;
use crate::api::elements::{GraphEdge, GraphEdgeType, GraphElement, GraphNode};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::facade::cascade_controller::{self, CascadeController};
use crate::facade::graph_changes::{GraphChanges, edges_replaced};
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};
use crate::facade::sort::sort_index_map::SortIndexMap;
use crate::facade::sort::sorted_base_controller::SortedBaseController;
use crate::linear_bek::linear_bek_graph::{LinearBekGraph, WorkingLinearBekGraph};
use crate::linear_bek::linear_bek_graph_builder::{
    LinearBekGraphBuilder, MAX_BLOCK_SIZE, MergeFragment,
};
use crate::utils::bfs_util::{BfsSearch, BfsWalk, ReversePriorityQueue};
use crate::utils::int_hash_set_flags::IntHashSetFlags;
use crate::utils::linear_graph_utils::{
    as_lite_linear_graph, create_selected_answer, get_down_nodes,
};

const MAX_COLLAPSED_MERGES: usize = 10;

pub struct LinearBekController<CommitId> {
    delegate_controller: SortedBaseController,
    compiled_graph: Arc<LinearBekGraph>,
    bek_graph_layout: BekGraphLayout<CommitId>,
    merge_commits: HashSet<i32>,
}

impl<CommitId: 'static> LinearBekController<CommitId> {
    pub fn new(
        controller: SortedBaseController,
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    ) -> LinearBekController<CommitId> {
        let compiled_graph = Arc::new(LinearBekGraph::new(controller.compiled_graph()));
        let bek_graph_layout =
            BekGraphLayout::new(permanent_graph_info, controller.bek_int_map().clone());
        let merge_commits =
            LinearBekGraphBuilder::new(&compiled_graph, &bek_graph_layout).collapse_all();
        LinearBekController {
            delegate_controller: controller,
            compiled_graph,
            bek_graph_layout,
            merge_commits,
        }
    }

    fn delegate_graph(&self) -> Arc<dyn LinearGraph> {
        self.delegate_controller.compiled_graph()
    }

    fn linear_bek_graph_builder(&self) -> LinearBekGraphBuilder<'_> {
        LinearBekGraphBuilder::new(&self.compiled_graph, &self.bek_graph_layout)
    }

    fn get_all_adjacent_dotted_edges(&self, graph_element: &GraphNode) -> Vec<GraphEdge> {
        self.compiled_graph
            .get_adjacent_edges(graph_element.node_index, EdgeFilter::All)
            .into_iter()
            .filter(|graph_edge| graph_edge.edge_type == GraphEdgeType::Dotted)
            .collect()
    }

    fn expand_all(&self) -> LinearGraphAnswer {
        let compiled_graph = self.compiled_graph.clone();
        LinearGraphAnswer::with_changes(
            Some(GraphChanges::some_changes()),
            Some(GraphUpdater::new(move || {
                compiled_graph.dotted_edges().remove_all();
                compiled_graph.hidden_edges().remove_all();
            })),
        )
    }

    fn collapse_all(&self) -> LinearGraphAnswer {
        let working_graph = WorkingLinearBekGraph::new(self.compiled_graph.clone());
        LinearBekGraphBuilder::new(&working_graph.working, &self.bek_graph_layout).collapse_all();
        let removed: Vec<GraphEdge> = working_graph.get_removed_edges().into_iter().collect();
        let added: Vec<GraphEdge> = working_graph.get_added_edges().into_iter().collect();
        LinearGraphAnswer::with_changes(
            Some(edges_replaced(&removed, &added, &*self.delegate_graph())),
            Some(GraphUpdater::new(move || working_graph.apply_changes())),
        )
    }

    fn iterate_merges<T>(
        &self,
        start: i32,
        mut function: impl FnMut(i32) -> Option<T>,
    ) -> Option<T> {
        let delegate_graph = self.delegate_graph();
        let lite = as_lite_linear_graph(delegate_graph.clone());
        let walk = BfsWalk::with_queue(start, &lite, false, ReversePriorityQueue::new(start));
        let mut search = BfsSearch::new(walk, MAX_BLOCK_SIZE);
        let mut visited = IntHashSetFlags::new(delegate_graph.nodes_count());
        search.find(&mut visited, |it| {
            if self.merge_commits.contains(&it) {
                function(it)
            } else {
                None
            }
        })
    }

    fn is_collapsed(&self, merge: i32) -> bool {
        get_down_nodes(&*self.compiled_graph, merge).len() == 1
    }

    fn highlight_node(&self, node: &GraphNode) -> Option<LinearGraphAnswer> {
        self.iterate_merges(node.node_index, |merge| {
            if !self.is_collapsed(merge) {
                let fragments = self.collect_child_fragments(merge);
                if fragments.is_empty() {
                    return None;
                }

                let nodes_to_highlight: HashSet<i32> =
                    fragments.iter().flat_map(|it| it.get_all_nodes()).collect();
                if nodes_to_highlight.contains(&node.node_index) {
                    return Some(create_selected_answer(
                        &*self.compiled_graph,
                        nodes_to_highlight,
                    ));
                }
            }
            None
        })
    }

    fn collapse_node(&self, node: &GraphNode) -> Option<LinearGraphAnswer> {
        self.iterate_merges(node.node_index, |merge| {
            if !self.is_collapsed(merge) {
                let fragments = self.collect_child_fragments(merge);
                if fragments.is_empty() {
                    return None;
                }

                let nodes_to_collapse: BTreeSet<i32> = fragments
                    .iter()
                    .flat_map(|f| {
                        let mut nodes = f.get_tails_and_body();
                        nodes.insert(f.get_parent());
                        nodes
                    })
                    .collect();
                if !nodes_to_collapse.contains(&node.node_index)
                    && !fragments
                        .iter()
                        .any(|it| it.get_left_child() == node.node_index)
                {
                    return None;
                }
                // reverse order (upstream: TreeSet(Comparator.reverseOrder()))
                for &i in nodes_to_collapse.iter().rev() {
                    self.linear_bek_graph_builder().collapse_fragment(i);
                }
                return Some(LinearGraphAnswer::with_changes(
                    Some(GraphChanges::some_changes()),
                    None,
                ));
            }
            None
        })
    }

    fn collect_child_fragments(&self, merge: i32) -> Vec<MergeFragment> {
        let builder = self.linear_bek_graph_builder();
        let mut result = Vec::new();
        // a LinkedHashSet
        let mut to_process = VecDeque::from([merge]);
        let mut in_to_process = HashSet::from([merge]);
        while let Some(next_node) = to_process.pop_front() {
            in_to_process.remove(&next_node);
            let Some(fragment) = builder.get_fragment(next_node) else {
                continue;
            };
            let tails_and_body: BTreeSet<i32> = fragment.get_tails_and_body().into_iter().collect();
            for node in tails_and_body {
                if in_to_process.insert(node) {
                    to_process.push_back(node);
                }
            }
            result.push(fragment);
            if result.len() > MAX_COLLAPSED_MERGES {
                break;
            }
        }
        result
    }

    fn highlight_edge(&self, edge: &GraphEdge) -> Option<LinearGraphAnswer> {
        if edge.edge_type != GraphEdgeType::Dotted {
            return None;
        }
        Some(create_selected_answer(
            &*self.compiled_graph,
            edge.up_node_index.into_iter().chain(edge.down_node_index),
        ))
    }

    fn expand_edge(&self, edge: &GraphEdge) -> Option<LinearGraphAnswer> {
        if edge.edge_type != GraphEdgeType::Dotted {
            return None;
        }
        let expanded: Vec<GraphEdge> = self.compiled_graph.expand_edge(edge).into_iter().collect();
        Some(LinearGraphAnswer::with_changes(
            Some(edges_replaced(&[*edge], &expanded, &*self.delegate_graph())),
            None,
        ))
    }
}

impl<CommitId: 'static> LinearGraphController for LinearBekController<CommitId> {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.compiled_graph.clone()
    }

    fn perform_linear_graph_action(&mut self, action: &LinearGraphAction) -> LinearGraphAnswer {
        cascade_controller::perform_linear_graph_action(self, action)
    }

    fn delegate(&self) -> Option<&dyn LinearGraphController> {
        Some(&self.delegate_controller)
    }

    fn delegate_mut(&mut self) -> Option<&mut dyn LinearGraphController> {
        Some(&mut self.delegate_controller)
    }
}

impl<CommitId: 'static> CascadeController for LinearBekController<CommitId> {
    fn perform_action(&mut self, action: &LinearGraphAction) -> Option<LinearGraphAnswer> {
        if action.action_type == GraphActionType::ButtonCollapse {
            return Some(self.collapse_all());
        }
        if action.action_type == GraphActionType::ButtonExpand {
            return Some(self.expand_all());
        }
        let affected_element = action.affected_element.as_ref()?;

        if action.action_type == GraphActionType::MouseClick {
            match affected_element.graph_element() {
                GraphElement::Node(graph_element) => {
                    if let Some(answer) = self.collapse_node(&graph_element) {
                        return Some(answer);
                    }
                    for dotted_edge in self.get_all_adjacent_dotted_edges(&graph_element) {
                        if let Some(expanded_answer) = self.expand_edge(&dotted_edge) {
                            return Some(expanded_answer);
                        }
                    }
                }
                GraphElement::Edge(graph_element) => return self.expand_edge(&graph_element),
            }
        } else if action.action_type == GraphActionType::MouseOver {
            match affected_element.graph_element() {
                GraphElement::Node(graph_element) => {
                    if let Some(answer) = self.highlight_node(&graph_element) {
                        return Some(answer);
                    }
                    for dotted_edge in self.get_all_adjacent_dotted_edges(&graph_element) {
                        if let Some(highlight_answer) = self.highlight_edge(&dotted_edge) {
                            return Some(highlight_answer);
                        }
                    }
                }
                GraphElement::Edge(graph_element) => return self.highlight_edge(&graph_element),
            }
        }
        None
    }
}

/// The permanent layout seen through the BEK order.
struct BekGraphLayout<CommitId> {
    permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    sort_index_map: Arc<SortIndexMap>,
    /// Computed once: `get_head_node_index` returns a slice.
    head_node_index: Vec<i32>,
}

impl<CommitId> BekGraphLayout<CommitId> {
    fn new(
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
        sort_index_map: Arc<SortIndexMap>,
    ) -> BekGraphLayout<CommitId> {
        let head_node_index = permanent_graph_info
            .permanent_graph_layout()
            .get_head_node_index()
            .iter()
            .map(|&it| sort_index_map.get_sorted_index(it))
            .collect();
        BekGraphLayout {
            permanent_graph_info,
            sort_index_map,
            head_node_index,
        }
    }
}

impl<CommitId> GraphLayout for BekGraphLayout<CommitId> {
    fn get_layout_index(&self, node_index: i32) -> i32 {
        self.permanent_graph_info
            .permanent_graph_layout()
            .get_layout_index(self.sort_index_map.get_usual_index(node_index))
    }

    fn get_one_of_head_node_index(&self, node_index: i32) -> i32 {
        let usual_index = self
            .permanent_graph_info
            .permanent_graph_layout()
            .get_one_of_head_node_index(self.sort_index_map.get_usual_index(node_index));
        self.sort_index_map.get_sorted_index(usual_index)
    }

    fn get_head_node_index(&self) -> &[i32] {
        &self.head_node_index
    }
}
```

`crates/ig-log/src/linear_bek/linear_bek_graph.rs`:

```rust
//! The linear BEK graph: the BEK-sorted graph with some edges hidden and dotted edges added.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/linearBek/LinearBekGraph.java`.
//!
//! Deviations:
//! - `LinearBekGraph extends LinearGraphWrapper` becomes a struct embedding a
//!   [`LinearGraphWrapper`]; `WorkingLinearBekGraph` embeds a `LinearBekGraph`.
//! - Edge sets are `BTreeSet`s (deterministic order; upstream `HashSet`).

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphEdgeType, GraphNode};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::VcsLogGraphNodeId;
use crate::collapsing::edge_storage_wrapper::EdgeStorageWrapper;
use crate::facade::linear_graph_wrapper::LinearGraphWrapper;
use crate::visible_graph::VcsLogVisibleGraphIndex;

pub struct LinearBekGraph {
    wrapper: LinearGraphWrapper,
}

impl LinearBekGraph {
    pub fn new(graph: Arc<dyn LinearGraph>) -> LinearBekGraph {
        LinearBekGraph {
            wrapper: LinearGraphWrapper::new(graph),
        }
    }

    pub fn graph(&self) -> &Arc<dyn LinearGraph> {
        &self.wrapper.graph
    }

    pub fn hidden_edges(&self) -> &EdgeStorageWrapper {
        &self.wrapper.hidden_edges
    }

    pub fn dotted_edges(&self) -> &EdgeStorageWrapper {
        &self.wrapper.dotted_edges
    }

    /// Replaces the dotted `edge` by the edges it stands for (recursively); returns them.
    ///
    /// # Panics
    /// If `edge` is not a normal edge (upstream: an assertion).
    pub fn expand_edge(&self, edge: &GraphEdge) -> BTreeSet<GraphEdge> {
        let mut result = BTreeSet::new();

        debug_assert_eq!(edge.edge_type, GraphEdgeType::Dotted);
        self.dotted_edges().remove_edge(edge);

        let tail = edge
            .up_node_index
            .expect("Collapsed from to an unloaded node");
        let first_child = edge
            .down_node_index
            .expect("Collapsed edge to an unloaded node");

        let down_dotted_edges = self
            .hidden_edges()
            .get_adjacent_edges(tail, EdgeFilter::NormalDown);
        let up_dotted_edges = self
            .hidden_edges()
            .get_adjacent_edges(first_child, EdgeFilter::NormalUp);
        for e in down_dotted_edges.iter().chain(up_dotted_edges.iter()) {
            self.hidden_edges().remove_edge(e);
            if e.edge_type == GraphEdgeType::Dotted {
                result.extend(self.expand_edge(e));
            } else {
                result.insert(*e);
            }
        }

        result
    }
}

impl LinearGraph for LinearBekGraph {
    fn nodes_count(&self) -> i32 {
        self.wrapper.nodes_count()
    }

    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        self.wrapper.get_adjacent_edges(node_index, filter)
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        self.wrapper.get_graph_node(node_index)
    }

    fn get_node_id(&self, node_index: VcsLogVisibleGraphIndex) -> VcsLogGraphNodeId {
        self.wrapper.get_node_id(node_index)
    }

    fn get_node_index(&self, node_id: VcsLogGraphNodeId) -> Option<VcsLogVisibleGraphIndex> {
        self.wrapper.get_node_index(node_id)
    }
}

/// A scratch copy of a [`LinearBekGraph`] (same base graph, fresh edges) whose differences can be
/// reported and applied to the original.
pub struct WorkingLinearBekGraph {
    pub working: LinearBekGraph,
    linear_graph: Arc<LinearBekGraph>,
}

fn dotted(edges: &BTreeSet<GraphEdge>) -> impl Iterator<Item = &GraphEdge> {
    edges
        .iter()
        .filter(|edge| edge.edge_type == GraphEdgeType::Dotted)
}

impl WorkingLinearBekGraph {
    pub fn new(graph: Arc<LinearBekGraph>) -> WorkingLinearBekGraph {
        WorkingLinearBekGraph {
            working: LinearBekGraph::new(graph.graph().clone()),
            linear_graph: graph,
        }
    }

    pub fn get_added_edges(&self) -> BTreeSet<GraphEdge> {
        let mut result = self.working.dotted_edges().get_edges();
        let hidden = self.working.hidden_edges().get_edges();
        for edge in dotted(&hidden) {
            result.remove(edge);
        }
        for edge in &self.linear_graph.dotted_edges().get_edges() {
            result.remove(edge);
        }
        result
    }

    pub fn get_removed_edges(&self) -> BTreeSet<GraphEdge> {
        let mut result = BTreeSet::new();
        let hidden = self.working.hidden_edges().get_edges();
        result.extend(
            hidden
                .iter()
                .filter(|edge| edge.edge_type != GraphEdgeType::Dotted),
        );
        let original_dotted = self.linear_graph.dotted_edges().get_edges();
        result.extend(hidden.intersection(&original_dotted));
        for edge in &self.linear_graph.hidden_edges().get_edges() {
            result.remove(edge);
        }
        result
    }

    pub fn apply_changes(&self) {
        self.linear_graph.dotted_edges().remove_all();
        self.linear_graph.hidden_edges().remove_all();

        for e in self.working.dotted_edges().get_edges() {
            self.linear_graph.dotted_edges().create_edge(&e);
        }
        for e in self.working.hidden_edges().get_edges() {
            self.linear_graph.hidden_edges().create_edge(&e);
        }
    }
}
```

`crates/ig-log/src/linear_bek/linear_bek_graph_builder.rs`:

```rust
//! Finds the merge fragments of the BEK graph and collapses them into dotted edges.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/linearBek/LinearBekGraphBuilder.java`.
//!
//! Deviations:
//! - The edge and magic-set queues are [`JavaPriorityQueue`]s, so edges that tie on their down
//!   node are polled in upstream's order.
//! - `GraphEdgeComparator` orders two edges without a down node by a deterministic hash (upstream:
//!   `hashCode` differences, where the edge type's hash is an identity hash). Such edges never
//!   reach the queue, which only holds normal edges.
//! - `MergeFragment` collapses its tail edges and tails in ascending order (upstream: fastutil hash
//!   order); the resulting edges do not depend on it.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashSet};

use crate::api::EdgeFilter;
use crate::api::GraphLayout;
use crate::api::elements::{GraphEdge, GraphEdgeType};
use crate::api::linear_graph::LinearGraph;
use crate::linear_bek::linear_bek_graph::LinearBekGraph;
use crate::print::print_element_generator_impl::LONG_EDGE_SIZE;
use crate::utils::int_int_multi_map::IntIntMultiMap;
use crate::utils::java_priority_queue::JavaPriorityQueue;
use crate::utils::linear_graph_utils::{get_down_nodes, get_edge};

pub const MAX_BLOCK_SIZE: i32 = 200;
const MAGIC_SET_SIZE: i32 = LONG_EDGE_SIZE;

pub struct LinearBekGraphBuilder<'a> {
    graph_layout: &'a dyn GraphLayout,
    linear_bek_graph: &'a LinearBekGraph,
}

impl<'a> LinearBekGraphBuilder<'a> {
    pub fn new(
        bek_graph: &'a LinearBekGraph,
        graph_layout: &'a dyn GraphLayout,
    ) -> LinearBekGraphBuilder<'a> {
        LinearBekGraphBuilder {
            graph_layout,
            linear_bek_graph: bek_graph,
        }
    }

    /// Collapses every merge fragment, bottom up; returns the merges that were collapsed.
    pub fn collapse_all(&self) -> HashSet<i32> {
        let mut collapsed_merges = HashSet::new();
        for i in (0..self.linear_bek_graph.graph().nodes_count()).rev() {
            if let Some(fragment) = self.get_fragment(i) {
                fragment.collapse(self.linear_bek_graph);
                collapsed_merges.insert(fragment.get_parent());
            }
        }
        collapsed_merges
    }

    pub fn collapse_fragment(&self, merge_commit: i32) -> Option<MergeFragment> {
        let fragment = self.get_fragment(merge_commit)?;
        fragment.collapse(self.linear_bek_graph);
        Some(fragment)
    }

    pub fn get_fragment(&self, merge_commit: i32) -> Option<MergeFragment> {
        let mut down_nodes = get_down_nodes(self.linear_bek_graph, merge_commit);
        down_nodes.sort_unstable();
        if down_nodes.len() != 2 {
            return None;
        }

        self.get_fragment_of(down_nodes[1], down_nodes[0], merge_commit)
    }

    fn get_fragment_of(
        &self,
        left_child: i32,
        right_child: i32,
        parent: i32,
    ) -> Option<MergeFragment> {
        let mut fragment = MergeFragment::new(parent, left_child, right_child);

        let left_li = self.graph_layout.get_layout_index(left_child);
        let right_li = self.graph_layout.get_layout_index(right_child);
        let mut rows_count = 1;
        let mut block_size = 1;

        let mut queue = JavaPriorityQueue::new(graph_edge_comparator);
        queue.add_all(
            self.linear_bek_graph
                .get_adjacent_edges(right_child, EdgeFilter::NormalDown),
        );

        let mut magic_set: Option<HashSet<i32>> = None;

        while let Some(next_edge) = queue.poll() {
            let next = next_edge.down_node_index;
            // can not happen
            let up_node_index = next_edge.up_node_index.expect("edge without an up node");
            let Some(next) = next else {
                fragment.add_tail(up_node_index);
                continue; // allow very long edges down
            };

            if next == left_child {
                // found first child
                fragment.add_tail(up_node_index);
                fragment.set_merge_with_old_commit(true);
            } else if next == right_child + rows_count {
                // all is fine, continuing
                rows_count += 1;
                block_size += 1;
                queue.add_all(
                    self.linear_bek_graph
                        .get_adjacent_edges(next, EdgeFilter::NormalDown),
                );
                fragment.add_body(up_node_index);
            } else if next > right_child + rows_count && next < left_child {
                rows_count = next - right_child + 1;
                block_size += 1;
                queue.add_all(
                    self.linear_bek_graph
                        .get_adjacent_edges(next, EdgeFilter::NormalDown),
                );
                fragment.add_body(up_node_index);
            } else if next > left_child {
                let li = self.graph_layout.get_layout_index(next);
                if left_li > right_li && !fragment.is_merge_with_old_commit() {
                    if next > left_child + MAGIC_SET_SIZE {
                        return None;
                    }
                    let magic_set =
                        magic_set.get_or_insert_with(|| self.calculate_magic_set(left_child));

                    if magic_set.contains(&next) {
                        fragment.add_tail_edge(up_node_index, next);
                    } else {
                        return None;
                    }
                } else if (li > left_li && li < right_li) || li == left_li {
                    fragment.add_tail_edge(up_node_index, next);
                } else if li >= right_li {
                    return None;
                } else if next > left_child + MAGIC_SET_SIZE {
                    if !fragment.has_tail_edge(up_node_index) && !fragment.is_body(up_node_index) {
                        return None;
                    }
                } else {
                    let magic_set =
                        magic_set.get_or_insert_with(|| self.calculate_magic_set(left_child));

                    if magic_set.contains(&next) {
                        fragment.add_tail_edge(up_node_index, next);
                    } else {
                        return None;
                    }
                }
            }

            if block_size >= MAX_BLOCK_SIZE {
                return None;
            }
        }

        if fragment.get_tails().is_empty() {
            return None; // this can happen if we ran into initial import
        }

        Some(fragment)
    }

    fn calculate_magic_set(&self, node: i32) -> HashSet<i32> {
        let mut magic_set = HashSet::with_capacity(MAGIC_SET_SIZE as usize);

        let down_nodes_of = |n: i32| {
            self.linear_bek_graph
                .get_adjacent_edges(n, EdgeFilter::NormalDown)
                .into_iter()
                .map(graph_edge_to_down_node)
        };
        let mut magic_queue = JavaPriorityQueue::new(|a: &Option<i32>, b: &Option<i32>| a.cmp(b));
        magic_queue.add_all(down_nodes_of(node));
        while let Some(i) = magic_queue.poll() {
            // a normal down edge always has a down node
            let i = i.expect("normal edge without a down node");
            if i > node + MAGIC_SET_SIZE {
                break;
            }
            magic_set.insert(i);
            magic_queue.add_all(down_nodes_of(i));
        }
        magic_set
    }
}

/// A merge with its two children and the nodes in between (the "body" of the merged branch and
/// its "tails", the nodes from which it reaches the left child or below).
#[derive(Clone, Debug)]
pub struct MergeFragment {
    parent: i32,
    left_child: i32,
    right_child: i32,

    merge_with_old_commit: bool,
    tail_edges: IntIntMultiMap,
    block_body: HashSet<i32>,
    tails: HashSet<i32>,
}

impl MergeFragment {
    fn new(parent: i32, left_child: i32, right_child: i32) -> MergeFragment {
        MergeFragment {
            parent,
            left_child,
            right_child,
            merge_with_old_commit: false,
            tail_edges: IntIntMultiMap::new(),
            block_body: HashSet::new(),
            tails: HashSet::new(),
        }
    }

    pub fn is_merge_with_old_commit(&self) -> bool {
        self.merge_with_old_commit
    }

    pub fn set_merge_with_old_commit(&mut self, merge_with_old_commit: bool) {
        self.merge_with_old_commit = merge_with_old_commit;
    }

    pub fn add_tail(&mut self, tail: i32) {
        if !self.block_body.contains(&tail) {
            self.tails.insert(tail);
        }
    }

    pub fn add_tail_edge(&mut self, up_node_index: i32, down_node_index: i32) {
        if !self.block_body.contains(&up_node_index) {
            self.tails.insert(up_node_index);
            self.tail_edges.put_value(up_node_index, down_node_index);
        }
    }

    pub fn add_body(&mut self, body: i32) {
        self.block_body.insert(body);
    }

    pub fn get_tails(&self) -> &HashSet<i32> {
        &self.tails
    }

    pub fn get_tails_and_body(&self) -> HashSet<i32> {
        self.block_body.union(&self.tails).copied().collect()
    }

    pub fn get_all_nodes(&self) -> HashSet<i32> {
        let mut nodes = HashSet::new();
        nodes.insert(self.parent);
        nodes.insert(self.left_child);
        nodes.insert(self.right_child);
        nodes.extend(self.get_tails_and_body());
        nodes
    }

    pub fn collapse(&self, graph: &LinearBekGraph) {
        let up_node_indexes: BTreeSet<i32> = self.tail_edges.keys().into_iter().collect();
        for up_node_index in up_node_indexes {
            for &down_node_index in self.tail_edges.get(up_node_index) {
                remove_edge(graph, up_node_index, down_node_index);
            }
        }

        let tails: BTreeSet<i32> = self.tails.iter().copied().collect();
        for tail in tails {
            if !get_down_nodes(graph, tail).contains(&self.left_child) {
                add_edge(graph, tail, self.left_child);
            } else {
                replace_edge(graph, tail, self.left_child);
            }
        }
        remove_edge(graph, self.parent, self.left_child);
    }

    pub fn get_parent(&self) -> i32 {
        self.parent
    }

    pub fn has_tail_edge(&self, index: i32) -> bool {
        !self.tail_edges.get(index).is_empty()
    }

    pub fn is_body(&self, index: i32) -> bool {
        self.block_body.contains(&index)
    }

    pub fn get_left_child(&self) -> i32 {
        self.left_child
    }
}

fn dotted_edge(up: i32, down: i32) -> GraphEdge {
    GraphEdge::new(Some(up), Some(down), None, GraphEdgeType::Dotted)
}

fn add_edge(graph: &LinearBekGraph, up: i32, down: i32) {
    graph.dotted_edges().create_edge(&dotted_edge(up, down));
}

/// # Panics
/// If there is no edge between `up` and `down` (upstream: an assertion, then a
/// `NullPointerException`); the fragment search only collects existing edges.
fn remove_edge(graph: &LinearBekGraph, up: i32, down: i32) {
    if graph.dotted_edges().has_edge(up, down) {
        graph.dotted_edges().remove_edge(&dotted_edge(up, down));
        graph.hidden_edges().create_edge(&dotted_edge(up, down));
    } else {
        let edge = get_edge(&**graph.graph(), up, down)
            .unwrap_or_else(|| panic!("No edge between {up} and {down}"));
        graph.hidden_edges().create_edge(&edge);
    }
}

/// # Panics
/// See [`remove_edge`].
fn replace_edge(graph: &LinearBekGraph, up: i32, down: i32) {
    if !graph.dotted_edges().has_edge(up, down) {
        let edge = get_edge(&**graph.graph(), up, down)
            .unwrap_or_else(|| panic!("No edge between {up} and {down}"));
        graph.hidden_edges().create_edge(&edge);
        graph.dotted_edges().create_edge(&dotted_edge(up, down));
    }
}

/// `GraphEdgeComparator`: by down node, edges without a down node last.
fn graph_edge_comparator(e1: &GraphEdge, e2: &GraphEdge) -> Ordering {
    match (e1.down_node_index, e2.down_node_index) {
        (None, None) => java_hash_code(e1).cmp(&java_hash_code(e2)),
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(d1), Some(d2)) => d1.cmp(&d2),
    }
}

/// `GraphEdge.hashCode`, with the type's ordinal for its (identity) hash.
fn java_hash_code(edge: &GraphEdge) -> i32 {
    let mut result = edge.up_node_index.unwrap_or(0);
    result = result
        .wrapping_mul(31)
        .wrapping_add(edge.down_node_index.unwrap_or(0));
    result = result
        .wrapping_mul(31)
        .wrapping_add(edge.target_id.unwrap_or(0));
    result
        .wrapping_mul(31)
        .wrapping_add(edge.edge_type.ordinal())
}

/// `GraphEdgeToDownNode`.
fn graph_edge_to_down_node(graph_edge: GraphEdge) -> Option<i32> {
    graph_edge.down_node_index
}
```

`crates/ig-log/src/linear_bek/mod.rs`:

```rust
//! Linear BEK ("IntelliJ style"): incoming commits are shown on top of the main branch as if they
//! were rebased, with the merges collapsed into dotted edges.
//!
//! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/linearBek/`; see each submodule.

pub mod linear_bek_controller;
pub mod linear_bek_graph;
pub mod linear_bek_graph_builder;
```

`crates/ig-log/src/utils/java_priority_queue.rs`:

```rust
//! `java.util.PriorityQueue` with a comparator: a binary min-heap with Java's exact sift order, so
//! elements that compare equal are polled in the same order as upstream.
//!
//! IdeaGit helper (no upstream file): linear BEK polls graph edges that tie on their down node,
//! and which one comes first changes the result.

use std::cmp::Ordering;

pub struct JavaPriorityQueue<T, C> {
    queue: Vec<T>,
    comparator: C,
}

impl<T: Clone, C: Fn(&T, &T) -> Ordering> JavaPriorityQueue<T, C> {
    pub fn new(comparator: C) -> JavaPriorityQueue<T, C> {
        JavaPriorityQueue {
            queue: Vec::new(),
            comparator,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// `offer`: appends and sifts up.
    pub fn add(&mut self, e: T) {
        let k = self.queue.len();
        self.queue.push(e.clone());
        self.sift_up(k, e);
    }

    /// `AbstractQueue.addAll`: adds the elements one by one.
    pub fn add_all(&mut self, elements: impl IntoIterator<Item = T>) {
        for e in elements {
            self.add(e);
        }
    }

    /// `poll`: removes the head, moves the last element to the top and sifts it down.
    pub fn poll(&mut self) -> Option<T> {
        if self.queue.is_empty() {
            return None;
        }
        let x = self.queue.pop().expect("not empty");
        if self.queue.is_empty() {
            return Some(x);
        }
        let result = self.queue[0].clone();
        self.sift_down(0, x);
        Some(result)
    }

    fn sift_up(&mut self, mut k: usize, x: T) {
        while k > 0 {
            let parent = (k - 1) >> 1;
            if (self.comparator)(&x, &self.queue[parent]) != Ordering::Less {
                break;
            }
            self.queue[k] = self.queue[parent].clone();
            k = parent;
        }
        self.queue[k] = x;
    }

    fn sift_down(&mut self, mut k: usize, x: T) {
        let n = self.queue.len();
        let half = n >> 1;
        while k < half {
            let mut child = 2 * k + 1;
            let right = child + 1;
            if right < n
                && (self.comparator)(&self.queue[child], &self.queue[right]) == Ordering::Greater
            {
                child = right;
            }
            if (self.comparator)(&x, &self.queue[child]) != Ordering::Greater {
                break;
            }
            self.queue[k] = self.queue[child].clone();
            k = child;
        }
        self.queue[k] = x;
    }
}
```

`crates/ig-log/src/utils/mod.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/utils/mod.rs b/crates/ig-log/src/utils/mod.rs
index 7e383fb..5a253e1 100644
--- a/crates/ig-log/src/utils/mod.rs
+++ b/crates/ig-log/src/utils/mod.rs
@@ -12,6 +12,7 @@ pub mod int_hash_set_flags;
 pub mod int_int_multi_map;
 pub mod int_list;
 pub mod int_to_int_map;
+pub mod java_priority_queue;
 pub mod java_sort;
 pub mod linear_graph_utils;
 pub mod list_int_to_int_map;
```

- [ ] **Step 4: Run the new tests and watch them pass**

```bash
cargo test -q --locked -p ig-log --test linear_bek --test linear_bek_expand
```

Expected: PASS.

- [ ] **Step 5: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code 6ac6c8a -- crates/ig-log/tests/common/linear_bek.rs crates/ig-log/tests/linear_bek.rs crates/ig-log/tests/linear_bek_expand.rs crates/ig-log/src/lib.rs crates/ig-log/src/linear_bek/linear_bek_controller.rs crates/ig-log/src/linear_bek/linear_bek_graph.rs crates/ig-log/src/linear_bek/linear_bek_graph_builder.rs crates/ig-log/src/linear_bek/mod.rs crates/ig-log/src/utils/java_priority_queue.rs crates/ig-log/src/utils/mod.rs
```

Expected: no output.

- [ ] **Step 6: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 645 passed, 2 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m 'feat(ig-log): linear BEK (LinearBekGraph, LinearBekGraphBuilder, LinearBekController)'
```

---

### Task 6: Every visible graph option: PermanentGraphImpl's controller chain; create_visible_graph is infallible

**Files:**
- Create: `crates/ig-log/tests/graph_options.rs`, `crates/ig-log/src/facade/simple_graph_info.rs`
- Modify: `crates/ig-git/tests/log_perf.rs`, `crates/ig-git/tests/log_refresh.rs`, `crates/ig-log/tests/permanent_graph.rs`, `crates/ig-log/tests/permanent_graph_behaviour.rs`, `crates/ig-log/tests/print_element_generator_extra.rs`, `crates/ig-app/src/log/worker.rs`, `crates/ig-log/src/facade/mod.rs`, `crates/ig-log/src/facade/permanent_graph_impl.rs`, `crates/ig-log/src/facade/visible_graph_impl.rs`, `crates/ig-log/src/lib.rs`, `crates/ig-log/src/linear_bek/linear_bek_controller.rs`, `crates/ig-log/src/permanent_graph.rs`

**Source commit:** `1ec0a3d` on `wip/graph-extras-prototype`.

**Review focus:** `create_visible_graph` against `PermanentGraphImpl.createFilteredController` row by row, the `OnceLock` BEK map, the empty-repository fix (`getReachableNodes(null)`), and the removed `UnsupportedGraphOptions` in every caller.

Wires everything into `PermanentGraphImpl`. Unfiltered `Normal` goes back to upstream's `CollapsedController`, which brings fragment hover and collapse/expand. The app and `ig-git` callers lose their `Result` handling.

- [ ] **Step 1: Copy the tests first**

```bash
git checkout 1ec0a3d -- crates/ig-git/tests/log_perf.rs crates/ig-git/tests/log_refresh.rs crates/ig-log/tests/graph_options.rs crates/ig-log/tests/permanent_graph.rs crates/ig-log/tests/permanent_graph_behaviour.rs crates/ig-log/tests/print_element_generator_extra.rs
```

`crates/ig-git/tests/log_perf.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-git/tests/log_perf.rs b/crates/ig-git/tests/log_perf.rs
index 6238c4b..8a835e8 100644
--- a/crates/ig-git/tests/log_perf.rs
+++ b/crates/ig-git/tests/log_perf.rs
@@ -3,6 +3,10 @@
 //!
 //! Ignored by default. Run in release mode to see the timings:
 //! `cargo test --release -p ig-git --test log_perf -- --ignored --nocapture`
+//!
+//! `graph_options_on_50k_commits` times the visible graphs of every sort and filter option (BEK,
+//! linear BEK, first-parent, branch and commit filters) and a fragment hover on the same
+//! repository.
 
 mod common;
 
@@ -15,13 +19,15 @@ use std::time::{Duration, Instant};
 
 use common::log_helpers::{commit_at, provider};
 use common::{git, git_command};
+use ig_log::actions::{GraphAction, GraphActionType};
+use ig_log::api::printer::GraphPrintElement;
 use ig_log::data::graph_data::build_data;
 use ig_log::data::refresher::compact_commit;
 use ig_log::data::{
     InMemoryStorage, LogProviders, RootRefsModel, VcsLogProvider, VcsLogRefresher, VcsLogStorage,
     vcs_root,
 };
-use ig_log::{Options, PermanentGraph, VisibleGraph};
+use ig_log::{Options, PermanentGraph, SortType, VisibleGraph};
 
 const TOTAL_COMMITS: usize = 50_000;
 
@@ -210,9 +216,7 @@ fn load_50k_commits() {
 
     let graph = pack.permanent_graph().unwrap();
     let start = Instant::now();
-    let visible = graph
-        .create_visible_graph(Options::DEFAULT, None, None)
-        .unwrap();
+    let visible = graph.create_visible_graph(Options::DEFAULT, None, None);
     let t_visible = start.elapsed();
     assert_eq!(visible.visible_commit_count() as usize, commits);
 
@@ -305,3 +309,114 @@ fn load_50k_commits() {
         ms(t_refresh_tags / NEW_TAGS as u32)
     );
 }
+
+#[test]
+#[ignore = "performance smoke test; run with --release -- --ignored --nocapture"]
+fn graph_options_on_50k_commits() {
+    let tmp = tempfile::tempdir().unwrap();
+    let dir = tmp.path().join("big");
+    std::fs::create_dir_all(&dir).unwrap();
+    let (commits, branches) = generate_repository(&dir);
+    let root = vcs_root(&dir);
+    let provider = Arc::new(provider());
+    let mut timed = Vec::with_capacity(commits);
+    let data = provider
+        .read_all_hashes(&root, &mut |c| timed.push(c))
+        .unwrap();
+    let storage: Arc<dyn VcsLogStorage> = Arc::new(InMemoryStorage::new());
+    let graph_commits: Vec<_> = timed
+        .iter()
+        .map(|c| compact_commit(storage.as_ref(), c, &root))
+        .collect();
+    let refs = RootRefsModel::create(data.refs.iter().cloned(), storage.as_ref());
+    let mut providers = LogProviders::new();
+    providers.insert(root.clone(), provider.clone() as Arc<dyn VcsLogProvider>);
+    let pack = build_data(
+        &graph_commits,
+        BTreeMap::from([(root.clone(), Arc::new(refs))]),
+        &providers,
+        &storage,
+        true,
+    );
+    let graph = pack.permanent_graph().unwrap();
+    eprintln!("{commits} commits, {branches} branches");
+
+    let timed_graph = |label: &str, options, heads, matched| {
+        let start = Instant::now();
+        let visible = graph.create_visible_graph(options, heads, matched);
+        let elapsed = start.elapsed();
+        eprintln!(
+            "{label:<40} {}  ({} rows)",
+            ms(elapsed),
+            visible.visible_commit_count()
+        );
+        visible
+    };
+    let mut normal = timed_graph("Normal (CollapsedController)", Options::DEFAULT, None, None);
+    timed_graph(
+        "Bek (first: computes the BEK map)",
+        Options::Base(SortType::Bek),
+        None,
+        None,
+    );
+    timed_graph(
+        "Bek (shared BEK map)",
+        Options::Base(SortType::Bek),
+        None,
+        None,
+    );
+    let mut linear_bek = timed_graph("LinearBek", Options::LinearBek, None, None);
+    timed_graph("FirstParent", Options::FirstParent, None, None);
+
+    let head = normal.get_row_info(0).commit();
+    let heads = std::collections::HashSet::from([head]);
+    timed_graph(
+        "Normal, one visible branch",
+        Options::DEFAULT,
+        Some(&heads),
+        None,
+    );
+    timed_graph(
+        "LinearBek, one visible branch",
+        Options::LinearBek,
+        Some(&heads),
+        None,
+    );
+    let matched: std::collections::HashSet<_> = (0..normal.visible_commit_count())
+        .step_by(3)
+        .map(|row| normal.get_row_info(row).commit())
+        .collect();
+    timed_graph(
+        "Normal, every third commit matched",
+        Options::DEFAULT,
+        None,
+        Some(&matched),
+    );
+
+    // fragment hover on a commit in the middle of the main line
+    let row = normal.visible_commit_count() / 2;
+    let node = normal
+        .get_row_info(row)
+        .print_elements()
+        .into_iter()
+        .find(|e| matches!(e, GraphPrintElement::Node(_)))
+        .unwrap();
+    let start = Instant::now();
+    normal
+        .action_controller()
+        .perform_action(GraphAction::new(Some(node), GraphActionType::MouseOver));
+    eprintln!("{:<40} {}", "fragment hover (Normal)", ms(start.elapsed()));
+
+    // linear BEK hover walks up to the first merge that answers (upstream quirk)
+    let node = linear_bek
+        .get_row_info(row)
+        .print_elements()
+        .into_iter()
+        .find(|e| matches!(e, GraphPrintElement::Node(_)))
+        .unwrap();
+    let start = Instant::now();
+    linear_bek
+        .action_controller()
+        .perform_action(GraphAction::new(Some(node), GraphActionType::MouseOver));
+    eprintln!("{:<40} {}", "merge hover (LinearBek)", ms(start.elapsed()));
+}
```

`crates/ig-git/tests/log_refresh.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-git/tests/log_refresh.rs b/crates/ig-git/tests/log_refresh.rs
index 97bcfe7..8c2e10c 100644
--- a/crates/ig-git/tests/log_refresh.rs
+++ b/crates/ig-git/tests/log_refresh.rs
@@ -233,9 +233,7 @@ fn multi_repo_initial_load() {
     assert_eq!(best.name(), "master");
     assert!(graph.get_containing_branches(&a_head).contains(&a_head));
 
-    let visible = graph
-        .create_visible_graph(Options::DEFAULT, None, None)
-        .unwrap();
+    let visible = graph.create_visible_graph(Options::DEFAULT, None, None);
     assert_eq!(visible.visible_commit_count(), 12);
 }
 
```

`crates/ig-log/tests/graph_options.rs`:

```rust
//! IdeaGit tests for `PermanentGraphImpl::create_visible_graph` with every option: the controller
//! chains, the shared BEK map, linear BEK actions and `SimpleGraphInfo`. Upstream has no tests for
//! these paths; expected values follow from the upstream algorithms.

use std::collections::HashSet;
use std::sync::Arc;

use ig_log::actions::{GraphAction, GraphActionType};
use ig_log::api::permanent::PermanentGraphInfo;
use ig_log::api::printer::GraphPrintElement;
use ig_log::facade::permanent_graph_impl::PermanentGraphImpl;
use ig_log::facade::visible_graph_impl::VisibleGraphImpl;
use ig_log::graph_commit::{GraphCommit, GraphCommitImpl};
use ig_log::permanent_graph::{Options, PermanentGraph, SortType};
use ig_log::print::graph_color_getter::GraphColorGetterByNodeFactory;
use ig_log::print_element::PrintElement;
use ig_log::visible_graph::VisibleGraph;

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

/// Two merged feature branches, a branch that is not merged, and not-loaded parents (`x`, `y`).
const GRAPH: &str = "
m0|-m1 f2
f2|-f1
m1|-m2 g1
g1|-g0 y
m2|-m3
f1|-f0
f0|-m3
g0|-m3
m3|-m4
b1|-b0
b0|-m4
m4|-x
";

fn options() -> [Options; 4] {
    [
        Options::Base(SortType::Normal),
        Options::Base(SortType::Bek),
        Options::LinearBek,
        Options::FirstParent,
    ]
}

fn act(
    visible: &mut VisibleGraphImpl<String>,
    element: Option<GraphPrintElement>,
    kind: GraphActionType,
) {
    let answer = visible
        .action_controller()
        .perform_action(GraphAction::new(element, kind));
    if let Some(updater) = answer.graph_updater {
        visible.run_graph_updater(updater);
    }
}

/// Reads every row and performs every action on every element (re-reading the rows, as actions
/// may change the graph).
fn exercise(visible: &mut VisibleGraphImpl<String>, stale: &[GraphPrintElement]) {
    for kind in [GraphActionType::MouseOver, GraphActionType::MouseClick] {
        let mut row = 0;
        while row < visible.visible_commit_count() {
            let elements = {
                let info = visible.get_row_info(row);
                let _ = (info.commit(), info.one_of_heads(), info.row_type());
                let _ = (info.adjacent_rows(true), info.adjacent_rows(false));
                info.print_elements()
            };
            for element in elements {
                if element.row_index() < visible.visible_commit_count() {
                    act(visible, Some(element), kind);
                }
            }
            row += 1;
        }
        act(visible, None, kind);
    }
    for element in stale {
        act(visible, Some(element.clone()), GraphActionType::MouseOver);
        act(visible, Some(element.clone()), GraphActionType::MouseClick);
    }
    for kind in [
        GraphActionType::ButtonCollapse,
        GraphActionType::ButtonExpand,
        GraphActionType::ButtonCollapse,
    ] {
        act(visible, None, kind);
        assert!(visible.recommended_width() >= 0);
    }
    assert_eq!(visible.get_visible_row_index(&"zz".to_string()), None);
    assert_eq!(visible.get_visible_row_index(&"x".to_string()), None);
}

/// IdeaGit: every option with every kind of visible heads and matched commits, including empty
/// sets and ids that are unknown or not loaded, builds a visible graph that survives every action.
#[test]
fn every_option_combination_works() {
    let graph = permanent_graph(GRAPH, &["m0", "b1"]);
    // elements from a bigger graph: out of range here
    let mut long_text = String::from("n0|-n1 n40\n");
    for i in 1..40 {
        long_text.push_str(&format!("n{i}|-n{}\n", i + 1));
    }
    long_text.push_str("n40|-\n");
    let big = permanent_graph(&long_text, &[]).create_visible_graph(Options::DEFAULT, None, None);
    let stale: Vec<GraphPrintElement> = [1, 20, 39]
        .iter()
        .flat_map(|&row| big.get_row_info(row).print_elements())
        .collect();

    let heads = [
        None,
        Some(set(&[])),
        Some(set(&["b1"])),
        Some(set(&["m1", "f1"])),
        Some(set(&["zz", "x", "y", "g1"])),
    ];
    let matched = [
        None,
        Some(set(&[])),
        Some(set(&["m0", "f0", "m3", "b0"])),
        Some(set(&["zz", "x", "g1", "m4"])),
    ];
    for options in options() {
        for heads in &heads {
            for matched in &matched {
                let mut visible =
                    graph.create_visible_graph(options, heads.as_ref(), matched.as_ref());
                let count = visible.visible_commit_count();
                if matched.as_ref().is_some_and(HashSet::is_empty) {
                    assert_eq!(count, 0, "{options:?} {heads:?} {matched:?}");
                }
                exercise(&mut visible, &stale);
            }
        }
    }
}

#[test]
fn an_empty_graph_works_with_every_option() {
    let graph = permanent_graph("", &[]);
    for options in options() {
        let mut visible = graph.create_visible_graph(options, None, None);
        assert_eq!(visible.visible_commit_count(), 0);
        exercise(&mut visible, &[]);
    }
}

#[test]
fn the_bek_map_is_computed_once_and_shared() {
    let graph = permanent_graph(GRAPH, &["m0", "b1"]);
    let first = graph.bek_int_map();
    let bek = graph.create_visible_graph(Options::Base(SortType::Bek), None, None);
    let linear_bek = graph.create_visible_graph(Options::LinearBek, None, None);
    assert!(Arc::ptr_eq(&first, &graph.bek_int_map()));
    assert_eq!(first.size(), 12);
    // the Bek graph shows the permanent rows in BEK order
    for row in 0..bek.visible_commit_count() {
        assert_eq!(
            bek.get_row_info(row).commit(),
            graph
                .all_commits()
                .get(first.get_usual_index(row) as usize)
                .id()
                .clone()
        );
        assert_eq!(
            linear_bek.get_row_info(row).commit(),
            bek.get_row_info(row).commit()
        );
    }
}

/// Commits of the rows whose node is printed selected.
fn selected_commits(visible: &VisibleGraphImpl<String>) -> Vec<String> {
    (0..visible.visible_commit_count())
        .filter(|&row| {
            visible
                .get_row_info(row)
                .print_elements()
                .iter()
                .any(|e| matches!(e, GraphPrintElement::Node(_)) && e.is_selected())
        })
        .map(|row| visible.get_row_info(row).commit())
        .collect()
}

fn dotted_edges(visible: &VisibleGraphImpl<String>) -> usize {
    use ig_log::api::EdgeFilter;
    use ig_log::api::elements::GraphEdgeType;
    let graph = visible.linear_graph();
    (0..graph.nodes_count())
        .flat_map(|row| graph.get_adjacent_edges(row, EdgeFilter::NormalDown))
        .filter(|edge| edge.edge_type == GraphEdgeType::Dotted)
        .count()
}

fn node(visible: &VisibleGraphImpl<String>, commit: &str) -> GraphPrintElement {
    let row = visible.get_visible_row_index(&commit.to_string()).unwrap();
    visible
        .get_row_info(row)
        .print_elements()
        .into_iter()
        .find(|e| matches!(e, GraphPrintElement::Node(_)))
        .unwrap()
}

fn act_node(visible: &mut VisibleGraphImpl<String>, commit: &str, kind: GraphActionType) {
    let element = node(visible, commit);
    act(visible, Some(element), kind);
}

/// The linear BEK example of `LinearBekTest.simpleGraphTest`, through the facade.
#[test]
fn linear_bek_collapses_merges_and_expands_them_on_click() {
    let graph = permanent_graph(
        "
        c0|-c4 c1
        c1|-c2
        c2|-c3
        c3|-c7
        c4|-c5
        c5|-c6
        c6|-c7
        c7|-
        ",
        &["c0"],
    );
    let mut visible = graph.create_visible_graph(Options::LinearBek, None, None);
    assert_eq!(
        format!("{visible:?}"),
        r#"VisibleGraph["c0", "c1", "c2", "c3", "c4", "c5", "c6", "c7"]"#
    );
    assert_eq!(dotted_edges(&visible), 1);

    // hovering the upper node of a dotted edge highlights the edge's ends
    act_node(&mut visible, "c3", GraphActionType::MouseOver);
    assert_eq!(selected_commits(&visible), ["c3", "c4"]);

    // a click on it expands the merge
    act_node(&mut visible, "c3", GraphActionType::MouseClick);
    assert_eq!(dotted_edges(&visible), 0);

    // hovering a commit of an expanded merge highlights the merge's fragment
    act_node(&mut visible, "c2", GraphActionType::MouseOver);
    assert_eq!(selected_commits(&visible), ["c0", "c1", "c2", "c3", "c4"]);

    // a click on it collapses the merge again
    act_node(&mut visible, "c2", GraphActionType::MouseClick);
    assert_eq!(dotted_edges(&visible), 1);

    act(&mut visible, None, GraphActionType::ButtonExpand);
    assert_eq!(dotted_edges(&visible), 0);
    act(&mut visible, None, GraphActionType::ButtonCollapse);
    assert_eq!(dotted_edges(&visible), 1);
}

#[test]
fn simple_graph_info_copies_the_rows_around_a_row() {
    let graph = permanent_graph(GRAPH, &["m0", "b1"]);
    let visible = graph.create_visible_graph(Options::DEFAULT, None, None);
    let info = visible.build_simple_graph_info(3, 2);
    let commits = info.permanent_commits_info();
    // rows 1..5
    let ids: Vec<String> = (0..4).map(|i| commits.get_commit_id(i)).collect();
    let expected: Vec<String> = (1..5).map(|r| visible.get_row_info(r).commit()).collect();
    assert_eq!(ids, expected);
    assert_eq!(info.linear_graph().nodes_count(), 4);
    assert_eq!(commits.get_node_id(&expected[2]), 2);
    assert_eq!(commits.get_node_id(&"zz".to_string()), -1);
    assert!(
        !info
            .permanent_graph_layout()
            .get_head_node_index()
            .is_empty()
    );
}
```

`crates/ig-log/tests/permanent_graph.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/tests/permanent_graph.rs b/crates/ig-log/tests/permanent_graph.rs
index 9679264..5d28e26 100644
--- a/crates/ig-log/tests/permanent_graph.rs
+++ b/crates/ig-log/tests/permanent_graph.rs
@@ -1,4 +1,4 @@
-//! IdeaGit tests for the facade (`PermanentGraphImpl`, `VisibleGraphImpl`) with `SortType::Normal`.
+//! IdeaGit tests for the facade (`PermanentGraphImpl`, `VisibleGraphImpl`).
 //! Upstream has no direct tests for this path; expected values follow from the upstream algorithms
 //! (and from the ported `containingBranches` fixtures).
 
@@ -119,9 +119,7 @@ fn children_and_containing_branches() {
 #[test]
 fn visible_graph_normal_rows() {
     let graph = permanent_graph(NOT_FULL_GRAPH, &["a0", "a2"]);
-    let visible = graph
-        .create_visible_graph(Options::DEFAULT, None, None)
-        .unwrap();
+    let visible = graph.create_visible_graph(Options::DEFAULT, None, None);
 
     assert_eq!(visible.visible_commit_count(), 5);
     assert_eq!(
@@ -161,9 +159,7 @@ fn visible_graph_print_elements_and_colors() {
         |a: &String, b: &String| a.cmp(b),
         &set(&["a0", "a2"]),
     );
-    let visible = graph
-        .create_visible_graph_sorted(SortType::Normal, None, None)
-        .unwrap();
+    let visible = graph.create_visible_graph_sorted(SortType::Normal, None, None);
 
     // row 0: node a0 with its down edges to a1 and a3 and to the not-loaded a6
     let elements = visible.get_row_info(0).print_elements();
@@ -190,9 +186,7 @@ fn long_edge_arrow_actions() {
     }
     text.push_str("n40|-\n");
     let graph = permanent_graph(&text, &[]);
-    let mut visible = graph
-        .create_visible_graph(Options::DEFAULT, None, None)
-        .unwrap();
+    let mut visible = graph.create_visible_graph(Options::DEFAULT, None, None);
 
     // long edges are hidden by default: at row 1 the edge ends in a terminal down arrow
     assert!(visible.action_controller().are_long_edges_hidden());
@@ -232,14 +226,6 @@ fn long_edge_arrow_actions() {
     assert_eq!(click.cursor_to_set, Some(Cursor::Default));
     assert!(click.do_jump);
 
-    // a click on a node goes to the BaseController: default cursor, nothing to jump to
-    let node = visible.get_row_info(5).print_elements().pop().unwrap();
-    let answer = visible
-        .action_controller()
-        .perform_action(GraphAction::new(Some(node), GraphActionType::MouseClick));
-    assert_eq!(answer.cursor_to_set, Some(Cursor::Default));
-    assert!(answer.commit_to_jump.is_none());
-
     // hidden long edges are not drawn in the middle; shown ones are (with arrows next to the ends)
     let long_edge_parts = |visible: &ig_log::facade::visible_graph_impl::VisibleGraphImpl<
         String,
@@ -262,45 +248,63 @@ fn long_edge_arrow_actions() {
             .iter()
             .any(|e| matches!(e, GraphPrintElement::Edge(edge) if edge.has_arrow()))
     );
+
+    // a click on a node collapses its linear fragment (CollapsedController): n2..=n38 are hidden
+    // behind a dotted edge n1 - n39; nothing to jump to
+    let node = visible.get_row_info(5).print_elements().pop().unwrap();
+    let answer = visible
+        .action_controller()
+        .perform_action(GraphAction::new(Some(node), GraphActionType::MouseClick));
+    assert_eq!(answer.cursor_to_set, None);
+    assert!(answer.commit_to_jump.is_none());
+    assert_eq!(
+        format!("{visible:?}"),
+        r#"VisibleGraph["n0", "n1", "n39", "n40"]"#
+    );
 }
 
 #[test]
-fn unsupported_options_are_errors() {
+fn every_option_is_supported() {
     let graph = permanent_graph(NOT_FULL_GRAPH, &[]);
-    let heads = set(&["a0"]);
-    assert!(
-        graph
-            .create_visible_graph(Options::Base(SortType::Bek), None, None)
-            .is_err()
+    let heads = set(&["a2"]);
+    let shown = |options, heads: Option<&HashSet<String>>, matched: Option<&HashSet<String>>| {
+        format!("{:?}", graph.create_visible_graph(options, heads, matched))
+    };
+    assert_eq!(
+        shown(Options::Base(SortType::Bek), None, None),
+        r#"VisibleGraph["a2", "a0", "a3", "a1", "a4"]"#
     );
-    assert!(
-        graph
-            .create_visible_graph(Options::LinearBek, None, None)
-            .is_err()
+    assert_eq!(
+        shown(Options::LinearBek, None, None),
+        r#"VisibleGraph["a2", "a0", "a3", "a1", "a4"]"#
     );
-    assert!(
-        graph
-            .create_visible_graph(Options::FirstParent, None, None)
-            .is_err()
+    assert_eq!(
+        shown(Options::FirstParent, None, None),
+        r#"VisibleGraph["a0", "a1", "a2", "a3", "a4"]"#
     );
-    assert!(
-        graph
-            .create_visible_graph(Options::DEFAULT, Some(&heads), None)
-            .is_err()
+    assert_eq!(
+        shown(Options::DEFAULT, Some(&heads), None),
+        r#"VisibleGraph["a2", "a3"]"#
+    );
+    assert_eq!(
+        shown(Options::DEFAULT, None, Some(&set(&["a0", "a3", "a4"]))),
+        r#"VisibleGraph["a0", "a3", "a4"]"#
+    );
+    assert_eq!(
+        shown(Options::LinearBek, Some(&heads), None),
+        r#"VisibleGraph["a2", "a3"]"#
+    );
+    assert_eq!(
+        shown(Options::FirstParent, Some(&heads), None),
+        r#"VisibleGraph["a2", "a3"]"#
     );
-    let error = graph
-        .create_visible_graph(Options::DEFAULT, None, Some(&heads))
-        .unwrap_err();
-    assert!(error.to_string().contains("Plan 2b"), "{error}");
 }
 
 #[test]
 fn empty_graph() {
     let graph = permanent_graph("", &[]);
     assert!(graph.all_commits().is_empty());
-    let visible = graph
-        .create_visible_graph(Options::DEFAULT, None, None)
-        .unwrap();
+    let visible = graph.create_visible_graph(Options::DEFAULT, None, None);
     assert_eq!(visible.visible_commit_count(), 0);
     assert_eq!(visible.recommended_width(), 0);
 }
```

`crates/ig-log/tests/permanent_graph_behaviour.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/tests/permanent_graph_behaviour.rs b/crates/ig-log/tests/permanent_graph_behaviour.rs
index 97faa79..7f8cde6 100644
--- a/crates/ig-log/tests/permanent_graph_behaviour.rs
+++ b/crates/ig-log/tests/permanent_graph_behaviour.rs
@@ -83,9 +83,7 @@ fn colored_graph(text: &str, branches: &[&str]) -> PermanentGraphImpl<String> {
 }
 
 fn visible(graph: &PermanentGraphImpl<String>) -> VisibleGraphImpl<String> {
-    graph
-        .create_visible_graph_sorted(SortType::Normal, None, None)
-        .unwrap()
+    graph.create_visible_graph_sorted(SortType::Normal, None, None)
 }
 
 fn not_full() -> VisibleGraphImpl<String> {
@@ -162,9 +160,7 @@ fn node_and_edge_colors() {
 #[test]
 fn hover_up_arrow_of_long_edge_jumps_to_top_node() {
     let graph = permanent_graph(&long_edge_text(), &[]);
-    let mut visible = graph
-        .create_visible_graph(Options::DEFAULT, None, None)
-        .unwrap();
+    let mut visible = graph.create_visible_graph(Options::DEFAULT, None, None);
     let row = (0..41)
         .find(|&r| {
             visible.get_row_info(r).print_elements().iter().any(|e| {
@@ -239,16 +235,24 @@ fn action_support() {
     let node = node_of(&visible, 0);
     let controller = visible.action_controller();
     let action = |kind| GraphAction::new(Some(node.clone()), kind);
+    assert!(controller.is_action_supported(&action(GraphActionType::ButtonCollapse)));
+    assert!(controller.is_action_supported(&action(GraphActionType::ButtonExpand)));
+    assert!(controller.is_action_supported(&action(GraphActionType::MouseOver)));
+    assert!(controller.is_action_supported(&action(GraphActionType::MouseClick)));
+
+    // a filtered graph (FilteredController) cannot collapse or expand
+    let graph = colored_graph(NOT_FULL_GRAPH, &["a0", "a2"]);
+    let matched: HashSet<String> = ["a0".to_string()].into_iter().collect();
+    let mut filtered = graph.create_visible_graph(Options::DEFAULT, None, Some(&matched));
+    let controller = filtered.action_controller();
     assert!(!controller.is_action_supported(&action(GraphActionType::ButtonCollapse)));
     assert!(!controller.is_action_supported(&action(GraphActionType::ButtonExpand)));
     assert!(controller.is_action_supported(&action(GraphActionType::MouseOver)));
-    assert!(controller.is_action_supported(&action(GraphActionType::MouseClick)));
 }
 
 #[test]
 fn hover_then_refetch_shows_selection_and_old_snapshot_does_not() {
-    // only arrows are selectable on hover: a node hover goes to the BaseController, which
-    // selects nothing
+    // the hovered arrow is selected, and only in the print elements fetched after the hover
     let mut visible = not_full();
     let before = visible.get_row_info(1).print_elements();
     let arrow = arrow_in(&visible, 1, EdgePrintElementType::Down);
@@ -266,18 +270,14 @@ fn hover_then_refetch_shows_selection_and_old_snapshot_does_not() {
 fn stale_arrow_elements_are_ignored() {
     // long-edge arrow from a 41-row graph on a 2-row graph
     let big = permanent_graph(&long_edge_text(), &[]);
-    let mut big = big
-        .create_visible_graph(Options::DEFAULT, None, None)
-        .unwrap();
+    let mut big = big.create_visible_graph(Options::DEFAULT, None, None);
     let long_arrow = arrow_in(&big, 1, EdgePrintElementType::Down);
     let up_arrow = arrow_in(&big, 39, EdgePrintElementType::Up);
     // not-loaded arrow (target -2) from NOT_FULL_GRAPH on a graph without not-loaded commits
     let not_loaded_arrow = arrow_in(&not_full(), 1, EdgePrintElementType::Down);
 
     let small = permanent_graph("x0|-x1\nx1|-\n", &[]);
-    let mut small = small
-        .create_visible_graph(Options::DEFAULT, None, None)
-        .unwrap();
+    let mut small = small.create_visible_graph(Options::DEFAULT, None, None);
     assert_eq!(small.visible_commit_count(), 2);
     for stale in [&long_arrow, &up_arrow, &not_loaded_arrow] {
         for kind in [GraphActionType::MouseOver, GraphActionType::MouseClick] {
@@ -297,9 +297,7 @@ fn stale_arrow_elements_are_ignored() {
 }
 
 fn two_row_graph() -> impl VisibleGraph<String> {
-    permanent_graph("x0|-x1\nx1|-\n", &[])
-        .create_visible_graph(Options::DEFAULT, None, None)
-        .unwrap()
+    permanent_graph("x0|-x1\nx1|-\n", &[]).create_visible_graph(Options::DEFAULT, None, None)
 }
 
 #[test]
```

`crates/ig-log/tests/print_element_generator_extra.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/tests/print_element_generator_extra.rs b/crates/ig-log/tests/print_element_generator_extra.rs
index 2c5cc7e..2af7171 100644
--- a/crates/ig-log/tests/print_element_generator_extra.rs
+++ b/crates/ig-log/tests/print_element_generator_extra.rs
@@ -42,9 +42,7 @@ fn linear_chain(n: usize) -> Vec<GraphCommitImpl<String>> {
 fn recommended_width_of_linear_history_is_one() {
     for n in [10, 36] {
         let graph = build(&linear_chain(n), &HashSet::new());
-        let visible = graph
-            .create_visible_graph(Options::DEFAULT, None, None)
-            .unwrap();
+        let visible = graph.create_visible_graph(Options::DEFAULT, None, None);
         assert_eq!(visible.recommended_width(), 1, "n = {n}");
     }
 }
@@ -87,9 +85,7 @@ fn long_edge_cut_points_are_terminal_arrows() {
     )];
     commits.extend(linear_chain(80));
     let graph = build(&commits, &HashSet::new());
-    let visible = graph
-        .create_visible_graph(Options::DEFAULT, None, None)
-        .unwrap();
+    let visible = graph.create_visible_graph(Options::DEFAULT, None, None);
     let edges: Vec<_> = (0..visible.visible_commit_count())
         .flat_map(|row| visible.get_row_info(row).print_elements())
         .filter_map(|e| match e {
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -q --locked -p ig-git --test log_perf --test log_refresh
cargo test -q --locked -p ig-log --test graph_options --test permanent_graph --test permanent_graph_behaviour --test print_element_generator_extra
```

Expected: FAIL — the tests call the infallible `create_visible_graph`, which still returns `Result`, so they fail to compile. The simulation's first error:

```text
error[E0599]: no method named `visible_commit_count` found for enum `Result<T, E>` in the current scope
```

- [ ] **Step 3: Copy the code**

```bash
git checkout 1ec0a3d -- crates/ig-app/src/log/worker.rs crates/ig-log/src/facade/mod.rs crates/ig-log/src/facade/permanent_graph_impl.rs crates/ig-log/src/facade/simple_graph_info.rs crates/ig-log/src/facade/visible_graph_impl.rs crates/ig-log/src/lib.rs crates/ig-log/src/linear_bek/linear_bek_controller.rs crates/ig-log/src/permanent_graph.rs
```

`crates/ig-app/src/log/worker.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-app/src/log/worker.rs b/crates/ig-app/src/log/worker.rs
index dcb3bdf..edf3c48 100644
--- a/crates/ig-app/src/log/worker.rs
+++ b/crates/ig-app/src/log/worker.rs
@@ -669,7 +669,7 @@ impl SnapshotThread {
         let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
         let visible = data
             .permanent_graph()
-            .and_then(|g| g.create_visible_graph(Options::DEFAULT, None, None).ok());
+            .map(|g| g.create_visible_graph(Options::DEFAULT, None, None));
         let recommended_width = visible.as_ref().map_or(0, |v| v.recommended_width());
         if let Some(visible) = &visible {
             let first_rows: Vec<_> = (0..visible.visible_commit_count().min(FIRST_ROWS))
```

`crates/ig-log/src/facade/mod.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/facade/mod.rs b/crates/ig-log/src/facade/mod.rs
index 674997b..df1990a 100644
--- a/crates/ig-log/src/facade/mod.rs
+++ b/crates/ig-log/src/facade/mod.rs
@@ -1,8 +1,7 @@
 //! The graph facade: the permanent graph, visible graphs and the controller chain behind them.
 //!
-//! Ports the `Normal`-sort subset of `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/`;
-//! see each submodule. Not ported yet (Plan 2b): `CascadeController.kt`, `FilteredController.kt`,
-//! `FirstParentController.kt`, `GraphModificationUtil.kt` and `SimpleGraphInfo.java`.
+//! Ports `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/`; see each submodule.
+//! `RowsMapping.kt` lives in [`permanent_commits_info_impl`](crate::permanent::permanent_commits_info_impl).
 
 pub mod base_controller;
 pub mod cascade_controller;
@@ -15,5 +14,6 @@ pub mod linear_graph_wrapper;
 pub mod permanent_graph_impl;
 pub mod print_element_presentation_manager_impl;
 pub mod reachable_nodes;
+pub mod simple_graph_info;
 pub mod sort;
 pub mod visible_graph_impl;
```

`crates/ig-log/src/facade/permanent_graph_impl.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/facade/permanent_graph_impl.rs b/crates/ig-log/src/facade/permanent_graph_impl.rs
index 4240e9f..d7cfd48 100644
--- a/crates/ig-log/src/facade/permanent_graph_impl.rs
+++ b/crates/ig-log/src/facade/permanent_graph_impl.rs
@@ -2,13 +2,19 @@
 //!
 //! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/PermanentGraphImpl.kt`.
 //!
+//! [`create_visible_graph`](PermanentGraph::create_visible_graph) builds upstream's controller
+//! chains (`createFilteredController`):
+//! - `Base(Normal)` / `Base(Bek)`: [`BaseController`] / [`SortedBaseController`], wrapped in a
+//!   [`FilteredController`] if commits are matched, otherwise in a [`CollapsedController`] (which
+//!   also shows only the visible branches);
+//! - `FirstParent`: [`BaseController`] in a first-parent [`FilteredController`];
+//! - `LinearBek`: [`LinearBekController`] over [`SortedBaseController`], wrapped in a
+//!   [`FilteredController`] if commits are matched, otherwise in a [`BranchFilterController`] if
+//!   branches are filtered.
+//!
 //! Deviations:
-//! - Only `Options::Base(SortType::Normal)` without branch or text filters is supported; other
-//!   options fail with [`UnsupportedGraphOptions`] (Plan 2b ports BEK, linear BEK, first-parent,
-//!   `CollapsedController` and `FilteredController`).
-//! - The unfiltered `Normal` graph uses [`BaseController`] directly. Upstream wraps it in a
-//!   `CollapsedController` with every node visible, whose compiled graph is identical; only the
-//!   collapse/expand and fragment hover actions are missing (see `VisibleGraphImpl`).
+//! - Upstream's `bekIntMap by lazy` is a `OnceLock`: the BEK order is computed the first time a
+//!   `Bek` or `LinearBek` visible graph is created, then shared by every later one.
 //! - Branch node ids are passed to the layout builder sorted ascending (upstream: hash-set order,
 //!   which only matters for heads that `head_commits_comparator` considers equal).
 //! - The `Int`-specialised commit and condition classes are not needed.
@@ -16,17 +22,25 @@
 use std::cmp::Ordering;
 use std::collections::{HashMap, HashSet};
 use std::hash::Hash;
-use std::sync::Arc;
+use std::sync::{Arc, OnceLock};
 
 use crate::api::GraphLayout;
 use crate::api::linear_graph::LinearGraph;
 use crate::api::permanent::{PermanentCommitsInfo, PermanentGraphInfo, VcsLogGraphNodeId};
 use crate::api::printer::{GraphColorGetter, GraphColorGetterFactory};
+use crate::collapsing::branch_filter_controller::BranchFilterController;
+use crate::collapsing::collapsed_controller::CollapsedController;
 use crate::facade::base_controller::BaseController;
+use crate::facade::filtered_controller::FilteredController;
+use crate::facade::first_parent_controller;
 use crate::facade::linear_graph_controller::LinearGraphController;
 use crate::facade::reachable_nodes::ReachableNodes;
+use crate::facade::sort::bek::bek_sorter;
+use crate::facade::sort::sort_index_map::SortIndexMap;
+use crate::facade::sort::sorted_base_controller::SortedBaseController;
 use crate::facade::visible_graph_impl::VisibleGraphImpl;
 use crate::graph_commit::{GraphCommit, GraphCommitImpl};
+use crate::linear_bek::linear_bek_controller::LinearBekController;
 use crate::permanent::graph_layout_builder;
 use crate::permanent::graph_layout_impl::GraphLayoutImpl;
 use crate::permanent::permanent_commits_info_impl::PermanentCommitsInfoImpl;
@@ -34,7 +48,6 @@ use crate::permanent::permanent_linear_graph_builder::PermanentLinearGraphBuilde
 use crate::permanent::permanent_linear_graph_impl::PermanentLinearGraphImpl;
 use crate::permanent_graph::{
     AllCommits, ContainedInBranchCondition, Options, PermanentGraph, SortType,
-    UnsupportedGraphOptions,
 };
 use crate::utils::linear_graph_utils::{
     LiteLinearGraphAdapter, as_lite_linear_graph, get_up_nodes,
@@ -85,6 +98,7 @@ impl<CommitId: Clone> NotLoadedCommitsIdsGenerator<CommitId> {
 
 pub struct PermanentGraphImpl<CommitId> {
     parts: Arc<PermanentGraphParts<CommitId>>,
+    bek_int_map: OnceLock<Arc<SortIndexMap>>,
     graph_color_getter: Arc<dyn GraphColorGetter>,
     reachable_nodes: ReachableNodes<LiteLinearGraphAdapter<Arc<PermanentLinearGraphImpl>>>,
 }
@@ -138,6 +152,7 @@ impl<CommitId: CommitIdBounds> PermanentGraphImpl<CommitId> {
 
         PermanentGraphImpl {
             parts,
+            bek_int_map: OnceLock::new(),
             graph_color_getter,
             reachable_nodes,
         }
@@ -160,35 +175,87 @@ impl<CommitId: CommitIdBounds> PermanentGraphImpl<CommitId> {
         &self.parts.permanent_graph_layout
     }
 
+    /// The BEK order of the permanent graph, computed on first use (`bekIntMap by lazy`).
+    pub fn bek_int_map(&self) -> Arc<SortIndexMap> {
+        self.bek_int_map
+            .get_or_init(|| {
+                Arc::new(bek_sorter::create_bek_map(
+                    &*self.parts.permanent_linear_graph,
+                    &self.parts.permanent_graph_layout,
+                    self.parts.permanent_commits_info.timestamp_getter(),
+                ))
+            })
+            .clone()
+    }
+
+    /// The controller chain for `options` (`createFilteredController`). Unknown commits in
+    /// `visible_heads` / `matching_commits` are skipped.
     fn create_filtered_controller(
         &self,
         options: Options,
         visible_heads: Option<&HashSet<CommitId>>,
         matching_commits: Option<&HashSet<CommitId>>,
-    ) -> Result<Box<dyn LinearGraphController>, UnsupportedGraphOptions> {
+    ) -> Box<dyn LinearGraphController> {
+        let info = &self.parts.permanent_commits_info;
+        let visible_heads_ids = visible_heads.map(|heads| info.convert_to_node_ids(heads));
+        let matching_commit_ids = matching_commits.map(|commits| info.convert_to_node_ids(commits));
+        let permanent_graph_info = self.permanent_graph_info();
+
         match options {
-            Options::Base(SortType::Normal) => {
-                if matching_commits.is_some() {
-                    return Err(UnsupportedGraphOptions(
-                        "Filtering by matched commits (FilteredController)".to_string(),
+            Options::Base(sort_type) => {
+                let base_controller: Box<dyn LinearGraphController> = match sort_type {
+                    SortType::Normal => Box::new(BaseController::new(permanent_graph_info.clone())),
+                    SortType::Bek => Box::new(SortedBaseController::new(
+                        &*permanent_graph_info,
+                        self.bek_int_map(),
+                    )),
+                };
+                if let Some(matching_commit_ids) = &matching_commit_ids {
+                    return Box::new(FilteredController::create(
+                        base_controller,
+                        &*permanent_graph_info,
+                        matching_commit_ids,
+                        visible_heads_ids.as_ref(),
+                    ));
+                }
+                Box::new(CollapsedController::new(
+                    base_controller,
+                    permanent_graph_info,
+                    visible_heads_ids.as_ref(),
+                ))
+            }
+            Options::FirstParent => {
+                let base_controller = Box::new(BaseController::new(permanent_graph_info.clone()));
+                Box::new(first_parent_controller::create(
+                    base_controller,
+                    &*permanent_graph_info,
+                    matching_commit_ids.as_ref(),
+                    visible_heads_ids.as_ref(),
+                ))
+            }
+            Options::LinearBek => {
+                let base_controller: Box<dyn LinearGraphController> =
+                    Box::new(LinearBekController::new(
+                        SortedBaseController::new(&*permanent_graph_info, self.bek_int_map()),
+                        permanent_graph_info.clone(),
+                    ));
+                if let Some(matching_commit_ids) = &matching_commit_ids {
+                    return Box::new(FilteredController::create(
+                        base_controller,
+                        &*permanent_graph_info,
+                        matching_commit_ids,
+                        visible_heads_ids.as_ref(),
                     ));
                 }
-                if visible_heads.is_some() {
-                    return Err(UnsupportedGraphOptions(
-                        "Filtering by visible branches (CollapsedController)".to_string(),
+                if let Some(visible_heads_ids) = &visible_heads_ids {
+                    return Box::new(BranchFilterController::new(
+                        base_controller,
+                        &*permanent_graph_info,
+                        Some(visible_heads_ids),
                     ));
                 }
-                Ok(Box::new(BaseController::new(self.permanent_graph_info())))
+                base_controller
             }
-            Options::Base(SortType::Bek) => Err(UnsupportedGraphOptions(
-                "SortType.Bek (SortedBaseController)".to_string(),
-            )),
-            Options::FirstParent => Err(UnsupportedGraphOptions(
-                "Options.FirstParent (FirstParentController)".to_string(),
-            )),
-            Options::LinearBek => Err(UnsupportedGraphOptions(
-                "Options.LinearBek (LinearBekController)".to_string(),
-            )),
         }
     }
 
@@ -200,15 +267,15 @@ impl<CommitId: CommitIdBounds> PermanentGraphImpl<CommitId> {
         visible_heads: Option<&HashSet<CommitId>>,
         matching_commits: Option<&HashSet<CommitId>>,
         preprocessor: impl FnOnce(&mut dyn LinearGraphController, &dyn PermanentGraphInfo<CommitId>),
-    ) -> Result<VisibleGraphImpl<CommitId>, UnsupportedGraphOptions> {
+    ) -> VisibleGraphImpl<CommitId> {
         let mut controller =
-            self.create_filtered_controller(options, visible_heads, matching_commits)?;
+            self.create_filtered_controller(options, visible_heads, matching_commits);
         preprocessor(controller.as_mut(), &*self.parts);
-        Ok(VisibleGraphImpl::new(
+        VisibleGraphImpl::new(
             controller,
             self.permanent_graph_info(),
             self.graph_color_getter.clone(),
-        ))
+        )
     }
 }
 
@@ -220,7 +287,7 @@ impl<CommitId: CommitIdBounds> PermanentGraph<CommitId> for PermanentGraphImpl<C
         options: Options,
         heads_of_visible_branches: Option<&HashSet<CommitId>>,
         matched_commits: Option<&HashSet<CommitId>>,
-    ) -> Result<VisibleGraphImpl<CommitId>, UnsupportedGraphOptions> {
+    ) -> VisibleGraphImpl<CommitId> {
         self.create_visible_graph_with(
             options,
             heads_of_visible_branches,
```

`crates/ig-log/src/facade/simple_graph_info.rs`:

```rust
//! A standalone graph of a range of visible rows, with its own layout and commit mapping.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/SimpleGraphInfo.java`.
//!
//! Upstream quirk kept: the branch node ids become node indexes of the visible graph the info was
//! built from, not of the new graph.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;

use crate::api::GraphLayout;
use crate::api::linear_graph::{LinearGraph, LiteLinearGraph, NodeFilter};
use crate::api::permanent::{PermanentCommitsInfo, PermanentGraphInfo, VcsLogGraphNodeId};
use crate::graph_commit::{GraphCommit, GraphCommitImpl};
use crate::permanent::graph_layout_impl::GraphLayoutImpl;
use crate::permanent::permanent_commits_info_impl::RowsMapping;
use crate::permanent::permanent_linear_graph_builder::PermanentLinearGraphBuilder;
use crate::permanent::permanent_linear_graph_impl::PermanentLinearGraphImpl;
use crate::utils::TimestampGetter;
use crate::utils::java_sort;
use crate::utils::linear_graph_utils::{as_lite_linear_graph, convert_ids_to_node_indexes};

pub struct SimpleGraphInfo<CommitId> {
    linear_graph: Arc<PermanentLinearGraphImpl>,
    graph_layout: GraphLayoutImpl,
    branch_node_ids: HashSet<VcsLogGraphNodeId>,
    commits_info: SimpleCommitsInfo<CommitId>,
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> SimpleGraphInfo<CommitId> {
    /// The rows `visible_row - visible_range..` of `linear_graph` (no more than
    /// `2 * visible_range` of them).
    pub fn build(
        linear_graph: &dyn LinearGraph,
        old_layout: &dyn GraphLayout,
        permanent_commits_info: &dyn PermanentCommitsInfo<CommitId>,
        permanent_graph_size: i32,
        branch_node_ids: &HashSet<VcsLogGraphNodeId>,
        visible_row: i32,
        visible_range: i32,
    ) -> SimpleGraphInfo<CommitId> {
        let start = 0.max(visible_row - visible_range);
        // no more than 2*visibleRange commits
        let end = linear_graph.nodes_count().min(start + 2 * visible_range);
        let lite = as_lite_linear_graph(linear_graph);

        let mut rows_mapping = RowsMapping::new((end - start).max(0) as usize);
        let mut graph_commits = Vec::with_capacity((end - start).max(0) as usize);
        for row in start..end {
            let node_id = linear_graph.get_node_id(row);
            let commit = permanent_commits_info.get_commit_id(node_id);
            let parents: Vec<CommitId> = lite
                .get_nodes(row, NodeFilter::Down)
                .into_iter()
                .filter(|&row1| row1 >= start && row1 < end)
                .map(|row1| permanent_commits_info.get_commit_id(linear_graph.get_node_id(row1)))
                .collect();
            let timestamp = permanent_commits_info.get_timestamp(node_id);
            rows_mapping.add(commit.clone(), timestamp);
            graph_commits.push(GraphCommitImpl::create_commit(commit, parents, timestamp));
        }

        let new_linear_graph = PermanentLinearGraphBuilder::new_instance(&graph_commits).build();
        let new_lite = as_lite_linear_graph(&new_linear_graph);

        let mut layout_indexes = vec![0; graph_commits.len()];
        let mut head_node_indexes = Vec::new();

        let commit_id_to_integer =
            reverse_commit_id_map(permanent_commits_info, permanent_graph_size);
        for row in start..end {
            let commit_id = graph_commits[(row - start) as usize].id();
            // fastutil's getInt returns 0 for a missing key
            let layout_index = old_layout
                .get_layout_index(commit_id_to_integer.get(commit_id).copied().unwrap_or(0));
            layout_indexes[(row - start) as usize] = layout_index;
            if new_lite.get_nodes(row - start, NodeFilter::Up).is_empty() {
                head_node_indexes.push(row - start);
            }
        }

        java_sort::sort_by(&mut head_node_indexes, |&a, &b| {
            layout_indexes[a as usize].cmp(&layout_indexes[b as usize])
        });
        let new_layout = GraphLayoutImpl::new(layout_indexes, head_node_indexes);

        let nodes_count = new_linear_graph.nodes_count();
        SimpleGraphInfo {
            linear_graph: Arc::new(new_linear_graph),
            graph_layout: new_layout,
            branch_node_ids: convert_ids_to_node_indexes(
                linear_graph,
                branch_node_ids.iter().copied(),
            ),
            commits_info: SimpleCommitsInfo {
                rows_mapping,
                nodes_count,
            },
        }
    }
}

fn reverse_commit_id_map<CommitId: Clone + Eq + Hash>(
    permanent_commits_info: &dyn PermanentCommitsInfo<CommitId>,
    size: i32,
) -> HashMap<CommitId, i32> {
    (0..size)
        .map(|i| (permanent_commits_info.get_commit_id(i), i))
        .collect()
}

/// The `PermanentCommitsInfo` of a [`SimpleGraphInfo`]: rows of the new graph.
struct SimpleCommitsInfo<CommitId> {
    rows_mapping: RowsMapping<CommitId>,
    nodes_count: i32,
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> PermanentCommitsInfo<CommitId>
    for SimpleCommitsInfo<CommitId>
{
    fn get_commit_id(&self, node_id: VcsLogGraphNodeId) -> CommitId {
        self.rows_mapping.get_commit_id(node_id)
    }

    fn try_get_commit_id(&self, node_id: VcsLogGraphNodeId) -> Option<CommitId> {
        usize::try_from(node_id)
            .ok()
            .and_then(|row| self.rows_mapping.commit_id_mapping().get(row).cloned())
    }

    fn get_timestamp(&self, node_id: VcsLogGraphNodeId) -> i64 {
        self.rows_mapping.get_timestamp(node_id)
    }

    fn get_node_id(&self, commit_id: &CommitId) -> VcsLogGraphNodeId {
        (0..self.nodes_count)
            .find(|&id| self.rows_mapping.get_commit_id(id) == *commit_id)
            .unwrap_or(-1)
    }

    fn convert_to_node_ids(&self, commit_ids: &HashSet<CommitId>) -> HashSet<VcsLogGraphNodeId> {
        (0..self.nodes_count)
            .filter(|&id| commit_ids.contains(&self.rows_mapping.get_commit_id(id)))
            .collect()
    }
}

impl<CommitId: Clone + Eq + Hash + Send + Sync> PermanentGraphInfo<CommitId>
    for SimpleGraphInfo<CommitId>
{
    fn permanent_commits_info(&self) -> &dyn PermanentCommitsInfo<CommitId> {
        &self.commits_info
    }

    fn linear_graph(&self) -> Arc<dyn LinearGraph> {
        self.linear_graph.clone()
    }

    fn permanent_graph_layout(&self) -> &dyn GraphLayout {
        &self.graph_layout
    }

    fn branch_node_ids(&self) -> &HashSet<VcsLogGraphNodeId> {
        &self.branch_node_ids
    }
}
```

`crates/ig-log/src/facade/visible_graph_impl.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/facade/visible_graph_impl.rs b/crates/ig-log/src/facade/visible_graph_impl.rs
index 1b38efe..bf39d21 100644
--- a/crates/ig-log/src/facade/visible_graph_impl.rs
+++ b/crates/ig-log/src/facade/visible_graph_impl.rs
@@ -6,11 +6,8 @@
 //! - `VisibleGraphImpl` implements `ActionController` itself (upstream: an inner class).
 //! - The graph updater of an answer is run with [`VisibleGraph::run_graph_updater`], which then
 //!   rebuilds the print element generator (upstream wraps the `Runnable`).
-//! - `BUTTON_COLLAPSE` / `BUTTON_EXPAND` are reported unsupported until `CollapsedController`
-//!   is ported (Plan 2b); upstream supports them on every non-filtered graph.
 //! - An action on a stale print element whose node indexes are out of this graph's range (e.g. a
 //!   hover racing a log refresh) is performed without an affected element; upstream throws.
-//! - `buildSimpleGraphInfo` (`SimpleGraphInfo`) is not ported yet.
 
 use std::collections::HashSet;
 use std::fmt;
@@ -23,6 +20,7 @@ use crate::api::permanent::{PermanentGraphInfo, VcsLogGraphNodeId};
 use crate::api::printer::{GraphColorGetter, GraphPrintElement, PrintElementGenerator};
 use crate::facade::linear_graph_controller::{LinearGraphAnswer, LinearGraphController};
 use crate::facade::print_element_presentation_manager_impl::PrintElementPresentationManagerImpl;
+use crate::facade::simple_graph_info::SimpleGraphInfo;
 use crate::print::graph_element_comparator_by_layout_index::GraphElementComparatorByLayoutIndex;
 use crate::print::print_element_generator_impl::PrintElementGeneratorImpl;
 use crate::print_element::{EdgePrintElement, EdgePrintElementType};
@@ -101,6 +99,26 @@ impl<CommitId: 'static> VisibleGraphImpl<CommitId> {
         );
     }
 
+    /// A standalone copy of the rows around `visible_row` (at most `2 * visible_range` rows).
+    pub fn build_simple_graph_info(
+        &self,
+        visible_row: VcsLogVisibleGraphIndex,
+        visible_range: i32,
+    ) -> SimpleGraphInfo<CommitId>
+    where
+        CommitId: Clone + Eq + std::hash::Hash + Send + Sync,
+    {
+        SimpleGraphInfo::build(
+            &*self.graph_controller.compiled_graph(),
+            self.permanent_graph.permanent_graph_layout(),
+            self.permanent_graph.permanent_commits_info(),
+            self.permanent_graph.linear_graph().nodes_count(),
+            self.permanent_graph.branch_node_ids(),
+            visible_row,
+            visible_range,
+        )
+    }
+
     /// The controller's compiled graph (`linearGraph`).
     pub fn linear_graph(&self) -> Arc<dyn LinearGraph> {
         self.graph_controller.compiled_graph()
@@ -238,11 +256,14 @@ impl<CommitId: 'static> ActionController<CommitId> for VisibleGraphImpl<CommitId
         self.update_print_element_generator();
     }
 
+    /// Collapse and expand are supported unless the graph is filtered.
     fn is_action_supported(&self, action: &GraphAction) -> bool {
-        !matches!(
-            action.action_type,
-            GraphActionType::ButtonCollapse | GraphActionType::ButtonExpand
-        )
+        match action.action_type {
+            GraphActionType::ButtonCollapse | GraphActionType::ButtonExpand => {
+                self.graph_controller.supports_collapse()
+            }
+            _ => true,
+        }
     }
 }
 
```

`crates/ig-log/src/lib.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/lib.rs b/crates/ig-log/src/lib.rs
index f327f3b..002dbbd 100644
--- a/crates/ig-log/src/lib.rs
+++ b/crates/ig-log/src/lib.rs
@@ -11,8 +11,9 @@
 //! commits have negative ids. Upstream exceptions are panics; upstream `assert` statements (disabled
 //! in production) are `debug_assert!`s.
 //!
-//! Scope (Plan 2): building the permanent graph and showing it unfiltered with `SortType::Normal`.
-//! BEK sorting, linear BEK, first-parent, branch and text filters and collapsing are Plan 2b.
+//! Scope: the whole graph module. Every option of `PermanentGraph::create_visible_graph` (`Normal`
+//! and BEK sorting, linear BEK, first-parent) with branch and commit filters, collapsing and
+//! fragment hover.
 //!
 //! # Example
 //!
@@ -35,9 +36,7 @@
 //!     |a: &String, b: &String| a.cmp(b),
 //!     &HashSet::new(),
 //! );
-//! let visible = permanent
-//!     .create_visible_graph(Options::DEFAULT, None, None)
-//!     .unwrap();
+//! let visible = permanent.create_visible_graph(Options::DEFAULT, None, None);
 //!
 //! let mut ids = Vec::new();
 //! for row in 0..visible.visible_commit_count() {
@@ -69,7 +68,7 @@ pub use facade::permanent_graph_impl::PermanentGraphImpl;
 pub use facade::visible_graph_impl::VisibleGraphImpl;
 pub use graph_color_manager::GraphColorManager;
 pub use graph_commit::{GraphCommit, GraphCommitImpl};
-pub use permanent_graph::{Options, PermanentGraph, SortType, UnsupportedGraphOptions};
+pub use permanent_graph::{Options, PermanentGraph, SortType};
 pub use print_element::{EdgePrintElement, NodePrintElement, PrintElement};
 pub use visible_graph::{RowInfo, RowType, VisibleGraph};
 
```

`crates/ig-log/src/linear_bek/linear_bek_controller.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/linear_bek/linear_bek_controller.rs b/crates/ig-log/src/linear_bek/linear_bek_controller.rs
index 4f18130..357939d 100644
--- a/crates/ig-log/src/linear_bek/linear_bek_controller.rs
+++ b/crates/ig-log/src/linear_bek/linear_bek_controller.rs
@@ -10,9 +10,8 @@
 //!   a list (upstream: a set of objects without `equals`, i.e. also a list).
 //! - The creation time is not logged.
 //!
-//! Upstream quirk kept: `iterateMerges`' search limit never stops it (see
-//! [`BfsSearch`]), so it walks up to the first merge that
-//! answers.
+//! Upstream quirk kept: `iterateMerges`' search limit never stops it (see [`BfsSearch`]), so it
+//! walks up to the first merge that answers.
 
 use std::collections::{BTreeSet, HashSet, VecDeque};
 use std::sync::Arc;
```

`crates/ig-log/src/permanent_graph.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/permanent_graph.rs b/crates/ig-log/src/permanent_graph.rs
index 9eaab93..7f0c4af 100644
--- a/crates/ig-log/src/permanent_graph.rs
+++ b/crates/ig-log/src/permanent_graph.rs
@@ -2,9 +2,8 @@
 //!
 //! Ported from `platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/PermanentGraph.kt`.
 //!
-//! Deviations: `create_visible_graph` returns `Result`, failing with [`UnsupportedGraphOptions`]
-//! for options that are not ported yet (Plan 2b); `all_commits` returns a lazy [`AllCommits`]
-//! view; `get_contained_in_branch_condition` returns a [`ContainedInBranchCondition`].
+//! Deviations: `all_commits` returns a lazy [`AllCommits`] view;
+//! `get_contained_in_branch_condition` returns a [`ContainedInBranchCondition`].
 
 use std::collections::HashSet;
 use std::fmt;
@@ -57,11 +56,6 @@ impl Options {
     pub const DEFAULT: Options = Options::Base(SortType::Normal);
 }
 
-/// `create_visible_graph` was asked for something this port does not support yet.
-#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
-#[error("{0} is not yet supported by ig-log (Plan 2b)")]
-pub struct UnsupportedGraphOptions(pub String);
-
 /// Lazy list of all commits of a permanent graph, with parents read back from the graph.
 pub struct AllCommits<'a, Id> {
     len: usize,
@@ -124,13 +118,14 @@ pub trait PermanentGraph<Id> {
     type Visible: VisibleGraph<Id> + Send;
 
     /// Creates a visible graph. `heads_of_visible_branches == None` means all branches are visible;
-    /// `matched_commits == None` means all commits are visible.
+    /// `matched_commits == None` means all commits are visible. Ids that are not in the graph are
+    /// skipped.
     fn create_visible_graph(
         &self,
         options: Options,
         heads_of_visible_branches: Option<&HashSet<Id>>,
         matched_commits: Option<&HashSet<Id>>,
-    ) -> Result<Self::Visible, UnsupportedGraphOptions>;
+    ) -> Self::Visible;
 
     /// `createVisibleGraph(sortType, ...)`: same as `Options::Base(sort_type)`.
     fn create_visible_graph_sorted(
@@ -138,7 +133,7 @@ pub trait PermanentGraph<Id> {
         sort_type: SortType,
         heads_of_visible_branches: Option<&HashSet<Id>>,
         matched_commits: Option<&HashSet<Id>>,
-    ) -> Result<Self::Visible, UnsupportedGraphOptions> {
+    ) -> Self::Visible {
         self.create_visible_graph(
             Options::Base(sort_type),
             heads_of_visible_branches,
```

- [ ] **Step 4: Run the new tests and watch them pass**

```bash
cargo test -q --locked -p ig-git --test log_perf --test log_refresh
cargo test -q --locked -p ig-log --test graph_options --test permanent_graph --test permanent_graph_behaviour --test print_element_generator_extra
```

Expected: PASS.

- [ ] **Step 5: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code 1ec0a3d -- crates/ig-git/tests/log_perf.rs crates/ig-git/tests/log_refresh.rs crates/ig-log/tests/graph_options.rs crates/ig-log/tests/permanent_graph.rs crates/ig-log/tests/permanent_graph_behaviour.rs crates/ig-log/tests/print_element_generator_extra.rs crates/ig-app/src/log/worker.rs crates/ig-log/src/facade/mod.rs crates/ig-log/src/facade/permanent_graph_impl.rs crates/ig-log/src/facade/simple_graph_info.rs crates/ig-log/src/facade/visible_graph_impl.rs crates/ig-log/src/lib.rs crates/ig-log/src/linear_bek/linear_bek_controller.rs crates/ig-log/src/permanent_graph.rs
```

Expected: no output.

- [ ] **Step 6: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 650 passed, 3 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m 'feat(ig-log): every visible graph option (PermanentGraphImpl controller chains); create_visible_graph is infallible'
```

---

### Task 7: App: fragment hover; a click on a node collapses its fragment

**Files:**
- Modify: `crates/ig-app/tests/ui.rs`, `crates/ig-app/src/log/graph_painter.rs`, `crates/ig-app/src/log/model.rs`

**Source commit:** `61fd29a` on `wip/graph-extras-prototype`.

**Review focus:** the after-click refresh (new generation, hover cleared, selection kept by commit, as upstream's `handleGraphAnswer`) and the new `GraphGeometry::node_center` helper.

Hovering a node highlights its linear fragment, and a click collapses or expands it, as in IntelliJ.

- [ ] **Step 1: Copy the tests first**

```bash
git checkout 61fd29a -- crates/ig-app/tests/ui.rs
```

`crates/ig-app/tests/ui.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-app/tests/ui.rs b/crates/ig-app/tests/ui.rs
index 14fa1f6..bdb19c7 100644
--- a/crates/ig-app/tests/ui.rs
+++ b/crates/ig-app/tests/ui.rs
@@ -1,5 +1,6 @@
 //! UI tests with GPUI's `TestAppContext` (headless, no GPU): the Log loads, selects and scrolls,
-//! an arrow click jumps, Page Down pages by the visible rows, Refresh reaches the worker, the Log
+//! an arrow click jumps, hovering a node highlights its fragment and clicking it collapses the
+//! fragment, Page Down pages by the visible rows, Refresh reaches the worker, the Log
 //! fills the main area with focus, the Console tool window shows/hides (removed entirely, its
 //! height kept) and gives focus back to the Log, Cmd/Alt+9 focuses the Log, Open Recent opens by
 //! path, and a folder opens in one window only. Every git (the tests' and the app's) runs with an
@@ -24,6 +25,7 @@ use ig_app::actions::{
     ActivateConsoleToolWindow, ActivateVersionControlToolWindow, HideActiveWindow,
 };
 use ig_app::app::{AppState, GitEnvironment};
+use ig_app::log::metadata::Lookup;
 use ig_app::log::model::{LogModel, LogState};
 use ig_app::menus::OpenRecent;
 use ig_app::project::ProjectWindow;
@@ -228,6 +230,147 @@ fn arrow_click_jumps_to_the_target_row(cx: &mut TestAppContext) {
     });
 }
 
+/// `main`: c0, c1, c2, a merge of `feature` (f0, f1, f2, forked from c1, then deleted) and c3.
+fn repo_with_a_merge() -> tempfile::TempDir {
+    let dir = tempfile::tempdir().unwrap();
+    let d = dir.path();
+    git(d, &["init", "-q"]);
+    git(d, &["symbolic-ref", "HEAD", "refs/heads/main"]);
+    let commit = |message: &str| git(d, &["commit", "-q", "--allow-empty", "-m", message]);
+    commit("c0");
+    commit("c1");
+    git(d, &["checkout", "-q", "-b", "feature"]);
+    commit("f0");
+    commit("f1");
+    commit("f2");
+    git(d, &["checkout", "-q", "main"]);
+    commit("c2");
+    git(d, &["merge", "-q", "--no-ff", "-m", "merge", "feature"]);
+    git(d, &["branch", "-q", "-D", "feature"]);
+    commit("c3");
+    dir
+}
+
+fn subject(m: &mut LogModel, row: usize) -> String {
+    let (commit, _) = m.row(row).unwrap();
+    match m.metadata(commit) {
+        Lookup::Loaded(meta) => meta.subject.clone(),
+        _ => panic!("no metadata for row {row}"),
+    }
+}
+
+fn row_of(m: &mut LogModel, subject_of_row: &str) -> usize {
+    (0..m.row_count())
+        .find(|&row| subject(m, row) == subject_of_row)
+        .unwrap_or_else(|| panic!("no row {subject_of_row}"))
+}
+
+/// Subjects of the rows whose node is painted selected.
+fn selected_nodes(m: &mut LogModel) -> Vec<String> {
+    use ig_log::print_element::PrintElement;
+    let rows: Vec<usize> = (0..m.row_count())
+        .filter(|&row| {
+            m.row(row)
+                .unwrap()
+                .1
+                .iter()
+                .any(|e| matches!(e, GraphPrintElement::Node(_)) && e.is_selected())
+        })
+        .collect();
+    let mut subjects: Vec<String> = rows.into_iter().map(|row| subject(m, row)).collect();
+    subjects.sort();
+    subjects
+}
+
+#[gpui::test]
+fn hovering_a_node_highlights_its_fragment(cx: &mut TestAppContext) {
+    use ig_app::log::graph_painter::{Color, Shape};
+    use ig_log::print_element::PrintElement;
+
+    let dir = repo_with_a_merge();
+    let model = log_model(cx, dir.path());
+    model.update(cx, |m, _| m.request_metadata(0..m.row_count()));
+    wait(cx, &model, |m| {
+        m.metadata_loaded_for(m.row_count()) && !m.is_busy()
+    });
+
+    let geometry = ig_app::log::graph_painter::GraphGeometry::new(26.0, 2.0);
+    model.update(cx, |m, _| {
+        assert_eq!(m.row_count(), 8);
+        assert!(selected_nodes(m).is_empty());
+
+        // hover the node of f1, in the middle of the merged branch
+        let row = row_of(m, "f1");
+        let elements = m.row(row).unwrap().1;
+        let node = elements
+            .iter()
+            .find(|e| matches!(e, GraphPrintElement::Node(_)))
+            .unwrap();
+        let (x, y) = geometry.node_center(node.position_in_current_row());
+        assert!(m.hover_graph(row, x, y, &geometry));
+        assert!(m.hand_cursor);
+
+        // the linear fragment f2 - f1 - f0 is selected, nodes and the edges between them
+        assert_eq!(selected_nodes(m), ["f0", "f1", "f2"]);
+        let elements = m.row(row).unwrap().1;
+        assert!(
+            elements
+                .iter()
+                .any(|e| matches!(e, GraphPrintElement::Edge(_)) && e.is_selected())
+        );
+        // and painted selected: a thick outline in the mark color under the node
+        const MARK: u32 = 0x123456;
+        let shapes = geometry.shapes(&elements, |_| 0xffffff, MARK);
+        assert!(shapes.iter().any(|shape| matches!(
+            shape,
+            Shape::Circle {
+                color: Color::Rgb(MARK),
+                ..
+            } | Shape::Line { color: MARK, .. }
+        )));
+
+        // leaving the graph clears the highlight
+        assert!(m.unhover_graph());
+        assert!(selected_nodes(m).is_empty());
+    });
+}
+
+#[gpui::test]
+fn clicking_a_node_collapses_its_fragment(cx: &mut TestAppContext) {
+    use ig_log::print_element::PrintElement;
+
+    let dir = repo_with_a_merge();
+    let model = log_model(cx, dir.path());
+    model.update(cx, |m, _| m.request_metadata(0..m.row_count()));
+    wait(cx, &model, |m| {
+        m.metadata_loaded_for(m.row_count()) && !m.is_busy()
+    });
+
+    let geometry = ig_app::log::graph_painter::GraphGeometry::new(26.0, 2.0);
+    model.update(cx, |m, _| {
+        let c3 = row_of(m, "c3");
+        m.click_row(c3, false, false);
+        let generation = m.generation;
+        let row = row_of(m, "f1");
+        let node = m
+            .row(row)
+            .unwrap()
+            .1
+            .into_iter()
+            .find(|e| matches!(e, GraphPrintElement::Node(_)))
+            .unwrap();
+        let (x, y) = geometry.node_center(node.position_in_current_row());
+        assert!(m.click_graph(row, x, y, &geometry));
+        // f1 is hidden behind a dotted edge f2 - f0; rows moved, the selection is kept
+        assert_eq!(m.row_count(), 7);
+        assert!(m.generation > generation);
+        assert_eq!(m.selection.len(), 1);
+        let row = row_of(m, "c3");
+        let (c3_commit, _) = m.row(row).unwrap();
+        assert!(m.is_selected(c3_commit));
+    });
+}
+
 fn arrow_row(element: &GraphPrintElement) -> usize {
     use ig_log::print_element::PrintElement;
     element.row_index() as usize
```

- [ ] **Step 2: Run them and watch them fail**

```bash
cargo test -q --locked -p ig-app --test ui
```

Expected: FAIL — `GraphGeometry::node_center` doesn't exist yet, so `tests/ui.rs` fails to compile. The simulation's first error:

```text
error[E0599]: no method named `node_center` found for struct `GraphGeometry` in the current scope
```

- [ ] **Step 3: Copy the code**

```bash
git checkout 61fd29a -- crates/ig-app/src/log/graph_painter.rs crates/ig-app/src/log/model.rs
```

`crates/ig-app/src/log/graph_painter.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-app/src/log/graph_painter.rs b/crates/ig-app/src/log/graph_painter.rs
index 8b7f46d..3735b86 100644
--- a/crates/ig-app/src/log/graph_painter.rs
+++ b/crates/ig-app/src/log/graph_painter.rs
@@ -155,6 +155,12 @@ impl GraphGeometry {
         self.element_width * position as f32 + self.element_center
     }
 
+    /// The center of a node at `position` in a row (row coordinates), where
+    /// [`element_under`](Self::element_under) finds it.
+    pub fn node_center(&self, position: i32) -> (f32, f32) {
+        (self.x(position), self.row_center)
+    }
+
     /// `GraphCommitCellUtil.getGraphWidth`: the width taken by the graph before the text.
     pub fn graph_width(&self, elements: &[GraphPrintElement], recommended_width: i32) -> f32 {
         if elements.is_empty() {
```

`crates/ig-app/src/log/model.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-app/src/log/model.rs b/crates/ig-app/src/log/model.rs
index c61f2a1..597ada5 100644
--- a/crates/ig-app/src/log/model.rs
+++ b/crates/ig-app/src/log/model.rs
@@ -3,8 +3,9 @@
 //!
 //! Graph actions follow `platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/GraphCommitCellController.java`
 //! and `VcsLogGraphTable.handleAnswer`: hovering an element asks the `ActionController` (it
-//! highlights the edge and sets the hand cursor); clicking an arrow jumps to the commit it points
-//! to (select + scroll). Print elements are snapshots, so rows are re-read on every repaint.
+//! highlights the edge, or the linear fragment of a node, and sets the hand cursor); clicking an
+//! arrow jumps to the commit it points to (select + scroll), clicking a node or edge collapses or
+//! expands its fragment. Print elements are snapshots, so rows are re-read on every repaint.
 
 use std::path::PathBuf;
 use std::sync::Arc;
@@ -354,8 +355,9 @@ impl LogModel {
         true
     }
 
-    /// Clicks a print element of the current graph (an arrow jumps to its target). Elements the
-    /// current graph doesn't print (e.g. from an older snapshot) are ignored.
+    /// Clicks a print element of the current graph (an arrow jumps to its target, a node or edge
+    /// collapses or expands its fragment). Elements the current graph doesn't print (e.g. from an
+    /// older snapshot) are ignored.
     pub fn click_element(&mut self, element: GraphPrintElement) {
         use ig_log::print_element::PrintElement as _;
         let current = usize::try_from(element.row_index())
@@ -364,6 +366,12 @@ impl LogModel {
             .is_some_and(|(_, elements)| elements.contains(&element));
         if current {
             self.perform(GraphAction::new(Some(element), GraphActionType::MouseClick));
+            // `GraphCommitCellController.handleGraphAnswer` with `dataCouldChange`: a click may
+            // have collapsed or expanded the graph, so rows may have moved; the selection is kept
+            // by commit, minus the commits no longer shown
+            self.generation += 1;
+            self.hovered_element = None;
+            self.with_rows(|s, rows| s.retain_visible(rows));
         }
     }
 
```

- [ ] **Step 4: Run the new tests and watch them pass**

```bash
cargo test -q --locked -p ig-app --test ui
```

Expected: PASS.

- [ ] **Step 5: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code 61fd29a -- crates/ig-app/tests/ui.rs crates/ig-app/src/log/graph_painter.rs crates/ig-app/src/log/model.rs
```

Expected: no output.

- [ ] **Step 6: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 652 passed, 3 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m 'feat(ig-app): fragment hover in the Log graph; a click on a node collapses its fragment'
```

---

### Task 8: Upstream map and docs

**Files:**
- Modify: `crates/ig-log/src/collapsing/edge_storage_wrapper.rs`, `crates/ig-log/src/facade/linear_graph_wrapper.rs`, `crates/ig-log/src/lib.rs`, `crates/ig-log/src/print/edges_in_row_generator.rs`, `reference/upstream-map.toml`

**Source commit:** `9b5fce5` on `wip/graph-extras-prototype`.

**Review focus:** every new module's upstream paths, tests and deviations in `reference/upstream-map.toml`, and the removed Plan 2 deviations (unsupported options, collapse buttons, no `SimpleGraphInfo`).

- [ ] **Step 1: Copy the files**

```bash
git checkout 9b5fce5 -- crates/ig-log/src/collapsing/edge_storage_wrapper.rs crates/ig-log/src/facade/linear_graph_wrapper.rs crates/ig-log/src/lib.rs crates/ig-log/src/print/edges_in_row_generator.rs reference/upstream-map.toml
```

`crates/ig-log/src/collapsing/edge_storage_wrapper.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/collapsing/edge_storage_wrapper.rs b/crates/ig-log/src/collapsing/edge_storage_wrapper.rs
index b53d2f0..8f97d11 100644
--- a/crates/ig-log/src/collapsing/edge_storage_wrapper.rs
+++ b/crates/ig-log/src/collapsing/edge_storage_wrapper.rs
@@ -44,7 +44,7 @@ impl EdgeStorageWrapper {
     /// Converts with the node ids / indexes of `graph`.
     ///
     /// Captures `Arc<dyn LinearGraph>` clones; a graph that owns its own wrapper (upstream
-    /// `CollapsedGraph`) must pass a `Weak` or index closures to avoid an `Arc` cycle (Plan 2b).
+    /// `CollapsedGraph`) must pass a `Weak` or index closures to avoid an `Arc` cycle.
     pub fn for_graph(
         edge_storage: Arc<RwLock<EdgeStorage>>,
         graph: Arc<dyn LinearGraph>,
```

`crates/ig-log/src/facade/linear_graph_wrapper.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/facade/linear_graph_wrapper.rs b/crates/ig-log/src/facade/linear_graph_wrapper.rs
index 2bedcbe..1a5691c 100644
--- a/crates/ig-log/src/facade/linear_graph_wrapper.rs
+++ b/crates/ig-log/src/facade/linear_graph_wrapper.rs
@@ -1,7 +1,7 @@
 //! A linear graph with extra (dotted) edges added and some edges hidden.
 //!
 //! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphWrapper.kt`.
-//! Used by the filter and first-parent controllers (Plan 2b); not on the unfiltered `Normal` path.
+//! Used by the first-parent controller and by linear BEK.
 
 use std::sync::Arc;
 
```

`crates/ig-log/src/lib.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/lib.rs b/crates/ig-log/src/lib.rs
index 002dbbd..152bcdb 100644
--- a/crates/ig-log/src/lib.rs
+++ b/crates/ig-log/src/lib.rs
@@ -4,8 +4,8 @@
 //!
 //! Layout mirrors upstream packages: the top-level modules are the public `graph-api`
 //! (`com.intellij.vcs.log.graph`), [`api`] is the internal graph API
-//! (`com.intellij.vcs.log.graph.api`), and [`permanent`], [`mod@print`], [`facade`], [`collapsing`]
-//! and [`utils`] are the `impl` packages. Upstream names are kept in snake_case.
+//! (`com.intellij.vcs.log.graph.api`), and [`permanent`], [`mod@print`], [`facade`], [`collapsing`],
+//! [`linear_bek`] and [`utils`] are the `impl` packages. Upstream names are kept in snake_case.
 //!
 //! Conventions: node indexes, node ids and counts are `i32`, like upstream, because not-loaded
 //! commits have negative ids. Upstream exceptions are panics; upstream `assert` statements (disabled
```

`crates/ig-log/src/print/edges_in_row_generator.rs` (changed; diff against the previous task):

```diff
diff --git a/crates/ig-log/src/print/edges_in_row_generator.rs b/crates/ig-log/src/print/edges_in_row_generator.rs
index 1ccaae0..eb10ec0 100644
--- a/crates/ig-log/src/print/edges_in_row_generator.rs
+++ b/crates/ig-log/src/print/edges_in_row_generator.rs
@@ -12,8 +12,8 @@
 //! edge index.
 //!
 //! **Caching:** block results are cached FIFO by insertion age (not LRU). Callers must call
-//! [`EdgesInRowGenerator::invalidate`] after the graph changes; Plan 2b's mutating controllers
-//! must clear this cache and the print element generator's row cache together.
+//! [`EdgesInRowGenerator::invalidate`] after the graph changes. The visible graph instead rebuilds
+//! its print element generator (with both caches) after every action that changes the graph.
 //!
 //! **Thread safety:** `Send` but not `Sync` (the caches live in `RefCell`s).
 
```

`reference/upstream-map.toml` (changed; diff against the previous task):

```diff
diff --git a/reference/upstream-map.toml b/reference/upstream-map.toml
index cd8b8e2..062540e 100644
--- a/reference/upstream-map.toml
+++ b/reference/upstream-map.toml
@@ -123,7 +123,6 @@ deviations = ["Int-specialised commit classes not needed; GraphCommitImpl is gen
 pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
 upstream = ["platform/vcs-log/graph-api/src/com/intellij/vcs/log/graph/PermanentGraph.kt"]
 deviations = [
-  "create_visible_graph returns Result and fails with UnsupportedGraphOptions for Bek, LinearBek, FirstParent and branch/text filters (Plan 2b)",
   "all_commits returns a lazy AllCommits view; the in-branch predicate is a ContainedInBranchCondition",
 ]
 
@@ -158,6 +157,7 @@ upstream = [
 deviations = [
   "AWT Cursor becomes Cursor::{Default, Hand}",
   "The affected element is always a GraphPrintElement (no lookup of foreign PrintElements by equality)",
+  "An affected element whose node indexes are out of the graph's range (a stale element) is dropped from the action instead of throwing",
   "Runnable graph updater becomes GraphUpdater, run through VisibleGraphImpl::run_graph_updater",
 ]
 
@@ -202,12 +202,102 @@ upstream = [
   "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/printer/PrintElementGenerator.java",
   "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/api/printer/PrintElementPresentationManager.java",
 ]
-deviations = ["GraphPrintElement is an enum over the two concrete print element types"]
+deviations = [
+  "GraphPrintElement is an enum over the two concrete print element types",
+  "GraphPrintElement::with_graph_element replaces CascadeController.convertPrintElement's delegating object",
+]
 
 [modules."ig-log::utils"]
 pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
 upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/TimestampGetter.java"]
 
+[modules."ig-log::utils::bfs_util"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/BfsUtil.kt"]
+tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/BfsTests.kt"]
+deviations = [
+  "Visited flags are passed to step/walk (GraphUtil.subgraphDifference's flags read another walk's queue, which Rust borrows only allow per step)",
+  "The overridable queue is a BfsQueue type parameter: VecDeque (FIFO) or ReversePriorityQueue (PriorityQueue(reverseOrder()))",
+  "BfsSearch holds a BfsWalk instead of subclassing it",
+  "Upstream quirk kept: BfsSearch's count only grows when a node is found, so the limit never stops the search",
+]
+
+[modules."ig-log::utils::graph_util"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/GraphUtil.kt"]
+tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/GraphUtilTests.kt"]
+deviations = [
+  "Bug fix: getReachableNodes(null) of an empty graph returns an empty set; upstream calls UnsignedBitSet.set(0, -1), which throws, so an unfiltered CollapsedController cannot be built for an empty repository (tests: utils_extra::reachable_nodes_of_an_empty_graph, graph_options::an_empty_graph_works_with_every_option)",
+  "The LiteLinearGraph / LinearGraph exclusiveNodes overloads are lite_exclusive_nodes / exclusive_nodes; IntSet is HashSet<i32>",
+]
+
+[modules."ig-log::utils::int_hash_set_flags"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntHashSetFlags.kt"]
+
+[modules."ig-log::utils::int_list"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntList.java"]
+deviations = ["Unused upstream (the graph module's IntLists are fastutil's); kept for the file-to-file mapping"]
+
+[modules."ig-log::utils::int_to_int_map"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = [
+  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/IntToIntMap.java",
+  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/UpdatableIntToIntMap.java",
+  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/AbstractIntToIntMap.java",
+  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/IDIntToIntMap.java",
+]
+deviations = [
+  "AbstractIntToIntMap is a set of functions (abstract_get_short_index, check_*) that implementations call",
+  "IDIntToIntMap.EMPTY is IdIntToIntMap::EMPTY, which implements UpdatableIntToIntMap with a no-op update",
+]
+
+[modules."ig-log::utils::list_int_to_int_map"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/ListIntToIntMap.java"]
+tests = [
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/ListIntToIntMapTest.java",
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/UpdatableIntToIntMapTest.java",
+]
+deviations = [
+  "Owns its Flags and exposes them (flags / flags_mut); upstream keeps a reference to flags its owner mutates",
+  "new_instance always returns a ListIntToIntMap; for long_size 0 it behaves like IDIntToIntMap.EMPTY (which upstream returns)",
+]
+
+[modules."ig-log::utils::permanent_list_int_to_int_map"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/PermanentListIntToIntMap.java"]
+tests = [
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/PermanentListIntToIntMapTest.java",
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/UpdatableIntToIntMapTest.java",
+]
+deviations = ["new_instance always returns a PermanentListIntToIntMap; for short_size 0 it behaves like upstream's createEmptyIntToIntMap"]
+
+[modules."ig-log::utils::predicate_flags"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/PredicateFlags.java"]
+
+[modules."ig-log::utils::tree_int_to_int_map"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/impl/TreeIntToIntMap.java"]
+tests = [
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/TreeIntToIntMapTest.java",
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/UpdatableIntToIntMapTest.java",
+]
+deviations = ["new_instance always returns a TreeIntToIntMap; for long_size 0 it behaves like IDIntToIntMap.EMPTY (which upstream returns)"]
+
+[modules."ig-log::utils::unsigned_bit_set"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/utils/UnsignedBitSet.java"]
+tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/UnsignedBitSetTest.java"]
+deviations = ["java.util.BitSet becomes JavaBitSet (the subset used: get, set, set range)"]
+
+[modules."ig-log::utils::java_priority_queue"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = []
+deviations = ["IdeaGit helper replacing java.util.PriorityQueue with a comparator: the same binary heap sift order, so elements that tie are polled in Java's order (linear BEK depends on it)"]
+
 [modules."ig-log::utils::flags"]
 pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
 upstream = [
@@ -355,6 +445,67 @@ deviations = [
   "In-row edges arrive in BTreeSet order instead of Java HashSet order; the edge comparator is not a strict weak order, so ties can resolve differently from IntelliJ in rare rows (mostly rows with not-loaded commit edges) and positions there can differ",
 ]
 
+[modules."ig-log::collapsing::collapsed_graph"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/CollapsedGraph.java"]
+tests = [
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/DottedFilterEdgesGeneratorTest.kt",
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/InplaceFilterEdgesGeneratorTest.kt",
+]
+deviations = [
+  "State (visibility, node map, edge storage) is shared through Arc<RwLock<_>> by the graph, its compiled graph and its Modifications, so a graph updater (Send + 'static) applies a modification in place (Plan 2 mutation model)",
+  "assertNotUnderModification is replaced by the lock: Modification::apply holds the write lock",
+  "A Modification dropped without apply ends the modification (upstream would refuse every later one, e.g. after an unrun updater)",
+  "updateInstance copies the visibility (upstream shares it with the dropped old graph)",
+  "The compiled graph's edge storage wrapper takes the read lock per call, so no lock is held while another is taken",
+]
+
+[modules."ig-log::collapsing::graph_nodes_visibility"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/GraphNodesVisibility.java"]
+deviations = ["Implements Flags itself (upstream: the asFlags() view) so the node map can own it"]
+
+[modules."ig-log::collapsing::tree_set_node_iterator"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/TreeSetNodeIterator.java"]
+
+[modules."ig-log::collapsing::fragment_generator"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/FragmentGenerator.java"]
+tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/FragmentGeneratorTest.kt"]
+
+[modules."ig-log::collapsing::linear_fragment_generator"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/LinearFragmentGenerator.java"]
+deviations = ["getFragment's gray nodes are a BTreeSet (ascending; Java's HashSet<Integer> order for small indexes); the fragment found does not depend on the order"]
+
+[modules."ig-log::collapsing::dotted_filter_edges_generator"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/DottedFilterEdgesGenerator.kt"]
+tests = [
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/DottedFilterEdgesGeneratorTest.kt",
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/InplaceFilterEdgesGeneratorTest.kt",
+]
+deviations = [
+  "The abstract class is DottedFilterEdgesGenerator over a DottedFilterEdgesCallbacks implementation; update / hideInplace are free functions",
+  "Upstream quirk kept: hasDottedEdges returns false for the down direction even when it finds a down dotted edge; every caller updates the whole graph, where it does not matter",
+  "Upstream test kept ignored as upstream: DottedFilterEdgesGeneratorTest.anotherRoot (there has to be a 0-6 dotted edge, but there is none)",
+]
+
+[modules."ig-log::collapsing::collapsed_action_manager"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/CollapsedActionManager.java"]
+deviations = ["Action cases are functions tried in upstream's order; expand_nodes takes an ordered set (upstream: HashSet)"]
+
+[modules."ig-log::collapsing::collapsed_controller"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/CollapsedController.java"]
+deviations = ["Upstream quirk kept: convertToDelegate checks isMyCollapsedEdge with compiled node indexes (the storage is keyed by delegate indexes); delegate actions on edges do nothing in every chain with this controller"]
+
+[modules."ig-log::collapsing::branch_filter_controller"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/BranchFilterController.kt"]
+
 [modules."ig-log::collapsing::edge_storage"]
 pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
 upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/EdgeStorage.java"]
@@ -370,6 +521,42 @@ deviations = [
   "get_edges returns a BTreeSet (deterministic order; Java's HashSet order is deterministic, Rust's is not)",
 ]
 
+[modules."ig-log::linear_bek::linear_bek_graph"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/linearBek/LinearBekGraph.java"]
+tests = [
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/LinearBekExpandTest.kt",
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/TestUtils.kt",
+]
+deviations = [
+  "LinearBekGraph embeds a LinearGraphWrapper (upstream: subclass); WorkingLinearBekGraph embeds a LinearBekGraph",
+  "Edge sets are BTreeSets (deterministic order; upstream HashSet)",
+]
+
+[modules."ig-log::linear_bek::linear_bek_graph_builder"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/linearBek/LinearBekGraphBuilder.java"]
+tests = [
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/LinearBekTest.kt",
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/LinearBekExpandTest.kt",
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/TestUtils.kt",
+]
+deviations = [
+  "The edge and magic-set queues are JavaPriorityQueues (Java's tie order)",
+  "GraphEdgeComparator orders edges without a down node by a deterministic hash (upstream's includes the enum's identity hash); such edges never reach the queue",
+  "MergeFragment.collapse visits tail edges and tails in ascending order (upstream: fastutil hash order); the result does not depend on it",
+]
+
+[modules."ig-log::linear_bek::linear_bek_controller"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/linearBek/LinearBekController.kt"]
+deviations = [
+  "The graph builder is created per use (upstream keeps one referencing the compiled graph); BekGraphLayout precomputes its head nodes (get_head_node_index returns a slice)",
+  "collectChildFragments' work list is ordered by insertion with tails and body added ascending (upstream: LinkedHashSet fed from a fastutil set)",
+  "The creation time is not logged",
+  "Upstream quirk kept: iterateMerges' BfsSearch limit never stops it, so hover/click walk up to the first merge that answers",
+]
+
 [modules."ig-log::facade::base_controller"]
 pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
 upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/BaseController.java"]
@@ -385,12 +572,82 @@ deviations = ["Interfaces and *Impl classes collapse into structs; edges_replace
 [modules."ig-log::facade::linear_graph_controller"]
 pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
 upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphController.kt"]
-deviations = ["compiled_graph returns an Arc handle; LinearGraphAction is GraphAction"]
+deviations = [
+  "compiled_graph returns an Arc handle; LinearGraphAction is GraphAction",
+  "Upstream's downcasts are default methods: delegate / delegate_mut / delegate_graph_changed (is CascadeController), as_any_mut (is FilteredController), supports_collapse (VisibleGraphImpl.isActionSupported)",
+]
 
 [modules."ig-log::facade::linear_graph_wrapper"]
 pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
 upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/LinearGraphWrapper.kt"]
 
+[modules."ig-log::facade::sort::sort_index_map"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/SortIndexMap.java"]
+
+[modules."ig-log::facade::sort::sort_checker"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/SortChecker.java"]
+deviations = ["LOG.error (fails tests, logs in production) is a debug_assert!"]
+
+[modules."ig-log::facade::sort::sorted_base_controller"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/SortedBaseController.java"]
+tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/BekTest.kt"]
+
+[modules."ig-log::facade::sort::bek"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = [
+  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekBranch.java",
+  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekBranchCreator.java",
+  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekBranchMerger.java",
+  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekEdgeRestrictions.java",
+  "platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekSorter.java",
+]
+tests = [
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/BekTest.kt",
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/TestUtils.kt",
+]
+deviations = [
+  "LOG.assertTrue checks are debug_assert!s; a merger step without an insertable part panics (upstream: NullPointerException)",
+  "BekBranch's prepared part is a range into its node list (upstream: a subList view)",
+]
+
+[modules."ig-log::facade::cascade_controller"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/CascadeController.kt"]
+deviations = [
+  "The abstract class is the CascadeController trait (performAction, convertToDelegate) plus LinearGraphController's delegate / delegate_mut / delegate_graph_changed default methods; perform_linear_graph_action is the template method",
+  "permanentGraphInfo is kept only by the controllers that use it",
+  "convertPrintElement is GraphPrintElement::with_graph_element",
+]
+
+[modules."ig-log::facade::filtered_controller"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/FilteredController.kt"]
+deviations = [
+  "Takes the built CollapsedGraph (upstream: a builder lambda called right away)",
+  "delegateGraphChanged accepts any answer without changes or updater (upstream: identity with DEFAULT_GRAPH_ANSWER); other answers panic like upstream's UnsupportedOperationException, unreachable since a filter never passes actions on and is never below another controller",
+]
+
+[modules."ig-log::facade::first_parent_controller"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/FirstParentController.kt"]
+tests = ["platform/vcs-log/graph/test/com/intellij/vcs/log/graph/impl/facade/FirstParentTest.kt"]
+
+[modules."ig-log::facade::graph_modification_util"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/GraphModificationUtil.kt"]
+deviations = ["The CollapsedGraph / Modification extension functions are free functions; `is FilteredController` is LinearGraphController::as_any_mut + downcast"]
+
+[modules."ig-log::facade::simple_graph_info"]
+pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
+upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/SimpleGraphInfo.java"]
+deviations = [
+  "RowsMapping is the one of permanent::permanent_commits_info_impl (no Int specialisation)",
+  "Upstream quirk kept: branch node ids become node indexes of the source visible graph, not of the new graph",
+]
+
 [modules."ig-log::facade::reachable_nodes"]
 pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
 upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/ReachableNodes.kt"]
@@ -408,10 +665,8 @@ upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/P
 pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
 upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/PermanentGraphImpl.kt"]
 deviations = [
-  "Only Options.Base(Normal) without filters; others return UnsupportedGraphOptions (Plan 2b)",
-  "Unfiltered Normal uses BaseController directly instead of CollapsedController over an all-visible CollapsedGraph (same compiled graph; no collapse/expand/fragment hover)",
+  "bekIntMap by lazy is a OnceLock<Arc<SortIndexMap>>, computed by the first Bek or LinearBek visible graph and shared by the later ones",
   "Branch node ids sorted ascending before the layout builder",
-  "No bekIntMap / SortedBaseController yet",
   "get_children returns an empty list for unknown or not-loaded commits (upstream throws)",
 ]
 
@@ -420,10 +675,8 @@ pinned = "b3c1c1958bc81cc96ea882fd4beba72849c1ed98"
 upstream = ["platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/VisibleGraphImpl.kt"]
 deviations = [
   "Implements ActionController itself (upstream inner class)",
-  "BUTTON_COLLAPSE/BUTTON_EXPAND reported unsupported until CollapsedController (Plan 2b)",
-  "buildSimpleGraphInfo / SimpleGraphInfo not ported",
   "Graph updater run via run_graph_updater",
-  "Arrow actions on stale print elements (indices out of range / unknown not-loaded id) are ignored instead of throwing",
+  "Actions on stale print elements (node indexes out of range) are performed without an affected element, and arrows to an unknown not-loaded id are ignored, instead of throwing",
 ]
 
 [modules."ig-log::tests::common"]
@@ -439,6 +692,7 @@ upstream = [
   "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/SimpleCommit.java",
   "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/EdgeNodeCharConverter.java",
   "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/LinearGraphParser.java",
+  "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/linearBek/TestUtils.kt",
 ]
 tests = [
   "platform/vcs-log/graph/test/com/intellij/vcs/log/graph/TestGraphBuilderTest.kt",
@@ -446,7 +700,11 @@ tests = [
   "platform/vcs-log/graph/testData/testGraphBuilder",
   "platform/vcs-log/graph/testData/testParser",
 ]
-deviations = ["Test support code; asTestGraphString and TestPermanentGraphInfo not ported yet (needed by Plan 2b tests)"]
+deviations = [
+  "Test support code; the test_graph! macro accepts the Kotlin graph { } DSL syntax",
+  "TestUtils.kt's runBek / runLinearBek take the built graph (Kotlin builds it twice from the builder lambda)",
+  "TestGraphs.kt is not ported: no upstream test uses it",
+]
 
 # ---- Plan 2c: git log data
 
@@ -871,6 +1129,7 @@ upstream = [
 deviations = [
   "Dash pattern has no phase (GPUI dash arrays)",
   "OUTLINE nodes use the ordinary line thickness",
+  "GraphGeometry::node_center (IdeaGit, for tests) gives the point where element_under finds a node",
 ]
 
 [modules."ig-app::log::labels"]
```

- [ ] **Step 2: Check that the copies are exact**

```bash
git add -A
git diff --cached --exit-code 9b5fce5 -- crates/ig-log/src/collapsing/edge_storage_wrapper.rs crates/ig-log/src/facade/linear_graph_wrapper.rs crates/ig-log/src/lib.rs crates/ig-log/src/print/edges_in_row_generator.rs reference/upstream-map.toml
```

Expected: no output.

- [ ] **Step 3: Verify**

```bash
cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all
```

Expected: all clean; `cargo test` reports 652 passed, 3 ignored. Fix the source, never the expected values of ported tests.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m 'docs: upstream map for the graph extras (BEK, linear BEK, filters, collapsing)'
```

---

