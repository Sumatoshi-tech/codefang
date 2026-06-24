//! Bounded store-record computation.
//!
//! The store path emits compact, bounded records instead of dense matrices:
//! top-K file coupling pairs, a bounded developer matrix, per-file ownership,
//! and an aggregate summary. The sparse computations here avoid materializing
//! the dense `O(N²)` matrices while producing the same numbers as the dense
//! path.

use crate::matrix::RawFiles;
use crate::metrics::{
    coupling_strength_pub, AggregateData, FileCouplingData, COUPLING_THRESHOLD_HIGH,
};
use std::collections::HashMap;

/// Store record kinds.
pub const KIND_FILE_COUPLING: &str = "file_coupling";
pub const KIND_DEV_MATRIX: &str = "dev_matrix";
pub const KIND_OWNERSHIP: &str = "ownership";
pub const KIND_AGGREGATE: &str = "aggregate";

/// A bounded developer coupling matrix for store serialization.
/// JSON keys: `names`, `matrix`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreDevMatrix {
    pub names: Vec<String>,
    pub matrix: Vec<std::collections::BTreeMap<usize, i64>>,
}

/// Extracts file coupling pairs from the sparse coupling map without
/// materializing a dense matrix.
///
/// Only upper-triangle pairs (`j > i`) with `count >= min_weight` are kept.
/// Result is unsorted (the caller sorts by `co_changes` descending and
/// applies the top-K limit; see [`top_k_file_coupling`]).
#[must_use]
#[allow(clippy::implicit_hasher)] // public signature is frozen
pub fn compute_sparse_coupling(
    reduced_files: &RawFiles,
    files_sequence: &[String],
    files_index: &HashMap<String, usize>,
    min_weight: i64,
) -> Vec<FileCouplingData> {
    let mut result = Vec::new();
    for (i, file1) in files_sequence.iter().enumerate() {
        let Some(lane) = reduced_files.get(file1) else {
            continue;
        };
        if lane.is_empty() {
            continue;
        }
        let self_i = lane.get(file1).copied().unwrap_or(0);
        for (file2, &co_changes) in lane {
            let Some(&j) = files_index.get(file2) else {
                continue;
            };
            if j <= i {
                continue;
            }
            if co_changes < min_weight {
                continue;
            }
            let self_j = reduced_files
                .get(file2)
                .and_then(|l| l.get(file2))
                .copied()
                .unwrap_or(0);
            result.push(FileCouplingData {
                file1: file1.clone(),
                file2: file2.clone(),
                co_changes,
                strength: coupling_strength_pub(co_changes, self_i, self_j),
            });
        }
    }
    result
}

/// Sorts sparse coupling pairs by `co_changes` descending and truncates to
/// `top_k`.
#[must_use]
pub fn top_k_file_coupling(
    mut pairs: Vec<FileCouplingData>,
    top_k: usize,
) -> Vec<FileCouplingData> {
    pairs.sort_by(|a, b| b.co_changes.cmp(&a.co_changes));
    let limit = pairs.len().min(top_k);
    pairs.truncate(limit);
    pairs
}

/// Computes aggregate statistics directly from the sparse coupling map.
#[must_use]
#[allow(clippy::implicit_hasher)] // public signature is frozen
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // report fields are i32
#[allow(clippy::cast_precision_loss)] // contractual float math on counts
pub fn compute_sparse_aggregate(
    reduced_files: &RawFiles,
    files_sequence: &[String],
    files_index: &HashMap<String, usize>,
    reversed_names: &[String],
) -> AggregateData {
    let mut total_co_changes: i64 = 0;
    let mut pair_count: i64 = 0;
    let mut highly_coupled: i32 = 0;
    let mut total_strength: f64 = 0.0;

    for file1 in files_sequence {
        let i = files_index[file1];
        let Some(lane) = reduced_files.get(file1) else {
            continue;
        };
        let self_i = lane.get(file1).copied().unwrap_or(0);
        for (file2, &co_changes) in lane {
            let Some(&j) = files_index.get(file2) else {
                continue;
            };
            if j <= i || co_changes <= 0 {
                continue;
            }
            let self_j = reduced_files
                .get(file2)
                .and_then(|l| l.get(file2))
                .copied()
                .unwrap_or(0);
            total_co_changes += co_changes;
            pair_count += 1;
            if co_changes >= COUPLING_THRESHOLD_HIGH {
                highly_coupled += 1;
            }
            total_strength += coupling_strength_pub(co_changes, self_i, self_j);
        }
    }

    AggregateData {
        total_files: files_sequence.len() as i32,
        total_developers: reversed_names.len() as i32,
        total_co_changes,
        avg_coupling_strength: if pair_count > 0 {
            total_strength / pair_count as f64
        } else {
            0.0
        },
        highly_coupled_pairs: highly_coupled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matrix::build_files_index;
    use std::collections::BTreeMap;

    fn raw(pairs: &[(&str, &[(&str, i64)])]) -> RawFiles {
        pairs
            .iter()
            .map(|(f, inner)| {
                (
                    (*f).to_string(),
                    inner
                        .iter()
                        .map(|(o, c)| ((*o).to_string(), *c))
                        .collect::<BTreeMap<_, _>>(),
                )
            })
            .collect()
    }

    #[test]
    fn sparse_coupling_respects_min_weight() {
        let files = raw(&[
            ("a.go", &[("a.go", 4), ("b.go", 2)]),
            ("b.go", &[("b.go", 3), ("a.go", 2)]),
        ]);
        let (seq, idx) = build_files_index(&files);
        let pairs = compute_sparse_coupling(&files, &seq, &idx, 2);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].co_changes, 2);
        // raise min_weight above the count -> filtered out.
        let none = compute_sparse_coupling(&files, &seq, &idx, 3);
        assert!(none.is_empty());
    }

    #[test]
    fn top_k_limits_and_sorts() {
        let pairs = vec![
            FileCouplingData {
                file1: "a".into(),
                file2: "b".into(),
                co_changes: 1,
                strength: 0.0,
            },
            FileCouplingData {
                file1: "c".into(),
                file2: "d".into(),
                co_changes: 5,
                strength: 0.0,
            },
            FileCouplingData {
                file1: "e".into(),
                file2: "f".into(),
                co_changes: 3,
                strength: 0.0,
            },
        ];
        let top = top_k_file_coupling(pairs, 2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].co_changes, 5);
        assert_eq!(top[1].co_changes, 3);
    }

    #[test]
    fn sparse_aggregate_matches_dense() {
        let files = raw(&[
            ("a.go", &[("a.go", 4), ("b.go", 2)]),
            ("b.go", &[("b.go", 2), ("a.go", 2)]),
        ]);
        let (seq, idx) = build_files_index(&files);
        let agg = compute_sparse_aggregate(&files, &seq, &idx, &["Alice|a".into()]);
        assert_eq!(agg.total_files, 2);
        assert_eq!(agg.total_developers, 1);
        assert_eq!(agg.total_co_changes, 2);
        assert_eq!(agg.highly_coupled_pairs, 0);
        assert!((agg.avg_coupling_strength - 2.0 / 3.0).abs() < 1e-12);
    }
}
