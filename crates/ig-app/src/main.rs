//! `cargo run -p ig-app -- [path]`: opens a folder directly, or the welcome window.

use std::ffi::OsString;
use std::path::PathBuf;

const USAGE: &str = "usage: ideagit [<folder>]";

#[cfg(feature = "screenshots")]
const SCREENSHOTS_USAGE: &str = "\
usage: ideagit --screenshots <out dir> <repo>
    Renders the Log of <repo> to <out dir>/ideagit-log-{light,dark}.png (macOS only: GPUI's
    headless renderer is Metal). IDEAGIT_SCROLL_BENCH=1 also prints a scroll benchmark.";

fn main() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    #[cfg(feature = "screenshots")]
    if args.first().is_some_and(|a| a == "--screenshots") {
        // no logging::init: screenshots must not write to the user's ideagit.log
        let [_, out, repo] = args.as_slice() else {
            eprintln!("{SCREENSHOTS_USAGE}");
            std::process::exit(2);
        };
        std::process::exit(ig_app::screenshots::run(
            &PathBuf::from(out),
            &PathBuf::from(repo),
        ));
    }
    let path = match args.as_slice() {
        [] => None,
        [flag] if flag == "-h" || flag == "--help" => {
            println!("{USAGE}");
            return;
        }
        [path] if !path.to_string_lossy().starts_with('-') => Some(PathBuf::from(path)),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    ig_app::logging::init();
    ig_app::run(path);
}
