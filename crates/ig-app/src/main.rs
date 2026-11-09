//! `cargo run -p ig-app -- [path]`: opens a folder directly, or the welcome window.

fn main() {
    ig_app::logging::init();
    let mut args = std::env::args_os().skip(1);
    #[cfg(feature = "screenshots")]
    {
        let all: Vec<_> = std::env::args_os().skip(1).collect();
        if all.first().is_some_and(|a| a == "--screenshots") {
            let out = all
                .get(1)
                .map(std::path::PathBuf::from)
                .expect("--screenshots <out dir> <repo>");
            let repo = all
                .get(2)
                .map(std::path::PathBuf::from)
                .expect("--screenshots <out dir> <repo>");
            std::process::exit(ig_app::screenshots::run(&out, &repo));
        }
    }
    let path = args.next().map(std::path::PathBuf::from);
    ig_app::run(path);
}
