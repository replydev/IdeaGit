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
