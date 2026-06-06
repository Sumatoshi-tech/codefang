//! `cf-typos` — the codefang `history/typos` analyzer, ported to Rust.
//!
//! Port target documented in specs/rust-rewrite/DESIGN.md §1. The metric
//! computation and Go-`encoding/json` byte-compatible serialization
//! ([`metrics`], [`compat`], [`typos`], [`levenshtein`]) are wired and tested
//! here; they back the `run/history_typos.json` binding golden. The
//! streaming-pipeline glue (`analyzer`, `serialize`, `store`, `uast`) depends on
//! the broader analyzer stack and is integrated once those crates link.
#![allow(dead_code)]

pub mod compat;
pub mod levenshtein;
pub mod metrics;
pub mod typos;

pub use compat::{GoValue, Hash};
pub use metrics::{metrics_report_value, ReportData};
pub use typos::Typo;

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-typos";

/// Serializes the `history/typos` report for the given typos as compact Go
/// `json.Marshal` bytes (no trailing newline), matching the `--format json`
/// output of `codefang run --analyzers history/typos`.
///
/// This is the exact serialization the `run/history_typos.json` golden captures:
/// `common.MetricSet.ToJSON()` (a sorted map) marshaled with `json.Marshal`.
#[must_use]
pub fn report_json(typos: &[Typo]) -> String {
    let input = ReportData { typos: typos.to_vec() };
    metrics_report_value(&input).to_json()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_report_json_matches_golden() {
        assert_eq!(
            report_json(&[]),
            r#"{"aggregate":{"total_typos":0,"unique_patterns":0,"affected_files":0,"affected_commits":0},"file_typos":[],"patterns":null,"typo_list":[]}"#
        );
    }
}
