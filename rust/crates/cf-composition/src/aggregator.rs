//! Composition aggregator.
//!
//! Ported from Go `internal/analyzers/composition/aggregator.go`. Accumulates
//! per-file classification results into a breakdown / percentages / total-files
//! report.

use std::collections::HashMap;

use cf_gojson::{GoMap, GoValue, MapOrigin};

use crate::category::{CategoryCounts, ALL_CATEGORIES};

/// Report key for the per-category counts map.
pub const KEY_BREAKDOWN: &str = "breakdown";
/// Report key for the per-category percentages map.
pub const KEY_PERCENTAGE: &str = "percentages";
/// Report key for the total file count.
pub const KEY_TOTAL_FILES: &str = "total_files";
/// Report key holding the per-file category string.
pub const KEY_CATEGORY: &str = "category";

/// 100.0, used to convert a fraction to a percentage. Mirrors Go
/// `percentMultiplier`.
const PERCENT_MULTIPLIER: f64 = 100.0;

/// Aggregates file composition results across multiple files.
///
/// Mirrors Go `Aggregator`. The Go type embeds `common.PerFileRetainer` (which
/// retains per-file reports for later HTML rendering); that retention is not
/// observable in the machine-format report this crate is responsible for, so it
/// is intentionally omitted here and noted in the crate `todos`.
#[derive(Debug, Default)]
pub struct Aggregator {
    counts: CategoryCounts,
    total_files: i64,
}

impl Aggregator {
    /// Creates a new composition aggregator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Accumulates per-file classification results.
    ///
    /// `results` mirrors Go's `map[string]analyze.Report`: each value is a
    /// single-file report whose `category` key holds the classification string.
    /// Every report increments `total_files`; a present-and-string `category`
    /// additionally increments the per-category counter. A missing or
    /// non-string `category` is skipped (the file is still counted) — matching
    /// Go's `cat, ok := report[keyCategory].(string); if !ok { continue }`.
    pub fn aggregate(&mut self, results: &HashMap<String, HashMap<String, GoValue>>) {
        for report in results.values() {
            self.total_files += 1;

            if let Some(GoValue::Str(cat)) = report.get(KEY_CATEGORY) {
                self.counts.increment(cat);
            }
        }
    }

    /// Convenience entry point used in tests: aggregate a single category by
    /// its wire string.
    pub fn aggregate_category(&mut self, category: &str) {
        self.total_files += 1;
        self.counts.increment(category);
    }

    /// Builds the aggregated composition report.
    ///
    /// The returned [`GoMap`] is a *map-origin* container, so `cf-gojson` sorts
    /// its keys by raw UTF-8 bytes at encode time — exactly as Go encodes a
    /// `map[string]any` report (`breakdown`, `percentages`, `total_files`).
    /// The nested `breakdown` / `percentages` maps are likewise map-origin and
    /// byte-sorted.
    ///
    /// Percentages are only emitted when `total_files > 0`, matching Go (an
    /// empty repo yields an empty `percentages` map).
    #[must_use]
    pub fn get_result(&self) -> GoMap {
        // All three containers mirror Go maps (`map[string]int`,
        // `map[string]float64`, `map[string]any`), so they are map-origin and
        // byte-sort their keys at encode time.
        let mut breakdown = GoMap::new(MapOrigin::Map);
        let mut percentages = GoMap::new(MapOrigin::Map);

        for cat in ALL_CATEGORIES {
            let count = self.counts.get(cat);
            breakdown.push(cat.as_str(), GoValue::Int(count));

            if self.total_files > 0 {
                let pct = (count as f64) / (self.total_files as f64) * PERCENT_MULTIPLIER;
                percentages.push(cat.as_str(), GoValue::Float(pct));
            }
        }

        let mut report = GoMap::new(MapOrigin::Map);
        report.push(KEY_BREAKDOWN, GoValue::Map(breakdown));
        report.push(KEY_PERCENTAGE, GoValue::Map(percentages));
        report.push(KEY_TOTAL_FILES, GoValue::Int(self.total_files));
        report
    }
}
