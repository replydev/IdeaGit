# IdeaGit

A cross-platform desktop Git client written in Rust that reproduces the Git features and user interface of
[IntelliJ IDEA](https://www.jetbrains.com/idea/). It is ported from the open-source
[IntelliJ IDEA Community](https://github.com/JetBrains/intellij-community) code base.

IdeaGit is an independent project. It is not affiliated with or endorsed by JetBrains. IntelliJ IDEA is a trademark
of JetBrains s.r.o.

## Status

Early development. The first runnable build opens a folder, shows IntelliJ's New UI look (theme, icons, fonts,
tool-window stripes) and browses the Git history in a read-only **Log** tab with IntelliJ's commit graph, next to the
**Git Console**. Light and dark themes follow the OS appearance.

| Crate | What it does | Ported from (IntelliJ Community) |
|---|---|---|
| [`ig-git`](crates/ig-git) | Runs the `git` CLI the way IntelliJ does (same flags, environment, locking and Git console events). Parses `git status`, refs and remotes, and reads repository state (merge, rebase, cherry-pick, revert, detached HEAD, worktrees, reftable). Detects repository roots. Reads the log (`git log` parser, ref manager, log provider). | [`plugins/git4idea`](https://github.com/JetBrains/intellij-community/tree/b3c1c1958bc81cc96ea882fd4beba72849c1ed98/plugins/git4idea) |
| [`ig-log`](crates/ig-log) | The Git Log commit graph: building, lane layout, and per-row print elements, colors and arrow actions. The VCS Log data layer: storage, refs model, sorter, joiners and refresher. | [`platform/vcs-log/graph`](https://github.com/JetBrains/intellij-community/tree/b3c1c1958bc81cc96ea882fd4beba72849c1ed98/platform/vcs-log/graph), [`platform/vcs-log/graph-api`](https://github.com/JetBrains/intellij-community/tree/b3c1c1958bc81cc96ea882fd4beba72849c1ed98/platform/vcs-log/graph-api), [`platform/vcs-log/impl`](https://github.com/JetBrains/intellij-community/tree/b3c1c1958bc81cc96ea882fd4beba72849c1ed98/platform/vcs-log/impl) |
| [`ig-ui-kit`](crates/ig-ui-kit) | The IntelliJ New UI look on [GPUI](https://www.gpui.rs/): generated theme tokens, `expui` icons, Inter and JetBrains Mono, New UI metrics, the graph palette, tool-window stripes and headers, status bar, notifications. | `platform/platform-resources` themes, `platform/icons`, `JBUI` |
| [`ig-app`](crates/ig-app) | The `ideagit` application: welcome and project windows, menus and keymap, the Git tool window with the Log (graph and label painters, background log worker) and the Console. | [`platform/vcs-log/impl`](https://github.com/JetBrains/intellij-community/tree/b3c1c1958bc81cc96ea882fd4beba72849c1ed98/platform/vcs-log/impl) UI, `GitConsole` |
| [`xtask`](xtask) | `cargo xtask import-ui` imports the UI resources from the pinned upstream commit; `cargo xtask screenshots` renders the Log to PNG. | |

Planned next: the Git Log UI (details, changes, filters, actions), Commit tool window, diff and merge, and branches,
fetch, pull and push. See the [design spec](docs/superpowers/specs/2026-11-08-ideagit-rust-design.md) and the
[implementation plans](docs/superpowers/plans).

## How the port relates to IntelliJ IDEA

IdeaGit tracks a pinned revision of the official sources so upstream fixes can be found and ported:

- **Upstream repository:** <https://github.com/JetBrains/intellij-community>
- **Pinned revision:** branch `263`, commit
  [`b3c1c19`](https://github.com/JetBrains/intellij-community/tree/b3c1c1958bc81cc96ea882fd4beba72849c1ed98), recorded in
  [`reference/UPSTREAM`](reference/UPSTREAM)
- **Module map:** [`reference/upstream-map.toml`](reference/upstream-map.toml) lists, for each Rust module, the upstream
  files it is ported from and the upstream tests it carries. It also lists every intentional deviation, such as fixed
  upstream bugs and Rust-specific adaptations.
- **Tests:** upstream unit tests are ported along with their test data, copied byte-identical (for example
  `GitVersionTest`, `GitRepositoryReaderTest`, and the VCS Log graph tests). Upstream fixture-based tests become
  integration tests against the real `git` executable.

Useful official references:

- IntelliJ IDEA Git documentation: <https://www.jetbrains.com/help/idea/using-git-integration.html>
- IntelliJ IDEA Community source code: <https://github.com/JetBrains/intellij-community>
- Git plugin (`git4idea`) sources:
  <https://github.com/JetBrains/intellij-community/tree/master/plugins/git4idea>
- VCS Log sources: <https://github.com/JetBrains/intellij-community/tree/master/platform/vcs-log>

## Building, running and testing

Requires Rust 1.95 or newer and `git` 2.17 or newer (2.19.2 on Windows). On Linux, GPUI needs the X11, Wayland,
Vulkan and fontconfig development packages; see `.github/workflows/ci.yml` for the list. On Windows it needs
`llvm-rc` (from LLVM).

Zed's gpui revision is pinned only in `Cargo.lock`, so always pass `--locked`:

```bash
cargo run --locked -p ig-app -- [path]   # opens a folder, or the welcome window
cargo test --locked --all
cargo clippy --locked --all-targets -- -D warnings
```

UI resources are imported from an intellij-community clone at the pinned commit with
`cargo xtask import-ui --upstream <clone>`. Before a release, run through
[`docs/app-shell-checklist.md`](docs/app-shell-checklist.md).

## License

Licensed under the [Apache License, Version 2.0](LICENSE). IdeaGit contains code ported from IntelliJ IDEA
Community Edition, Copyright JetBrains s.r.o. and contributors, also licensed under Apache 2.0. See [`NOTICE`](NOTICE)
for attribution.
