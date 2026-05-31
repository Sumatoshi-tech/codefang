//! The cohesion [`Analyzer`] — port of `internal/analyzers/cohesion/cohesion.go`
//! and `types.go`.
//!
//! Drives function discovery, variable extraction, per-function cohesion, the three
//! module scalars, and assembly of the intermediate [`Report`].

use crate::calc;
use crate::report_value::{Report, ReportValue};
use crate::uast::{role, ty, Node};
use std::collections::BTreeMap;

/// Default maximum UAST traversal depth (Go `MaxDepthValue`). Retained for parity;
/// the depth limit is enforced by the shared traverser in the integrated build.
pub const MAX_DEPTH_VALUE: i64 = 10;

// --- Assessment thresholds (cohesion.go) ---

/// `cohesionThresholdHigh`.
const COHESION_THRESHOLD_HIGH: f64 = 0.6;
/// `cohesionThresholdMedium`.
const COHESION_THRESHOLD_MEDIUM: f64 = 0.4;
/// `cohesionThresholdLow`.
const COHESION_THRESHOLD_LOW: f64 = 0.3;

/// `countThresholdHigh`.
const COUNT_THRESHOLD_HIGH: usize = 3;
/// `lineCountThresholdHigh`.
const LINE_COUNT_THRESHOLD_HIGH: i64 = 10;
/// `magic7`.
const MAGIC7: usize = 7;
/// `magic30`.
const MAGIC30: i64 = 30;

// --- Detail-message thresholds (these come from aggregator.go's score* consts) ---

/// `scoreThresholdHigh` (aggregator.go).
const SCORE_THRESHOLD_HIGH: f64 = 0.7;
/// `scoreThresholdMedium` (aggregator.go).
const SCORE_THRESHOLD_MEDIUM: f64 = 0.4;
/// `scoreThresholdLow` (aggregator.go).
const SCORE_THRESHOLD_LOW: f64 = 0.3;

/// A function with its cohesion metrics (Go `cohesion.Function`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Function {
    /// Function name.
    pub name: String,
    /// All variable names found within the function (with duplicates, as Go collects
    /// them; de-duplication happens later where the algorithm requires it).
    pub variables: Vec<String>,
    /// Source line count.
    pub line_count: i64,
    /// Per-function cohesion score, filled in by
    /// [`Analyzer::compute_per_function_cohesion`].
    pub cohesion: f64,
}

/// Typed per-function report item (Go `FunctionReportItem`).
///
/// Field order here is cosmetic (the JSON shape is produced by
/// [`Analyzer::convert_cohesion_function_items`]); it follows the Go struct.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionReportItem {
    /// Function name.
    pub name: String,
    /// Emoji cohesion assessment.
    pub cohesion_assessment: String,
    /// Emoji variable-count assessment.
    pub variable_assessment: String,
    /// Emoji size assessment.
    pub size_assessment: String,
    /// Source line count.
    pub line_count: i64,
    /// Distinct-with-duplicates variable count (Go `len(fn.Variables)`).
    pub variable_count: usize,
    /// Per-function cohesion.
    pub cohesion: f64,
}

/// The cohesion analyzer (Go `cohesion.Analyzer`).
///
/// The Go struct holds a `traverser` and `extractor`; in this port those generic
/// helpers are inlined into the extraction routines, so the struct is a zero-sized
/// marker. Construct via [`Analyzer::new`].
#[derive(Debug, Clone, Default)]
pub struct Analyzer;

/// Error returned when [`Analyzer::analyze`] is given no root node (Go
/// `analyze.ErrNilRootNode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NilRootNode;

impl std::fmt::Display for NilRootNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("root node is nil")
    }
}

impl std::error::Error for NilRootNode {}

impl Analyzer {
    /// Creates a new analyzer (Go `NewAnalyzer`).
    #[must_use]
    pub fn new() -> Self {
        Analyzer
    }

    /// The analyzer name (Go `(*Analyzer).Name`).
    #[must_use]
    pub fn name(&self) -> &'static str {
        crate::ANALYZER_NAME
    }

    /// The CLI flag (Go `(*Analyzer).Flag`).
    #[must_use]
    pub fn flag(&self) -> &'static str {
        "cohesion-analysis"
    }

    /// The analyzer description (Go `(*Analyzer).Description` / `Descriptor`).
    #[must_use]
    pub fn description(&self) -> &'static str {
        "Calculates LCOM-HS (Henderson-Sellers) and cohesion metrics."
    }

    /// Performs cohesion analysis on a UAST root (Go `(*Analyzer).Analyze`).
    ///
    /// Returns an intermediate [`Report`]; convert to the machine format with
    /// [`crate::metrics::compute_all_metrics`].
    ///
    /// # Errors
    ///
    /// This port takes `&N` (non-nullable), so the Go `nil` check cannot fire; the
    /// signature still returns `Result` to mirror Go and to leave room for the
    /// integrated traverser to surface errors. Pass an explicitly-empty subtree to
    /// model "no functions".
    pub fn analyze<N: Node>(&self, root: &N) -> Result<Report, NilRootNode> {
        let functions = self.find_functions(root);

        if functions.is_empty() {
            return Ok(self.build_empty_result());
        }

        let mut functions = functions;
        self.compute_per_function_cohesion(&mut functions);

        let metrics = self.calculate_metrics(&functions);
        Ok(self.build_result(&functions, &metrics))
    }

    /// Computes per-function cohesion via the global shared-variable Bloom filter
    /// (Go `computePerFunctionCohesion`).
    pub fn compute_per_function_cohesion(&self, functions: &mut [Function]) {
        let global_filter = calc::build_global_variable_filter(functions);
        for f in functions.iter_mut() {
            f.cohesion = self.calculate_function_level_cohesion(f, global_filter.as_ref());
        }
    }

    /// Builds the empty result map (Go `buildEmptyResult` /
    /// `BuildCustomEmptyResult`).
    #[must_use]
    pub fn build_empty_result(&self) -> Report {
        let mut r = Report::new();
        r.insert("total_functions".into(), ReportValue::Int(0));
        r.insert("lcom".into(), ReportValue::Float(0.0));
        r.insert("cohesion_score".into(), ReportValue::Float(1.0));
        r.insert("function_cohesion".into(), ReportValue::Float(1.0));
        r.insert(
            "message".into(),
            ReportValue::Str("No functions found".into()),
        );
        r
    }

    /// Computes the three module scalars (Go `calculateMetrics`).
    #[must_use]
    pub fn calculate_metrics(&self, functions: &[Function]) -> Metrics {
        let lcom = self.calculate_lcom(functions);
        let cohesion_score = self.calculate_cohesion_score(lcom, functions.len());
        let function_cohesion = self.calculate_function_cohesion(functions);
        Metrics {
            lcom,
            cohesion_score,
            function_cohesion,
        }
    }

    /// Constructs the final intermediate result (Go `buildResult`).
    #[must_use]
    pub fn build_result(&self, functions: &[Function], metrics: &Metrics) -> Report {
        let report_items = self.build_detailed_functions_table(functions);
        let message = self.get_cohesion_message(metrics.cohesion_score);

        let mut r = Report::new();
        r.insert(
            "analyzer_name".into(),
            ReportValue::Str(crate::ANALYZER_NAME.into()),
        );
        r.insert(
            "total_functions".into(),
            ReportValue::Int(functions.len() as i64),
        );
        r.insert("lcom".into(), ReportValue::Float(metrics.lcom));
        r.insert(
            "cohesion_score".into(),
            ReportValue::Float(metrics.cohesion_score),
        );
        r.insert(
            "function_cohesion".into(),
            ReportValue::Float(metrics.function_cohesion),
        );
        r.insert(
            "functions".into(),
            ReportValue::Functions(self.convert_cohesion_function_items(&report_items, "")),
        );
        r.insert("message".into(), ReportValue::Str(message));
        r
    }

    /// Builds the typed per-function table (Go `buildDetailedFunctionsTable`).
    #[must_use]
    pub fn build_detailed_functions_table(&self, functions: &[Function]) -> Vec<FunctionReportItem> {
        functions
            .iter()
            .map(|f| FunctionReportItem {
                name: f.name.clone(),
                line_count: f.line_count,
                variable_count: f.variables.len(),
                cohesion: f.cohesion,
                cohesion_assessment: self.get_cohesion_assessment(f.cohesion),
                variable_assessment: self.get_variable_assessment(f.variables.len()),
                size_assessment: self.get_size_assessment(f.line_count),
            })
            .collect()
    }

    /// Converts typed items to the dynamic `[]map[string]any` shape (Go
    /// `convertCohesionFunctionItems`). `source_file`, when non-empty, is attached
    /// under the `_source_file` key (Go `analyze.SourceFileKey`).
    #[must_use]
    pub fn convert_cohesion_function_items(
        &self,
        items: &[FunctionReportItem],
        source_file: &str,
    ) -> Vec<BTreeMap<String, ReportValue>> {
        items
            .iter()
            .map(|fn_item| {
                let mut m: BTreeMap<String, ReportValue> = BTreeMap::new();
                m.insert("name".into(), ReportValue::Str(fn_item.name.clone()));
                m.insert("line_count".into(), ReportValue::Int(fn_item.line_count));
                m.insert(
                    "variable_count".into(),
                    ReportValue::Int(fn_item.variable_count as i64),
                );
                m.insert("cohesion".into(), ReportValue::Float(fn_item.cohesion));
                m.insert(
                    "cohesion_assessment".into(),
                    ReportValue::Str(fn_item.cohesion_assessment.clone()),
                );
                m.insert(
                    "variable_assessment".into(),
                    ReportValue::Str(fn_item.variable_assessment.clone()),
                );
                m.insert(
                    "size_assessment".into(),
                    ReportValue::Str(fn_item.size_assessment.clone()),
                );
                if !source_file.is_empty() {
                    m.insert(
                        crate::metrics::SOURCE_FILE_KEY.into(),
                        ReportValue::Str(source_file.into()),
                    );
                }
                m
            })
            .collect()
    }

    /// Detail message keyed by cohesion score (Go `getCohesionMessage` +
    /// `cohesionDetailMessageLabeler`). The labeler returns the first label whose
    /// `Limit` the score meets or exceeds, scanning highest first.
    #[must_use]
    pub fn get_cohesion_message(&self, score: f64) -> String {
        if score >= SCORE_THRESHOLD_HIGH {
            "Excellent cohesion - functions are well-focused and cohesive".into()
        } else if score >= SCORE_THRESHOLD_MEDIUM {
            "Good cohesion - functions have reasonable focus".into()
        } else if score >= SCORE_THRESHOLD_LOW {
            "Fair cohesion - some functions could be more focused".into()
        } else {
            "Poor cohesion - functions lack focus and should be refactored".into()
        }
    }

    /// Emoji cohesion assessment (Go `getCohesionAssessment`).
    #[must_use]
    pub fn get_cohesion_assessment(&self, cohesion: f64) -> String {
        if cohesion >= COHESION_THRESHOLD_HIGH {
            "\u{1F7E2} Excellent".into() // green circle
        } else if cohesion >= COHESION_THRESHOLD_MEDIUM {
            "\u{1F7E1} Good".into() // yellow circle
        } else if cohesion >= COHESION_THRESHOLD_LOW {
            "\u{1F7E1} Fair".into() // yellow circle
        } else {
            "\u{1F534} Poor".into() // red circle
        }
    }

    /// Emoji variable-count assessment (Go `getVariableAssessment`).
    #[must_use]
    pub fn get_variable_assessment(&self, count: usize) -> String {
        if count <= COUNT_THRESHOLD_HIGH {
            "\u{1F7E2} Few".into()
        } else if count <= MAGIC7 {
            "\u{1F7E1} Moderate".into()
        } else {
            "\u{1F534} Many".into()
        }
    }

    /// Emoji size assessment (Go `getSizeAssessment`).
    #[must_use]
    pub fn get_size_assessment(&self, line_count: i64) -> String {
        if line_count <= LINE_COUNT_THRESHOLD_HIGH {
            "\u{1F7E2} Small".into()
        } else if line_count <= MAGIC30 {
            "\u{1F7E1} Medium".into()
        } else {
            "\u{1F534} Large".into()
        }
    }

    // --- Machine-format report writers (cohesion.go FormatReport*) ---

    /// Writes the JSON machine report (Go `FormatReportJSON`): the report is reduced
    /// to [`crate::metrics::ComputedMetrics`] then encoded with two-space indent,
    /// HTML escaping ON, and **no trailing newline**.
    ///
    /// # Errors
    ///
    /// Returns any I/O error from `w`.
    pub fn format_report_json<W: std::io::Write>(
        &self,
        report: &Report,
        w: &mut W,
    ) -> std::io::Result<()> {
        let metrics = crate::metrics::compute_all_metrics(report);
        w.write_all(&crate::serialize::encode_json(&metrics))
    }

    /// Writes the YAML machine report (Go `FormatReportYAML`).
    ///
    /// # Errors
    ///
    /// Returns any I/O error from `w`.
    pub fn format_report_yaml<W: std::io::Write>(
        &self,
        report: &Report,
        w: &mut W,
    ) -> std::io::Result<()> {
        let metrics = crate::metrics::compute_all_metrics(report);
        w.write_all(&crate::serialize::encode_yaml(&metrics))
    }

    /// Writes the binary (CFB1) machine report (Go `FormatReportBinary`).
    ///
    /// # Errors
    ///
    /// Returns any I/O error from `w`.
    pub fn format_report_binary<W: std::io::Write>(
        &self,
        report: &Report,
        w: &mut W,
    ) -> std::io::Result<()> {
        let metrics = crate::metrics::compute_all_metrics(report);
        w.write_all(&crate::serialize::encode_binary(&metrics))
    }

    // --- Function discovery / extraction ---

    /// Finds all functions in the tree (Go `findFunctions`).
    ///
    /// The Go code combines role-based and type-based matches, deduplicates via a Go
    /// map (which yields *nondeterministic* order — see the crate-level docs), then
    /// extracts. This port deduplicates by walking once in a deterministic preorder;
    /// because the downstream `functions` array order is a canonicalized golden path,
    /// the only observable invariant that must hold is the *set* of functions and the
    /// stable scalars derived from it.
    #[must_use]
    pub fn find_functions<N: Node>(&self, root: &N) -> Vec<Function> {
        let mut out = Vec::new();
        self.collect_functions(root, &mut out);
        out
    }

    fn collect_functions<N: Node>(&self, n: &N, out: &mut Vec<Function>) {
        if self.is_function_node(n) {
            out.push(self.extract_function(n));
        }
        for child in n.children() {
            self.collect_functions(child, out);
        }
    }

    /// True if a node should be treated as a function (Go `findFunctions` matches
    /// role "Function" OR type "Function"/"Method").
    fn is_function_node<N: Node>(&self, n: &N) -> bool {
        n.has_any_role(&[role::FUNCTION]) || n.has_any_type(&[ty::FUNCTION, ty::METHOD])
    }

    /// Extracts a [`Function`] from a node (Go `extractFunction`). `cohesion` starts
    /// at `0.0` and is filled later.
    #[must_use]
    pub fn extract_function<N: Node>(&self, n: &N) -> Function {
        let variables = self.extract_variables(n);
        let name = self.extract_function_name(n);
        let line_count = n.count_lines();
        Function {
            name,
            line_count,
            variables,
            cohesion: 0.0,
        }
    }

    /// Extracts the function name (Go `extractFunctionName`).
    #[must_use]
    pub fn extract_function_name<N: Node>(&self, n: &N) -> String {
        n.entity_name().to_string()
    }

    /// Collects all variable names within a function subtree (Go `extractVariables`
    /// -> `findVariables`).
    #[must_use]
    pub fn extract_variables<N: Node>(&self, n: &N) -> Vec<String> {
        let mut vars = Vec::new();
        self.find_variables(n, &mut vars);
        vars
    }

    fn find_variables<N: Node>(&self, n: &N, vars: &mut Vec<String>) {
        self.process_variable_node(n, vars);
        for child in n.children() {
            self.find_variables(child, vars);
        }
    }

    /// Processes a single node for variable extraction (Go `processVariableNode`).
    ///
    /// NOTE the Go code checks declaration AND identifier independently (two `if`s,
    /// not `else if`), so a node satisfying both contributes its name **twice** — a
    /// quirk faithfully preserved here because it affects `len(fn.Variables)` and
    /// therefore `variable_count` in the report.
    fn process_variable_node<N: Node>(&self, n: &N, vars: &mut Vec<String>) {
        if self.is_variable_declaration(n) {
            self.add_variable_if_valid(n, vars);
        }
        if self.is_variable_identifier(n) {
            self.add_variable_if_valid(n, vars);
        }
    }

    fn is_variable_declaration<N: Node>(&self, n: &N) -> bool {
        n.has_any_type(&[ty::VARIABLE, ty::PARAMETER]) && n.has_any_role(&[role::DECLARATION])
    }

    fn is_variable_identifier<N: Node>(&self, n: &N) -> bool {
        n.has_any_type(&[ty::IDENTIFIER]) && n.has_any_role(&[role::VARIABLE, role::NAME])
    }

    fn add_variable_if_valid<N: Node>(&self, n: &N, vars: &mut Vec<String>) {
        let name = n.entity_name();
        if !name.is_empty() {
            vars.push(name.to_string());
        }
    }
}

/// The three module scalars produced by [`Analyzer::calculate_metrics`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    /// LCOM-HS.
    pub lcom: f64,
    /// Cohesion score (`1 - lcom`).
    pub cohesion_score: f64,
    /// Average per-function cohesion.
    pub function_cohesion: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::uast::TestNode;

    #[test]
    fn empty_tree_yields_empty_result() {
        let a = Analyzer::new();
        let root = TestNode::block(vec![]);
        let report = a.analyze(&root).unwrap();
        assert_eq!(report.get("total_functions"), Some(&ReportValue::Int(0)));
        assert_eq!(report.get("cohesion_score"), Some(&ReportValue::Float(1.0)));
        assert_eq!(
            report.get("function_cohesion"),
            Some(&ReportValue::Float(1.0))
        );
        assert_eq!(report.get("lcom"), Some(&ReportValue::Float(0.0)));
        assert_eq!(
            report.get("message"),
            Some(&ReportValue::Str("No functions found".into()))
        );
        // Empty result has no analyzer_name / functions keys (Go BuildCustomEmptyResult).
        assert!(report.get("functions").is_none());
    }

    #[test]
    fn finds_functions_and_variables() {
        let a = Analyzer::new();
        // fn f { var x; var y }  fn g { var x }
        let f = TestNode::function(
            "f",
            5,
            vec![TestNode::variable("x"), TestNode::variable("y")],
        );
        let g = TestNode::function("g", 3, vec![TestNode::variable("x")]);
        let root = TestNode::block(vec![f, g]);

        let funcs = a.find_functions(&root);
        assert_eq!(funcs.len(), 2);
        let mut names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["f", "g"]);
    }

    #[test]
    fn analyze_produces_stable_scalars() {
        let a = Analyzer::new();
        // f uses {x,y}, g uses {x}: m=2, a=2, sumMA=3 -> LCOM = 1 - 3/4 = 0.25.
        let f = TestNode::function(
            "f",
            5,
            vec![TestNode::variable("x"), TestNode::variable("y")],
        );
        let g = TestNode::function("g", 3, vec![TestNode::variable("x")]);
        let root = TestNode::block(vec![f, g]);

        let report = a.analyze(&root).unwrap();
        assert_eq!(report.get("total_functions"), Some(&ReportValue::Int(2)));
        let lcom = report.get("lcom").unwrap().as_float().unwrap();
        assert!((lcom - 0.25).abs() < 1e-9, "lcom was {lcom}");
        let score = report.get("cohesion_score").unwrap().as_float().unwrap();
        assert!((score - 0.75).abs() < 1e-9, "score was {score}");
        // analyzer_name present in the non-empty result.
        assert_eq!(
            report.get("analyzer_name"),
            Some(&ReportValue::Str("cohesion".into()))
        );
    }

    #[test]
    fn assessments_match_go_thresholds() {
        let a = Analyzer::new();
        assert_eq!(a.get_cohesion_assessment(0.6), "\u{1F7E2} Excellent");
        assert_eq!(a.get_cohesion_assessment(0.4), "\u{1F7E1} Good");
        assert_eq!(a.get_cohesion_assessment(0.3), "\u{1F7E1} Fair");
        assert_eq!(a.get_cohesion_assessment(0.29), "\u{1F534} Poor");

        assert_eq!(a.get_variable_assessment(3), "\u{1F7E2} Few");
        assert_eq!(a.get_variable_assessment(7), "\u{1F7E1} Moderate");
        assert_eq!(a.get_variable_assessment(8), "\u{1F534} Many");

        assert_eq!(a.get_size_assessment(10), "\u{1F7E2} Small");
        assert_eq!(a.get_size_assessment(30), "\u{1F7E1} Medium");
        assert_eq!(a.get_size_assessment(31), "\u{1F534} Large");
    }

    #[test]
    fn detail_messages_match_go() {
        let a = Analyzer::new();
        assert_eq!(
            a.get_cohesion_message(0.7),
            "Excellent cohesion - functions are well-focused and cohesive"
        );
        assert_eq!(
            a.get_cohesion_message(0.4),
            "Good cohesion - functions have reasonable focus"
        );
        assert_eq!(
            a.get_cohesion_message(0.3),
            "Fair cohesion - some functions could be more focused"
        );
        assert_eq!(
            a.get_cohesion_message(0.1),
            "Poor cohesion - functions lack focus and should be refactored"
        );
    }
}
