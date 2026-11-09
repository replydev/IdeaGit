import os, sys, subprocess
R, OUT = sys.argv[1], sys.argv[2]
C = "crates/ig-log/"
def rd(p): return open(os.path.join(R, p)).read().rstrip("\n") + "\n"
def code(lang, body): return f"```{lang}\n{body}```\n"
def src_files():
    fs=[]
    for d,_,files in os.walk(os.path.join(R,C,"src")):
        for f in files: fs.append(os.path.relpath(os.path.join(d,f), R))
    return sorted(fs)
umap = rd("reference/upstream-map.toml")
umap_log = umap[umap.index('[modules."ig-log::'):]

GROUPS = [
 ("Crate root and public graph API", ["src/lib.rs","src/graph_commit.rs","src/permanent_graph.rs","src/visible_graph.rs","src/print_element.rs","src/graph_color_manager.rs","src/actions.rs"]),
 ("Internal graph API (`api`)", ["src/api/mod.rs","src/api/elements.rs","src/api/linear_graph.rs","src/api/permanent.rs","src/api/printer.rs"]),
 ("Utilities (`utils`)", ["src/utils/mod.rs","src/utils/flags.rs","src/utils/normal_edge.rs","src/utils/int_int_multi_map.rs","src/utils/java_sort.rs","src/utils/bounded_cache.rs","src/utils/linear_graph_utils.rs","src/utils/dfs_util.rs"]),
 ("Permanent graph (`permanent`)", ["src/permanent/mod.rs","src/permanent/duplicate_parent_fixer.rs","src/permanent/permanent_linear_graph_builder.rs","src/permanent/permanent_linear_graph_impl.rs","src/permanent/permanent_commits_info_impl.rs","src/permanent/graph_layout_builder.rs","src/permanent/graph_layout_impl.rs"]),
 ("Printing (`print`)", ["src/print/mod.rs","src/print/elements.rs","src/print/graph_element_comparator_by_layout_index.rs","src/print/graph_color_getter.rs","src/print/edges_in_row_generator.rs","src/print/print_element_generator_impl.rs"]),
 ("Edge storage (`collapsing`)", ["src/collapsing/mod.rs","src/collapsing/edge_storage.rs","src/collapsing/edge_storage_wrapper.rs"]),
 ("Facade (`facade`)", ["src/facade/mod.rs","src/facade/graph_changes.rs","src/facade/linear_graph_controller.rs","src/facade/linear_graph_wrapper.rs","src/facade/base_controller.rs","src/facade/reachable_nodes.rs","src/facade/print_element_presentation_manager_impl.rs","src/facade/permanent_graph_impl.rs","src/facade/visible_graph_impl.rs"]),
]
listed = {C+f for _,fs in GROUPS for f in fs}
missing = [f for f in src_files() if f not in listed]
assert not missing, missing

TESTS = [
 ("test_graph_builder", ["testGraphBuilder","testParser"], 7, True,
  "Test infrastructure: text graph parser/printer and `TestGraphBuilder`",
  "the test infrastructure in `tests/common/` and the graph element types in `src/api/elements.rs`",
  "`TestGraphBuilderTest` (graph and `impl.print` variants)"),
 ("utils", [], 10, False, "Utilities", "`src/utils/*`", "`BitSetFlagsTest`, `IntIntMultiMapTest`"),
 ("graph_builder", ["graphBuilder","graphHashIndex"], 28, False, "Permanent graph builder",
  "`src/permanent/{duplicate_parent_fixer,permanent_linear_graph_builder,permanent_linear_graph_impl,permanent_commits_info_impl}.rs`, `src/graph_commit.rs`",
  "`GraphBuilderTest`, `GraphBuilderHashIndexTest` (String and Integer ids)"),
 ("graph_layout_builder", ["layoutBuilder"], 12, False, "Graph layout",
  "`src/permanent/{graph_layout_builder,graph_layout_impl}.rs`, `src/utils/dfs_util.rs`",
  "`GraphLayoutBuilderTest`, `GraphLayoutBuilderHeadOrderTest`"),
 ("containing_branches", ["containingBranches"], 10, False, "Containing branches",
  "`src/facade/reachable_nodes.rs`", "`ContainingBranchesTest`"),
 ("edges_in_row", ["edgesInRow"], 9, False, "Edges in row",
  "`src/print/edges_in_row_generator.rs`, `src/utils/bounded_cache.rs`", "`EdgesInRowTest`"),
 ("print_element_generator", ["elementGenerator"], 6, False, "Print element generator",
  "`src/print/{print_element_generator_impl,elements,graph_element_comparator_by_layout_index}.rs`, `src/print_element.rs`, `src/api/printer.rs`, `src/utils/java_sort.rs`",
  "`PrintElementGeneratorTest`"),
 ("edge_storage", [], 10, False, "Edge storage",
  "`src/collapsing/*`, `src/facade/linear_graph_wrapper.rs`", "`EdgeStorageTest`"),
 ("permanent_graph", [], 8, False, "Facade: permanent and visible graph",
  "`src/facade/*`, `src/permanent_graph.rs`, `src/visible_graph.rs`, `src/actions.rs`, `src/print/graph_color_getter.rs`",
  "IdeaGit facade tests (upstream has no direct facade test for this path)"),
]

o=[]; w=o.append
w("""# IdeaGit Plan 2 — Commit Graph Core (`ig-log`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the `ig-log` crate: a faithful port of IntelliJ's VCS Log commit graph (`platform/vcs-log/graph` + `graph-api`). It builds the permanent graph, lays it out, and produces per-row print elements for the default `Normal` sort without filters, with the upstream graph tests ported and passing.

**Architecture:** Plain Rust library, no UI dependency, generic over the commit id type. Module layout mirrors upstream packages: the top-level modules are the public `graph-api`, `api` is the internal graph API, and `permanent`, `print`, `facade`, `collapsing` and `utils` are the `impl` packages. Upstream names are kept in snake_case so upstream diffs map file-to-file. Node indexes and ids are `i32` (not-loaded commits have negative ids). Upstream exceptions are panics, and upstream `assert`s are `debug_assert!`s.

**Tech Stack:** Rust 2024, `thiserror`. Tests use upstream text test data, copied byte-identically.

**Spec:** `docs/superpowers/specs/2026-11-08-ideagit-rust-design.md` (Git Log section).

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
- `get_children` panics for commits that are not loaded, as upstream does. `ReachableNodes` serializes queries through a `Mutex`.

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

""" + code("toml", rd(C+"Cargo.toml")) + "\n")
step=3
for title, files in GROUPS:
    w(f"- [ ] **Step {step}: {title}.** Create these files with exactly this content.\n\n")
    for f in files:
        w(f"`crates/ig-log/{f}`:\n\n" + code("rust", rd(C+f)) + "\n")
    step+=1
w(f"""- [ ] **Step {step}: Append the module map.** Append this to `reference/upstream-map.toml`, after the last `ig-git` entry:

""" + code("toml", umap_log) + f"""
- [ ] **Step {step+1}: Build, lint and run the unit tests**

```bash
cargo build -p ig-log
cargo clippy -q --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ig-log
cargo test -p ig-log --lib
```
Expected: no errors or warnings, and `test result: ok. 3 passed` (the `java_sort` unit tests).

- [ ] **Step {step+2}: Commit**

```bash
git add Cargo.toml Cargo.lock .gitattributes reference crates/ig-log
git commit -m "feat(ig-log): commit graph core ported from IntelliJ vcs-log graph"
```

---
""")
n=2
for target, fixtures, count, with_common, title, covers, ported in TESTS:
    files = [f"`crates/ig-log/tests/{target}.rs`"]
    if with_common: files = ["`crates/ig-log/tests/common/{mod,parser,str_utils,test_graph_builder}.rs`"] + files
    if fixtures: files.append(", ".join(f"`crates/ig-log/tests/fixtures/{d}/**`" for d in fixtures))
    w(f"""### Task {n}: {title}

**Files:**
- Create: {", ".join(files)}

Ports {ported}. **Review focus:** {covers}.

""")
    s=1
    if fixtures:
        w(f"- [ ] **Step {s}: Copy the upstream test data, byte-identical**\n\n```bash\nmkdir -p crates/ig-log/tests/fixtures\n" +
          "".join(f'cp -R "$IJ/platform/vcs-log/graph/testData/{d}" crates/ig-log/tests/fixtures/\n' for d in fixtures) +
          "".join(f'diff -r "$IJ/platform/vcs-log/graph/testData/{d}" crates/ig-log/tests/fixtures/{d} && echo "{d} identical"\n' for d in fixtures) +
          "```\n\n"); s+=1
    if with_common:
        for f in ["mod.rs","parser.rs","str_utils.rs","test_graph_builder.rs"]:
            w(f"- [ ] **Step {s}: Create `crates/ig-log/tests/common/{f}`**\n\n" + code("rust", rd(C+"tests/common/"+f)) + "\n"); s+=1
    w(f"- [ ] **Step {s}: Create `crates/ig-log/tests/{target}.rs`**\n\n" + code("rust", rd(C+f"tests/{target}.rs")) + "\n"); s+=1
    name = ported.split("`")[1] if ported.startswith("`") else target
    msg = f"test(ig-log): port {name}" + (" with upstream test data" if fixtures else "")
    w(f"""- [ ] **Step {s}: Run it**

Run: `cargo test -p ig-log --test {target}`
Expected: `test result: ok. {count} passed; 0 failed`. A failure means the port deviates from upstream. Fix the source, never the expected data.

- [ ] **Step {s+1}: Lint and commit**

```bash
cargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings
git add crates/ig-log/tests
git commit -m "{msg}"
```

---
""")
    n+=1
w(f"""### Task {n}: Full verification

- [ ] **Step 1: Run everything**

```bash
cargo fmt --all --check
cargo clippy -q --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps -p ig-git -p ig-log
cargo test --all
for d in $(ls crates/ig-log/tests/fixtures); do diff -rq "$IJ/platform/vcs-log/graph/testData/$d" crates/ig-log/tests/fixtures/$d; done
```
Expected: all clean. `ig-log` runs 3 unit tests plus 7+10+28+12+10+9+6+10+8 integration tests (103 in total), `ig-git` stays at 67, and the fixture diff prints nothing.

- [ ] **Step 2: Add the CI doc build for the new crate.** In `.github/workflows/ci.yml`, change `cargo doc --no-deps -p ig-git` to `cargo doc --no-deps -p ig-git -p ig-log`, then commit:

```bash
git add .github/workflows/ci.yml && git commit -m "ci: build ig-log docs"
```
""")
open(OUT,"w").write("".join(o))
print("lines", sum(1 for _ in open(OUT)))
