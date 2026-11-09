//! `assets/theme.keys` parsing and generation of `ThemeTokens` (`tokens.rs`) plus the
//! `assets/themes/{light,dark}.json` review files. Key lookups use [`crate::theme`].
//!
//! `theme.keys` lines: `[f32 ]<field> = <alt> | <alt> ... [# comment]`. An alternative is a
//! theme `ui` key, `scheme:<OPTION>` (scheme `<colors>`), `scheme:<ATTR>.FOREGROUND` (or another
//! upper-case attribute field), or a literal: `#RRGGBB[AA]`, a float, or `<light>/<dark>` for a
//! `JBColor(light, dark)`-style pair. Literals need a trailing comment saying where upstream
//! hardcodes the value. A comment starts at a `#` that begins the line or is followed by a space.

use std::fmt::Write as _;

use serde_json::{Map, Value as Json};

use crate::theme::{Theme, parse_hex};
use crate::{Error, Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Color,
    F32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Alt {
    Key(String),
    SchemeColor(String),
    SchemeAttribute(String, String),
    /// Light and dark literal (the same text when the literal isn't a pair).
    Literal(String, String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyLine {
    pub line: usize,
    pub field: String,
    pub kind: Kind,
    pub alts: Vec<Alt>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Resolved {
    Color(u32),
    Float(f64),
}

/// A field with the alternative that resolved for each theme (`(description, value)`).
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub light: (String, Resolved),
    pub dark: (String, Resolved),
}

#[derive(Debug, Clone, Copy)]
pub enum Variant {
    Light,
    Dark,
}

fn split_comment(line: &str) -> (&str, &str) {
    let bytes = line.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        let starts = i == 0 || bytes[i - 1].is_ascii_whitespace();
        let ends = bytes.get(i + 1).is_none_or(u8::is_ascii_whitespace);
        if b == b'#' && starts && (ends || line[..i].trim().is_empty()) {
            return (&line[..i], line[i + 1..].trim());
        }
    }
    (line, "")
}

fn is_literal(s: &str) -> bool {
    s.starts_with('#') || s.starts_with(|c: char| c.is_ascii_digit()) || s.contains('/')
}

fn parse_alt(s: &str) -> Alt {
    if let Some(rest) = s.strip_prefix("scheme:") {
        if let Some((attr, field)) = rest.rsplit_once('.')
            && !field.is_empty()
            && field.bytes().all(|b| b.is_ascii_uppercase() || b == b'_')
        {
            return Alt::SchemeAttribute(attr.to_string(), field.to_string());
        }
        return Alt::SchemeColor(rest.to_string());
    }
    if is_literal(s) {
        let (light, dark) = s.split_once('/').unwrap_or((s, s));
        return Alt::Literal(light.trim().to_string(), dark.trim().to_string());
    }
    Alt::Key(s.to_string())
}

/// Parses `theme.keys`.
pub fn parse_keys(text: &str) -> Result<Vec<KeyLine>> {
    let mut lines: Vec<KeyLine> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let (body, comment) = split_comment(raw);
        let body = body.trim();
        if body.is_empty() {
            continue;
        }
        let Some((lhs, rhs)) = body.split_once('=') else {
            bail!("theme.keys:{line}: expected `<field> = <keys>`")
        };
        let lhs = lhs.trim();
        let (kind, field) = match lhs.strip_prefix("f32 ") {
            Some(f) => (Kind::F32, f.trim()),
            None => (Kind::Color, lhs),
        };
        if field.is_empty()
            || !field.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
            || !field
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            bail!("theme.keys:{line}: {field:?} is not a snake_case field name");
        }
        if lines.iter().any(|l| l.field == field) {
            bail!("theme.keys:{line}: field `{field}` is listed twice");
        }
        let alts: Vec<Alt> = rhs
            .split('|')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(parse_alt)
            .collect();
        if alts.is_empty() {
            bail!("theme.keys:{line}: field `{field}` has no keys");
        }
        if alts.iter().any(|a| matches!(a, Alt::Literal(..))) && comment.is_empty() {
            bail!(
                "theme.keys:{line}: field `{field}` uses a literal without a comment citing its upstream source"
            );
        }
        lines.push(KeyLine {
            line,
            field: field.to_string(),
            kind,
            alts,
        });
    }
    Ok(lines)
}

fn describe(alt: &Alt) -> String {
    match alt {
        Alt::Key(k) => k.clone(),
        Alt::SchemeColor(o) => format!("scheme:{o}"),
        Alt::SchemeAttribute(a, f) => format!("scheme:{a}.{f}"),
        Alt::Literal(l, d) if l == d => l.clone(),
        Alt::Literal(l, d) => format!("{l}/{d}"),
    }
}

fn resolve_one(line: &KeyLine, theme: &Theme, variant: Variant) -> Result<(String, Resolved)> {
    for alt in &line.alts {
        let found = match (alt, line.kind) {
            (Alt::Key(k), Kind::Color) => theme.color(k).map(|(c, via)| {
                let desc = match via {
                    Some(p) if p != *k => format!("{k} ({p})"),
                    _ => k.clone(),
                };
                (desc, Resolved::Color(c))
            }),
            (Alt::Key(k), Kind::F32) => theme.number(k).map(|n| (k.clone(), Resolved::Float(n))),
            (Alt::SchemeColor(o), Kind::Color) => theme
                .scheme_color(o)
                .map(|c| (describe(alt), Resolved::Color(c))),
            (Alt::SchemeAttribute(a, f), Kind::Color) => theme
                .scheme_attribute(a, f)
                .map(|c| (describe(alt), Resolved::Color(c))),
            (Alt::Literal(l, d), kind) => {
                let text = match variant {
                    Variant::Light => l,
                    Variant::Dark => d,
                };
                let value = match kind {
                    Kind::Color => parse_hex(text)
                        .filter(|_| text.starts_with('#'))
                        .map(Resolved::Color),
                    Kind::F32 => text.parse::<f64>().ok().map(Resolved::Float),
                };
                match value {
                    Some(v) => Some((format!("literal {text}"), v)),
                    None => bail!(
                        "theme.keys:{}: field `{}`: bad literal {text:?}",
                        line.line,
                        line.field
                    ),
                }
            }
            (_, Kind::F32) => bail!(
                "theme.keys:{}: f32 field `{}` can't use {}",
                line.line,
                line.field,
                describe(alt)
            ),
        };
        if let Some(found) = found {
            return Ok(found);
        }
    }
    let tried: Vec<String> = line
        .alts
        .iter()
        .map(|a| format!("`{}`", describe(a)))
        .collect();
    Err(Error(format!(
        "theme.keys:{}: field `{}`: none of {} resolved for theme {:?} ({})",
        line.line,
        line.field,
        tried.join(", "),
        theme.name,
        theme.files.first().map(String::as_str).unwrap_or_default(),
    )))
}

/// Resolves every field for both themes; fails on the first field that doesn't resolve.
pub fn resolve(lines: &[KeyLine], light: &Theme, dark: &Theme) -> Result<Vec<Field>> {
    lines
        .iter()
        .map(|line| {
            Ok(Field {
                name: line.field.clone(),
                light: resolve_one(line, light, Variant::Light)?,
                dark: resolve_one(line, dark, Variant::Dark)?,
            })
        })
        .collect()
}

fn rust_value(v: Resolved) -> String {
    match v {
        Resolved::Color(c) => format!("0x{c:08X}"),
        Resolved::Float(f) => format!("{:?}", f as f32),
    }
}

/// Renders `tokens.rs` (rustfmt-clean as generated).
pub fn render_rust(fields: &[Field], light: &Theme, dark: &Theme, commit: &str) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "//! @generated by `cargo xtask import-ui` from intellij-community {commit}. Do not edit."
    );
    s.push_str("//!\n//! Colors are `0xRRGGBBAA` (alpha last). Each field's doc names the upstream key that\n");
    s.push_str("//! resolved for the light / dark theme (see `assets/theme.keys`).\n\n");
    s.push_str("/// Colors and metrics of an IntelliJ New UI theme.\n");
    s.push_str("#[derive(Clone, Copy, Debug, PartialEq)]\npub struct ThemeTokens {\n");
    for f in fields {
        let ty = if matches!(f.light.1, Resolved::Float(_)) {
            "f32"
        } else {
            "u32"
        };
        let _ = writeln!(s, "    /// `{}` / `{}`", f.light.0, f.dark.0);
        let _ = writeln!(s, "    pub {}: {ty},", f.name);
    }
    s.push_str("}\n");
    for (name, theme, variant) in [
        ("LIGHT", light, Variant::Light),
        ("DARK", dark, Variant::Dark),
    ] {
        let file = theme.files.first().map(String::as_str).unwrap_or_default();
        let _ = writeln!(s, "\n/// The `{}` theme (`{file}`).", theme.name);
        let _ = writeln!(s, "pub const {name}: ThemeTokens = ThemeTokens {{");
        for f in fields {
            let v = match variant {
                Variant::Light => f.light.1,
                Variant::Dark => f.dark.1,
            };
            let _ = writeln!(s, "    {}: {},", f.name, rust_value(v));
        }
        s.push_str("};\n");
    }
    s
}

/// Renders `assets/themes/<variant>.json`: `{ field: { "key": ..., "value": ... } }`.
pub fn render_json(fields: &[Field], variant: Variant) -> String {
    let mut map = Map::new();
    for f in fields {
        let (key, value) = match variant {
            Variant::Light => &f.light,
            Variant::Dark => &f.dark,
        };
        let value = match *value {
            Resolved::Color(c) => Json::String(format!("#{c:08X}")),
            Resolved::Float(n) => serde_json::Number::from_f64(n)
                .map(Json::Number)
                .unwrap_or(Json::Null),
        };
        let mut entry = Map::new();
        entry.insert("key".into(), Json::String(key.clone()));
        entry.insert("value".into(), value);
        map.insert(f.name.clone(), Json::Object(entry));
    }
    let mut text = serde_json::to_string_pretty(&Json::Object(map)).unwrap_or_default();
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_alternatives_comments_and_literals() {
        let keys = parse_keys(
            "# header\n\nf32 sat = A.b | 0.4 # DefaultColorGenerator.kt\nbg = *.background | scheme:X.FOREGROUND\n\
             err = Banner.e | #FFF7F7/#402929 # JBUI.java\nsb = scheme:ScrollBar.Mac.thumbColor\n",
        )
        .unwrap();
        assert_eq!(keys.len(), 4);
        assert_eq!(keys[0].kind, Kind::F32);
        assert_eq!(keys[0].alts[1], Alt::Literal("0.4".into(), "0.4".into()));
        assert_eq!(
            keys[1].alts[1],
            Alt::SchemeAttribute("X".into(), "FOREGROUND".into())
        );
        assert_eq!(
            keys[2].alts[1],
            Alt::Literal("#FFF7F7".into(), "#402929".into())
        );
        assert_eq!(
            keys[3].alts[0],
            Alt::SchemeColor("ScrollBar.Mac.thumbColor".into())
        );
    }

    #[test]
    fn literal_without_comment_is_rejected() {
        let err = parse_keys("x = A.b | #FFFFFF\n").unwrap_err();
        assert!(err.0.contains("`x`"), "{err}");
        assert!(parse_keys("Bad = A\n").is_err());
        assert!(parse_keys("x = A\nx = B\n").is_err());
    }
}
