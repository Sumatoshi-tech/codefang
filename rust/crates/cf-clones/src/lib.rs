//! `cf-clones` — static code-clone detection (MinHash + LSH).
//!
//! Port of the Go package `internal/analyzers/clones` (analyzer id
//! `static/clones`). It detects duplicate and near-duplicate functions in a UAST
//! by compressing each function's structural shingle set into a 128-function
//! MinHash signature, indexing the signatures in a 16×8 LSH index, and reporting
//! the candidate pairs whose estimated Jaccard similarity clears the Type-3
//! threshold.
//!
//! The Go package is split across `analyzer.go`, `shingler.go`, `aggregator.go`,
//! `visitor.go`, `report.go`, and `report_section.go` (plus the non-binding
//! `plot.go`); this crate mirrors that split across the [`shingler`],
//! [`engine`], [`analyzer`], [`aggregator`], [`visitor`], [`report`], and
//! [`report_section`] modules.
//!
//! # Byte-identity
//!
//! The machine export path ([`analyzer::Analyzer::format_report_json`] /
//! `_yaml` / `_binary`) does **not** serialize the raw `analyze::Report` map.
//! Mirroring Go's `FormatReportJSON`/`YAML`/`Binary`, it first projects the
//! report into [`report::ComputedMetrics`] (a *struct*: fields emit in
//! declaration order, `clone_type_distribution` honors `omitempty`) and routes
//! the resulting [`cf_gojson::GoValue`] tree through [`cf_gojson`] (JSON) and
//! [`cf_reportutil`] (the CFB1 `bin` envelope). The intermediate
//! `analyze::Report` is a map-origin [`cf_gojson::GoMap`] whose keys byte-sort on
//! encode, matching Go's `map[string]any`. Per
//! `specs/rust-rewrite/DESIGN.md` §2 nothing here uses `serde_json`.
//!
//! The numeric core — the `similarity` floats that reach the report — is
//! produced by [`cf_alg_minhash`] / [`cf_alg_lsh`], which are bit-identical ports
//! of `pkg/alg/minhash` and `pkg/alg/lsh` (FNV-1a + SplitMix64-seeded mixing,
//! verified against Go goldens). So the detection results match Go for the same
//! UAST input.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

pub mod aggregator;
pub mod analyzer;
pub mod engine;
pub mod report;
pub mod report_section;
pub mod shingler;
pub mod uast;
pub mod visitor;

pub use aggregator::Aggregator;
pub use analyzer::Analyzer;
pub use report::{
    classify_clone_type, ClonePair, ComputedMetrics, CLONE_TYPE1, CLONE_TYPE2, CLONE_TYPE3,
    DEFAULT_MAX_CLONE_PAIRS,
};
pub use report_section::ReportSection;
pub use shingler::Shingler;
pub use visitor::Visitor;

/// Number of MinHash hash functions per signature. Mirrors Go `numHashes`.
pub const NUM_HASHES: usize = 128;

/// Number of LSH bands. Mirrors Go `numBands`.
pub const NUM_BANDS: usize = 16;

/// Number of rows per LSH band. Mirrors Go `numRows`.
pub const NUM_ROWS: usize = 8;

/// Minimum AST nodes a function must have to be considered. Mirrors Go
/// `minFunctionNodes` — trivial getters/setters below this threshold hash
/// identically regardless of purpose and produce false positives.
pub const MIN_FUNCTION_NODES: usize = 20;

/// The registered short analyzer name. Mirrors Go `analyzerName`.
pub const ANALYZER_NAME: &str = "clones";

/// The CLI flag for the analyzer. Mirrors Go `analyzerFlag`.
pub const ANALYZER_FLAG: &str = "clone-detection";

/// Human-readable analyzer description. Mirrors Go `analyzerDescription`.
pub const ANALYZER_DESCRIPTION: &str =
    "Detects duplicate and near-duplicate code using MinHash and LSH.";

/// The full analyzer ID for registration. Mirrors Go `analyzerID`.
pub const ANALYZER_ID: &str = "static/clones";

// --- Threshold constants for the `thresholds()` method (Go analyzer.go) ---

/// `clone_ratio` yellow threshold. Mirrors Go `thresholdCloneRatioYellow`.
pub const THRESHOLD_CLONE_RATIO_YELLOW: f64 = 0.1;
/// `clone_ratio` red threshold. Mirrors Go `thresholdCloneRatioRed`.
pub const THRESHOLD_CLONE_RATIO_RED: f64 = 0.3;
/// `total_clone_pairs` yellow threshold. Mirrors Go `thresholdClonePairsYellow`.
pub const THRESHOLD_CLONE_PAIRS_YELLOW: i64 = 5;
/// `total_clone_pairs` red threshold. Mirrors Go `thresholdClonePairsRed`.
pub const THRESHOLD_CLONE_PAIRS_RED: i64 = 20;

// --- Message constants (Go analyzer.go) ---

/// Mirrors Go `msgNoClones`.
pub const MSG_NO_CLONES: &str = "No code clones detected";
/// Mirrors Go `msgLowClones`.
pub const MSG_LOW_CLONES: &str = "Low duplication - few clone pairs detected";
/// Mirrors Go `msgModClones`.
pub const MSG_MOD_CLONES: &str = "Moderate duplication - consider refactoring clone pairs";
/// Mirrors Go `msgHighClones`.
pub const MSG_HIGH_CLONES: &str = "High duplication - significant refactoring recommended";
/// Mirrors Go `msgNoFunctions`.
pub const MSG_NO_FUNCTIONS: &str = "No functions found for clone analysis";
/// Mirrors Go `msgEmptyAST`.
pub const MSG_EMPTY_AST: &str = "No AST provided";

/// Pair-count boundary for the "low duplication" message. Mirrors Go
/// `pairCountLow`.
pub const PAIR_COUNT_LOW: usize = 5;
/// Pair-count boundary for the "moderate duplication" message. Mirrors Go
/// `pairCountMod`.
pub const PAIR_COUNT_MOD: usize = 15;

// --- Report keys (Go report.go) ---

/// Report key `analyzer_name`.
pub const KEY_ANALYZER_NAME: &str = "analyzer_name";
/// Report key `total_clone_pairs`.
pub const KEY_TOTAL_CLONE_PAIRS: &str = "total_clone_pairs";
/// Report key `clone_pairs`.
pub const KEY_CLONE_PAIRS: &str = "clone_pairs";
/// Report key `total_functions`.
pub const KEY_TOTAL_FUNCTIONS: &str = "total_functions";
/// Report key `message`.
pub const KEY_MESSAGE: &str = "message";
/// Report key `clone_ratio`.
pub const KEY_CLONE_RATIO: &str = "clone_ratio";
/// Report key `_func_signatures` (internal; consumed by the aggregator).
pub const KEY_FUNC_SIGNATURES: &str = "_func_signatures";
/// Report key `clone_type_distribution`.
pub const KEY_CLONE_TYPE_DISTRIBUTION: &str = "clone_type_distribution";

/// Returns the human-readable message for a given clone-pair count.
///
/// Mirrors Go `cloneMessage`.
#[must_use]
pub fn clone_message(pair_count: usize) -> &'static str {
    if pair_count == 0 {
        MSG_NO_CLONES
    } else if pair_count <= PAIR_COUNT_LOW {
        MSG_LOW_CLONES
    } else if pair_count <= PAIR_COUNT_MOD {
        MSG_MOD_CLONES
    } else {
        MSG_HIGH_CLONES
    }
}

/// Computes the fraction of functions involved in at least one clone pair.
///
/// Mirrors Go `computeCloneRatio`: returns `0.0` when either argument is `0`,
/// else `cloned_funcs / total_functions`.
#[must_use]
pub fn compute_clone_ratio(cloned_funcs: usize, total_functions: usize) -> f64 {
    if total_functions == 0 || cloned_funcs == 0 {
        return 0.0;
    }
    cloned_funcs as f64 / total_functions as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_constants_match_go() {
        assert_eq!(ANALYZER_ID, "static/clones");
        assert_eq!(ANALYZER_NAME, "clones");
        assert_eq!(ANALYZER_FLAG, "clone-detection");
        assert_eq!(NUM_HASHES, 128);
        assert_eq!(NUM_BANDS, 16);
        assert_eq!(NUM_ROWS, 8);
        assert_eq!(NUM_BANDS * NUM_ROWS, NUM_HASHES);
        assert_eq!(MIN_FUNCTION_NODES, 20);
    }

    #[test]
    fn clone_message_thresholds() {
        assert_eq!(clone_message(0), MSG_NO_CLONES);
        assert_eq!(clone_message(1), MSG_LOW_CLONES);
        assert_eq!(clone_message(5), MSG_LOW_CLONES);
        assert_eq!(clone_message(6), MSG_MOD_CLONES);
        assert_eq!(clone_message(15), MSG_MOD_CLONES);
        assert_eq!(clone_message(16), MSG_HIGH_CLONES);
    }

    #[test]
    fn clone_ratio_zero_when_either_is_zero() {
        assert_eq!(compute_clone_ratio(0, 10), 0.0);
        assert_eq!(compute_clone_ratio(5, 0), 0.0);
        assert!((compute_clone_ratio(1, 4) - 0.25).abs() < 1e-12);
    }
}
