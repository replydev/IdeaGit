# IdeaGit — Checkpoint for the next agent (2026-11-09)

Read this first. It records where the project is, how the work has been done, and what comes next.

## What IdeaGit is

A cross-platform desktop Git client in **Rust + GPUI**. It reproduces IntelliJ IDEA's Git features and UI as closely as possible and is ported from the open-source `intellij-community` code. Upstream fixes are found and ported by hand; there is no merging.

- **Main spec:** `docs/superpowers/specs/2026-11-08-ideagit-rust-design.md`
- **App-shell spec:** `docs/superpowers/specs/2026-11-08-app-shell-design.md`. Approved. Its decisions were updated after the prototype spike.
- **Technical debt:** `docs/tech-debt.md`
- **README:** `README.md`

## Repository state

- **`master`** on `origin` = https://github.com/replydev/IdeaGit. CI is green on Linux, macOS, Windows and the MSRV job.
- **Crates on `master`:**
  - `ig-git`
    - git CLI runner and console events
    - status, refs, config remotes, repository reader, root detection
    - `log`: parser, collector, `GitLogProvider`, `GitRefManager`
  - `ig-log`
    - the IntelliJ commit graph: permanent graph, layout, print elements, facade
    - `data`: storage, refs model, sorter, joiners, graph data, the synchronous `VcsLogRefresher`
- **Tests:** about 398 pass, plus 1 ignored perf test. That covers ported upstream tests (byte-identical fixtures) and IdeaGit tests.
- **Branch `wip/app-shell-prototype`** (on `origin` too): an **unreviewed but verified** prototype of the app shell and read-only Log. It's the source for the next plan.
  - **Crates:** adds `ig-ui-kit`, `ig-app` and `xtask` (`import-ui`, `screenshots`).
  - **Assets and screenshots:** `assets/`, and screenshots in `docs/ui-reference/`.
  - **Tests and MSRV:** 460 tests pass on Rust 1.96. MSRV is 1.95.
  - **Changes outside the new crates:** a few small ones in `ig-git` (`ConsoleSink::silent_command`), `ig-log` (`EdgePrintElementImpl::is_terminal`) and lint fixes.
  - **Plan generators:** `docs/superpowers/plangen/gen*.py`, the scripts that turned earlier prototypes into plans.
- **Reference map:** `reference/UPSTREAM` pins `intellij-community` branch `263` at commit `b3c1c1958bc81cc96ea882fd4beba72849c1ed98`. `reference/upstream-map.toml` maps every Rust module to its upstream files, tests and deviations. All of its paths exist at the pinned commit.

## Plans

| Plan | Status |
|---|---|
| 1 — git core (`ig-git`) | Done, merged |
| 2 — commit graph core (`ig-log`) | Done, merged |
| 2c — git log data layer (`ig-log::data`, `ig-git::log`) | Done, merged |
| App shell + read-only Log | Spec approved; prototype on `wip/app-shell-prototype`. **Next: write the plan from the prototype, then execute it.** |
| 2b — graph extras (BEK, linear BEK, filters, collapsing, first-parent, fragment hover) | Not started |
| Log UI (details pane, changes tree, filters, context actions) | Not started |
| Diff comparison (`ig-diff`, port of `com.intellij.diff.comparison` with upstream tests) | Not started |
| Change model + commit backend (changelists, line-status tracking, partial commit via temp index) | Not started |
| Commit tool window, diff viewer, basic editor | Not started |
| Branches popup, checkout/smart checkout, askpass, fetch/pull/push/update | Not started |
| Packaging (`xtask dist`), `xtask upstream-watch` | Not started |

Plan files are in `docs/superpowers/plans/`. Each one has "Not in this plan" and "Notes for …" sections that collect review findings deferred to later plans. **Read the notes in Plans 2 and 2c before starting the Log UI and file-watching work.** Important deferred items include:

- **Tag-fetch refresh is slow.** One `git log` runs per new tag, so 300 tags took about 19 s. This needs a refresh-strategy decision.
- **Some repo changes need a full reload.** The watcher must force one when `.git/shallow`, grafts or `refs/replace` change.
- **Runner cancellation and grandchild pipe hangs** are still open.
- **Two `Hash` types** exist and should be unified into a leaf crate.
- **`Git` should be split** into an app-wide executable part and per-project parts.
- **The error contract** needs settling.

## How the work is done (keep doing this)

1. **Brainstorm → spec**, using `superpowers:brainstorming`. Ask one question at a time and get user approval.
2. **Verified prototype first**, in a scratch clone. A strong-model subagent builds the full implementation plus tests in `/tmp/ig-protoN`. You then independently check fmt, clippy, docs, tests and fixtures.
3. **Generate the plan from the prototype**, using the scripts in `docs/superpowers/plangen/` as models. Each task embeds the verified code.
4. **Simulate the task order** on a fresh clone. Apply each task's files in order and run fmt, clippy and tests after each one. Every step must be green.
5. **Execute subagent-driven** (`superpowers:subagent-driven-development`), on a feature branch:
   1. A cheap implementer subagent per task. It copies the files from the prototype and checks them with `cmp`. It must report BLOCKED instead of editing code, and it must run a "files unchanged since prototype" precheck before overwriting files that earlier fixes may have touched.
   2. Spec-compliance check: you `cmp` the result against the prototype.
   3. Quality review by a subagent: sonnet for small module groups, opus for core or concurrency-heavy ones. Point it at the upstream sources so it can compare.
   4. A fix subagent applies the review findings. Re-review if any finding was Important.
   5. Collect deviations for the upstream map as you go, and apply them in the plan's map task.
6. **Final whole-branch review** with opus. Fix blockers, then fast-forward merge to `master`, push, and watch CI.

## Conventions

- **Faithful port.**
  - Keep upstream algorithms and quirks.
  - Fix only real bugs, and record each fix as a deviation in the module docs and `reference/upstream-map.toml`.
  - Upstream exceptions become panics only where unreachable from the UI. Public facade or UI paths must not panic on stale or unknown input.
- **Tests.**
  - Ported upstream tests stay faithful. Only minimal harness changes are allowed, such as version guards or old-git-safe setup. Expected values never change.
  - IdeaGit-only tests go in `*_extra.rs` (or similar) files headed "IdeaGit tests (not ported from upstream)".
  - Tests are hermetic: use `Git::with_base_environment` and the scrubbed environment in `tests/common`.
- **Commits.**
  - Never add AI attribution or `Co-Authored-By` (user preference).
  - Commits are SSH-signed through 1Password. A transient "failed to fill whole buffer" error means retry; never disable signing.
- **Pushing as `replydev`.** The global git config rewrites `https://github.com/` to SSH as `lorenzopatane`, who has no write access. Push with:
  ```
  git -c credential.helper= -c 'credential.helper=!f(){ echo username=replydev; echo "password=$(gh auth token --user replydev)"; }; f' push origin <branch>
  ```
  `origin` is `https://replydev@github.com/replydev/IdeaGit.git`. Check CI with `GH_TOKEN=$(gh auth token --user replydev) gh run list -R replydev/IdeaGit`.
- **GPUI dependencies.** On the prototype branch, Zed's gpui is pinned via `Cargo.lock`, so always build with `--locked`. Locally, fetching git dependencies needs `CARGO_NET_GIT_FETCH_WITH_CLI=true` because of the SSH rewrite. See `docs/tech-debt.md`.

## Environment notes (this machine)

- **Upstream reference clone:** `~/Code/intellij-community`. It's a sparse, blob-less clone of branch `263` at `b3c1c19`. Add paths with `git sparse-checkout add <path>`, or read any file with `git show HEAD:<path>`.
- **Rust toolchains.** The default is 1.96, which builds everything. The subagents also installed 1.85, 1.93, 1.94, 1.95, 1.97, 1.98 and 1.99. CI needs 1.95 (the MSRV). Remove the rest with `rustup toolchain uninstall 1.85 1.93 1.94 1.97 1.98 1.99` if the user wants.
- **`/tmp/ig-proto*` and `/tmp/ig-tasks*` are disposable.** Everything needed is on `master` or on `wip/app-shell-prototype`.

## Next steps

1. Write the app-shell plan from `wip/app-shell-prototype`.
   - Use the order the prototype author gave:
     1. workspace and MSRV
     2. `xtask import-ui` and assets
     3. `ig-ui-kit` logic, then widgets
     4. the small `ig-log` / `ig-git` additions
     5. `ig-app` logic modules
     6. worker and model, plus the headless UI tests
     7. view, project, welcome, menus, `main`
     8. screenshots
   - The plan must say:
     - Never glob-import `gpui` in test modules, because `#[gpui::test]` recursion crashes rustc.
     - Use `--locked` everywhere.
     - The `runtime_shaders` feature is needed on Macs without full Xcode.
     - The first CI run is the spike for Windows (needs `llvm-rc`) and Linux (system packages).
   - Known gaps to keep or schedule:
     - column resize, Root column, current-branch highlight, tooltips
     - `docs/app-shell-checklist.md`
     - the git-missing notification outside the welcome window
     - the welcome window and native menus were never visually checked
2. Execute the plan with the process above, then merge.
3. Then brainstorm the next plan with the user. Likely candidates are the Log UI (details, changes, filters, actions, plus Plan 2b) or the diff/commit path.
