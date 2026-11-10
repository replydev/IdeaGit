//! The linear BEK controller: collapses all merges on creation; a click on a merge's commits
//! collapses it again, a click on a dotted edge expands it, and hovering highlights them.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/linearBek/LinearBekController.kt`.
//!
//! Deviations:
//! - The graph builder is created per use (upstream keeps one that references the compiled graph).
//! - `collectChildFragments`' work list is ordered by insertion with tails and body added in
//!   ascending order (upstream: a `LinkedHashSet` fed from a fastutil hash set). The fragments are
//!   a list (upstream: a set of objects without `equals`, i.e. also a list).
//! - The creation time is not logged.
//!
//! Upstream quirk kept: `iterateMerges`' search limit never stops it (see [`BfsSearch`]), so it
//! walks up to the first merge that answers.

use std::collections::{BTreeSet, HashSet, VecDeque};
use std::sync::Arc;

use crate::actions::{GraphActionType, GraphUpdater};
use crate::api::EdgeFilter;
use crate::api::GraphLayout;
use crate::api::elements::{GraphEdge, GraphEdgeType, GraphElement, GraphNode};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::facade::cascade_controller::{self, CascadeController};
use crate::facade::graph_changes::{GraphChanges, edges_replaced};
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};
use crate::facade::sort::sort_index_map::SortIndexMap;
use crate::facade::sort::sorted_base_controller::SortedBaseController;
use crate::linear_bek::linear_bek_graph::{LinearBekGraph, WorkingLinearBekGraph};
use crate::linear_bek::linear_bek_graph_builder::{
    LinearBekGraphBuilder, MAX_BLOCK_SIZE, MergeFragment,
};
use crate::utils::bfs_util::{BfsSearch, BfsWalk, ReversePriorityQueue};
use crate::utils::int_hash_set_flags::IntHashSetFlags;
use crate::utils::linear_graph_utils::{
    as_lite_linear_graph, create_selected_answer, get_down_nodes,
};

const MAX_COLLAPSED_MERGES: usize = 10;

pub struct LinearBekController<CommitId> {
    delegate_controller: SortedBaseController,
    compiled_graph: Arc<LinearBekGraph>,
    bek_graph_layout: BekGraphLayout<CommitId>,
    merge_commits: HashSet<i32>,
}

impl<CommitId: 'static> LinearBekController<CommitId> {
    pub fn new(
        controller: SortedBaseController,
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    ) -> LinearBekController<CommitId> {
        let compiled_graph = Arc::new(LinearBekGraph::new(controller.compiled_graph()));
        let bek_graph_layout =
            BekGraphLayout::new(permanent_graph_info, controller.bek_int_map().clone());
        let merge_commits =
            LinearBekGraphBuilder::new(&compiled_graph, &bek_graph_layout).collapse_all();
        LinearBekController {
            delegate_controller: controller,
            compiled_graph,
            bek_graph_layout,
            merge_commits,
        }
    }

    fn delegate_graph(&self) -> Arc<dyn LinearGraph> {
        self.delegate_controller.compiled_graph()
    }

    fn linear_bek_graph_builder(&self) -> LinearBekGraphBuilder<'_> {
        LinearBekGraphBuilder::new(&self.compiled_graph, &self.bek_graph_layout)
    }

    fn get_all_adjacent_dotted_edges(&self, graph_element: &GraphNode) -> Vec<GraphEdge> {
        self.compiled_graph
            .get_adjacent_edges(graph_element.node_index, EdgeFilter::All)
            .into_iter()
            .filter(|graph_edge| graph_edge.edge_type == GraphEdgeType::Dotted)
            .collect()
    }

    fn expand_all(&self) -> LinearGraphAnswer {
        let compiled_graph = self.compiled_graph.clone();
        LinearGraphAnswer::with_changes(
            Some(GraphChanges::some_changes()),
            Some(GraphUpdater::new(move || {
                compiled_graph.dotted_edges().remove_all();
                compiled_graph.hidden_edges().remove_all();
            })),
        )
    }

    fn collapse_all(&self) -> LinearGraphAnswer {
        let working_graph = WorkingLinearBekGraph::new(self.compiled_graph.clone());
        LinearBekGraphBuilder::new(&working_graph.working, &self.bek_graph_layout).collapse_all();
        let removed: Vec<GraphEdge> = working_graph.get_removed_edges().into_iter().collect();
        let added: Vec<GraphEdge> = working_graph.get_added_edges().into_iter().collect();
        LinearGraphAnswer::with_changes(
            Some(edges_replaced(&removed, &added, &*self.delegate_graph())),
            Some(GraphUpdater::new(move || working_graph.apply_changes())),
        )
    }

    fn iterate_merges<T>(
        &self,
        start: i32,
        mut function: impl FnMut(i32) -> Option<T>,
    ) -> Option<T> {
        let delegate_graph = self.delegate_graph();
        let lite = as_lite_linear_graph(delegate_graph.clone());
        let walk = BfsWalk::with_queue(start, &lite, false, ReversePriorityQueue::new(start));
        let mut search = BfsSearch::new(walk, MAX_BLOCK_SIZE);
        let mut visited = IntHashSetFlags::new(delegate_graph.nodes_count());
        search.find(&mut visited, |it| {
            if self.merge_commits.contains(&it) {
                function(it)
            } else {
                None
            }
        })
    }

    fn is_collapsed(&self, merge: i32) -> bool {
        get_down_nodes(&*self.compiled_graph, merge).len() == 1
    }

    fn highlight_node(&self, node: &GraphNode) -> Option<LinearGraphAnswer> {
        self.iterate_merges(node.node_index, |merge| {
            if !self.is_collapsed(merge) {
                let fragments = self.collect_child_fragments(merge);
                if fragments.is_empty() {
                    return None;
                }

                let nodes_to_highlight: HashSet<i32> =
                    fragments.iter().flat_map(|it| it.get_all_nodes()).collect();
                if nodes_to_highlight.contains(&node.node_index) {
                    return Some(create_selected_answer(
                        &*self.compiled_graph,
                        nodes_to_highlight,
                    ));
                }
            }
            None
        })
    }

    fn collapse_node(&self, node: &GraphNode) -> Option<LinearGraphAnswer> {
        self.iterate_merges(node.node_index, |merge| {
            if !self.is_collapsed(merge) {
                let fragments = self.collect_child_fragments(merge);
                if fragments.is_empty() {
                    return None;
                }

                let nodes_to_collapse: BTreeSet<i32> = fragments
                    .iter()
                    .flat_map(|f| {
                        let mut nodes = f.get_tails_and_body();
                        nodes.insert(f.get_parent());
                        nodes
                    })
                    .collect();
                if !nodes_to_collapse.contains(&node.node_index)
                    && !fragments
                        .iter()
                        .any(|it| it.get_left_child() == node.node_index)
                {
                    return None;
                }
                // reverse order (upstream: TreeSet(Comparator.reverseOrder()))
                for &i in nodes_to_collapse.iter().rev() {
                    self.linear_bek_graph_builder().collapse_fragment(i);
                }
                return Some(LinearGraphAnswer::with_changes(
                    Some(GraphChanges::some_changes()),
                    None,
                ));
            }
            None
        })
    }

    fn collect_child_fragments(&self, merge: i32) -> Vec<MergeFragment> {
        let builder = self.linear_bek_graph_builder();
        let mut result = Vec::new();
        // a LinkedHashSet
        let mut to_process = VecDeque::from([merge]);
        let mut in_to_process = HashSet::from([merge]);
        while let Some(next_node) = to_process.pop_front() {
            in_to_process.remove(&next_node);
            let Some(fragment) = builder.get_fragment(next_node) else {
                continue;
            };
            let tails_and_body: BTreeSet<i32> = fragment.get_tails_and_body().into_iter().collect();
            for node in tails_and_body {
                if in_to_process.insert(node) {
                    to_process.push_back(node);
                }
            }
            result.push(fragment);
            if result.len() > MAX_COLLAPSED_MERGES {
                break;
            }
        }
        result
    }

    fn highlight_edge(&self, edge: &GraphEdge) -> Option<LinearGraphAnswer> {
        if edge.edge_type != GraphEdgeType::Dotted {
            return None;
        }
        Some(create_selected_answer(
            &*self.compiled_graph,
            edge.up_node_index.into_iter().chain(edge.down_node_index),
        ))
    }

    fn expand_edge(&self, edge: &GraphEdge) -> Option<LinearGraphAnswer> {
        if edge.edge_type != GraphEdgeType::Dotted {
            return None;
        }
        let expanded: Vec<GraphEdge> = self.compiled_graph.expand_edge(edge).into_iter().collect();
        Some(LinearGraphAnswer::with_changes(
            Some(edges_replaced(&[*edge], &expanded, &*self.delegate_graph())),
            None,
        ))
    }
}

impl<CommitId: 'static> LinearGraphController for LinearBekController<CommitId> {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.compiled_graph.clone()
    }

    fn perform_linear_graph_action(&mut self, action: &LinearGraphAction) -> LinearGraphAnswer {
        cascade_controller::perform_linear_graph_action(self, action)
    }

    fn delegate(&self) -> Option<&dyn LinearGraphController> {
        Some(&self.delegate_controller)
    }

    fn delegate_mut(&mut self) -> Option<&mut dyn LinearGraphController> {
        Some(&mut self.delegate_controller)
    }
}

impl<CommitId: 'static> CascadeController for LinearBekController<CommitId> {
    fn perform_action(&mut self, action: &LinearGraphAction) -> Option<LinearGraphAnswer> {
        if action.action_type == GraphActionType::ButtonCollapse {
            return Some(self.collapse_all());
        }
        if action.action_type == GraphActionType::ButtonExpand {
            return Some(self.expand_all());
        }
        let affected_element = action.affected_element.as_ref()?;

        if action.action_type == GraphActionType::MouseClick {
            match affected_element.graph_element() {
                GraphElement::Node(graph_element) => {
                    if let Some(answer) = self.collapse_node(&graph_element) {
                        return Some(answer);
                    }
                    for dotted_edge in self.get_all_adjacent_dotted_edges(&graph_element) {
                        if let Some(expanded_answer) = self.expand_edge(&dotted_edge) {
                            return Some(expanded_answer);
                        }
                    }
                }
                GraphElement::Edge(graph_element) => return self.expand_edge(&graph_element),
            }
        } else if action.action_type == GraphActionType::MouseOver {
            match affected_element.graph_element() {
                GraphElement::Node(graph_element) => {
                    if let Some(answer) = self.highlight_node(&graph_element) {
                        return Some(answer);
                    }
                    for dotted_edge in self.get_all_adjacent_dotted_edges(&graph_element) {
                        if let Some(highlight_answer) = self.highlight_edge(&dotted_edge) {
                            return Some(highlight_answer);
                        }
                    }
                }
                GraphElement::Edge(graph_element) => return self.highlight_edge(&graph_element),
            }
        }
        None
    }
}

/// The permanent layout seen through the BEK order.
struct BekGraphLayout<CommitId> {
    permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    sort_index_map: Arc<SortIndexMap>,
    /// Computed once: `get_head_node_index` returns a slice.
    head_node_index: Vec<i32>,
}

impl<CommitId> BekGraphLayout<CommitId> {
    fn new(
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
        sort_index_map: Arc<SortIndexMap>,
    ) -> BekGraphLayout<CommitId> {
        let head_node_index = permanent_graph_info
            .permanent_graph_layout()
            .get_head_node_index()
            .iter()
            .map(|&it| sort_index_map.get_sorted_index(it))
            .collect();
        BekGraphLayout {
            permanent_graph_info,
            sort_index_map,
            head_node_index,
        }
    }
}

impl<CommitId> GraphLayout for BekGraphLayout<CommitId> {
    fn get_layout_index(&self, node_index: i32) -> i32 {
        self.permanent_graph_info
            .permanent_graph_layout()
            .get_layout_index(self.sort_index_map.get_usual_index(node_index))
    }

    fn get_one_of_head_node_index(&self, node_index: i32) -> i32 {
        let usual_index = self
            .permanent_graph_info
            .permanent_graph_layout()
            .get_one_of_head_node_index(self.sort_index_map.get_usual_index(node_index));
        self.sort_index_map.get_sorted_index(usual_index)
    }

    fn get_head_node_index(&self) -> &[i32] {
        &self.head_node_index
    }
}
