//! Loading the log and keeping it up to date.
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogRefresherImpl.kt`
//! (with `RefreshSessionData`).
//!
//! Like upstream, initialization shows the first block of recent commits, then the whole history;
//! a refresh reads the recent commits again and joins them to the loaded log
//! ([`joiner`]), reading 5x more once if they don't reach it, and reloads
//! everything if that still fails.
//!
//! Deviations:
//! - Synchronous: each call runs on the caller's thread (the UI calls it on a background executor
//!   and serializes calls). There is no request queue, so concurrent requests are not batched;
//!   pass all changed roots to one [`VcsLogRefresher::refresh`]. No progress, cancellation,
//!   overlay data or tracing.
//! - Every commit list is checked before the graph is built: commit ids must be unique and every
//!   parent that is in the list must come after its child (upstream builds the graph silently,
//!   treating such parents as not loaded). A joined log failing the check is treated like a failed
//!   join (full reload); a first block or full log failing it gives an error data pack.
//! - Roots are processed in path order (upstream: `HashMap` order), so multi-repository joins are
//!   deterministic.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use crate::data::commit::VcsCommitMetadata;
use crate::data::graph_data::{LogGraphCommit, LogProviders, VcsLogGraphData, build_data};
use crate::data::hash::Hash;
use crate::data::joiner;
use crate::data::multi_repo_joiner;
use crate::data::provider::{Requirements, VcsException};
use crate::data::refs_model::RootRefsModel;
use crate::data::root::VcsRoot;
use crate::data::storage::{VcsLogCommitStorageIndex, VcsLogStorage};
use crate::data::vcs_ref::VcsRefsList;
use crate::graph_commit::{GraphCommit, GraphCommitImpl};
use crate::permanent_graph::PermanentGraph;

/// Upstream `EmptyRefs`.
static EMPTY_REFS: VcsRefsList = VcsRefsList(Vec::new());

type DataPackHandler = Box<dyn FnMut(&Arc<VcsLogGraphData>) + Send>;
type RecentDetailsConsumer = Box<dyn FnMut(&VcsRoot, &[VcsCommitMetadata]) + Send>;

/// Commits and refs read in one refresh, by root (`RefreshSessionData`).
#[derive(Default)]
struct RefreshSessionData {
    refs_by_root: BTreeMap<VcsRoot, Arc<RootRefsModel>>,
    commits_by_root: BTreeMap<VcsRoot, Vec<LogGraphCommit>>,
}

impl RefreshSessionData {
    fn put_all(&mut self, other: RefreshSessionData) {
        self.commits_by_root.extend(other.commits_by_root);
        self.refs_by_root.extend(other.refs_by_root);
    }

    fn put(&mut self, root: VcsRoot, commits: Vec<LogGraphCommit>, refs: RootRefsModel) {
        self.commits_by_root.insert(root.clone(), commits);
        self.refs_by_root.insert(root, Arc::new(refs));
    }
}

/// Loads and refreshes the log of a set of roots (`VcsLogRefresherImpl`, synchronous).
pub struct VcsLogRefresher {
    storage: Arc<dyn VcsLogStorage>,
    providers: LogProviders,
    recent_commit_count: usize,
    current_data_pack: Arc<VcsLogGraphData>,
    data_pack_handler: Option<DataPackHandler>,
    recent_details_consumer: Option<RecentDetailsConsumer>,
}

impl VcsLogRefresher {
    /// `recent_commit_count` is the size of the first block (upstream default: 1000, from the
    /// registry key `vcs.log.recent.commits.count`).
    pub fn new(
        storage: Arc<dyn VcsLogStorage>,
        providers: LogProviders,
        recent_commit_count: usize,
    ) -> VcsLogRefresher {
        VcsLogRefresher {
            current_data_pack: Arc::new(VcsLogGraphData::empty(storage.clone())),
            storage,
            providers,
            recent_commit_count,
            data_pack_handler: None,
            recent_details_consumer: None,
        }
    }

    /// Called with every new data pack (upstream `dataPackUpdateHandler`), on the refreshing thread.
    pub fn set_data_pack_handler(
        &mut self,
        handler: impl FnMut(&Arc<VcsLogGraphData>) + Send + 'static,
    ) {
        self.data_pack_handler = Some(Box::new(handler));
    }

    /// Receives the metadata read with each first block (upstream
    /// `VcsLogCommitDataConsumer.storeRecentDetails`), e.g. to cache the top rows of the table.
    pub fn set_recent_details_consumer(
        &mut self,
        consumer: impl FnMut(&VcsRoot, &[VcsCommitMetadata]) + Send + 'static,
    ) {
        self.recent_details_consumer = Some(Box::new(consumer));
    }

    pub fn storage(&self) -> &Arc<dyn VcsLogStorage> {
        &self.storage
    }

    pub fn current_data_pack(&self) -> &Arc<VcsLogGraphData> {
        &self.current_data_pack
    }

    fn publish(&mut self, data_pack: VcsLogGraphData) -> Arc<VcsLogGraphData> {
        let data_pack = Arc::new(data_pack);
        self.current_data_pack = data_pack.clone();
        if let Some(handler) = &mut self.data_pack_handler {
            handler(&data_pack);
        }
        data_pack
    }

    /// `initialize`: publishes the first block, then loads and publishes the full log, which is
    /// returned.
    pub fn initialize(&mut self) -> Arc<VcsLogGraphData> {
        let first = self
            .load_first_block()
            .unwrap_or_else(|e| VcsLogGraphData::error(self.storage.clone(), e));
        self.publish(first);
        let roots: Vec<VcsRoot> = self.providers.keys().cloned().collect();
        self.refresh(&roots)
    }

    /// `refresh` (one request): rereads the recent commits of `roots` and joins them to the log,
    /// or loads the full log if it is not loaded yet or can't be joined. Publishes and returns the
    /// new data pack (the current one when `roots` is empty).
    pub fn refresh(&mut self, roots: &[VcsRoot]) -> Arc<VcsLogGraphData> {
        if roots.is_empty() {
            return self.current_data_pack.clone();
        }
        let current = self.current_data_pack.clone();
        let result = (|| {
            let partially_loaded_log = if current.is_full() {
                self.load_updated_data_pack(&current, roots)?
            } else {
                None
            };
            match partially_loaded_log {
                Some(data_pack) => Ok(data_pack),
                None => self.load_full_log(),
            }
        })();
        let data_pack = result.unwrap_or_else(|e| VcsLogGraphData::error(self.storage.clone(), e));
        self.publish(data_pack)
    }

    fn load_first_block(&mut self) -> Result<VcsLogGraphData, VcsException> {
        let count = self.recent_commit_count;
        let requirements = self
            .providers
            .keys()
            .map(|root| (root.clone(), Requirements::simple(count)))
            .collect();
        let data = self.load_recent_data(requirements)?;
        let mut compound_list =
            multi_repo_joiner::join(data.commits_by_root.into_values().collect());
        compound_list.truncate(count);
        check_commits(&compound_list).map_err(VcsException)?;
        Ok(build_data(
            &compound_list,
            data.refs_by_root,
            &self.providers,
            &self.storage,
            false,
        ))
    }

    fn load_updated_data_pack(
        &mut self,
        data_pack: &VcsLogGraphData,
        roots: &[VcsRoot],
    ) -> Result<Option<VcsLogGraphData>, VcsException> {
        let full_log: Vec<LogGraphCommit> = data_pack
            .permanent_graph()
            .map(|graph| graph.all_commits().iter().collect())
            .unwrap_or_default();
        let current_refs = data_pack.refs_model().refs_by_root().clone();
        let mut session = RefreshSessionData::default();
        let mut commit_count = self.recent_commit_count;
        for _ in 0..2 {
            let requirements = prepare_requirements(roots, commit_count, &current_refs);
            let attempt = self.load_recent_data(requirements)?;
            session.put_all(attempt);
            let compound_log =
                multi_repo_joiner::join(session.commits_by_root.values().cloned().collect());
            let all_new_refs: BTreeMap<VcsRoot, Arc<RootRefsModel>> = current_refs
                .iter()
                .map(|(root, refs)| {
                    let refs = session.refs_by_root.get(root).unwrap_or(refs);
                    (root.clone(), refs.clone())
                })
                .collect();
            let joined = join(&full_log, compound_log, &current_refs, &all_new_refs)
                .filter(|joined| check_commits(joined).is_ok());
            if let Some(joined) = joined {
                return Ok(Some(build_data(
                    &joined,
                    all_new_refs,
                    &self.providers,
                    &self.storage,
                    true,
                )));
            }
            commit_count *= 5;
        }
        // couldn't join => need to reload everything
        Ok(None)
    }

    fn load_full_log(&mut self) -> Result<VcsLogGraphData, VcsException> {
        let data = self.read_full_log_from_vcs()?;
        let graph_commits = multi_repo_joiner::join(data.commits_by_root.into_values().collect());
        check_commits(&graph_commits).map_err(VcsException)?;
        Ok(build_data(
            &graph_commits,
            data.refs_by_root,
            &self.providers,
            &self.storage,
            true,
        ))
    }

    fn load_recent_data(
        &mut self,
        requirements: Vec<(VcsRoot, Requirements<'_>)>,
    ) -> Result<RefreshSessionData, VcsException> {
        let mut session = RefreshSessionData::default();
        for (root, requirements) in requirements {
            let provider = self
                .providers
                .get(&root)
                .ok_or_else(|| {
                    VcsException(format!("Cannot find provider for root {}", root.display()))
                })?
                .clone();
            let data = provider.read_first_block(&root, &requirements)?;
            let commits = data
                .commits
                .iter()
                .map(|c| compact_commit(self.storage.as_ref(), c, &root))
                .collect();
            let refs = RootRefsModel::create(data.refs, self.storage.as_ref());
            session.put(root.clone(), commits, refs);
            if let Some(consumer) = &mut self.recent_details_consumer {
                consumer(&root, &data.commits);
            }
        }
        Ok(session)
    }

    fn read_full_log_from_vcs(&mut self) -> Result<RefreshSessionData, VcsException> {
        let mut session = RefreshSessionData::default();
        for (root, provider) in &self.providers {
            let mut graph_commits = Vec::new();
            let storage = self.storage.as_ref();
            let data = provider.read_all_hashes(root, &mut |commit| {
                graph_commits.push(compact_commit(storage, &commit, root));
            })?;
            let refs = RootRefsModel::create(data.refs, storage);
            session.put(root.clone(), graph_commits, refs);
        }
        Ok(session)
    }
}

/// `prepareRequirements` with previous refs.
fn prepare_requirements<'a>(
    roots: &[VcsRoot],
    commit_count: usize,
    prev_refs: &'a BTreeMap<VcsRoot, Arc<RootRefsModel>>,
) -> Vec<(VcsRoot, Requirements<'a>)> {
    roots
        .iter()
        .map(|root| {
            let requirements = match prev_refs.get(root) {
                None => Requirements::refresh_with(commit_count, true, &EMPTY_REFS, false),
                Some(refs) => Requirements::refresh(commit_count, true, refs.as_ref()),
            };
            (root.clone(), requirements)
        })
        .collect()
}

/// `compactCommit`: the commit with storage indexes.
pub fn compact_commit(
    storage: &dyn VcsLogStorage,
    commit: &impl GraphCommit<Hash>,
    root: &VcsRoot,
) -> LogGraphCommit {
    let hashes: Vec<&Hash> = std::iter::once(commit.id())
        .chain(commit.parents())
        .collect();
    let mut indexes = storage.get_commit_indexes(&hashes, root);
    let id = indexes.remove(0);
    GraphCommitImpl::create_commit(id, indexes, commit.timestamp())
}

/// `join`: `None` when the recent commits can't be joined to `full_log`.
fn join(
    full_log: &[LogGraphCommit],
    recent_commits: Vec<LogGraphCommit>,
    previous_refs: &BTreeMap<VcsRoot, Arc<RootRefsModel>>,
    new_refs: &BTreeMap<VcsRoot, Arc<RootRefsModel>>,
) -> Option<Vec<LogGraphCommit>> {
    if full_log.is_empty() {
        return Some(recent_commits);
    }
    let prev_ref_indices: HashSet<VcsLogCommitStorageIndex> = previous_refs
        .values()
        .flat_map(|r| r.get_refs_indexes())
        .collect();
    let new_ref_indices: HashSet<VcsLogCommitStorageIndex> = new_refs
        .values()
        .flat_map(|r| r.get_refs_indexes())
        .collect();
    // NotEnoughData is a valid case (e.g. a long-developed branch was merged); IllegalState
    // "happens from time to time" upstream. Both lead to a full reload.
    joiner::add_commits(
        full_log,
        &prev_ref_indices,
        &recent_commits,
        &new_ref_indices,
    )
    .ok()
    .map(|(log, _)| log)
}

/// Checks that commit ids are unique and that no parent in the list precedes its child.
pub fn check_commits(commits: &[LogGraphCommit]) -> Result<(), String> {
    let max_id = commits.iter().map(|c| *c.id()).max().unwrap_or(0).max(0) as usize;
    let mut positions = vec![usize::MAX; max_id + 1];
    for (i, commit) in commits.iter().enumerate() {
        let id = *commit.id();
        if id < 0 {
            return Err(format!("Invalid commit id {id}"));
        }
        if positions[id as usize] != usize::MAX {
            return Err(format!(
                "Duplicate commit id {id} at rows {} and {i}",
                positions[id as usize]
            ));
        }
        positions[id as usize] = i;
    }
    for (i, commit) in commits.iter().enumerate() {
        for &parent in commit.parents() {
            let p = positions
                .get(parent as usize)
                .copied()
                .unwrap_or(usize::MAX);
            if p != usize::MAX && p < i {
                return Err(format!(
                    "Commits are not in topological order: parent {parent} (row {p}) of {} (row {i})",
                    commit.id()
                ));
            }
        }
    }
    Ok(())
}
