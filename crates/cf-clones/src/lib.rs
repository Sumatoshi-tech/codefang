//! `cf-clones` — static code-clone detection (analyzer id `static/clones`).
//!
//! Finds duplicate and near-duplicate functions across a whole tree of
//! source files. Per function the [`visitor`] builds a [`profile`] (the
//! pre-order node-type sequence, a *structure* fingerprint over it, a *token*
//! fingerprint over the leaf tokens, and the exact set of `k`-gram shingles)
//! plus a `MinHash` signature of that shingle set. The [`aggregator`] indexes
//! every signature in one global banded LSH index ([`NUM_BANDS`] ×
//! [`NUM_ROWS`]), and the [`engine`] turns the index's candidate pairs into
//! verified clone pairs:
//!
//! | Clone type | Rule |
//! |---|---|
//! | **Type-1** (exact) | identical structure **and** identical tokens |
//! | **Type-2** (renamed) | identical structure, different identifiers / literals |
//! | **Type-3** (near-miss) | different structure, exact shingle Jaccard ≥ [`SIMILARITY_THRESHOLD`] |
//!
//! The reported `similarity` is the exact Jaccard of the two shingle sets
//! (`1.0` for identical structure), never the `MinHash` estimate. Pairs are
//! ordered by similarity descending, then by name, so every output is
//! deterministic.
//!
//! `clone_ratio` is the share of functions that take part in at least one
//! pair (`cloned functions / total functions`, bounded to `[0, 1]`); the
//! status message and the `N/10` score are both keyed off that ratio — see
//! [`clone_message`].
//!
//! Comment nodes are ignored, functions with fewer than
//! [`MIN_FUNCTION_NODES`] nodes are counted but never paired, and closures
//! (UAST `Lambda`) are candidates like any named function. Report values are
//! built as [`cf_gojson`] trees; nothing here uses `serde_json`.
//!
//! # Example
//!
//! ```
//! use cf_clones::{Aggregator, Visitor};
//! use cf_uast::Parser;
//!
//! let src = b"package main\n\nfunc Sum(xs []int) int {\n\ttotal := 0\n\tfor _, x := range xs {\n\t\tif x > 0 {\n\t\t\ttotal += x\n\t\t} else {\n\t\t\ttotal -= x\n\t\t}\n\t}\n\treturn total\n}\n";
//! let parser = Parser::new();
//! let mut agg = Aggregator::new();
//! for file in ["a.go", "b.go"] {
//!     let root = parser.parse(file, src).expect("parses");
//!     agg.add_file(file, Visitor::new().scan(&root));
//! }
//! let report = agg.finish();
//! assert_eq!(report.total_functions, 2);
//! assert_eq!(report.total_clone_pairs, 1);
//! assert_eq!(report.pairs[0].func_a, "a.go::Sum");
//! assert_eq!(report.pairs[0].clone_type.as_str(), "Type-1");
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
pub mod engine;
pub mod profile;
pub mod report;
pub mod report_section;
pub mod shingler;
pub mod visitor;

pub use aggregator::Aggregator;
pub use engine::FuncEntry;
pub use profile::Fingerprint;
pub use report::{ClonePair, CloneReport, CloneType, CloneTypeCounts, FileSummary};
pub use shingler::Shingler;
pub use visitor::{FileScan, Visitor};

/// Number of `MinHash` hash functions per signature.
pub const NUM_HASHES: usize = 128;

/// Number of LSH bands. With [`NUM_ROWS`] rows the candidate S-curve's 50%
/// point sits at `(1/32)^(1/4) ≈ 0.42`, below the reporting threshold, so a
/// pair at Jaccard 0.5 becomes a candidate with probability ≈ 0.87 and a pair
/// at 0.64 with probability > 0.99 (see [`cf_alg_lsh::candidate_probability`]).
pub const NUM_BANDS: usize = 32;

/// Number of rows per LSH band.
pub const NUM_ROWS: usize = 4;

/// Minimum number of UAST nodes (comments excluded) a function must have to
/// become a clone candidate. Smaller functions are counted in
/// `total_functions` but never paired: trivial getters/setters below this
/// size hash identically regardless of purpose and would only produce false
/// positives.
pub const MIN_FUNCTION_NODES: usize = 20;

/// Minimum exact shingle Jaccard for a pair of structurally different
/// functions to be reported (as Type-3).
pub const SIMILARITY_THRESHOLD: f64 = 0.5;

/// Maximum number of clone pairs stored in the report detail; the pair count,
/// the clone-type distribution and the ratio stay exact. Zero means
/// unlimited.
pub const DEFAULT_MAX_CLONE_PAIRS: usize = 1000;

/// The registered short analyzer name.
pub const ANALYZER_NAME: &str = "clones";

/// The CLI flag for the analyzer.
pub const ANALYZER_FLAG: &str = "clone-detection";

/// Human-readable analyzer description.
pub const ANALYZER_DESCRIPTION: &str =
    "Detects duplicate and near-duplicate functions using MinHash and LSH.";

/// The full analyzer ID for registration.
pub const ANALYZER_ID: &str = "static/clones";

/// `clone_ratio` above which duplication is "moderate" (the yellow
/// threshold).
pub const THRESHOLD_CLONE_RATIO_YELLOW: f64 = 0.1;
/// `clone_ratio` above which duplication is "high" (the red threshold).
pub const THRESHOLD_CLONE_RATIO_RED: f64 = 0.3;

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
/// Report key `clone_type_distribution`.
pub const KEY_CLONE_TYPE_DISTRIBUTION: &str = "clone_type_distribution";

/// Returns the status message for a clone ratio: `0` is "no clones", up to
/// [`THRESHOLD_CLONE_RATIO_YELLOW`] is low, up to [`THRESHOLD_CLONE_RATIO_RED`]
/// is moderate, above that high. The score shown next to the message is
/// derived from the same ratio ([`report_section::compute_score`]), so the two
/// never disagree.
///
/// ```
/// use cf_clones::{clone_message, MSG_NO_CLONES, MSG_LOW_CLONES, MSG_MOD_CLONES, MSG_HIGH_CLONES};
/// assert_eq!(clone_message(0.0), MSG_NO_CLONES);
/// assert_eq!(clone_message(0.1), MSG_LOW_CLONES);
/// assert_eq!(clone_message(0.3), MSG_MOD_CLONES);
/// assert_eq!(clone_message(0.31), MSG_HIGH_CLONES);
/// ```
#[must_use]
pub fn clone_message(clone_ratio: f64) -> &'static str {
    if clone_ratio <= 0.0 {
        MSG_NO_CLONES
    } else if clone_ratio <= THRESHOLD_CLONE_RATIO_YELLOW {
        MSG_LOW_CLONES
    } else if clone_ratio <= THRESHOLD_CLONE_RATIO_RED {
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
        assert_eq!(NUM_BANDS * NUM_ROWS, NUM_HASHES);
        assert_eq!(MIN_FUNCTION_NODES, 20);
    }

    #[test]
    fn lsh_curve_midpoint_sits_below_the_threshold() {
        // The 50% point of the candidate S-curve must not exceed the
        // reporting threshold, otherwise reportable pairs are mostly missed.
        let midpoint = (1.0 / NUM_BANDS as f64).powf(1.0 / NUM_ROWS as f64);
        assert!(midpoint < SIMILARITY_THRESHOLD, "midpoint {midpoint}");
        assert!(
            cf_alg_lsh::candidate_probability(SIMILARITY_THRESHOLD, NUM_BANDS, NUM_ROWS) > 0.85
        );
    }

    #[test]
    fn clone_message_is_keyed_off_the_ratio() {
        assert_eq!(clone_message(0.0), MSG_NO_CLONES);
        assert_eq!(clone_message(0.05), MSG_LOW_CLONES);
        assert_eq!(clone_message(THRESHOLD_CLONE_RATIO_YELLOW), MSG_LOW_CLONES);
        assert_eq!(clone_message(0.2), MSG_MOD_CLONES);
        assert_eq!(clone_message(THRESHOLD_CLONE_RATIO_RED), MSG_MOD_CLONES);
        assert_eq!(clone_message(1.0), MSG_HIGH_CLONES);
    }

    // Exact on purpose: the zero-denominator guard returns 0.0 exactly.
    #[allow(clippy::float_cmp)]
    #[test]
    fn clone_ratio_zero_when_either_is_zero() {
        assert_eq!(compute_clone_ratio(0, 10), 0.0);
        assert_eq!(compute_clone_ratio(5, 0), 0.0);
        assert!((compute_clone_ratio(1, 4) - 0.25).abs() < 1e-12);
    }
}
