//! A graph with some nodes of a delegate graph hidden and extra (dotted) edges added: the compiled
//! graph of the filtering and collapsing controllers.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/collapsing/CollapsedGraph.java`.
//!
//! Deviations (Plan 2's mutation model):
//! - The state (visibility, node map, edge storage) lives behind an `Arc<RwLock<_>>` shared by the
//!   [`CollapsedGraph`], its compiled graph and its [`Modification`]s, so a modification can be
//!   applied later by a `Send + 'static` graph updater, in place.
//! - `assertNotUnderModification` is replaced by the lock: [`Modification::apply`] holds the write
//!   lock, so a single guarded read never sees a half-modified state. The guarantee is per guard:
//!   `get_adjacent_edges` reads the delegate edges and the added edges under separate guards, so
//!   an `apply` on another thread could interleave between them (as upstream). Graph updaters run
//!   on the caller's thread, so this does not happen today.
//! - Only one modification may be running at a time, as upstream; a [`Modification`] dropped
//!   without [`apply`](Modification::apply) (e.g. a graph updater that is never run) ends the
//!   modification, where upstream would refuse every later one.
//! - `updateInstance` copies the visibility (upstream shares it with the old graph, which is
//!   dropped).
//! - The compiled graph's edge storage wrapper resolves indexes through the shared state, taking
//!   the read lock per call.
//! - Lock order: locks are always taken in the order outer graph (this state) -> inner (delegate)
//!   graph -> edge storage, never the reverse (e.g. `get_graph_node`, `compiled_node_id`,
//!   `compiled_index_of_id` and `apply` call the delegate graph while holding this guard, and
//!   `apply` takes the edge-storage lock under the state lock), so the nesting cannot deadlock.
//!   Keep that order when changing this code.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::api::EdgeFilter;
use crate::api::elements::{GraphEdge, GraphNode};
use crate::api::linear_graph::LinearGraph;
use crate::api::permanent::VcsLogGraphNodeId;
use crate::collapsing::edge_storage::EdgeStorage;
use crate::collapsing::edge_storage_wrapper::EdgeStorageWrapper;
use crate::collapsing::graph_nodes_visibility::GraphNodesVisibility;
use crate::utils::int_to_int_map::{IntToIntMap, UpdatableIntToIntMap};
use crate::utils::list_int_to_int_map::ListIntToIntMap;
use crate::utils::unsigned_bit_set::UnsignedBitSet;
use crate::visible_graph::VcsLogVisibleGraphIndex;

struct CollapsedState {
    delegated_graph: Arc<dyn LinearGraph>,
    /// Visibility by delegate node index (`myDelegateNodesVisibility` / `myNodesMap`).
    nodes_map: ListIntToIntMap<GraphNodesVisibility>,
}

impl CollapsedState {
    fn is_visible(&self, delegate_node_index: i32) -> bool {
        self.nodes_map.flags().is_visible(delegate_node_index)
    }

    /// `CompiledGraph.compiledNodeIndex`: `-1` for a hidden node.
    fn compiled_node_index(&self, delegate_node_index: Option<i32>) -> Option<i32> {
        let delegate_node_index = delegate_node_index?;
        if self.is_visible(delegate_node_index) {
            Some(self.nodes_map.get_short_index(delegate_node_index))
        } else {
            Some(-1)
        }
    }

    fn compiled_node_id(&self, node_index: i32) -> VcsLogGraphNodeId {
        let delegate_index = self.nodes_map.get_long_index(node_index);
        self.delegated_graph.get_node_id(delegate_index)
    }

    fn compiled_index_of_id(&self, node_id: VcsLogGraphNodeId) -> Option<VcsLogVisibleGraphIndex> {
        let delegate_index = self.delegated_graph.get_node_index(node_id)?;
        if self.is_visible(delegate_index) {
            Some(self.nodes_map.get_short_index(delegate_index))
        } else {
            None
        }
    }
}

type SharedState = Arc<RwLock<CollapsedState>>;

fn read(state: &SharedState) -> RwLockReadGuard<'_, CollapsedState> {
    // A poisoned lock is recovered and used anyway. A panic in the middle of `apply` could leave the
    // visibility and the index map out of sync (upstream would refuse later reads); this is
    // accepted because such a panic is not reachable.
    state.read().unwrap_or_else(PoisonError::into_inner)
}

fn write(state: &SharedState) -> RwLockWriteGuard<'_, CollapsedState> {
    state.write().unwrap_or_else(PoisonError::into_inner)
}

pub struct CollapsedGraph {
    state: SharedState,
    matched_node_id: Arc<UnsignedBitSet>,
    edge_storage: Arc<RwLock<EdgeStorage>>,
    compiled_graph: Arc<CompiledGraph>,
    modification_running: Arc<AtomicBool>,
}

impl CollapsedGraph {
    /// `newInstance`: nodes whose id is set in `matched_node_id` are visible.
    pub fn new_instance(
        delegate_graph: Arc<dyn LinearGraph>,
        matched_node_id: UnsignedBitSet,
    ) -> CollapsedGraph {
        let visible_nodes_id = matched_node_id.clone();
        CollapsedGraph::new(
            delegate_graph,
            Arc::new(matched_node_id),
            visible_nodes_id,
            Arc::new(RwLock::new(EdgeStorage::new())),
        )
    }

    /// `updateInstance`: the same visibility and edges over a new delegate graph.
    pub fn update_instance(
        prev_collapsed_graph: &CollapsedGraph,
        new_delegate_graph: Arc<dyn LinearGraph>,
    ) -> CollapsedGraph {
        let visible_nodes_id = read(&prev_collapsed_graph.state)
            .nodes_map
            .flags()
            .get_node_visibility_by_id()
            .clone();
        CollapsedGraph::new(
            new_delegate_graph,
            prev_collapsed_graph.matched_node_id.clone(),
            visible_nodes_id,
            prev_collapsed_graph.edge_storage.clone(),
        )
    }

    fn new(
        delegated_graph: Arc<dyn LinearGraph>,
        matched_node_id: Arc<UnsignedBitSet>,
        visible_nodes_id: UnsignedBitSet,
        edge_storage: Arc<RwLock<EdgeStorage>>,
    ) -> CollapsedGraph {
        let delegate_nodes_visibility =
            GraphNodesVisibility::new(delegated_graph.clone(), visible_nodes_id);
        let nodes_map = ListIntToIntMap::new_instance(delegate_nodes_visibility);
        let state = Arc::new(RwLock::new(CollapsedState {
            delegated_graph,
            nodes_map,
        }));
        let compiled_graph = Arc::new(CompiledGraph::new(state.clone(), edge_storage.clone()));
        CollapsedGraph {
            state,
            matched_node_id,
            edge_storage,
            compiled_graph,
            modification_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn delegated_graph(&self) -> Arc<dyn LinearGraph> {
        read(&self.state).delegated_graph.clone()
    }

    pub fn is_node_visible(&self, delegate_node_index: i32) -> bool {
        read(&self.state).is_visible(delegate_node_index)
    }

    /// # Panics
    /// If another modification is running (upstream `RuntimeException`).
    pub fn start_modification(&self) -> Modification {
        if self
            .modification_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            panic!("Can not start a new modification while the other one is still running.");
        }
        Modification {
            state: self.state.clone(),
            edge_storage: self.edge_storage.clone(),
            matched_node_id: self.matched_node_id.clone(),
            modification_running: self.modification_running.clone(),
            edges_to_add: EdgeStorageWrapper::create_simple_edge_storage(),
            edges_to_remove: EdgeStorageWrapper::create_simple_edge_storage(),
            nodes_to_hide: HashSet::new(),
            nodes_to_show: HashSet::new(),
            clear_edges: false,
            clear_visibility: false,
            min_affected_node_index: i32::MAX,
            max_affected_node_index: i32::MIN,
        }
    }

    pub fn compiled_graph(&self) -> Arc<dyn LinearGraph> {
        self.compiled_graph.clone()
    }

    /// # Panics
    /// If `compiled_node_index` is out of `0..compiled_graph().nodes_count()`.
    pub fn convert_to_delegate_node_index(&self, compiled_node_index: i32) -> i32 {
        read(&self.state)
            .nodes_map
            .get_long_index(compiled_node_index)
    }

    pub fn matched_node_id(&self) -> &UnsignedBitSet {
        &self.matched_node_id
    }

    /// Whether a normal edge between the two delegate node indexes was added by this graph
    /// (upstream "todo proper name").
    pub fn is_my_collapsed_edge(&self, up_node_index: i32, down_node_index: i32) -> bool {
        EdgeStorageWrapper::for_graph(self.edge_storage.clone(), self.delegated_graph())
            .has_edge(up_node_index, down_node_index)
    }
}

/// A batch of changes to a [`CollapsedGraph`]: collected first, then applied at once. Everywhere
/// here "node indexes" are node indexes in the delegated graph.
pub struct Modification {
    state: SharedState,
    edge_storage: Arc<RwLock<EdgeStorage>>,
    matched_node_id: Arc<UnsignedBitSet>,
    modification_running: Arc<AtomicBool>,
    edges_to_add: EdgeStorageWrapper,
    edges_to_remove: EdgeStorageWrapper,
    nodes_to_hide: HashSet<i32>,
    nodes_to_show: HashSet<i32>,
    clear_edges: bool,
    clear_visibility: bool,
    min_affected_node_index: i32,
    max_affected_node_index: i32,
}

impl Modification {
    fn touch_index(&mut self, node_index: i32) {
        self.min_affected_node_index = self.min_affected_node_index.min(node_index);
        self.max_affected_node_index = self.max_affected_node_index.max(node_index);
    }

    fn touch_all(&mut self) {
        self.min_affected_node_index = 0;
        self.max_affected_node_index = read(&self.state).delegated_graph.nodes_count() - 1;
    }

    fn touch_edge(&mut self, edge: &GraphEdge) {
        if let Some(up) = edge.up_node_index {
            self.touch_index(up);
        }
        if let Some(down) = edge.down_node_index {
            self.touch_index(down);
        }
    }

    pub fn show_node(&mut self, node_index: i32) {
        self.nodes_to_show.insert(node_index);
        self.touch_index(node_index);
    }

    pub fn hide_node(&mut self, node_index: i32) {
        self.nodes_to_hide.insert(node_index);
        self.touch_index(node_index);
    }

    pub fn create_edge(&mut self, edge: &GraphEdge) {
        self.edges_to_add.create_edge(edge);
        self.touch_edge(edge);
    }

    /// Upstream "todo add support for removing edge from delegate graph".
    pub fn remove_edge(&mut self, edge: &GraphEdge) {
        self.edges_to_remove.create_edge(edge);
        self.touch_edge(edge);
    }

    pub fn remove_additional_edges(&mut self) {
        self.clear_edges = true;
        self.touch_all();
    }

    pub fn reset_nodes_visibility(&mut self) {
        self.clear_visibility = true;
        self.touch_all();
    }

    /// Upstream "package private": "I'm not entirely happy about this method".
    pub fn edges_to_add(&self) -> &EdgeStorageWrapper {
        &self.edges_to_add
    }

    pub fn is_node_hidden(&self, node_index: i32) -> bool {
        self.nodes_to_hide.contains(&node_index)
    }

    pub fn is_node_shown(&self, node_index: i32) -> bool {
        self.nodes_to_show.contains(&node_index)
    }

    /// # Panics
    /// If `node_index` is out of the compiled graph's range.
    pub fn convert_to_delegate_node_index(&self, node_index: i32) -> i32 {
        read(&self.state).nodes_map.get_long_index(node_index)
    }

    pub fn apply(self) {
        let mut state = write(&self.state);

        if self.clear_visibility {
            state
                .nodes_map
                .flags_mut()
                .set_node_visibility_by_id((*self.matched_node_id).clone());
        }
        if self.clear_edges {
            self.edge_storage
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .remove_all();
        }

        for &to_show in &self.nodes_to_show {
            state.nodes_map.flags_mut().show(to_show);
        }
        for &to_hide in &self.nodes_to_hide {
            state.nodes_map.flags_mut().hide(to_hide);
        }

        let edge_storage_wrapper =
            EdgeStorageWrapper::for_graph(self.edge_storage.clone(), state.delegated_graph.clone());
        for edge in self.edges_to_add.get_edges() {
            edge_storage_wrapper.create_edge(&edge);
        }
        for edge in self.edges_to_remove.get_edges() {
            edge_storage_wrapper.remove_edge(&edge);
        }

        if self.min_affected_node_index != i32::MAX && self.max_affected_node_index != i32::MIN {
            state
                .nodes_map
                .update(self.min_affected_node_index, self.max_affected_node_index);
        }
        // `Drop` ends the modification
    }
}

impl Drop for Modification {
    fn drop(&mut self) {
        self.modification_running.store(false, Ordering::Release);
    }
}

/// The collapsed graph as a [`LinearGraph`]: visible delegate nodes, their delegate edges between
/// visible nodes, and the added edges.
struct CompiledGraph {
    state: SharedState,
    edge_storage_wrapper: EdgeStorageWrapper,
}

impl CompiledGraph {
    fn new(state: SharedState, edge_storage: Arc<RwLock<EdgeStorage>>) -> CompiledGraph {
        let index_state = state.clone();
        let id_state = state.clone();
        let edge_storage_wrapper = EdgeStorageWrapper::new(
            edge_storage,
            move |node_id| read(&index_state).compiled_index_of_id(node_id),
            move |node_index| read(&id_state).compiled_node_id(node_index),
        );
        CompiledGraph {
            state,
            edge_storage_wrapper,
        }
    }
}

fn is_visible_edge(compiled_up_node: Option<i32>, compiled_down_node: Option<i32>) -> bool {
    compiled_up_node != Some(-1) && compiled_down_node != Some(-1)
}

impl LinearGraph for CompiledGraph {
    fn nodes_count(&self) -> i32 {
        read(&self.state).nodes_map.short_size()
    }

    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        let mut result = Vec::new();
        {
            let state = read(&self.state);
            let delegate_index = state.nodes_map.get_long_index(node_index);

            // add delegate edges
            for delegate_edge in state
                .delegated_graph
                .get_adjacent_edges(delegate_index, filter)
            {
                let compiled_up_index = state.compiled_node_index(delegate_edge.up_node_index);
                let compiled_down_index = state.compiled_node_index(delegate_edge.down_node_index);
                if is_visible_edge(compiled_up_index, compiled_down_index) {
                    result.push(GraphEdge::new(
                        compiled_up_index,
                        compiled_down_index,
                        delegate_edge.target_id,
                        delegate_edge.edge_type,
                    ));
                }
            }
        }

        // the wrapper takes the read lock itself
        result.extend(
            self.edge_storage_wrapper
                .get_adjacent_edges(node_index, filter),
        );

        result
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        let state = read(&self.state);
        let delegate_index = state.nodes_map.get_long_index(node_index);
        let graph_node = state.delegated_graph.get_graph_node(delegate_index);
        GraphNode::with_type(node_index, graph_node.node_type)
    }

    fn get_node_id(&self, node_index: VcsLogVisibleGraphIndex) -> VcsLogGraphNodeId {
        read(&self.state).compiled_node_id(node_index)
    }

    fn get_node_index(&self, node_id: VcsLogGraphNodeId) -> Option<VcsLogVisibleGraphIndex> {
        read(&self.state).compiled_index_of_id(node_id)
    }
}
