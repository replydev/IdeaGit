//! The base of the controllers that wrap another controller (filters, collapsing, linear BEK).
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/CascadeController.kt`.
//!
//! Deviations:
//! - The abstract class becomes the [`CascadeController`] trait (`performAction`,
//!   `convertToDelegate`) plus [`LinearGraphController`]'s `delegate*` default methods, which every
//!   cascade controller implements; [`perform_linear_graph_action`] is the template method.
//! - `permanentGraphInfo` is kept only by the controllers that use it.
//! - `convertPrintElement` is [`GraphPrintElement::with_graph_element`].

use crate::actions::GraphAction;
use crate::api::elements::GraphElement;
use crate::api::printer::GraphPrintElement;
use crate::facade::graph_changes::GraphChanges;
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};

pub trait CascadeController: LinearGraphController {
    /// `None` means that this action must be performed by the delegate controller.
    fn perform_action(&mut self, action: &LinearGraphAction) -> Option<LinearGraphAnswer>;

    /// The element of the delegate graph for `graph_element` of this controller's graph; `None` if
    /// there is none.
    fn convert_to_delegate(&self, graph_element: &GraphElement) -> Option<GraphElement> {
        Some(*graph_element)
    }
}

/// `CascadeController.performLinearGraphAction`: this controller's action, or the delegate's
/// (with the affected element converted) followed by
/// [`delegate_graph_changed`](LinearGraphController::delegate_graph_changed).
pub fn perform_linear_graph_action<C: CascadeController + ?Sized>(
    controller: &mut C,
    action: &LinearGraphAction,
) -> LinearGraphAnswer {
    if let Some(answer) = controller.perform_action(action) {
        return answer;
    }
    let delegate_action = GraphAction::new(
        convert_print_element_to_delegate(controller, action.affected_element.as_ref()),
        action.action_type,
    );
    let delegate_answer = match controller.delegate_mut() {
        Some(delegate) => delegate.perform_linear_graph_action(&delegate_action),
        None => unreachable!("a CascadeController has a delegate"),
    };
    controller.delegate_graph_changed(delegate_answer)
}

fn convert_print_element_to_delegate<C: CascadeController + ?Sized>(
    controller: &C,
    element: Option<&GraphPrintElement>,
) -> Option<GraphPrintElement> {
    let element = element?;
    let converted_graph_element = controller.convert_to_delegate(&element.graph_element())?;
    element.with_graph_element(converted_graph_element)
}

/// `performActionRecursively`: runs `action` on `controller`, then on its delegates until one
/// returns changes, which every controller above it then sees through `delegate_graph_changed`.
pub fn perform_action_recursively(
    controller: &mut dyn LinearGraphController,
    action: &mut dyn FnMut(&mut dyn LinearGraphController) -> Option<GraphChanges<i32>>,
) -> Option<GraphChanges<i32>> {
    if let Some(graph_changes) = action(controller) {
        return Some(graph_changes);
    }

    let delegate = controller.delegate_mut()?;
    let result = perform_action_recursively(delegate, action)?;
    controller
        .delegate_graph_changed(LinearGraphAnswer::with_changes(Some(result), None))
        .graph_changes
}
