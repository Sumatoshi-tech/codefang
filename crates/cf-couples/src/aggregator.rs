//! In-memory coupling accumulator and pure reduction helpers.
//!
//! The full streaming aggregator runs over a disk-backed spill store, and the
//! consume/fork/merge pipeline driver depends on git tree diffs, identity
//! detection, and Bloom-filtered merge dedup. Those framework crates are not
//! yet wired in, so this module provides the **pure in-memory accumulation
//! and report-building** logic that the streaming aggregator wraps, behind
//! small traits the eventual framework integration can satisfy. See crate
//! TODOs.

use crate::matrix::{
    build_files_index, compute_files_matrix, compute_people_matrix, People, RawFiles,
};
use crate::metrics::ReportData;
use crate::tc::{CommitData, RenamePair};
use std::collections::BTreeMap;

/// Minimal interface the streaming aggregator needs from a spill store.
///
/// Defined here so the in-memory core compiles without `cf-spillstore`. The
/// real integration implements this over the spill store.
pub trait FileSpill {
    /// Returns the current in-memory `file -> otherFile -> count` buffer.
    fn current(&self) -> &RawFiles;
    /// Mutable access to the current buffer.
    fn current_mut(&mut self) -> &mut RawFiles;
    /// Number of files held in memory.
    fn len(&self) -> usize;
    /// True when no files are held in memory.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A trivial all-in-memory [`FileSpill`] (no disk overflow). Sufficient for
/// tests and small repos; the streaming aggregator swaps in a spilling impl.
#[derive(Debug, Default)]
pub struct MemFileSpill {
    files: RawFiles,
}

impl FileSpill for MemFileSpill {
    fn current(&self) -> &RawFiles {
        &self.files
    }
    fn current_mut(&mut self) -> &mut RawFiles {
        &mut self.files
    }
    fn len(&self) -> usize {
        self.files.len()
    }
}

/// In-memory coupling accumulator.
#[derive(Debug, Default)]
pub struct Aggregator {
    files: MemFileSpill,
    people: People,
    people_commits: Vec<i64>,
    renames: Vec<RenamePair>,
    people_number: usize,
}

impl Aggregator {
    /// Creates an aggregator sized for `people_number` developers. Slices are
    /// `people_number + 1` long to leave room for the "missing author" slot.
    #[must_use]
    pub fn new(people_number: usize) -> Self {
        Self {
            files: MemFileSpill::default(),
            people: vec![BTreeMap::new(); people_number + 1],
            people_commits: vec![0; people_number + 1],
            renames: Vec::new(),
            people_number,
        }
    }

    /// Ingests a single per-commit payload.
    pub fn add(&mut self, author: usize, data: &CommitData) {
        self.ensure_capacity(author + 1);
        if data.commit_counted {
            self.people_commits[author] += 1;
        }
        self.add_author_files(&data.author_files, author);
        self.add_file_couplings(&data.coupling_files);
        self.renames.extend(data.renames.iter().cloned());
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
        let files = self.files.current_mut();
        for a in coupling_files {
            let lane = files.entry(a.clone()).or_default();
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
    pub fn raw_files(&self) -> &RawFiles {
        self.files.current()
    }

    /// Returns the accumulated per-developer file maps.
    #[must_use]
    pub const fn people(&self) -> &People {
        &self.people
    }

    /// Builds the dense [`ReportData`] from the accumulated state, restricted
    /// to the given current-file set (without the git-tree line counting,
    /// which requires libgit2).
    ///
    /// `current_files`: when `Some`, only these files are retained (callers
    /// derive this from the final commit tree); when `None`, all accumulated
    /// files are kept. `files_lines_by_name` is supplied by the caller because
    /// line counts require reading blobs from the final commit (libgit2).
    #[must_use]
    pub fn build_report(
        &self,
        current_files: Option<&std::collections::HashSet<String>>,
        files_lines_by_name: &BTreeMap<String, i32>,
    ) -> ReportData {
        let (reduced_files, reduced_people) = self.reduce(current_files);
        let (files_sequence, files_index) = build_files_index(&reduced_files);

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

    /// Filters files and people to the current-file set — the public surface
    /// the store-record path shares with [`Aggregator::build_report`]. With
    /// `current_files = None`, keeps everything (the streaming fallback where
    /// the last commit's tree is unavailable).
    #[must_use]
    pub fn reduced(
        &self,
        current_files: Option<&std::collections::HashSet<String>>,
    ) -> (RawFiles, People) {
        self.reduce(current_files)
    }

    /// Filters files and people to the current-file set. With
    /// `current_files = None`, keeps everything.
    fn reduce(
        &self,
        current_files: Option<&std::collections::HashSet<String>>,
    ) -> (RawFiles, People) {
        let keep = |f: &str| current_files.map_or(true, |s| s.contains(f));

        let mut reduced_files: RawFiles = BTreeMap::new();
        for (file, refmap) in self.files.current() {
            if !keep(file) || refmap.is_empty() {
                continue;
            }
            let mut fmap: BTreeMap<String, i64> = BTreeMap::new();
            for (other, &refval) in refmap {
                if refval > 0 && keep(other) {
                    fmap.insert(other.clone(), refval);
                }
            }
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
        let current: std::collections::HashSet<String> = ["a.go".to_string(), "b.go".to_string()]
            .into_iter()
            .collect();
        let lines = BTreeMap::new();
        let report = agg.build_report(Some(&current), &lines);
        // old.go dropped.
        assert_eq!(report.files, vec!["a.go", "b.go"]);
        // a.go self count = 2 (appeared in 2 commits).
        let a_idx = 0;
        assert_eq!(report.files_matrix[a_idx][&a_idx], 2);
        // a.go-b.go co-change = 1.
        assert_eq!(report.files_matrix[0][&1], 1);
    }

    #[test]
    fn ensure_capacity_grows_for_new_author() {
        let mut agg = Aggregator::new(1);
        agg.add(5, &commit(&["x.go"]));
        assert!(agg.people().len() >= 6);
        assert_eq!(agg.people()[5]["x.go"], 1);
    }
}
