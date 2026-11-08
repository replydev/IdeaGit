//! The visible graph: rows, print elements and actions over a controller's compiled graph.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/VisibleGraphImpl.kt`.
//!
//! Deviations:
//! - `VisibleGraphImpl` implements `ActionController` itself (upstream: an inner class).
//! - The graph updater of an answer is run with [`VisibleGraph::run_graph_updater`], which then
//!   rebuilds the print element generator (upstream wraps the `Runnable`).
//! - `BUTTON_COLLAPSE` / `BUTTON_EXPAND` are reported unsupported until `CollapsedController`
//!   is ported (Plan 2b); upstream supports them on every non-filtered graph.
//! - Arrow actions on a stale print element (one that does not belong to this graph's current
//!   `print_elements`, e.g. a hover racing a log refresh) are ignored and fall through to the
//!   controller's default answer; upstream throws.
//! - `buildSimpleGraphInfo` (`SimpleGraphInfo`) is not ported yet.

use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

use crate::actions::{ActionController, GraphAction, GraphActionType, GraphAnswer, GraphUpdater};
use crate::api::elements::{GraphEdgeType, GraphElement, GraphNodeType};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::{PermanentGraphInfo, VcsLogGraphNodeId};
use crate::api::printer::{GraphColorGetter, GraphPrintElement, PrintElementGenerator};
use crate::facade::linear_graph_controller::{LinearGraphAnswer, LinearGraphController};
use crate::facade::print_element_presentation_manager_impl::PrintElementPresentationManagerImpl;
use crate::print::graph_element_comparator_by_layout_index::GraphElementComparatorByLayoutIndex;
use crate::print::print_element_generator_impl::PrintElementGeneratorImpl;
use crate::print_element::{EdgePrintElement, EdgePrintElementType};
use crate::utils::linear_graph_utils::{get_cursor, get_down_nodes, get_up_nodes};
use crate::visible_graph::{RowInfo, RowType, VcsLogVisibleGraphIndex, VisibleGraph};

pub struct VisibleGraphImpl<CommitId> {
    graph_controller: Box<dyn LinearGraphController>,
    permanent_graph: Arc<dyn PermanentGraphInfo<CommitId>>,
    color_generator: Arc<dyn GraphColorGetter>,
    print_element_generator:
        PrintElementGeneratorImpl<PrintElementPresentationManagerImpl<CommitId>>,
    is_show_long_edges: bool,
}

impl<CommitId: 'static> VisibleGraphImpl<CommitId> {
    pub fn new(
        graph_controller: Box<dyn LinearGraphController>,
        permanent_graph: Arc<dyn PermanentGraphInfo<CommitId>>,
        color_generator: Arc<dyn GraphColorGetter>,
    ) -> VisibleGraphImpl<CommitId> {
        let print_element_generator = Self::create_print_element_generator(
            &*graph_controller,
            &permanent_graph,
            &color_generator,
            false,
        );
        VisibleGraphImpl {
            graph_controller,
            permanent_graph,
            color_generator,
            print_element_generator,
            is_show_long_edges: false,
        }
    }

    fn create_print_element_generator(
        graph_controller: &dyn LinearGraphController,
        permanent_graph: &Arc<dyn PermanentGraphInfo<CommitId>>,
        color_generator: &Arc<dyn GraphColorGetter>,
        is_show_long_edges: bool,
    ) -> PrintElementGeneratorImpl<PrintElementPresentationManagerImpl<CommitId>> {
        let linear_graph = graph_controller.compiled_graph();
        let presentation_manager = PrintElementPresentationManagerImpl::new(
            permanent_graph.clone(),
            linear_graph.clone(),
            color_generator.clone(),
        );
        let layout_graph = linear_graph.clone();
        let layout_info = permanent_graph.clone();
        let comparator = GraphElementComparatorByLayoutIndex::new(move |node_index| {
            let node_id = layout_graph.get_node_id(node_index);
            if node_id < 0 {
                return node_id;
            }
            layout_info
                .permanent_graph_layout()
                .get_layout_index(node_id)
        });
        PrintElementGeneratorImpl::new(
            linear_graph,
            presentation_manager,
            is_show_long_edges,
            move |a: &GraphElement, b: &GraphElement| comparator.ordering(a, b),
        )
    }

    /// `updatePrintElementGenerator`: rebuilds generator and presentation manager (the selection
    /// is reset).
    pub fn update_print_element_generator(&mut self) {
        self.print_element_generator = Self::create_print_element_generator(
            &*self.graph_controller,
            &self.permanent_graph,
            &self.color_generator,
            self.is_show_long_edges,
        );
    }

    /// The controller's compiled graph (`linearGraph`).
    pub fn linear_graph(&self) -> Arc<dyn LinearGraph> {
        self.graph_controller.compiled_graph()
    }

    pub fn permanent_graph(&self) -> &Arc<dyn PermanentGraphInfo<CommitId>> {
        &self.permanent_graph
    }

    fn convert_to_node_id(
        &self,
        node_index: Option<VcsLogVisibleGraphIndex>,
    ) -> Option<VcsLogGraphNodeId> {
        node_index.map(|index| self.graph_controller.compiled_graph().get_node_id(index))
    }

    fn perform_arrow_action(&mut self, action: &GraphAction) -> Option<GraphAnswer<CommitId>> {
        let Some(GraphPrintElement::Edge(edge_print_element)) = &action.affected_element else {
            return None;
        };
        if !edge_print_element.has_arrow() {
            return None;
        }

        let edge = edge_print_element.graph_edge();

        // A stale element (from an older visible graph) may refer to nodes this graph does not
        // have: ignore it (deviation: upstream throws).
        let nodes_count = self.graph_controller.compiled_graph().nodes_count();
        let in_range = |index: Option<VcsLogVisibleGraphIndex>| {
            index.is_none_or(|index| (0..nodes_count).contains(&index))
        };
        if !in_range(edge.up_node_index) || !in_range(edge.down_node_index) {
            return None;
        }

        let mut target_id = None;
        if edge.edge_type == GraphEdgeType::NotLoadCommit {
            debug_assert!(edge_print_element.edge_print_type() == EdgePrintElementType::Down);
            target_id = edge.target_id;
        }
        if edge.edge_type.is_normal_edge() {
            target_id = if edge_print_element.edge_print_type() == EdgePrintElementType::Down {
                self.convert_to_node_id(edge.down_node_index)
            } else {
                self.convert_to_node_id(edge.up_node_index)
            };
        }
        let target_id = target_id?;

        // a negative id must be a known not-loaded id
        let commit = self
            .permanent_graph
            .permanent_commits_info()
            .try_get_commit_id(target_id)?;
        match action.action_type {
            GraphActionType::MouseOver => {
                let selection_changed = self
                    .print_element_generator
                    .presentation_manager_mut()
                    .set_selected_element(GraphPrintElement::Edge(edge_print_element.clone()));
                Some(GraphAnswer {
                    cursor_to_set: Some(get_cursor(true)),
                    commit_to_jump: Some(commit),
                    graph_updater: None,
                    do_jump: false,
                    is_repaint_required: selection_changed,
                })
            }
            GraphActionType::MouseClick => {
                let selection_changed = self
                    .print_element_generator
                    .presentation_manager_mut()
                    .set_selected_elements(HashSet::new());
                Some(GraphAnswer {
                    cursor_to_set: Some(get_cursor(false)),
                    commit_to_jump: Some(commit),
                    graph_updater: None,
                    do_jump: true,
                    is_repaint_required: selection_changed,
                })
            }
            _ => None,
        }
    }

    fn convert_answer(answer: LinearGraphAnswer, selection_changed: bool) -> GraphAnswer<CommitId> {
        GraphAnswer {
            cursor_to_set: answer.cursor_to_set,
            commit_to_jump: None,
            graph_updater: answer.graph_updater,
            do_jump: false,
            is_repaint_required: selection_changed,
        }
    }
}

impl<CommitId: 'static> ActionController<CommitId> for VisibleGraphImpl<CommitId> {
    fn perform_action(&mut self, graph_action: GraphAction) -> GraphAnswer<CommitId> {
        if let Some(graph_answer) = self.perform_arrow_action(&graph_action) {
            return graph_answer;
        }

        let answer = self
            .graph_controller
            .perform_linear_graph_action(&graph_action);
        let selected = answer.selected_node_ids.clone().unwrap_or_default();
        let selection_changed = self
            .print_element_generator
            .presentation_manager_mut()
            .set_selected_elements(selected);
        if answer.graph_changes.is_some() {
            self.update_print_element_generator();
        }
        Self::convert_answer(answer, selection_changed)
    }

    fn are_long_edges_hidden(&self) -> bool {
        !self.is_show_long_edges
    }

    fn set_long_edges_hidden(&mut self, long_edges_hidden: bool) {
        self.is_show_long_edges = !long_edges_hidden;
        self.update_print_element_generator();
    }

    fn is_action_supported(&self, action: &GraphAction) -> bool {
        !matches!(
            action.action_type,
            GraphActionType::ButtonCollapse | GraphActionType::ButtonExpand
        )
    }
}

struct RowInfoImpl<'a, CommitId> {
    visible_graph: &'a VisibleGraphImpl<CommitId>,
    node_id: VcsLogGraphNodeId,
    visible_row: VcsLogVisibleGraphIndex,
}

impl<CommitId: 'static> RowInfo<CommitId> for RowInfoImpl<'_, CommitId> {
    fn commit(&self) -> CommitId {
        self.visible_graph
            .permanent_graph
            .permanent_commits_info()
            .get_commit_id(self.node_id)
    }

    fn one_of_heads(&self) -> CommitId {
        let permanent_graph = &self.visible_graph.permanent_graph;
        let head_node_id = permanent_graph
            .permanent_graph_layout()
            .get_one_of_head_node_index(self.node_id);
        permanent_graph
            .permanent_commits_info()
            .get_commit_id(head_node_id)
    }

    fn print_elements(&self) -> Vec<GraphPrintElement> {
        self.visible_graph
            .print_element_generator
            .get_print_elements(self.visible_row)
    }

    /// # Panics
    /// For a not-loaded node (upstream throws `UnsupportedOperationException`).
    fn row_type(&self) -> RowType {
        let node_type = self
            .visible_graph
            .linear_graph()
            .get_graph_node(self.visible_row)
            .node_type;
        match node_type {
            GraphNodeType::Usual => RowType::Normal,
            GraphNodeType::Unmatched => RowType::Unmatched,
            GraphNodeType::NotLoadCommit => panic!("Unsupported node type: {node_type:?}"),
        }
    }

    fn adjacent_rows(&self, parent: bool) -> Vec<VcsLogVisibleGraphIndex> {
        let graph = self.visible_graph.linear_graph();
        if parent {
            get_down_nodes(&*graph, self.visible_row)
        } else {
            get_up_nodes(&*graph, self.visible_row)
        }
    }
}

impl<CommitId: 'static> VisibleGraph<CommitId> for VisibleGraphImpl<CommitId> {
    fn visible_commit_count(&self) -> i32 {
        self.graph_controller.compiled_graph().nodes_count()
    }

    fn recommended_width(&self) -> i32 {
        self.print_element_generator.recommended_width()
    }

    fn action_controller(&mut self) -> &mut dyn ActionController<CommitId> {
        self
    }

    fn get_row_info(
        &self,
        visible_row: VcsLogVisibleGraphIndex,
    ) -> Box<dyn RowInfo<CommitId> + '_> {
        let count = self.visible_commit_count();
        assert!(
            (0..count).contains(&visible_row),
            "get_row_info: visible row {visible_row} is out of range 0..{count}"
        );
        let node_id = self
            .graph_controller
            .compiled_graph()
            .get_node_id(visible_row);
        debug_assert!(node_id >= 0); // todo remake for all id
        Box::new(RowInfoImpl {
            visible_graph: self,
            node_id,
            visible_row,
        })
    }

    /// Runs the updater from a [`GraphAnswer`], then rebuilds the print element generator.
    fn run_graph_updater(&mut self, updater: GraphUpdater) {
        updater.run();
        self.update_print_element_generator();
    }

    fn get_visible_row_index(&self, commit_id: &CommitId) -> Option<VcsLogVisibleGraphIndex> {
        let node_id = self
            .permanent_graph
            .permanent_commits_info()
            .get_node_id(commit_id);
        self.graph_controller
            .compiled_graph()
            .get_node_index(node_id)
    }
}

/// Like upstream `toString`: `VisibleGraph[commit, commit, ...]`.
impl<CommitId: fmt::Debug + 'static> fmt::Debug for VisibleGraphImpl<CommitId> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let commits: Vec<String> = (0..self.visible_commit_count())
            .map(|i| format!("{:?}", self.get_row_info(i).commit()))
            .collect();
        write!(f, "VisibleGraph[{}]", commits.join(", "))
    }
}
