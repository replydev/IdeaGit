//! A minimal XML reader for the upstream color scheme and keymap files: elements and attributes
//! only (text, comments, processing instructions, DOCTYPE and CDATA are skipped). Not a general
//! XML parser; it accepts the well-formed subset those files use.

use crate::{Error, Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }
}

/// Parses `text` and returns its root element.
pub fn parse(text: &str) -> Result<Element> {
    let mut p = Parser { s: text, pos: 0 };
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    while let Some(lt) = p.s[p.pos..].find('<') {
        p.pos += lt;
        let rest = &p.s[p.pos..];
        if rest.starts_with("<!--") {
            p.skip_past("-->")?;
        } else if rest.starts_with("<![CDATA[") {
            p.skip_past("]]>")?;
        } else if rest.starts_with("<?") {
            p.skip_past("?>")?;
        } else if rest.starts_with("<!") {
            p.skip_past(">")?;
        } else if rest.starts_with("</") {
            p.pos += 2;
            let name = p.name()?;
            p.skip_ws();
            p.expect('>')?;
            let Some(el) = stack.pop() else {
                bail!("unexpected </{name}> at byte {}", p.pos)
            };
            if el.name != name {
                bail!("</{name}> closes <{}> at byte {}", el.name, p.pos);
            }
            match stack.last_mut() {
                Some(parent) => parent.children.push(el),
                None => root = Some(el),
            }
        } else {
            p.pos += 1;
            let (el, closed) = p.start_tag()?;
            if !closed {
                stack.push(el);
            } else if let Some(parent) = stack.last_mut() {
                parent.children.push(el);
            } else {
                root = Some(el);
            }
        }
        if root.is_some() {
            break;
        }
    }
    if let Some(open) = stack.last() {
        bail!("unclosed <{}>", open.name);
    }
    root.ok_or_else(|| Error("no root element".into()))
}

struct Parser<'a> {
    s: &'a str,
    pos: usize,
}

impl Parser<'_> {
    fn skip_past(&mut self, end: &str) -> Result<()> {
        match self.s[self.pos..].find(end) {
            Some(i) => {
                self.pos += i + end.len();
                Ok(())
            }
            None => bail!(
                "unterminated markup at byte {} (expected {end:?})",
                self.pos
            ),
        }
    }

    fn skip_ws(&mut self) {
        let rest = &self.s[self.pos..];
        self.pos += rest.len() - rest.trim_start().len();
    }

    fn expect(&mut self, c: char) -> Result<()> {
        if self.s[self.pos..].starts_with(c) {
            self.pos += c.len_utf8();
            Ok(())
        } else {
            bail!("expected {c:?} at byte {}", self.pos)
        }
    }

    fn name(&mut self) -> Result<String> {
        let rest = &self.s[self.pos..];
        let len = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '=' | '/' | '>' | '"' | '\''))
            .unwrap_or(rest.len());
        if len == 0 {
            bail!("expected a name at byte {}", self.pos);
        }
        self.pos += len;
        Ok(rest[..len].to_string())
    }

    /// Parses after `<` up to and including `>`; returns the element and whether it self-closed.
    fn start_tag(&mut self) -> Result<(Element, bool)> {
        let name = self.name()?;
        let mut el = Element {
            name,
            attrs: Vec::new(),
            children: Vec::new(),
        };
        loop {
            self.skip_ws();
            let rest = &self.s[self.pos..];
            if rest.starts_with("/>") {
                self.pos += 2;
                return Ok((el, true));
            }
            if rest.starts_with('>') {
                self.pos += 1;
                return Ok((el, false));
            }
            let key = self.name()?;
            self.skip_ws();
            self.expect('=')?;
            self.skip_ws();
            let quote = match self.s[self.pos..].chars().next() {
                Some(q @ ('"' | '\'')) => q,
                _ => bail!("expected a quoted value for {key} at byte {}", self.pos),
            };
            self.pos += 1;
            let Some(end) = self.s[self.pos..].find(quote) else {
                bail!("unterminated value for {key} at byte {}", self.pos)
            };
            let raw = &self.s[self.pos..self.pos + end];
            self.pos += end + 1;
            el.attrs.push((key, unescape(raw)?));
        }
    }
}

fn unescape(raw: &str) -> Result<String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.find(';') else {
            bail!("bad entity in {raw:?}")
        };
        let entity = &rest[1..semi];
        let c = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code =
                    if let Some(hex) = entity.strip_prefix("#x").or(entity.strip_prefix("#X")) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        entity.strip_prefix('#').and_then(|d| d.parse().ok())
                    };
                match code.and_then(char::from_u32) {
                    Some(c) => c,
                    None => bail!("unknown entity &{entity}; in {raw:?}"),
                }
            }
        };
        out.push(c);
        rest = &rest[semi + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_elements_attributes_and_entities() {
        let doc = parse(
            "<?xml version=\"1.0\"?>\n<!-- c --><keymap name=\"$default\" parent='x &amp; y'>\
             <action id=\"A\"><keyboard-shortcut first-keystroke=\"control &#65;\"/></action>\
             <action id=\"B\"/>text</keymap>",
        )
        .unwrap();
        assert_eq!(doc.name, "keymap");
        assert_eq!(doc.attr("parent"), Some("x & y"));
        let actions: Vec<_> = doc.children_named("action").collect();
        assert_eq!(actions.len(), 2);
        assert_eq!(
            actions[0].children[0].attr("first-keystroke"),
            Some("control A")
        );
        assert!(actions[1].children.is_empty());
    }

    #[test]
    fn rejects_mismatched_tags() {
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a>").is_err());
    }
}
