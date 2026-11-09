//! UI strings from the upstream bundles (`assets/strings/*.properties`, written by
//! `cargo xtask import-ui`), with the `java.text.MessageFormat` subset they use.
//!
//! Supported patterns: `{0}`, `{0,number}` and `{0,choice,limit#text|...}` (`ChoiceFormat`, whose
//! texts may contain `{0,number}`); `''` is a literal quote. Properties escapes `\uXXXX`, `\n`,
//! `\t`, `\\` and line continuations are decoded.

use std::collections::HashMap;
use std::sync::OnceLock;

static BUNDLES: &[(&str, &str)] = &[
    (
        "VcsLogBundle",
        include_str!("../../../assets/strings/VcsLogBundle.properties"),
    ),
    (
        "GitBundle",
        include_str!("../../../assets/strings/GitBundle.properties"),
    ),
    (
        "VcsBundle",
        include_str!("../../../assets/strings/VcsBundle.properties"),
    ),
    (
        "UtilBundle",
        include_str!("../../../assets/strings/UtilBundle.properties"),
    ),
];

fn bundles() -> &'static HashMap<(&'static str, String), String> {
    static MAP: OnceLock<HashMap<(&'static str, String), String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = HashMap::new();
        for (bundle, text) in BUNDLES {
            for (key, value) in parse_properties(text) {
                map.insert((*bundle, key), value);
            }
        }
        map
    })
}

/// The raw text of `key` in `bundle`; the key itself if missing (as upstream's `!key!` fallback
/// would hide less). `import-ui` fails on missing keys, so this only happens for typos.
pub fn text(bundle: &str, key: &str) -> String {
    bundles()
        .iter()
        .find(|((b, k), _)| *b == bundle && k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| key.to_string())
}

/// `Bundle.message(key, args...)`.
pub fn message(bundle: &str, key: &str, args: &[&str]) -> String {
    format_message(&text(bundle, key), args)
}

/// Parses `.properties` text: `key=value` / `key: value` lines, `#`/`!` comments.
pub fn parse_properties(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut logical = String::new();
    for line in text.lines() {
        let line = line.trim_start();
        if logical.is_empty() && (line.is_empty() || line.starts_with('#') || line.starts_with('!'))
        {
            continue;
        }
        let continued = line.ends_with('\\') && !line.ends_with("\\\\");
        logical.push_str(if continued {
            &line[..line.len() - 1]
        } else {
            line
        });
        if continued {
            continue;
        }
        let entry = std::mem::take(&mut logical);
        let split = entry
            .char_indices()
            .find(|&(i, c)| (c == '=' || c == ':') && !entry[..i].ends_with('\\'))
            .map(|(i, _)| i)
            .unwrap_or(entry.len());
        let key = unescape(entry[..split].trim_end());
        let value = unescape(entry.get(split + 1..).unwrap_or("").trim_start());
        out.push((key, value));
    }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// `MessageFormat.format` for the supported subset.
pub fn format_message(pattern: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(i) = rest.find(['{', '\'']) {
        out.push_str(&rest[..i]);
        if rest[i..].starts_with("''") {
            out.push('\'');
            rest = &rest[i + 2..];
            continue;
        }
        if rest[i..].starts_with('\'') {
            out.push('\'');
            rest = &rest[i + 1..];
            continue;
        }
        // find the matching close brace (choice formats nest `{0,number}`)
        let mut depth = 0;
        let mut end = None;
        for (j, c) in rest[i..].char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i + j);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            out.push_str(&rest[i..]);
            return out;
        };
        out.push_str(&format_argument(&rest[i + 1..end], args));
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn format_argument(spec: &str, args: &[&str]) -> String {
    let mut parts = spec.splitn(3, ',');
    let index: usize = parts
        .next()
        .unwrap_or("")
        .trim()
        .parse()
        .unwrap_or(usize::MAX);
    let arg = args.get(index).copied().unwrap_or("");
    match (parts.next().map(str::trim), parts.next()) {
        (Some("choice"), Some(choices)) => {
            let n: f64 = arg.parse().unwrap_or(0.0);
            let mut chosen = None;
            for choice in choices.split('|') {
                let Some((limit, text)) = choice.split_once(['#', '<']) else {
                    continue;
                };
                let limit: f64 = limit.trim().parse().unwrap_or(f64::MAX);
                if chosen.is_none() || n >= limit {
                    chosen = Some(text);
                }
                if n < limit {
                    break;
                }
            }
            format_message(chosen.unwrap_or(""), args)
        }
        _ => arg.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_format_picks_the_last_reached_limit() {
        let pattern = "{0,choice, 0#Moments|1#A minute|2#{0,number} minutes|60#1 hour} ago";
        assert_eq!(format_message(pattern, &["0"]), "Moments ago");
        assert_eq!(format_message(pattern, &["1"]), "A minute ago");
        assert_eq!(format_message(pattern, &["42"]), "42 minutes ago");
        assert_eq!(format_message(pattern, &["61"]), "1 hour ago");
    }

    #[test]
    fn placeholders_and_quotes() {
        assert_eq!(
            format_message("Git {0} isn''t supported", &["1.0"]),
            "Git 1.0 isn't supported"
        );
    }

    #[test]
    fn properties_escapes_and_continuations() {
        let parsed = parse_properties("# c\na=b\\u00e9\nlong=x\\\n   y\nk\\=x : v\n");
        assert_eq!(
            parsed,
            vec![
                ("a".to_string(), "bé".to_string()),
                ("long".to_string(), "xy".to_string()),
                ("k=x".to_string(), "v".to_string()),
            ]
        );
    }
}
