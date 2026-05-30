//! Standard report builders (`result_builder.go`).
//!
//! Produces the conventional report shapes (empty, basic, detailed, collection,
//! metric) used across analyzers. Each builder returns a [`Report`] with the
//! canonical `analyzer_name` / `total_items` / `message` keys plus any merged
//! custom fields.

use crate::report::{Item, Report, Value};

/// Stateless builder of standard analyzer result reports.
///
/// Mirrors `common.ResultBuilder`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ResultBuilder;

impl ResultBuilder {
    /// Creates a new builder. Mirrors `common.NewResultBuilder`.
    #[must_use]
    pub fn new() -> Self {
        ResultBuilder
    }

    /// Builds the standard empty result for when no data is found.
    ///
    /// Mirrors `common.ResultBuilder.BuildEmptyResult`:
    /// `{analyzer_name, total_items: 0, message: "No data found"}`.
    #[must_use]
    pub fn build_empty_result(&self, analyzer_name: &str) -> Report {
        let mut r = Report::new();
        r.insert("analyzer_name".into(), Value::Str(analyzer_name.into()));
        r.insert("total_items".into(), Value::Int(0));
        r.insert("message".into(), Value::Str("No data found".into()));
        r
    }

    /// Builds an empty result containing only the given custom fields.
    ///
    /// Mirrors `common.ResultBuilder.BuildCustomEmptyResult`.
    #[must_use]
    pub fn build_custom_empty_result(&self, fields: Item) -> Report {
        let mut r = Report::new();
        r.extend(fields);
        r
    }

    /// Builds a basic result with the canonical name/count/message fields.
    ///
    /// Mirrors `common.ResultBuilder.BuildBasicResult`.
    #[must_use]
    pub fn build_basic_result(&self, analyzer_name: &str, total_items: i64, message: &str) -> Report {
        let mut r = Report::new();
        r.insert("analyzer_name".into(), Value::Str(analyzer_name.into()));
        r.insert("total_items".into(), Value::Int(total_items));
        r.insert("message".into(), Value::Str(message.into()));
        r
    }

    /// Builds a detailed result: `analyzer_name` plus merged custom fields.
    ///
    /// Mirrors `common.ResultBuilder.BuildDetailedResult`. Custom fields
    /// overwrite the seeded `analyzer_name` if they collide (matching Go's
    /// `maps.Copy` semantics).
    #[must_use]
    pub fn build_detailed_result(&self, analyzer_name: &str, fields: Item) -> Report {
        let mut r = Report::new();
        r.insert("analyzer_name".into(), Value::Str(analyzer_name.into()));
        r.extend(fields);
        r
    }

    /// Builds a result carrying a collection of items.
    ///
    /// Mirrors `common.ResultBuilder.BuildCollectionResult`:
    /// `{analyzer_name, total_<collection_key>: len(items), <collection_key>:
    /// items, message}` plus merged metrics. Metrics overwrite any colliding
    /// seeded keys.
    #[must_use]
    pub fn build_collection_result(
        &self,
        analyzer_name: &str,
        collection_key: &str,
        items: Vec<Item>,
        metrics: Item,
        message: &str,
    ) -> Report {
        let mut r = Report::new();
        r.insert("analyzer_name".into(), Value::Str(analyzer_name.into()));
        r.insert(format!("total_{collection_key}"), Value::Int(items.len() as i64));
        r.insert(collection_key.into(), Value::Collection(items));
        r.insert("message".into(), Value::Str(message.into()));
        r.extend(metrics);
        r
    }

    /// Builds a metric-focused result: `analyzer_name`, `message`, and merged
    /// metrics.
    ///
    /// Mirrors `common.ResultBuilder.BuildMetricResult`.
    #[must_use]
    pub fn build_metric_result(&self, analyzer_name: &str, metrics: Item, message: &str) -> Report {
        let mut r = Report::new();
        r.insert("analyzer_name".into(), Value::Str(analyzer_name.into()));
        r.insert("message".into(), Value::Str(message.into()));
        r.extend(metrics);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(pairs: &[(&str, Value)]) -> Item {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }

    #[test]
    fn build_empty_result() {
        let r = ResultBuilder::new().build_empty_result("test");
        assert_eq!(r.get("analyzer_name"), Some(&Value::Str("test".into())));
        assert_eq!(r.get("total_items"), Some(&Value::Int(0)));
        assert_eq!(r.get("message"), Some(&Value::Str("No data found".into())));
    }

    #[test]
    fn build_basic_result() {
        let r = ResultBuilder::new().build_basic_result("analyzer", 10, "done");
        assert_eq!(r.get("analyzer_name"), Some(&Value::Str("analyzer".into())));
        assert_eq!(r.get("total_items"), Some(&Value::Int(10)));
        assert_eq!(r.get("message"), Some(&Value::Str("done".into())));
    }

    #[test]
    fn build_collection_result() {
        let items = vec![item(&[("name", Value::Str("item1".into()))]), item(&[("name", Value::Str("item2".into()))])];
        let metrics = item(&[("avg_score", Value::Float(0.75))]);
        let r = ResultBuilder::new().build_collection_result("analyzer", "items", items, metrics, "complete");

        assert_eq!(r.get("analyzer_name"), Some(&Value::Str("analyzer".into())));
        assert_eq!(r.get("total_items"), Some(&Value::Int(2)));
        assert_eq!(r.get("avg_score"), Some(&Value::Float(0.75)));
    }

    #[test]
    fn build_detailed_result() {
        let fields = item(&[("custom_field", Value::Str("value".into())), ("count", Value::Int(42))]);
        let r = ResultBuilder::new().build_detailed_result("analyzer", fields);
        assert_eq!(r.get("analyzer_name"), Some(&Value::Str("analyzer".into())));
        assert_eq!(r.get("custom_field"), Some(&Value::Str("value".into())));
    }

    #[test]
    fn build_metric_result() {
        let metrics = item(&[("score", Value::Float(0.9))]);
        let r = ResultBuilder::new().build_metric_result("analyzer", metrics, "metrics computed");
        assert_eq!(r.get("analyzer_name"), Some(&Value::Str("analyzer".into())));
        assert_eq!(r.get("score"), Some(&Value::Float(0.9)));
    }

    #[test]
    fn build_custom_empty_result() {
        let fields = item(&[("status", Value::Str("empty".into()))]);
        let r = ResultBuilder::new().build_custom_empty_result(fields);
        assert_eq!(r.get("status"), Some(&Value::Str("empty".into())));
    }
}
