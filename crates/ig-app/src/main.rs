//! `cargo run -p ig-app -- [path]`: opens a folder directly, or the welcome window.

fn main() {
    ig_app::logging::init();
    let mut args = std::env::args_os().skip(1);
    let path = args.next().map(std::path::PathBuf::from);
    ig_app::run(path);
}
