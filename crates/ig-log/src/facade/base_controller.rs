//! The bottom of every controller chain: shows the permanent linear graph as is.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/BaseController.java`.

use std::sync::Arc;

use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::PermanentGraphInfo;
use crate::facade::linear_graph_controller::{
    LinearGraphAction, LinearGraphAnswer, LinearGraphController,
};
use crate::utils::linear_graph_utils::default_graph_answer;

pub struct BaseController<CommitId> {
    permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
}

impl<CommitId> BaseController<CommitId> {
    pub fn new(permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>) -> Self {
        BaseController {
            permanent_graph_info,
        }
    }
}

impl<CommitId> LinearGraphController for BaseController<CommitId> {
    fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.permanent_graph_info.linear_graph()
    }

    /// Always the default answer: default cursor, nothing selected.
    fn perform_linear_graph_action(&mut self, _action: &LinearGraphAction) -> LinearGraphAnswer {
        default_graph_answer()
    }
}
