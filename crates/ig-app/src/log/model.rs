//! The Log tab's state on the UI thread: the newest snapshot, the metadata cache, the selection
//! and the graph's mouse actions.
//!
//! Graph actions follow `platform/vcs-log/impl/src/com/intellij/vcs/log/ui/table/GraphCommitCellController.java`
//! and `VcsLogGraphTable.handleAnswer`: hovering an element asks the `ActionController` (it
//! highlights the edge and sets the hand cursor); clicking an arrow jumps to the commit it points
//! to (select + scroll). Print elements are snapshots, so rows are re-read on every repaint.

use std::path::PathBuf;
use std::sync::Arc;

use gpui::{Context, EventEmitter, ScrollStrategy, UniformListScrollHandle};
use ig_git::runner::Git;
use ig_log::actions::{Cursor, GraphAction, GraphActionType};
use ig_log::data::VcsLogCommitStorageIndex;
use ig_log::{GraphPrintElement, VisibleGraph};

use super::graph_painter::GraphGeometry;
use super::metadata::{Lookup, MetadataCache};
use super::selection::{Rows, Selection};
use super::worker::{EventSink, LogEvent, LogSnapshot, LogWorker};

/// `GraphTableModel.UP_PRELOAD_COUNT` / `DOWN_PRELOAD_COUNT`.
const UP_PRELOAD_COUNT: usize = 20;
const DOWN_PRELOAD_COUNT: usize = 40;

/// What the Log tab shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogState {
    NoRoots,
    Loading,
    Ready,
    Error(String),
}

/// Events for the project window.
pub enum LogModelEvent {
    WorkerFailed(String),
    BusyChanged,
}

pub struct LogModel {
    roots: Vec<PathBuf>,
    worker: Option<LogWorker>,
    snapshot: Option<LogSnapshot>,
    pub cache: MetadataCache,
    pub selection: Selection,
    busy: bool,
    pub scroll: UniformListScrollHandle,
    hovered_element: Option<GraphPrintElement>,
    pub hand_cursor: bool,
    /// Bumped whenever rows change (a new snapshot, or a graph action that collapsed or expanded
    /// edges); row indices captured by painted rows of an older generation are stale.
    pub generation: u64,
    /// When the worker started, for the "first rows painted" log line.
    pub started: std::time::Instant,
    pub first_rows_logged: bool,
}

impl EventEmitter<LogModelEvent> for LogModel {}

struct SnapshotRows<'a>(&'a dyn VisibleGraph<VcsLogCommitStorageIndex>);

impl Rows for SnapshotRows<'_> {
    fn count(&self) -> usize {
        self.0.visible_commit_count().max(0) as usize
    }
    fn commit(&self, row: usize) -> VcsLogCommitStorageIndex {
        self.0.get_row_info(row as i32).commit()
    }
    fn row(&self, commit: VcsLogCommitStorageIndex) -> Option<usize> {
        self.0.get_visible_row_index(&commit).map(|r| r as usize)
    }
}

impl LogModel {
    /// Starts the worker for `roots` (none: the "no repositories" state).
    pub fn new(git: Arc<Git>, roots: Vec<PathBuf>, cx: &mut Context<LogModel>) -> LogModel {
        let worker = (!roots.is_empty()).then(|| {
            let (tx, rx) = flume::unbounded::<(u64, LogEvent)>();
            cx.spawn(async move |this, cx| {
                while let Ok(event) = rx.recv_async().await {
                    let mut events = vec![event];
                    events.extend(rx.try_iter());
                    if this
                        .update(cx, |model, cx| {
                            for (epoch, event) in events {
                                model.on_event(epoch, event, cx);
                            }
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            })
            .detach();
            let sink: EventSink = Arc::new(move |epoch, event| tx.send((epoch, event)).is_ok());
            LogWorker::start(git, roots.clone(), sink)
        });
        LogModel {
            roots,
            worker,
            snapshot: None,
            cache: MetadataCache::new(),
            selection: Selection::default(),
            busy: false,
            scroll: UniformListScrollHandle::new(),
            hovered_element: None,
            hand_cursor: false,
            generation: 0,
            started: std::time::Instant::now(),
            first_rows_logged: false,
        }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn is_busy(&self) -> bool {
        self.busy
    }

    pub fn snapshot(&self) -> Option<&LogSnapshot> {
        self.snapshot.as_ref()
    }

    pub fn state(&self) -> LogState {
        if self.roots.is_empty() {
            return LogState::NoRoots;
        }
        match &self.snapshot {
            None => LogState::Loading,
            Some(s) => match s.error() {
                Some(e) => LogState::Error(e.to_string()),
                None => LogState::Ready,
            },
        }
    }

    pub fn row_count(&self) -> usize {
        self.snapshot
            .as_ref()
            .map_or(0, |s| s.row_count().max(0) as usize)
    }

    /// `Vcs.Log.Refresh`: also restarts a crashed worker.
    pub fn refresh(&mut self) {
        if let Some(worker) = &mut self.worker {
            worker.refresh();
        }
    }

    fn on_event(&mut self, epoch: u64, event: LogEvent, cx: &mut Context<LogModel>) {
        // events of threads replaced by a restart are stale
        if self.worker.as_ref().is_none_or(|w| w.epoch() != epoch) {
            return;
        }
        match event {
            LogEvent::Snapshot(snapshot) => {
                log::info!(
                    "log snapshot {} ({} rows, full: {}) after {:?}",
                    snapshot.generation,
                    snapshot.row_count(),
                    snapshot.data.is_full(),
                    self.started.elapsed()
                );
                self.generation += 1;
                self.hovered_element = None;
                self.hand_cursor = false;
                self.snapshot = Some(*snapshot);
                if let Some(visible) = self.snapshot.as_ref().and_then(|s| s.visible.as_ref()) {
                    self.selection.retain_visible(&SnapshotRows(visible));
                } else {
                    self.selection = Selection::default();
                }
            }
            LogEvent::Metadata { loaded, failed } => {
                if !failed.is_empty() {
                    log::debug!("metadata of {} commits failed to load", failed.len());
                }
                self.cache.complete(loaded, &failed);
            }
            LogEvent::Busy(busy) => {
                self.busy = busy;
                cx.emit(LogModelEvent::BusyChanged);
            }
            LogEvent::WorkerFailed(message) => {
                self.busy = false;
                cx.emit(LogModelEvent::WorkerFailed(message));
            }
        }
        cx.notify();
    }

    /// The commit and print elements of `row`; `None` if there is no such row (e.g. a row index
    /// from an older snapshot).
    pub fn row(&self, row: usize) -> Option<(VcsLogCommitStorageIndex, Vec<GraphPrintElement>)> {
        let visible = self.snapshot.as_ref()?.visible.as_ref()?;
        let row = i32::try_from(row)
            .ok()
            .filter(|&r| r < visible.visible_commit_count())?;
        let info = visible.get_row_info(row);
        Some((info.commit(), info.print_elements()))
    }

    /// Whether the first `rows` rows have their metadata (for timing logs).
    pub fn metadata_loaded_for(&mut self, rows: usize) -> bool {
        rows > 0
            && rows <= self.row_count()
            && (0..rows).all(|r| {
                self.row(r)
                    .is_some_and(|(c, _)| matches!(self.cache.get(c), Lookup::Loaded(_)))
            })
    }

    pub fn metadata(&mut self, commit: VcsLogCommitStorageIndex) -> Lookup {
        self.cache.get(commit)
    }

    /// Requests the metadata of the rows around `range` (`GraphTableModel.getCommitMetadata`
    /// preloading).
    pub fn request_metadata(&mut self, range: std::ops::Range<usize>) {
        let count = self.row_count();
        let Some(visible) = self.snapshot.as_ref().and_then(|s| s.visible.as_ref()) else {
            return;
        };
        let from = range.start.saturating_sub(UP_PRELOAD_COUNT);
        let to = (range.end + DOWN_PRELOAD_COUNT).min(count);
        let commits: Vec<_> = (from..to)
            .map(|r| visible.get_row_info(r as i32).commit())
            .collect();
        let missing = self.cache.request(commits);
        if let Some(worker) = &self.worker {
            worker.load_metadata(missing);
        }
    }

    fn with_rows<R>(&mut self, f: impl FnOnce(&mut Selection, &dyn Rows) -> R) -> Option<R> {
        let visible = self.snapshot.as_ref()?.visible.as_ref()?;
        Some(f(&mut self.selection, &SnapshotRows(visible)))
    }

    /// A click on `row` (ignored if out of range).
    pub fn click_row(&mut self, row: usize, toggle: bool, extend: bool) {
        self.with_rows(|s, rows| s.click(row, toggle, extend, rows));
    }

    pub fn move_selection(&mut self, delta: isize, extend: bool) {
        if let Some(Some(row)) = self.with_rows(|s, rows| s.move_by(delta, extend, rows)) {
            self.scroll.scroll_to_item(row, ScrollStrategy::Nearest);
        }
    }

    /// Home / End (`selectFirstRow` / `selectLastRow`): the first or last row, with or without a
    /// selection.
    pub fn select_first(&mut self) {
        self.move_to(0);
    }

    pub fn select_last(&mut self) {
        self.move_to(usize::MAX);
    }

    fn move_to(&mut self, row: usize) {
        if let Some(Some(row)) = self.with_rows(|s, rows| s.move_to(row, rows)) {
            self.scroll.scroll_to_item(row, ScrollStrategy::Nearest);
        }
    }

    /// Rows that fit the table's viewport (Page Up / Page Down step), from the last layout; 1
    /// before the table was laid out.
    pub fn page_rows(&self) -> usize {
        let state = self.scroll.0.borrow();
        let viewport = f32::from(state.base_handle.bounds().size.height);
        let row = state
            .last_item_size
            .map_or(0.0, |size| f32::from(size.item.height));
        if row > 0.0 && viewport > row {
            (viewport / row).floor() as usize
        } else {
            1
        }
    }

    /// Page Up / Page Down: moves the lead by a viewport of rows.
    pub fn move_page(&mut self, down: bool) {
        let page = self.page_rows() as isize;
        self.move_selection(if down { page } else { -page }, false);
    }

    pub fn select_all(&mut self) {
        self.with_rows(|s, rows| s.select_all(rows));
    }

    pub fn is_selected(&self, commit: VcsLogCommitStorageIndex) -> bool {
        self.selection.is_selected(commit)
    }

    fn visible_mut(&mut self) -> Option<&mut ig_log::VisibleGraphImpl<VcsLogCommitStorageIndex>> {
        self.snapshot.as_mut()?.visible.as_mut()
    }

    /// Mouse over the graph of `row` at `(x, y)` (row coordinates). Returns whether to repaint.
    /// A row out of range counts as no element.
    pub fn hover_graph(&mut self, row: usize, x: f32, y: f32, geometry: &GraphGeometry) -> bool {
        let element = self
            .row(row)
            .and_then(|(_, elements)| geometry.element_under(&elements, x, y));
        if element == self.hovered_element {
            return false;
        }
        self.hovered_element = element.clone();
        self.perform(GraphAction::new(element, GraphActionType::MouseOver))
    }

    /// The pointer is over `row` but outside its graph: drops the hover if it was on that row's
    /// graph (moving from the graph to the subject keeps no edge highlighted or hand cursor).
    pub fn left_graph_of(&mut self, row: usize) -> bool {
        use ig_log::print_element::PrintElement as _;
        let on_row = self
            .hovered_element
            .as_ref()
            .is_some_and(|e| usize::try_from(e.row_index()).ok() == Some(row));
        on_row && self.unhover_graph()
    }

    /// The mouse left the graph.
    pub fn unhover_graph(&mut self) -> bool {
        if self.hovered_element.is_none() {
            return false;
        }
        self.hovered_element = None;
        self.perform(GraphAction::new(None, GraphActionType::MouseOver))
    }

    /// A click on the graph of `row`. Returns `true` if it was handled (an element was hit), so the
    /// click doesn't change the selection (`VcsLogGraphTable.shouldChangeSelect`).
    pub fn click_graph(&mut self, row: usize, x: f32, y: f32, geometry: &GraphGeometry) -> bool {
        let Some(element) = self
            .row(row)
            .and_then(|(_, elements)| geometry.element_under(&elements, x, y))
        else {
            return false;
        };
        self.click_element(element);
        true
    }

    /// Clicks a print element of the current graph (an arrow jumps to its target). Elements the
    /// current graph doesn't print (e.g. from an older snapshot) are ignored.
    pub fn click_element(&mut self, element: GraphPrintElement) {
        use ig_log::print_element::PrintElement as _;
        let current = usize::try_from(element.row_index())
            .ok()
            .and_then(|row| self.row(row))
            .is_some_and(|(_, elements)| elements.contains(&element));
        if current {
            self.perform(GraphAction::new(Some(element), GraphActionType::MouseClick));
        }
    }

    /// Runs an action and handles the answer (`GraphCommitCellController.handleGraphAnswer`).
    fn perform(&mut self, action: GraphAction) -> bool {
        let Some(visible) = self.visible_mut() else {
            return false;
        };
        let answer = visible.action_controller().perform_action(action);
        if let Some(updater) = answer.graph_updater {
            visible.run_graph_updater(updater);
            // rows may have moved: handlers painted for the old rows must not act on new ones
            self.generation += 1;
            self.hovered_element = None;
        }
        if let Some(cursor) = answer.cursor_to_set {
            self.hand_cursor = cursor == Cursor::Hand;
        }
        if let (Some(commit), true) = (answer.commit_to_jump, answer.do_jump) {
            self.jump_to_commit(commit);
        }
        true
    }

    /// Selects and scrolls to `commit` (`VcsLogGraphTable.jumpToRow`).
    pub fn jump_to_commit(&mut self, commit: VcsLogCommitStorageIndex) -> Option<usize> {
        let row = self
            .snapshot
            .as_ref()?
            .visible
            .as_ref()?
            .get_visible_row_index(&commit)? as usize;
        self.with_rows(|s, rows| s.select_only(row, rows));
        self.scroll.scroll_to_item(row, ScrollStrategy::Center);
        Some(row)
    }
}
