//! Default merge-based aggregator.
//!
//! `GenericAggregator` is
//! the default [`Aggregator`] backed by a user-supplied report merge function.

use cf_gojson::GoValue;

use crate::error::AnalyzeError;
use crate::interfaces::{Aggregator, Tc};
use crate::report::{new_report, Report};

/// Combines two reports in place: `into` accumulates `from`.
pub type ReportMergeFunc = Box<dyn Fn(&mut Report, &Report) + Send + Sync>;

/// The default aggregator implementation backed by a merge function.
pub struct GenericAggregator {
    report: Report,
    merge: ReportMergeFunc,
}

impl GenericAggregator {
    /// Creates an aggregator with the provided merge function.
    #[must_use]
    pub fn new(merge: ReportMergeFunc) -> Self {
        Self {
            report: new_report(),
            merge,
        }
    }
}

impl Aggregator for GenericAggregator {
    /// Merges a TC's data into the running report. A nil/empty TC is a no-op.
    fn consume(&mut self, tc: &Tc) -> Result<(), AnalyzeError> {
        if tc.data.is_empty() {
            return Ok(());
        }
        // The merge function receives the TC's data as the same map view.
        let from = tc.data.clone();
        (self.merge)(&mut self.report, &from);
        Ok(())
    }

    /// Returns the merged report.
    fn finalize(&mut self) -> Result<Report, AnalyzeError> {
        Ok(self.report.clone())
    }
}

/// A simple "last write wins" merge: every key in `from` overwrites `into`.
/// Convenience helper for callers that do not need numeric summation.
pub fn overwrite_merge(into: &mut Report, from: &Report) {
    for (k, v) in from.entries() {
        upsert(into, k, v.clone());
    }
}

/// Inserts or replaces `key` in `report` (map-origin, so order is irrelevant).
fn upsert(report: &mut Report, key: &str, value: GoValue) {
    // GoMap has no remove; rebuild without the key, then push. Map-origin
    // objects sort on encode, so we only need set-semantics here.
    let mut rebuilt = new_report();
    for (k, v) in report.entries() {
        if k != key {
            rebuilt.push(k.clone(), v.clone());
        }
    }
    rebuilt.push(key.to_string(), value);
    *report = rebuilt;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consume_empty_is_noop() {
        let mut agg = GenericAggregator::new(Box::new(overwrite_merge));
        agg.consume(&Tc::new()).unwrap();
        assert!(agg.finalize().unwrap().is_empty());
    }

    #[test]
    fn consume_merges_data() {
        let mut agg = GenericAggregator::new(Box::new(overwrite_merge));
        let mut tc = Tc::new();
        tc.data.push("x", GoValue::Int(1));
        agg.consume(&tc).unwrap();
        let mut tc2 = Tc::new();
        tc2.data.push("y", GoValue::Int(2));
        agg.consume(&tc2).unwrap();
        let result = agg.finalize().unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn overwrite_replaces_existing_key() {
        let mut into = new_report();
        into.push("x", GoValue::Int(1));
        let mut from = new_report();
        from.push("x", GoValue::Int(9));
        overwrite_merge(&mut into, &from);
        assert_eq!(into.len(), 1);
        let out = cf_gojson::marshal(&GoValue::Object(into));
        assert_eq!(out, br#"{"x":9}"#);
    }
}
