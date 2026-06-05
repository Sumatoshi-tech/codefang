//! Deterministic coupling-matrix construction (pure logic from `history.go`).
//!
//! These functions take the raw accumulated coupling data and produce the
//! index-keyed matrices that form the analyzer report. All index assignment
//! flows from byte-sorted name lists so the output is reproducible.

use std::collections::{BTreeMap, HashMap};

/// Raw file co-occurrence data: `file -> otherFile -> count`.
pub type RawFiles = BTreeMap<String, BTreeMap<String, i64>>;

/// Per-developer file-touch counts: index = developer ID, value = `file ->
/// commits`.
pub type People = Vec<BTreeMap<String, i64>>;

/// Builds the byte-sorted file-name sequence and a name→index map
/// (Go: `buildFilesIndex`).
///
/// The sequence is sorted with `sort.Strings` semantics (raw byte order), which
/// matches Rust's default `str` ordering for the ASCII/UTF-8 paths involved.
pub fn build_files_index(files: &RawFiles) -> (Vec<String>, HashMap<String, usize>) {
    let mut sequence: Vec<String> = files.keys().cloned().collect();
    sequence.sort();
    let mut index = HashMap::with_capacity(sequence.len());
    for (i, file) in sequence.iter().enumerate() {
        index.insert(file.clone(), i);
    }
    (sequence, index)
}

/// A developer's commit count on a file (Go: `devCommit`).
#[derive(Debug, Clone, Copy)]
struct DevCommit {
    dev_id: usize,
    commits: i64,
}

/// Builds the developer co-occurrence matrix and per-developer file-index lists
/// (Go: `computePeopleMatrix`).
///
/// Returns `(matrix, people_files)` where:
/// * `matrix[i]` maps developer `j` → `sum over files of min(commits_i,
///   commits_j)`, including the diagonal `i == j`.
/// * `people_files[i]` is the byte-sorted list of file indices developer `i`
///   touched.
///
/// Both have length `people_number + 1`, matching Go.
pub fn compute_people_matrix(
    people: &People,
    files_index: &HashMap<String, usize>,
    people_number: usize,
) -> (Vec<BTreeMap<usize, i64>>, Vec<Vec<usize>>) {
    let people_files = build_people_file_indices(people, files_index, people_number);
    let inverted = build_inverted_index(people, files_index, people_number);
    let matrix = accumulate_matrix(&inverted, people_number);
    (matrix, people_files)
}

/// Builds sorted per-developer file-index lists (Go: `buildPeopleFileIndices`).
fn build_people_file_indices(
    people: &People,
    files_index: &HashMap<String, usize>,
    people_number: usize,
) -> Vec<Vec<usize>> {
    let mut result = vec![Vec::new(); people_number + 1];
    for (i, files) in people.iter().enumerate() {
        if i > people_number {
            break;
        }
        for file in files.keys() {
            if let Some(&fi) = files_index.get(file) {
                result[i].push(fi);
            }
        }
        result[i].sort_unstable();
    }
    result
}

/// Builds the `file -> [(devID, commits)]` inverted index
/// (Go: `buildInvertedIndex`). Only positive commit counts are recorded.
fn build_inverted_index(
    people: &People,
    _files_index: &HashMap<String, usize>,
    people_number: usize,
) -> BTreeMap<String, Vec<DevCommit>> {
    // Go keys the inverted index by file name regardless of `files_index`
    // membership (`buildInvertedIndex` in history.go): filtering to indexed
    // files happens separately via `peopleFiles`. The `files_index` parameter is
    // retained only for signature parity with `compute_people_matrix`; the
    // accumulated matrix is identical either way because only co-touched files
    // ever produce a non-zero `min(commits_i, commits_j)` contribution.
    let mut inverted: BTreeMap<String, Vec<DevCommit>> = BTreeMap::new();
    for (i, files) in people.iter().enumerate() {
        if i > people_number {
            break;
        }
        for (file, &commits) in files {
            if commits > 0 {
                inverted
                    .entry(file.clone())
                    .or_default()
                    .push(DevCommit { dev_id: i, commits });
            }
        }
    }
    inverted
}

/// Accumulates the developer co-occurrence matrix from the inverted index
/// (Go: `accumulateMatrix`).
///
/// For each file, adds `min(commits_a, commits_b)` for every ordered developer
/// pair (including `a == b`).
fn accumulate_matrix(
    inverted: &BTreeMap<String, Vec<DevCommit>>,
    people_number: usize,
) -> Vec<BTreeMap<usize, i64>> {
    let mut matrix: Vec<BTreeMap<usize, i64>> = vec![BTreeMap::new(); people_number + 1];
    for devs in inverted.values() {
        for a in devs {
            for b in devs {
                let delta = a.commits.min(b.commits);
                if delta > 0 {
                    *matrix[a.dev_id].entry(b.dev_id).or_insert(0) += delta;
                }
            }
        }
    }
    matrix
}

/// Builds the file co-occurrence matrix keyed by file index
/// (Go: `computeFilesMatrix`).
///
/// `matrix[i]` maps the index of every co-changed file to its count, for the
/// file at sorted position `i`.
pub fn compute_files_matrix(
    raw_files: &RawFiles,
    files_sequence: &[String],
    files_index: &HashMap<String, usize>,
) -> Vec<BTreeMap<usize, i64>> {
    let mut matrix: Vec<BTreeMap<usize, i64>> = vec![BTreeMap::new(); files_index.len()];
    for (i, row) in matrix.iter_mut().enumerate() {
        if let Some(inner) = raw_files.get(&files_sequence[i]) {
            for (other_file, &cooccs) in inner {
                if let Some(&j) = files_index.get(other_file) {
                    row.insert(j, cooccs);
                }
            }
        }
    }
    matrix
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(pairs: &[(&str, &[(&str, i64)])]) -> RawFiles {
        pairs
            .iter()
            .map(|(f, inner)| {
                (
                    f.to_string(),
                    inner.iter().map(|(o, c)| (o.to_string(), *c)).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn files_index_is_byte_sorted() {
        let files = raw(&[("b.go", &[]), ("a.go", &[]), ("c.go", &[])]);
        let (seq, idx) = build_files_index(&files);
        assert_eq!(seq, vec!["a.go", "b.go", "c.go"]);
        assert_eq!(idx["a.go"], 0);
        assert_eq!(idx["c.go"], 2);
    }

    #[test]
    fn files_matrix_maps_to_indices() {
        // a.go co-changes with a.go(2 self) and b.go(1).
        let files = raw(&[
            ("a.go", &[("a.go", 2), ("b.go", 1)]),
            ("b.go", &[("b.go", 2), ("a.go", 1)]),
        ]);
        let (seq, idx) = build_files_index(&files);
        let m = compute_files_matrix(&files, &seq, &idx);
        // a.go = 0, b.go = 1.
        assert_eq!(m[0][&0], 2);
        assert_eq!(m[0][&1], 1);
        assert_eq!(m[1][&1], 2);
        assert_eq!(m[1][&0], 1);
    }

    #[test]
    fn people_matrix_uses_min_commits() {
        // file a.go touched by dev0 (3 commits) and dev1 (5 commits).
        let files = raw(&[("a.go", &[("a.go", 1)])]);
        let (_seq, idx) = build_files_index(&files);
        let mut people: People = vec![BTreeMap::new(), BTreeMap::new()];
        people[0].insert("a.go".to_string(), 3);
        people[1].insert("a.go".to_string(), 5);
        let (matrix, pfiles) = compute_people_matrix(&people, &idx, 1);
        // diagonal: dev0 = min(3,3)=3, dev1 = min(5,5)=5.
        assert_eq!(matrix[0][&0], 3);
        assert_eq!(matrix[1][&1], 5);
        // off-diagonal: min(3,5) = 3 both directions.
        assert_eq!(matrix[0][&1], 3);
        assert_eq!(matrix[1][&0], 3);
        // both devs touched file index 0.
        assert_eq!(pfiles[0], vec![0]);
        assert_eq!(pfiles[1], vec![0]);
    }
}
