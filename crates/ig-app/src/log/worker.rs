//! The log worker of a project window: loads the log in the background and sends immutable
//! snapshots and row metadata to the UI.
//!
//! Follows the threading notes of Plan 2c (`docs/superpowers/plans/2026-11-08-plan-2c-git-log-data.md`)
//! and upstream's `VcsLogRefresherImpl` / `MiniDetailsGetter` split:
//! - The **refresher thread** owns the `VcsLogRefresher`. It runs `initialize()`, then serves
//!   refresh requests, coalescing those queued meanwhile into one `repository_manager.update()` +
//!   `refresh(roots)` (upstream batches requests in its refresh queue).
//! - The data-pack handler and the recent-details consumer only forward to the **snapshot
//!   thread** over a channel; they never call back into the refresher (re-entrancy rule).
//! - The snapshot thread builds the visible graph of the newest pack (`Options::DEFAULT`), loads
//!   the metadata of its first rows, and sends a [`LogSnapshot`]. It also serves the UI's metadata
//!   requests in batches of [`BATCH_SIZE`], newest request first, keeping the newest
//!   [`MAX_LOADING_TASKS`] (upstream `SequentialLimitedLifoExecutor`).
//! - A panic on either thread is caught at the thread boundary, logged and reported as
//!   [`LogEvent::WorkerFailed`]; [`LogWorker::refresh`] restarts a dead worker under a new epoch
//!   (events carry their epoch; the UI drops stale snapshots and busy states, but keeps metadata,
//!   which is keyed by commit id). Storage and repository manager are kept, so commit ids stay
//!   stable; the new epoch rereads every root's repository state before `initialize()`.
//! - Epochs never load concurrently: the new refresh thread waits for the old one to finish (it
//!   was cancelled, so it stops at its next check) before reading anything.
//! - Dropping the [`LogWorker`] (closing the window) cancels the epoch: the threads stop at the
//!   next check (data-pack handler, recent-details consumer, each provider read - so between
//!   roots and between the first block and the full log -, between metadata batches, the snapshot
//!   loop) without starting new work. A running `git` process is not killed.
//!
//! Deviation from the app-shell spec ("one dedicated thread"): two threads, because the refresher
//! is synchronous and the first block must be shown while `initialize()` still loads the full
//! history, and metadata must load while a refresh runs.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use ig_git::log::provider::GitLogProvider;
use ig_git::log::repository_manager::GitRepositoryManager;
use ig_git::runner::Git;
use ig_log::data::{
    DetailedLogData, InMemoryStorage, LogData, LogProviders, Requirements, TimedVcsCommit,
    VcsCommitMetadata, VcsException, VcsLogCommitStorageIndex, VcsLogGraphData, VcsLogProvider,
    VcsLogRefManager, VcsLogRefresher, VcsLogStorage, VcsRoot, VcsUser, vcs_root,
};
use ig_log::{GraphCommit, Options, PermanentGraph, VisibleGraph, VisibleGraphImpl};

use super::metadata::{BATCH_SIZE, MAX_LOADING_TASKS, RowMeta};

/// `vcs.log.recent.commits.count`.
pub const RECENT_COMMITS_COUNT: usize = 1000;
/// Rows whose metadata comes with a snapshot (about two screens).
pub const FIRST_ROWS: i32 = 100;

/// An immutable view of a data pack, built on the snapshot thread and moved to the UI.
pub struct LogSnapshot {
    /// Increases with every snapshot of a worker, also across restarts.
    pub generation: u64,
    pub data: Arc<VcsLogGraphData>,
    /// `None` for an empty or failed pack.
    pub visible: Option<VisibleGraphImpl<VcsLogCommitStorageIndex>>,
    pub recommended_width: i32,
    /// Each root's current branch and remotes, as read when the snapshot was built.
    pub repos: Vec<RepoSummary>,
}

/// The repository state the Log UI needs (status bar branch, label grouping).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoSummary {
    pub root: VcsRoot,
    /// `None` when HEAD is detached or there are no commits.
    pub current_branch: Option<String>,
    /// The full hash of HEAD; `None` before the first commit.
    pub head: Option<String>,
    pub remotes: Vec<String>,
}

impl LogSnapshot {
    pub fn repo(&self, root: &VcsRoot) -> Option<&RepoSummary> {
        self.repos.iter().find(|r| &r.root == root)
    }
}

impl LogSnapshot {
    pub fn row_count(&self) -> i32 {
        self.visible
            .as_ref()
            .map_or(0, |v| v.visible_commit_count())
    }

    pub fn error(&self) -> Option<&str> {
        self.data.load_error().map(|e| e.message())
    }
}

/// What the worker tells the UI.
pub enum LogEvent {
    Snapshot(Box<LogSnapshot>),
    Metadata {
        loaded: Vec<(VcsLogCommitStorageIndex, RowMeta)>,
        failed: Vec<VcsLogCommitStorageIndex>,
    },
    /// A refresh (or the initial load) started / finished: shown as status bar progress.
    /// `Busy(false)` follows the snapshot of the refresh's final data pack.
    Busy(bool),
    /// A worker thread panicked; the message is logged.
    WorkerFailed(String),
}

enum RefreshCommand {
    Refresh,
    /// Test hook: the refresh thread counts as reading until the sender is dropped or sends.
    #[cfg(test)]
    Hold(Receiver<()>),
}

enum SnapshotCommand {
    Pack(Arc<VcsLogGraphData>),
    Recent(VcsRoot, Vec<VcsCommitMetadata>),
    Load(Vec<VcsLogCommitStorageIndex>),
    /// A refresh started (`true`) or published its final pack (`false`). Sent through the snapshot
    /// thread so `Busy(false)` reaches the UI after the final snapshot.
    Busy(bool),
    /// Test hook: makes the snapshot thread panic.
    #[cfg(test)]
    Panic,
}

/// Sends events to the UI, tagged with the epoch of the worker threads that sent them (see
/// [`LogWorker::epoch`]); returns `false` once the UI is gone.
pub type EventSink = Arc<dyn Fn(u64, LogEvent) -> bool + Send + Sync>;

/// The flags shared by the threads of one epoch.
#[derive(Default)]
struct EpochState {
    /// Set when the worker is dropped or restarted, or a sibling thread failed: no new work.
    cancelled: AtomicBool,
    /// Set (before [`LogEvent::WorkerFailed`] is sent) when a thread of the epoch panicked.
    failed: AtomicBool,
}

impl EpochState {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// Counts the log reads (first block, full history) running at once, across epochs.
#[derive(Default)]
struct ReadGauge {
    active: AtomicUsize,
    /// The most reads seen running at once.
    max: AtomicUsize,
}

/// Decrements [`ReadGauge::active`] when a read ends (also by a panic).
struct Reading<'a>(&'a ReadGauge);

impl ReadGauge {
    fn enter(&self) -> Reading<'_> {
        let active = self.active.fetch_add(1, Ordering::AcqRel) + 1;
        self.max.fetch_max(active, Ordering::AcqRel);
        Reading(self)
    }
}

impl Drop for Reading<'_> {
    fn drop(&mut self) {
        self.0.active.fetch_sub(1, Ordering::AcqRel);
    }
}

/// The provider of one epoch: once the epoch is cancelled, log reads fail without running `git`,
/// so a cancelled `initialize()` / `refresh()` stops at its next root or step (the resulting
/// error data pack is dropped by the cancelled data-pack handler). The refresher itself has no
/// cancellation (see its module docs).
struct EpochProvider {
    inner: Arc<dyn VcsLogProvider>,
    state: Arc<EpochState>,
    reads: Arc<ReadGauge>,
}

impl EpochProvider {
    fn check(&self) -> Result<(), VcsException> {
        if self.state.is_cancelled() {
            Err(VcsException::new("the log worker was stopped"))
        } else {
            Ok(())
        }
    }
}

impl VcsLogProvider for EpochProvider {
    fn read_first_block(
        &self,
        root: &VcsRoot,
        requirements: &Requirements<'_>,
    ) -> Result<DetailedLogData, VcsException> {
        self.check()?;
        let _reading = self.reads.enter();
        self.inner.read_first_block(root, requirements)
    }

    fn read_all_hashes(
        &self,
        root: &VcsRoot,
        commit_consumer: &mut dyn FnMut(TimedVcsCommit),
    ) -> Result<LogData, VcsException> {
        self.check()?;
        let _reading = self.reads.enter();
        self.inner.read_all_hashes(root, commit_consumer)
    }

    fn read_metadata(
        &self,
        root: &VcsRoot,
        hashes: &[String],
        consumer: &mut dyn FnMut(VcsCommitMetadata),
    ) -> Result<(), VcsException> {
        self.check()?;
        self.inner.read_metadata(root, hashes, consumer)
    }

    fn reference_manager(&self) -> Arc<dyn VcsLogRefManager> {
        self.inner.reference_manager()
    }

    fn get_current_user(&self, root: &VcsRoot) -> Result<Option<VcsUser>, VcsException> {
        self.inner.get_current_user(root)
    }

    fn get_current_branch(&self, root: &VcsRoot) -> Option<String> {
        self.inner.get_current_branch(root)
    }

    fn is_full_hash(&self, root: &VcsRoot, hash: &str) -> bool {
        self.inner.is_full_hash(root, hash)
    }
}

/// Sends the events of one epoch.
#[derive(Clone)]
struct Emitter {
    sink: EventSink,
    epoch: u64,
}

impl Emitter {
    fn send(&self, event: LogEvent) -> bool {
        (self.sink)(self.epoch, event)
    }
}

/// The UI's handle on the worker threads.
///
/// The storage (commit ids) and the repository manager outlive the threads: a restart after a
/// crash keeps the ids the UI's snapshot, cache and selection refer to. Dropping the handle
/// cancels the running work (checked between steps; a running `git` process is not killed).
pub struct LogWorker {
    roots: Vec<PathBuf>,
    sink: EventSink,
    storage: Arc<InMemoryStorage>,
    repository_manager: Arc<GitRepositoryManager>,
    /// Snapshot generations, increasing across restarts.
    generation: Arc<AtomicU64>,
    epoch: u64,
    state: Arc<EpochState>,
    refresh_tx: Sender<RefreshCommand>,
    snapshot_tx: Sender<SnapshotCommand>,
    /// The refresh thread, then the snapshot thread, of the current epoch.
    threads: Vec<JoinHandle<()>>,
    reads: Arc<ReadGauge>,
}

/// What the threads of one epoch share.
struct Shared {
    roots: Vec<VcsRoot>,
    emitter: Emitter,
    storage: Arc<InMemoryStorage>,
    repository_manager: Arc<GitRepositoryManager>,
    generation: Arc<AtomicU64>,
    state: Arc<EpochState>,
    reads: Arc<ReadGauge>,
    /// The refresh thread of the previous epoch, to wait for before loading.
    previous: Option<JoinHandle<()>>,
}

impl LogWorker {
    /// Starts loading the log of `roots` (`initialize()`).
    pub fn start(git: Arc<Git>, roots: Vec<PathBuf>, sink: EventSink) -> LogWorker {
        let (refresh_tx, _) = channel();
        let (snapshot_tx, _) = channel();
        let mut worker = LogWorker {
            roots,
            sink,
            storage: Arc::new(InMemoryStorage::new()),
            repository_manager: Arc::new(GitRepositoryManager::new(git)),
            generation: Arc::new(AtomicU64::new(0)),
            epoch: 0,
            state: Arc::new(EpochState::default()),
            refresh_tx,
            snapshot_tx,
            threads: Vec::new(),
            reads: Arc::new(ReadGauge::default()),
        };
        worker.spawn();
        worker
    }

    /// The epoch of the running threads: events of older epochs are stale.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Cancels the current threads (if any) and starts new ones under a new epoch. The new refresh
    /// thread waits for the old one before loading, so two epochs never load at once.
    fn spawn(&mut self) {
        self.state.cancel();
        // the old snapshot thread is detached: it stops at its next check
        let previous = (!self.threads.is_empty()).then(|| self.threads.swap_remove(0));
        self.epoch += 1;
        self.state = Arc::new(EpochState::default());
        let (refresh_tx, refresh_rx) = channel();
        let (snapshot_tx, snapshot_rx) = channel();
        let shared = Shared {
            roots: self.roots.iter().map(vcs_root).collect(),
            emitter: Emitter {
                sink: self.sink.clone(),
                epoch: self.epoch,
            },
            storage: self.storage.clone(),
            repository_manager: self.repository_manager.clone(),
            generation: self.generation.clone(),
            state: self.state.clone(),
            reads: self.reads.clone(),
            previous,
        };
        self.threads = spawn_threads(shared, refresh_rx, snapshot_tx.clone(), snapshot_rx);
        // dropping the old senders lets the old threads' loops end (an idle refresh thread
        // returns from `recv`)
        self.refresh_tx = refresh_tx;
        self.snapshot_tx = snapshot_tx;
    }

    fn is_alive(&self) -> bool {
        !self.state.failed.load(Ordering::Acquire) && self.threads.iter().all(|t| !t.is_finished())
    }

    /// Requests a refresh of all roots; restarts the worker if a thread died. Returns whether it
    /// restarted: the metadata requests pending in the old epoch are then lost.
    pub fn refresh(&mut self) -> bool {
        if !self.is_alive() {
            log::info!("restarting the log worker");
            self.spawn();
            return true;
        }
        let _ = self.refresh_tx.send(RefreshCommand::Refresh);
        false
    }

    /// Requests the metadata of `commits` (already marked loading in the UI cache).
    pub fn load_metadata(&self, commits: Vec<VcsLogCommitStorageIndex>) {
        if !commits.is_empty() {
            let _ = self.snapshot_tx.send(SnapshotCommand::Load(commits));
        }
    }

    /// Test hook: makes the current snapshot thread panic.
    #[cfg(test)]
    pub(crate) fn panic_snapshot_thread(&self) {
        let _ = self.snapshot_tx.send(SnapshotCommand::Panic);
    }
}

impl Drop for LogWorker {
    fn drop(&mut self) {
        self.state.cancel();
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

/// Runs `body` on a named thread; a panic marks the epoch failed and cancels its other thread
/// before [`LogEvent::WorkerFailed`] is sent, so a refresh requested on that event restarts.
fn guarded(
    name: &'static str,
    emitter: Emitter,
    state: Arc<EpochState>,
    body: impl FnOnce() + Send + 'static,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            if let Err(payload) = catch_unwind(AssertUnwindSafe(body)) {
                let message = format!("{name} panicked: {}", panic_message(payload.as_ref()));
                log::error!("{message}");
                state.failed.store(true, Ordering::Release);
                state.cancel();
                emitter.send(LogEvent::WorkerFailed(message));
            }
        })
        .expect("spawning a log worker thread")
}

fn spawn_threads(
    shared: Shared,
    refresh_rx: Receiver<RefreshCommand>,
    snapshot_tx: Sender<SnapshotCommand>,
    snapshot_rx: Receiver<SnapshotCommand>,
) -> Vec<JoinHandle<()>> {
    let Shared {
        roots,
        emitter,
        storage,
        repository_manager,
        generation,
        state,
        reads,
        previous,
    } = shared;
    let provider: Arc<dyn VcsLogProvider> = Arc::new(EpochProvider {
        inner: Arc::new(GitLogProvider::with_repository_manager(
            repository_manager.clone(),
        )),
        state: state.clone(),
        reads: reads.clone(),
    });
    let providers: LogProviders = roots
        .iter()
        .map(|root| (root.clone(), provider.clone()))
        .collect();

    let snapshot_thread = {
        let thread = SnapshotThread::new(
            emitter.clone(),
            storage.clone(),
            repository_manager.clone(),
            providers.clone(),
            generation,
            state.clone(),
        );
        guarded(
            "ig-log-snapshot",
            emitter.clone(),
            state.clone(),
            move || thread.run(snapshot_rx),
        )
    };
    let refresh_thread = guarded("ig-log-refresh", emitter, state.clone(), move || {
        let _ = snapshot_tx.send(SnapshotCommand::Busy(true));
        // never load while the previous epoch still does (its panic was already reported)
        if let Some(previous) = previous {
            let _ = previous.join();
        }
        // the repository state may have changed since the previous epoch read it (the refresh
        // that restarted the worker asked for exactly this)
        for root in &roots {
            if state.is_cancelled() {
                return;
            }
            repository_manager.update(root);
        }
        if state.is_cancelled() {
            return;
        }
        let mut refresher = VcsLogRefresher::new(storage, providers, RECENT_COMMITS_COUNT);
        let (tx, cancel) = (snapshot_tx.clone(), state.clone());
        refresher.set_data_pack_handler(move |pack| {
            if !cancel.is_cancelled() {
                let _ = tx.send(SnapshotCommand::Pack(pack.clone()));
            }
        });
        let (tx, cancel) = (snapshot_tx.clone(), state.clone());
        refresher.set_recent_details_consumer(move |root, details| {
            if cancel.is_cancelled() {
                return;
            }
            // only the first block: the 5x retry of a refresh would evict visible rows from
            // the UI cache (upstream keeps these in a separate `TopCommitsCache`)
            let details = &details[..details.len().min(RECENT_COMMITS_COUNT)];
            let _ = tx.send(SnapshotCommand::Recent(root.clone(), details.to_vec()));
        });
        refresher.initialize();
        let _ = snapshot_tx.send(SnapshotCommand::Busy(false));
        'serve: while let Ok(command) = refresh_rx.recv() {
            match command {
                RefreshCommand::Refresh => {}
                #[cfg(test)]
                RefreshCommand::Hold(release) => {
                    let _reading = reads.enter();
                    let _ = release.recv();
                    continue;
                }
            }
            // coalesce the requests queued meanwhile
            while refresh_rx.try_recv().is_ok() {}
            if state.is_cancelled() {
                break;
            }
            let _ = snapshot_tx.send(SnapshotCommand::Busy(true));
            for root in &roots {
                if state.is_cancelled() {
                    break 'serve;
                }
                repository_manager.update(root);
            }
            if state.is_cancelled() {
                break;
            }
            refresher.refresh(&roots);
            if snapshot_tx.send(SnapshotCommand::Busy(false)).is_err() {
                break;
            }
        }
    });
    vec![refresh_thread, snapshot_thread]
}

struct SnapshotThread {
    emitter: Emitter,
    storage: Arc<dyn VcsLogStorage>,
    repository_manager: Arc<GitRepositoryManager>,
    /// The epoch's providers, so metadata loads before the epoch's first snapshot.
    providers: LogProviders,
    generation: Arc<AtomicU64>,
    state: Arc<EpochState>,
    /// Commits whose metadata the UI was sent recently, so first rows aren't loaded twice: the
    /// recent details received since the last snapshot and that snapshot's first rows (bounded).
    known: HashSet<VcsLogCommitStorageIndex>,
    /// The busy state last sent to the UI.
    busy: bool,
}

impl SnapshotThread {
    fn new(
        emitter: Emitter,
        storage: Arc<dyn VcsLogStorage>,
        repository_manager: Arc<GitRepositoryManager>,
        providers: LogProviders,
        generation: Arc<AtomicU64>,
        state: Arc<EpochState>,
    ) -> SnapshotThread {
        SnapshotThread {
            emitter,
            storage,
            repository_manager,
            providers,
            generation,
            state,
            known: HashSet::new(),
            busy: false,
        }
    }

    fn run(mut self, rx: Receiver<SnapshotCommand>) {
        let mut loads: VecDeque<Vec<VcsLogCommitStorageIndex>> = VecDeque::new();
        loop {
            if self.state.is_cancelled() {
                return;
            }
            // block for one command, then take everything queued
            let first = if loads.is_empty() {
                match rx.recv() {
                    Ok(command) => Some(command),
                    Err(_) => return,
                }
            } else {
                None
            };
            let mut newest_pack = None;
            let mut busy = self.busy;
            for command in first.into_iter().chain(rx.try_iter()) {
                match command {
                    SnapshotCommand::Pack(pack) => newest_pack = Some(pack),
                    SnapshotCommand::Recent(root, details) => {
                        if !self.recent(&root, &details) {
                            return;
                        }
                    }
                    SnapshotCommand::Load(commits) => {
                        loads.push_back(commits);
                        // LIFO, limited: drop the oldest requests
                        while loads.len() > MAX_LOADING_TASKS as usize {
                            loads.pop_front();
                        }
                    }
                    SnapshotCommand::Busy(b) => busy = b,
                    #[cfg(test)]
                    SnapshotCommand::Panic => panic!("test panic"),
                }
            }
            if self.state.is_cancelled() {
                return;
            }
            if let Some(pack) = newest_pack
                && !self.snapshot(pack)
            {
                return;
            }
            if busy != self.busy {
                self.busy = busy;
                if !self.emitter.send(LogEvent::Busy(busy)) {
                    return;
                }
            }
            if let Some(commits) = loads.pop_back()
                && !self.load(&commits)
            {
                return;
            }
        }
    }

    fn recent(&mut self, root: &VcsRoot, details: &[VcsCommitMetadata]) -> bool {
        let loaded: Vec<_> = details
            .iter()
            .map(|d| {
                (
                    self.storage.get_commit_index(d.id(), root),
                    RowMeta::from_metadata(d),
                )
            })
            .collect();
        self.known.extend(loaded.iter().map(|(i, _)| *i));
        self.emitter.send(LogEvent::Metadata {
            loaded,
            failed: Vec::new(),
        })
    }

    fn snapshot(&mut self, data: Arc<VcsLogGraphData>) -> bool {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let visible = data
            .permanent_graph()
            .and_then(|g| g.create_visible_graph(Options::DEFAULT, None, None).ok());
        let recommended_width = visible.as_ref().map_or(0, |v| v.recommended_width());
        if let Some(visible) = &visible {
            let first_rows: Vec<_> = (0..visible.visible_commit_count().min(FIRST_ROWS))
                .map(|row| visible.get_row_info(row).commit())
                .collect();
            let missing: Vec<_> = first_rows
                .iter()
                .copied()
                .filter(|c| !self.known.contains(c))
                .collect();
            self.known = first_rows.into_iter().collect();
            if !self.load(&missing) {
                return false;
            }
        } else {
            self.known.clear();
        }
        let repos = data
            .log_providers()
            .keys()
            .map(|root| {
                let repo = self.repository_manager.get_repository(root);
                RepoSummary {
                    root: root.clone(),
                    current_branch: repo.current_branch_name().map(str::to_string),
                    head: repo.state.current_revision.as_ref().map(|h| h.to_string()),
                    remotes: repo.remotes.iter().map(|r| r.name.clone()).collect(),
                }
            })
            .collect();
        self.emitter.send(LogEvent::Snapshot(Box::new(LogSnapshot {
            generation,
            data,
            visible,
            recommended_width,
            repos,
        })))
    }

    /// Reads the metadata of `commits` by root, in batches; reports loaded and failed commits.
    fn load(&mut self, commits: &[VcsLogCommitStorageIndex]) -> bool {
        if commits.is_empty() {
            return true;
        }
        let mut by_root: BTreeMap<VcsRoot, Vec<(String, VcsLogCommitStorageIndex)>> =
            BTreeMap::new();
        let mut failed = Vec::new();
        for &commit in commits {
            match self.storage.get_commit_id(commit) {
                Some(id) => by_root
                    .entry(id.root)
                    .or_default()
                    .push((id.hash.as_string(), commit)),
                None => failed.push(commit),
            }
        }
        let mut loaded = Vec::with_capacity(commits.len());
        for (root, hashes) in by_root {
            let Some(provider) = self.providers.get(&root) else {
                failed.extend(hashes.iter().map(|(_, c)| *c));
                continue;
            };
            for batch in hashes.chunks(BATCH_SIZE) {
                if self.state.is_cancelled() {
                    return false;
                }
                let index: HashMap<&str, VcsLogCommitStorageIndex> =
                    batch.iter().map(|(h, c)| (h.as_str(), *c)).collect();
                let names: Vec<String> = batch.iter().map(|(h, _)| h.clone()).collect();
                let mut found = Vec::with_capacity(batch.len());
                let result = provider.read_metadata(&root, &names, &mut |m| {
                    if let Some(&commit) = index.get(m.id().as_string().as_str()) {
                        found.push((commit, RowMeta::from_metadata(&m)));
                    }
                });
                match result {
                    Ok(()) => {
                        let got: HashSet<_> = found.iter().map(|(c, _)| *c).collect();
                        failed.extend(batch.iter().map(|(_, c)| *c).filter(|c| !got.contains(c)));
                        loaded.extend(found);
                    }
                    Err(e) => {
                        log::warn!("reading metadata in {}: {}", root.display(), e.message());
                        failed.extend(batch.iter().map(|(_, c)| *c));
                    }
                }
            }
        }
        self.emitter.send(LogEvent::Metadata { loaded, failed })
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::time::{Duration, Instant};

    use ig_git::console::NullConsole;

    use super::*;

    /// Inherited variables that would redirect git away from the test repository (as in
    /// `crates/ig-git/tests/common/mod.rs`).
    const SCRUBBED_ENV: &[&str] = &[
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CEILING_DIRECTORIES",
        "GIT_NAMESPACE",
        "GIT_PREFIX",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG",
        "GIT_AUTHOR_DATE",
        "GIT_COMMITTER_DATE",
    ];

    /// A repository with `n` commits on `main` and a branch `other` at HEAD, plus an empty global
    /// git config next to it.
    struct Repo {
        dir: tempfile::TempDir,
        config: PathBuf,
    }

    impl Repo {
        fn new(n: usize) -> Repo {
            let dir = tempfile::tempdir().unwrap();
            let config = dir.path().join("empty-gitconfig");
            std::fs::write(&config, "").unwrap();
            std::fs::create_dir(dir.path().join("repo")).unwrap();
            let repo = Repo { dir, config };
            repo.git(&["init", "-q"]);
            repo.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
            for i in 0..n {
                repo.git(&["commit", "-q", "--allow-empty", "-m", &format!("c{i}")]);
            }
            repo.git(&["branch", "other"]);
            repo
        }

        fn root(&self) -> PathBuf {
            self.dir.path().join("repo").canonicalize().unwrap()
        }

        fn git(&self, args: &[&str]) {
            let mut command = Command::new("git");
            command
                .args(args)
                .current_dir(self.dir.path().join("repo"))
                .env("GIT_CONFIG_GLOBAL", &self.config)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_AUTHOR_NAME", "T")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "T")
                .env("GIT_COMMITTER_EMAIL", "t@example.com");
            for v in SCRUBBED_ENV {
                command.env_remove(v);
            }
            let output = command.output().unwrap();
            assert!(output.status.success(), "git {args:?}");
        }

        /// The runner the worker uses, isolated the same way.
        fn runner(&self) -> Arc<Git> {
            let git = Git::detect("git", Arc::new(NullConsole))
                .unwrap()
                .with_base_environment(
                    &[
                        ("GIT_CONFIG_GLOBAL", self.config.to_str().unwrap()),
                        ("GIT_CONFIG_NOSYSTEM", "1"),
                    ],
                    SCRUBBED_ENV,
                );
            Arc::new(git)
        }
    }

    fn start(repo: &Repo) -> (LogWorker, Receiver<(u64, LogEvent)>) {
        let (tx, rx) = channel();
        let tx = std::sync::Mutex::new(tx);
        let sink: EventSink =
            Arc::new(move |epoch, event| tx.lock().unwrap().send((epoch, event)).is_ok());
        (LogWorker::start(repo.runner(), vec![repo.root()], sink), rx)
    }

    /// The events up to the first one `until` accepts (included).
    fn events_until(
        rx: &Receiver<(u64, LogEvent)>,
        mut until: impl FnMut(u64, &LogEvent) -> bool,
    ) -> Vec<(u64, LogEvent)> {
        let started = Instant::now();
        let mut events = Vec::new();
        loop {
            let left = Duration::from_secs(30).saturating_sub(started.elapsed());
            let (epoch, event) = rx.recv_timeout(left).expect("timed out");
            let done = until(epoch, &event);
            events.push((epoch, event));
            if done {
                return events;
            }
        }
    }

    fn snapshots(events: &[(u64, LogEvent)]) -> Vec<&LogSnapshot> {
        events
            .iter()
            .filter_map(|(_, e)| match e {
                LogEvent::Snapshot(s) => Some(&**s),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_restart_rereads_the_repository_and_loads_metadata_right_away() {
        let repo = Repo::new(3);
        let (mut worker, rx) = start(&repo);
        assert_eq!(worker.epoch(), 1);
        let events = events_until(&rx, |_, e| matches!(e, LogEvent::Busy(false)));
        let last = *snapshots(&events).last().unwrap();
        assert!(last.data.is_full());
        assert_eq!(last.repos[0].current_branch.as_deref(), Some("main"));
        let visible = last.visible.as_ref().unwrap();
        let commits: Vec<_> = (0..visible.visible_commit_count())
            .map(|r| visible.get_row_info(r).commit())
            .collect();
        assert_eq!(commits.len(), 3);

        worker.panic_snapshot_thread();
        events_until(&rx, |epoch, e| {
            epoch == 1 && matches!(e, LogEvent::WorkerFailed(_))
        });
        // the repository changes while the worker is down
        repo.git(&["checkout", "-q", "other"]);
        assert!(worker.refresh(), "a failed worker restarts");
        assert_eq!(worker.epoch(), 2);
        // requested before the new epoch's first snapshot: loads anyway
        worker.load_metadata(commits.clone());
        let events = events_until(&rx, |epoch, e| {
            epoch == 2 && matches!(e, LogEvent::Busy(false))
        });
        let mut loaded = HashSet::new();
        for (_, event) in &events {
            if let LogEvent::Metadata { loaded: l, failed } = event {
                assert!(failed.is_empty(), "metadata failed: {failed:?}");
                loaded.extend(l.iter().map(|(c, _)| *c));
            }
        }
        assert!(commits.iter().all(|c| loaded.contains(c)));
        let last = *snapshots(&events).last().unwrap();
        // same commit ids, and the repository state read again
        assert_eq!(
            last.visible.as_ref().unwrap().get_row_info(0).commit(),
            commits[0]
        );
        assert_eq!(last.repos[0].current_branch.as_deref(), Some("other"));
        // the new epoch didn't load while the old one did
        assert_eq!(worker.reads.max.load(Ordering::Acquire), 1);
        // a healthy worker just refreshes
        assert!(!worker.refresh());
        assert_eq!(worker.epoch(), 2);
    }

    #[test]
    fn a_restart_waits_for_the_old_epochs_read() {
        let repo = Repo::new(3);
        let (mut worker, rx) = start(&repo);
        events_until(&rx, |_, e| matches!(e, LogEvent::Busy(false)));
        // the old refresh thread is stuck in a (simulated) long git read
        let (release, hold) = channel();
        worker.refresh_tx.send(RefreshCommand::Hold(hold)).unwrap();
        let started = Instant::now();
        while worker.reads.active.load(Ordering::Acquire) == 0 {
            assert!(started.elapsed() < Duration::from_secs(30), "timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
        worker.panic_snapshot_thread();
        events_until(&rx, |_, e| matches!(e, LogEvent::WorkerFailed(_)));
        assert!(worker.refresh());
        // the new epoch waits instead of loading next to it
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(worker.reads.max.load(Ordering::Acquire), 1);
        while let Ok((epoch, event)) = rx.try_recv() {
            assert!(
                !(epoch == 2 && matches!(event, LogEvent::Snapshot(_))),
                "the new epoch loaded while the old one read"
            );
        }
        release.send(()).unwrap();
        let events = events_until(&rx, |epoch, e| {
            epoch == 2 && matches!(e, LogEvent::Busy(false))
        });
        assert!(snapshots(&events).last().unwrap().data.is_full());
        assert_eq!(worker.reads.max.load(Ordering::Acquire), 1);
    }

    #[test]
    fn a_cancelled_epoch_reads_nothing() {
        let repo = Repo::new(2);
        let git = repo.runner();
        let state = Arc::new(EpochState::default());
        let reads = Arc::new(ReadGauge::default());
        let provider = EpochProvider {
            inner: Arc::new(GitLogProvider::new(git)),
            state: state.clone(),
            reads: reads.clone(),
        };
        let root = vcs_root(repo.root());
        let first = provider.read_first_block(&root, &Requirements::simple(10));
        assert_eq!(first.unwrap().commits.len(), 2);
        assert_eq!(reads.active.load(Ordering::Acquire), 0);
        state.cancel();
        assert!(
            provider
                .read_first_block(&root, &Requirements::simple(10))
                .is_err()
        );
        let mut count = 0;
        assert!(
            provider
                .read_all_hashes(&root, &mut |_| count += 1)
                .is_err()
        );
        assert_eq!(count, 0);
        assert!(provider.read_metadata(&root, &[], &mut |_| {}).is_err());
        assert_eq!(reads.max.load(Ordering::Acquire), 1);
    }

    #[test]
    fn dropping_the_worker_stops_its_threads() {
        let repo = Repo::new(2);
        let (worker, rx) = start(&repo);
        events_until(&rx, |_, e| matches!(e, LogEvent::Busy(false)));
        let threads: Vec<_> = worker.threads.iter().map(|t| t.thread().clone()).collect();
        assert_eq!(threads.len(), 2);
        let state = worker.state.clone();
        drop(worker);
        assert!(state.is_cancelled());
        // both threads end: the event channel closes once their senders are gone
        let started = Instant::now();
        while rx.recv_timeout(Duration::from_millis(50)).is_ok() || Arc::strong_count(&state) > 1 {
            assert!(
                started.elapsed() < Duration::from_secs(30),
                "threads still running"
            );
        }
    }
}
