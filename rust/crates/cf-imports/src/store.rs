//! Store records for the history analyzer.
//!
//! Port of `internal/analyzers/imports/store_reader.go` and `store_writer.go`.
//! The history analyzer persists pre-computed per-import usage as a stream of
//! [`ImportUsageRecord`]s (kind [`KIND_IMPORT_USAGE`]), aggregated across all
//! authors/languages/ticks and ordered by count descending, so plots can be
//! rebuilt without recomputing metrics.
//!
//! The Go code streams these through the framework's `ReportWriter`/`ReportReader`
//! (file-backed store). Per DESIGN §3, persisted state uses a Rust-native codec
//! (the gob path is dropped) — the on-disk store belongs to the not-yet-ported
//! `cf-persist`/`cf-analyze` crates. This module ports the record type and the
//! pure transform ([`compute_usage_records`]) that the writer applies; the
//! actual store I/O is wired in once those crates exist (see crate todos).

use crate::history::{aggregate_import_counts, top_imports, ImportsMap};

/// Store record kind for per-import usage. Mirrors Go `KindImportUsage`.
pub const KIND_IMPORT_USAGE: &str = "import_usage";

/// Pre-computed import usage count for a single import path.
///
/// Mirrors Go `ImportUsageRecord`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportUsageRecord {
    /// The import path.
    pub import: String,
    /// Total usage count.
    pub count: i64,
}

/// Computes the ordered import-usage records the writer would persist.
///
/// Mirrors the pure core of Go `(*HistoryAnalyzer).WriteToStore`: merge all
/// ticks' maps, aggregate per-import counts, take the top imports (count desc),
/// and emit one record per import in that order.
pub fn compute_usage_records(tick_maps: &[ImportsMap]) -> Vec<ImportUsageRecord> {
    let mut merged: ImportsMap = ImportsMap::new();
    for m in tick_maps {
        crate::history::merge_import_maps(&mut merged, m);
    }
    let counts = aggregate_import_counts(&merged);
    let (labels, data) = top_imports(&counts);
    labels
        .into_iter()
        .zip(data)
        .map(|(import, count)| ImportUsageRecord { import, count })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn mk(pairs: &[(i64, &str, &str, i64, i64)]) -> ImportsMap {
        let mut m: ImportsMap = BTreeMap::new();
        for (author, lang, imp, tick, count) in pairs {
            m.entry(*author)
                .or_default()
                .entry(lang.to_string())
                .or_default()
                .entry(imp.to_string())
                .or_default()
                .insert(*tick, *count);
        }
        m
    }

    // Mirrors TestWriteToStore_RoundTrip (the pure-transform portion): fmt has
    // the highest count (3+5+2=10) and must be first; os totals 5.
    #[test]
    fn test_compute_usage_records_round_trip() {
        let tick0 = mk(&[
            (0, "go", "fmt", 0, 3),
            (0, "go", "os", 0, 2),
            (0, "go", "strings", 0, 1),
        ]);
        let tick1 = mk(&[
            (0, "go", "fmt", 1, 5),
            (0, "go", "io", 1, 1),
            (1, "go", "fmt", 1, 2),
            (1, "go", "os", 1, 3),
        ]);
        let records = compute_usage_records(&[tick0, tick1]);
        assert!(!records.is_empty());
        assert_eq!(records[0].import, "fmt");
        assert_eq!(records[0].count, 10);

        let os = records.iter().find(|r| r.import == "os").expect("os record");
        assert_eq!(os.count, 5);
    }

    // Mirrors TestWriteToStore_EmptyTicks.
    #[test]
    fn test_compute_usage_records_empty() {
        assert!(compute_usage_records(&[]).is_empty());
    }
}
