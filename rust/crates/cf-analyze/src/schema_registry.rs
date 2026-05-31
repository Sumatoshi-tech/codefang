//! Static registry of analyzer output schemas.
//!
//! Port of `internal/analyzers/analyze/schema_registry.go`. The schema is
//! attached to [`crate::conversion::AnalyzerResult`] (`schema,omitempty`) and
//! therefore serialized, so [`FieldMeta`]'s field order (`type`, `grain`,
//! `description`) and omitempty are preserved, and the registry contents match
//! the Go table verbatim.

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Describes a single field in an analyzer's output schema.
///
/// Mirrors `FieldMeta` (schema_registry.go:4). Serialized order is `type`,
/// `grain` (omitempty), `description` (omitempty).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FieldMeta {
    /// Field kind (`type`): `list`, `aggregate`, `risk`, `scalar`,
    /// `time_series`, …
    pub r#type: String,
    /// Optional data grain (`grain,omitempty`): `function`, `file`, `tick`, …
    pub grain: String,
    /// Optional human description (`description,omitempty`).
    pub description: String,
}

impl FieldMeta {
    fn new(r#type: &str, grain: &str, description: &str) -> Self {
        Self {
            r#type: r#type.to_string(),
            grain: grain.to_string(),
            description: description.to_string(),
        }
    }
}

/// Maps output field names to their metadata.
///
/// Mirrors `AnalyzerSchema = map[string]FieldMeta` (schema_registry.go:11). A
/// [`BTreeMap`] gives the byte-sorted key order the encoder would apply to a Go
/// `map[string]…` anyway, keeping serialization byte-identical.
pub type AnalyzerSchema = BTreeMap<String, FieldMeta>;

/// Returns the output schema for `analyzer_id`, or `None` if unregistered.
///
/// Mirrors `SchemaForAnalyzer` (schema_registry.go:15).
#[must_use]
pub fn schema_for_analyzer(analyzer_id: &str) -> Option<AnalyzerSchema> {
    registry().get(analyzer_id).cloned()
}

fn registry() -> &'static BTreeMap<String, AnalyzerSchema> {
    static REGISTRY: OnceLock<BTreeMap<String, AnalyzerSchema>> = OnceLock::new();
    REGISTRY.get_or_init(build_registry)
}

#[allow(clippy::too_many_lines)]
fn build_registry() -> BTreeMap<String, AnalyzerSchema> {
    let mut reg: BTreeMap<String, AnalyzerSchema> = BTreeMap::new();

    let mut schema = |id: &str, fields: &[(&str, FieldMeta)]| {
        let mut s = AnalyzerSchema::new();
        for (name, meta) in fields {
            s.insert((*name).to_string(), meta.clone());
        }
        reg.insert(id.to_string(), s);
    };

    schema(
        "static/complexity",
        &[
            ("function_complexity", FieldMeta::new("list", "function", "Per-function cyclomatic and cognitive complexity")),
            ("distribution", FieldMeta::new("aggregate", "", "Complexity distribution (simple/moderate/complex)")),
            ("high_risk_functions", FieldMeta::new("risk", "function", "Functions exceeding complexity thresholds")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "static/halstead",
        &[
            ("function_halstead", FieldMeta::new("list", "function", "Per-function Halstead volume, effort, and bugs")),
            ("distribution", FieldMeta::new("aggregate", "", "Effort distribution (low/medium/high/very_high)")),
            ("high_effort_functions", FieldMeta::new("risk", "function", "Functions with high Halstead effort")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "static/cohesion",
        &[
            ("function_cohesion", FieldMeta::new("list", "function", "Per-function LCOM cohesion score")),
            ("distribution", FieldMeta::new("aggregate", "", "Cohesion distribution")),
            ("low_cohesion_functions", FieldMeta::new("risk", "function", "Functions with poor cohesion")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "static/comments",
        &[
            ("comment_quality", FieldMeta::new("list", "comment", "Per-comment quality assessment")),
            ("function_documentation", FieldMeta::new("list", "function", "Per-function documentation status")),
            ("undocumented_functions", FieldMeta::new("risk", "function", "Functions lacking documentation")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "static/clones",
        &[
            ("clone_pairs", FieldMeta::new("list", "pair", "Detected clone pairs with similarity")),
            ("clone_type_distribution", FieldMeta::new("aggregate", "", "Clone type breakdown (Type-1/2/3)")),
            ("total_functions", FieldMeta::new("scalar", "", "Total functions analyzed")),
            ("total_clone_pairs", FieldMeta::new("scalar", "", "Total clone pairs (uncapped)")),
            ("clone_ratio", FieldMeta::new("scalar", "", "Fraction of functions involved in duplication")),
        ],
    );
    schema(
        "static/imports",
        &[
            ("import_list", FieldMeta::new("list", "import", "All import statements")),
            ("dependencies", FieldMeta::new("list", "dependency", "External dependencies with risk")),
            ("categories", FieldMeta::new("aggregate", "", "Import category breakdown")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "static/composition",
        &[
            ("breakdown", FieldMeta::new("aggregate", "", "File count per category")),
            ("percentages", FieldMeta::new("aggregate", "", "Percentage per category")),
            ("total_files", FieldMeta::new("scalar", "", "Total files analyzed")),
        ],
    );
    schema(
        "history/sentiment",
        &[
            ("time_series", FieldMeta::new("time_series", "tick", "Per-tick sentiment scores")),
            ("trend", FieldMeta::new("aggregate", "", "Sentiment trend direction")),
            ("low_sentiment_periods", FieldMeta::new("risk", "tick", "Ticks with negative sentiment")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "history/anomaly",
        &[
            ("anomalies", FieldMeta::new("list", "tick", "Detected anomalous ticks")),
            ("time_series", FieldMeta::new("time_series", "tick", "Per-tick anomaly metrics and z-scores")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "history/devs",
        &[
            ("developers", FieldMeta::new("list", "developer", "Per-developer contribution statistics")),
            ("languages", FieldMeta::new("list", "language", "Per-language contribution breakdown")),
            ("busfactor", FieldMeta::new("list", "language", "Bus factor per language")),
            ("activity", FieldMeta::new("time_series", "tick", "Per-tick commit activity by developer")),
            ("churn", FieldMeta::new("time_series", "tick", "Per-tick lines added/removed")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "history/file-history",
        &[
            ("file_churn", FieldMeta::new("list", "file", "Per-file change frequency and contributors")),
            ("file_contributors", FieldMeta::new("list", "file", "Per-file contributor breakdown")),
            ("hotspots", FieldMeta::new("risk", "file", "High-churn files")),
            ("composition", FieldMeta::new("aggregate", "", "File type composition")),
            ("composition_ts", FieldMeta::new("time_series", "tick", "File composition over time")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "history/couples",
        &[
            ("file_coupling", FieldMeta::new("list", "pair", "Co-changed file pairs")),
            ("developer_coupling", FieldMeta::new("list", "pair", "Developer collaboration pairs")),
            ("file_ownership", FieldMeta::new("list", "file", "Per-file ownership")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "history/shotness",
        &[
            ("node_hotness", FieldMeta::new("list", "node", "AST node change frequency")),
            ("node_coupling", FieldMeta::new("list", "pair", "Co-changed AST node pairs")),
            ("hotspot_nodes", FieldMeta::new("risk", "node", "Frequently changed nodes")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "history/burndown",
        &[
            ("global_survival", FieldMeta::new("time_series", "sample", "Global code survival curve")),
            ("file_survival", FieldMeta::new("list", "file", "Per-file survival data")),
            ("developer_survival", FieldMeta::new("list", "developer", "Per-developer survival data")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "history/quality",
        &[
            ("time_series", FieldMeta::new("time_series", "tick", "Per-tick code quality metrics")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "history/imports",
        &[
            ("import_list", FieldMeta::new("list", "import", "Import statements (requires UAST mode)")),
            ("dependencies", FieldMeta::new("list", "dependency", "Dependencies (requires UAST mode)")),
            ("categories", FieldMeta::new("aggregate", "", "Import category breakdown")),
            ("aggregate", FieldMeta::new("aggregate", "", "Summary statistics")),
        ],
    );
    schema(
        "history/typos",
        &[("typos", FieldMeta::new("list", "identifier", "Detected identifier typos (requires UAST mode)"))],
    );

    reg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_analyzer_has_schema() {
        let s = schema_for_analyzer("static/complexity").expect("registered");
        let fc = s.get("function_complexity").expect("field");
        assert_eq!(fc.r#type, "list");
        assert_eq!(fc.grain, "function");
    }

    #[test]
    fn unknown_analyzer_yields_none() {
        assert!(schema_for_analyzer("history/nonexistent").is_none());
    }

    #[test]
    fn typos_has_single_field() {
        let s = schema_for_analyzer("history/typos").expect("registered");
        assert_eq!(s.len(), 1);
        assert!(s.contains_key("typos"));
    }

    #[test]
    fn registry_covers_all_go_entries() {
        // 17 analyzer IDs in schema_registry.go.
        for id in [
            "static/complexity",
            "static/halstead",
            "static/cohesion",
            "static/comments",
            "static/clones",
            "static/imports",
            "static/composition",
            "history/sentiment",
            "history/anomaly",
            "history/devs",
            "history/file-history",
            "history/couples",
            "history/shotness",
            "history/burndown",
            "history/quality",
            "history/imports",
            "history/typos",
        ] {
            assert!(schema_for_analyzer(id).is_some(), "missing {id}");
        }
    }
}
