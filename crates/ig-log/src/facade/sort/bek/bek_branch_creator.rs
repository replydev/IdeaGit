//! Splits the permanent graph into BEK branches, one per head, walking down each head's layout.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/facade/sort/bek/BekBranchCreator.java`.

use crate::api::GraphLayout;
use crate::api::linear_graph::LinearGraph;
use crate::facade::sort::bek::bek_branch::BekBranch;
use crate::facade::sort::bek::bek_edge_restrictions::BekEdgeRestrictions;
use crate::permanent::graph_layout_impl::GraphLayoutImpl;
use crate::utils::dfs_util::{NODE_NOT_FOUND, walk};
use crate::utils::flags::{BitSetFlags, Flags};
use crate::utils::linear_graph_utils::{get_down_nodes, get_up_nodes};

pub struct BekBranchCreator<'a, G: LinearGraph + ?Sized> {
    permanent_graph: &'a G,
    graph_layout: &'a GraphLayoutImpl,
    done_nodes: BitSetFlags,
    edge_restrictions: BekEdgeRestrictions,
}

impl<'a, G: LinearGraph + ?Sized> BekBranchCreator<'a, G> {
    pub fn new(permanent_graph: &'a G, graph_layout: &'a GraphLayoutImpl) -> Self {
        BekBranchCreator {
            permanent_graph,
            graph_layout,
            done_nodes: BitSetFlags::with_default(permanent_graph.nodes_count(), false),
            edge_restrictions: BekEdgeRestrictions::default(),
        }
    }

    pub fn get_result(mut self) -> (Vec<BekBranch>, BekEdgeRestrictions) {
        let mut bek_branches = Vec::new();

        for &head_node in self.graph_layout.get_head_node_index() {
            if self.done_nodes.get(head_node) {
                continue;
            }
            let next_branch = self.create_next_branch(head_node);
            bek_branches.push(BekBranch::new(next_branch));
        }
        (bek_branches, self.edge_restrictions)
    }

    pub fn create_next_branch(&mut self, head_node: i32) -> Vec<i32> {
        self.done_nodes.set(head_node, true);
        let mut node_indexes = vec![head_node];

        let graph = self.permanent_graph;
        let layout = self.graph_layout;
        let done_nodes = &mut self.done_nodes;
        let edge_restrictions = &mut self.edge_restrictions;
        let start_layout = layout.get_layout_index(head_node);

        walk(head_node, |current_node| {
            let current_layout = layout.get_layout_index(current_node);
            let down_nodes = get_down_nodes(graph, current_node);
            for &down_node in down_nodes.iter().rev() {
                if done_nodes.get(down_node) {
                    if layout.get_layout_index(down_node) < start_layout {
                        edge_restrictions.add_restriction(current_node, down_node);
                    }
                } else if current_layout <= layout.get_layout_index(down_node) {
                    // almost ok node, except (may be) up nodes
                    let mut has_undone_up_nodes = false;
                    for up_node in get_up_nodes(graph, down_node) {
                        if !done_nodes.get(up_node)
                            && layout.get_layout_index(up_node)
                                <= layout.get_layout_index(down_node)
                        {
                            has_undone_up_nodes = true;
                            break;
                        }
                    }

                    if !has_undone_up_nodes {
                        done_nodes.set(down_node, true);
                        node_indexes.push(down_node);
                        return down_node;
                    }
                }
            }
            NODE_NOT_FOUND
        });

        node_indexes
    }
}
