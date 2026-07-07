//! The cohesion [`Analyzer`].
//!
//! Drives function discovery, variable extraction, per-function cohesion, the three
//! module scalars, and assembly of the intermediate [`Report`].

use crate::calc;
use crate::report_value::{Report, ReportValue};
use crate::uast::{role, ty, Node};
use std::collections::BTreeMap;

/// Default maximum UAST traversal depth. The depth limit is enforced by the
/// shared traverser in the integrated build.
pub const MAX_DEPTH_VALUE: i64 = 10;

// --- Assessment thresholds (report contract) ---

const COHESION_THRESHOLD_HIGH: f64 = 0.6;
const COHESION_THRESHOLD_MEDIUM: f64 = 0.4;
const COHESION_THRESHOLD_LOW: f64 = 0.3;

const COUNT_THRESHOLD_HIGH: usize = 3;
const LINE_COUNT_THRESHOLD_HIGH: i64 = 10;
const MAGIC7: usize = 7;
const MAGIC30: i64 = 30;

// --- Detail-message thresholds (shared with the aggregated-score message) ---

const SCORE_THRESHOLD_HIGH: f64 = 0.7;
const SCORE_THRESHOLD_MEDIUM: f64 = 0.4;
const SCORE_THRESHOLD_LOW: f64 = 0.3;

/// A function with its cohesion metrics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Function {
    /// Function name.
    pub name: String,
    /// All variable names found within the function (with duplicates;
    /// de-duplication happens later where the algorithm requires it).
    pub variables: Vec<String>,
    /// Source line count.
    pub line_count: i64,
    /// Per-function cohesion score, filled in by
    /// [`Analyzer::compute_per_function_cohesion`].
    pub cohesion: f64,
}

/// Typed per-function report item.
///
/// Field order here is cosmetic; the JSON shape is produced by
/// [`Analyzer::convert_cohesion_function_items`].
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
    /// Variable count, duplicates included (the length of
    /// [`Function::variables`]).
    pub variable_count: usize,
    /// Per-function cohesion.
    pub cohesion: f64,
}

/// The cohesion analyzer.
///
/// Stateless: the traversal and extraction helpers are inlined into the
/// extraction routines, so the struct is a zero-sized marker. Construct via
/// [`Analyzer::new`].
#[derive(Debug, Clone, Default)]
pub struct Analyzer;

/// Error returned when [`Analyzer::analyze`] is given no root node. The error
/// text is part of the CLI contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NilRootNode;

impl std::fmt::Display for NilRootNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("root node is nil")
    }
}

impl std::error::Error for NilRootNode {}

impl Analyzer {
    /// Creates a new analyzer.
    #[must_use]
    pub fn new() -> Self {
        Analyzer
    }

    /// The analyzer name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        crate::ANALYZER_NAME
    }

    /// The CLI flag.
    #[must_use]
    pub fn flag(&self) -> &'static str {
        "cohesion-analysis"
    }

    /// The analyzer description.
    #[must_use]
    pub fn description(&self) -> &'static str {
        "Calculates LCOM-HS (Henderson-Sellers) and cohesion metrics."
    }

    /// Performs cohesion analysis on a UAST root.
    ///
    /// Returns an intermediate [`Report`]; convert to the machine format with
    /// [`crate::metrics::compute_all_metrics`].
    ///
    /// A subtree with no functions yields the empty-result shape: `lcom = 0`,
    /// `cohesion_score = 1`, `function_cohesion = 1`, and no `analyzer_name` /
    /// `functions` keys:
    ///
    /// ```
    /// use cf_cohesion::Analyzer;
    /// use cf_cohesion::report_value::ReportValue;
    /// use cf_cohesion::uast::TestNode;
    ///
    /// let report = Analyzer::new().analyze(&TestNode::block(vec![])).unwrap();
    /// assert_eq!(report.get("total_functions"), Some(&ReportValue::Int(0)));
    /// assert_eq!(report.get("cohesion_score"), Some(&ReportValue::Float(1.0)));
    /// assert!(report.get("functions").is_none());
    /// ```
    ///
    /// # Errors
    ///
    /// Currently never fails: the root is non-nullable, so the missing-root
    /// error cannot fire. The `Result` signature leaves room for the integrated
    /// traverser to surface errors. Pass an explicitly-empty subtree to model
    /// "no functions".
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

    /// Performs cohesion analysis the way the **static pipeline actually runs
    /// it**: via the cohesion [`visitor`](crate::visitor), driven by a preorder
    /// DFS over the UAST.
    ///
    /// This is NOT the same as [`analyze`](Self::analyze): the visitor's function
    /// detection (type Function/Method OR roles {Function AND Declaration}) and
    /// its context-stack variable attribution (variables belong to the
    /// *innermost* enclosing function) differ from the [`find_functions`]
    /// (Self::find_functions) path, and the static pipeline uses the visitor.
    /// Use this for folder-walk parity.
    #[must_use]
    pub fn analyze_visitor<N: Node>(&self, root: &N) -> Report {
        let functions = crate::visitor::collect_functions_via_visitor(self, root);
        if functions.is_empty() {
            return self.build_empty_result();
        }
        let mut functions = functions;
        self.compute_per_function_cohesion(&mut functions);
        let metrics = self.calculate_metrics(&functions);
        self.build_result(&functions, &metrics)
    }

    /// Visitor function predicate: type Function/Method OR roles
    /// {Function AND Declaration}. Distinct from the `find_functions` predicate.
    #[must_use]
    pub fn is_visitor_function<N: Node>(&self, n: &N) -> bool {
        n.has_any_type(&[ty::FUNCTION, ty::METHOD])
            || n.has_all_roles(&[role::FUNCTION, role::DECLARATION])
    }

    /// Computes per-function cohesion via the global shared-variable Bloom
    /// filter.
    pub fn compute_per_function_cohesion(&self, functions: &mut [Function]) {
        let global_filter = calc::build_global_variable_filter(functions);
        for f in functions.iter_mut() {
            f.cohesion = self.calculate_function_level_cohesion(f, global_filter.as_ref());
        }
    }

    /// Builds the empty result map (the "no functions" report shape).
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

    /// Computes the three module scalars.
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

    /// Constructs the final intermediate result.
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

    /// Builds the typed per-function table.
    #[must_use]
    pub fn build_detailed_functions_table(
        &self,
        functions: &[Function],
    ) -> Vec<FunctionReportItem> {
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

    /// Converts typed items to the dynamic per-function map shape.
    /// `source_file`, when non-empty, is attached under the `_source_file` key.
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

    /// Detail message keyed by cohesion score: the first label whose limit the
    /// score meets or exceeds, scanning highest first.
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

    /// Emoji cohesion assessment.
    ///
    /// `>= 0.6` is excellent, `>= 0.4` good, `>= 0.3` fair, otherwise poor:
    ///
    /// ```
    /// use cf_cohesion::Analyzer;
    /// let a = Analyzer::new();
    /// assert_eq!(a.get_cohesion_assessment(0.6), "\u{1F7E2} Excellent");
    /// assert_eq!(a.get_cohesion_assessment(0.4), "\u{1F7E1} Good");
    /// assert_eq!(a.get_cohesion_assessment(0.3), "\u{1F7E1} Fair");
    /// assert_eq!(a.get_cohesion_assessment(0.29), "\u{1F534} Poor");
    /// ```
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

    /// Emoji variable-count assessment.
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

    /// Emoji size assessment.
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

    // --- Machine-format report writers ---

    /// Writes the JSON machine report: the report is reduced to
    /// [`crate::metrics::ComputedMetrics`] then encoded with two-space indent,
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

    /// Writes the YAML machine report.
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

    /// Writes the binary (CFB1) machine report.
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

    /// Finds all functions in the tree.
    ///
    /// The reference implementation combines role-based and type-based matches
    /// and deduplicates via a map with *nondeterministic* iteration order (see
    /// the crate-level docs). This implementation deduplicates by walking once
    /// in a deterministic preorder; because the downstream `functions` array
    /// order is a canonicalized golden path, the only observable invariant that
    /// must hold is the *set* of functions and the stable scalars derived from
    /// it.
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

    /// True if a node should be treated as a function: role `Function` OR type
    /// `Function`/`Method`.
    fn is_function_node<N: Node>(&self, n: &N) -> bool {
        n.has_any_role(&[role::FUNCTION]) || n.has_any_type(&[ty::FUNCTION, ty::METHOD])
    }

    /// Extracts a [`Function`] from a node. `cohesion` starts at `0.0` and is
    /// filled later.
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

    /// Extracts the function name.
    #[must_use]
    pub fn extract_function_name<N: Node>(&self, n: &N) -> String {
        n.entity_name()
    }

    /// Collects all variable names within a function subtree.
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

    /// Processes a single node for variable extraction.
    ///
    /// NOTE: the declaration and identifier predicates are checked independently
    /// (two `if`s, not `else if`), so a node satisfying both contributes its name
    /// **twice** — a report-contract quirk that affects `variable_count` and is
    /// pinned by the differential gate.
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

    /// Public wrapper of the declaration predicate, used by the visitor module.
    #[must_use]
    pub fn is_variable_declaration_pub<N: Node>(&self, n: &N) -> bool {
        self.is_variable_declaration(n)
    }

    /// Public wrapper of the identifier predicate, used by the visitor module.
    #[must_use]
    pub fn is_variable_identifier_pub<N: Node>(&self, n: &N) -> bool {
        self.is_variable_identifier(n)
    }

    fn add_variable_if_valid<N: Node>(&self, n: &N, vars: &mut Vec<String>) {
        let name = n.entity_name();
        if !name.is_empty() {
            vars.push(name);
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
        // The empty result has no analyzer_name / functions keys.
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
    fn assessments_match_contract_thresholds() {
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
    fn detail_messages_match_contract() {
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
