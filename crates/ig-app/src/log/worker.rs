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
//!   [`LogEvent::WorkerFailed`]; [`LogWorker::refresh`] restarts a dead worker.
//!
//! Deviation from the app-shell spec ("one dedicated thread"): two threads, because the refresher
//! is synchronous and the first block must be shown while `initialize()` still loads the full
//! history, and metadata must load while a refresh runs.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use ig_git::log::provider::GitLogProvider;
use ig_git::log::repository_manager::GitRepositoryManager;
use ig_git::runner::Git;
use ig_log::data::{
    InMemoryStorage, LogProviders, VcsCommitMetadata, VcsLogCommitStorageIndex, VcsLogGraphData,
    VcsLogProvider, VcsLogRefresher, VcsLogStorage, VcsRoot, vcs_root,
};
use ig_log::{GraphCommit, Options, PermanentGraph, VisibleGraph, VisibleGraphImpl};

use super::metadata::{BATCH_SIZE, MAX_LOADING_TASKS, RowMeta};

/// `vcs.log.recent.commits.count`.
pub const RECENT_COMMITS_COUNT: usize = 1000;
/// Rows whose metadata comes with a snapshot (about two screens).
pub const FIRST_ROWS: i32 = 100;

/// An immutable view of a data pack, built on the snapshot thread and moved to the UI.
pub struct LogSnapshot {
    /// Increases with every snapshot of a worker.
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
    Busy(bool),
    /// A worker thread panicked; the message is logged.
    WorkerFailed(String),
}

enum RefreshCommand {
    Refresh,
}

enum SnapshotCommand {
    Pack(Arc<VcsLogGraphData>),
    Recent(VcsRoot, Vec<VcsCommitMetadata>),
    Load(Vec<VcsLogCommitStorageIndex>),
}

/// Sends events to the UI; returns `false` once the UI is gone.
pub type EventSink = Arc<dyn Fn(LogEvent) -> bool + Send + Sync>;

/// The UI's handle on the worker threads.
pub struct LogWorker {
    git: Arc<Git>,
    roots: Vec<PathBuf>,
    sink: EventSink,
    refresh_tx: Sender<RefreshCommand>,
    snapshot_tx: Sender<SnapshotCommand>,
    threads: Vec<JoinHandle<()>>,
}

impl LogWorker {
    /// Starts loading the log of `roots` (`initialize()`).
    pub fn start(git: Arc<Git>, roots: Vec<PathBuf>, sink: EventSink) -> LogWorker {
        let (refresh_tx, refresh_rx) = channel();
        let (snapshot_tx, snapshot_rx) = channel();
        let threads = spawn_threads(
            git.clone(),
            &roots,
            sink.clone(),
            refresh_rx,
            snapshot_tx.clone(),
            snapshot_rx,
        );
        LogWorker {
            git,
            roots,
            sink,
            refresh_tx,
            snapshot_tx,
            threads,
        }
    }

    fn is_alive(&self) -> bool {
        self.threads.iter().all(|t| !t.is_finished())
    }

    /// Requests a refresh of all roots; restarts the worker if a thread died.
    pub fn refresh(&mut self) {
        if !self.is_alive() {
            log::info!("restarting the log worker");
            *self = LogWorker::start(self.git.clone(), self.roots.clone(), self.sink.clone());
            return;
        }
        let _ = self.refresh_tx.send(RefreshCommand::Refresh);
    }

    /// Requests the metadata of `commits` (already marked loading in the UI cache).
    pub fn load_metadata(&self, commits: Vec<VcsLogCommitStorageIndex>) {
        if !commits.is_empty() {
            let _ = self.snapshot_tx.send(SnapshotCommand::Load(commits));
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "unknown panic".to_string())
}

fn guarded(
    name: &'static str,
    sink: EventSink,
    body: impl FnOnce() + Send + 'static,
) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            if let Err(payload) = catch_unwind(AssertUnwindSafe(body)) {
                let message = format!("{name} panicked: {}", panic_message(payload.as_ref()));
                log::error!("{message}");
                sink(LogEvent::Busy(false));
                sink(LogEvent::WorkerFailed(message));
            }
        })
        .expect("spawning a log worker thread")
}

fn spawn_threads(
    git: Arc<Git>,
    roots: &[PathBuf],
    sink: EventSink,
    refresh_rx: Receiver<RefreshCommand>,
    snapshot_tx: Sender<SnapshotCommand>,
    snapshot_rx: Receiver<SnapshotCommand>,
) -> Vec<JoinHandle<()>> {
    let repository_manager = Arc::new(GitRepositoryManager::new(git));
    let provider = Arc::new(GitLogProvider::with_repository_manager(
        repository_manager.clone(),
    ));
    let roots: Vec<VcsRoot> = roots.iter().map(vcs_root).collect();
    let storage = Arc::new(InMemoryStorage::new());

    let snapshot_thread = {
        let sink = sink.clone();
        let storage: Arc<dyn VcsLogStorage> = storage.clone();
        let repository_manager = repository_manager.clone();
        guarded("ig-log-snapshot", sink.clone(), move || {
            SnapshotThread::new(sink, storage, repository_manager).run(snapshot_rx)
        })
    };
    let refresh_thread = guarded("ig-log-refresh", sink.clone(), move || {
        let mut providers = LogProviders::new();
        for root in &roots {
            providers.insert(root.clone(), provider.clone() as Arc<dyn VcsLogProvider>);
        }
        let mut refresher = VcsLogRefresher::new(storage, providers, RECENT_COMMITS_COUNT);
        let tx = snapshot_tx.clone();
        refresher.set_data_pack_handler(move |pack| {
            let _ = tx.send(SnapshotCommand::Pack(pack.clone()));
        });
        let tx = snapshot_tx;
        refresher.set_recent_details_consumer(move |root, details| {
            let _ = tx.send(SnapshotCommand::Recent(root.clone(), details.to_vec()));
        });
        sink(LogEvent::Busy(true));
        refresher.initialize();
        sink(LogEvent::Busy(false));
        while let Ok(RefreshCommand::Refresh) = refresh_rx.recv() {
            // coalesce the requests queued meanwhile
            while refresh_rx.try_recv().is_ok() {}
            sink(LogEvent::Busy(true));
            for root in &roots {
                repository_manager.update(root);
            }
            refresher.refresh(&roots);
            if !sink(LogEvent::Busy(false)) {
                break;
            }
        }
    });
    vec![refresh_thread, snapshot_thread]
}

struct SnapshotThread {
    sink: EventSink,
    storage: Arc<dyn VcsLogStorage>,
    repository_manager: Arc<GitRepositoryManager>,
    generation: u64,
    current: Option<Arc<VcsLogGraphData>>,
    /// Commits whose metadata the UI has (or was sent), so first rows aren't loaded twice.
    known: HashSet<VcsLogCommitStorageIndex>,
}

impl SnapshotThread {
    fn new(
        sink: EventSink,
        storage: Arc<dyn VcsLogStorage>,
        repository_manager: Arc<GitRepositoryManager>,
    ) -> SnapshotThread {
        SnapshotThread {
            sink,
            storage,
            repository_manager,
            generation: 0,
            current: None,
            known: HashSet::new(),
        }
    }

    fn run(mut self, rx: Receiver<SnapshotCommand>) {
        let mut loads: VecDeque<Vec<VcsLogCommitStorageIndex>> = VecDeque::new();
        loop {
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
            for command in first.into_iter().chain(rx.try_iter()) {
                match command {
                    SnapshotCommand::Pack(pack) => newest_pack = Some(pack),
                    SnapshotCommand::Recent(root, details) => self.recent(&root, &details),
                    SnapshotCommand::Load(commits) => {
                        loads.push_back(commits);
                        // LIFO, limited: drop the oldest requests
                        while loads.len() > MAX_LOADING_TASKS as usize {
                            loads.pop_front();
                        }
                    }
                }
            }
            if let Some(pack) = newest_pack
                && !self.snapshot(pack)
            {
                return;
            }
            if let Some(commits) = loads.pop_back()
                && !self.load(&commits)
            {
                return;
            }
        }
    }

    fn recent(&mut self, root: &VcsRoot, details: &[VcsCommitMetadata]) {
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
        (self.sink)(LogEvent::Metadata {
            loaded,
            failed: Vec::new(),
        });
    }

    fn snapshot(&mut self, data: Arc<VcsLogGraphData>) -> bool {
        self.generation += 1;
        let visible = data
            .permanent_graph()
            .and_then(|g| g.create_visible_graph(Options::DEFAULT, None, None).ok());
        let recommended_width = visible.as_ref().map_or(0, |v| v.recommended_width());
        if let Some(visible) = &visible {
            let first: Vec<_> = (0..visible.visible_commit_count().min(FIRST_ROWS))
                .map(|row| visible.get_row_info(row).commit())
                .filter(|c| !self.known.contains(c))
                .collect();
            self.current = Some(data.clone());
            if !self.load(&first) {
                return false;
            }
        }
        let repos = data
            .log_providers()
            .keys()
            .map(|root| {
                let repo = self.repository_manager.get_repository(root);
                RepoSummary {
                    root: root.clone(),
                    current_branch: repo.current_branch_name().map(str::to_string),
                    remotes: repo.remotes.iter().map(|r| r.name.clone()).collect(),
                }
            })
            .collect();
        self.current = Some(data.clone());
        (self.sink)(LogEvent::Snapshot(Box::new(LogSnapshot {
            generation: self.generation,
            data,
            visible,
            recommended_width,
            repos,
        })))
    }

    /// Reads the metadata of `commits` by root, in batches; reports loaded and failed commits.
    fn load(&mut self, commits: &[VcsLogCommitStorageIndex]) -> bool {
        let Some(data) = self.current.clone() else {
            return (self.sink)(LogEvent::Metadata {
                loaded: Vec::new(),
                failed: commits.to_vec(),
            });
        };
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
            let Some(provider) = data.log_providers().get(&root) else {
                failed.extend(hashes.iter().map(|(_, c)| *c));
                continue;
            };
            for batch in hashes.chunks(BATCH_SIZE) {
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
        self.known.extend(loaded.iter().map(|(c, _)| *c));
        (self.sink)(LogEvent::Metadata { loaded, failed })
    }
}
