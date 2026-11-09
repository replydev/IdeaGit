import os, sys, subprocess
R, OUT = sys.argv[1], sys.argv[2]
def rd(p): return open(os.path.join(R, p)).read().rstrip("\n") + "\n"
def code(lang, body): return f"```{lang}\n{body}```\n"
def ls(d): return sorted(os.path.join(d,f) for f in os.listdir(os.path.join(R,d)) if f.endswith(".rs"))
def files_block(paths):
    return "".join(f"`{p}`:\n\n" + code("rust" if p.endswith(".rs") else "toml", rd(p)) + "\n" for p in paths)
def count(target, pkg):
    out = subprocess.run(["cargo","test","-q","-p",pkg,"--test",target],cwd=R,capture_output=True,text=True).stdout
    return [l.split()[3] for l in out.splitlines() if l.startswith("test result")][0]
umap = rd("reference/upstream-map.toml"); umap_new = umap[umap.index('[modules."ig-log::data') if '[modules."ig-log::data' in umap else 0:]
base_map = subprocess.run(["git","show","HEAD:reference/upstream-map.toml"],cwd=R,capture_output=True,text=True).stdout
lib_git_final = rd("crates/ig-git/src/lib.rs")
lib_git_mid = lib_git_final.replace("pub mod log;\n","")
assert lib_git_mid != lib_git_final

o=[]; w=o.append
w("""# IdeaGit Plan 2c — Git Log Data Layer Implementation Plan

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
""")
tasks=[]
def task(title, files_desc, body, commit, review=None):
    tasks.append((title, files_desc, body, commit, review))

task("ig-log: O(1) node-id lookups",
 "- Modify: `crates/ig-log/src/permanent/permanent_commits_info_impl.rs`",
 "- [ ] **Step 1: Replace the file** with this content. It adds id → node-index maps and makes `get_node_id`, `convert_to_node_ids` and `contains_all` O(1) per id, with results unchanged:\n\n" + files_block(["crates/ig-log/src/permanent/permanent_commits_info_impl.rs"]) +
 "- [ ] **Step 2: Run the existing graph suite**\n\nRun: `cargo test -p ig-log`\nExpected: all existing targets pass, unchanged (145 tests).\n\n",
 "perf(ig-log): O(1) node-id lookups in permanent commits info",
 "`permanent_commits_info_impl.rs`: index maps vs upstream semantics (duplicate ids, not-loaded ids)")

data_files = ["crates/ig-log/src/data/mod.rs"]+[f for f in ls("crates/ig-log/src/data") if not f.endswith("mod.rs")]
task("ig-log: the `data` module",
 "- Create: `crates/ig-log/src/data/*.rs`, `crates/ig-log/tests/common/{timed_commit_parser,test_vcs_log_provider}.rs`\n- Modify: `crates/ig-log/src/lib.rs`, `crates/ig-log/tests/common/mod.rs`",
 "- [ ] **Step 1: Create the data module files**\n\n" + files_block(data_files) +
 "- [ ] **Step 2: Replace `crates/ig-log/src/lib.rs`**\n\n" + files_block(["crates/ig-log/src/lib.rs"]) +
 "- [ ] **Step 3: Test helpers.** Replace `crates/ig-log/tests/common/mod.rs` and create the two helpers:\n\n" + files_block(["crates/ig-log/tests/common/mod.rs","crates/ig-log/tests/common/timed_commit_parser.rs","crates/ig-log/tests/common/test_vcs_log_provider.rs"]) +
 "- [ ] **Step 4: Build and run the unit tests**\n\n```bash\ncargo clippy -q --all-targets -- -D warnings\nRUSTDOCFLAGS=\"-D warnings\" cargo doc --no-deps -p ig-log\ncargo test -p ig-log\n```\nExpected: clean. The `data` unit tests (hash, user, storage, color manager) pass, and all earlier targets still pass.\n\n",
 "feat(ig-log): VCS log data layer (storage, refs, sorter, joiners, graph data, refresher)",
 "crate-level design of `data`: types, storage interning, refresher API and threading")

def ttask(title, pkg, targets, ported, review):
    paths=[f"crates/{pkg}/tests/{t}.rs" for t in targets]
    body=f"Ports {ported}.\n\n- [ ] **Step 1: Create the test file(s)**\n\n" + files_block(paths)
    runs="\n".join(f"cargo test -p {pkg} --test {t}" for t in targets)
    exp=", ".join(f"`{t}`: {count(t,pkg)} passed" + (" (plus 1 ignored)" if t=="log_perf" else "") for t in targets)
    body+=f"- [ ] **Step 2: Run them**\n\n```bash\n{runs}\n```\nExpected: {exp}. Fix the source, never the expectations.\n\n"
    if "log_perf" in targets:
        body+="- [ ] **Step 2b: Run the performance smoke test once (release)**\n\n```bash\ncargo test -p ig-git --release --test log_perf -- --ignored --nocapture\n```\nExpected: it passes. Record the printed timings in the commit message body. The reference machine (macOS arm64, 50k commits) took about 240 ms for `initialize` and about 40 ms for a one-commit `refresh`.\n\n"
    tasks.append((title, "- Create: "+", ".join(f"`{p}`" for p in paths), body, f"test({pkg}): {title.split(': ',1)[-1]}", review))

ttask("ig-log tests: sorter and multi-repo joiner", "ig-log", ["vcs_log_sorter","vcs_log_multi_repo_joiner"],
 "`VcsLogSorterTest` and `VcsLogMultiRepoJoinerTest`", "`data/{sorter,multi_repo_joiner,commit,hash,root}.rs`")
ttask("ig-log tests: refresh joiner", "ig-log", ["vcs_log_joiner"], "`VcsLogJoinerTest`", "`data/joiner.rs`")
ttask("ig-log tests: graph data and refresher", "ig-log", ["vcs_log_graph_data","vcs_log_refresher"],
 "`VcsLogRefresherTest` (4/7) plus IdeaGit graph-data tests", "`data/{storage,refs_model,graph_color_manager_impl,graph_data,refresher,provider}.rs`")

task("ig-git: runner, handler and version additions; natural comparator",
 "- Modify: `crates/ig-git/src/{runner,handler,version,lib}.rs`\n- Create: `crates/ig-git/src/natural_comparator.rs`",
 "These are the runner capabilities the log reader needs: streaming output to a line listener without collecting it, stdin input, and config parameters on handlers. They also add two version specialties and upstream's `NaturalComparator`, the ref-name ordering.\n\n- [ ] **Step 1: Replace or create these files**\n\n" + files_block(["crates/ig-git/src/runner.rs","crates/ig-git/src/handler.rs","crates/ig-git/src/version.rs","crates/ig-git/src/natural_comparator.rs"]) +
 "- [ ] **Step 2: Register the comparator.** Replace `crates/ig-git/src/lib.rs` with this content. The `log` module is added in the next task.\n\n" + code("rust", lib_git_mid) +
 "\n- [ ] **Step 3: Run**\n\n```bash\ncargo clippy -q --all-targets -- -D warnings\ncargo test -p ig-git\n```\nExpected: clean. All existing `ig-git` targets pass, plus the new `natural_comparator` unit tests.\n\n",
 "feat(ig-git): runner line listener and stdin, version specialties, natural comparator",
 "`runner.rs`/`handler.rs` additions (no behavior change for existing callers), `natural_comparator.rs` vs upstream `NaturalComparator`")

log_files=["crates/ig-git/src/log/mod.rs"]+[f for f in ls("crates/ig-git/src/log") if not f.endswith("mod.rs")]
task("ig-git: the `log` module",
 "- Create: `crates/ig-git/src/log/*.rs`, `crates/ig-git/tests/common/log_helpers.rs`\n- Modify: `crates/ig-git/Cargo.toml`, `crates/ig-git/src/lib.rs`, `crates/ig-git/tests/common/mod.rs`",
 "- [ ] **Step 1: Depend on `ig-log`.** Replace `crates/ig-git/Cargo.toml`:\n\n" + files_block(["crates/ig-git/Cargo.toml"]) +
 "- [ ] **Step 2: Create the log module files**\n\n" + files_block(log_files) +
 "- [ ] **Step 3: Replace `crates/ig-git/src/lib.rs`**\n\n" + code("rust", lib_git_final) +
 "\n- [ ] **Step 4: Test helpers.** Replace `crates/ig-git/tests/common/mod.rs` and create `log_helpers.rs`:\n\n" + files_block(["crates/ig-git/tests/common/mod.rs","crates/ig-git/tests/common/log_helpers.rs"]) +
 "- [ ] **Step 5: Build and run the unit tests**\n\n```bash\ncargo clippy -q --all-targets -- -D warnings\nRUSTDOCFLAGS=\"-D warnings\" cargo doc --no-deps --all\ncargo test -p ig-git\n```\nExpected: clean. The parser, record and unescape unit tests pass, and all earlier targets still pass.\n\n",
 "feat(ig-git): git log provider, parser, ref manager",
 "crate-level design of `ig_git::log` and the new ig-git → ig-log dependency")

ttask("ig-git tests: log parser", "ig-git", ["log_parser"], "`GitLogParserTest`", "`log/{parser,record,unescape}.rs`")
ttask("ig-git tests: ref manager", "ig-git", ["log_ref_manager"], "`GitBranchComparatorTest` and `GitLabelComparatorTest`", "`log/{ref_manager,repository_manager}.rs`, `natural_comparator.rs`")
ttask("ig-git tests: output splitter and collector", "ig-git", ["log_collector"], "IdeaGit tests for `GitLogOutputSplitter` and `GitLogRecordCollector`", "`log/{splitter,collector}.rs`")
ttask("ig-git tests: log provider", "ig-git", ["log_provider"], "`GitLogProviderTest` (5), `GitReadRecentCommitsTest` (6) and `GitLogUtilTest` (2, adapted)", "`log/{provider,util}.rs`")
ttask("ig-git tests: refresh and performance", "ig-git", ["log_refresh","log_perf"], "IdeaGit refresh tests and the 50k-commit performance smoke test", "end-to-end: `log_refresh.rs` against `ig-log::data::refresher` and the git provider")

task("Module map and full verification",
 "- Modify: `reference/upstream-map.toml`",
 "- [ ] **Step 1: Replace `reference/upstream-map.toml`** with this content. It adds the new `ig-log::data::*` and `ig-git::log::*` entries, and two deviation lines on existing entries (runner and commits-info index):\n\n" + code("toml", umap) +
 "\n- [ ] **Step 2: Verify everything**\n\n```bash\ncargo fmt --all --check\ncargo clippy -q --all-targets -- -D warnings\nRUSTDOCFLAGS=\"-D warnings\" cargo doc --no-deps --all\ncargo test --all\n```\nExpected: all clean, and 334 tests pass with 1 ignored (the performance smoke test).\n\n",
 "docs: upstream map for the git log data layer", None)

for i,(title, fd, body, commit, review) in enumerate(tasks, start=1):
    rv = f"**Review focus:** {review}.\n\n" if review else ""
    w(f"### Task {i}: {title}\n\n**Files:**\n{fd}\n\n{rv}{body}- [ ] **Step {'N' if False else 'last'}: Lint and commit**\n\n```bash\ncargo fmt --all --check && cargo clippy -q --all-targets -- -D warnings\ngit add -A crates reference Cargo.lock\ngit commit -m \"{commit}\"\n```\n\n---\n\n")
open(OUT,"w").write("".join(o).replace("**Step last:","**Final step:"))
print("tasks", len(tasks), "lines", sum(1 for _ in open(OUT)))
