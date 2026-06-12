//! `cf-typos` — the codefang `history/typos` analyzer.
//!
//! Detects typo-fix pairs across commit history and computes the typos metric
//! set. The metric computation and report-contract serialization ([`metrics`],
//! [`compat`], [`typos`], [`levenshtein`]) live and are tested here; they back
//! the `run/history_typos.json` binding golden (output bytes are pinned
//! against the reference binary by `rust/tests/compat`). The
//! streaming-pipeline glue (consume/serialize/store/UAST traversal) lives in
//! the consumer: `cf-commands` `handlers/history.rs` drives these modules and
//! `cf-uast-node` directly.

pub mod compat;
pub mod levenshtein;
pub mod metrics;
pub mod typos;

pub use compat::{GoValue, Hash};
pub use metrics::{metrics_report_value, metrics_yaml_value, ReportData};
pub use typos::Typo;

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-typos";

/// Serializes the `history/typos` report for the given typos as compact JSON
/// bytes (no trailing newline), matching the `--format json` output of
/// `codefang run --analyzers history/typos`.
///
/// This is the exact serialization the `run/history_typos.json` golden
/// captures: the metric-set map (byte-sorted keys) marshaled compactly.
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
