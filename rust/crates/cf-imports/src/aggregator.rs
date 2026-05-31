//! Cross-file aggregation for the static analyzer.
//!
//! Port of `internal/analyzers/imports/aggregator.go`. The [`Aggregator`]
//! combines per-file static reports into a single report holding the unique
//! import set, per-import counts, the unique-import count, and the total file
//! count.

use std::collections::BTreeMap;

use crate::report::ReportValue;

/// Aggregates import analysis results across multiple files.
///
/// Mirrors Go `Aggregator` (minus the `PerFileRetainer` mixin, which is a
/// framework concern). `all_imports` maps import path -> occurrence count.
#[derive(Debug, Default, Clone)]
pub struct Aggregator {
    /// Import path -> count across all files.
    all_imports: BTreeMap<String, i64>,
    /// Number of files aggregated.
    total_files: i64,
}

impl Aggregator {
    /// Creates a new aggregator. Mirrors Go `NewAggregator`.
    pub fn new() -> Self {
        Aggregator::default()
    }

    /// Aggregates one file's report.
    ///
    /// Mirrors the per-report body of Go `(*Aggregator).Aggregate`: increments
    /// the file count and, for each entry in the report's `imports` list,
    /// increments that import's count.
    pub fn aggregate_report(&mut self, report: &ReportValue) {
        self.total_files += 1;
        if let Some(map) = report.as_map() {
            if let Some(ReportValue::List(imports)) = map.get("imports") {
                for v in imports {
                    if let ReportValue::Str(imp) = v {
                        *self.all_imports.entry(imp.clone()).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    /// Returns the aggregated report.
    ///
    /// Mirrors Go `(*Aggregator).GetResult`, producing `imports` (the unique
    /// import keys), `import_counts`, `count` (number of unique imports), and
    /// `total_files`. Go iterates a map for `imports` (nondeterministic order);
    /// this port emits them sorted (the [`BTreeMap`] key order) for determinism.
    pub fn get_result(&self) -> ReportValue {
        let imports: Vec<ReportValue> = self
            .all_imports
            .keys()
            .map(|k| ReportValue::Str(k.clone()))
            .collect();

        let mut import_counts = ReportValue::map();
        for (imp, count) in &self.all_imports {
            import_counts.insert(imp.clone(), ReportValue::Int(*count));
        }

        let mut result = ReportValue::map();
        result.insert("imports", ReportValue::List(imports));
        result.insert("import_counts", import_counts);
        result.insert("count", ReportValue::Int(self.all_imports.len() as i64));
        result.insert("total_files", ReportValue::Int(self.total_files));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(imports: &[&str]) -> ReportValue {
        let mut r = ReportValue::map();
        r.insert(
            "imports",
            ReportValue::List(imports.iter().map(|s| ReportValue::Str(s.to_string())).collect()),
        );
        r
    }

    #[test]
    fn aggregate_counts_and_files() {
        let mut agg = Aggregator::new();
        agg.aggregate_report(&report(&["fmt", "os"]));
        agg.aggregate_report(&report(&["fmt", "io"]));
        let result = agg.get_result();
        let map = result.as_map().unwrap();
        assert_eq!(map["total_files"], ReportValue::Int(2));
        assert_eq!(map["count"], ReportValue::Int(3)); // fmt, os, io.
        let counts = map["import_counts"].as_map().unwrap();
        assert_eq!(counts["fmt"], ReportValue::Int(2));
        assert_eq!(counts["os"], ReportValue::Int(1));
    }
}
