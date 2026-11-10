//! Row selection of the Log table, kept by commit so it survives refreshes.
//!
//! Behaves like the Swing `JTable` the Log uses (`VcsLogGraphTable`, `ListSelectionModel`
//! `MULTIPLE_INTERVAL_SELECTION`): click selects one row, Cmd/Ctrl+click toggles a row,
//! Shift+click and Shift+arrows extend from the anchor, arrows move the lead and select it.
//! Upstream restores the selection by commit after a refresh (`VcsLogGraphTable.Selection`);
//! here the selection *is* the set of commits, so rows are recomputed from the new graph, and
//! commits no longer shown are dropped ([`Selection::retain_visible`]).
//!
//! After the rows change, a lead or anchor no longer shown moves to a selected commit, or to none
//! (as `fireTableDataChanged` + `SelectionSnapshot.restore` leave them). Deviations: upstream
//! moves them even when still shown, to the last row `restore` re-adds (in hash-set order); here a
//! shown lead and anchor stay, and the replacement is the top-most selected commit. An anchor
//! that is somehow not shown at a Shift+click is replaced by the clicked commit. Without any
//! anchor, Shift+click extends from row 0, like `JTable.changeSelection` with no anchor (`-1`
//! becomes row 0).

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

    /// The lead commit (`ListSelectionModel.getLeadSelectionIndex`).
    pub fn lead(&self) -> Option<Commit> {
        self.lead
    }

    /// The anchor commit (`ListSelectionModel.getAnchorSelectionIndex`).
    pub fn anchor(&self) -> Option<Commit> {
        self.anchor
    }

    /// The lead row (where keyboard navigation continues), if its commit is still shown.
    pub fn lead_row(&self, rows: &dyn Rows) -> Option<usize> {
        self.lead.and_then(|c| rows.row(c))
    }

    /// A click on `row`: `toggle` with Cmd/Ctrl, `extend` with Shift (`JTable.changeSelection`).
    /// Ignored if `row` is out of range.
    pub fn click(&mut self, row: usize, toggle: bool, extend: bool, rows: &dyn Rows) {
        if row >= rows.count() {
            return;
        }
        let commit = rows.commit(row);
        if extend {
            let (anchor_row, anchor_selected) = match self.anchor {
                // JTable: no anchor (-1) extends from row 0 and counts as not selected
                None => {
                    self.anchor = Some(rows.commit(0));
                    (0, false)
                }
                Some(anchor) => match rows.row(anchor) {
                    Some(anchor_row) => (anchor_row, self.commits.contains(&anchor)),
                    // deviation: a vanished anchor is replaced by the clicked commit
                    None => {
                        self.anchor = Some(commit);
                        (row, self.commits.contains(&commit))
                    }
                },
            };
            let (from, to) = (anchor_row.min(row), anchor_row.max(row));
            let range = (from..=to).map(|r| rows.commit(r));
            if !toggle {
                // `setSelectionInterval(anchor, row)`
                self.commits.clear();
                self.commits.extend(range);
            } else if anchor_selected {
                // `addSelectionInterval(anchor, row)`
                self.commits.extend(range);
            } else {
                // `removeSelectionInterval(anchor, row)`
                for c in range {
                    self.commits.remove(&c);
                }
            }
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

    /// Moves the lead to `row` (clamped to the last row) and selects only it: Home / End, which
    /// don't depend on a current lead. Returns the row, to scroll to.
    pub fn move_to(&mut self, row: usize, rows: &dyn Rows) -> Option<usize> {
        let count = rows.count();
        if count == 0 {
            return None;
        }
        let target = row.min(count - 1);
        self.click(target, false, false, rows);
        Some(target)
    }

    /// Selects every row (`$SelectAll`).
    pub fn select_all(&mut self, rows: &dyn Rows) {
        self.commits = (0..rows.count()).map(|r| rows.commit(r)).collect();
    }

    /// Drops the commits that `rows` no longer shows (after the rows changed). A lead or anchor no
    /// longer shown moves, with the other, to the top-most selected commit, or to none if no
    /// commit is left: upstream `fireTableDataChanged` clears the lead and anchor, then
    /// `SelectionSnapshot.restore` re-adds each selected row with `addRowSelectionInterval`, which
    /// makes it the lead and anchor.
    pub fn retain_visible(&mut self, rows: &dyn Rows) {
        self.commits.retain(|&c| rows.row(c).is_some());
        let shown = |commit: Option<Commit>| commit.is_none_or(|c| rows.row(c).is_some());
        if !shown(self.lead) || !shown(self.anchor) {
            let top = self.commits.iter().copied().min_by_key(|&c| rows.row(c));
            self.lead = top;
            self.anchor = top;
        }
    }

    /// Selects only `row` (a jump); ignored if out of range.
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
    fn out_of_range_rows_are_ignored() {
        let rows = Linear(3);
        let mut s = Selection::default();
        s.click(1, false, false, &rows);
        s.click(3, false, false, &rows);
        s.click(usize::MAX, true, true, &rows);
        s.select_only(10, &rows);
        assert_eq!(rows_of(&s), [1]);
        assert_eq!(s.lead_row(&rows), Some(1));
    }

    #[test]
    fn invisible_anchor_is_replaced_by_the_clicked_commit() {
        let mut s = Selection::default();
        s.click(8, false, false, &Linear(10)); // anchor: commit 108
        // only 5 rows are shown, and the selection was not refreshed: Shift+click extends from
        // the clicked row
        let rows = Linear(5);
        s.click(2, false, true, &rows);
        assert_eq!(rows_of(&s), [2]);
        s.click(4, false, true, &rows);
        assert_eq!(rows_of(&s), [2, 3, 4]);
    }

    #[test]
    fn retain_visible_moves_a_hidden_lead_and_anchor() {
        // nothing left: no lead or anchor, so Shift+click extends from row 0
        let mut s = Selection::default();
        s.click(8, false, false, &Linear(10));
        let rows = Linear(5);
        s.retain_visible(&rows);
        assert!(s.is_empty());
        assert_eq!((s.lead(), s.anchor()), (None, None));
        s.click(2, false, true, &rows);
        assert_eq!(rows_of(&s), [0, 1, 2]);
        // the lead and anchor move to the top-most selected commit
        let mut s = Selection::default();
        s.click(3, false, false, &Linear(10));
        s.click(1, true, false, &Linear(10));
        s.click(8, true, false, &Linear(10));
        s.retain_visible(&Linear(5));
        assert_eq!(rows_of(&s), [1, 3]);
        assert_eq!((s.lead(), s.anchor()), (Some(101), Some(101)));
        // a shown lead and anchor stay
        let mut s = Selection::default();
        s.click(1, false, false, &Linear(10));
        s.click(3, false, true, &Linear(10));
        s.retain_visible(&Linear(5));
        assert_eq!((s.lead(), s.anchor()), (Some(103), Some(101)));
    }

    #[test]
    fn shift_click_without_an_anchor_extends_from_row_0() {
        let rows = Linear(10);
        let mut s = Selection::default();
        s.click(3, false, true, &rows);
        assert_eq!(rows_of(&s), [0, 1, 2, 3]);
        // the anchor stays on row 0
        s.click(5, false, true, &rows);
        assert_eq!(rows_of(&s), [0, 1, 2, 3, 4, 5]);
        // Cmd+Shift with no anchor: the anchor counts as unselected, so the range is removed
        let mut s = Selection::default();
        s.select_all(&rows);
        s.click(2, true, true, &rows);
        assert_eq!(rows_of(&s), [3, 4, 5, 6, 7, 8, 9]);
        // Shift+Down with no selection also anchors on row 0 (lead goes to row 0)
        let mut s = Selection::default();
        assert_eq!(s.move_by(1, true, &rows), Some(0));
        assert_eq!(rows_of(&s), [0]);
    }

    #[test]
    fn toggle_extend_follows_the_anchor_state() {
        let rows = Linear(10);
        let mut s = Selection::default();
        s.click(0, false, false, &rows);
        s.click(9, true, false, &rows);
        s.click(2, true, false, &rows); // anchor 2, selected
        s.click(4, true, true, &rows); // adds 2..=4
        assert_eq!(rows_of(&s), [0, 2, 3, 4, 9]);
        s.click(3, true, false, &rows); // anchor 3, now unselected
        s.click(9, true, true, &rows); // removes 3..=9
        assert_eq!(rows_of(&s), [0, 2]);
    }

    #[test]
    fn home_and_end_without_a_selection() {
        let rows = Linear(5);
        let mut s = Selection::default();
        assert_eq!(s.move_to(0, &rows), Some(0));
        assert_eq!(rows_of(&s), [0]);
        let mut s = Selection::default();
        assert_eq!(s.move_to(usize::MAX, &rows), Some(4));
        assert_eq!(rows_of(&s), [4]);
        assert_eq!(Selection::default().move_to(0, &Linear(0)), None);
    }

    #[test]
    fn retain_visible_drops_hidden_commits() {
        let mut s = Selection::default();
        s.select_all(&Linear(6));
        s.retain_visible(&Linear(4));
        assert_eq!(rows_of(&s), [0, 1, 2, 3]);
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
