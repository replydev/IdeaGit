//! Minimal reader for the `[remote "..."]` sections of `.git/config`, enough for branch parsing.
//! Port target: `plugins/git4idea/backend/src/repo/GitConfig.java` (`parseRemotes`).
//!
//! Not yet ported: `include`/`includeIf`, `url.<base>.insteadOf`/`pushInsteadOf`, branch tracking info.
//! These arrive with the push/pull plan.

use std::path::Path;

use crate::error::Result;

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

/// Reads remotes from a git config file. A missing file yields no remotes.
pub fn read_remotes(config_file: &Path) -> Result<Vec<GitRemote>> {
    if !config_file.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(config_file)?;
    Ok(parse_remotes(&content))
}

pub fn parse_remotes(content: &str) -> Vec<GitRemote> {
    let mut remotes: Vec<GitRemote> = Vec::new();
    let mut current: Option<usize> = None;
    for raw in content.lines() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            current = None;
            if let Some((section, Some(sub))) = parse_section_header(line) {
                if section == "remote" {
                    let idx = match remotes.iter().position(|r| r.name == sub) {
                        Some(i) => i,
                        None => {
                            remotes.push(GitRemote::new(&sub));
                            remotes.len() - 1
                        }
                    };
                    current = Some(idx);
                }
            }
            continue;
        }
        let Some(idx) = current else { continue };
        let (key, value) = match line.split_once('=') {
            Some((k, v)) => (k.trim().to_ascii_lowercase(), unquote(v.trim())),
            None => (line.to_ascii_lowercase(), "true".to_string()),
        };
        let remote = &mut remotes[idx];
        match key.as_str() {
            "url" => remote.urls.push(value),
            "pushurl" => remote.push_urls.push(value),
            "fetch" => remote.fetch_refspecs.push(value),
            "push" => remote.push_refspecs.push(value),
            _ => {}
        }
    }
    remotes
}

/// `[remote "origin"]` → `("remote", Some("origin"))`; `[core]` → `("core", None)`;
/// deprecated `[remote.origin]` → `("remote", Some("origin"))`.
fn parse_section_header(line: &str) -> Option<(String, Option<String>)> {
    let inner = line.strip_prefix('[')?.split(']').next()?.trim();
    if let Some(q) = inner.find('"') {
        let section = inner[..q].trim().to_ascii_lowercase();
        let sub = inner[q + 1..]
            .trim_end_matches('"')
            .replace("\\\"", "\"")
            .replace("\\\\", "\\");
        return Some((section, Some(sub)));
    }
    match inner.split_once('.') {
        Some((s, sub)) => Some((s.to_ascii_lowercase(), Some(sub.to_ascii_lowercase()))),
        None => Some((inner.to_ascii_lowercase(), None)),
    }
}

fn strip_comment(line: &str) -> &str {
    let mut in_quotes = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            '#' | ';' if !in_quotes => return &line[..i],
            _ => {}
        }
    }
    line
}

fn unquote(v: &str) -> String {
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_remotes_and_ignores_other_sections() {
        let cfg = "[core]\n\tbare = false\n[remote \"origin\"]\n\turl = git@h:o.git ; comment\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n[branch \"master\"]\n\tremote = origin\n[remote \"up\"]\n\turl = \"https://h/u.git\"\n\tpushurl = ssh://h/u.git\n";
        let remotes = parse_remotes(cfg);
        assert_eq!(remotes.len(), 2);
        assert_eq!(remotes[0].name, "origin");
        assert_eq!(remotes[0].urls, ["git@h:o.git"]);
        assert_eq!(
            remotes[0].fetch_refspecs,
            ["+refs/heads/*:refs/remotes/origin/*"]
        );
        assert_eq!(remotes[1].urls, ["https://h/u.git"]);
        assert_eq!(remotes[1].push_urls, ["ssh://h/u.git"]);
    }
}
