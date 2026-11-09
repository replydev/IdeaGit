//! Row metadata (subject, author, date) for the Log table, loaded lazily for the visible rows.
//!
//! Port of the UI-thread side of `platform/vcs-log/impl/src/com/intellij/vcs/log/data/MiniDetailsGetter.kt`
//! with `TopCommitsCache.java`:
//! - The cache is first filled with the recent commits read with each first block (upstream
//!   `TopCommitsCache` fed by `storeRecentDetails`).
//! - Rows not cached get a "loading" placeholder tagged with the number of the load task that
//!   asked for them, so a scroll doesn't re-request rows already being loaded.
//! - Placeholders of tasks [`MAX_LOADING_TASKS`] or more behind the current one are dropped (the
//!   background loader is a LIFO queue keeping the newest tasks; older ones are discarded).
//! - A failed load keeps its placeholders (upstream doesn't remove them either): they age out
//!   like any other, so a failing commit is not requested again on every frame.
//! - The cache is bounded ([`CACHE_SIZE`], upstream `maximumSize(10000)`); entries are evicted
//!   least recently used first (every insertion and hit is its own clock tick).
//!
//! Deviation: one cache for both upstream maps (top commits are not kept separately).

use std::collections::HashMap;
use std::sync::Arc;

use ig_log::GraphCommit;
use ig_log::data::{Hash, VcsCommitMetadata, VcsLogCommitStorageIndex};

/// `MiniDetailsGetter.MAX_LOADING_TASKS`.
pub const MAX_LOADING_TASKS: u64 = 10;
/// `MiniDetailsGetter`'s Caffeine `maximumSize`.
pub const CACHE_SIZE: usize = 10_000;
/// Rows loaded per `read_metadata` call (`VcsLogData`/`GitLogProvider` batch size).
pub const BATCH_SIZE: usize = 100;

/// What a Log row shows besides the graph and labels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowMeta {
    pub hash: Hash,
    pub subject: String,
    pub author: String,
    /// Epoch milliseconds.
    pub author_time: i64,
}

impl RowMeta {
    pub fn from_metadata(metadata: &VcsCommitMetadata) -> RowMeta {
        RowMeta {
            hash: metadata.id().clone(),
            // `GraphCommitCellRenderer.appendText`: tabs become spaces, trimmed
            subject: metadata.subject().replace('\t', " ").trim().to_string(),
            // `CommitPresentationUtil.getAuthorPresentation`: the author name
            author: metadata.author().name().to_string(),
            author_time: metadata.author_time(),
        }
    }
}

#[derive(Clone, Debug)]
enum Entry {
    Loaded(Arc<RowMeta>),
    Loading { task: u64 },
}

/// The cache with loading placeholders.
#[derive(Default)]
pub struct MetadataCache {
    entries: HashMap<VcsLogCommitStorageIndex, (Entry, u64)>,
    current_task: u64,
    clock: u64,
}

/// A cache lookup.
#[derive(Clone, Debug, PartialEq)]
pub enum Lookup {
    Loaded(Arc<RowMeta>),
    /// Requested, not loaded yet ("Loading…").
    Loading,
    Missing,
}

impl MetadataCache {
    pub fn new() -> MetadataCache {
        MetadataCache::default()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `getFromCacheAndCleanOldPlaceholder` (without loading).
    pub fn get(&mut self, commit: VcsLogCommitStorageIndex) -> Lookup {
        self.clock += 1;
        let clock = self.clock;
        let current = self.current_task;
        match self.entries.get_mut(&commit) {
            Some((Entry::Loaded(meta), used)) => {
                *used = clock;
                Lookup::Loaded(meta.clone())
            }
            Some((Entry::Loading { task }, _)) => {
                if *task + MAX_LOADING_TASKS <= current {
                    self.entries.remove(&commit);
                    Lookup::Missing
                } else {
                    Lookup::Loading
                }
            }
            None => Lookup::Missing,
        }
    }

    /// `getCommitData(commit, commitsToLoad)`: marks the missing ones of `commits` as loading under
    /// a new task and returns them (empty: nothing to request).
    pub fn request(
        &mut self,
        commits: impl IntoIterator<Item = VcsLogCommitStorageIndex>,
    ) -> Vec<VcsLogCommitStorageIndex> {
        let missing: Vec<_> = commits
            .into_iter()
            .filter(|&c| self.get(c) == Lookup::Missing)
            .collect();
        if missing.is_empty() {
            return missing;
        }
        let task = self.current_task;
        self.current_task += 1;
        for &commit in &missing {
            self.clock += 1;
            self.entries
                .insert(commit, (Entry::Loading { task }, self.clock));
        }
        self.evict();
        missing
    }

    /// Stores loaded metadata (`saveInCache`), also for the recent details of a first block.
    pub fn put(&mut self, loaded: impl IntoIterator<Item = (VcsLogCommitStorageIndex, RowMeta)>) {
        for (commit, meta) in loaded {
            self.clock += 1;
            self.entries
                .insert(commit, (Entry::Loaded(Arc::new(meta)), self.clock));
        }
        self.evict();
    }

    /// The result of a load task: stores `loaded`; the placeholders of `failed` commits stay
    /// until they age out (upstream keeps them too), so they aren't re-requested every frame.
    pub fn complete(
        &mut self,
        loaded: impl IntoIterator<Item = (VcsLogCommitStorageIndex, RowMeta)>,
        failed: &[VcsLogCommitStorageIndex],
    ) {
        let _ = failed;
        self.put(loaded);
    }

    /// Drops every loading placeholder (the loads they wait for were abandoned, e.g. by a worker
    /// restart), so the rows are requested again.
    pub fn drop_loading(&mut self) {
        self.entries
            .retain(|_, (entry, _)| matches!(entry, Entry::Loaded(_)));
    }

    fn evict(&mut self) {
        if self.entries.len() <= CACHE_SIZE {
            return;
        }
        let mut used: Vec<u64> = self.entries.values().map(|(_, u)| *u).collect();
        let excess = self.entries.len() - CACHE_SIZE;
        let (_, cutoff, _) = used.select_nth_unstable(excess - 1);
        let cutoff = *cutoff;
        self.entries.retain(|_, (_, u)| *u > cutoff);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(subject: &str) -> RowMeta {
        RowMeta {
            hash: Hash::build("0123456789abcdef0123456789abcdef01234567"),
            subject: subject.to_string(),
            author: "A".to_string(),
            author_time: 0,
        }
    }

    #[test]
    fn requested_rows_are_loading_until_put() {
        let mut cache = MetadataCache::new();
        assert_eq!(cache.request([1, 2, 3]), [1, 2, 3]);
        assert_eq!(cache.get(2), Lookup::Loading);
        // already loading: no new request
        assert!(cache.request([1, 2, 3]).is_empty());
        cache.complete([(2, meta("two"))], &[1]);
        assert_eq!(cache.get(2), Lookup::Loaded(Arc::new(meta("two"))));
        assert_eq!(cache.get(1), Lookup::Loading);
    }

    #[test]
    fn failed_rows_are_not_requested_again_until_their_placeholder_ages_out() {
        let mut cache = MetadataCache::new();
        assert_eq!(cache.request([1, 2]), [1, 2]);
        cache.complete([], &[1, 2]);
        // every frame asks for the same rows: no new request
        for _ in 0..100 {
            assert!(cache.request([1, 2]).is_empty());
        }
        // only other load tasks (scrolling) let the placeholders expire
        for i in 0..MAX_LOADING_TASKS as i32 {
            cache.request([10 + i]);
        }
        assert_eq!(cache.request([1, 2]), [1, 2]);
    }

    #[test]
    fn eviction_drops_exactly_the_excess_and_also_runs_on_request() {
        let mut cache = MetadataCache::new();
        // one batch of more than CACHE_SIZE: per-entry ticks, so ties don't drop the batch
        cache.put((0..CACHE_SIZE as i32 + 5).map(|i| (i, meta("x"))));
        assert_eq!(cache.len(), CACHE_SIZE);
        assert_eq!(cache.get(4), Lookup::Missing);
        assert!(matches!(cache.get(5), Lookup::Loaded(_)));
        // placeholders count too: a request evicts the least recently used
        let requested: Vec<i32> = (-10..0).collect();
        assert_eq!(cache.request(requested.clone()), requested);
        assert_eq!(cache.len(), CACHE_SIZE);
        assert!(requested.iter().all(|&c| cache.get(c) == Lookup::Loading));
        assert!(matches!(cache.get(5), Lookup::Loaded(_))); // touched above
        assert_eq!(cache.get(6), Lookup::Missing);
    }

    #[test]
    fn dropped_placeholders_are_requested_again_and_loaded_rows_kept() {
        let mut cache = MetadataCache::new();
        cache.put([(1, meta("one"))]);
        assert_eq!(cache.request([1, 2, 3]), [2, 3]);
        cache.drop_loading();
        assert_eq!(cache.len(), 1);
        assert!(matches!(cache.get(1), Lookup::Loaded(_)));
        assert_eq!(cache.request([1, 2, 3]), [2, 3]);
    }

    #[test]
    fn stale_placeholders_expire_after_max_loading_tasks() {
        let mut cache = MetadataCache::new();
        cache.request([100]); // task 0
        // upstream drops a placeholder once `task <= current - MAX_LOADING_TASKS`
        for i in 0..MAX_LOADING_TASKS as i32 - 2 {
            cache.request([i]);
        }
        assert_eq!(cache.get(100), Lookup::Loading);
        cache.request([1000]);
        assert_eq!(cache.get(100), Lookup::Missing);
        assert_eq!(cache.request([100]), [100]);
    }

    #[test]
    fn cache_is_bounded_and_keeps_recently_used_rows() {
        let mut cache = MetadataCache::new();
        cache.put([(0, meta("first"))]);
        cache.put((1..CACHE_SIZE as i32).map(|i| (i, meta("x"))));
        assert!(matches!(cache.get(0), Lookup::Loaded(_))); // touch the oldest
        cache.put([(-1, meta("new"))]);
        assert_eq!(cache.len(), CACHE_SIZE);
        assert!(matches!(cache.get(0), Lookup::Loaded(_)));
        assert_eq!(cache.get(1), Lookup::Missing);
    }

    #[test]
    fn subject_is_trimmed_with_tabs_replaced() {
        let metadata = VcsCommitMetadata::new(
            Hash::build("ab"),
            vec![],
            5,
            ig_log::data::vcs_root("/r"),
            "\tfix\tit \n".to_string(),
            ig_log::data::VcsUser::create_user("Ann", "a@x"),
            "fix\tit".to_string(),
            ig_log::data::VcsUser::create_user("Bob", "b@x"),
            7,
        );
        let row = RowMeta::from_metadata(&metadata);
        assert_eq!(
            (row.subject.as_str(), row.author.as_str(), row.author_time),
            ("fix it", "Ann", 7)
        );
    }
}
