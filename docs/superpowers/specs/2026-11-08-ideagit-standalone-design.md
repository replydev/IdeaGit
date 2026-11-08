# IdeaGit — Standalone Git client from IntelliJ Community sources

Date: 2026-11-08
Status: Approved design, pending implementation plan

## Goal

Ship IntelliJ IDEA's Git features as a standalone, cross-platform Java/Kotlin desktop application,
reusing the upstream `intellij-community` code **unchanged** wherever possible, so that upstream fixes
can be pulled in repeatedly with minimal effort.

### In scope (v1)

All features provided by the upstream `git4idea` plugin and the VCS platform it sits on, including:

- Commit tool window: changelists, partial (chunk/line) commits, amend, commit message history
- Git Log: graph, filters, details, cherry-pick, revert, reset, undo/edit commit,
  interactive rebase (drop, squash, fixup, reword, reorder)
- Branches popup and branches dashboard; tags; remotes management
- Smart checkout / smart merge / smart rebase; fetch, pull, push dialogs
- Merge conflicts dialog and the 3-way merge window with auto-resolve
- Diff viewer, file history, blame/annotate
- Stash and shelve
- Git console / command log
- **Basic file editing** (plain text editor; needed anyway by diff/merge)
- **Multi-repo** (synchronized branch control) and **worktrees**, as provided upstream

### Out of scope (v1)

- GitHub / GitLab integrations (PR review, account clone)
- Local History
- Language support, run/build, terminal, plugin marketplace, telemetry, update checks
- Native installers (later milestone)

## Key decisions

| Decision | Choice | Reason |
|---|---|---|
| App shape | Stripped IntelliJ Platform product | git4idea depends deeply on the platform (VFS, project model, message bus, diff/merge, VCS Log, actions). Re-implementing the platform would defeat "unchanged code" and make syncs painful. |
| Upstream storage | Vendored module subset + sync script | Keeps the repo manageable while preserving upstream layout for mechanical re-syncs. |
| Build | Gradle, generated from upstream `.iml` / library metadata | Build is regenerated on each sync so it never drifts; avoids adopting upstream's Bazel/JPS tooling. |
| Main window | IDE-like, Git-only (upstream tool windows) | Least custom code, so upstream fixes cover the most surface. |
| UI mode | New UI only | Halves the UI surface to verify. |
| Targets | Cross-platform; `./gradlew run` + distZip first | Native packaging deferred. |
| Upstream tracking | Stable release branch (e.g. `253.x`), not `master` | Fixes without feature churn; branch bumps are deliberate syncs. |

## Architecture

```
IdeaGit/
├── upstream/                 # vendored, never hand-edited; mirrors intellij-community paths
│   ├── platform/...
│   ├── plugins/git4idea/...
│   ├── UPSTREAM_COMMIT       # pinned upstream hash, branch, date
│   └── MODULES.lock          # every vendored module + resolved library coordinates
├── patches/                  # ordered *.patch applied on top of upstream/ (target: ~0)
├── ideagit/
│   ├── product/              # ApplicationInfo, product module/plugin layout, icons, splash
│   ├── customization/        # IdeaGit plugin: trims actions, tool windows, settings; welcome screen
│   └── launcher/             # Gradle application/distribution
├── tools/sync/               # closure calculator, copier, patch applier, Gradle generator
├── build-logic/              # Gradle convention plugins
└── docs/
```

### Runtime

- Entry point is upstream's standard bootstrap (`com.intellij.idea.Main`) driven by the IdeaGit product
  descriptor, which selects the content modules and bundled plugins to load: platform core/UI, plain text
  editor, VCS core, diff/merge, VCS Log, `git4idea`, and the IdeaGit customization plugin.
- Runs on the **JetBrains Runtime (JBR)** at the version pinned by upstream, provisioned via Gradle
  toolchains. The locally installed JDK is not used to run the app.
- Licensing: upstream is Apache 2.0. Preserve LICENSE/NOTICE and file headers. Patches document changes.

## Sync workflow (`tools/sync`)

Implemented in Kotlin (JVM), invoked as:

```
./tools/sync/sync.sh --upstream <path-to-intellij-community-clone> --commit <sha|tag>
```

1. **Resolve:** check out the requested commit in the external upstream clone (outside this repo).
2. **Compute closure:** parse `.iml` files (module deps, library refs), `.idea/libraries/*.xml`, and
   plugin/content-module XML descriptors. Start from `tools/sync/roots.txt`, follow transitive
   dependencies, and apply scope rules (test and `provided` handling configurable). `tools/sync/excludes.txt`
   prunes known-unwanted branches (e.g. onboarding, statistics upload, marketplace).
3. **Copy:** replace `upstream/` with exactly the closure, keeping upstream paths. Write `UPSTREAM_COMMIT` and
   `MODULES.lock`.
4. **Patch:** apply `patches/*.patch` in order with `git apply --3way`. On failure, stop and leave conflicts
   for a human. `tools/sync/refresh-patches` regenerates patch files from the working tree.
5. **Generate build:** emit `settings.gradle.kts` and one `build.gradle.kts` per vendored module from
   `.iml` data: source/resource/test roots, Java vs Kotlin, compiler plugins (kotlinx-serialization,
   Compose if required), module deps → `project(...)`, libraries → Maven coordinates from Maven Central
   or JetBrains' public `intellij-dependencies` repository.
6. **Report:** print modules added/removed, library changes, and patch results. The developer commits
   `sync: upstream <sha>` and then any patch-fix commits.

**Guard rail:** a CI check rebuilds "pinned commit + patches" and fails if `upstream/` differs.

## Customization and trimming

Order of preference: **don't load it → customize via extension points → patch (last resort).**

1. **Product layout (`ideagit/product`):** `IdeaGitApplicationInfo.xml` (name, version derived from the
   upstream build number, branding) and the product module/plugin list. Language plugins, run/build,
   terminal, marketplace, and similar are not loaded, even if their classes sit on the classpath as
   compile dependencies.
2. **Customization plugin (`ideagit/customization`):**
   - Actions: an `ActionConfigurationCustomizer` unregisters/hides non-Git actions and reshapes the main
     menu into File / Edit / View / Git / Window / Help.
   - Tool windows: keep Commit, Git (Log, Console, Branches), Shelf, and a files-only project view. Hide
     the others.
   - Settings: show only Appearance, Keymap, Editor (general/fonts/colors), Version Control → Git.
   - Welcome screen: Open Repository / Clone / recent repositories.
   - Project open: a `ProjectOpenProcessor` + `DirectoryProjectConfigurator` register every Git root under
     the opened folder, with no SDK/module setup.
   - Disable statistics, feedback, "What's New", and update checks.
3. **Patches:** only where no extension point exists. Each patch carries a header with its rationale and
   the extension point that would let us remove it.

Known risks: code paths that assume Java/indexing (e.g. `DumbService` checks) are found by running the
app and handled by customization first, patches second.

## Build, run, distribution

- **Build:** Gradle Kotlin DSL. `build-logic/` provides `ideagit.upstream-module` and `ideagit.own-module`
  conventions. Kotlin/Java versions and flags come from upstream (`.idea/kotlinc.xml`, `.iml` language
  level). Gradle build cache and configuration cache are enabled.
- **Run:** `./gradlew :ideagit:launcher:run --args="/path/to/repo"` uses a sandboxed config/system
  dir under `build/sandbox`.
- **Distribution:** `./gradlew :ideagit:launcher:distZip` produces `bin/ideagit{,.bat}`, `lib/`, the plugin
  layout the platform expects, and either a bundled JBR or the `IDEAGIT_JDK` override.

## Testing

- **Upstream tests:** a curated subset of upstream `git4idea` / `vcs-impl` tests (`tools/sync/tests.txt`)
  runs against our build to validate the vendored code.
- **Sync tool tests:** unit tests on fixture `.iml` trees for closure computation, Gradle generation, and
  patch application.
- **Smoke test:** a headless launch opens a fixture repo, asserts the Git root is registered and the Log
  loads, then exits. Runs in CI on Linux, macOS, and Windows.
- **Manual feature checklist** (`docs/feature-checklist.md`): commit, log, interactive rebase
  (drop/squash/reword), conflicted merge, smart checkout, stash, shelve, Git console, blame, multi-repo,
  worktrees. Run after each upstream sync.

## Milestones

Each milestone gets its own implementation plan.

1. **Sync tool + generated build:** `upstream/` populated at a pinned stable-branch commit, and all
   vendored modules compile. *(Highest risk: closure size and generator fidelity. Planned first.)*
2. **Bootable product:** the IdeaGit product launches, opens a repo, and Git features work; non-Git UI
   is still present.
3. **Customization:** Git-only menus, tool windows, settings, and welcome screen.
4. **Distribution + CI:** distZip, cross-OS smoke test, "upstream unchanged" guard.
5. **First real re-sync:** move to a newer upstream commit to prove the update workflow.
