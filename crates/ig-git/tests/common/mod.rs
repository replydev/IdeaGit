//! Helpers for integration tests that run the real `git` executable.
#![allow(dead_code)] // each test crate uses a different subset

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use ig_git::console::{ConsoleSink, NullConsole};
use ig_git::handler::GitHandler;
use ig_git::runner::Git;

/// An empty file used as the global git config, so the developer's config can't affect tests.
fn empty_global_config() -> PathBuf {
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("empty-gitconfig");
    if !path.exists() {
        std::fs::write(&path, "").unwrap();
    }
    path
}

/// Inherited variables that would redirect git away from the test repository.
pub const SCRUBBED_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
];

/// Runs git in `dir` with a fixed identity and no global/system config; returns the raw output.
pub fn git_output(dir: &Path, args: &[&str]) -> std::process::Output {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", empty_global_config())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com");
    for v in SCRUBBED_ENV {
        cmd.env_remove(v);
    }
    cmd.output().expect("git is installed")
}

/// Applies the same isolation as [`git_output`] (no global/system config, fixed identity) to a
/// handler run through the library runner.
pub fn isolate(h: &mut GitHandler) {
    let config = empty_global_config();
    h.add_custom_environment_variable("GIT_CONFIG_GLOBAL", config.to_str().expect("utf-8 path"));
    h.add_custom_environment_variable("GIT_CONFIG_NOSYSTEM", "1");
    h.add_custom_environment_variable("GIT_AUTHOR_NAME", "Test");
    h.add_custom_environment_variable("GIT_AUTHOR_EMAIL", "test@example.com");
    h.add_custom_environment_variable("GIT_COMMITTER_NAME", "Test");
    h.add_custom_environment_variable("GIT_COMMITTER_EMAIL", "test@example.com");
    for v in SCRUBBED_ENV {
        h.remove_inherited_environment_variable(v);
    }
}

/// Like [`git_output`], panicking on failure; returns stdout.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = git_output(dir, args);
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `git init -b main` with one commit of `a.txt`.
pub fn init_repo(dir: &Path) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("a.txt"), "one\n").unwrap();
    git(dir, &["add", "a.txt"]);
    git(dir, &["commit", "-q", "-m", "initial"]);
}

pub fn runner() -> Git {
    runner_with(Arc::new(NullConsole))
}

pub fn runner_with(console: Arc<dyn ConsoleSink>) -> Git {
    let config = empty_global_config();
    Git::detect("git", console)
        .expect("git is installed and detectable")
        .with_base_environment(
            &[
                ("GIT_CONFIG_GLOBAL", config.to_str().expect("utf-8 path")),
                ("GIT_CONFIG_NOSYSTEM", "1"),
                ("GIT_AUTHOR_NAME", "Test"),
                ("GIT_AUTHOR_EMAIL", "test@example.com"),
                ("GIT_COMMITTER_NAME", "Test"),
                ("GIT_COMMITTER_EMAIL", "test@example.com"),
            ],
            SCRUBBED_ENV,
        )
}
