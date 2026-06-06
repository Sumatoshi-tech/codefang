//! The per-file clone-detection [`Analyzer`].
//!
//! Port of `internal/analyzers/clones/analyzer.go`. It builds MinHash signatures
//! for every function in a UAST, indexes them with LSH, finds the clone pairs,
//! and produces the `analyze::Report` map plus the machine-format projections.

use std::io::Write;

use cf_analyze::descriptor::{new_descriptor, AnalyzerMode, Descriptor};
use cf_analyze::history::AnalyzerError;
use cf_analyze::{GoMap, GoValue, MapOrigin, Report, Thresholds};
use cf_uast_node::Node;

use crate::engine::{
    build_index, build_signatures, count_distinct_funcs, find_clone_pairs, is_function_node,
};
use crate::uast::{ROLE_FUNCTION, UAST_FUNCTION, UAST_METHOD};
use crate::report::{
    categorize_clone_pairs, clone_type_dist_map, ClonePair, ComputedMetrics, SIMILARITY_TYPE3,
};
use crate::shingler::Shingler;
use crate::{
    clone_message, compute_clone_ratio, ANALYZER_DESCRIPTION, ANALYZER_FLAG, ANALYZER_NAME,
    KEY_ANALYZER_NAME, KEY_CLONE_PAIRS, KEY_CLONE_RATIO, KEY_CLONE_TYPE_DISTRIBUTION, KEY_MESSAGE,
    KEY_TOTAL_CLONE_PAIRS, KEY_TOTAL_FUNCTIONS, MSG_EMPTY_AST, MSG_NO_FUNCTIONS, NUM_BANDS,
    NUM_HASHES, NUM_ROWS, THRESHOLD_CLONE_PAIRS_RED, THRESHOLD_CLONE_PAIRS_YELLOW,
    THRESHOLD_CLONE_RATIO_RED, THRESHOLD_CLONE_RATIO_YELLOW,
};

/// Clone-detection analyzer using MinHash and LSH. Mirrors Go `Analyzer`.
#[derive(Debug, Clone)]
pub struct Analyzer {
    shingler: Shingler,
    cfg_num_hashes: usize,
    cfg_num_bands: usize,
    cfg_num_rows: usize,
    cfg_similarity_type3: f64,
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyzer {
    /// Creates a new clone-detection analyzer with the Go defaults. Mirrors Go
    /// `NewAnalyzer`.
    #[must_use]
    pub fn new() -> Self {
        Self {
            shingler: Shingler::default(),
            cfg_num_hashes: NUM_HASHES,
            cfg_num_bands: NUM_BANDS,
            cfg_num_rows: NUM_ROWS,
            cfg_similarity_type3: SIMILARITY_TYPE3,
        }
    }

    /// The analyzer name. Mirrors Go `Analyzer.Name`.
    #[must_use]
    pub fn name(&self) -> &'static str {
        ANALYZER_NAME
    }

    /// The CLI flag. Mirrors Go `Analyzer.Flag`.
    #[must_use]
    pub fn flag(&self) -> &'static str {
        ANALYZER_FLAG
    }

    /// Stable analyzer metadata. Mirrors Go `Analyzer.Descriptor`.
    #[must_use]
    pub fn descriptor(&self) -> Descriptor {
        new_descriptor(AnalyzerMode::Static, ANALYZER_NAME, ANALYZER_DESCRIPTION)
    }

    /// Color-coded thresholds for clone metrics. Mirrors Go `Analyzer.Thresholds`.
    #[must_use]
    pub fn thresholds(&self) -> Thresholds {
        let mut ratio = GoMap::new(MapOrigin::Map);
        ratio.push("green", GoValue::Float(0.0));
        ratio.push("yellow", GoValue::Float(THRESHOLD_CLONE_RATIO_YELLOW));
        ratio.push("red", GoValue::Float(THRESHOLD_CLONE_RATIO_RED));

        let mut pairs = GoMap::new(MapOrigin::Map);
        pairs.push("green", GoValue::Int(0));
        pairs.push("yellow", GoValue::Int(THRESHOLD_CLONE_PAIRS_YELLOW));
        pairs.push("red", GoValue::Int(THRESHOLD_CLONE_PAIRS_RED));

        let mut t = GoMap::new(MapOrigin::Map);
        t.push("clone_ratio", GoValue::Object(ratio));
        t.push("total_clone_pairs", GoValue::Object(pairs));
        t
    }

    /// Analyzes a UAST root into a clone-detection [`Report`].
    ///
    /// Mirrors Go `Analyzer.Analyze`. A `None` root yields the "No AST provided"
    /// empty report; no functions yields the "No functions found" empty report.
    #[must_use]
    pub fn analyze_node(&self, root: Option<&Node>) -> Report {
        let Some(root) = root else {
            return build_empty_report(MSG_EMPTY_AST);
        };

        let functions = self.find_functions(root);
        if functions.is_empty() {
            return build_empty_report(MSG_NO_FUNCTIONS);
        }

        let pairs = self.detect_clones(&functions);
        self.build_report(functions.len(), &pairs)
    }

    /// Finds all function and method nodes in the UAST.
    ///
    /// Mirrors Go `findFunctions`: collects by type (`Function`/`Method`) then by
    /// the `Function` role, deduplicating by node identity in that order.
    fn find_functions<'a>(&self, root: &'a Node) -> Vec<&'a Node> {
        let type_nodes = root.find(|n| n.has_any_type(&[UAST_FUNCTION, UAST_METHOD]));
        let role_nodes = root.find(|n| n.has_any_role(&[ROLE_FUNCTION]));

        let mut seen: Vec<*const Node> = Vec::new();
        let mut functions: Vec<&Node> = Vec::new();

        let consider = |n: &'a Node, seen: &mut Vec<*const Node>, out: &mut Vec<&'a Node>| {
            let ptr = std::ptr::from_ref::<Node>(n);
            if !seen.contains(&ptr) && is_function_node(n) {
                seen.push(ptr);
                out.push(n);
            }
        };

        for n in type_nodes {
            consider(n, &mut seen, &mut functions);
        }
        for n in role_nodes {
            consider(n, &mut seen, &mut functions);
        }

        functions
    }

    /// Builds MinHash signatures, indexes them, and finds clone pairs.
    ///
    /// Mirrors Go `detectClones` (per-file: no cap, threshold = Type-3).
    fn detect_clones(&self, functions: &[&Node]) -> Vec<ClonePair> {
        let entries = build_signatures(functions, &self.shingler, self.cfg_num_hashes);
        if entries.is_empty() {
            return Vec::new();
        }

        let Some(idx) = build_index(&entries, self.cfg_num_bands, self.cfg_num_rows) else {
            return Vec::new();
        };

        // Per-file detection: no cap (single-file scope, bounded by func count).
        find_clone_pairs(&entries, &idx, 0, self.cfg_similarity_type3).pairs
    }

    /// Constructs the analysis report map. Mirrors Go `buildReport`.
    fn build_report(&self, total_functions: usize, pairs: &[ClonePair]) -> Report {
        let clone_ratio = compute_clone_ratio(count_distinct_funcs(pairs), total_functions);
        let message = clone_message(pairs.len());

        let pairs_value =
            GoValue::Array(pairs.iter().map(ClonePair::to_go_value).collect::<Vec<_>>());
        let dist = clone_type_dist_map(categorize_clone_pairs(pairs));

        // analyze::Report is a map-origin GoMap: keys byte-sort on encode.
        let mut report = GoMap::new(MapOrigin::Map);
        report.push(KEY_ANALYZER_NAME, GoValue::Str(ANALYZER_NAME.to_string()));
        report.push(KEY_TOTAL_FUNCTIONS, GoValue::Int(total_functions as i64));
        report.push(KEY_TOTAL_CLONE_PAIRS, GoValue::Int(pairs.len() as i64));
        report.push(KEY_CLONE_RATIO, GoValue::Float(clone_ratio));
        report.push(KEY_CLONE_PAIRS, pairs_value);
        report.push(KEY_CLONE_TYPE_DISTRIBUTION, dist);
        report.push(KEY_MESSAGE, GoValue::Str(message.to_string()));
        report
    }

    /// Writes the report as indented JSON of [`ComputedMetrics`].
    ///
    /// Mirrors Go `FormatReportJSON` (`json.MarshalIndent("", "  ")`). Routes
    /// through [`cf_gojson`], never `serde_json`.
    ///
    /// # Errors
    /// Returns [`AnalyzerError`] if the writer fails.
    pub fn format_report_json(&self, report: &Report, w: &mut dyn Write) -> Result<(), AnalyzerError> {
        let metrics = compute_metrics_from_report(report);
        let bytes = cf_gojson::marshal_indent(&metrics.to_go_value());
        w.write_all(&bytes)
            .map_err(|e| AnalyzerError::Other(format!("formatreportjson: {e}")))
    }

    /// Writes the report as a CFB1 binary envelope of [`ComputedMetrics`].
    ///
    /// Mirrors Go `FormatReportBinary` (`reportutil.EncodeBinaryEnvelope`).
    ///
    /// # Errors
    /// Returns [`AnalyzerError`] if encoding or the writer fails.
    pub fn format_report_binary(
        &self,
        report: &Report,
        w: &mut dyn Write,
    ) -> Result<(), AnalyzerError> {
        let metrics = compute_metrics_from_report(report);
        let bytes = cf_reportutil::encode_binary_envelope(&metrics.to_go_value())
            .map_err(|e| AnalyzerError::Other(format!("formatreportbinary: {e}")))?;
        w.write_all(&bytes)
            .map_err(|e| AnalyzerError::Other(format!("formatreportbinary: {e}")))
    }

    /// Returns the [`ComputedMetrics`] projection used by the YAML path.
    ///
    /// Mirrors the `metrics := computeMetricsFromReport(report)` step shared by
    /// Go's `FormatReportYAML`. The YAML emitter (`cf-goyaml`) is wired by the
    /// framework once it lands; YAML is a non-binding capture (DESIGN §6), so the
    /// projection — not the emitter — is the byte-identity-critical part and is
    /// exposed here for it.
    #[must_use]
    pub fn computed_metrics(&self, report: &Report) -> ComputedMetrics {
        compute_metrics_from_report(report)
    }
}

/// Creates an empty report with the given message. Mirrors Go `buildEmptyReport`.
///
/// The Go path runs through `common.NewResultBuilder().BuildCustomEmptyResult`,
/// which returns exactly these five keys as a `map[string]any`.
#[must_use]
pub fn build_empty_report(message: &str) -> Report {
    let mut report = GoMap::new(MapOrigin::Map);
    report.push(KEY_ANALYZER_NAME, GoValue::Str(ANALYZER_NAME.to_string()));
    report.push(KEY_TOTAL_FUNCTIONS, GoValue::Int(0));
    report.push(KEY_TOTAL_CLONE_PAIRS, GoValue::Int(0));
    report.push(KEY_CLONE_RATIO, GoValue::Float(0.0));
    report.push(KEY_MESSAGE, GoValue::Str(message.to_string()));
    report
}

/// Projects a report map into [`ComputedMetrics`]. Mirrors Go
/// `computeMetricsFromReport`.
#[must_use]
pub fn compute_metrics_from_report(report: &Report) -> ComputedMetrics {
    let total_functions = cf_reportutil::get_int(report, KEY_TOTAL_FUNCTIONS);
    let total_clone_pairs = cf_reportutil::get_int(report, KEY_TOTAL_CLONE_PAIRS);
    let clone_ratio = cf_reportutil::get_float64(report, KEY_CLONE_RATIO);
    let message = cf_reportutil::get_string(report, KEY_MESSAGE);
    let clone_pairs = extract_clone_pairs(report);
    let clone_type_dist = extract_clone_type_dist(report);

    ComputedMetrics {
        total_functions,
        total_clone_pairs,
        clone_ratio,
        clone_type_dist,
        clone_pairs,
        message,
    }
}

/// Extracts the clone pairs from a report. Mirrors Go `extractClonePairs`
/// (handling the `[]map[string]any` case produced by [`Analyzer::build_report`]).
fn extract_clone_pairs(report: &Report) -> Vec<ClonePair> {
    let Some(GoValue::Array(items)) = cf_reportutil::get(report, KEY_CLONE_PAIRS) else {
        return Vec::new();
    };

    let mut pairs = Vec::with_capacity(items.len());
    for item in items {
        if let GoValue::Map(m) = item {
            pairs.push(clone_pair_from_map(m));
        }
    }
    pairs
}

/// Extracts a [`ClonePair`] from a map value. Mirrors Go `clonePairFromMap`.
fn clone_pair_from_map(m: &GoMap) -> ClonePair {
    let mut pair = ClonePair {
        func_a: String::new(),
        func_b: String::new(),
        similarity: 0.0,
        clone_type: String::new(),
    };
    for (k, v) in m.entries() {
        match (k.as_str(), v) {
            ("func_a", GoValue::Str(s)) => pair.func_a = s.clone(),
            ("func_b", GoValue::Str(s)) => pair.func_b = s.clone(),
            ("similarity", GoValue::Float(f)) => pair.similarity = *f,
            ("clone_type", GoValue::Str(s)) => pair.clone_type = s.clone(),
            _ => {}
        }
    }
    pair
}

/// Extracts the `clone_type_distribution` counts, if present, mirroring the Go
/// `report[keyCloneTypeDistribution].(map[string]int)` branch of
/// `computeMetricsFromReport`.
fn extract_clone_type_dist(report: &Report) -> Option<crate::report::CloneTypeCounts> {
    let Some(GoValue::Map(m)) =
        cf_reportutil::get(report, KEY_CLONE_TYPE_DISTRIBUTION)
    else {
        return None;
    };

    let mut counts = crate::report::CloneTypeCounts::default();
    for (k, v) in m.entries() {
        if let GoValue::Int(n) = v {
            match k.as_str() {
                crate::report::CLONE_TYPE1 => counts.type1 = *n,
                crate::report::CLONE_TYPE2 => counts.type2 = *n,
                crate::report::CLONE_TYPE3 => counts.type3 = *n,
                _ => {}
            }
        }
    }
    Some(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uast::NodeBuilder;
    use cf_gojson::Encoder;
    use cf_uast_node::Node;

    /// Builds a function subtree with at least `MIN_FUNCTION_NODES` nodes and the
    /// given identifier name, so it survives the size gate and produces shingles.
    fn function(name: &str) -> Node {
        let name_node = NodeBuilder::new("Identifier").role("Name").token(name).build();
        let mut f = NodeBuilder::new("Function").role("Function").child(name_node).build();
        // Add a deterministic body of ≥20 nodes so countNodes >= 20 and there
        // are enough types (>= k=5) to shingle.
        let mut block = NodeBuilder::new("Block").build();
        for i in 0..24 {
            let kind = ["Identifier", "Call", "Literal", "Operator"][i % 4];
            block.add_child(NodeBuilder::new(kind).build());
        }
        f.add_child(block);
        f
    }

    #[test]
    fn descriptor_is_static_clones() {
        let a = Analyzer::new();
        assert_eq!(a.descriptor().id, "static/clones");
        assert_eq!(a.name(), "clones");
        assert_eq!(a.flag(), "clone-detection");
    }

    #[test]
    fn nil_root_returns_empty_ast_message() {
        let a = Analyzer::new();
        let report = a.analyze_node(None);
        assert_eq!(cf_reportutil::get_string(&report, KEY_MESSAGE), MSG_EMPTY_AST);
        assert_eq!(cf_reportutil::get_int(&report, KEY_TOTAL_FUNCTIONS), 0);
    }

    #[test]
    fn no_functions_returns_no_functions_message() {
        let a = Analyzer::new();
        let root = NodeBuilder::new("File").build(); // no function nodes
        let report = a.analyze_node(Some(&root));
        assert_eq!(
            cf_reportutil::get_string(&report, KEY_MESSAGE),
            MSG_NO_FUNCTIONS
        );
    }

    #[test]
    fn identical_functions_detected_as_type1_clone() {
        let a = Analyzer::new();
        // Two structurally identical functions -> similarity 1.0 -> Type-1.
        let root = NodeBuilder::new("File")
            .child(function("foo"))
            .child(function("bar"))
            .build();
        let report = a.analyze_node(Some(&root));
        assert_eq!(cf_reportutil::get_int(&report, KEY_TOTAL_FUNCTIONS), 2);
        assert_eq!(cf_reportutil::get_int(&report, KEY_TOTAL_CLONE_PAIRS), 1);

        // clone_ratio = 2 distinct cloned funcs / 2 total = 1.0.
        assert!((cf_reportutil::get_float64(&report, KEY_CLONE_RATIO) - 1.0).abs() < 1e-12);

        let metrics = compute_metrics_from_report(&report);
        assert_eq!(metrics.clone_pairs.len(), 1);
        assert_eq!(metrics.clone_pairs[0].clone_type, "Type-1");
        assert!((metrics.clone_pairs[0].similarity - 1.0).abs() < 1e-9);
    }

    #[test]
    fn empty_report_json_round_trips_through_computed_metrics() {
        let a = Analyzer::new();
        let report = a.analyze_node(None);
        let mut buf = Vec::new();
        a.format_report_json(&report, &mut buf).expect("json ok");
        let json = String::from_utf8(buf).unwrap();
        // Indented; ComputedMetrics field order; no clone_type_distribution
        // (omitempty); clone_pairs null.
        assert!(json.contains("\"total_functions\": 0"));
        assert!(json.contains("\"clone_pairs\": null"));
        assert!(json.contains("\"message\": \"No AST provided\""));
        assert!(!json.contains("clone_type_distribution"));
    }

    #[test]
    fn thresholds_structure_matches_go() {
        let a = Analyzer::new();
        let json = Encoder::marshal().encode_to_string(&GoValue::Object(a.thresholds()));
        assert_eq!(
            json,
            concat!(
                r#"{"clone_ratio":{"green":0,"red":0.3,"yellow":0.1},"#,
                r#""total_clone_pairs":{"green":0,"red":20,"yellow":5}}"#
            )
        );
    }
}
