//! IdeaGit tests for `java_sort` and `DfsWalk`. Upstream has no tests for these helpers.

use std::cmp::Ordering;

use ig_log::api::linear_graph::{LiteLinearGraph, NodeFilter};
use ig_log::utils::dfs_util::DfsWalk;
use ig_log::utils::flags::BitSetFlags;
use ig_log::utils::java_sort;

/// Not a strict weak order: odd/even mixture that is not transitive.
fn inconsistent(a: &i32, b: &i32) -> Ordering {
    match (a % 3, b % 3) {
        (x, y) if x == y => Ordering::Equal,
        (0, 1) | (1, 2) | (2, 0) => Ordering::Less,
        _ => Ordering::Greater,
    }
}

fn input(n: usize) -> Vec<i32> {
    // deterministic pseudo-random values
    let mut x: u32 = 12345;
    (0..n)
        .map(|_| {
            x = x.wrapping_mul(1103515245).wrapping_add(12345);
            ((x >> 16) % 1000) as i32
        })
        .collect()
}

#[test]
fn inconsistent_comparator_gives_a_permutation_and_does_not_panic() {
    for n in [10, 31, 32, 100] {
        let original = input(n);
        let mut sorted = original.clone();
        java_sort::sort_by(&mut sorted, inconsistent);
        let mut a = original;
        let mut b = sorted;
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b, "n = {n}");
    }
}

#[test]
fn sort_is_stable_on_descending_run_with_equal_elements() {
    let mut v = vec![(3, 'a'), (3, 'b'), (2, 'c'), (1, 'd')];
    java_sort::sort_by(&mut v, |x, y| x.0.cmp(&y.0));
    assert_eq!(v, vec![(1, 'd'), (2, 'c'), (3, 'a'), (3, 'b')]);

    // Larger input (merge path), many equal keys.
    let mut big: Vec<(i32, usize)> = (0..100).map(|i| (10 - (i as i32 / 12), i)).collect();
    java_sort::sort_by(&mut big, |x, y| x.0.cmp(&y.0));
    for w in big.windows(2) {
        assert!(w[0].0 < w[1].0 || (w[0].0 == w[1].0 && w[0].1 < w[1].1));
    }
}

/// Diamond `0 -> {1, 2} -> 3` (down edges only matter), plus an isolated node 4.
struct Diamond;

impl LiteLinearGraph for Diamond {
    fn nodes_count(&self) -> i32 {
        5
    }

    fn get_nodes(&self, node: i32, filter: NodeFilter) -> Vec<i32> {
        match (node, filter) {
            (0, NodeFilter::Down) => vec![1, 2],
            (1, NodeFilter::Down) | (2, NodeFilter::Down) => vec![3],
            _ => vec![],
        }
    }
}

#[test]
fn dfs_false_on_descendant_ends_only_that_start_nodes_walk() {
    let mut visited = BitSetFlags::new(5);
    let mut seen = Vec::new();
    DfsWalk::new([0, 4], &Diamond, &mut visited).walk(true, |n| {
        seen.push(n);
        n != 1
    });
    // The walk from 0 stops at descendant 1 and start node 4 is still walked. The stale stack still
    // holds 0, so after 4 the leftover walk resumes from 0 and reaches 2 and 3.
    assert_eq!(seen, vec![0, 1, 4, 2, 3]);
}

#[test]
fn dfs_false_on_start_node_ends_whole_walk() {
    let mut visited = BitSetFlags::new(5);
    let mut seen = Vec::new();
    DfsWalk::new([0, 4], &Diamond, &mut visited).walk(true, |n| {
        seen.push(n);
        n != 0
    });
    assert_eq!(seen, vec![0]);
}
