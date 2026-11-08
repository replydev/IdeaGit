//! Unquoting paths printed by git.
//!
//! Port of `platform/vcs-impl/src/com/intellij/vcsUtil/VcsFileUtil.java#unescapeGitPath` (used by
//! `GitUtil.unescapePath`). Octal escapes are UTF-8 bytes (upstream: the default charset).

/// An escape sequence is unterminated or unknown (upstream `IllegalArgumentException`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct UnescapeError(pub String);

/// Strips surrounding quotes and decodes `\\`, `\t`, `\n`, `\r`, `\a`, `\b`, `\f`, `\"` and octal
/// byte sequences.
pub fn unescape_git_path(path: &str) -> Result<String, UnescapeError> {
    let original = path;
    let path = if path.len() >= 2 && path.starts_with('"') && path.ends_with('"') {
        &path[1..path.len() - 1]
    } else if path == "\"" {
        // upstream: `startsWith` and `endsWith` both hold for a lone quote; substring(1, 0) throws
        return Err(UnescapeError(format!("Invalid quoted path: {original}")));
    } else {
        path
    };
    if !path.contains('\\') {
        return Ok(path.to_string());
    }

    let chars: Vec<char> = path.chars().collect();
    let l = chars.len();
    let is_octal = |c: char| ('0'..='7').contains(&c);
    let mut rc = String::with_capacity(path.len());
    let mut i = 0;
    while i < l {
        let c = chars[i];
        if c != '\\' {
            rc.push(c);
            i += 1;
            continue;
        }
        i += 1;
        if i >= l {
            return Err(UnescapeError(format!(
                "Unterminated escape sequence in the path: {path}"
            )));
        }
        let e = chars[i];
        match e {
            '\\' => rc.push('\\'),
            't' => rc.push('\t'),
            'n' => rc.push('\n'),
            'r' => rc.push('\r'),
            'a' => rc.push('\u{7}'),
            'b' => rc.push('\u{8}'),
            'f' => rc.push('\u{c}'),
            '"' => rc.push('"'),
            _ if is_octal(e) => {
                // a sequence of octal escapes is one byte array
                let mut bytes = Vec::new();
                while i < l {
                    if is_octal(chars[i]) {
                        let mut code: u32 = 0;
                        let mut k = 0;
                        while k < 3 && i < l && is_octal(chars[i]) {
                            code = code * 8 + (chars[i] as u32 - '0' as u32);
                            i += 1;
                            k += 1;
                        }
                        bytes.push(code as u8);
                    }
                    if i + 1 >= l || chars[i] != '\\' || !is_octal(chars[i + 1]) {
                        break;
                    }
                    i += 1;
                }
                rc.push_str(&String::from_utf8_lossy(&bytes));
                continue;
            }
            other => {
                return Err(UnescapeError(format!(
                    "Unknown escape sequence '\\{other}' in the path: {path}"
                )));
            }
        }
        i += 1;
    }
    Ok(rc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescapes_quoted_paths() {
        assert_eq!(unescape_git_path("plain.txt").unwrap(), "plain.txt");
        assert_eq!(
            unescape_git_path("\"a\\tb\\\\c\\\"d\"").unwrap(),
            "a\tb\\c\"d"
        );
        assert_eq!(
            unescape_git_path("\"\\320\\264\\320\\260.txt\"").unwrap(),
            "да.txt"
        );
        assert_eq!(unescape_git_path("\"x\\303\\251y\"").unwrap(), "xéy");
        assert!(unescape_git_path("\"a\\\"").is_err());
        assert!(unescape_git_path("a\\q").is_err());
    }
}
