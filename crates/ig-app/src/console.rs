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
//! Output lines are cut like `GitVcsConsoleWriter.showMessage` (only beyond 1.2 x the maximum:
//! first 3,000 chars, `...`, last 7,000). Deviations: no ANSI escape decoding
//! (`AnsiEscapeDecoder`); `\r` progress output keeps the text after the last `\r` and tabs are
//! expanded; the console keeps the last [`MAX_LINES`] lines and at most [`MAX_BYTES`] bytes
//! (upstream's buffer is bounded by `idea.cycle.buffer.size`); the sink never blocks a git thread:
//! when the UI lags and the channel is full, messages are dropped and a single
//! [`DROPPED_MARKER`] line is shown; the view follows new output only when it was at the bottom.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

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
/// Bytes of text kept in the console.
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
/// Messages buffered between the git threads and the UI before the sink starts dropping.
pub const CHANNEL_CAPACITY: usize = 4096;
/// The line shown where messages were dropped.
pub const DROPPED_MARKER: &str = "… output dropped";
/// `showMessage`'s ellipsis threshold is 1.2 x [`MAX_CONSOLE_OUTPUT_SIZE`].
const OUTPUT_ELLIPSIS_THRESHOLD: usize = MAX_CONSOLE_OUTPUT_SIZE * 6 / 5;
const OUTPUT_PREFIX: usize = MAX_CONSOLE_OUTPUT_SIZE * 3 / 10;
const OUTPUT_SUFFIX: usize = MAX_CONSOLE_OUTPUT_SIZE * 7 / 10;
const TAB_WIDTH: usize = 8;

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
    pub text: SharedString,
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

/// `GitVcsConsoleWriter.showMessage(String, ...)` for output lines: unchanged up to 1.2 x the
/// maximum, else the first 3,000 chars, `...` and the last 7,000. No allocation when short.
pub fn shorten_output(text: &str) -> Cow<'_, str> {
    // bytes >= chars: short byte length proves a short char count
    if text.len() <= OUTPUT_ELLIPSIS_THRESHOLD {
        return Cow::Borrowed(text);
    }
    let total = text.chars().count();
    if total <= OUTPUT_ELLIPSIS_THRESHOLD {
        return Cow::Borrowed(text);
    }
    let prefix_end = text
        .char_indices()
        .nth(OUTPUT_PREFIX)
        .map_or(text.len(), |(i, _)| i);
    let suffix_start = text
        .char_indices()
        .nth(total - OUTPUT_SUFFIX)
        .map_or(text.len(), |(i, _)| i);
    Cow::Owned(format!(
        "{}...{}",
        &text[..prefix_end],
        &text[suffix_start..]
    ))
}

/// A carriage return rewrites the line (progress output): keep the text after the last one;
/// tabs become spaces up to the next multiple of `TAB_WIDTH`.
pub fn clean_output(line: &str) -> Cow<'_, str> {
    let line = line.trim_end_matches('\r');
    let line = line.rsplit('\r').next().unwrap_or(line);
    if !line.contains('\t') {
        return Cow::Borrowed(line);
    }
    let mut out = String::with_capacity(line.len() + TAB_WIDTH);
    let mut column = 0;
    for c in line.chars() {
        if c == '\t' {
            let n = TAB_WIDTH - column % TAB_WIDTH;
            out.extend(std::iter::repeat_n(' ', n));
            column += n;
        } else {
            out.push(c);
            column += 1;
        }
    }
    Cow::Owned(out)
}

/// What a console sink sends to the UI.
pub enum ConsoleMessage {
    Event(DateTime<Local>, ConsoleEvent),
    Silent(DateTime<Local>, std::path::PathBuf, String),
    /// Messages were dropped before this point (the channel was full).
    Dropped,
}

/// The `ConsoleSink` given to a project's `Git`: forwards events over a bounded channel (it runs
/// on git threads and never blocks: when the channel is full the message is dropped and the next
/// message that fits is preceded by one [`ConsoleMessage::Dropped`]).
pub struct ChannelConsole(pub flume::Sender<ConsoleMessage>, Arc<AtomicBool>);

impl ChannelConsole {
    pub fn new(sender: flume::Sender<ConsoleMessage>) -> ChannelConsole {
        ChannelConsole(sender, Arc::new(AtomicBool::new(false)))
    }

    fn send(&self, message: ConsoleMessage) {
        if self.1.load(Ordering::Relaxed) && self.0.try_send(ConsoleMessage::Dropped).is_ok() {
            self.1.store(false, Ordering::Relaxed);
        }
        if self.0.try_send(message).is_err() {
            self.1.store(true, Ordering::Relaxed);
        }
    }
}

impl ConsoleSink for ChannelConsole {
    fn event(&self, event: ConsoleEvent) {
        self.send(ConsoleMessage::Event(Local::now(), event));
    }

    fn silent_command(&self, working_dir: &Path, command_line: &str) {
        self.send(ConsoleMessage::Silent(
            Local::now(),
            working_dir.to_path_buf(),
            command_line.to_string(),
        ));
    }
}

/// The console's lines, bounded by [`MAX_LINES`] and [`MAX_BYTES`].
#[derive(Default)]
pub struct ConsoleBuffer {
    lines: VecDeque<ConsoleLine>,
    bytes: usize,
    silent: usize,
}

impl ConsoleBuffer {
    pub fn push(&mut self, message: ConsoleMessage) {
        self.push_line(message);
    }

    /// Adds the line of `message`, if it has one; returns whether that line is a silent
    /// command's.
    pub fn push_line(&mut self, message: ConsoleMessage) -> Option<bool> {
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
                text: format_command_start(time, &working_dir, &command_line).into(),
                silent: false,
            },
            ConsoleMessage::Event(_, ConsoleEvent::Output { kind, line, .. }) => ConsoleLine {
                kind: if kind == OutputKind::Stderr {
                    LineKind::Error
                } else {
                    LineKind::Normal
                },
                text: SharedString::from(shorten_output(&clean_output(&line)).into_owned()),
                silent: false,
            },
            // `showCommandFinished` prints nothing
            ConsoleMessage::Event(_, ConsoleEvent::Finished { .. }) => return None,
            ConsoleMessage::Silent(time, dir, command_line) => ConsoleLine {
                kind: LineKind::System,
                text: format_command_start(time, &dir, &command_line).into(),
                silent: true,
            },
            ConsoleMessage::Dropped => ConsoleLine {
                kind: LineKind::System,
                text: DROPPED_MARKER.into(),
                silent: false,
            },
        };
        let silent = line.silent;
        self.bytes += line.text.len();
        self.silent += usize::from(silent);
        self.lines.push_back(line);
        while self.lines.len() > MAX_LINES || (self.bytes > MAX_BYTES && self.lines.len() > 1) {
            if let Some(old) = self.lines.pop_front() {
                self.bytes -= old.text.len();
                self.silent -= usize::from(old.silent);
            }
        }
        Some(silent)
    }

    /// The lines shown, with or without silent commands.
    pub fn visible(&self, show_silent: bool) -> impl Iterator<Item = &ConsoleLine> + '_ {
        self.lines.iter().filter(move |l| show_silent || !l.silent)
    }

    /// How many lines [`visible`](Self::visible) yields.
    pub fn visible_count(&self, show_silent: bool) -> usize {
        if show_silent {
            self.lines.len()
        } else {
            self.lines.len() - self.silent
        }
    }

    /// Bytes of text held.
    pub fn bytes(&self) -> usize {
        self.bytes
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
                batch.extend(rx.try_iter().take(CHANNEL_CAPACITY));
                if this
                    .update(cx, |view, cx| {
                        // as `ConsoleViewImpl`: scroll to the end only when already there
                        let at_bottom =
                            view.follow || view.scroll.is_scrolled_to_end() != Some(false);
                        let mut added = false;
                        for m in batch {
                            if let Some(silent) = view.buffer.push_line(m) {
                                added |= !silent || view.show_silent;
                            }
                        }
                        if added {
                            view.follow = at_bottom;
                            cx.notify();
                        }
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
        self.buffer.visible_count(self.show_silent)
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
        let count = self.line_count();
        if self.follow && count > 0 {
            self.scroll
                .scroll_to_item(count - 1, gpui::ScrollStrategy::Bottom);
        }
        self.follow = false;
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
                uniform_list(
                    "git-console-lines",
                    count,
                    // only the visible range is read from the buffer (refcount bumps)
                    cx.processor(move |this, range: Range<usize>, _, _| {
                        this.buffer
                            .visible(this.show_silent)
                            .skip(range.start)
                            .take(range.len())
                            .map(|l| {
                                let fg = colors.iter().find(|(k, _)| *k == l.kind).map(|(_, c)| *c);
                                div()
                                    .px(px(8.0))
                                    .whitespace_nowrap()
                                    .when_some(fg, |d, c| d.text_color(c))
                                    .child(l.text.clone())
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .size_full()
                .track_scroll(&self.scroll),
            )
    }
}

/// Creates the console view and the sink to give to the project's `Git`.
pub fn create(cx: &mut App) -> (Entity<ConsoleView>, ChannelConsole) {
    let (tx, rx) = flume::bounded(CHANNEL_CAPACITY);
    (
        cx.new(|cx| ConsoleView::new(rx, cx)),
        ChannelConsole::new(tx),
    )
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
        let kinds: Vec<LineKind> = buffer.visible(false).map(|l| l.kind).collect();
        assert_eq!(kinds, [LineKind::System, LineKind::Normal, LineKind::Error]);
        assert_eq!(buffer.visible_count(false), 3);
        assert_eq!(buffer.visible_count(true), 4);
        assert!(
            buffer
                .visible(true)
                .nth(3)
                .unwrap()
                .text
                .ends_with("[/r] git log")
        );
    }

    #[test]
    fn long_lines_keep_head_and_tail() {
        let long = "a".repeat(5000) + &"b".repeat(10_000);
        let short = shorten(&long);
        assert_eq!(short.chars().count(), MAX_CONSOLE_OUTPUT_SIZE);
        assert!(short.starts_with("aaa") && short.ends_with("bbb") && short.contains("..."));
    }

    fn output(line: &str) -> ConsoleMessage {
        ConsoleMessage::Event(
            Local::now(),
            ConsoleEvent::Output {
                id: 1,
                kind: OutputKind::Stdout,
                line: line.into(),
            },
        )
    }

    #[test]
    fn output_is_cut_only_beyond_one_point_two_times_the_maximum() {
        let at_limit = "x".repeat(12_000);
        assert!(matches!(shorten_output(&at_limit), Cow::Borrowed(_)));
        let long = "a".repeat(5_000) + &"é".repeat(8_000);
        let cut = shorten_output(&long);
        assert_eq!(cut.chars().count(), 3_000 + 3 + 7_000);
        assert!(cut.starts_with(&"a".repeat(3_000)) && cut.ends_with(&"é".repeat(7_000)));
        assert_eq!(&cut[3_000..3_003], "...");
        assert!(matches!(shorten_output("short"), Cow::Borrowed(_)));
    }

    #[test]
    fn carriage_returns_and_tabs_are_cleaned() {
        assert_eq!(clean_output("10%\r50%\r100%"), "100%");
        assert_eq!(clean_output("done\r"), "done");
        assert_eq!(clean_output("a\tb\t\tc"), "a       b               c");
        assert!(matches!(clean_output("plain"), Cow::Borrowed(_)));
        let mut buffer = ConsoleBuffer::default();
        buffer.push(output("x\r\ty"));
        assert_eq!(buffer.visible(false).next().unwrap().text, "        y");
    }

    #[test]
    fn buffer_is_bounded_by_lines_and_bytes() {
        let mut buffer = ConsoleBuffer::default();
        for i in 0..MAX_LINES + 5 {
            buffer.push(output(&i.to_string()));
        }
        assert_eq!(buffer.visible_count(false), MAX_LINES);
        assert_eq!(buffer.visible(false).next().unwrap().text, "5");

        let mut buffer = ConsoleBuffer::default();
        let big = "z".repeat(9_000);
        for _ in 0..2_000 {
            buffer.push(output(&big));
        }
        assert!(buffer.bytes() <= MAX_BYTES);
        assert!(buffer.visible_count(false) >= MAX_BYTES / 9_000 - 1);
        assert!(buffer.visible_count(false) < 2_000);
    }

    #[test]
    fn full_channel_drops_and_marks_once() {
        let (tx, rx) = flume::bounded(2);
        let sink = ChannelConsole::new(tx);
        for _ in 0..6 {
            sink.silent_command(Path::new("/r"), "git log");
        }
        assert_eq!(rx.len(), 2);
        rx.drain().for_each(drop);
        sink.silent_command(Path::new("/r"), "git log");
        sink.silent_command(Path::new("/r"), "git log");
        let received: Vec<_> = rx.drain().collect();
        let markers = received
            .iter()
            .filter(|m| matches!(m, ConsoleMessage::Dropped))
            .count();
        assert_eq!(markers, 1);
        assert!(matches!(received[0], ConsoleMessage::Dropped));
        let mut buffer = ConsoleBuffer::default();
        buffer.push(ConsoleMessage::Dropped);
        assert_eq!(buffer.visible(false).next().unwrap().text, DROPPED_MARKER);
    }

    #[test]
    fn silent_lines_do_not_count_unless_shown() {
        let mut buffer = ConsoleBuffer::default();
        assert_eq!(
            buffer.push_line(ConsoleMessage::Silent(
                Local::now(),
                "/r".into(),
                "git x".into()
            )),
            Some(true)
        );
        assert_eq!(buffer.push_line(output("a")), Some(false));
        assert_eq!(buffer.visible_count(false), 1);
        assert_eq!(buffer.visible_count(true), 2);
    }
}
