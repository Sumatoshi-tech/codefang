//! Static complexity analyzer (cyclomatic / cognitive / nesting).
//!
//! Walks a UAST tree, computes per-function cyclomatic complexity, cognitive
//! complexity (`SonarSource` model), nesting depth, decision points and lines
//! of code, and returns a deterministic result. The counting rules live in
//! [`metrics`](crate::metrics) (module docs), discovery and naming in
//! [`discover`](crate::discover), every threshold in [`thresholds`], and the
//! multi-file aggregate view in [`report`].
//!
//! Pass the file's source bytes through [`AnalysisOptions::source`] whenever
//! you have them: operators, identifier names and lines of code are recovered
//! from the source when the UAST does not carry them.
//!
//! # Output shape
//!
//! [`Analyzer::analyze`] returns a map-origin [`cf_gojson::GoValue`] object
//! with `analyzer_name`, `total_functions`, `average_complexity`,
//! `max_complexity`, `total_complexity`, `cognitive_complexity` (sum),
//! `nesting_depth` (max), `decision_points`, `functions` (one object per
//! function with the metric keys plus the assessment strings) and `message`.
//! A missing root or a tree with no functions yields the empty shape
//! (`total_functions`, `average_complexity`, `max_complexity`,
//! `total_complexity`, `message`).
//!
//! # Example
//!
//! ```
//! use cf_complexity::Analyzer;
//! use cf_complexity::node::{uast, role, Node};
//! use cf_gojson::GoValue;
//!
//! // function foo() { if (...) { ... } }
//! let body = Node::new(uast::BLOCK)
//!     .with_roles(vec![role::BODY])
//!     .with_children(vec![Node::new(uast::IF)]);
//! let func = Node::new(uast::FUNCTION)
//!     .with_roles(vec![role::FUNCTION, role::DECLARATION])
//!     .with_prop("name", "foo")
//!     .with_children(vec![body]);
//! let root = Node::new(uast::FILE).with_children(vec![func]);
//!
//! let report = Analyzer.analyze(Some(&root));
//! let m = report.as_map().unwrap();
//! assert_eq!(m.get("total_functions"), Some(&GoValue::Int(1)));
//! // cyclomatic = 1 (base) + 1 (the `if`) = 2.
//! assert_eq!(m.get("total_complexity"), Some(&GoValue::Int(2)));
//! assert_eq!(m.get("cognitive_complexity"), Some(&GoValue::Int(1)));
//! assert_eq!(m.get("analyzer_name").unwrap().as_str(), Some("complexity"));
//! ```
//!
//! A missing root (or a tree with no functions) yields the empty-result shape:
//!
//! ```
//! use cf_complexity::Analyzer;
//! use cf_gojson::GoValue;
//!
//! let report = Analyzer.analyze(None);
//! let m = report.as_map().unwrap();
//! assert_eq!(m.get("total_functions"), Some(&GoValue::Int(0)));
//! assert_eq!(m.get("message").unwrap().as_str(), Some("No AST provided"));
//! assert!(m.get("functions").is_none());
//! ```

// Metric counts are far below the f64 mantissa / i64 range; the pedantic
// cast lints add noise without value here.
#![allow(clippy::cast_possible_wrap, clippy::cast_precision_loss)]

mod discover;
mod metrics;
pub mod node;
pub mod report;
pub mod thresholds;

use std::cmp::Ordering;

use cf_gojson::{GoMap, GoValue, MapOrigin};
use node::Node;

/// The complexity analyzer. Stateless: all configuration is fixed by the
/// report contract.
#[derive(Debug, Default, Clone, Copy)]
pub struct Analyzer;

/// Per-function complexity metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionMetrics {
    /// Function name (or `"anonymous"`).
    pub name: String,
    /// Cyclomatic complexity (1 + decision points).
    pub cyclomatic_complexity: i64,
    /// Cognitive complexity (`SonarSource`, nesting-weighted).
    pub cognitive_complexity: i64,
    /// Maximum nesting depth.
    pub nesting_depth: i64,
    /// Decision points (`cyclomatic - 1`).
    pub decision_points: i64,
    /// Lines of code (non-blank, non-comment-only lines of the span).
    pub lines_of_code: i64,
    /// 1-based start line (0 when the node has no position).
    pub line: i64,
}

/// Optional inputs of an analysis run.
#[derive(Debug, Default, Clone, Copy)]
pub struct AnalysisOptions<'a> {
    /// The analyzed file's bytes: enables operator, name and line-of-code
    /// recovery from the source.
    pub source: Option<&'a [u8]>,
    /// Discovery depth cap: only functions at depth `<= cap` below the root
    /// (root at depth 0) are found. The history `quality` analyzer caps at
    /// 10; the static surfaces run uncapped.
    pub find_depth: Option<usize>,
}

const ANALYZER_NAME: &str = "complexity";
const MSG_NO_AST: &str = "No AST provided";
const MSG_NO_FUNCTIONS: &str = "No functions found";

impl Analyzer {
    /// Analyzer name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        ANALYZER_NAME
    }

    /// CLI flag.
    #[must_use]
    pub fn flag(&self) -> &'static str {
        "complexity-analysis"
    }

    /// Performs complexity analysis without source text and returns the
    /// report object as a [`GoValue`].
    #[must_use]
    pub fn analyze(&self, root: Option<&Node>) -> GoValue {
        self.analyze_with(root, &AnalysisOptions::default())
    }

    /// [`analyze`](Self::analyze) with a discovery depth cap (see
    /// [`AnalysisOptions::find_depth`]).
    #[must_use]
    pub fn analyze_with_find_depth(
        &self,
        root: Option<&Node>,
        find_depth: Option<usize>,
    ) -> GoValue {
        self.analyze_with(
            root,
            &AnalysisOptions {
                source: None,
                find_depth,
            },
        )
    }

    /// Performs complexity analysis with the given options.
    #[must_use]
    pub fn analyze_with(&self, root: Option<&Node>, opts: &AnalysisOptions) -> GoValue {
        let Some(root) = root else {
            return build_empty_result(MSG_NO_AST);
        };
        let functions = self.function_metrics_with(Some(root), opts);
        if functions.is_empty() {
            return build_empty_result(MSG_NO_FUNCTIONS);
        }
        build_result(&functions)
    }

    /// Computes per-function metrics without source text, in
    /// [`compare_metrics`] order. A `None` root yields an empty vector.
    ///
    /// ```
    /// use cf_complexity::Analyzer;
    /// use cf_complexity::node::{uast, role, Node};
    ///
    /// let body = |children| Node::new(uast::BLOCK).with_roles(vec![role::BODY]).with_children(children);
    /// let simple = Node::new(uast::FUNCTION)
    ///     .with_roles(vec![role::FUNCTION, role::DECLARATION])
    ///     .with_prop("name", "simple")
    ///     .with_children(vec![body(vec![])]);
    /// let branchy = Node::new(uast::FUNCTION)
    ///     .with_roles(vec![role::FUNCTION, role::DECLARATION])
    ///     .with_prop("name", "branchy")
    ///     .with_children(vec![body(vec![Node::new(uast::IF)])]);
    /// let root = Node::new(uast::FILE).with_children(vec![simple, branchy]);
    ///
    /// let metrics = Analyzer.function_metrics(Some(&root));
    /// assert_eq!(metrics[0].name, "branchy");
    /// assert_eq!(metrics[0].cyclomatic_complexity, 2);
    /// assert_eq!(metrics[1].name, "simple");
    /// assert_eq!(metrics[1].cyclomatic_complexity, 1);
    /// assert!(Analyzer.function_metrics(None).is_empty());
    /// ```
    #[must_use]
    pub fn function_metrics(&self, root: Option<&Node>) -> Vec<FunctionMetrics> {
        self.function_metrics_with(root, &AnalysisOptions::default())
    }

    /// Computes per-function metrics with the given options, in
    /// [`compare_metrics`] order.
    #[must_use]
    pub fn function_metrics_with(
        &self,
        root: Option<&Node>,
        opts: &AnalysisOptions,
    ) -> Vec<FunctionMetrics> {
        let Some(root) = root else {
            return Vec::new();
        };
        let index = opts.source.map(metrics::LineIndex::new);
        let mut out: Vec<FunctionMetrics> =
            discover::find_functions(root, opts.source, opts.find_depth)
                .into_iter()
                .map(|fn_node| function_metrics_of(fn_node, opts.source, index.as_ref()))
                .collect();
        out.sort_by(compare_metrics);
        out
    }
}

fn function_metrics_of(
    fn_node: &Node,
    source: Option<&[u8]>,
    index: Option<&metrics::LineIndex>,
) -> FunctionMetrics {
    let name = discover::function_name(fn_node, source);
    let measured = metrics::measure(fn_node, &name, source);
    FunctionMetrics {
        name,
        cyclomatic_complexity: measured.cyclomatic,
        cognitive_complexity: measured.cognitive,
        nesting_depth: measured.nesting,
        decision_points: (measured.cyclomatic - 1).max(0),
        lines_of_code: metrics::lines_of_code(fn_node, source, index),
        line: fn_node.pos.as_ref().map_or(0, |p| i64::from(p.start_line)),
    }
}

/// The per-file ordering: cyclomatic descending, cognitive descending,
/// nesting descending, then name and start line ascending.
#[must_use]
pub fn compare_metrics(a: &FunctionMetrics, b: &FunctionMetrics) -> Ordering {
    b.cyclomatic_complexity
        .cmp(&a.cyclomatic_complexity)
        .then(b.cognitive_complexity.cmp(&a.cognitive_complexity))
        .then(b.nesting_depth.cmp(&a.nesting_depth))
        .then_with(|| a.name.cmp(&b.name))
        .then(a.line.cmp(&b.line))
}

fn build_empty_result(message: &str) -> GoValue {
    let mut m = GoMap::new(MapOrigin::Map);
    m.insert("total_functions", GoValue::Int(0));
    m.insert("average_complexity", GoValue::Float(0.0));
    m.insert("max_complexity", GoValue::Int(0));
    m.insert("total_complexity", GoValue::Int(0));
    m.insert("message", GoValue::Str(message.to_string()));
    GoValue::Map(m)
}

/// One `functions` entry: the metric keys plus the assessment strings.
fn function_value(fm: &FunctionMetrics) -> GoValue {
    let mut fo = GoMap::new(MapOrigin::Map);
    fo.insert("name", GoValue::Str(fm.name.clone()));
    fo.insert(
        "cyclomatic_complexity",
        GoValue::Int(fm.cyclomatic_complexity),
    );
    fo.insert(
        "cognitive_complexity",
        GoValue::Int(fm.cognitive_complexity),
    );
    fo.insert("nesting_depth", GoValue::Int(fm.nesting_depth));
    fo.insert("lines_of_code", GoValue::Int(fm.lines_of_code));
    fo.insert(
        "complexity_assessment",
        GoValue::Str(thresholds::complexity_assessment(fm.cyclomatic_complexity).to_string()),
    );
    fo.insert(
        "cognitive_assessment",
        GoValue::Str(thresholds::cognitive_assessment(fm.cognitive_complexity).to_string()),
    );
    fo.insert(
        "nesting_assessment",
        GoValue::Str(thresholds::nesting_assessment(fm.nesting_depth).to_string()),
    );
    GoValue::Map(fo)
}

fn build_result(functions: &[FunctionMetrics]) -> GoValue {
    let inputs: Vec<report::FunctionInput> = functions
        .iter()
        .map(|m| report::FunctionInput::stamped(m, "", "", ""))
        .collect();
    let agg = report::aggregate(&inputs);

    let mut m = GoMap::new(MapOrigin::Map);
    m.insert("analyzer_name", GoValue::Str(ANALYZER_NAME.to_string()));
    m.insert("total_functions", GoValue::Int(agg.total_functions));
    m.insert("average_complexity", GoValue::Float(agg.average_complexity));
    m.insert("max_complexity", GoValue::Int(agg.max_complexity));
    m.insert("total_complexity", GoValue::Int(agg.total_complexity));
    m.insert(
        "cognitive_complexity",
        GoValue::Int(agg.cognitive_complexity),
    );
    m.insert("nesting_depth", GoValue::Int(agg.nesting_depth));
    m.insert("decision_points", GoValue::Int(agg.decision_points));
    m.insert(
        "functions",
        GoValue::Array(functions.iter().map(function_value).collect()),
    );
    m.insert("message", GoValue::Str(agg.message.to_string()));
    GoValue::Map(m)
}

#[cfg(test)]
mod tests {
    use super::{AnalysisOptions, Analyzer};
    use crate::node::{role, uast, Node};
    use cf_gojson::GoValue;

    fn obj_get<'a>(v: &'a GoValue, key: &str) -> &'a GoValue {
        v.as_map()
            .and_then(|m| m.get(key))
            .unwrap_or_else(|| panic!("missing key {key}"))
    }

    fn as_int(v: &GoValue) -> i64 {
        match v {
            GoValue::Int(i) => *i,
            other => panic!("not an int: {other:?}"),
        }
    }

    fn body(children: Vec<Node>) -> Node {
        Node::new(uast::BLOCK)
            .with_roles(vec![role::BODY])
            .with_children(children)
    }

    fn func(name: &str, children: Vec<Node>) -> Node {
        Node::new(uast::FUNCTION)
            .with_roles(vec![role::FUNCTION, role::DECLARATION])
            .with_prop("name", name)
            .with_children(vec![body(children)])
    }

    #[test]
    fn analyzer_identity() {
        assert_eq!(Analyzer.name(), "complexity");
        assert_eq!(Analyzer.flag(), "complexity-analysis");
    }

    #[test]
    fn nil_root_and_no_functions_use_the_empty_shape() {
        let none = Analyzer.analyze(None);
        assert_eq!(as_int(obj_get(&none, "total_functions")), 0);
        assert_eq!(obj_get(&none, "message").as_str(), Some("No AST provided"));

        let root = Node::new(uast::FILE);
        let empty = Analyzer.analyze(Some(&root));
        assert_eq!(
            obj_get(&empty, "message").as_str(),
            Some("No functions found")
        );
        assert!(empty.as_map().unwrap().get("functions").is_none());
    }

    #[test]
    fn result_shape_carries_sums_and_assessments() {
        let nested = Node::new(uast::IF).with_children(vec![
            Node::new(uast::IDENTIFIER),
            body(vec![Node::new(uast::IF)]),
        ]);
        let root = Node::new(uast::FILE).with_children(vec![
            func("a", vec![nested]),
            func("b", vec![Node::new(uast::LOOP)]),
        ]);
        let result = Analyzer.analyze(Some(&root));
        for key in [
            "analyzer_name",
            "total_functions",
            "average_complexity",
            "max_complexity",
            "total_complexity",
            "cognitive_complexity",
            "nesting_depth",
            "decision_points",
            "functions",
            "message",
        ] {
            assert!(result.as_map().unwrap().contains_key(key), "missing {key}");
        }
        assert_eq!(as_int(obj_get(&result, "total_functions")), 2);
        // a: cc 3 (two ifs), cog 1 + 2 = 3, nesting 2; b: cc 2, cog 1, nesting 1.
        assert_eq!(as_int(obj_get(&result, "total_complexity")), 5);
        assert_eq!(as_int(obj_get(&result, "cognitive_complexity")), 4);
        assert_eq!(as_int(obj_get(&result, "nesting_depth")), 2);
        assert_eq!(as_int(obj_get(&result, "max_complexity")), 3);
        assert_eq!(as_int(obj_get(&result, "decision_points")), 3);
        let GoValue::Array(functions) = obj_get(&result, "functions") else {
            panic!("functions is not an array")
        };
        assert_eq!(
            functions[0].as_map().unwrap().get("name").unwrap().as_str(),
            Some("a")
        );
        for key in [
            "complexity_assessment",
            "cognitive_assessment",
            "nesting_assessment",
        ] {
            assert!(
                functions[0].as_map().unwrap().contains_key(key),
                "missing {key}"
            );
        }
    }

    #[test]
    fn functions_sort_by_cyclomatic_cognitive_nesting_then_name() {
        let root = Node::new(uast::FILE).with_children(vec![
            func("zeta", vec![Node::new(uast::IF)]),
            func("alpha", vec![Node::new(uast::IF)]),
            func("beta", vec![]),
        ]);
        let names: Vec<String> = Analyzer
            .function_metrics(Some(&root))
            .into_iter()
            .map(|m| m.name)
            .collect();
        assert_eq!(names, vec!["alpha", "zeta", "beta"]);
    }

    #[test]
    fn find_depth_caps_discovery_for_the_quality_analyzer() {
        let deep = Node::new(uast::BLOCK).with_children(vec![
            Node::new(uast::BLOCK).with_children(vec![func("deep", vec![])])
        ]);
        let root = Node::new(uast::FILE).with_children(vec![func("top", vec![]), deep]);
        let capped = Analyzer.analyze_with_find_depth(Some(&root), Some(1));
        assert_eq!(as_int(obj_get(&capped, "total_functions")), 1);
        let uncapped = Analyzer.analyze_with(Some(&root), &AnalysisOptions::default());
        assert_eq!(as_int(obj_get(&uncapped, "total_functions")), 2);
    }
}
