#!/usr/bin/env python3
"""Plan 3 (app shell + read-only Log) generator.

The tasks are data: each one copies files verbatim from the prototype and/or writes intermediate
("mid") versions of files that later tasks replace. The same data renders the plan and applies a
task to a scratch clone, so the simulation runs exactly what the plan says.

    gen4.py plan  <prototype checkout> <out.md> [counts.json]
    gen4.py apply <prototype checkout> <target clone> <task number>
    gen4.py count                      # number of tasks
"""
import json, os, re, shutil, subprocess, sys

PROTO_REF = "origin/wip/app-shell-prototype"
R = None  # prototype checkout


def rd(p):
    return open(os.path.join(R, p), encoding="utf-8").read()


def is_binary(p):
    return p.endswith((".ttf", ".png")) or b"\0" in open(os.path.join(R, p), "rb").read()


def expand(paths):
    """Directories expand to their tracked files in the prototype."""
    out = []
    for p in paths:
        full = os.path.join(R, p)
        if os.path.isdir(full):
            ls = subprocess.run(["git", "ls-files", "-z", "--", p], cwd=R, capture_output=True, check=True).stdout
            out += sorted(x.decode() for x in ls.split(b"\0") if x)
        else:
            out.append(p)
    return out


def drop(text, *snippets):
    for s in snippets:
        assert s in text, f"snippet not found: {s!r}"
        text = text.replace(s, "", 1)
    return text


def sub(text, old, new):
    assert old in text, f"snippet not found: {old!r}"
    return text.replace(old, new, 1)


# ---------------------------------------------------------------------------------------------
# Intermediate file versions
# ---------------------------------------------------------------------------------------------

def mids():
    m = {}
    m["ui_kit_lib_skeleton"] = (
        "//! IntelliJ New UI look for IdeaGit. The modules land in the next tasks of the app-shell plan.\n"
    )
    m["app_lib_skeleton"] = (
        "//! IdeaGit's application. The modules land in the next tasks of the app-shell plan.\n"
    )
    m["app_main_skeleton"] = (
        "//! `cargo run -p ig-app -- [path]`. The application lands in a later task of the app-shell plan.\n\n"
        "fn main() {}\n"
    )
    m["xtask_lib_skeleton"] = (
        "//! Developer tasks for IdeaGit. `import-ui` lands in the next task of the app-shell plan.\n"
    )
    m["xtask_main_skeleton"] = "fn main() {}\n"

    kit = rd("crates/ig-ui-kit/src/lib.rs")
    m["ui_kit_lib_logic"] = drop(
        kit,
        "pub mod notification;\n",
        "pub mod status_bar;\n",
        "pub mod stripe;\n",
        "pub mod table;\n",
        "pub mod tool_window;\n",
    )

    app = rd("crates/ig-app/src/lib.rs")
    head = app[: app.index("pub mod actions;")]
    m["app_lib_basics"] = head + (
        "pub mod actions;\n"
        "pub mod console;\n"
        "pub mod logging;\n"
        "pub mod paths;\n"
        "pub mod settings;\n"
        "pub mod strings;\n"
    )
    m["app_lib_log"] = sub(m["app_lib_basics"], "pub mod console;\n", "pub mod console;\npub mod log;\n")
    m["app_lib_view"] = drop(app, '#[cfg(feature = "screenshots")]\npub mod screenshots;\n')

    logmod = rd("crates/ig-app/src/log/mod.rs")
    doc_end = logmod.index("pub mod ")
    mods = logmod[doc_end:]
    m["log_mod_helpers"] = (
        "//! The read-only Git Log: the ported painters ([`graph_painter`], [`labels`]) and their helpers.\n"
        "//! Loading and the table land in the next tasks.\n\n"
        + drop(mods, "pub mod model;\n", "pub mod view;\n", "pub mod worker;\n")
    )
    m["log_mod_model"] = (
        "//! The read-only Git Log: background loading ([`worker`]), the UI-side state ([`model`]) and the\n"
        "//! ported painters ([`graph_painter`], [`labels`]) with their helpers. The table lands in the\n"
        "//! next task.\n\n"
        + drop(mods, "pub mod view;\n")
    )

    main = rd("crates/ig-app/src/main.rs")
    block_start = main.index('    #[cfg(feature = "screenshots")]')
    block_end = main.index("    let path = args.next()")
    m["app_main_view"] = main[:block_start] + main[block_end:]

    ui = rd("crates/ig-app/tests/ui.rs")
    ui = sub(
        ui,
        "//! an arrow click jumps, Refresh reaches the worker, and the Git tool window toggles.\n",
        "//! an arrow click jumps, and Refresh reaches the worker.\n",
    )
    m["ui_tests_model"] = ui[: ui.index("\n#[gpui::test]\nfn git_tool_window_toggles")]
    # Written for this plan (not on the prototype branch).
    m["readme"] = rd("README.md")
    m["checklist"] = rd("docs/app-shell-checklist.md")
    return m


# ---------------------------------------------------------------------------------------------
# Tasks
# ---------------------------------------------------------------------------------------------

class Task:
    def __init__(self, title, commit, review, intro=""):
        self.title, self.commit, self.review, self.intro = title, commit, review, intro
        self.steps = []  # ("copy", paths, text) | ("write", path, mid key, text) | ("run", cmds, text) | ("note", text)

    def copy(self, paths, text, embed=True, hidden=False):
        self.steps.append(("copy", paths, text, embed, hidden))
        return self

    def write(self, path, key, text):
        self.steps.append(("write", path, key, text))
        return self

    def run(self, cmds, text):
        self.steps.append(("run", cmds, text))
        return self

    def note(self, text):
        self.steps.append(("note", text))
        return self

    def touched(self):
        out = []
        for s in self.steps:
            if s[0] == "copy":
                out += expand(s[1])
            elif s[0] == "write":
                out.append(s[1])
        return out


CHECK = """cargo fmt --all --check
cargo clippy -q --locked --all-targets -- -D warnings
cargo clippy -q --locked -p ig-app --all-targets --features screenshots -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc -q --no-deps --all
cargo test -q --locked --all"""

LINT_FIXES = [
    "crates/ig-git/src/config.rs",
    "crates/ig-git/src/log/util.rs",
    "crates/ig-git/src/refs.rs",
    "crates/ig-git/tests/log_perf.rs",
    "crates/ig-log/src/permanent/permanent_commits_info_impl.rs",
    "crates/ig-log/src/utils/flags.rs",
]

XTASK_INPUTS = ["assets/theme.keys", "assets/icons.list", "assets/strings.list", "assets/keymap.list"]
XTASK_GENERATED = [
    "crates/ig-ui-kit/src/theme/tokens.rs",
    "assets/themes",
    "assets/icons",
    "assets/strings",
    "assets/keymaps",
    "assets/fonts",
]


def tasks():
    t = []

    a = Task(
        "Workspace, MSRV 1.95, crate skeletons and CI",
        "build: GPUI workspace (MSRV 1.95), ig-ui-kit/ig-app/xtask skeletons, CI for GPUI",
        "the `Cargo.toml` pin comments (Zed rev only in `Cargo.lock`), the CI package list, and the let-chain lint fixes (behavior unchanged)",
        "This task makes `Cargo.lock` final. The three new crates get their final manifests and empty sources, "
        "so every later task builds with `--locked` against the same dependency tree. It is also the first gpui "
        "build, which takes several minutes.",
    )
    a.copy(
        ["Cargo.toml", ".cargo/config.toml", ".gitignore", ".github/workflows/ci.yml"],
        "**Workspace files.** `Cargo.toml` adds the three members and the gpui dependencies, and raises "
        "`rust-version` to 1.95. `.cargo/config.toml` adds the `cargo xtask` alias. CI installs GPUI's Linux "
        "packages, uses `--locked`, lints the `screenshots` feature, and moves the MSRV job to 1.95.",
    )
    a.copy(["Cargo.lock"], "**`Cargo.lock`.** Take it from the prototype. Never regenerate it: it is the only place the Zed rev is pinned.", embed=False)
    a.copy(
        ["crates/ig-ui-kit/Cargo.toml", "crates/ig-app/Cargo.toml", "xtask/Cargo.toml"],
        "**Final crate manifests.** `ig-app` builds the `ideagit` binary. Its optional `screenshots` feature "
        "turns on GPUI test support and `image`.",
    )
    a.write("crates/ig-ui-kit/src/lib.rs", "ui_kit_lib_skeleton", "**Skeleton:** `crates/ig-ui-kit/src/lib.rs`")
    a.write("crates/ig-app/src/lib.rs", "app_lib_skeleton", "**Skeleton:** `crates/ig-app/src/lib.rs`")
    a.write("crates/ig-app/src/main.rs", "app_main_skeleton", "**Skeleton:** `crates/ig-app/src/main.rs`")
    a.write("xtask/src/lib.rs", "xtask_lib_skeleton", "**Skeleton:** `xtask/src/lib.rs`")
    a.write("xtask/src/main.rs", "xtask_main_skeleton", "**Skeleton:** `xtask/src/main.rs`")
    a.copy(
        LINT_FIXES,
        "**Lint fixes.** With `rust-version = 1.95`, clippy's `collapsible_if` suggests let chains and "
        "`manual_is_multiple_of` fires. These are mechanical rewrites with no behavior change.",
    )
    t.append(a)

    b = Task(
        "`xtask import-ui` and the imported assets",
        "feat(xtask): import-ui (theme tokens, icons, strings, keymaps, fonts) and the imported assets",
        "theme parent-chain resolution and `*` pattern order vs upstream `UITheme`/`UIManager` loading; keymap parsing vs `KeymapImpl`; missing-key failure leaves the workspace untouched",
        "`import-ui` reads a pinned intellij-community checkout through `git show` (`GitSource`) or a plain "
        "directory (`FsSource`, used by the fixture test). It resolves everything before writing anything.",
    )
    b.copy(["xtask/src"], "**The `xtask` sources.**")
    b.copy(["xtask/tests"], "**Tests and fixtures.** The fixture `.ttf` files are tiny binary stand-ins; copy them from the branch.")
    b.copy(XTASK_INPUTS, "**Hand-written input lists** under `assets/`.")
    b.copy(["NOTICE"], "**`NOTICE`** gains the font attribution.")
    b.run(
        "cargo xtask import-ui --upstream ~/Code/intellij-community\n"
        f"git add -A && git diff --cached --exit-code {PROTO_REF} -- assets crates/ig-ui-kit/src/theme/tokens.rs",
        "**Generate the assets** from the pinned upstream clone. This writes `assets/{themes,icons,strings,keymaps,fonts}` "
        "and `crates/ig-ui-kit/src/theme/tokens.rs` (not yet compiled: `ig-ui-kit` declares `theme` in the next task). "
        "Expected: the generated files equal the prototype's. If the upstream clone can't be read, check them out from "
        f"`{PROTO_REF}` instead and say so in the report.",
    )
    b.copy(XTASK_GENERATED, "", embed=False, hidden=True)  # the simulation copies what step 5 generates
    t.append(b)

    c = Task(
        "`ig-ui-kit`: theme, fonts, icons, metrics, palette",
        "feat(ig-ui-kit): theme tokens, fonts, icons, New UI metrics, graph palette",
        "`palette.rs` vs upstream `DefaultColorGenerator`/`GraphColorManagerImpl`; `metrics.rs` constants vs their cited `JBUI` sources; `theme` installs into gpui-component's theme without leaking globals",
    )
    c.copy(
        [
            "crates/ig-ui-kit/build.rs",
            "crates/ig-ui-kit/src/fonts.rs",
            "crates/ig-ui-kit/src/icons.rs",
            "crates/ig-ui-kit/src/metrics.rs",
            "crates/ig-ui-kit/src/palette.rs",
            "crates/ig-ui-kit/src/theme/mod.rs",
        ],
        "**The logic modules.** `build.rs` embeds `assets/icons/**/*.svg` into the binary.",
    )
    c.write("crates/ig-ui-kit/src/lib.rs", "ui_kit_lib_logic", "**`crates/ig-ui-kit/src/lib.rs`**, without the widgets yet")
    t.append(c)

    d = Task(
        "`ig-ui-kit`: widgets",
        "feat(ig-ui-kit): tool window stripes and header, status bar, notifications, table colors",
        "`IjDockRenderer` against gpui-component 0.5.2's dock API; stripe and header layout vs `ToolWindowPane`/`ToolWindowHeader` New UI metrics",
    )
    d.copy(
        [
            "crates/ig-ui-kit/src/notification.rs",
            "crates/ig-ui-kit/src/status_bar.rs",
            "crates/ig-ui-kit/src/stripe.rs",
            "crates/ig-ui-kit/src/table.rs",
            "crates/ig-ui-kit/src/tool_window.rs",
            "crates/ig-ui-kit/src/lib.rs",
        ],
        "**The widgets and the final `lib.rs`.**",
    )
    t.append(d)

    e = Task(
        "`ig-log` and `ig-git` additions for the UI",
        "feat: EdgePrintElementImpl::is_terminal and ConsoleSink::silent_command",
        "`is_terminal` keeps upstream equality (ignored, as in `EdgePrintElementImpl.equals`); `silent_command` default is a no-op so existing sinks are unchanged",
        "Two small additions. The graph painter leaves a gap before a terminal arrow, as `SimpleGraphCellPainter` "
        "does, so it needs to know which edges are `TerminalEdgePrintElement`s. The console debug toggle needs "
        "silent commands reported.",
    )
    e.copy(
        [
            "crates/ig-log/src/print/elements.rs",
            "crates/ig-log/tests/print_element_generator_extra.rs",
            "crates/ig-git/src/console.rs",
            "crates/ig-git/src/runner.rs",
            "crates/ig-git/tests/runner_integration.rs",
        ],
        "**The changed files.** Each test addition is marked as an IdeaGit test.",
    )
    t.append(e)

    f = Task(
        "`ig-app`: strings, paths, settings, logging, actions, console",
        "feat(ig-app): strings, settings, logging, action registry and keymap, Git console",
        "`console.rs` format vs upstream `GitConsole`/`VcsConsoleLine`; action ids and keymap loading vs `KeymapImpl`; settings persistence of recent projects",
    )
    f.copy(
        [
            "crates/ig-app/src/strings.rs",
            "crates/ig-app/src/paths.rs",
            "crates/ig-app/src/settings.rs",
            "crates/ig-app/src/logging.rs",
            "crates/ig-app/src/actions.rs",
            "crates/ig-app/src/console.rs",
        ],
        "**The modules.**",
    )
    f.write("crates/ig-app/src/lib.rs", "app_lib_basics", "**`crates/ig-app/src/lib.rs`**, with these modules only")
    t.append(f)

    g = Task(
        "`ig-app`: Log painters and helpers",
        "feat(ig-app): graph painter, label painter, ref groups, relative dates, selection, metadata cache",
        "`graph_painter.rs` vs `SimpleGraphCellPainter`; `labels.rs` vs `LabelPainter` (grouping and compaction); `ref_groups.rs` vs `GitRefManager` groups; `date.rs` vs `DateFormatUtil.formatPrettyDateTime`",
    )
    g.copy(
        [
            "crates/ig-app/src/log/date.rs",
            "crates/ig-app/src/log/ref_groups.rs",
            "crates/ig-app/src/log/selection.rs",
            "crates/ig-app/src/log/metadata.rs",
            "crates/ig-app/src/log/graph_painter.rs",
            "crates/ig-app/src/log/labels.rs",
        ],
        "**The modules.**",
    )
    g.write("crates/ig-app/src/log/mod.rs", "log_mod_helpers", "**`crates/ig-app/src/log/mod.rs`**, helpers only")
    g.write("crates/ig-app/src/lib.rs", "app_lib_log", "**`crates/ig-app/src/lib.rs`** adds `pub mod log;`")
    t.append(g)

    h = Task(
        "`ig-app`: log worker, model and headless UI tests",
        "feat(ig-app): background log worker, Log model, headless UI tests",
        "threading in `worker.rs` (one thread, coalesced refreshes, panic catch and restart, data-pack handler never re-enters the refresher); metadata loading per visible range; `model.rs` re-fetches print elements after `is_repaint_required`",
        "**Never glob-import `gpui` in test modules** (`use gpui::*;`): `#[gpui::test]` then expands recursively and crashes rustc. Import the names you need.",
    )
    h.copy(["crates/ig-app/src/log/worker.rs", "crates/ig-app/src/log/model.rs"], "**The modules.**")
    h.write("crates/ig-app/src/log/mod.rs", "log_mod_model", "**`crates/ig-app/src/log/mod.rs`** adds `model` and `worker`")
    h.write(
        "crates/ig-app/tests/ui.rs",
        "ui_tests_model",
        "**`crates/ig-app/tests/ui.rs`**, the headless UI tests on `TestAppContext`. The tool-window test is added in the next task",
    )
    t.append(h)

    i = Task(
        "`ig-app`: Log view, project window, welcome window, menus, `main`",
        "feat(ig-app): Log table, project and welcome windows, menus; ideagit runs",
        "`view.rs` virtualized table, keyboard and multi-select, arrow hover/click; `project.rs` dock wiring, refresh on focus, worker-panic notification; `welcome.rs` git-missing banner and recent projects; window lifecycle in `app.rs`",
        "`app`, `menus`, `project` and `welcome` import each other, so they land together. After this task "
        "`cargo run --locked -p ig-app -- .` opens IdeaGit on this repository.",
    )
    i.copy(
        [
            "crates/ig-app/src/log/view.rs",
            "crates/ig-app/src/log/mod.rs",
            "crates/ig-app/src/app.rs",
            "crates/ig-app/src/menus.rs",
            "crates/ig-app/src/project.rs",
            "crates/ig-app/src/welcome.rs",
            "crates/ig-app/tests/ui.rs",
        ],
        "**The modules**, the final `log/mod.rs`, and the final `tests/ui.rs` (adds `git_tool_window_toggles`).",
    )
    i.write("crates/ig-app/src/lib.rs", "app_lib_view", "**`crates/ig-app/src/lib.rs`**: everything but `screenshots`")
    i.write("crates/ig-app/src/main.rs", "app_main_view", "**`crates/ig-app/src/main.rs`**, without the `--screenshots` switch")
    i.run(
        "cargo run --locked -p ig-app -- .",
        "**Smoke-run it (macOS, by hand if you can).** Expected: a project window with the Git tool window open on "
        "the Log tab, showing this repository's history with the graph and ref labels. Close it. Skip this step in a "
        "headless environment and say so in the report.",
    )
    t.append(i)

    j = Task(
        "Screenshots",
        "feat(ig-app): headless Log screenshots (cargo xtask screenshots)",
        "`screenshots.rs` waits for the worker with a timeout and never hangs; feature-gated so default builds don't pull test support",
    )
    j.copy(["crates/ig-app/src/screenshots.rs", "crates/ig-app/src/lib.rs", "crates/ig-app/src/main.rs"], "**The module and the final `lib.rs` and `main.rs`.**")
    j.copy(["docs/ui-reference"], "**Reference screenshots** (PNG; copy from the branch).", embed=False)
    j.run(
        "cargo xtask screenshots --out $CLAUDE_JOB_DIR/tmp/shots   # or any scratch dir",
        "**Render once (macOS, by hand).** Expected: `ideagit-log-light.png` and `ideagit-log-dark.png` that look "
        "like `docs/ui-reference/ideagit-log-{light,dark}.png`. Needs a GPU; skip it in a headless environment and say so.",
    )
    t.append(j)

    k = Task(
        "Module map, README, manual checklist, full verification",
        "docs: upstream map, README and manual checklist for the app shell",
        None,
    )
    k.copy(["reference/upstream-map.toml"], "**`reference/upstream-map.toml`** adds the `ig-ui-kit`, `ig-app` and `xtask` entries and the deviation lines for `is_terminal` and `silent_command`.")
    k.write("README.md", "readme", "**`README.md`**: status, the new crates, and run and build instructions")
    k.write("docs/app-shell-checklist.md", "checklist", "**`docs/app-shell-checklist.md`**, the manual checklist from the spec")
    t.append(k)
    return t


# ---------------------------------------------------------------------------------------------
# apply
# ---------------------------------------------------------------------------------------------

def apply(target, n):
    task = tasks()[n - 1]
    m = mids()
    for s in task.steps:
        if s[0] == "copy":
            for p in expand(s[1]):
                dst = os.path.join(target, p)
                os.makedirs(os.path.dirname(dst) or ".", exist_ok=True)
                shutil.copyfile(os.path.join(R, p), dst)
        elif s[0] == "write":
            dst = os.path.join(target, s[1])
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            open(dst, "w", encoding="utf-8").write(m[s[2]])


# ---------------------------------------------------------------------------------------------
# plan
# ---------------------------------------------------------------------------------------------

def fence(p):
    if p.endswith(".rs"):
        return "rust"
    if p.endswith(".toml") or p == "Cargo.lock":
        return "toml"
    if p.endswith((".yml", ".yaml")):
        return "yaml"
    if p.endswith(".json"):
        return "json"
    if p.endswith((".xml", ".svg")):
        return "xml"
    if p.endswith(".md"):
        return "markdown"
    return "text"


def block(lang, body):
    ticks = "````" if "```" in body else "```"
    return f"{ticks}{lang}\n{body if body.endswith(chr(10)) else body + chr(10)}{ticks}\n"


def shq(p):
    return "'" + p + "'" if re.search(r"[^\w./-]", p) else p


def render_task(i, task, m, counts):
    o = [f"### Task {i}: {task.title}\n\n"]
    files = task.touched()
    new = [p for p in files if subprocess.run(["git", "cat-file", "-e", f"HEAD:{p}"], cwd=os.environ["IG_MASTER"], capture_output=True).returncode != 0]
    changed = [p for p in files if p not in new]
    o.append("**Files:**\n")
    if new:
        o.append("- Create: " + ", ".join(f"`{p}`" for p in compact(new)) + "\n")
    if changed:
        o.append("- Modify: " + ", ".join(f"`{p}`" for p in compact(changed)) + "\n")
    o.append("\n")
    if task.review:
        o.append(f"**Review focus:** {task.review}.\n\n")
    if task.intro:
        o.append(task.intro + "\n\n")
    step = 1
    verbatim = []
    for s in task.steps:
        if s[0] == "copy":
            paths = expand(s[1])
            verbatim += s[1]
            if s[4]:
                continue
            o.append(f"- [ ] {label(step, s[2])}\n\n")
            o.append(block("bash", f"git checkout {PROTO_REF} -- " + " ".join(shq(p) for p in s[1])))
            o.append("\n")
            if s[3]:
                for p in paths:
                    if is_binary(p):
                        o.append(f"`{p}`: binary, from the branch.\n\n")
                    else:
                        o.append(f"`{p}`:\n\n" + block(fence(p), rd(p)) + "\n")
            step += 1
        elif s[0] == "write":
            o.append(f"- [ ] {label(step, s[3] + '. Write exactly:')}\n\n")
            o.append(block(fence(s[1]), m[s[2]]) + "\n")
            step += 1
        elif s[0] == "run":
            o.append(f"- [ ] {label(step, s[2])}\n\n" + block("bash", s[1]) + "\n")
            step += 1
        elif s[0] == "note":
            o.append(s[1] + "\n\n")
    o.append(f"- [ ] **Step {step}: Check that the copies are exact**\n\n")
    o.append(block("bash", f"git add -A\ngit diff --cached --exit-code {PROTO_REF} -- " + " ".join(shq(p) for p in verbatim)))
    o.append("\nExpected: no output. The intermediate files written above differ from the branch until their final task.\n\n")
    step += 1
    c = counts.get(str(i)) if counts else None
    exp = f"all clean; `cargo test` reports {c['passed']} passed, {c['ignored']} ignored" if c else "all clean"
    o.append(f"- [ ] **Step {step}: Verify**\n\n" + block("bash", CHECK) + f"\nExpected: {exp}. Fix the source, never the expected values of ported tests.\n\n")
    step += 1
    o.append(f"- [ ] **Step {step}: Commit**\n\n" + block("bash", f"git add -A\ngit commit -m \"{task.commit}\"") + "\n---\n\n")
    return "".join(o)


def label(step, text):
    """`**X.** rest` becomes `**Step n: X.** rest`."""
    if text.startswith("**"):
        end = text.index("**", 2)
        return f"**Step {step}: {text[2:end]}**{text[end + 2:]}"
    return f"**Step {step}:** {text}"


def compact(paths):
    """Collapses long lists of files under one directory into `dir/` entries."""
    out, seen = [], set()
    for p in paths:
        for d in ("assets/icons", "assets/fonts", "xtask/tests/fixtures", "docs/ui-reference"):
            if p.startswith(d + "/"):
                if d not in seen:
                    seen.add(d)
                    out.append(d + "/**")
                break
        else:
            out.append(p)
    return out


def plan(out, counts):
    m = mids()
    head = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "gen4_head.md"), encoding="utf-8").read()
    body = "".join(render_task(i, t, m, counts) for i, t in enumerate(tasks(), start=1))
    open(out, "w", encoding="utf-8").write(head + "\n---\n\n" + body)
    print("tasks", len(tasks()), "lines", sum(1 for _ in open(out, encoding="utf-8")))


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "count":
        R = os.getcwd()
        print(len(tasks()))
    elif cmd == "apply":
        R = os.path.abspath(sys.argv[2])
        apply(os.path.abspath(sys.argv[3]), int(sys.argv[4]))
    elif cmd == "plan":
        R = os.path.abspath(sys.argv[2])
        counts = json.load(open(sys.argv[4])) if len(sys.argv) > 4 else None
        plan(sys.argv[3], counts)
