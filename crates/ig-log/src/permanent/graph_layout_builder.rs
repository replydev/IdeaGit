//! Assigns layout indexes: heads are sorted by importance, then a depth-first walk from every head
//! numbers the fragments it reaches first.
//!
//! Ported from `platform/vcs-log/graph/src/com/intellij/vcs/log/graph/impl/permanent/GraphLayoutBuilder.kt`.
//!
//! Deviations: `branches` is an ordered slice (upstream: a `Set`, iterated in hash order before the
//! stable sort, so it only matters for heads the comparator considers equal). The comparator cannot
//! throw, so upstream's `sortCatching` is a plain (Java-compatible) stable sort.

use std::cmp::Ordering;
use std::collections::HashSet;

use crate::api::linear_graph::LinearGraph;
use crate::permanent::graph_layout_impl::GraphLayoutImpl;
use crate::utils::dfs_util::{NODE_NOT_FOUND, walk};
use crate::utils::java_sort;
use crate::utils::linear_graph_utils::{get_down_nodes, get_up_nodes};

/// `build(graph, comparator)`: no extra branch heads.
pub fn build(
    graph: &(impl LinearGraph + ?Sized),
    comparator: impl FnMut(i32, i32) -> Ordering,
) -> GraphLayoutImpl {
    build_with_branches(graph, &[], comparator)
}

/// `build(graph, branches, comparator)`: `branches` (node indexes of branch heads) come first,
/// followed by the graph's own heads; all are sorted with `comparator` (most important first).
pub fn build_with_branches(
    graph: &(impl LinearGraph + ?Sized),
    branches: &[i32],
    mut comparator: impl FnMut(i32, i32) -> Ordering,
) -> GraphLayoutImpl {
    let mut seen = HashSet::new();
    let mut sorted_heads: Vec<i32> = branches
        .iter()
        .copied()
        .chain(get_heads(graph))
        .filter(|head| seen.insert(*head))
        .collect();
    java_sort::sort_by(&mut sorted_heads, |a, b| comparator(*a, *b));

    build_sorted(graph, &sorted_heads)
}

/// Nodes without up (child) edges.
pub(crate) fn get_heads(graph: &(impl LinearGraph + ?Sized)) -> Vec<i32> {
    (0..graph.nodes_count())
        .filter(|&i| get_up_nodes(graph, i).is_empty())
        .collect()
}

fn build_sorted(graph: &(impl LinearGraph + ?Sized), sorted_heads: &[i32]) -> GraphLayoutImpl {
    let mut layout_index = vec![0; graph.nodes_count() as usize];
    let mut important_heads = Vec::new();

    let mut current_layout_index = 1;
    for &head in sorted_heads {
        if layout_index[head as usize] != 0 {
            continue;
        }

        important_heads.push(head);

        walk(head, |current_node| {
            let first_visit = layout_index[current_node as usize] == 0;
            if first_visit {
                layout_index[current_node as usize] = current_layout_index;
            }

            let child_without_layout_index = get_down_nodes(graph, current_node)
                .into_iter()
                .find(|&it| layout_index[it as usize] == 0);
            match child_without_layout_index {
                None => {
                    if first_visit {
                        current_layout_index += 1;
                    }
                    NODE_NOT_FOUND
                }
                Some(child) => child,
            }
        });
    }

    GraphLayoutImpl::new(layout_index, important_heads)
}
