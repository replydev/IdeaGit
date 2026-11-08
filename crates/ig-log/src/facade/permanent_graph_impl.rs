//! The permanent graph: built once from the topologically sorted commit list.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/PermanentGraphImpl.kt`.
//!
//! Deviations:
//! - Only `Options::Base(SortType::Normal)` without branch or text filters is supported; other
//!   options fail with [`UnsupportedGraphOptions`] (Plan 2b ports BEK, linear BEK, first-parent,
//!   `CollapsedController` and `FilteredController`).
//! - The unfiltered `Normal` graph uses [`BaseController`] directly. Upstream wraps it in a
//!   `CollapsedController` with every node visible, whose compiled graph is identical; only the
//!   collapse/expand and fragment hover actions are missing (see `VisibleGraphImpl`).
//! - Branch node ids are passed to the layout builder sorted ascending (upstream: hash-set order,
//!   which only matters for heads that `head_commits_comparator` considers equal).
//! - The `Int`-specialised commit and condition classes are not needed.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;

use crate::api::GraphLayout;
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::{PermanentCommitsInfo, PermanentGraphInfo, VcsLogGraphNodeId};
use crate::api::printer::{GraphColorGetter, GraphColorGetterFactory};
use crate::facade::base_controller::BaseController;
use crate::facade::linear_graph_controller::LinearGraphController;
use crate::facade::reachable_nodes::ReachableNodes;
use crate::facade::visible_graph_impl::VisibleGraphImpl;
use crate::graph_commit::{GraphCommit, GraphCommitImpl};
use crate::permanent::graph_layout_builder;
use crate::permanent::graph_layout_impl::GraphLayoutImpl;
use crate::permanent::permanent_commits_info_impl::PermanentCommitsInfoImpl;
use crate::permanent::permanent_linear_graph_builder::PermanentLinearGraphBuilder;
use crate::permanent::permanent_linear_graph_impl::PermanentLinearGraphImpl;
use crate::permanent_graph::{
    AllCommits, ContainedInBranchCondition, Options, PermanentGraph, SortType,
    UnsupportedGraphOptions,
};
use crate::utils::linear_graph_utils::{
    LiteLinearGraphAdapter, as_lite_linear_graph, get_up_nodes,
};

/// Bounds every commit id type must satisfy.
pub trait CommitIdBounds: Clone + Eq + Hash + Send + Sync + 'static {}
impl<T: Clone + Eq + Hash + Send + Sync + 'static> CommitIdBounds for T {}

/// The parts shared with visible graphs and color getters (upstream: `this` as `PermanentGraphInfo`).
struct PermanentGraphParts<CommitId> {
    permanent_linear_graph: Arc<PermanentLinearGraphImpl>,
    permanent_graph_layout: GraphLayoutImpl,
    permanent_commits_info: PermanentCommitsInfoImpl<CommitId>,
    branch_node_ids: HashSet<VcsLogGraphNodeId>,
}

impl<CommitId: CommitIdBounds> PermanentGraphInfo<CommitId> for PermanentGraphParts<CommitId> {
    fn permanent_commits_info(&self) -> &dyn PermanentCommitsInfo<CommitId> {
        &self.permanent_commits_info
    }

    fn linear_graph(&self) -> Arc<dyn LinearGraph> {
        self.permanent_linear_graph.clone()
    }

    fn permanent_graph_layout(&self) -> &dyn GraphLayout {
        &self.permanent_graph_layout
    }

    fn branch_node_ids(&self) -> &HashSet<VcsLogGraphNodeId> {
        &self.branch_node_ids
    }
}

/// Assigns ids `-2, -3, ...` to commits that are referenced as parents but not loaded.
struct NotLoadedCommitsIdsGenerator<CommitId> {
    not_loaded_commits: HashMap<i32, CommitId>,
}

impl<CommitId: Clone> NotLoadedCommitsIdsGenerator<CommitId> {
    fn apply(&mut self, dom: &CommitId) -> VcsLogGraphNodeId {
        let node_id = -(self.not_loaded_commits.len() as i32 + 2);
        self.not_loaded_commits.insert(node_id, dom.clone());
        node_id
    }
}

pub struct PermanentGraphImpl<CommitId> {
    parts: Arc<PermanentGraphParts<CommitId>>,
    graph_color_getter: Arc<dyn GraphColorGetter>,
    reachable_nodes: ReachableNodes<LiteLinearGraphAdapter<Arc<PermanentLinearGraphImpl>>>,
}

impl<CommitId: CommitIdBounds> PermanentGraphImpl<CommitId> {
    /// Creates the permanent graph.
    ///
    /// - `graph_commits`: topologically sorted commits.
    /// - `head_commits_comparator`: orders branch heads by importance (`Less` = more important).
    ///   More important branches are laid out further to the left, and their color is reused by
    ///   the subgraph below the point where the branches diverged.
    /// - `branches_commit_id`: commit ids of all branch heads.
    pub fn new_instance<G: GraphCommit<CommitId>>(
        graph_commits: &[G],
        color_getter_factory: &dyn GraphColorGetterFactory<CommitId>,
        mut head_commits_comparator: impl FnMut(&CommitId, &CommitId) -> Ordering,
        branches_commit_id: &HashSet<CommitId>,
    ) -> PermanentGraphImpl<CommitId> {
        let mut ids_generator = NotLoadedCommitsIdsGenerator {
            not_loaded_commits: HashMap::new(),
        };
        let linear_graph = PermanentLinearGraphBuilder::new_instance(graph_commits)
            .build_with(|commit| ids_generator.apply(commit));
        let permanent_commits_info =
            PermanentCommitsInfoImpl::new_instance(graph_commits, ids_generator.not_loaded_commits);
        let mut branch_indexes: Vec<i32> = permanent_commits_info
            .convert_to_node_ids_skipping(branches_commit_id, true)
            .into_iter()
            .collect();
        branch_indexes.sort_unstable();
        let permanent_graph_layout = graph_layout_builder::build_with_branches(
            &linear_graph,
            &branch_indexes,
            |node1, node2| {
                let commit_id1 = permanent_commits_info.get_commit_id(node1);
                let commit_id2 = permanent_commits_info.get_commit_id(node2);
                head_commits_comparator(&commit_id1, &commit_id2)
            },
        );

        let branch_node_ids = permanent_commits_info.convert_to_node_ids(branches_commit_id);
        let permanent_linear_graph = Arc::new(linear_graph);
        let parts = Arc::new(PermanentGraphParts {
            permanent_linear_graph: permanent_linear_graph.clone(),
            permanent_graph_layout,
            permanent_commits_info,
            branch_node_ids,
        });
        let graph_color_getter = color_getter_factory.create_color_getter(parts.clone());
        let reachable_nodes = ReachableNodes::new(as_lite_linear_graph(permanent_linear_graph));

        PermanentGraphImpl {
            parts,
            graph_color_getter,
            reachable_nodes,
        }
    }

    /// This graph as the `PermanentGraphInfo` handed to controllers and color getters.
    pub fn permanent_graph_info(&self) -> Arc<dyn PermanentGraphInfo<CommitId>> {
        self.parts.clone()
    }

    pub fn permanent_commits_info_impl(&self) -> &PermanentCommitsInfoImpl<CommitId> {
        &self.parts.permanent_commits_info
    }

    pub fn permanent_linear_graph(&self) -> &PermanentLinearGraphImpl {
        &self.parts.permanent_linear_graph
    }

    pub fn permanent_graph_layout_impl(&self) -> &GraphLayoutImpl {
        &self.parts.permanent_graph_layout
    }

    fn create_filtered_controller(
        &self,
        options: Options,
        visible_heads: Option<&HashSet<CommitId>>,
        matching_commits: Option<&HashSet<CommitId>>,
    ) -> Result<Box<dyn LinearGraphController>, UnsupportedGraphOptions> {
        match options {
            Options::Base(SortType::Normal) => {
                if matching_commits.is_some() {
                    return Err(UnsupportedGraphOptions(
                        "Filtering by matched commits (FilteredController)".to_string(),
                    ));
                }
                if visible_heads.is_some() {
                    return Err(UnsupportedGraphOptions(
                        "Filtering by visible branches (CollapsedController)".to_string(),
                    ));
                }
                Ok(Box::new(BaseController::new(self.permanent_graph_info())))
            }
            Options::Base(SortType::Bek) => Err(UnsupportedGraphOptions(
                "SortType.Bek (SortedBaseController)".to_string(),
            )),
            Options::FirstParent => Err(UnsupportedGraphOptions(
                "Options.FirstParent (FirstParentController)".to_string(),
            )),
            Options::LinearBek => Err(UnsupportedGraphOptions(
                "Options.LinearBek (LinearBekController)".to_string(),
            )),
        }
    }

    /// `createVisibleGraph` with a `preprocessor` that may modify the controller before the visible
    /// graph is built.
    pub fn create_visible_graph_with(
        &self,
        options: Options,
        visible_heads: Option<&HashSet<CommitId>>,
        matching_commits: Option<&HashSet<CommitId>>,
        preprocessor: impl FnOnce(&mut dyn LinearGraphController, &dyn PermanentGraphInfo<CommitId>),
    ) -> Result<VisibleGraphImpl<CommitId>, UnsupportedGraphOptions> {
        let mut controller =
            self.create_filtered_controller(options, visible_heads, matching_commits)?;
        preprocessor(controller.as_mut(), &*self.parts);
        Ok(VisibleGraphImpl::new(
            controller,
            self.permanent_graph_info(),
            self.graph_color_getter.clone(),
        ))
    }
}

impl<CommitId: CommitIdBounds> PermanentGraph<CommitId> for PermanentGraphImpl<CommitId> {
    type Visible = VisibleGraphImpl<CommitId>;

    fn create_visible_graph(
        &self,
        options: Options,
        heads_of_visible_branches: Option<&HashSet<CommitId>>,
        matched_commits: Option<&HashSet<CommitId>>,
    ) -> Result<VisibleGraphImpl<CommitId>, UnsupportedGraphOptions> {
        self.create_visible_graph_with(
            options,
            heads_of_visible_branches,
            matched_commits,
            |_, _| {},
        )
    }

    fn all_commits(&self) -> AllCommits<'_, CommitId> {
        let info = &self.parts.permanent_commits_info;
        let graph = &*self.parts.permanent_linear_graph;
        AllCommits::new(graph.nodes_count() as usize, move |index| {
            let index = index as i32;
            let mut parents = Vec::new();
            graph.for_each_parent_node(index, |parent_id| {
                parents.push(info.get_commit_id(parent_id))
            });
            GraphCommitImpl::create_commit(
                info.get_commit_id(index),
                parents,
                info.get_timestamp(index),
            )
        })
    }

    /// Deviation: upstream throws for commits without a node; the UI must not crash on a stale id.
    fn get_children(&self, commit: &CommitId) -> Vec<CommitId> {
        let info = &self.parts.permanent_commits_info;
        let commit_index = info.get_node_id(commit);
        if commit_index < 0 {
            return Vec::new();
        }
        info.convert_to_commit_id_list(get_up_nodes(
            &*self.parts.permanent_linear_graph,
            commit_index,
        ))
    }

    fn get_containing_branches(&self, commit: &CommitId) -> HashSet<CommitId> {
        let info = &self.parts.permanent_commits_info;
        let node_id = info.get_node_id(commit);
        info.convert_to_commit_id_set(
            self.reachable_nodes
                .get_containing_branches(node_id, &self.parts.branch_node_ids),
        )
    }

    fn get_contained_in_branch_condition(
        &self,
        current_branch_head: &[CommitId],
    ) -> ContainedInBranchCondition<CommitId> {
        let info = &self.parts.permanent_commits_info;
        let head_ids: Vec<i32> = current_branch_head
            .iter()
            .map(|head| info.get_node_id(head))
            .collect();
        let mut branch_nodes = HashSet::new();
        self.reachable_nodes.walk_down(head_ids, |node| {
            branch_nodes.insert(info.get_commit_id(node));
        });
        ContainedInBranchCondition::new(branch_nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::print::graph_color_getter::GraphColorGetterByNodeFactory;

    #[test]
    fn unknown_or_not_loaded_commits_do_not_panic() {
        let factory = GraphColorGetterByNodeFactory::new(|_: &String, layout_index| layout_index);
        let commits = vec![
            GraphCommitImpl::create_commit("c".to_string(), vec!["b".to_string()], 3),
            GraphCommitImpl::create_commit("b".to_string(), vec!["a".to_string()], 2),
            // "a" is only a parent of "b" here, so it is a not-loaded commit
        ];
        let branches: HashSet<String> = ["c".to_string()].into_iter().collect();
        let graph = PermanentGraphImpl::new_instance(
            &commits,
            &factory,
            |a: &String, b: &String| a.cmp(b),
            &branches,
        );

        let unknown = "zzz".to_string();
        assert!(graph.get_children(&unknown).is_empty());
        assert!(graph.get_children(&"a".to_string()).is_empty());
        assert!(graph.get_containing_branches(&unknown).is_empty());
        assert!(graph.get_containing_branches(&"a".to_string()).is_empty());
        let _ = graph.get_contained_in_branch_condition(&[unknown]);
        assert_eq!(graph.get_children(&"b".to_string()), vec!["c".to_string()]);
    }
}
