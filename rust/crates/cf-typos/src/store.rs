//! Typos store writer.
//!
//! Port of Go `internal/analyzers/typos/store_writer.go`. Extracts typos from
//! the final report, computes per-file counts and aggregate stats, and exposes
//! them as two record streams: `file_typos` (sorted desc) and `aggregate`.

use cf_gojson::GoValue;

use crate::metrics::{compute_aggregate, compute_file_typos, ReportData};

/// Store record kind: per-file typo counts (Go `KindFileTypos`).
pub const KIND_FILE_TYPOS: &str = "file_typos";
/// Store record kind: aggregate summary (Go `KindAggregate`).
pub const KIND_AGGREGATE: &str = "aggregate";

/// A store record: a kind label and its encoded value.
///
/// Mirrors the `(kind, value)` pairs the Go `WriteToStore` streams to the
/// `analyze.ReportWriter`. `file_typos` is written as a slice (one record per
/// file, via `WriteSliceKind`); `aggregate` as a single record.
#[derive(Debug, Clone, PartialEq)]
pub struct StoreRecord {
    /// Record kind.
    pub kind: String,
    /// Encoded record value (struct-origin object).
    pub value: GoValue,
}

/// Builds the store records for the given typos input.
///
/// Port of Go `Analyzer.WriteToStore`: emits one `file_typos` record per file
/// (in typo-count-descending order) followed by one `aggregate` record.
pub fn write_to_store(input: &ReportData) -> Vec<StoreRecord> {
    let mut records = Vec::new();

    for ft in compute_file_typos(input) {
        records.push(StoreRecord {
            kind: KIND_FILE_TYPOS.to_string(),
            value: GoValue::Struct(vec![
                ("file".to_string(), GoValue::Str(ft.file)),
                ("typo_count".to_string(), GoValue::Int(ft.typo_count)),
                ("fixed_typos".to_string(), GoValue::Int(ft.fixed_typos)),
            ]),
        });
    }

    records.push(StoreRecord {
        kind: KIND_AGGREGATE.to_string(),
        value: compute_aggregate(input).to_govalue(),
    });

    records
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typos::Typo;
    use cf_gitlib::Hash;

    fn typo(wrong: &str, file: &str) -> Typo {
        Typo {
            wrong: wrong.to_string(),
            correct: "x".to_string(),
            file: file.to_string(),
            commit: Hash::default(),
            line: 0,
        }
    }

    #[test]
    fn writes_file_typos_then_aggregate() {
        let input = ReportData {
            typos: vec![typo("a", "a.go"), typo("b", "a.go"), typo("c", "b.go")],
        };
        let records = write_to_store(&input);
        // 2 file_typos records + 1 aggregate.
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].kind, KIND_FILE_TYPOS);
        assert_eq!(records[1].kind, KIND_FILE_TYPOS);
        assert_eq!(records[2].kind, KIND_AGGREGATE);
    }

    #[test]
    fn empty_input_still_writes_aggregate() {
        let records = write_to_store(&ReportData::default());
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].kind, KIND_AGGREGATE);
    }
}
