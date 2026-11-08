//! Port of the command-line and environment parts of
//! `plugins/git4idea/backend/src/commands/GitHandler.java` and `GitLineHandler.java`.
//!
//! Not ported: WSL/Eel executables, Windows `.cmd` parameter escaping, `VcsEnvCustomizer` extensions.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::command::{GitCommand, LockingPolicy};

/// Marker env var that lets git hooks detect IdeaGit.
pub const IDEAGIT_HANDLER_MARKER_ENV: &str = "IDEAGIT_GIT_EXECUTABLE";
/// Upstream's marker (`GitCommand.IJ_HANDLER_MARKER_ENV`), also set so hooks written for IntelliJ
/// behave the same way under IdeaGit.
pub const IJ_HANDLER_MARKER_ENV: &str = "INTELLIJ_GIT_EXECUTABLE";

/// Port of `VcsLocaleHelper.getDefaultLocaleEnvironmentVars("git")`.
pub fn locale_environment() -> [(&'static str, &'static str); 2] {
    [("LANGUAGE", ""), ("LC_ALL", "en_US.UTF-8")]
}

#[derive(Debug, Clone)]
pub struct GitHandler {
    executable: PathBuf,
    working_dir: PathBuf,
    command: GitCommand,
    /// Everything after the executable: `-c` pairs, the command name, then its arguments.
    parameters: Vec<String>,
    custom_env: Vec<(String, String)>,
    removed_env: Vec<String>,
    silent: bool,
    stdout_suppressed: bool,
    stderr_suppressed: bool,
    input: Option<Vec<u8>>,
}

impl GitHandler {
    pub fn new(
        working_dir: impl Into<PathBuf>,
        executable: impl Into<PathBuf>,
        command: GitCommand,
    ) -> Self {
        Self::with_config(working_dir, executable, command, &[])
    }

    /// `config_parameters` are passed as `-c <param>` before the command name, after the defaults.
    pub fn with_config(
        working_dir: impl Into<PathBuf>,
        executable: impl Into<PathBuf>,
        command: GitCommand,
        config_parameters: &[&str],
    ) -> Self {
        let mut parameters = Vec::new();
        for p in default_config_parameters(config_parameters) {
            parameters.push("-c".to_string());
            parameters.push(p);
        }
        parameters.push(command.name().to_string());
        Self {
            executable: executable.into(),
            working_dir: working_dir.into(),
            command,
            parameters,
            custom_env: Vec::new(),
            removed_env: Vec::new(),
            // Upstream: non-write commands are not logged to the console by default.
            silent: command.locking_policy() != LockingPolicy::Write,
            stdout_suppressed: true,
            stderr_suppressed: false,
            input: None,
        }
    }

    pub fn command(&self) -> GitCommand {
        self.command
    }

    pub fn working_dir(&self) -> &Path {
        &self.working_dir
    }

    pub fn parameters(&self) -> &[String] {
        &self.parameters
    }

    pub fn add_parameters<I, S>(&mut self, params: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.parameters.extend(params.into_iter().map(Into::into));
    }

    /// Adds `--`, separating options from paths.
    pub fn end_options(&mut self) {
        self.parameters.push("--".to_string());
    }

    /// Adds paths relative to the working directory, with `/` separators.
    pub fn add_relative_paths<P: AsRef<Path>>(&mut self, paths: &[P]) {
        for p in paths {
            let rel = relative_path(&self.working_dir, p.as_ref());
            self.parameters.push(rel);
        }
    }

    /// Silent commands are not logged to the Git console. Like upstream `setSilent`, making a
    /// handler silent also suppresses both stdout and stderr copying.
    pub fn set_silent(&mut self, silent: bool) {
        self.silent = silent;
        if silent {
            self.stdout_suppressed = true;
            self.stderr_suppressed = true;
        }
    }
    pub fn is_silent(&self) -> bool {
        self.silent
    }
    pub fn set_stdout_suppressed(&mut self, suppressed: bool) {
        self.stdout_suppressed = suppressed;
    }
    pub fn is_stdout_suppressed(&self) -> bool {
        self.stdout_suppressed
    }
    pub fn set_stderr_suppressed(&mut self, suppressed: bool) {
        self.stderr_suppressed = suppressed;
    }
    pub fn is_stderr_suppressed(&self) -> bool {
        self.stderr_suppressed
    }

    /// Bytes written to the command's stdin, which is then closed (upstream `setInputProcessor`).
    ///
    /// The input is buffered fully, whereas upstream streams it through a
    /// `ThrowableConsumer<OutputStream>`. Stdin is written on its own thread, so large input
    /// can't deadlock against full output pipes.
    pub fn set_input(&mut self, input: Vec<u8>) {
        self.input = Some(input);
    }

    pub fn input(&self) -> Option<&[u8]> {
        self.input.as_deref()
    }

    /// Adds or replaces an environment variable for this command.
    pub fn add_custom_environment_variable(&mut self, name: &str, value: &str) {
        if let Some(entry) = self.custom_env.iter_mut().find(|(k, _)| k == name) {
            entry.1 = value.to_string();
        } else {
            self.custom_env.push((name.to_string(), value.to_string()));
        }
    }

    /// Removes an inherited environment variable from the command's environment.
    pub fn remove_inherited_environment_variable(&mut self, name: &str) {
        if !self.removed_env.iter().any(|n| n == name) {
            self.removed_env.push(name.to_string());
        }
    }

    /// Upstream `containsCustomEnvironmentVariable`.
    pub fn contains_custom_environment_variable(&self, name: &str) -> bool {
        self.custom_env.iter().any(|(k, _)| k == name)
    }

    /// The environment applied on top of the inherited process environment, in application order
    /// (upstream `prepareEnvironment`: locale, then custom variables, then the marker).
    pub fn environment(&self) -> Vec<(String, String)> {
        let mut env: Vec<(String, String)> = locale_environment()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        env.extend(self.custom_env.iter().cloned());
        env.push((IJ_HANDLER_MARKER_ENV.to_string(), "true".to_string()));
        env.push((IDEAGIT_HANDLER_MARKER_ENV.to_string(), "true".to_string()));
        env
    }

    /// The command line as shown in the Git console, e.g. `git -c core.quotepath=false ... status`.
    pub fn printable_command_line(&self) -> String {
        std::iter::once("git".to_string())
            .chain(self.parameters.iter().map(|p| quote_if_needed(p)))
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn build_command(&self) -> Command {
        let mut cmd = Command::new(&self.executable);
        cmd.args(&self.parameters).current_dir(&self.working_dir);
        for name in &self.removed_env {
            cmd.env_remove(name);
        }
        for (k, v) in self.environment() {
            cmd.env(k, v);
        }
        cmd
    }
}

fn default_config_parameters(requested: &[&str]) -> Vec<String> {
    let mut params = vec![
        "core.quotepath=false".to_string(),
        "log.showSignature=false".to_string(),
    ];
    params.extend(requested.iter().map(|s| s.to_string()));
    params
}

fn quote_if_needed(p: &str) -> String {
    if p.is_empty() || p.chars().any(char::is_whitespace) {
        format!("\"{}\"", p.replace('"', "\\\""))
    } else {
        p.to_string()
    }
}

/// Port of `VcsFileUtil.relativePath`: `/`-separated path relative to `base`, `.` for `base` itself.
/// Paths outside `base` are passed through unchanged.
pub(crate) fn relative_path(base: &Path, path: &Path) -> String {
    match path.strip_prefix(base) {
        Ok(rel) if rel.as_os_str().is_empty() => ".".to_string(),
        Ok(rel) => rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_default_parameters_and_silence() {
        let mut h = GitHandler::new("/repo", "git", GitCommand::STATUS);
        h.add_parameters(["--porcelain", "-z"]);
        assert_eq!(
            h.parameters(),
            [
                "-c",
                "core.quotepath=false",
                "-c",
                "log.showSignature=false",
                "status",
                "--porcelain",
                "-z"
            ]
        );
        assert!(h.is_silent(), "read commands are silent by default");
        assert!(!GitHandler::new("/repo", "git", GitCommand::COMMIT).is_silent());
        assert!(h.is_stdout_suppressed());
    }

    #[test]
    fn extra_config_and_printable_line() {
        let mut h = GitHandler::with_config("/repo", "git", GitCommand::COMMIT, &["user.name=A B"]);
        h.add_parameters(["-m", "two words"]);
        assert_eq!(
            h.printable_command_line(),
            "git -c core.quotepath=false -c log.showSignature=false -c \"user.name=A B\" commit -m \"two words\""
        );
    }

    #[test]
    fn environment_order_and_override() {
        let mut h = GitHandler::new("/repo", "git", GitCommand::STATUS);
        h.add_custom_environment_variable("GIT_OPTIONAL_LOCKS", "1");
        h.add_custom_environment_variable("GIT_OPTIONAL_LOCKS", "0");
        let env = h.environment();
        assert_eq!(env[0], ("LANGUAGE".into(), "".into()));
        assert_eq!(env[1], ("LC_ALL".into(), "en_US.UTF-8".into()));
        assert_eq!(
            env.iter()
                .filter(|(k, _)| k == "GIT_OPTIONAL_LOCKS")
                .count(),
            1
        );
        assert!(env.contains(&("GIT_OPTIONAL_LOCKS".into(), "0".into())));
        assert!(env.contains(&(IJ_HANDLER_MARKER_ENV.into(), "true".into())));
    }

    #[test]
    fn removed_inherited_variable_is_reported_by_command() {
        let mut h = GitHandler::new("/repo", "git", GitCommand::STATUS);
        h.remove_inherited_environment_variable("GIT_DIR");
        h.remove_inherited_environment_variable("GIT_DIR");
        let cmd = h.build_command();
        let removed: Vec<_> = cmd
            .get_envs()
            .filter(|(k, _)| *k == std::ffi::OsStr::new("GIT_DIR"))
            .collect();
        assert_eq!(removed, [(std::ffi::OsStr::new("GIT_DIR"), None)]);
    }

    #[test]
    fn set_silent_suppresses_output_and_custom_env_lookup() {
        let mut h = GitHandler::new("/repo", "git", GitCommand::COMMIT);
        assert!(!h.is_stderr_suppressed());
        h.set_silent(true);
        assert!(h.is_silent() && h.is_stdout_suppressed() && h.is_stderr_suppressed());
        assert!(!h.contains_custom_environment_variable("GIT_EDITOR"));
        h.add_custom_environment_variable("GIT_EDITOR", "true");
        assert!(h.contains_custom_environment_variable("GIT_EDITOR"));
    }

    #[test]
    fn relative_paths() {
        let base = Path::new("/repo");
        assert_eq!(relative_path(base, Path::new("/repo/a/b.txt")), "a/b.txt");
        assert_eq!(relative_path(base, Path::new("/repo")), ".");
        assert_eq!(relative_path(base, Path::new("/other/x")), "/other/x");
    }
}
