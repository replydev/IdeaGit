//! Port of `plugins/git4idea/backend/src/config/GitConfigUtil.java` (`getValues`) and the remote part
//! of `plugins/git4idea/backend/src/repo/GitConfig.kt` (`read`, `parseSectionNameAndVariable`,
//! `createRemote`, and the `urls.isNotEmpty()` filter of `mapConfiguredRemotes`).
//!
//! As upstream, git itself parses the configuration (`git config --null -l`), so includes, escapes,
//! continuation lines and all config scopes behave exactly as in git.
//!
//! Not yet ported: `url.<base>.insteadOf`/`pushInsteadOf` substitution, branch tracking, hooks,
//! core settings and object format (they arrive with the push/pull plan).

use std::path::Path;

use crate::command::GitCommand;
use crate::error::Result;
use crate::runner::Git;

const GIT_REMOTE_SECTION: &str = "remote";
const SVN_REMOTE_SECTION: &str = "svn-remote";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRemote {
    pub name: String,
    pub urls: Vec<String>,
    pub push_urls: Vec<String>,
    pub fetch_refspecs: Vec<String>,
    pub push_refspecs: Vec<String>,
}

impl GitRemote {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            urls: Vec::new(),
            push_urls: Vec::new(),
            fetch_refspecs: Vec::new(),
            push_refspecs: Vec::new(),
        }
    }
}

/// Config values by key, in order of first appearance (upstream uses a `LinkedHashMap`).
/// Keys are as git prints them: section and variable names lowercased, subsections verbatim.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigValues {
    entries: Vec<(String, Vec<String>)>,
}

impl ConfigValues {
    pub fn get(&self, key: &str) -> Option<&[String]> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_slice())
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }

    fn push(&mut self, key: &str, value: &str) {
        match self.entries.iter_mut().find(|(k, _)| k == key) {
            Some((_, values)) => values.push(value.to_string()),
            None => self
                .entries
                .push((key.to_string(), vec![value.to_string()])),
        }
    }
}

/// Parses `git config --null` output: records of `key\nvalue\0`.
///
/// Deviation: a key printed without value (`key\0`, a valueless boolean) is read as `true`; upstream's
/// parser would merge it into the next record.
pub fn parse_null_config_output(output: &str) -> ConfigValues {
    let mut result = ConfigValues::default();
    let mut rest = output;
    while !rest.is_empty() {
        let Some(nul) = rest.find('\0') else { break };
        let record = &rest[..nul];
        match record.split_once('\n') {
            Some((key, value)) => result.push(key, value),
            None => result.push(record, "true"),
        }
        rest = &rest[nul + 1..];
    }
    result
}

/// Upstream `GitConfigUtil.getValues(project, root, null)`: all values visible from `root`.
pub fn get_values(git: &Git, root: &Path) -> Result<ConfigValues> {
    let mut h = git.handler(root, GitCommand::CONFIG);
    h.set_silent(true);
    h.add_parameters(["--null", "-l"]);
    let output = git.run_command(h).output_or_error()?;
    Ok(parse_null_config_output(&output))
}

/// Upstream `GitConfig.read(project, root).parseRemotes()` without url substitutions: remotes (and
/// svn-remotes) that have at least one url. Like upstream, a failure to read the config yields none.
pub fn read_remotes(git: &Git, root: &Path) -> Vec<GitRemote> {
    get_values(git, root)
        .map(|v| remotes_from_values(&v))
        .unwrap_or_default()
}

pub fn remotes_from_values(values: &ConfigValues) -> Vec<GitRemote> {
    let mut sections: Vec<(&str, &str)> = Vec::new();
    for key in values.keys() {
        if let Some(sv) = parse_section_name_and_variable(key)
            && !sections.contains(&sv)
        {
            sections.push(sv);
        }
    }
    sections
        .into_iter()
        .filter(|(section, _)| *section == GIT_REMOTE_SECTION || *section == SVN_REMOTE_SECTION)
        .map(|(section, name)| create_remote(values, section, name))
        .filter(|remote| !remote.urls.is_empty())
        .collect()
}

/// `remote.origin.url` → `("remote", "origin")`; `remote.team.a.url` → `("remote", "team.a")`.
fn parse_section_name_and_variable(key: &str) -> Option<(&str, &str)> {
    let first = key.find('.')?;
    let last = key.rfind('.')?;
    (first != last).then(|| (&key[..first], &key[first + 1..last]))
}

fn create_remote(values: &ConfigValues, section: &str, name: &str) -> GitRemote {
    let get = |variable: &str| {
        values
            .get(&format!("{section}.{name}.{variable}"))
            .map(<[String]>::to_vec)
            .unwrap_or_default()
    };
    GitRemote {
        name: name.to_string(),
        urls: get("url"),
        push_urls: get("pushurl"),
        fetch_refspecs: get("fetch"),
        push_refspecs: get("push"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_null_output_with_multiline_and_valueless_entries() {
        let out = "core.bare\nfalse\0core.flag\0remote.origin.url\nu1\0remote.origin.url\nu2\0alias.x\nline1\nline2\0";
        let v = parse_null_config_output(out);
        assert_eq!(v.get("core.bare"), Some(&["false".to_string()][..]));
        assert_eq!(v.get("core.flag"), Some(&["true".to_string()][..]));
        assert_eq!(v.get("remote.origin.url").map(<[String]>::len), Some(2));
        assert_eq!(v.get("alias.x"), Some(&["line1\nline2".to_string()][..]));
        assert_eq!(
            v.keys().collect::<Vec<_>>(),
            ["core.bare", "core.flag", "remote.origin.url", "alias.x"]
        );
    }

    #[test]
    fn builds_remotes_like_upstream() {
        let out = "remote.origin.url\ngit@h:o.git\0remote.origin.fetch\n+refs/heads/*:refs/remotes/origin/*\0\
                   remote.NoUrl.fetch\n+refs/heads/*:refs/remotes/NoUrl/*\0\
                   remote.team.a.url\nhttps://h/a.git\0remote.team.a.pushurl\nssh://h/a.git\0\
                   svn-remote.svn.url\nsvn://h/r\0branch.main.remote\norigin\0";
        let remotes = remotes_from_values(&parse_null_config_output(out));
        let names: Vec<&str> = remotes.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            ["origin", "team.a", "svn"],
            "url-less remotes are skipped"
        );
        assert_eq!(
            remotes[0].fetch_refspecs,
            ["+refs/heads/*:refs/remotes/origin/*"]
        );
        assert_eq!(remotes[1].push_urls, ["ssh://h/a.git"]);
    }
}
