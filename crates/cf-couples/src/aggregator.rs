//! The in-memory coupling accumulator: sums the per-commit payloads into the
//! file co-occurrence matrix and the per-developer file touches, applies
//! renames as they happen, and reduces the result to the report's file set.

use crate::matrix::{
    build_files_index, compute_files_matrix, compute_people_matrix, People, RawFiles,
};
use crate::metrics::ReportData;
use crate::tc::{CommitData, RenamePair};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// In-memory coupling accumulator.
#[derive(Debug, Default)]
pub struct Aggregator {
    files: RawFiles,
    people: People,
    people_commits: Vec<i64>,
    people_number: usize,
}

impl Aggregator {
    /// Creates an aggregator sized for `people_number` developers. Slices are
    /// `people_number + 1` long to leave room for the "missing author" slot;
    /// they grow on demand when a later author id appears.
    #[must_use]
    pub fn new(people_number: usize) -> Self {
        Self {
            files: BTreeMap::new(),
            people: vec![BTreeMap::new(); people_number + 1],
            people_commits: vec![0; people_number + 1],
            people_number,
        }
    }

    /// Ingests a single per-commit payload: the commit's renames are applied
    /// first (so the moved file keeps one identity), then its author touches
    /// and its coupling context are accumulated.
    pub fn add(&mut self, author: usize, data: &CommitData) {
        self.ensure_capacity(author + 1);
        for rename in &data.renames {
            self.apply_rename(rename);
        }
        if data.commit_counted {
            self.people_commits[author] += 1;
        }
        self.add_author_files(&data.author_files, author);
        self.add_file_couplings(&data.coupling_files);
    }

    /// Carries every accumulated count of `from_name` onto `to_name`: the
    /// co-change row and column (the diagonal included) and every
    /// developer's touch count. A co-change recorded between the two paths
    /// themselves (a rename onto a path that was deleted earlier in the
    /// walk) is dropped, so the merged diagonal is the sum of both paths'
    /// change counts.
    fn apply_rename(&mut self, rename: &RenamePair) {
        let (from, to) = (rename.from_name.as_str(), rename.to_name.as_str());
        if from == to {
            return;
        }
        if let Some(row) = self.files.remove(from) {
            for (other, count) in row {
                if other == to {
                    // The cross term: dropped on both sides.
                    if let Some(lane) = self.files.get_mut(to) {
                        lane.remove(from);
                    }
                    continue;
                }
                let key = if other == from { to } else { other.as_str() };
                if other != from {
                    if let Some(lane) = self.files.get_mut(&other) {
                        if let Some(moved) = lane.remove(from) {
                            *lane.entry(to.to_string()).or_insert(0) += moved;
                        }
                    }
                }
                *self
                    .files
                    .entry(to.to_string())
                    .or_default()
                    .entry(key.to_string())
                    .or_insert(0) += count;
            }
        }
        for touches in &mut self.people {
            if let Some(moved) = touches.remove(from) {
                *touches.entry(to.to_string()).or_insert(0) += moved;
            }
        }
    }

    /// Merges per-commit author file touches.
    fn add_author_files(&mut self, author_files: &BTreeMap<String, i64>, author: usize) {
        for (file, &count) in author_files {
            *self.people[author].entry(file.clone()).or_insert(0) += count;
        }
    }

    /// Increments the file co-occurrence matrix for every ordered file pair,
    /// including the diagonal.
    fn add_file_couplings(&mut self, coupling_files: &[String]) {
        for a in coupling_files {
            let lane = self.files.entry(a.clone()).or_default();
            for b in coupling_files {
                *lane.entry(b.clone()).or_insert(0) += 1;
            }
        }
    }

    /// Grows the people/commit slices to at least `min_size`.
    fn ensure_capacity(&mut self, min_size: usize) {
        if min_size <= self.people.len() {
            return;
        }
        self.people.resize_with(min_size, BTreeMap::new);
        self.people_commits.resize(min_size, 0);
    }

    /// Returns the accumulated raw file coupling map.
    #[must_use]
    pub const fn raw_files(&self) -> &RawFiles {
        &self.files
    }

    /// Returns the accumulated per-developer file maps.
    #[must_use]
    pub const fn people(&self) -> &People {
        &self.people
    }

    /// Builds the dense [`ReportData`] from the accumulated state.
    ///
    /// The report lists every file that was coupled or touched in the walk,
    /// restricted to `current_files` when given (the files present at the
    /// window head; `None` keeps every accumulated file). `files_lines_by_name`
    /// supplies the per-file line counts the caller read from the head tree.
    #[must_use]
    pub fn build_report(
        &self,
        current_files: Option<&HashSet<String>>,
        files_lines_by_name: &BTreeMap<String, i32>,
    ) -> ReportData {
        let (reduced_files, reduced_people) = self.reduce(current_files);
        let names: BTreeSet<&str> = reduced_files
            .keys()
            .chain(reduced_people.iter().flat_map(BTreeMap::keys))
            .map(String::as_str)
            .collect();
        let (files_sequence, files_index) = build_files_index(names);

        let files_lines: Vec<i32> = files_sequence
            .iter()
            .map(|f| files_lines_by_name.get(f).copied().unwrap_or(0))
            .collect();

        let effective_people = if reduced_people.len() > self.people_number + 1 {
            reduced_people.len() - 1
        } else {
            self.people_number
        };

        let (people_matrix, people_files) =
            compute_people_matrix(&reduced_people, &files_index, effective_people);
        let files_matrix = compute_files_matrix(&reduced_files, &files_sequence, &files_index);

        ReportData {
            people_matrix,
            people_files,
            files: files_sequence,
            files_lines,
            files_matrix,
            reversed_people_dict: Vec::new(),
        }
    }

    /// Filters files and people to the current-file set. With
    /// `current_files = None`, keeps everything.
    fn reduce(&self, current_files: Option<&HashSet<String>>) -> (RawFiles, People) {
        let keep = |f: &str| current_files.map_or(true, |s| s.contains(f));

        let mut reduced_files: RawFiles = BTreeMap::new();
        for (file, refmap) in &self.files {
            if !keep(file) {
                continue;
            }
            let fmap: BTreeMap<String, i64> = refmap
                .iter()
                .filter(|(other, &refval)| refval > 0 && keep(other))
                .map(|(o, &c)| (o.clone(), c))
                .collect();
            if !fmap.is_empty() {
                reduced_files.insert(file.clone(), fmap);
            }
        }

        let reduced_people: People = self
            .people
            .iter()
            .map(|counts| {
                counts
                    .iter()
                    .filter(|(file, &count)| count > 0 && keep(file))
                    .map(|(f, &c)| (f.clone(), c))
                    .collect()
            })
            .collect();

        (reduced_files, reduced_people)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(files: &[&str]) -> CommitData {
        CommitData {
            coupling_files: files.iter().map(|s| (*s).to_string()).collect(),
            author_files: files.iter().map(|s| ((*s).to_string(), 1)).collect(),
            renames: Vec::new(),
            commit_counted: true,
        }
    }

    fn rename(from: &str, to: &str) -> RenamePair {
        RenamePair {
            from_name: from.to_string(),
            to_name: to.to_string(),
        }
    }

    fn current(files: &[&str]) -> HashSet<String> {
        files.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn add_builds_symmetric_file_matrix() {
        let mut agg = Aggregator::new(1);
        agg.add(0, &commit(&["a.go", "b.go"]));
        let raw = agg.raw_files();
        assert_eq!(raw["a.go"]["a.go"], 1);
        assert_eq!(raw["a.go"]["b.go"], 1);
        assert_eq!(raw["b.go"]["a.go"], 1);
        assert_eq!(raw["b.go"]["b.go"], 1);
    }

    #[test]
    fn repeated_commits_accumulate() {
        let mut agg = Aggregator::new(1);
        agg.add(0, &commit(&["a.go", "b.go"]));
        agg.add(0, &commit(&["a.go", "b.go"]));
        assert_eq!(agg.raw_files()["a.go"]["b.go"], 2);
        // author touched each file twice.
        assert_eq!(agg.people()[0]["a.go"], 2);
    }

    #[test]
    fn build_report_reduces_to_current_files() {
        let mut agg = Aggregator::new(1);
        agg.add(0, &commit(&["a.go", "b.go"]));
        agg.add(0, &commit(&["a.go", "old.go"]));
        let lines = BTreeMap::new();
        let report = agg.build_report(Some(&current(&["a.go", "b.go"])), &lines);
        // old.go dropped.
        assert_eq!(report.files, vec!["a.go", "b.go"]);
        // a.go self count = 2 (appeared in 2 commits).
        let a_idx = 0;
        assert_eq!(report.files_matrix[a_idx][&a_idx], 2);
        // a.go-b.go co-change = 1.
        assert_eq!(report.files_matrix[0][&1], 1);
    }

    /// A file touched only by an author (no coupling context — an oversized
    /// commit) is still listed, with an empty coupling row.
    #[test]
    fn build_report_lists_files_with_only_author_touches() {
        let mut agg = Aggregator::new(1);
        agg.add(
            0,
            &CommitData {
                coupling_files: Vec::new(),
                author_files: BTreeMap::from([("big.go".to_string(), 1)]),
                renames: Vec::new(),
                commit_counted: true,
            },
        );
        agg.add(1, &commit(&["a.go"]));
        let report = agg.build_report(None, &BTreeMap::new());
        assert_eq!(report.files, vec!["a.go", "big.go"]);
        assert!(report.files_matrix[1].is_empty());
        assert_eq!(report.people_files[0], vec![1]);
        assert_eq!(report.people_matrix[0][&0], 1);
    }

    #[test]
    fn ensure_capacity_grows_for_new_author() {
        let mut agg = Aggregator::new(1);
        agg.add(5, &commit(&["x.go"]));
        assert!(agg.people().len() >= 6);
        assert_eq!(agg.people()[5]["x.go"], 1);
    }

    /// `git mv b d`: d inherits b's row, column, diagonal and owners, plus
    /// the rename commit's own touch; b disappears.
    #[test]
    fn rename_merges_the_old_row_column_and_owners_into_the_new_path() {
        let mut agg = Aggregator::new(1);
        agg.add(0, &commit(&["a", "b"]));
        agg.add(1, &commit(&["a", "b", "c"]));
        let mut mv = commit(&["d"]);
        mv.renames.push(rename("b", "d"));
        agg.add(0, &mv);

        let raw = agg.raw_files();
        assert!(!raw.contains_key("b"), "{raw:?}");
        assert!(!raw["a"].contains_key("b"), "{raw:?}");
        assert!(!raw["c"].contains_key("b"), "{raw:?}");
        assert_eq!(raw["d"]["d"], 3, "b's 2 changes + the rename commit");
        assert_eq!(raw["d"]["a"], 2);
        assert_eq!(raw["a"]["d"], 2);
        assert_eq!(raw["d"]["c"], 1);
        assert_eq!(raw["c"]["d"], 1);
        assert_eq!(agg.people()[0].get("b"), None);
        assert_eq!(agg.people()[0]["d"], 2);
        assert_eq!(agg.people()[1]["d"], 1);

        let report = agg.build_report(Some(&current(&["a", "c", "d"])), &BTreeMap::new());
        assert_eq!(report.files, vec!["a", "c", "d"]);
        assert_eq!(report.files_matrix[0][&2], 2);
        assert_eq!(report.files_matrix[2][&2], 3);
    }

    /// Renames chain: b → d, then d → e carries everything to e.
    #[test]
    fn chained_renames_follow_the_latest_path() {
        let mut agg = Aggregator::new(1);
        agg.add(0, &commit(&["a", "b"]));
        let mut first = commit(&["d"]);
        first.renames.push(rename("b", "d"));
        agg.add(0, &first);
        let mut second = commit(&["e"]);
        second.renames.push(rename("d", "e"));
        agg.add(0, &second);

        let raw = agg.raw_files();
        assert_eq!(raw.keys().collect::<Vec<_>>(), vec!["a", "e"]);
        assert_eq!(raw["e"]["e"], 3);
        assert_eq!(raw["e"]["a"], 1);
        assert_eq!(raw["a"]["e"], 1);
        assert_eq!(agg.people()[0]["e"], 3);
    }

    /// A rename whose source has no history (outside the window) and a
    /// self-rename are no-ops.
    #[test]
    fn rename_without_history_is_a_no_op() {
        let mut agg = Aggregator::new(1);
        let mut data = commit(&["d"]);
        data.renames.push(rename("ghost", "d"));
        data.renames.push(rename("d", "d"));
        agg.add(0, &data);
        assert_eq!(agg.raw_files()["d"]["d"], 1);
        assert_eq!(agg.people()[0]["d"], 1);
    }

    /// A rename onto a path deleted earlier drops the cross term: the merged
    /// diagonal is the sum of both paths' change counts.
    #[test]
    fn rename_onto_a_dead_path_sums_the_change_counts() {
        let mut agg = Aggregator::new(1);
        agg.add(0, &commit(&["b", "d"]));
        agg.add(0, &commit(&["b"]));
        let mut mv = commit(&["d"]);
        mv.renames.push(rename("b", "d"));
        agg.add(0, &mv);
        let raw = agg.raw_files();
        assert_eq!(raw.keys().collect::<Vec<_>>(), vec!["d"]);
        assert_eq!(raw["d"].keys().collect::<Vec<_>>(), vec!["d"]);
        assert_eq!(raw["d"]["d"], 1 + 2 + 1);
        assert_eq!(agg.people()[0]["d"], 4);
    }
}
