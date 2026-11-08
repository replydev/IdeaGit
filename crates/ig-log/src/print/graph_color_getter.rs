//! The two color getter factories: by branch head (main branches and their fragments) and by node.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphColorGetterByHead.kt`
//! and `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/print/GraphColorGetterByNode.kt`.

use std::sync::Arc;

use crate::api::permanent::{PermanentGraphInfo, VcsLogGraphNodeId};
use crate::api::printer::{GraphColorGetter, GraphColorGetterFactory};
use crate::graph_color_manager::GraphColorManager;

struct GraphColorGetterByHead<CommitId> {
    permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    color_manager: Arc<dyn GraphColorManager<CommitId>>,
}

impl<CommitId> GraphColorGetter for GraphColorGetterByHead<CommitId> {
    fn get_node_color(&self, node_id: VcsLogGraphNodeId, layout_index: i32) -> i32 {
        let layout = self.permanent_graph_info.permanent_graph_layout();
        let head_node_id = if node_id < 0 {
            0
        } else {
            layout.get_one_of_head_node_index(node_id)
        };
        let head_commit_id = self
            .permanent_graph_info
            .permanent_commits_info()
            .get_commit_id(head_node_id);
        let head_layout_index = layout.get_layout_index(head_node_id);
        self.color_manager
            .get_color(&head_commit_id, head_layout_index, layout_index)
    }
}

/// Colors main branches and graph fragments rather than individual nodes, using a
/// [`GraphColorManager`] keyed by the head commit of the main branch and the fragment's layout index.
pub struct GraphColorGetterByHeadFactory<CommitId> {
    color_manager: Arc<dyn GraphColorManager<CommitId>>,
}

impl<CommitId> GraphColorGetterByHeadFactory<CommitId> {
    pub fn new(color_manager: Arc<dyn GraphColorManager<CommitId>>) -> Self {
        GraphColorGetterByHeadFactory { color_manager }
    }
}

impl<CommitId: 'static> GraphColorGetterFactory<CommitId>
    for GraphColorGetterByHeadFactory<CommitId>
{
    fn create_color_getter(
        &self,
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    ) -> Arc<dyn GraphColorGetter> {
        Arc::new(GraphColorGetterByHead {
            permanent_graph_info,
            color_manager: self.color_manager.clone(),
        })
    }
}

type NodeColorFn<CommitId> = Arc<dyn Fn(&CommitId, i32) -> i32 + Send + Sync>;

struct GraphColorGetterByNode<CommitId> {
    permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    color_getter: NodeColorFn<CommitId>,
}

impl<CommitId> GraphColorGetter for GraphColorGetterByNode<CommitId> {
    fn get_node_color(&self, node_id: VcsLogGraphNodeId, layout_index: i32) -> i32 {
        let commit_id = self
            .permanent_graph_info
            .permanent_commits_info()
            .get_commit_id(node_id);
        (self.color_getter)(&commit_id, layout_index)
    }
}

/// A simple color getter factory defining the color of each node individually, from its commit id
/// and layout index.
pub struct GraphColorGetterByNodeFactory<CommitId> {
    color_getter: NodeColorFn<CommitId>,
}

impl<CommitId> GraphColorGetterByNodeFactory<CommitId> {
    pub fn new(color_getter: impl Fn(&CommitId, i32) -> i32 + Send + Sync + 'static) -> Self {
        GraphColorGetterByNodeFactory {
            color_getter: Arc::new(color_getter),
        }
    }
}

impl<CommitId: 'static> GraphColorGetterFactory<CommitId>
    for GraphColorGetterByNodeFactory<CommitId>
{
    fn create_color_getter(
        &self,
        permanent_graph_info: Arc<dyn PermanentGraphInfo<CommitId>>,
    ) -> Arc<dyn GraphColorGetter> {
        Arc::new(GraphColorGetterByNode {
            permanent_graph_info,
            color_getter: self.color_getter.clone(),
        })
    }
}
