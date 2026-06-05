//! The composite quality analyzer (Go `quality.Analyzer`).
//!
//! Runs the four static component analyzers — complexity, Halstead, comments,
//! cohesion — on each changed file's UAST per commit, recording **scalars only**,
//! and aggregates them order-independently (per-commit results keyed by hash;
//! `Merge` is a no-op). Analyzer ID is `history/quality`.
//!
//! # Component coupling (DESIGN rule 5)
//!
//! The component analyzers (`cf-complexity`, `cf-halstead`, `cf-comments`,
//! `cf-cohesion`) each expose `Analyzer::new()` + `analyze(&root) -> Result<Report, _>`
//! returning a per-crate report map. This module consumes only the documented
//! scalar keys via the [`ScalarReport`] accessor trait, so it is decoupled from
//! each crate's concrete `Report`/`ReportValue` representation. The framework
//! plumbing (`UASTChangesAnalyzer`, `TicksSinceStart`, `BaseHistoryAnalyzer`,
//! `Aggregator`) is abstracted behind [`ComponentSet`] until the exact
//! `cf-analyze` / `cf-plumbing` surfaces are wired in.

use std::collections::BTreeMap;

use crate::data::TickQuality;

/// Analyzer identifier (Go `Descriptor.ID`).
pub const ID: &str = "history/quality";

/// Analyzer description (Go `Descriptor.Description`).
pub const DESCRIPTION: &str =
    "Tracks complexity, Halstead, comment quality, and cohesion metrics over commit history.";

/// Estimated bytes of TC payload per commit (Go `qualityAvgTCSize`).
pub const ESTIMATED_TC_SIZE: usize = 2 * 1024;

/// Read-side accessor over a component analyzer's report map.
///
/// Mirrors Go `reportutil.GetInt` / `reportutil.GetFloat64`: a missing or
/// wrong-typed key yields `0`. Each component crate's report type implements
/// this (or is adapted to it) so the quality analyzer can pull scalars uniformly.
pub trait ScalarReport {
    /// Returns the integer value at `key`, truncating floats toward zero
    /// (`reportutil.GetInt`); `0` when absent or non-numeric.
    fn get_int(&self, key: &str) -> i64;
    /// Returns the float value at `key` (`reportutil.GetFloat64`); `0.0` when
    /// absent or non-numeric.
    fn get_float(&self, key: &str) -> f64;
}

/// The four component analyzers, run per changed file (Go static analyzers held
/// on `quality.Analyzer`).
///
/// Implementors invoke each component's `analyze` and return its report (or
/// `None` on error, matching the Go code which silently skips a component whose
/// `Analyze` errored). The default [`accumulate_file`] glue records the same
/// scalars the Go `analyzeComplexity/Halstead/Comments/Cohesion` helpers do.
pub trait ComponentSet {
    /// The complexity report type (e.g. `cf_complexity::Report`).
    type Complexity: ScalarReport;
    /// The Halstead report type.
    type Halstead: ScalarReport;
    /// The comments report type.
    type Comments: ScalarReport;
    /// The cohesion report type.
    type Cohesion: ScalarReport;
    /// The UAST node type fed to each component analyzer.
    type Node;

    /// Runs the complexity analyzer; `None` mirrors a Go error return.
    fn analyze_complexity(&self, root: &Self::Node) -> Option<Self::Complexity>;
    /// Runs the Halstead analyzer.
    fn analyze_halstead(&self, root: &Self::Node) -> Option<Self::Halstead>;
    /// Runs the comments analyzer.
    fn analyze_comments(&self, root: &Self::Node) -> Option<Self::Comments>;
    /// Runs the cohesion analyzer.
    fn analyze_cohesion(&self, root: &Self::Node) -> Option<Self::Cohesion>;
}

/// Accumulates one file's scalars into `tq` (Go `(*Analyzer).analyzeNode`).
///
/// Order of the four component calls matches Go
/// (`complexity, halstead, comments, cohesion`). A component that errored
/// (`None`) contributes nothing — exactly like the Go helpers' early `return`.
pub fn accumulate_file<C: ComponentSet>(components: &C, root: &C::Node, tq: &mut TickQuality) {
    if let Some(r) = components.analyze_complexity(root) {
        tq.complexities
            .push(r.get_int("total_complexity") as f64);
        tq.cognitives
            .push(r.get_int("cognitive_complexity") as f64);
        tq.max_complexities.push(r.get_int("max_complexity"));
        tq.functions.push(r.get_int("total_functions"));
    }
    if let Some(r) = components.analyze_halstead(root) {
        tq.halstead_volumes.push(r.get_float("volume"));
        tq.halstead_efforts.push(r.get_float("effort"));
        tq.delivered_bugs.push(r.get_float("delivered_bugs"));
    }
    if let Some(r) = components.analyze_comments(root) {
        tq.comment_scores.push(r.get_float("overall_score"));
        tq.doc_coverages
            .push(r.get_float("documentation_coverage"));
    }
    if let Some(r) = components.analyze_cohesion(root) {
        tq.cohesion_scores.push(r.get_float("cohesion_score"));
    }
}

/// Builds the per-commit [`TickQuality`] for one commit's changed UAST roots
/// (Go `(*Analyzer).Consume`).
///
/// `roots` are the `change.After` nodes (callers skip deletions where
/// `After == nil`). Each root is analyzed by all four components.
pub fn consume_commit<C: ComponentSet>(components: &C, roots: &[&C::Node]) -> TickQuality {
    let mut tq = TickQuality::new();
    for root in roots {
        accumulate_file(components, root, &mut tq);
    }
    tq
}

/// Folds per-commit results into the canonical `commit_quality` map
/// (the order-independent core of the framework aggregator).
///
/// Later writes for the same hash overwrite earlier ones, matching Go's
/// `acc.commitQuality[hash] = tq` and `maps.Copy`.
#[must_use]
pub fn fold_commits<I>(per_commit: I) -> BTreeMap<String, TickQuality>
where
    I: IntoIterator<Item = (String, TickQuality)>,
{
    let mut out = BTreeMap::new();
    for (hash, tq) in per_commit {
        out.insert(hash, tq);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // A minimal in-memory ScalarReport for testing accumulate/consume.
    struct MapReport(BTreeMap<&'static str, f64>);
    impl ScalarReport for MapReport {
        fn get_int(&self, key: &str) -> i64 {
            self.0.get(key).copied().unwrap_or(0.0) as i64
        }
        fn get_float(&self, key: &str) -> f64 {
            self.0.get(key).copied().unwrap_or(0.0)
        }
    }

    struct FakeComponents {
        ok: bool,
    }
    impl ComponentSet for FakeComponents {
        type Complexity = MapReport;
        type Halstead = MapReport;
        type Comments = MapReport;
        type Cohesion = MapReport;
        type Node = ();

        fn analyze_complexity(&self, _: &()) -> Option<MapReport> {
            self.ok.then(|| {
                MapReport(BTreeMap::from([
                    ("total_complexity", 7.0),
                    ("cognitive_complexity", 3.0),
                    ("max_complexity", 5.0),
                    ("total_functions", 2.0),
                ]))
            })
        }
        fn analyze_halstead(&self, _: &()) -> Option<MapReport> {
            self.ok.then(|| {
                MapReport(BTreeMap::from([
                    ("volume", 100.0),
                    ("effort", 50.0),
                    ("delivered_bugs", 0.03),
                ]))
            })
        }
        fn analyze_comments(&self, _: &()) -> Option<MapReport> {
            self.ok.then(|| {
                MapReport(BTreeMap::from([
                    ("overall_score", 0.8),
                    ("documentation_coverage", 0.6),
                ]))
            })
        }
        fn analyze_cohesion(&self, _: &()) -> Option<MapReport> {
            self.ok
                .then(|| MapReport(BTreeMap::from([("cohesion_score", 0.9)])))
        }
    }

    // Analog of TestAnalyzer_Consume_ReturnsTCWithTickQuality / MultipleFiles.
    #[test]
    fn consume_records_one_sample_per_file() {
        let c = FakeComponents { ok: true };
        let unit = ();
        let tq = consume_commit(&c, &[&unit, &unit]);
        assert_eq!(tq.files_analyzed(), 2);
        assert_eq!(tq.complexities, vec![7.0, 7.0]);
        assert_eq!(tq.max_complexities, vec![5, 5]);
        assert_eq!(tq.functions, vec![2, 2]);
        assert_eq!(tq.delivered_bugs, vec![0.03, 0.03]);
        assert_eq!(tq.cohesion_scores, vec![0.9, 0.9]);
    }

    // Analog of TestAnalyzer_Consume_EmptyChanges.
    #[test]
    fn consume_empty_yields_zero_files() {
        let c = FakeComponents { ok: true };
        let tq = consume_commit(&c, &[]);
        assert_eq!(tq.files_analyzed(), 0);
    }

    // A component that errors contributes nothing (Go early-return).
    #[test]
    fn consume_skips_errored_components() {
        let c = FakeComponents { ok: false };
        let unit = ();
        let tq = consume_commit(&c, &[&unit]);
        assert_eq!(tq.files_analyzed(), 0);
        assert!(tq.cohesion_scores.is_empty());
    }

    #[test]
    fn fold_overwrites_duplicate_hash() {
        let a = TickQuality {
            complexities: vec![1.0],
            ..TickQuality::default()
        };
        let b = TickQuality {
            complexities: vec![2.0, 3.0],
            ..TickQuality::default()
        };
        let out = fold_commits([("h".to_string(), a), ("h".to_string(), b)]);
        assert_eq!(out["h"].complexities, vec![2.0, 3.0]);
    }
}
