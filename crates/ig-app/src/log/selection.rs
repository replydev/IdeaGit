//! Row selection of the Log table, kept by commit so it survives refreshes.
//!
//! Behaves like the Swing `JTable` the Log uses (`VcsLogGraphTable`, `ListSelectionModel`
//! `MULTIPLE_INTERVAL_SELECTION`): click selects one row, Cmd/Ctrl+click toggles a row,
//! Shift+click and Shift+arrows extend from the anchor, arrows move the lead and select it.
//! Upstream restores the selection by commit after a refresh (`VcsLogGraphTable.Selection`);
//! here the selection *is* the set of commits, so rows are recomputed from the new graph.

use std::collections::BTreeSet;

use ig_log::data::VcsLogCommitStorageIndex as Commit;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    commits: BTreeSet<Commit>,
    anchor: Option<Commit>,
    lead: Option<Commit>,
}

/// Maps rows to commits and back for the current graph.
pub trait Rows {
    fn count(&self) -> usize;
    fn commit(&self, row: usize) -> Commit;
    fn row(&self, commit: Commit) -> Option<usize>;
}

impl Selection {
    pub fn is_selected(&self, commit: Commit) -> bool {
        self.commits.contains(&commit)
    }

    pub fn commits(&self) -> &BTreeSet<Commit> {
        &self.commits
    }

    pub fn len(&self) -> usize {
        self.commits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commits.is_empty()
    }

    /// The lead row (where keyboard navigation continues), if its commit is still shown.
    pub fn lead_row(&self, rows: &dyn Rows) -> Option<usize> {
        self.lead.and_then(|c| rows.row(c))
    }

    /// A click on `row`: `toggle` with Cmd/Ctrl, `extend` with Shift.
    pub fn click(&mut self, row: usize, toggle: bool, extend: bool, rows: &dyn Rows) {
        let commit = rows.commit(row);
        if extend {
            let anchor_row = self.anchor.and_then(|a| rows.row(a)).unwrap_or(row);
            if !toggle {
                self.commits.clear();
            }
            let (from, to) = (anchor_row.min(row), anchor_row.max(row));
            self.commits.extend((from..=to).map(|r| rows.commit(r)));
            self.anchor.get_or_insert(commit);
        } else if toggle {
            if !self.commits.remove(&commit) {
                self.commits.insert(commit);
            }
            self.anchor = Some(commit);
        } else {
            self.commits.clear();
            self.commits.insert(commit);
            self.anchor = Some(commit);
        }
        self.lead = Some(commit);
    }

    /// Moves the lead by `delta` rows (clamped); `extend` keeps the anchor (Shift+arrows).
    /// Returns the new lead row, to scroll to.
    pub fn move_by(&mut self, delta: isize, extend: bool, rows: &dyn Rows) -> Option<usize> {
        let count = rows.count();
        if count == 0 {
            return None;
        }
        let target = match self.lead_row(rows) {
            Some(lead) => (lead as isize + delta).clamp(0, count as isize - 1) as usize,
            None => {
                if delta >= 0 {
                    0
                } else {
                    count - 1
                }
            }
        };
        self.click(target, false, extend, rows);
        Some(target)
    }

    /// Selects every row (`$SelectAll`).
    pub fn select_all(&mut self, rows: &dyn Rows) {
        self.commits = (0..rows.count()).map(|r| rows.commit(r)).collect();
    }

    /// Selects only `row` (a jump).
    pub fn select_only(&mut self, row: usize, rows: &dyn Rows) {
        self.click(row, false, false, rows);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Row r shows commit 100 + r.
    struct Linear(usize);

    impl Rows for Linear {
        fn count(&self) -> usize {
            self.0
        }
        fn commit(&self, row: usize) -> Commit {
            100 + row as Commit
        }
        fn row(&self, commit: Commit) -> Option<usize> {
            let r = (commit - 100) as usize;
            (r < self.0).then_some(r)
        }
    }

    fn rows_of(s: &Selection) -> Vec<Commit> {
        s.commits().iter().map(|c| c - 100).collect()
    }

    #[test]
    fn click_toggle_and_shift_extend() {
        let rows = Linear(10);
        let mut s = Selection::default();
        s.click(2, false, false, &rows);
        s.click(5, false, true, &rows);
        assert_eq!(rows_of(&s), [2, 3, 4, 5]);
        s.click(8, true, false, &rows);
        assert_eq!(rows_of(&s), [2, 3, 4, 5, 8]);
        s.click(3, true, false, &rows);
        assert_eq!(rows_of(&s), [2, 4, 5, 8]);
        s.click(1, false, false, &rows);
        assert_eq!(rows_of(&s), [1]);
    }

    #[test]
    fn keyboard_moves_and_extends_within_bounds() {
        let rows = Linear(4);
        let mut s = Selection::default();
        assert_eq!(s.move_by(1, false, &rows), Some(0));
        assert_eq!(s.move_by(1, true, &rows), Some(1));
        assert_eq!(s.move_by(10, true, &rows), Some(3));
        assert_eq!(rows_of(&s), [0, 1, 2, 3]);
        assert_eq!(s.move_by(-1, false, &rows), Some(2));
        assert_eq!(rows_of(&s), [2]);
        s.select_all(&rows);
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn selection_follows_commits_to_new_rows() {
        let mut s = Selection::default();
        s.click(1, false, false, &Linear(3));
        // after a refresh, commit 101 is gone: no lead row, the next move starts at the top
        assert_eq!(s.lead_row(&Linear(1)), None);
        assert_eq!(s.move_by(1, false, &Linear(1)), Some(0));
    }
}
