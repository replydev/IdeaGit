//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/utils/BfsTests.kt`.

mod common;

use common::test_graph_builder::test_graph;
use ig_log::api::linear_graph::LinearGraph;
use ig_log::utils::bfs_util::BfsWalk;
use ig_log::utils::flags::{BitSetFlags, Flags};
use ig_log::utils::linear_graph_utils::as_lite_linear_graph;

fn set_all(mut flags: BitSetFlags, values: &[i32]) -> BitSetFlags {
    values.iter().for_each(|&it| flags.set(it, true));
    flags
}

#[test]
fn test_visited() {
    let graph = test_graph! {
        0(3)
        1(3)
        2(4)
        3(4)
        4(5, 6)
        5(7)
        6(8)
        7()
        8()
    };
    let nodes_count = graph.nodes_count();
    let mut visited = BitSetFlags::new(nodes_count);
    let lite = as_lite_linear_graph(&graph);

    BfsWalk::new(0, &lite, true).walk(&mut visited, |_| true);
    assert_eq!(
        set_all(BitSetFlags::new(nodes_count), &[0, 3, 4, 5, 6, 7, 8]),
        visited
    );
    visited.set_all(false);

    BfsWalk::new(3, &lite, true).walk(&mut visited, |_| true);
    assert_eq!(
        set_all(BitSetFlags::new(nodes_count), &[3, 4, 5, 6, 7, 8]),
        visited
    );
    visited.set_all(false);

    BfsWalk::new(5, &lite, true).walk(&mut visited, |_| true);
    assert_eq!(set_all(BitSetFlags::new(nodes_count), &[5, 7]), visited);
    visited.set_all(false);

    BfsWalk::new(8, &lite, true).walk(&mut visited, |_| true);
    assert_eq!(set_all(BitSetFlags::new(nodes_count), &[8]), visited);
    visited.set_all(false);
}
