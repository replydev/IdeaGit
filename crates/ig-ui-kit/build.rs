//! Embeds `assets/icons/**/*.svg` (written by `cargo xtask import-ui`) into the binary: generates
//! `$OUT_DIR/icons.rs` with a sorted `(asset path, bytes)` table read by `crate::icons`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn collect(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, base, out);
        } else if path.extension().is_some_and(|e| e == "svg") {
            let rel = path
                .strip_prefix(base)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((format!("icons/{rel}"), path));
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let icons = manifest.join("../../assets/icons");
    println!("cargo::rerun-if-changed={}", icons.display());
    let mut files = Vec::new();
    collect(&icons, &icons, &mut files);
    files.sort();
    if files.is_empty() {
        println!(
            "cargo::warning=ig-ui-kit: no SVGs found in {} (missing or empty; run `cargo xtask import-ui`), no icons are embedded",
            icons.display()
        );
    }
    let mut code = String::from("pub(crate) static ICONS: &[(&str, &[u8])] = &[\n");
    for (name, path) in &files {
        println!("cargo::rerun-if-changed={}", path.display());
        writeln!(
            code,
            "    ({name:?}, include_bytes!({:?})),",
            path.display().to_string()
        )
        .unwrap();
    }
    code.push_str("];\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("icons.rs");
    std::fs::write(out, code).unwrap();
}
