//! `ideagit.log` in the OS log directory, and panic logging.
//!
//! A minimal `log` backend (no new dependency): info and above to the file, warnings and above
//! also to stderr. Panics anywhere are logged with their location before the default hook runs.

use std::io::Write;
use std::sync::Mutex;

struct FileLogger {
    file: Option<Mutex<std::fs::File>>,
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = format!(
            "{} {:5} {}: {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
            record.level(),
            record.target(),
            record.args()
        );
        if let Some(file) = &self.file {
            let mut file = file.lock().unwrap_or_else(|e| e.into_inner());
            let _ = file.write_all(line.as_bytes());
        }
        if record.level() <= log::Level::Warn {
            eprint!("{line}");
        }
    }

    fn flush(&self) {}
}

/// Installs the logger and the panic hook. Safe to call once.
pub fn init() {
    let path = crate::paths::log_file();
    let file = path
        .parent()
        .and_then(|dir| std::fs::create_dir_all(dir).ok())
        .and_then(|_| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .ok()
        })
        .map(Mutex::new);
    let logger = Box::leak(Box::new(FileLogger { file }));
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("panic: {info}");
        default_hook(info);
    }));
    log::info!(
        "IdeaGit {} starting; log at {}",
        env!("CARGO_PKG_VERSION"),
        path.display()
    );
}
