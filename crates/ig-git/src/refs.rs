//! Port of `plugins/git4idea/backend/src/repo/GitRefUtil.java`,
//! `plugins/git4idea/backend/src/validators/GitRefNameValidator.java` (`checkInput`), and
//! `plugins/git4idea/shared/src/com/intellij/vcs/git/ref/GitRefUtil.kt` (`stripRefsPrefix`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::hash::Hash;
use crate::repo_files::try_load_file;

pub const REFS_HEADS_PREFIX: &str = "refs/heads/";
pub const REFS_REMOTES_PREFIX: &str = "refs/remotes/";
pub const REFS_TAGS_PREFIX: &str = "refs/tags/";

static BRANCH_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^ *(?:ref:)? */?((?:refs/heads/|refs/remotes/)?\S+)\s*$").expect("valid regex")
});

pub fn strip_refs_prefix(ref_name: &str) -> &str {
    for prefix in [REFS_HEADS_PREFIX, REFS_REMOTES_PREFIX, REFS_TAGS_PREFIX] {
        if let Some(rest) = ref_name.strip_prefix(prefix) {
            return rest;
        }
    }
    ref_name
}

/// Parses a `<hash> <refname>` line, e.g. from `.git/packed-refs`. Returns `(refname, hash)`.
/// Comments, peeled-tag lines (`^...`) and malformed lines yield `None`.
pub fn parse_refs_line(line: &str) -> Option<(String, String)> {
    let line = line.trim();
    let first = line.chars().next()?;
    if first == '#' || first == '^' {
        return None;
    }
    let (hash_end, sep) = line.char_indices().find(|(_, c)| !c.is_alphanumeric())?;
    let hash = &line[..hash_end];
    if !sep.is_whitespace() {
        return None;
    }
    let start = hash_end + sep.len_utf8();
    let rest = &line[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some((rest[..end].to_string(), hash.trim().to_string()))
}

/// Like [`parse_refs_line`], but only for local and remote branches.
pub fn parse_branches_line(line: &str) -> Option<(String, String)> {
    let (name, hash) = parse_refs_line(line)?;
    if name.starts_with(REFS_HEADS_PREFIX) || name.starts_with(REFS_REMOTES_PREFIX) {
        Some((name, hash))
    } else {
        None
    }
}

/// Target of a symbolic ref value such as `ref: refs/heads/main` (or `ref:main` → `refs/heads/main`).
pub fn get_target(ref_value: &str) -> Option<String> {
    let caps = BRANCH_PATTERN.captures(ref_value)?;
    let target = caps.get(1)?.as_str();
    if target.starts_with(REFS_HEADS_PREFIX) || target.starts_with(REFS_REMOTES_PREFIX) {
        Some(target.to_string())
    } else {
        Some(format!("{REFS_HEADS_PREFIX}{target}"))
    }
}

/// Resolves `refname -> value` where value is a hash or a symbolic ref to another entry.
/// Unresolvable and cyclic symbolic refs are dropped.
pub fn resolve_refs(data: &HashMap<String, String>) -> HashMap<String, Hash> {
    let mut resolved: HashMap<String, Hash> = HashMap::new();
    for (name, value) in data {
        if let Some(hash) = Hash::parse(value) {
            resolved.insert(name.clone(), hash);
        }
    }
    let mut unresolved: Vec<(String, String)> = data
        .iter()
        .filter(|(name, _)| !resolved.contains_key(*name))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    let mut progressed = true;
    while progressed && !unresolved.is_empty() {
        progressed = false;
        unresolved.retain(|(name, value)| match get_target(value) {
            None => false, // unexpected record: dropped (upstream logs a warning)
            Some(link) => match resolved.get(&link).cloned() {
                Some(hash) => {
                    resolved.insert(name.clone(), hash);
                    progressed = true;
                    false
                }
                None => true, // transitive link, try again next round
            },
        });
    }
    resolved
}

/// Reads loose refs under `refs_root_dir` (e.g. `.git/refs/heads`) as `prefix + relative path -> value`.
/// Hidden files and directories (starting with `.`) and invalid ref names are skipped.
pub fn read_from_refs_files(refs_root_dir: &Path, prefix: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    if refs_root_dir.is_dir() {
        collect_refs(refs_root_dir, refs_root_dir, prefix, &mut result);
    }
    result
}

fn collect_refs(root: &Path, dir: &Path, prefix: &str, result: &mut HashMap<String, String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect_refs(root, &path, prefix, result);
            continue;
        }
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let branch_name = format!("{prefix}{rel}");
        if check_ref_name(&branch_name) {
            if let Ok(value) = try_load_file(&path) {
                result.insert(branch_name, value);
            }
        }
    }
}

/// Port of `GitRefNameValidator.checkInput`: true if `name` is a valid git reference name.
pub fn check_ref_name(name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    let starts_bad = name.starts_with('.') || name.starts_with('-') || name.starts_with('/');
    let has_bad_char = name.chars().any(|c| {
        matches!(c, '~' | ':' | '^' | '?' | '*' | '"' | '[' | '\\' | ' ')
            || (c as u32) < 32
            || c == '\u{7f}'
    });
    let bad_sequence = ["@{", "//", "..", "./", "/."]
        .iter()
        .any(|s| name.contains(s));
    let ends_bad = name.ends_with('.') || name.ends_with('/') || name.ends_with(".lock");
    !(starts_bad || has_bad_char || bad_sequence || ends_bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_packed_refs_lines() {
        assert_eq!(
            parse_refs_line("0e1d130689bc52f140c5c374aa9cc2b8916c0ad7 refs/heads/master"),
            Some((
                "refs/heads/master".into(),
                "0e1d130689bc52f140c5c374aa9cc2b8916c0ad7".into()
            ))
        );
        assert_eq!(parse_refs_line("# pack-refs with: peeled"), None);
        assert_eq!(
            parse_refs_line("^eb355ef03aaa139b76c3a938c27458f6ee5ca002"),
            None
        );
        assert_eq!(parse_refs_line(""), None);
        assert_eq!(
            parse_refs_line("0e1d130689bc52f140c5c374aa9cc2b8916c0ad7"),
            None
        );
        assert_eq!(
            parse_branches_line("c5cf9cb49c25899442bd98b77c2e60b41cd8b85d refs/tags/tag1"),
            None
        );
    }

    #[test]
    fn symbolic_targets() {
        assert_eq!(
            get_target("ref: refs/heads/master").as_deref(),
            Some("refs/heads/master")
        );
        assert_eq!(
            get_target("ref:master").as_deref(),
            Some("refs/heads/master")
        );
        assert_eq!(
            get_target("ref: refs/remotes/origin/feature").as_deref(),
            Some("refs/remotes/origin/feature")
        );
    }

    #[test]
    fn resolves_links_and_drops_cycles() {
        let h = "0e1d130689bc52f140c5c374aa9cc2b8916c0ad7";
        let data: HashMap<String, String> = [
            ("refs/heads/master", h),
            ("refs/heads/link", "ref: refs/heads/master"),
            ("refs/heads/transitive", "ref: refs/heads/link"),
            ("refs/heads/c1", "ref: refs/heads/c2"),
            ("refs/heads/c2", "ref: refs/heads/c1"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let resolved = resolve_refs(&data);
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved["refs/heads/transitive"].as_str(), h);
        assert!(!resolved.contains_key("refs/heads/c1"));
    }

    #[test]
    fn ref_name_validation() {
        assert!(check_ref_name("refs/heads/feature/x"));
        for bad in [
            "", " ", ".a", "-a", "/a", "a..b", "a//b", "a/.b", "a./b", "a.lock", "a/", "a.", "a b",
            "a~b", "a@{b", "a\u{1}",
        ] {
            assert!(!check_ref_name(bad), "{bad:?} should be invalid");
        }
    }
}
