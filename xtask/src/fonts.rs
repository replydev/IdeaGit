//! Font copying from `platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts/`
//! (the Inter and JetBrains Mono files Jewel's standalone Int UI theme bundles), plus their SIL
//! OFL 1.1 license texts.
//!
//! Upstream ships no license file next to these fonts, so each `*-OFL.txt` is assembled from the
//! font's own copyright notice (OpenType `name` table, name ID 0, read from the Regular face) and
//! the OFL 1.1 text of the one OFL license file in the upstream tree,
//! `platform/platform-tests/testData/fonts/Inconsolata_LICENSE.txt` (from its "This Font Software
//! is licensed" line on). Font files are copied unchanged.

use crate::source::Source;
use crate::{Error, Result, bail};

pub const FONTS_DIR: &str = "platform/jewel/int-ui/int-ui-standalone/src/main/resources/fonts/";
pub const OFL_SOURCE: &str = "platform/platform-tests/testData/fonts/Inconsolata_LICENSE.txt";
const OFL_START: &str = "This Font Software is licensed under the SIL Open Font License";

/// `(family license file, [(upstream path under FONTS_DIR, output file name)])`; the first
/// face of each family provides the copyright notice.
pub const FAMILIES: [(&str, &[(&str, &str)]); 2] = [
    (
        "Inter-OFL.txt",
        &[
            ("inter/Inter-Regular.ttf", "Inter-Regular.ttf"),
            ("inter/Inter-Medium.ttf", "Inter-Medium.ttf"),
            ("inter/Inter-SemiBold.ttf", "Inter-SemiBold.ttf"),
            ("inter/Inter-Bold.ttf", "Inter-Bold.ttf"),
        ],
    ),
    (
        "JetBrainsMono-OFL.txt",
        &[
            (
                "jetbrains-mono/JetBrainsMono-Regular.ttf",
                "JetBrainsMono-Regular.ttf",
            ),
            (
                "jetbrains-mono/JetBrainsMono-Bold.ttf",
                "JetBrainsMono-Bold.ttf",
            ),
        ],
    ),
];

fn be16(d: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_be_bytes([*d.get(at)?, *d.get(at + 1)?]) as usize)
}

fn be32(d: &[u8], at: usize) -> Option<usize> {
    Some(u32::from_be_bytes([
        *d.get(at)?,
        *d.get(at + 1)?,
        *d.get(at + 2)?,
        *d.get(at + 3)?,
    ]) as usize)
}

/// The string with name ID `id` from an sfnt (TrueType/OpenType) `name` table, preferring
/// Windows Unicode (platform 3, UTF-16BE) over Macintosh Roman (platform 1, read as ASCII).
pub fn name_string(font: &[u8], id: usize) -> Option<String> {
    let tables = be16(font, 4)?;
    let name = (0..tables).find(|i| font.get(12 + 16 * i..16 + 16 * i) == Some(b"name"))?;
    let off = be32(font, 12 + 16 * name + 8)?;
    let count = be16(font, off + 2)?;
    let strings = off + be16(font, off + 4)?;
    let mut fallback = None;
    for r in 0..count {
        let rec = off + 6 + 12 * r;
        let (platform, name_id) = (be16(font, rec)?, be16(font, rec + 6)?);
        let (len, at) = (be16(font, rec + 8)?, strings + be16(font, rec + 10)?);
        if name_id != id {
            continue;
        }
        let bytes = font.get(at..at + len)?;
        match platform {
            3 | 0 => {
                let (pairs, _) = bytes.as_chunks::<2>();
                let units: Vec<u16> = pairs.iter().map(|c| u16::from_be_bytes(*c)).collect();
                return String::from_utf16(&units).ok();
            }
            1 if bytes.is_ascii() => fallback = Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => {}
        }
    }
    fallback
}

/// Reads the fonts and builds the license files; returns `(output path, bytes)` pairs.
pub fn collect(src: &dyn Source) -> Result<Vec<(String, Vec<u8>)>> {
    let ofl = src.read_text(OFL_SOURCE)?;
    let Some(start) = ofl.find(OFL_START) else {
        bail!("{OFL_SOURCE}: no OFL text ({OFL_START:?})")
    };
    let ofl_text = &ofl[start..];

    let mut out = Vec::new();
    for (license, faces) in FAMILIES {
        let mut copyright = None;
        for (path, file) in faces {
            let full = format!("{FONTS_DIR}{path}");
            let bytes = src.read(&full)?;
            if copyright.is_none() {
                let notice = name_string(&bytes, 0).ok_or_else(|| {
                    Error(format!(
                        "{full}: no copyright notice (name ID 0) in the name table"
                    ))
                })?;
                copyright = Some(notice);
            }
            out.push((format!("assets/fonts/{file}"), bytes));
        }
        let mut text = copyright.unwrap_or_default();
        text.push_str("\n\n");
        text.push_str(ofl_text);
        if !text.ends_with('\n') {
            text.push('\n');
        }
        out.push((format!("assets/fonts/{license}"), text.into_bytes()));
    }
    Ok(out)
}
