//! `cf-clones` — static code-clone detection (`MinHash` + LSH; analyzer id
//! `static/clones`).
//!
//! Detects duplicate and near-duplicate functions in a UAST by compressing
//! each function's structural shingle set into a 128-function `MinHash`
//! signature, indexing the signatures in a 16×8 LSH index, and reporting the
//! candidate pairs whose estimated Jaccard similarity clears the Type-3
//! threshold. The pipeline is split across the [`shingler`], [`engine`],
//! [`analyzer`], [`aggregator`], [`visitor`], [`report`], and
//! [`report_section`] modules.
//!
//! # Byte-identity
//!
//! The machine export path ([`analyzer::Analyzer::format_report_json`] /
//! `_yaml` / `_binary`) does **not** serialize the raw `analyze::Report` map.
//! It first projects the report into [`report::ComputedMetrics`] (a *struct*:
//! fields emit in declaration order, `clone_type_distribution` honors
//! `omitempty`) and routes the resulting [`cf_gojson::GoValue`] tree through
//! [`cf_gojson`] (JSON) and [`cf_reportutil`] (the CFB1 `bin` envelope). The
//! intermediate `analyze::Report` is a map-origin [`cf_gojson::GoMap`] whose
//! keys byte-sort on encode. Per `specs/rust-rewrite/DESIGN.md` §2 nothing
//! here uses `serde_json`.
//!
//! The numeric core — the `similarity` floats that reach the report — is
//! produced by [`cf_alg_minhash`] / [`cf_alg_lsh`], whose seeds and hashing
//! (FNV-1a + SplitMix64-seeded mixing) are bit-identical to the reference
//! implementation (verified against goldens), so the detection results are
//! pinned by the differential gate in `rust/tests/compat`.
//!
//! # Example
//!
//! With no AST the analyzer returns the "No AST provided" empty report:
//!
//! ```
//! use cf_clones::Analyzer;
//! use cf_gojson::GoValue;
//!
//! let report = Analyzer::new().analyze_node(None);
//! assert_eq!(report.get("total_clone_pairs"), Some(&GoValue::Int(0)));
//! assert_eq!(
//!     report.get("message"),
//!     Some(&GoValue::Str(cf_clones::MSG_EMPTY_AST.to_string())),
//! );
//! ```

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]
// Counts (function/pair counts) are far below the i64/f64 ranges, and the
// float->int truncations in the terminal renderer are the output contract's
// exact semantics; the pedantic cast lints add noise without value here.
#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

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

/// Number of `MinHash` hash functions per signature.
pub const NUM_HASHES: usize = 128;

/// Number of LSH bands.
pub const NUM_BANDS: usize = 16;

/// Number of rows per LSH band.
pub const NUM_ROWS: usize = 8;

/// Minimum AST nodes a function must have to be considered — trivial
/// getters/setters below this threshold hash identically regardless of
/// purpose and produce false positives.
pub const MIN_FUNCTION_NODES: usize = 20;

/// The registered short analyzer name.
pub const ANALYZER_NAME: &str = "clones";

/// The CLI flag for the analyzer.
pub const ANALYZER_FLAG: &str = "clone-detection";

/// Human-readable analyzer description.
pub const ANALYZER_DESCRIPTION: &str =
    "Detects duplicate and near-duplicate code using MinHash and LSH.";

/// The full analyzer ID for registration.
pub const ANALYZER_ID: &str = "static/clones";

// --- Threshold constants for the `thresholds()` method ---

/// `clone_ratio` yellow threshold.
pub const THRESHOLD_CLONE_RATIO_YELLOW: f64 = 0.1;
/// `clone_ratio` red threshold.
pub const THRESHOLD_CLONE_RATIO_RED: f64 = 0.3;
/// `total_clone_pairs` yellow threshold.
pub const THRESHOLD_CLONE_PAIRS_YELLOW: i64 = 5;
/// `total_clone_pairs` red threshold.
pub const THRESHOLD_CLONE_PAIRS_RED: i64 = 20;

// --- Message constants (CLI contract) ---

/// "No clones" message.
pub const MSG_NO_CLONES: &str = "No code clones detected";
/// "Low duplication" message.
pub const MSG_LOW_CLONES: &str = "Low duplication - few clone pairs detected";
/// "Moderate duplication" message.
pub const MSG_MOD_CLONES: &str = "Moderate duplication - consider refactoring clone pairs";
/// "High duplication" message.
pub const MSG_HIGH_CLONES: &str = "High duplication - significant refactoring recommended";
/// "No functions" message.
pub const MSG_NO_FUNCTIONS: &str = "No functions found for clone analysis";
/// "No AST" message.
pub const MSG_EMPTY_AST: &str = "No AST provided";

/// Pair-count boundary for the "low duplication" message.
pub const PAIR_COUNT_LOW: usize = 5;
/// Pair-count boundary for the "moderate duplication" message.
pub const PAIR_COUNT_MOD: usize = 15;

// --- Report keys ---

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
/// `0` pairs is "no clones", `<= 5` is low, `<= 15` is moderate, else high:
///
/// ```
/// use cf_clones::{clone_message, MSG_NO_CLONES, MSG_LOW_CLONES, MSG_MOD_CLONES, MSG_HIGH_CLONES};
/// assert_eq!(clone_message(0), MSG_NO_CLONES);
/// assert_eq!(clone_message(5), MSG_LOW_CLONES);
/// assert_eq!(clone_message(15), MSG_MOD_CLONES);
/// assert_eq!(clone_message(16), MSG_HIGH_CLONES);
/// ```
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

/// Computes the fraction of functions involved in at least one clone pair:
/// `0.0` when either argument is `0`, else `cloned_funcs / total_functions`.
///
/// ```
/// use cf_clones::compute_clone_ratio;
/// assert_eq!(compute_clone_ratio(0, 10), 0.0);
/// assert_eq!(compute_clone_ratio(5, 0), 0.0);
/// assert!((compute_clone_ratio(1, 4) - 0.25).abs() < 1e-12);
/// ```
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
    fn descriptor_constants_match_contract() {
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
