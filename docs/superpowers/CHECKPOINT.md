# IdeaGit — Checkpoint for the next agent (2026-11-09, after Plan 3)

Read this first. It records where the project is, how the work has been done, and what comes next.

## What IdeaGit is

A cross-platform desktop Git client in **Rust + GPUI**. It reproduces IntelliJ IDEA's Git features and UI as closely as possible and is ported from the open-source `intellij-community` code. Upstream fixes are found and ported by hand; there is no merging.

- **Main spec:** `docs/superpowers/specs/2026-11-08-ideagit-rust-design.md`
- **App-shell spec:** `docs/superpowers/specs/2026-11-08-app-shell-design.md`
- **Technical debt:** `docs/tech-debt.md`
- **README:** `README.md` (build, run, crates)
- **Manual checklist:** `docs/app-shell-checklist.md`

## Repository state

- **`master`** on `origin` = https://github.com/replydev/IdeaGit. CI is green on Linux, macOS, Windows and the MSRV (1.95) job, including the GPUI build.
- **Crates:**
  - `ig-git`: git CLI runner and console events; status, refs, config remotes, repository reader, root detection; `log` (parser, collector, `GitLogProvider`, `GitRefManager`).
  - `ig-log`: the IntelliJ commit graph (permanent graph, layout, print elements, facade) and `data` (storage, refs model, sorter, joiners, graph data, synchronous `VcsLogRefresher`).
  - `ig-ui-kit`: the IntelliJ New UI look on GPUI. Generated theme tokens, `expui` icons, Inter and JetBrains Mono, New UI metrics, graph palette, stripes, tool-window header (`IjDockRenderer` on gpui-component's dock), status bar, notifications.
  - `ig-app`: the `ideagit` binary. It has welcome and project windows, menus and keymap, and the Git tool window. That tool window holds the read-only Log (graph and label painters, background log worker, metadata cache, selection) and the Console. `cargo run --locked -p ig-app -- [path]`.
  - `xtask`: `cargo xtask import-ui --upstream ~/Code/intellij-community` (assets from the pinned commit) and `cargo xtask screenshots` (macOS only).
- **Tests:** 507 pass, plus 1 ignored perf test. They cover ported upstream tests (byte-identical fixtures), IdeaGit tests, and headless GPUI UI tests (`crates/ig-app/tests/ui.rs`, `TestAppContext`).
- **Reference map:** `reference/UPSTREAM` pins `intellij-community` branch `263` at commit `b3c1c1958bc81cc96ea882fd4beba72849c1ed98`. `reference/upstream-map.toml` maps every Rust module to its upstream files, tests and deviations. `#member` anchors and directory entries are intentional.
- **Plan generators:** `docs/superpowers/plangen/gen*.py`. `gen4.py` (Plan 3) holds the tasks as data. The same script renders the plan and applies a task to a scratch clone, which makes the simulation step exact.

## Plans

| Plan | Status |
|---|---|
| 1 — git core (`ig-git`) | Done, merged |
| 2 — commit graph core (`ig-log`) | Done, merged |
| 2c — git log data layer (`ig-log::data`, `ig-git::log`) | Done, merged |
| 3 — app shell and read-only Log (`ig-ui-kit`, `ig-app`, `xtask`) | Done, merged (PR #1) |
| 2b — graph extras (BEK, linear BEK, filters, collapsing, first-parent, fragment hover) | Not started |
| Log UI (details pane, changes tree, filters, context actions) | Not started |
| Diff comparison (`ig-diff`, port of `com.intellij.diff.comparison` with upstream tests) | Not started |
| Change model + commit backend (changelists, line-status tracking, partial commit via temp index) | Not started |
| Commit tool window, diff viewer, basic editor | Not started |
| Branches popup, checkout/smart checkout, askpass, fetch/pull/push/update | Not started |
| Packaging (`xtask dist`), `xtask upstream-watch` | Not started |

Plan files are in `docs/superpowers/plans/`. Each one has "Not in this plan" / "Known gaps" and "Notes for …" sections that collect deferred review findings. **Read the notes in Plans 2c and 3 and `docs/tech-debt.md` before the Log UI and file-watching work.** Important open items include:

- **Tag-fetch refresh is slow.** One `git log` runs per new tag. This needs a refresh-strategy decision.
- **Some repo changes need a full reload.** The watcher must force one when `.git/shallow`, grafts or `refs/replace` change.
- **Runner cancellation and grandchild pipe hangs are still open.** Closing a window cancels the log worker between steps, but a running `git` is not killed.
- **Two `Hash` types** exist and should be unified into a leaf crate.
- **`Git` should be split** into an app-wide executable part and per-project parts.
- **The error contract** needs settling.
- **App-shell gaps:**
  - Log table: column resize, Root column, current-branch highlight, tooltips.
  - Dock resize: no `LayoutChanged` event, a closed dock doesn't reopen on drag.
  - Branch widget: no tag name or merge/rebase prefixes.
  - Root detection still runs on the UI thread.
- **Never checked by eye.** Nobody has run the app interactively in this pipeline: not the welcome window, the native menus, nor the focus behaviour. Run `docs/app-shell-checklist.md` on macOS once.

## How the work is done (keep doing this)

1. **Brainstorm → spec**, using `superpowers:brainstorming`. Ask one question at a time and get user approval.
2. **Verified prototype first**, in a scratch clone. A strong-model subagent builds the full implementation plus tests. You then check fmt, clippy, docs, tests and fixtures yourself.
3. **Generate the plan from the prototype.** Model it on `docs/superpowers/plangen/gen4.py`: tasks as data, files copied with `git checkout <prototype ref> -- <paths>`, intermediate files written in full.
4. **Simulate the task order** on a fresh clone, with every step green. `gen4.py apply` plus a small driver script does this.
5. **Execute subagent-driven** on a feature branch:
   1. A cheap (haiku) implementer per task copies the files and runs the checks. It reports BLOCKED instead of editing code.
   2. Spec compliance: diff the commit's tree against the simulation's tree for that task.
   3. Quality review: sonnet for small modules, opus for core or concurrency-heavy ones. Point it at the upstream sources.
   4. Fixes run in **separate git worktrees** (`git worktree add -b fix/<x> <dir> <base>`) with their own `CARGO_TARGET_DIR`. Then cherry-pick them. This lets implementers keep going in the main tree in parallel.
   5. Fixers must keep public APIs source-compatible with files that later tasks copy verbatim. Check this by temporarily checking out those later files and running `cargo check`.
   6. Re-review if any finding was Important.
   7. Collect deviations as you go, and apply them in the plan's map task.
6. **CI before merge.** CI runs on pushes to master and on pull requests, so open a draft PR for the feature branch. Fix CI on the branch.
7. **Final whole-branch review** with opus. Fix blockers, then fast-forward merge to `master`, push, and watch CI.

Lessons from Plan 3:
- **zsh has no `PIPESTATUS`.** Don't pipe verify commands, or use `set -o pipefail`.
- **Check copies by staging first.** `git diff <ref>` ignores untracked files, so the exact-copy check is `git add -A && git diff --cached --exit-code <ref> -- <paths>`.
- **`cargo test` stops at the first failing test binary.** A red CI job can hide more failures.
- **Test fixtures need `-text`** in `.gitattributes`, otherwise Windows checks them out with CRLF.
- **Tests that race worker threads must be made deterministic** with test hooks set before the worker starts.

## Conventions

- **Faithful port.**
  - Keep upstream algorithms and quirks.
  - Fix only real bugs, and record each fix as a deviation in the module docs and `reference/upstream-map.toml`.
  - Upstream exceptions become panics only where unreachable from the UI. Public facade or UI paths must not panic on stale or unknown input.
- **Tests.**
  - Ported upstream tests stay faithful. Only minimal harness changes are allowed, and expected values never change.
  - IdeaGit-only tests are marked as such.
  - Tests are hermetic: `Git::with_base_environment` with an empty global config, `GIT_CONFIG_NOSYSTEM=1`, and the scrubbed `GIT_*` variables. UI tests use scratch settings through `AppState`'s git environment and settings path.
  - Never glob-import `gpui` in test modules: `#[gpui::test]` then crashes rustc.
- **Builds.**
  - Always pass `--locked`: the Zed rev is pinned only in `Cargo.lock`.
  - Locally, set `CARGO_NET_GIT_FETCH_WITH_CLI=true`.
  - Never run a plain `cargo update`.
- **Commits.**
  - Never add AI attribution or `Co-Authored-By` (user preference).
  - Commits are SSH-signed through 1Password. A transient "failed to fill whole buffer" error means retry; never disable signing. `%G?` shows `N` locally because no allowed-signers file is configured; that's expected.
- **Pushing as `replydev`.** The global git config rewrites `https://github.com/` to SSH as `lorenzopatane`, who has no write access. Push with:
  ```
  git -c credential.helper= -c 'credential.helper=!f(){ echo username=replydev; echo "password=$(gh auth token --user replydev)"; }; f' push origin <branch>
  ```
  Check CI with `GH_TOKEN=$(gh auth token --user replydev) gh run list -R replydev/IdeaGit`.

## Environment notes (this machine)

- **Upstream reference clone:** `~/Code/intellij-community`, a sparse, blob-less clone of branch `263` at `b3c1c19`. Read any file with `git show HEAD:<path>`.
- **Rust toolchains:** the default is 1.96. CI needs 1.95 (the MSRV). Extra toolchains (1.85, 1.93, 1.94, 1.97–1.99) can be removed with `rustup toolchain uninstall`.
- **Disposable:** `/tmp/ig-proto*`, `/tmp/ig-tasks*`, and the `wip/app-shell-prototype` branch, which is fully merged in spirit. Delete it once you no longer want to compare against it.

## Next steps

1. Run `docs/app-shell-checklist.md` by hand on macOS. Nobody has used the app interactively yet. Fix what it finds.
2. Brainstorm the next plan with the user. The likely candidates are:
   - the Log UI (details, changes, filters, actions, plus Plan 2b's BEK, filters and collapsing)
   - the diff/commit path
