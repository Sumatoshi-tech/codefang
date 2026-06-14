//! Result aggregation across multiple comment analyses: sum of count keys,
//! average of numeric keys.
//!
//! Detailed-table merging (the `comments`/`functions` tables) is provided by
//! `cf-analyzers-common` and is wired in by the framework layer, not here.

use std::collections::BTreeMap;

use cf_gojson::{GoMap, GoValue, MapOrigin};

/// Numeric keys that are averaged across files.
const NUMERIC_KEYS: &[&str] = &[
    "overall_score",
    "good_comments_ratio",
    "documentation_coverage",
];

/// Count keys that are summed across files.
const COUNT_KEYS: &[&str] = &[
    "total_comments",
    "good_comments",
    "bad_comments",
    "total_functions",
    "documented_functions",
    "total_comment_details",
];

/// A single file's report as a key→numeric map (the slice of a `Report` the
/// aggregator consumes).
pub type NumericReport = BTreeMap<String, f64>;

/// Aggregates comment results across files.
#[derive(Debug, Default)]
pub struct Aggregator {
    sums: BTreeMap<String, f64>,
    numeric_totals: BTreeMap<String, f64>,
    file_count: i64,
}

impl Aggregator {
    /// Creates a new aggregator.
    pub fn new() -> Self {
        Aggregator::default()
    }

    /// Aggregates a batch of per-file numeric reports.
    ///
    /// Count keys are summed; numeric keys are averaged over the number of
    /// files. Iteration order over `results` does not affect the result
    /// (sums/averages are commutative).
    pub fn aggregate(&mut self, results: &BTreeMap<String, NumericReport>) {
        for report in results.values() {
            self.file_count += 1;
            for &k in COUNT_KEYS {
                if let Some(v) = report.get(k) {
                    *self.sums.entry(k.to_string()).or_insert(0.0) += v;
                }
            }
            for &k in NUMERIC_KEYS {
                if let Some(v) = report.get(k) {
                    *self.numeric_totals.entry(k.to_string()).or_insert(0.0) += v;
                }
            }
        }
    }

    /// Returns the aggregated result as a [`GoValue`].
    ///
    /// Empty aggregation yields zeros.
    pub fn get_result(&self) -> GoValue {
        let mut m = GoMap::new(MapOrigin::Map);
        for &k in COUNT_KEYS {
            let v = self.sums.get(k).copied().unwrap_or(0.0);
            m.push(k, GoValue::Int(v as i64));
        }
        for &k in NUMERIC_KEYS {
            let total = self.numeric_totals.get(k).copied().unwrap_or(0.0);
            let avg = if self.file_count > 0 {
                total / self.file_count as f64
            } else {
                0.0
            };
            m.push(k, GoValue::Float(avg));
        }
        GoValue::Map(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(pairs: &[(&str, f64)]) -> NumericReport {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    fn get<'a>(v: &'a GoValue, key: &str) -> &'a GoValue {
        match v {
            GoValue::Map(m) => &m.entries().iter().find(|(k, _)| k == key).unwrap().1,
            other => panic!("not a map: {other:?}"),
        }
    }

    #[test]
    fn aggregate_sums_and_averages() {
        let mut agg = Aggregator::new();
        let mut results: BTreeMap<String, NumericReport> = BTreeMap::new();
        results.insert(
            "file1".to_string(),
            report(&[
                ("total_comments", 2.0),
                ("good_comments", 1.0),
                ("bad_comments", 1.0),
                ("total_functions", 3.0),
                ("documented_functions", 1.0),
                ("overall_score", 0.5),
            ]),
        );
        results.insert(
            "file2".to_string(),
            report(&[
                ("total_comments", 1.0),
                ("good_comments", 1.0),
                ("bad_comments", 0.0),
                ("total_functions", 2.0),
                ("documented_functions", 1.0),
                ("overall_score", 1.0),
            ]),
        );
        agg.aggregate(&results);

        let res = agg.get_result();
        assert_eq!(get(&res, "total_comments"), &GoValue::Int(3));
        assert_eq!(get(&res, "good_comments"), &GoValue::Int(2));
        assert_eq!(get(&res, "bad_comments"), &GoValue::Int(1));
        assert_eq!(get(&res, "total_functions"), &GoValue::Int(5));
        assert_eq!(get(&res, "documented_functions"), &GoValue::Int(2));
        match get(&res, "overall_score") {
            GoValue::Float(f) => assert!((f - 0.75).abs() < 1e-9),
            other => panic!("overall_score not float: {other:?}"),
        }
    }

    #[test]
    fn empty_result_is_zero() {
        let agg = Aggregator::new();
        let res = agg.get_result();
        assert_eq!(get(&res, "total_comments"), &GoValue::Int(0));
        assert_eq!(get(&res, "good_comments"), &GoValue::Int(0));
        assert_eq!(get(&res, "bad_comments"), &GoValue::Int(0));
        assert_eq!(get(&res, "total_functions"), &GoValue::Int(0));
        assert_eq!(get(&res, "documented_functions"), &GoValue::Int(0));
        match get(&res, "overall_score") {
            GoValue::Float(f) => assert_eq!(*f, 0.0),
            other => panic!("overall_score not float: {other:?}"),
        }
    }
}
