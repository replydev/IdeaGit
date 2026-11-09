//! `cargo xtask <task>`: see [`xtask`] for the tasks.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use xtask::{Error, FsSource, GitSource, Result, Source};

const USAGE: &str = "\
usage: cargo xtask <task>

tasks:
  import-ui [--upstream <dir> | --fs <dir>] [--rev <rev>] [--out <workspace root>]
      Copy theme tokens, icons, strings, keymaps and fonts from intellij-community.
      --upstream  intellij-community git clone (default: $IDEAGIT_UPSTREAM)
      --fs        read upstream files from a plain directory tree instead of git
      --rev       upstream revision (default: `commit` in reference/UPSTREAM)
      --out       workspace root to write to (default: this workspace)
  screenshots [--repo <dir>] [--out <dir>]
      Renders the Log of <repo> (default: this workspace) in the light and dark themes to
      <out>/ideagit-log-{light,dark}.png (default: docs/ui-reference), headless (no display or
      screen-recording permission needed). Builds ig-app with its `screenshots` feature.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("import-ui") => import_ui(&args[1..]),
        Some("screenshots") => screenshots(&args[1..]),
        Some("-h" | "--help") => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        _ => Err(Error(USAGE.to_string())),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn import_ui(args: &[String]) -> Result<()> {
    let (mut upstream, mut fs, mut rev, mut out) = (None, None, None, None);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let slot = match flag.as_str() {
            "--upstream" => &mut upstream,
            "--fs" => &mut fs,
            "--rev" => &mut rev,
            "--out" => &mut out,
            other => {
                return Err(Error(format!(
                    "import-ui: unknown argument {other:?}\n\n{USAGE}"
                )));
            }
        };
        let Some(value) = it.next() else {
            return Err(Error(format!("import-ui: {flag} needs a value")));
        };
        *slot = Some(value.clone());
    }
    let out = out
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(".."));
    let rev = match rev {
        Some(rev) => rev,
        None => xtask::pinned_commit(&out)?,
    };
    let source: Box<dyn Source> = match (
        fs,
        upstream.or_else(|| std::env::var("IDEAGIT_UPSTREAM").ok()),
    ) {
        (Some(_), Some(_)) if args.iter().any(|a| a == "--upstream") => {
            return Err(Error(
                "import-ui: --fs and --upstream are mutually exclusive".into(),
            ));
        }
        (Some(dir), _) => Box::new(FsSource::new(Path::new(&dir))?),
        (None, Some(dir)) => Box::new(GitSource::new(Path::new(&dir), &rev)?),
        (None, None) => {
            return Err(Error(
                "import-ui: no upstream clone given. Pass --upstream <intellij-community clone> or set \
                 IDEAGIT_UPSTREAM (or --fs <dir> for a plain directory tree)."
                    .into(),
            ));
        }
    };
    eprintln!("import-ui: reading {}", source.describe());
    let summary = xtask::import_ui(source.as_ref(), &out, &rev)?;
    for rel in &summary.written {
        eprintln!("  wrote {rel}");
    }
    for rel in &summary.removed {
        eprintln!("  removed {rel}");
    }
    eprintln!(
        "import-ui: {} written, {} unchanged, {} removed",
        summary.written.len(),
        summary.unchanged,
        summary.removed.len()
    );
    Ok(())
}

/// `screenshots`: runs `ideagit --screenshots <out> <repo>` (feature `screenshots`, release).
fn screenshots(args: &[String]) -> Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut repo = root.clone();
    let mut out = root.join("docs/ui-reference");
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let value = || Error(format!("{arg} needs a value"));
        match arg.as_str() {
            "--repo" => repo = it.next().ok_or_else(value)?.into(),
            "--out" => out = it.next().ok_or_else(value)?.into(),
            other => return Err(Error(format!("unknown argument {other}\n{USAGE}"))),
        }
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = std::process::Command::new(cargo)
        .current_dir(&root)
        .args([
            "run",
            "-q",
            // the Zed rev is pinned only in Cargo.lock
            "--locked",
            "--release",
            "-p",
            "ig-app",
            "--features",
            "screenshots",
            "--",
            "--screenshots",
        ])
        .arg(&out)
        .arg(&repo)
        .status()
        .map_err(|e| Error(format!("running cargo: {e}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error(format!("ideagit --screenshots failed: {status}")))
    }
}
