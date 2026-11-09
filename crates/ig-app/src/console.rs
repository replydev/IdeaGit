//! The Git tool window's Console tab.
//!
//! Port of `plugins/git4idea/backend/src/commands/GitCommandOutputConsolePrinter.kt` and
//! `plugins/git4idea/backend/src/util/GitVcsConsoleWriter.java`: a command start prints
//! `HH:mm:ss.SSS: [<working dir>] <command line>` in the console's system-output color, stdout lines
//! in the normal color, stderr lines in the error color; lines are cut to 10,000 chars
//! (`MAX_CONSOLE_OUTPUT_SIZE`: first 30% … last 70%). Only non-silent commands are logged, as
//! upstream; the "Show read commands in Console" toggle (an IdeaGit addition) also shows the
//! command lines of silent (read) commands.
//!
//! Deviations: no ANSI escape decoding (`AnsiEscapeDecoder`); the console keeps the last
//! [`MAX_LINES`] lines (upstream's console buffer is bounded by `idea.cycle.buffer.size`).

use std::collections::VecDeque;
use std::path::Path;

use chrono::{DateTime, Local};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, AppContext as _, Context, Entity, FocusHandle, Focusable, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, Styled, UniformListScrollHandle, Window, div, px,
    uniform_list,
};
use ig_git::console::{ConsoleEvent, ConsoleSink, OutputKind};
use ig_ui_kit::{ActiveIjTheme, color};

/// `GitVcsConsoleWriter.MAX_CONSOLE_OUTPUT_SIZE`.
const MAX_CONSOLE_OUTPUT_SIZE: usize = 10_000;
/// Lines kept in the console.
pub const MAX_LINES: usize = 20_000;

/// `ConsoleViewContentType` of a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    /// `SYSTEM_OUTPUT`: the command line.
    System,
    /// `NORMAL_OUTPUT`: stdout.
    Normal,
    /// `ERROR_OUTPUT`: stderr.
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleLine {
    pub kind: LineKind,
    pub text: String,
    /// A silent command's line (shown only with the debug toggle).
    pub silent: bool,
}

/// `GitVcsConsoleWriter.showCommandLine` with `GitCommandOutputConsolePrinter.showCommandStart`.
pub fn format_command_start(
    time: DateTime<Local>,
    working_dir: &Path,
    command_line: &str,
) -> String {
    shorten(&format!(
        "{}: [{}] {command_line}",
        time.format("%H:%M:%S%.3f"),
        working_dir.display()
    ))
}

/// `StringUtil.shortenPathWithEllipsis(message, MAX_CONSOLE_OUTPUT_SIZE)` (`...` symbol).
pub fn shorten(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= MAX_CONSOLE_OUTPUT_SIZE {
        return text.to_string();
    }
    let suffix = (MAX_CONSOLE_OUTPUT_SIZE as f64 * 0.7) as usize;
    let prefix = MAX_CONSOLE_OUTPUT_SIZE - suffix - 3;
    let mut out: String = chars[..prefix].iter().collect();
    out.push_str("...");
    out.extend(&chars[chars.len() - suffix..]);
    out
}

/// What a console sink sends to the UI.
pub enum ConsoleMessage {
    Event(DateTime<Local>, ConsoleEvent),
    Silent(DateTime<Local>, std::path::PathBuf, String),
}

/// The `ConsoleSink` given to a project's `Git`: forwards events over a channel (it runs on git
/// threads and must not block).
pub struct ChannelConsole(pub flume::Sender<ConsoleMessage>);

impl ConsoleSink for ChannelConsole {
    fn event(&self, event: ConsoleEvent) {
        let _ = self.0.send(ConsoleMessage::Event(Local::now(), event));
    }

    fn silent_command(&self, working_dir: &Path, command_line: &str) {
        let _ = self.0.send(ConsoleMessage::Silent(
            Local::now(),
            working_dir.to_path_buf(),
            command_line.to_string(),
        ));
    }
}

/// The console's lines.
#[derive(Default)]
pub struct ConsoleBuffer {
    lines: VecDeque<ConsoleLine>,
}

impl ConsoleBuffer {
    pub fn push(&mut self, message: ConsoleMessage) {
        let line = match message {
            ConsoleMessage::Event(
                time,
                ConsoleEvent::Started {
                    working_dir,
                    command_line,
                    ..
                },
            ) => ConsoleLine {
                kind: LineKind::System,
                text: format_command_start(time, &working_dir, &command_line),
                silent: false,
            },
            ConsoleMessage::Event(_, ConsoleEvent::Output { kind, line, .. }) => ConsoleLine {
                kind: if kind == OutputKind::Stderr {
                    LineKind::Error
                } else {
                    LineKind::Normal
                },
                text: shorten(&line),
                silent: false,
            },
            // `showCommandFinished` prints nothing
            ConsoleMessage::Event(_, ConsoleEvent::Finished { .. }) => return,
            ConsoleMessage::Silent(time, dir, command_line) => ConsoleLine {
                kind: LineKind::System,
                text: format_command_start(time, &dir, &command_line),
                silent: true,
            },
        };
        self.lines.push_back(line);
        while self.lines.len() > MAX_LINES {
            self.lines.pop_front();
        }
    }

    /// The lines shown, with or without silent commands.
    pub fn visible(&self, show_silent: bool) -> Vec<&ConsoleLine> {
        self.lines
            .iter()
            .filter(|l| show_silent || !l.silent)
            .collect()
    }
}

/// The Console tab.
pub struct ConsoleView {
    buffer: ConsoleBuffer,
    show_silent: bool,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    follow: bool,
}

impl ConsoleView {
    /// Creates the view and starts reading `rx`.
    pub fn new(rx: flume::Receiver<ConsoleMessage>, cx: &mut Context<ConsoleView>) -> ConsoleView {
        cx.spawn(async move |this, cx| {
            while let Ok(message) = rx.recv_async().await {
                let mut batch = vec![message];
                batch.extend(rx.try_iter());
                if this
                    .update(cx, |view, cx| {
                        for m in batch {
                            view.buffer.push(m);
                        }
                        view.follow = true;
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        ConsoleView {
            buffer: ConsoleBuffer::default(),
            show_silent: false,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            follow: false,
        }
    }

    pub fn show_silent(&self) -> bool {
        self.show_silent
    }

    pub fn set_show_silent(&mut self, show: bool, cx: &mut Context<ConsoleView>) {
        self.show_silent = show;
        cx.notify();
    }

    pub fn line_count(&self) -> usize {
        self.buffer.visible(self.show_silent).len()
    }
}

impl Focusable for ConsoleView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ConsoleView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<ConsoleView>) -> impl IntoElement {
        let t = cx.ij().tokens;
        let lines: Vec<(LineKind, SharedString)> = self
            .buffer
            .visible(self.show_silent)
            .into_iter()
            .map(|l| (l.kind, SharedString::from(l.text.clone())))
            .collect();
        let count = lines.len();
        if self.follow && count > 0 {
            self.scroll
                .scroll_to_item(count - 1, gpui::ScrollStrategy::Bottom);
            self.follow = false;
        }
        let colors = [
            (LineKind::System, color(t.console_system)),
            (LineKind::Normal, color(t.console_normal)),
            (LineKind::Error, color(t.console_error)),
        ];
        div()
            .id("git-console")
            .track_focus(&self.focus)
            .size_full()
            .bg(color(t.console_background))
            .font_family(ig_ui_kit::fonts::MONO_FAMILY)
            .text_size(px(ig_ui_kit::metrics::MONO_FONT_SIZE))
            .child(
                uniform_list("git-console-lines", count, move |range, _, _| {
                    range
                        .map(|i| {
                            let (kind, text) = &lines[i];
                            let fg = colors.iter().find(|(k, _)| k == kind).map(|(_, c)| *c);
                            div()
                                .px(px(8.0))
                                .whitespace_nowrap()
                                .when_some(fg, |d, c| d.text_color(c))
                                .child(text.clone())
                        })
                        .collect()
                })
                .size_full()
                .track_scroll(&self.scroll),
            )
    }
}

/// Creates the console view and the sink to give to the project's `Git`.
pub fn create(cx: &mut App) -> (Entity<ConsoleView>, ChannelConsole) {
    let (tx, rx) = flume::unbounded();
    (cx.new(|cx| ConsoleView::new(rx, cx)), ChannelConsole(tx))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn command_start_has_time_dir_and_command() {
        let time = Local.with_ymd_and_hms(2026, 11, 8, 9, 5, 7).unwrap();
        assert_eq!(
            format_command_start(time, Path::new("/repo"), "git fetch origin"),
            "09:05:07.000: [/repo] git fetch origin"
        );
    }

    #[test]
    fn events_become_colored_lines_and_silent_ones_are_hidden_by_default() {
        let mut buffer = ConsoleBuffer::default();
        let now = Local::now();
        buffer.push(ConsoleMessage::Event(
            now,
            ConsoleEvent::Started {
                id: 1,
                working_dir: "/r".into(),
                command_line: "git commit".into(),
            },
        ));
        buffer.push(ConsoleMessage::Event(
            now,
            ConsoleEvent::Output {
                id: 1,
                kind: OutputKind::Stdout,
                line: "ok".into(),
            },
        ));
        buffer.push(ConsoleMessage::Event(
            now,
            ConsoleEvent::Output {
                id: 1,
                kind: OutputKind::Stderr,
                line: "warn".into(),
            },
        ));
        buffer.push(ConsoleMessage::Event(
            now,
            ConsoleEvent::Finished {
                id: 1,
                exit_code: 0,
            },
        ));
        buffer.push(ConsoleMessage::Silent(now, "/r".into(), "git log".into()));
        let kinds: Vec<LineKind> = buffer.visible(false).iter().map(|l| l.kind).collect();
        assert_eq!(kinds, [LineKind::System, LineKind::Normal, LineKind::Error]);
        assert_eq!(buffer.visible(true).len(), 4);
        assert!(buffer.visible(true)[3].text.ends_with("[/r] git log"));
    }

    #[test]
    fn long_lines_keep_head_and_tail() {
        let long = "a".repeat(5000) + &"b".repeat(10_000);
        let short = shorten(&long);
        assert_eq!(short.chars().count(), MAX_CONSOLE_OUTPUT_SIZE);
        assert!(short.starts_with("aaa") && short.ends_with("bbb") && short.contains("..."));
    }
}
