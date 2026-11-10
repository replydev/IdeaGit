# IdeaGit — Plan 2b: Graph Extras (Design)

Date: 2026-11-10
Status: Approved in brainstorming; awaiting review of this document.
Parent spec: `2026-11-08-ideagit-rust-design.md` ("Git Log" → Graph). Follows Plan 2 (`docs/superpowers/plans/2026-11-08-plan-2-graph-core.md`), whose "Where this plan fits" and "Notes for later plans" sections define this plan's starting point.

## Goal

Finish the port of upstream's `platform/vcs-log/graph`: BEK ("Standard") and linear BEK ("IntelliJ style") sorting, first-parent, graph-level branch and commit filtering, and collapsing with fragment hover. Every option combination of `PermanentGraph.createVisibleGraph` then works as upstream.

This plan comes before the read-only Log UI plan (details, changes tree, filters, view options), which consumes it. Changes to the repository (checkout, cherry-pick, revert, reset) move to the Branches/operations plan.

## Scope

In scope:
- **`ig-log`:** a faithful port of every not-yet-ported source file under `platform/vcs-log/graph/src` (list below), with upstream tests.
- **`ig-app`:** fragment hover and click. Hovering a commit node highlights its linear fragment, and a click collapses or expands it, as in IntelliJ (decided after the prototype: with `CollapsedController` back on the default path, clicks collapse upstream too). The app already sends graph actions and paints selected print elements (`log/graph_painter.rs`); after a click it refreshes the rows as upstream's `handleGraphAnswer` does (hover cleared, selection kept by commit). Plus UI tests.

Not in scope:
- Sort, first-parent, collapse/expand and long-edges controls. They belong in the Log toolbar's View Options popup (Log UI plan). Upstream's default stays: `Options.Base(SortType.Normal)` ("Off", `PermanentGraph.Options.Default`), so the app's graph does not change.
- Working out which commits match a filter (text, user, date, path). That is `VcsLogFilterer` in the data layer (Log UI plan). The graph only receives the resulting `headsOfVisibleBranches` / `matchedCommits` sets.
- The tag-fetch refresh strategy, file watching, runner cancellation and `Hash` unification (Log UI plan and later).

## Upstream files to port

All paths are relative to `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/` at the pinned commit (`reference/UPSTREAM`).

| Rust module | Upstream |
|---|---|
| `utils` | `BfsUtil.kt`, `GraphUtil.kt`, `IntHashSetFlags.kt`, `IntList.java`, `IntToIntMap.java`, `UpdatableIntToIntMap.java`, `TimestampGetter.java`, `UnsignedBitSet.java`, `impl/AbstractIntToIntMap.java`, `impl/IDIntToIntMap.java`, `impl/ListIntToIntMap.java`, `impl/PermanentListIntToIntMap.java`, `impl/PredicateFlags.java`, `impl/TreeIntToIntMap.java` |
| `facade::sort` | `impl/facade/sort/SortIndexMap.java`, `SortChecker.java`, `SortedBaseController.java`, `bek/BekBranch.java`, `bek/BekBranchCreator.java`, `bek/BekBranchMerger.java`, `bek/BekEdgeRestrictions.java`, `bek/BekSorter.java` |
| `linear_bek` | `linearBek/LinearBekGraph.java`, `LinearBekGraphBuilder.java`, `LinearBekController.kt` |
| `collapsing` | `collapsing/CollapsedGraph.java`, `CollapsedController.java`, `CollapsedActionManager.java`, `FragmentGenerator.java`, `LinearFragmentGenerator.java`, `GraphNodesVisibility.java`, `TreeSetNodeIterator.java`, `DottedFilterEdgesGenerator.kt`, `BranchFilterController.kt` |
| `facade` | `impl/facade/CascadeController.kt`, `FilteredController.kt`, `FirstParentController.kt`, `GraphChangesUtil.java`, `GraphModificationUtil.kt`, `RowsMapping.kt`, `SimpleGraphInfo.java`, and the missing branches of `PermanentGraphImpl.kt` |
| `print` | `impl/print/GraphColorGetterByHead.kt`, `GraphColorGetterByNode.kt`, if these paths use them (the prototype decides; unported ones are listed in the map with a reason) |

Each module is recorded in `reference/upstream-map.toml` with its upstream files, tests and deviations, as in earlier plans.

## Architecture

### Controller chain

`PermanentGraphImpl::create_visible_graph` mirrors upstream's `createFilteredController`:

| Options | Base controller | Wrapped in |
|---|---|---|
| `Base(Normal)` | `BaseController` | `FilteredController` if `matched_commits` is set, otherwise `CollapsedController` (which also applies `heads_of_visible_branches`) |
| `Base(Bek)` | `SortedBaseController` with the BEK map | same as `Base(Normal)` |
| `FirstParent` | `BaseController` | `FirstParentController`, for both filter kinds |
| `LinearBek` | `LinearBekController` over `SortedBaseController` | `FilteredController` if `matched_commits` is set, otherwise `BranchFilterController` if heads are filtered, otherwise nothing |

- **API change.** Every combination is supported, so `UnsupportedGraphOptions` and the `Result` around `create_visible_graph` / `create_visible_graph_with` go away. The app's callers are updated in the same task.
- **Restores Plan 2's deviation.** Unfiltered `Normal` uses `CollapsedController` again, as upstream. Collapse and expand become supported actions, and the "BUTTON_COLLAPSE/BUTTON_EXPAND reported unsupported" deviation is removed from the map.
- **Shared base.** `CascadeController` is the base of the wrapping controllers. Upstream's downcasts become default methods on `LinearGraphController`, as decided in Plan 2's Task 1 design review:
  - `delegate()`, for `performActionRecursively` and `GraphModificationUtil`
  - `as_any_mut()`
  - `supports_collapse()`, for `isActionSupported`'s `!is FilteredController` check

### Mutation model

Updaters stay `FnOnce + Send + 'static`, and `LinearGraph` stays `&self + Send + Sync` (Plan 2's mutation model). Collapsed-graph and linear-BEK state lives behind `Arc<RwLock<_>>` inside the same `Arc<dyn LinearGraph>`, as `EdgeStorageWrapper` already does. An updater mutates that state in place. It never replaces the `Arc`.

### BEK map

Upstream's `private val bekIntMap by lazy { BekSorter.createBekMap(...) }` becomes a `OnceLock<SortIndexMap>` on `PermanentGraphImpl`. It is computed the first time a `Bek` or `LinearBek` visible graph is requested, then shared by every later visible graph over the same permanent graph. Any comparator-based sort uses `utils::java_sort`, as elsewhere in the crate. Upstream comparators are not total orders, and Rust's sort may panic on them.

### Threading

There is no new threading. The log worker builds the visible graph for each snapshot off the UI thread, so the BEK map and linear-BEK graph are built there too. The UI owns the visible graph, which is `Send` but not `Sync`. Fragment hover is a `MouseOver` action on the UI thread. `LinearFragmentGenerator` walks only the hovered fragment, so the action is cheap. A new snapshot drops the highlighted fragment, the same way stale hover elements are dropped today.

## Errors and stale input

- **Unknown commits are skipped.** `heads_of_visible_branches` / `matched_commits` entries that are not in the graph (stale filter, refresh in progress) are skipped, not panicked on. This follows the existing `convert_to_node_ids` behaviour.
- **Stale actions get an empty answer.** Graph actions carrying node or edge indexes from an older visible graph return an empty answer, as today.
- **Unreachable assertions stay panics.** Upstream assertions that the facade or UI cannot reach remain panics. Where upstream would throw on input the UI can supply, the port does not panic, and the difference is recorded as a deviation.
- **Upstream bugs are fixed and recorded.** Real upstream bugs found while porting are fixed and recorded in the module docs and `reference/upstream-map.toml`, each with a test.

## Testing

**Ported upstream tests**, kept faithful (minimal harness changes, byte-identical fixtures, expected values never changed):
- `BfsTests`, `GraphUtilTests`, `UnsignedBitSetTest`
- `ListIntToIntMapTest`, `PermanentListIntToIntMapTest`, `TreeIntToIntMapTest`, `UpdatableIntToIntMapTest` (`BitSetFlagsTest` and `IntIntMultiMapTest` are already ported)
- `DottedFilterEdgesGeneratorTest`, `InplaceFilterEdgesGeneratorTest`, `FragmentGeneratorTest`
- `FirstParentTest`
- `BekTest`, `LinearBekTest`, `LinearBekExpandTest` (with `linearBek/TestUtils.kt`)

The test support code these tests need is ported too: `asTestGraphString`, `TestPermanentGraphInfo`, and whatever `GraphStrUtils` / `TestGraphBuilder` helpers are still missing.

**IdeaGit-only tests**, marked as such:
- **Option combinations.** `create_visible_graph` runs every combination of options × visible heads × matched commits, including unknown ids, on a small graph without panicking.
- **Fragment hover and click.** Headless GPUI UI tests in `crates/ig-app/tests/ui.rs` hover a node in a repository with a merge and check that exactly the fragment's elements are painted selected, and that a click collapses the fragment and keeps the row selection.
- **Performance.** An ignored perf test, next to the existing one, builds `Bek` and `LinearBek` visible graphs on the 50k-commit fixture. Its timings are recorded in the plan. The aim is well under a second, since it runs on the worker.

## Process

Follow the established workflow in `docs/superpowers/CHECKPOINT.md`:
1. A verified prototype in a scratch clone.
2. A plan generated from the prototype, modelled on `docs/superpowers/plangen/gen4.py`.
3. A simulated task order on a fresh clone.
4. Subagent-driven execution on a feature branch, with spec and quality reviews.
5. CI on a draft PR.
6. A final review, then a fast-forward merge.

Unlike Plan 2, tasks are **per module, upstream test first**. The new modules depend only on the existing core, not on each other in cycles, so each task ports one upstream test class with its fixtures, sees it fail, then adds the module group it covers and sees it pass. The tasks run in this order:
1. Utilities.
2. The sort index map and the BEK sorter.
3. The cascade, filtered and first-parent controllers.
4. Collapsing and fragments.
5. Linear BEK.
6. The `PermanentGraphImpl` chain and the API change.
7. App fragment hover.
8. The upstream map.

The final order follows the prototype's actual dependencies.

## Success criteria

- Every option combination works, and `UnsupportedGraphOptions` is gone.
- All ported upstream tests pass with unchanged expected values.
- The workspace stays green: `cargo fmt`, `clippy -D warnings`, docs, and tests on Linux, macOS, Windows and MSRV 1.95.
- Hovering a commit in the running app highlights its fragment, and a click collapses or expands it, as in IntelliJ.
- `reference/upstream-map.toml` lists every new module with its upstream files, tests and deviations.
