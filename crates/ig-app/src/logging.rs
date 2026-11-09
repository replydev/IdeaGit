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

/// `ideagit.log` is moved to `ideagit.log.1` at startup when larger than this.
const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;

/// Renames `path` to `<path>.1` (replacing an older one) when it exceeds `max` bytes.
fn rotate_if_larger(path: &std::path::Path, max: u64) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > max) {
        let mut rotated = path.as_os_str().to_os_string();
        rotated.push(".1");
        if let Err(e) = std::fs::rename(path, &rotated) {
            eprintln!("IdeaGit: can't rotate {}: {e}", path.display());
        }
    }
}

/// Opens `path` for appending (creating its directory); reports failure once on stderr.
fn open_log(path: &std::path::Path) -> Option<std::fs::File> {
    let opened = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|_| {
            rotate_if_larger(path, MAX_LOG_BYTES);
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
        });
    match opened {
        Ok(file) => Some(file),
        Err(e) => {
            eprintln!("IdeaGit: can't open log file {}: {e}", path.display());
            None
        }
    }
}

/// Installs the logger and the panic hook. Only the first call has an effect.
pub fn init() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let path = crate::paths::log_file();
        let file = open_log(&path).map(Mutex::new);
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
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_logs_rotate_once_and_small_ones_stay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ideagit.log");
        std::fs::write(&path, "0123456789").unwrap();
        rotate_if_larger(&path, 10);
        assert!(path.exists());
        std::fs::write(&path, "0123456789a").unwrap();
        std::fs::write(dir.path().join("ideagit.log.1"), "old").unwrap();
        rotate_if_larger(&path, 10);
        assert!(!path.exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("ideagit.log.1")).unwrap(),
            "0123456789a"
        );
    }

    #[test]
    fn unopenable_log_gives_none() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, "").unwrap();
        assert!(open_log(&blocker.join("sub/ideagit.log")).is_none());
        assert!(open_log(&dir.path().join("ok/ideagit.log")).is_some());
    }
}
