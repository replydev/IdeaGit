//! A stable sort that tolerates inconsistent comparators, like Java's `Collections.sort`.
//!
//! IdeaGit helper, no direct upstream file. Upstream sorts with `java.util.TimSort`
//! (`Collections.sort`, used by the print element generator) and fastutil's `stableSort`
//! (`IntArrayList.sort`, used by the layout builder). Some comparators of the graph (e.g.
//! `GraphElementComparatorByLayoutIndex`) are not total orders, and Rust's `slice::sort_by` may panic
//! on those. For fewer than 32 elements this is an exact port of TimSort's small-array path
//! (`countRunAndMakeAscending` + `binarySort`), so ties and inconsistencies resolve like upstream;
//! longer slices use a stable merge sort.
//!
//! For consistent comparators (strict weak orders) the result is identical to any stable sort,
//! including TimSort and fastutil's `stableSort`. For inconsistent comparators it matches TimSort
//! exactly below 32 elements and uses a plain stable merge sort otherwise; at 32 elements and above
//! it can differ from Java, and Java may even throw "Comparison method violates its general
//! contract".

use std::cmp::Ordering;

/// `TimSort.MIN_MERGE`.
const MIN_MERGE: usize = 32;

/// Sorts `v` stably with `compare`; never panics because of the comparator.
pub fn sort_by<T: Clone>(v: &mut [T], mut compare: impl FnMut(&T, &T) -> Ordering) {
    sort_range(v, &mut compare);
}

fn sort_range<T: Clone>(v: &mut [T], compare: &mut impl FnMut(&T, &T) -> Ordering) {
    let n = v.len();
    if n < 2 {
        return;
    }
    if n < MIN_MERGE {
        let init_run_len = count_run_and_make_ascending(v, compare);
        binary_sort(v, init_run_len, compare);
        return;
    }
    let mid = n / 2;
    sort_range(&mut v[..mid], compare);
    sort_range(&mut v[mid..], compare);
    merge(v, mid, compare);
}

/// `TimSort.binarySort`: `v[..start]` is already sorted.
fn binary_sort<T: Clone>(
    v: &mut [T],
    mut start: usize,
    compare: &mut impl FnMut(&T, &T) -> Ordering,
) {
    if start == 0 {
        start = 1;
    }
    while start < v.len() {
        let pivot = v[start].clone();
        let mut left = 0;
        let mut right = start;
        while left < right {
            let mid = (left + right) / 2;
            if compare(&pivot, &v[mid]) == Ordering::Less {
                right = mid;
            } else {
                left = mid + 1;
            }
        }
        v[left..=start].rotate_right(1);
        start += 1;
    }
}

/// `TimSort.countRunAndMakeAscending`: length of the run at the start; a strictly descending run is
/// reversed in place.
fn count_run_and_make_ascending<T>(
    v: &mut [T],
    compare: &mut impl FnMut(&T, &T) -> Ordering,
) -> usize {
    let hi = v.len();
    let mut run_hi = 1;
    if run_hi == hi {
        return 1;
    }
    let descending = compare(&v[run_hi], &v[0]) == Ordering::Less;
    run_hi += 1;
    if descending {
        while run_hi < hi && compare(&v[run_hi], &v[run_hi - 1]) == Ordering::Less {
            run_hi += 1;
        }
        v[..run_hi].reverse();
    } else {
        while run_hi < hi && compare(&v[run_hi], &v[run_hi - 1]) != Ordering::Less {
            run_hi += 1;
        }
    }
    run_hi
}

/// Stable merge of the sorted halves `v[..mid]` and `v[mid..]`.
fn merge<T: Clone>(v: &mut [T], mid: usize, compare: &mut impl FnMut(&T, &T) -> Ordering) {
    let left: Vec<T> = v[..mid].to_vec();
    let right: Vec<T> = v[mid..].to_vec();
    let (mut i, mut j) = (0, 0);
    for slot in v.iter_mut() {
        let take_right =
            i >= left.len() || (j < right.len() && compare(&right[j], &left[i]) == Ordering::Less);
        if take_right {
            *slot = right[j].clone();
            j += 1;
        } else {
            *slot = left[i].clone();
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_stably_small_and_large() {
        for n in [0usize, 1, 2, 5, 31, 32, 33, 100, 1000] {
            let mut v: Vec<(i32, usize)> = (0..n).map(|i| (((i * 7919) % 13) as i32, i)).collect();
            let mut expected = v.clone();
            expected.sort_by_key(|&(k, _)| k);
            sort_by(&mut v, |a, b| a.0.cmp(&b.0));
            assert_eq!(v, expected, "n = {n}");
        }
    }

    #[test]
    fn descending_run_is_reversed() {
        let mut v = vec![5, 4, 3, 2, 1];
        sort_by(&mut v, |a: &i32, b| a.cmp(b));
        assert_eq!(v, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn inconsistent_comparator_does_not_panic() {
        let mut v: Vec<i32> = (0..200).collect();
        sort_by(&mut v, |a, b| {
            if (a + b) % 3 == 0 {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        });
        assert_eq!(v.len(), 200);
    }
}
