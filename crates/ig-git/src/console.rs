//! Git console event stream. Mirrors what upstream's `GitCommandOutputLogger` sends to
//! `GitCommandOutputPrinter`: command start, output lines, and termination.

use std::path::PathBuf;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsoleEvent {
    Started {
        id: u64,
        working_dir: PathBuf,
        command_line: String,
    },
    Output {
        id: u64,
        kind: OutputKind,
        line: String,
    },
    Finished {
        id: u64,
        exit_code: i32,
    },
}

/// Receives console events. Implementations must be cheap and non-blocking: they are called from
/// the thread that runs the git process.
///
/// Must not run write commands on the same [`crate::runner::Git`]: the per-project write lock is not re-entrant.
pub trait ConsoleSink: Send + Sync {
    fn event(&self, event: ConsoleEvent);
}

/// Discards all events.
pub struct NullConsole;

impl ConsoleSink for NullConsole {
    fn event(&self, _event: ConsoleEvent) {}
}

/// Stores all events in memory; used by tests and as a simple buffer for the UI.
#[derive(Default)]
pub struct RecordingConsole {
    events: Mutex<Vec<ConsoleEvent>>,
}

impl RecordingConsole {
    pub fn events(&self) -> Vec<ConsoleEvent> {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

impl ConsoleSink for RecordingConsole {
    fn event(&self, event: ConsoleEvent) {
        self.events
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(event);
    }
}
