//! The actions of [`CollapsedController`](crate::collapsing::collapsed_controller::CollapsedController):
//! collapse / expand all, collapse or expand a linear fragment on click, and highlight it on hover.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/CollapsedActionManager.java`.
//!
//! Deviations:
//! - The action cases are functions tried in upstream's order.
//! - `expand_nodes` takes an ordered set (upstream: a `HashSet`).

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use crate::actions::GraphActionType;
use crate::actions::GraphUpdater;
use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphEdgeType, GraphElement};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::collapsing::collapsed_graph::CollapsedGraph;
use crate::collapsing::fragment_generator::FragmentGenerator;
use crate::collapsing::linear_fragment_generator::LinearFragmentGenerator;
use crate::facade::graph_changes::GraphChanges;
use crate::facade::linear_graph_controller::{LinearGraphAction, LinearGraphAnswer};
use crate::utils::linear_graph_utils::{
    LiteLinearGraphAdapter, as_lite_linear_graph, convert_ids_to_node_indexes,
    create_selected_answer,
};
use crate::utils::unsigned_bit_set::UnsignedBitSet;

/// `CollapsedActionManager.performAction`: the answer of the first action case that handles
/// `action`, or `None` for the delegate.
pub fn perform_action<CommitId>(
    collapsed_graph: &CollapsedGraph,
    permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
    action: &LinearGraphAction,
) -> Option<LinearGraphAnswer> {
    let context = ActionContext::new(collapsed_graph, permanent_graph_info, action);

    type ActionCase = fn(&ActionContext) -> Option<LinearGraphAnswer>;
    let filter_action_cases: [(ActionCase, &[GraphActionType]); 4] = [
        (collapse_all, &[GraphActionType::ButtonCollapse]),
        (expand_all, &[GraphActionType::ButtonExpand]),
        (
            linear_expand_case,
            &[GraphActionType::MouseClick, GraphActionType::MouseOver],
        ),
        (
            linear_collapse_case,
            &[GraphActionType::MouseClick, GraphActionType::MouseOver],
        ),
    ];
    for (action_case, supported_action_types) in filter_action_cases {
        if supported_action_types.contains(&context.action_type())
            && let Some(graph_answer) = action_case(&context)
        {
            return Some(graph_answer);
        }
    }
    None
}

/// Shows the hidden fragments around `nodes_to_show` (delegate node indexes).
pub fn expand_nodes(collapsed_graph: &CollapsedGraph, nodes_to_show: &BTreeSet<i32>) {
    let delegated_graph = as_lite_linear_graph(collapsed_graph.delegated_graph());
    let generator = FragmentGenerator::new(&delegated_graph, |node_index| {
        collapsed_graph.is_node_visible(node_index)
    });

    let mut modification = collapsed_graph.start_modification();

    for &node_to_show in nodes_to_show {
        if modification.is_node_shown(node_to_show) {
            continue;
        }

        let fragment = generator.get_green_fragment_for_collapse(node_to_show, i32::MAX);
        let (Some(up_red_node), Some(down_red_node)) =
            (fragment.up_red_node(), fragment.down_red_node())
        else {
            continue;
        };
        if up_red_node == down_red_node {
            continue;
        }

        for &n in fragment.middle_green_nodes() {
            modification.show_node(n);
        }

        modification.remove_edge(&GraphEdge::create_normal_edge(
            up_red_node,
            down_red_node,
            GraphEdgeType::Dotted,
        ));
    }

    modification.apply();
}

type Lite = LiteLinearGraphAdapter<Arc<dyn LinearGraph>>;

/// `FragmentGenerators`: the parts the generators of one graph need (they borrow these).
struct FragmentGenerators {
    linear_graph: Arc<dyn LinearGraph>,
    lite: Lite,
    branch_node_indexes: HashSet<i32>,
}

impl FragmentGenerators {
    fn new<CommitId>(
        linear_graph: Arc<dyn LinearGraph>,
        permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
    ) -> FragmentGenerators {
        let branch_node_indexes = convert_ids_to_node_indexes(
            &*linear_graph,
            permanent_graph_info.branch_node_ids().iter().copied(),
        );
        FragmentGenerators {
            lite: as_lite_linear_graph(linear_graph.clone()),
            linear_graph,
            branch_node_indexes,
        }
    }

    fn fragment_generator<'a>(
        &'a self,
        matched_node_id: &'a UnsignedBitSet,
    ) -> FragmentGenerator<'a, Lite, impl Fn(i32) -> bool + 'a> {
        let linear_graph = &self.linear_graph;
        FragmentGenerator::new(&self.lite, move |node_index| {
            matched_node_id.get(linear_graph.get_node_id(node_index))
        })
    }

    fn linear_fragment_generator(&self) -> LinearFragmentGenerator<'_, Lite> {
        LinearFragmentGenerator::new(&self.lite, self.branch_node_indexes.clone())
    }
}

struct ActionContext<'a> {
    collapsed_graph: &'a CollapsedGraph,
    graph_action: &'a LinearGraphAction,
    delegated_fragment_generators: FragmentGenerators,
    compiled_fragment_generators: FragmentGenerators,
}

impl<'a> ActionContext<'a> {
    fn new<CommitId>(
        collapsed_graph: &'a CollapsedGraph,
        permanent_graph_info: &dyn PermanentGraphInfo<CommitId>,
        graph_action: &'a LinearGraphAction,
    ) -> ActionContext<'a> {
        ActionContext {
            collapsed_graph,
            graph_action,
            delegated_fragment_generators: FragmentGenerators::new(
                collapsed_graph.delegated_graph(),
                permanent_graph_info,
            ),
            compiled_fragment_generators: FragmentGenerators::new(
                collapsed_graph.compiled_graph(),
                permanent_graph_info,
            ),
        }
    }

    fn action_type(&self) -> GraphActionType {
        self.graph_action.action_type
    }

    fn affected_graph_element(&self) -> Option<GraphElement> {
        self.graph_action
            .affected_element
            .as_ref()
            .map(|element| element.graph_element())
    }

    fn delegated_graph(&self) -> &Arc<dyn LinearGraph> {
        &self.delegated_fragment_generators.linear_graph
    }

    fn compiled_graph(&self) -> &Arc<dyn LinearGraph> {
        &self.compiled_fragment_generators.linear_graph
    }

    fn convert_to_delegate_node_index(&self, compiled_node_index: i32) -> i32 {
        self.collapsed_graph
            .convert_to_delegate_node_index(compiled_node_index)
    }

    fn convert_to_delegate_node_indexes(
        &self,
        compiled_node_indexes: &HashSet<i32>,
    ) -> HashSet<i32> {
        compiled_node_indexes
            .iter()
            .map(|&node_index| self.convert_to_delegate_node_index(node_index))
            .collect()
    }

    fn convert_to_delegate_edge(&self, compiled_edge: &GraphEdge) -> GraphEdge {
        let up_node_index = compiled_edge
            .up_node_index
            .map(|up| self.convert_to_delegate_node_index(up));
        let down_node_index = compiled_edge
            .down_node_index
            .map(|down| self.convert_to_delegate_node_index(down));

        GraphEdge::new(
            up_node_index,
            down_node_index,
            compiled_edge.target_id,
            compiled_edge.edge_type,
        )
    }
}

/// `LINEAR_COLLAPSE_CASE`: hover highlights the linear fragment, click collapses it.
fn linear_collapse_case(context: &ActionContext) -> Option<LinearGraphAnswer> {
    if is_for_delegate_graph(context) {
        return None;
    }

    let affected_graph_element = context.affected_graph_element()?;

    let compiled_linear_fragment_generator = context
        .compiled_fragment_generators
        .linear_fragment_generator();
    let matched_node_id = context.collapsed_graph.matched_node_id();
    let compiled_fragment_generator = context
        .compiled_fragment_generators
        .fragment_generator(matched_node_id);

    if context.action_type() == GraphActionType::MouseOver {
        let fragment =
            compiled_linear_fragment_generator.get_part_long_fragment(&affected_graph_element)?;
        let middle_compiled_nodes = compiled_fragment_generator.get_middle_nodes(
            fragment.up_node_index,
            fragment.down_node_index,
            false,
        );
        return Some(create_selected_answer(
            &**context.compiled_graph(),
            middle_compiled_nodes,
        ));
    }

    let fragment = compiled_linear_fragment_generator.get_long_fragment(&affected_graph_element)?;

    let middle_compiled_nodes = compiled_fragment_generator.get_middle_nodes(
        fragment.up_node_index,
        fragment.down_node_index,
        true,
    );
    let mut dotted_compiled_edges = HashSet::new();
    for &middle_node_index in &middle_compiled_nodes {
        dotted_compiled_edges.extend(
            context
                .compiled_graph()
                .get_adjacent_edges(middle_node_index, EdgeFilter::NormalAll)
                .into_iter()
                .filter(|edge| edge.edge_type == GraphEdgeType::Dotted),
        );
    }

    let up_node_index = context.convert_to_delegate_node_index(fragment.up_node_index);
    let down_node_index = context.convert_to_delegate_node_index(fragment.down_node_index);
    let middle_nodes = context.convert_to_delegate_node_indexes(&middle_compiled_nodes);
    let dotted_edges: HashSet<GraphEdge> = dotted_compiled_edges
        .iter()
        .map(|edge| context.convert_to_delegate_edge(edge))
        .collect();

    let mut modification = context.collapsed_graph.start_modification();
    for edge in &dotted_edges {
        modification.remove_edge(edge);
    }
    for &middle_node in &middle_nodes {
        modification.hide_node(middle_node);
    }
    modification.create_edge(&GraphEdge::new(
        Some(up_node_index),
        Some(down_node_index),
        None,
        GraphEdgeType::Dotted,
    ));

    modification.apply();
    Some(LinearGraphAnswer::with_changes(
        Some(GraphChanges::some_changes()),
        None,
    ))
}

/// `EXPAND_ALL`: the reset is applied by the answer's graph updater.
fn expand_all(context: &ActionContext) -> Option<LinearGraphAnswer> {
    let mut modification = context.collapsed_graph.start_modification();
    modification.remove_additional_edges();
    modification.reset_nodes_visibility();
    Some(LinearGraphAnswer::with_changes(
        Some(GraphChanges::some_changes()),
        Some(GraphUpdater::new(move || modification.apply())),
    ))
}

/// `COLLAPSE_ALL`: collapses every long fragment; applied by the answer's graph updater.
fn collapse_all(context: &ActionContext) -> Option<LinearGraphAnswer> {
    let mut modification = context.collapsed_graph.start_modification();
    modification.remove_additional_edges();
    modification.reset_nodes_visibility();

    let delegate_graph = context.delegated_graph();
    let linear_fragment_generator = context
        .delegated_fragment_generators
        .linear_fragment_generator();
    let fragment_generator = context
        .delegated_fragment_generators
        .fragment_generator(context.collapsed_graph.matched_node_id());
    for node_index in 0..delegate_graph.nodes_count() {
        if modification.is_node_hidden(node_index) {
            continue;
        }

        if let Some(fragment) = linear_fragment_generator.get_long_down_fragment(node_index) {
            let middle_nodes = fragment_generator.get_middle_nodes(
                fragment.up_node_index,
                fragment.down_node_index,
                true,
            );

            for &node_index_for_hide in &middle_nodes {
                modification.hide_node(node_index_for_hide);
            }
            modification.create_edge(&GraphEdge::new(
                Some(fragment.up_node_index),
                Some(fragment.down_node_index),
                None,
                GraphEdgeType::Dotted,
            ));
        }
    }

    Some(LinearGraphAnswer::with_changes(
        Some(GraphChanges::some_changes()),
        Some(GraphUpdater::new(move || modification.apply())),
    ))
}

/// `LINEAR_EXPAND_CASE`: hover highlights the ends of a collapsed fragment, click expands it.
fn linear_expand_case(context: &ActionContext) -> Option<LinearGraphAnswer> {
    if is_for_delegate_graph(context) {
        return None;
    }

    let dotted_edge = get_dotted_edge(
        context.affected_graph_element().as_ref(),
        &**context.compiled_graph(),
    )?;

    let up_node_index =
        context.convert_to_delegate_node_index(assert_int(dotted_edge.up_node_index));
    let down_node_index =
        context.convert_to_delegate_node_index(assert_int(dotted_edge.down_node_index));

    if context.action_type() == GraphActionType::MouseOver {
        return Some(create_selected_answer(
            &**context.delegated_graph(),
            [up_node_index, down_node_index],
        ));
    }

    let middle_nodes = context
        .delegated_fragment_generators
        .fragment_generator(context.collapsed_graph.matched_node_id())
        .get_middle_nodes(up_node_index, down_node_index, true);

    let mut modification = context.collapsed_graph.start_modification();
    for &middle_node in &middle_nodes {
        modification.show_node(middle_node);
    }
    modification.remove_edge(&GraphEdge::new(
        Some(up_node_index),
        Some(down_node_index),
        None,
        GraphEdgeType::Dotted,
    ));

    modification.apply();
    Some(LinearGraphAnswer::with_changes(
        Some(GraphChanges::some_changes()),
        None,
    ))
}

/// A dotted edge the collapsed graph did not add (e.g. a filter's), which the delegate handles.
fn is_for_delegate_graph(context: &ActionContext) -> bool {
    let Some(affected_graph_element) = context.affected_graph_element() else {
        return false;
    };

    if let Some(dotted_edge) =
        get_dotted_edge(Some(&affected_graph_element), &**context.compiled_graph())
    {
        let up_node_index =
            context.convert_to_delegate_node_index(assert_int(dotted_edge.up_node_index));
        let down_node_index =
            context.convert_to_delegate_node_index(assert_int(dotted_edge.down_node_index));

        if !context
            .collapsed_graph
            .is_my_collapsed_edge(up_node_index, down_node_index)
        {
            return true;
        }
    }
    false
}

/// A dotted edge always has both ends.
fn assert_int(value: Option<i32>) -> i32 {
    value.expect("dotted edge without a node index")
}

/// The element if it is a dotted edge, or the first dotted edge of a node.
fn get_dotted_edge(
    graph_element: Option<&GraphElement>,
    graph: &dyn LinearGraph,
) -> Option<GraphEdge> {
    match graph_element? {
        GraphElement::Edge(edge) if edge.edge_type == GraphEdgeType::Dotted => Some(*edge),
        GraphElement::Edge(_) => None,
        GraphElement::Node(node) => graph
            .get_adjacent_edges(node.node_index, EdgeFilter::NormalAll)
            .into_iter()
            .find(|edge| edge.edge_type == GraphEdgeType::Dotted),
    }
}
