//! Per-function and file-level metric structs and the report builder
//! (`halstead.go`: `Metrics`, `FunctionHalsteadMetrics`, `Analyzer.*`).

use std::collections::HashMap;

use crate::gojson::{GoMap, GoValue};

use crate::calculator::{HalsteadCounts, MetricsCalculator};
use crate::detector::{HalNode, OperatorOperandDetector};
use crate::formatter::ReportFormatter;
use crate::CMS_TOKEN_THRESHOLD;

/// Halstead metrics for a single function (`FunctionHalsteadMetrics`).
///
/// The CMS sketches are intentionally *not* serialized (Go: `json:"-"`); they
/// exist only to produce the `estimated_total_*` fields for large functions.
#[derive(Debug, Clone, Default)]
pub struct FunctionHalsteadMetrics {
    /// Operand → count (dynamic map; byte-sorted on encode).
    pub operands: HashMap<String, i64>,
    /// Operator → count (dynamic map; byte-sorted on encode).
    pub operators: HashMap<String, i64>,
    /// Function name (`"anonymous"` when unnamed).
    pub name: String,
    /// CMS-derived estimated total operators (populated only when CMS active).
    pub estimated_total_operators: i64,
    /// CMS-derived estimated total operands (populated only when CMS active).
    pub estimated_total_operands: i64,
    /// True once a CMS sketch was populated for this function (token count
    /// reached the threshold). Mirrors `OperatorSketch != nil`.
    pub cms_active: bool,
    /// N — length.
    pub length: i64,
    /// N2 — total operands.
    pub total_operands: i64,
    /// n — vocabulary.
    pub vocabulary: i64,
    /// N1 — total operators.
    pub total_operators: i64,
    /// Estimated length.
    pub estimated_length: f64,
    /// V — volume.
    pub volume: f64,
    /// D — difficulty.
    pub difficulty: f64,
    /// E — effort.
    pub effort: f64,
    /// T — time to program.
    pub time_to_program: f64,
    /// B — delivered bugs.
    pub delivered_bugs: f64,
    /// n2 — distinct operands.
    pub distinct_operands: i64,
    /// n1 — distinct operators.
    pub distinct_operators: i64,
}

impl FunctionHalsteadMetrics {
    /// The four raw counts of this function.
    #[must_use]
    pub fn counts(&self) -> HalsteadCounts {
        HalsteadCounts {
            distinct_operators: self.distinct_operators,
            distinct_operands: self.distinct_operands,
            total_operators: self.total_operators,
            total_operands: self.total_operands,
        }
    }

    /// Applies the derived metrics computed by the calculator.
    fn apply_derived(&mut self, calc: &MetricsCalculator) {
        let d = calc.calculate(self.counts());
        self.vocabulary = d.vocabulary;
        self.length = d.length;
        self.estimated_length = d.estimated_length;
        self.volume = d.volume;
        self.difficulty = d.difficulty;
        self.effort = d.effort;
        self.time_to_program = d.time_to_program;
        self.delivered_bugs = d.delivered_bugs;
    }
}

/// File-level aggregate Halstead metrics (`Metrics`).
#[derive(Debug, Clone, Default)]
pub struct Metrics {
    /// Per-function metrics that fed this aggregate.
    pub functions: Vec<FunctionHalsteadMetrics>,
    /// Estimated length.
    pub estimated_length: f64,
    /// Sum of per-function estimated total operators.
    pub estimated_total_operators: i64,
    /// Sum of per-function estimated total operands.
    pub estimated_total_operands: i64,
    /// N1 — total operators.
    pub total_operators: i64,
    /// N2 — total operands.
    pub total_operands: i64,
    /// n — vocabulary.
    pub vocabulary: i64,
    /// N — length.
    pub length: i64,
    /// n1 — distinct operators.
    pub distinct_operators: i64,
    /// V — volume.
    pub volume: f64,
    /// D — difficulty.
    pub difficulty: f64,
    /// E — effort.
    pub effort: f64,
    /// T — time to program.
    pub time_to_program: f64,
    /// B — delivered bugs.
    pub delivered_bugs: f64,
    /// n2 — distinct operands.
    pub distinct_operands: i64,
}

/// Computes Halstead metrics for a single function node, including the CMS path
/// for large functions (`calculateFunctionHalsteadMetrics` + `populateCMSSketches`).
///
/// The sketch is populated from the already-built operator/operand maps with the
/// same `epsilon`/`delta` the Go code uses; its total count is exact, so the
/// `estimated_total_*` fields equal the exact sums for active functions.
pub fn calculate_function_metrics<N: HalNode>(
    detector: &OperatorOperandDetector,
    calc: &MetricsCalculator,
    function_node: &N,
) -> FunctionHalsteadMetrics {
    let mut operators: HashMap<String, i64> = HashMap::new();
    let mut operands: HashMap<String, i64> = HashMap::new();
    detector.collect(function_node, &mut operators, &mut operands);

    let total_ops = calc.sum_map(&operators);
    let total_opnds = calc.sum_map(&operands);

    let mut m = FunctionHalsteadMetrics {
        distinct_operators: operators.len() as i64,
        distinct_operands: operands.len() as i64,
        total_operators: total_ops,
        total_operands: total_opnds,
        operators,
        operands,
        ..Default::default()
    };

    let total_tokens = total_ops + total_opnds;
    if total_tokens >= CMS_TOKEN_THRESHOLD {
        populate_cms_sketches(&mut m);
    }

    m.apply_derived(calc);
    m
}

/// Populates the estimated-total fields via a count-min-sketch, mirroring
/// `populateCMSSketches`. The CMS total count is an exact `i64` counter, so the
/// estimate equals the exact sum; if sketch construction fails the function is
/// left on the exact-only path (estimates stay 0, `cms_active` stays false),
/// exactly as the Go code's early `return` does.
fn populate_cms_sketches(m: &mut FunctionHalsteadMetrics) {
    let Ok(mut op_sketch) = cf_alg_cms::Sketch::new(crate::CMS_EPSILON, crate::CMS_DELTA) else {
        return;
    };
    for (key, count) in &m.operators {
        op_sketch.add(key.as_bytes(), *count);
    }

    let Ok(mut opnd_sketch) = cf_alg_cms::Sketch::new(crate::CMS_EPSILON, crate::CMS_DELTA) else {
        return;
    };
    for (key, count) in &m.operands {
        opnd_sketch.add(key.as_bytes(), *count);
    }

    m.estimated_total_operators = op_sketch.total_count();
    m.estimated_total_operands = opnd_sketch.total_count();
    m.cms_active = true;
}

/// Aggregates per-function metrics into file-level metrics
/// (`calculateFileLevelMetrics`).
#[must_use]
pub fn calculate_file_level_metrics(
    calc: &MetricsCalculator,
    function_metrics: Vec<FunctionHalsteadMetrics>,
) -> Metrics {
    let mut file_operators: HashMap<String, i64> = HashMap::new();
    let mut file_operands: HashMap<String, i64> = HashMap::new();
    let mut est_total_ops: i64 = 0;
    let mut est_total_opnds: i64 = 0;

    for fnm in &function_metrics {
        for (operator, count) in &fnm.operators {
            *file_operators.entry(operator.clone()).or_insert(0) += *count;
        }
        for (operand, count) in &fnm.operands {
            *file_operands.entry(operand.clone()).or_insert(0) += *count;
        }
        est_total_ops += fnm.estimated_total_operators;
        est_total_opnds += fnm.estimated_total_operands;
    }

    let counts = HalsteadCounts {
        distinct_operators: file_operators.len() as i64,
        distinct_operands: file_operands.len() as i64,
        total_operators: calc.sum_map(&file_operators),
        total_operands: calc.sum_map(&file_operands),
    };
    let d = calc.calculate(counts);

    Metrics {
        distinct_operators: counts.distinct_operators,
        distinct_operands: counts.distinct_operands,
        total_operators: counts.total_operators,
        total_operands: counts.total_operands,
        estimated_total_operators: est_total_ops,
        estimated_total_operands: est_total_opnds,
        vocabulary: d.vocabulary,
        length: d.length,
        estimated_length: d.estimated_length,
        volume: d.volume,
        difficulty: d.difficulty,
        effort: d.effort,
        time_to_program: d.time_to_program,
        delivered_bugs: d.delivered_bugs,
        functions: function_metrics,
    }
}

/// Builds the per-function report item map for one function
/// (`buildDetailedFunctionsTable` + `convertHalsteadFunctionItems`).
///
/// Keys mirror the Go `map[string]any` exactly; the encoder byte-sorts them. The
/// `_source_file` key is added only when a non-empty source file is supplied.
#[must_use]
pub fn function_report_item(
    formatter: &ReportFormatter,
    fnm: &FunctionHalsteadMetrics,
    source_file: Option<&str>,
) -> GoValue {
    let mut m = GoMap::new_map();
    m.push("name", GoValue::Str(fnm.name.clone()));
    m.push("volume", GoValue::Float(fnm.volume));
    m.push("difficulty", GoValue::Float(fnm.difficulty));
    m.push("effort", GoValue::Float(fnm.effort));
    m.push("time_to_program", GoValue::Float(fnm.time_to_program));
    m.push("delivered_bugs", GoValue::Float(fnm.delivered_bugs));
    m.push("distinct_operators", GoValue::Int(fnm.distinct_operators));
    m.push("distinct_operands", GoValue::Int(fnm.distinct_operands));
    m.push("total_operators", GoValue::Int(fnm.total_operators));
    m.push("total_operands", GoValue::Int(fnm.total_operands));
    m.push("vocabulary", GoValue::Int(fnm.vocabulary));
    m.push("length", GoValue::Int(fnm.length));
    m.push("estimated_length", GoValue::Float(fnm.estimated_length));
    m.push(
        "estimated_total_operators",
        GoValue::Int(fnm.estimated_total_operators),
    );
    m.push(
        "estimated_total_operands",
        GoValue::Int(fnm.estimated_total_operands),
    );
    m.push(
        "volume_assessment",
        GoValue::Str(formatter.volume_assessment(fnm.volume)),
    );
    m.push(
        "difficulty_assessment",
        GoValue::Str(formatter.difficulty_assessment(fnm.difficulty)),
    );
    m.push(
        "effort_assessment",
        GoValue::Str(formatter.effort_assessment(fnm.effort)),
    );
    m.push("operators", int_map_to_govalue(&fnm.operators));
    m.push("operands", int_map_to_govalue(&fnm.operands));
    if let Some(sf) = source_file.filter(|s| !s.is_empty()) {
        m.push("_source_file", GoValue::Str(sf.to_string()));
    }
    GoValue::Object(m)
}

/// Converts a `HashMap<String,i64>` into a dynamic (byte-sorted-on-encode)
/// `GoValue::Object`.
fn int_map_to_govalue(map: &HashMap<String, i64>) -> GoValue {
    let mut m = GoMap::new_map();
    for (k, v) in map {
        m.push(k, GoValue::Int(*v));
    }
    GoValue::Object(m)
}

/// Builds the final analysis result map for a non-empty analysis (`buildResult`).
///
/// The `functions` value is the list of per-function item maps in the order they
/// were produced (nondeterministic per the package contract). Top-level keys are
/// dynamic-map keys and are byte-sorted on encode.
#[must_use]
pub fn build_result(
    formatter: &ReportFormatter,
    file_metrics: &Metrics,
    message: &str,
) -> GoValue {
    let mut report = GoMap::new_map();
    report.push("analyzer_name", GoValue::Str(crate::ANALYZER_NAME.to_string()));
    report.push("volume", GoValue::Float(file_metrics.volume));
    report.push("difficulty", GoValue::Float(file_metrics.difficulty));
    report.push("effort", GoValue::Float(file_metrics.effort));
    report.push("time_to_program", GoValue::Float(file_metrics.time_to_program));
    report.push("delivered_bugs", GoValue::Float(file_metrics.delivered_bugs));
    report.push("distinct_operators", GoValue::Int(file_metrics.distinct_operators));
    report.push("distinct_operands", GoValue::Int(file_metrics.distinct_operands));
    report.push("total_operators", GoValue::Int(file_metrics.total_operators));
    report.push("total_operands", GoValue::Int(file_metrics.total_operands));
    report.push("vocabulary", GoValue::Int(file_metrics.vocabulary));
    report.push("length", GoValue::Int(file_metrics.length));
    report.push("estimated_length", GoValue::Float(file_metrics.estimated_length));
    report.push(
        "estimated_total_operators",
        GoValue::Int(file_metrics.estimated_total_operators),
    );
    report.push(
        "estimated_total_operands",
        GoValue::Int(file_metrics.estimated_total_operands),
    );
    report.push(
        "total_functions",
        GoValue::Int(file_metrics.functions.len() as i64),
    );
    let functions: Vec<GoValue> = file_metrics
        .functions
        .iter()
        .map(|fnm| function_report_item(formatter, fnm, None))
        .collect();
    report.push("functions", GoValue::Array(functions));
    report.push("message", GoValue::Str(message.to_string()));
    GoValue::Object(report)
}

/// Builds the empty-result map for analyses that found no functions
/// (`buildEmptyResult`). All numeric metrics are zero; the message is supplied.
#[must_use]
pub fn build_empty_result(message: &str) -> GoValue {
    let mut m = GoMap::new_map();
    m.push("total_functions", GoValue::Int(0));
    m.push("volume", GoValue::Float(0.0));
    m.push("difficulty", GoValue::Float(0.0));
    m.push("effort", GoValue::Float(0.0));
    m.push("time_to_program", GoValue::Float(0.0));
    m.push("delivered_bugs", GoValue::Float(0.0));
    m.push("distinct_operators", GoValue::Int(0));
    m.push("distinct_operands", GoValue::Int(0));
    m.push("total_operators", GoValue::Int(0));
    m.push("total_operands", GoValue::Int(0));
    m.push("vocabulary", GoValue::Int(0));
    m.push("length", GoValue::Int(0));
    m.push("estimated_length", GoValue::Float(0.0));
    m.push("message", GoValue::Str(message.to_string()));
    GoValue::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detector::test_support::TestNode;

    fn simple_function(name: &str) -> TestNode {
        TestNode::new("Function")
            .with_roles(&["Function", "Declaration"])
            .with_prop("name", name)
            .child(
                TestNode::new("Assignment")
                    .with_token("=")
                    .with_roles(&["Assignment"])
                    .with_prop("operator", "=")
                    .child(
                        TestNode::new("Identifier")
                            .with_token("x")
                            .with_roles(&["Variable"])
                            .with_prop("name", "x"),
                    )
                    .child(
                        TestNode::new("Literal")
                            .with_token("5")
                            .with_roles(&["Literal"])
                            .with_prop("value", "5"),
                    ),
            )
    }

    #[test]
    fn function_metrics_positive() {
        let detector = OperatorOperandDetector::new();
        let calc = MetricsCalculator::new();
        let mut fnm = calculate_function_metrics(&detector, &calc, &simple_function("f"));
        fnm.name = "f".to_string();
        assert!(fnm.volume > 0.0);
        assert!(fnm.difficulty > 0.0);
        assert!(fnm.effort > 0.0);
        assert_eq!(fnm.distinct_operators, 1); // "="
        assert_eq!(fnm.distinct_operands, 2); // x, 5
        assert!(!fnm.cms_active, "small function must not activate CMS");
    }

    #[test]
    fn file_level_aggregates_two_functions() {
        let detector = OperatorOperandDetector::new();
        let calc = MetricsCalculator::new();
        let mut f1 = calculate_function_metrics(&detector, &calc, &simple_function("a"));
        f1.name = "a".to_string();
        let mut f2 = calculate_function_metrics(&detector, &calc, &simple_function("b"));
        f2.name = "b".to_string();
        let file = calculate_file_level_metrics(&calc, vec![f1, f2]);
        assert_eq!(file.functions.len(), 2);
        // 2x "=" total operators, distinct = 1; operands x(2),5(2) distinct = 2.
        assert_eq!(file.distinct_operators, 1);
        assert_eq!(file.total_operators, 2);
        assert_eq!(file.distinct_operands, 2);
        assert_eq!(file.total_operands, 4);
        assert!(file.volume > 0.0);
    }

    /// Large function (>= threshold tokens) activates the CMS path and the exact
    /// total equals the CMS total. Mirrors `TestVisitor_CMSTotalMatchesExact`.
    #[test]
    fn large_function_activates_cms() {
        let detector = OperatorOperandDetector::new();
        let calc = MetricsCalculator::new();

        // Build a function with 2000 alternating operator/operand children.
        let mut function = TestNode::new("Function").with_roles(&["Function", "Declaration"]);
        function = function.child(
            TestNode::new("Identifier")
                .with_token("big")
                .with_roles(&["Name"])
                .with_prop("name", "big"),
        );
        for i in 0..2000 {
            if i % 2 == 0 {
                function = function.child(
                    TestNode::new("BinaryOp")
                        .with_roles(&["Operator"])
                        .with_prop("operator", &format!("op{}", i % 10)),
                );
            } else {
                function = function.child(
                    TestNode::new("Identifier")
                        .with_token(&format!("var{}", i % 20))
                        .with_roles(&["Variable"]),
                );
            }
        }

        let fnm = calculate_function_metrics(&detector, &calc, &function);
        assert!(fnm.cms_active, "large function must activate CMS");
        assert_eq!(fnm.estimated_total_operators, fnm.total_operators);
        assert_eq!(fnm.estimated_total_operands, fnm.total_operands);
        assert!(fnm.estimated_total_operators > 0);
        assert!(fnm.estimated_total_operands > 0);
    }

    #[test]
    fn empty_result_has_zero_total_functions() {
        let v = build_empty_result("No functions found");
        let GoValue::Object(map) = v else {
            panic!("expected object")
        };
        assert_eq!(map.get("total_functions"), Some(&GoValue::Int(0)));
        assert_eq!(
            map.get("message"),
            Some(&GoValue::Str("No functions found".to_string()))
        );
    }
}
