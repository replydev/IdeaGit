//! A loaded log: references plus the permanent graph of commit indexes ("data pack").
//!
//! Ported from `platform/vcs-log/impl/src/com/intellij/vcs/log/data/VcsLogGraphData.kt`,
//! `VcsLogGraphDataFactory.kt` and `DataPack.kt` (same package).
//!
//! Deviations: no overlay data (`OverlayData`, disabled in tests upstream); an empty or failed pack
//! has no permanent graph (upstream: `EmptyPermanentGraph`).

use std::collections::{BTreeMap, HashSet};
use std::fmt;
use std::sync::Arc;

use crate::data::graph_color_manager_impl::{GraphColorManagerImpl, HeadCommitsComparator};
use crate::data::provider::{VcsException, VcsLogProvider, VcsLogRefManager};
use crate::data::refs_model::{RefsModel, RootRefsModel};
use crate::data::root::VcsRoot;
use crate::data::storage::{VcsLogCommitStorageIndex, VcsLogStorage};
use crate::data::vcs_ref::VcsRefsContainer;
use crate::facade::permanent_graph_impl::PermanentGraphImpl;
use crate::graph_commit::GraphCommitImpl;
use crate::print::graph_color_getter::GraphColorGetterByHeadFactory;

/// The providers of the log, by root.
pub type LogProviders = BTreeMap<VcsRoot, Arc<dyn VcsLogProvider>>;

/// A commit of the log graph: storage indexes for the commit and its parents.
pub type LogGraphCommit = GraphCommitImpl<VcsLogCommitStorageIndex>;

/// The graph data of the log regardless of filters (`VcsLogGraphData`, formerly `DataPack`).
pub struct VcsLogGraphData {
    refs_model: Arc<RefsModel>,
    permanent_graph: Option<Arc<PermanentGraphImpl<VcsLogCommitStorageIndex>>>,
    log_providers: LogProviders,
    is_full: bool,
    error: Option<VcsException>,
}

impl VcsLogGraphData {
    /// `VcsLogGraphData.Empty`.
    pub fn empty(storage: Arc<dyn VcsLogStorage>) -> VcsLogGraphData {
        VcsLogGraphData {
            refs_model: Arc::new(RefsModel::create_empty_instance(storage)),
            permanent_graph: None,
            log_providers: LogProviders::new(),
            is_full: false,
            error: None,
        }
    }

    /// `VcsLogGraphData.Error`.
    pub fn error(storage: Arc<dyn VcsLogStorage>, error: VcsException) -> VcsLogGraphData {
        VcsLogGraphData {
            error: Some(error),
            ..VcsLogGraphData::empty(storage)
        }
    }

    pub fn refs_model(&self) -> &Arc<RefsModel> {
        &self.refs_model
    }

    /// The permanent graph; `None` for an empty log.
    pub fn permanent_graph(&self) -> Option<&Arc<PermanentGraphImpl<VcsLogCommitStorageIndex>>> {
        self.permanent_graph.as_ref()
    }

    pub fn log_providers(&self) -> &LogProviders {
        &self.log_providers
    }

    /// Whether the whole history is loaded (rather than the first block).
    pub fn is_full(&self) -> bool {
        self.is_full
    }

    /// The error of a failed load (`VcsLogGraphData.Error`).
    pub fn load_error(&self) -> Option<&VcsException> {
        self.error.as_ref()
    }

    /// The number of loaded commits.
    pub fn commits_count(&self) -> usize {
        use crate::permanent_graph::PermanentGraph;
        self.permanent_graph
            .as_ref()
            .map_or(0, |graph| graph.all_commits().len())
    }
}

impl fmt::Debug for VcsLogGraphData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(error) = &self.error {
            return write!(f, "{{Error. {error}}}");
        }
        write!(
            f,
            "{{DataPack. {} commits in {} roots}}",
            self.commits_count(),
            self.log_providers.len()
        )
    }
}

/// `VcsLogGraphDataFactory.buildData`.
pub fn build_data(
    commits: &[LogGraphCommit],
    refs: BTreeMap<VcsRoot, Arc<RootRefsModel>>,
    providers: &LogProviders,
    storage: &Arc<dyn VcsLogStorage>,
    full: bool,
) -> VcsLogGraphData {
    let ref_managers: BTreeMap<VcsRoot, Arc<dyn VcsLogRefManager>> = providers
        .iter()
        .map(|(root, provider)| (root.clone(), provider.reference_manager()))
        .collect();
    let refs_model = Arc::new(RefsModel::create(
        refs,
        storage.clone(),
        ref_managers.clone(),
    ));
    let permanent_graph =
        build_permanent_graph(commits, &refs_model, ref_managers, storage.as_ref()).map(Arc::new);
    VcsLogGraphData {
        refs_model,
        permanent_graph,
        log_providers: providers.clone(),
        is_full: full,
        error: None,
    }
}

fn build_permanent_graph(
    commits: &[LogGraphCommit],
    refs_model: &Arc<RefsModel>,
    ref_managers: BTreeMap<VcsRoot, Arc<dyn VcsLogRefManager>>,
    storage: &dyn VcsLogStorage,
) -> Option<PermanentGraphImpl<VcsLogCommitStorageIndex>> {
    if commits.is_empty() {
        return None;
    }
    let head_commits_comparator = HeadCommitsComparator::new(refs_model.clone(), ref_managers);
    let branches: HashSet<VcsLogCommitStorageIndex> = refs_model
        .branches()
        .map(|r| storage.get_commit_index(r.commit_hash(), r.root()))
        .collect();
    let factory = GraphColorGetterByHeadFactory::new(Arc::new(GraphColorManagerImpl::new(
        refs_model.clone(),
    )));
    Some(PermanentGraphImpl::new_instance(
        commits,
        &factory,
        |head1, head2| head_commits_comparator.compare(head1, head2),
        &branches,
    ))
}
