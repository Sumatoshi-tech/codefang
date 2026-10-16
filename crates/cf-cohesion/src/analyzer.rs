//! The cohesion [`Analyzer`] and its per-file [`FileReport`].

use std::collections::BTreeSet;

use cf_uast_node::Node;

use crate::metrics::{
    COHESION_THRESHOLD_EXCELLENT, COHESION_THRESHOLD_FAIR, COHESION_THRESHOLD_GOOD,
};
use crate::report_value::{Report, ReportValue};
use crate::{calc, visitor};

// --- Per-file message thresholds on `cohesion_score` ---

const MESSAGE_THRESHOLD_EXCELLENT: f64 = 0.7;
const MESSAGE_THRESHOLD_GOOD: f64 = 0.4;
const MESSAGE_THRESHOLD_FAIR: f64 = 0.3;

// --- Per-function assessment thresholds ---

const VARIABLES_FEW_MAX: usize = 3;
const VARIABLES_MODERATE_MAX: usize = 7;
const LINES_SMALL_MAX: u64 = 10;
const LINES_MEDIUM_MAX: u64 = 30;

/// Message of a file without functions.
pub const EMPTY_FILE_MESSAGE: &str = "No functions found";

/// A function and the distinct variables it uses.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Function {
    /// Function name, qualified `Type.name` when the UAST gives a receiver or
    /// enclosing class.
    pub name: String,
    /// Distinct variable names used by the function.
    pub variables: BTreeSet<String>,
    /// 1-based first source line (`0` without position).
    pub start_line: u64,
    /// 1-based last source line (`0` without position).
    pub end_line: u64,
}

impl Function {
    /// A function with the given distinct variables and no position.
    #[must_use]
    pub fn with_variables(name: &str, variables: &[&str]) -> Self {
        Function {
            name: name.to_string(),
            variables: variables.iter().map(|v| (*v).to_string()).collect(),
            start_line: 0,
            end_line: 0,
        }
    }

    /// Source lines spanned by the function node (`end - start + 1`).
    #[must_use]
    pub fn line_count(&self) -> u64 {
        self.end_line.saturating_sub(self.start_line) + 1
    }
}

/// Cohesion metrics of one function.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionMetrics {
    /// Function name (see [`Function::name`]).
    pub name: String,
    /// 1-based first source line.
    pub start_line: u64,
    /// Source lines spanned by the function.
    pub line_count: u64,
    /// Number of distinct variables used.
    pub variable_count: usize,
    /// Per-function cohesion: shared variables over distinct variables.
    pub cohesion: f64,
}

impl FunctionMetrics {
    /// Cohesion assessment label with its traffic-light emoji.
    #[must_use]
    pub fn cohesion_assessment(&self) -> &'static str {
        if self.cohesion >= COHESION_THRESHOLD_EXCELLENT {
            "\u{1F7E2} Excellent"
        } else if self.cohesion >= COHESION_THRESHOLD_GOOD {
            "\u{1F7E1} Good"
        } else if self.cohesion >= COHESION_THRESHOLD_FAIR {
            "\u{1F7E1} Fair"
        } else {
            "\u{1F534} Poor"
        }
    }

    /// Variable-count assessment label.
    #[must_use]
    pub fn variable_assessment(&self) -> &'static str {
        if self.variable_count <= VARIABLES_FEW_MAX {
            "\u{1F7E2} Few"
        } else if self.variable_count <= VARIABLES_MODERATE_MAX {
            "\u{1F7E1} Moderate"
        } else {
            "\u{1F534} Many"
        }
    }

    /// Size assessment label.
    #[must_use]
    pub fn size_assessment(&self) -> &'static str {
        if self.line_count <= LINES_SMALL_MAX {
            "\u{1F7E2} Small"
        } else if self.line_count <= LINES_MEDIUM_MAX {
            "\u{1F7E1} Medium"
        } else {
            "\u{1F534} Large"
        }
    }
}

/// Cohesion of one source file.
#[derive(Debug, Clone, PartialEq)]
pub struct FileReport {
    /// Number of functions found.
    pub total_functions: usize,
    /// LCOM-HS of the file (see [`calc::lcom`]).
    pub lcom: f64,
    /// `1 - lcom`.
    pub cohesion_score: f64,
    /// Mean per-function cohesion.
    pub function_cohesion: f64,
    /// Per-function metrics, in discovery order.
    pub functions: Vec<FunctionMetrics>,
    /// Human-readable summary (see [`file_message`]).
    pub message: String,
}

impl FileReport {
    /// The report of a file without functions: `lcom = 0`, perfect scores,
    /// [`EMPTY_FILE_MESSAGE`].
    #[must_use]
    pub fn empty() -> Self {
        FileReport {
            total_functions: 0,
            lcom: 0.0,
            cohesion_score: calc::PERFECT_COHESION,
            function_cohesion: calc::PERFECT_COHESION,
            functions: Vec::new(),
            message: EMPTY_FILE_MESSAGE.to_string(),
        }
    }

    /// True when the file holds at least one function.
    #[must_use]
    pub fn has_functions(&self) -> bool {
        self.total_functions > 0
    }

    /// The scalar view consumed by the history `quality` analyzer
    /// (`cf_quality::analyze_file`).
    #[must_use]
    pub fn to_report(&self) -> Report {
        let mut r = Report::new();
        r.insert(
            "total_functions".into(),
            ReportValue::Int(self.total_functions as i64),
        );
        r.insert("lcom".into(), ReportValue::Float(self.lcom));
        r.insert(
            "cohesion_score".into(),
            ReportValue::Float(self.cohesion_score),
        );
        r.insert(
            "function_cohesion".into(),
            ReportValue::Float(self.function_cohesion),
        );
        r.insert("message".into(), ReportValue::Str(self.message.clone()));
        r
    }
}

/// The cohesion analyzer. Stateless; construct via [`Analyzer::new`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Analyzer;

impl Analyzer {
    /// Creates a new analyzer.
    #[must_use]
    pub fn new() -> Self {
        Analyzer
    }

    /// Analyzes one file's UAST.
    #[must_use]
    pub fn analyze_file(&self, root: &Node) -> FileReport {
        self.analyze_file_capped(root, None)
    }

    /// [`analyze_file`](Self::analyze_file) discovering only function nodes
    /// at depth `<= cap` below `root` (root at depth 0) when `find_depth` is
    /// `Some(cap)`.
    #[must_use]
    pub fn analyze_file_capped(&self, root: &Node, find_depth: Option<usize>) -> FileReport {
        let functions = visitor::collect_functions(root, find_depth);
        if functions.is_empty() {
            return FileReport::empty();
        }
        let cohesions = calc::function_cohesions(&functions);
        let metrics: Vec<FunctionMetrics> = functions
            .iter()
            .zip(&cohesions)
            .map(|(f, &cohesion)| FunctionMetrics {
                name: f.name.clone(),
                start_line: f.start_line,
                line_count: f.line_count(),
                variable_count: f.variables.len(),
                cohesion,
            })
            .collect();
        let lcom = calc::lcom(&functions);
        let cohesion_score = calc::cohesion_score(lcom);
        FileReport {
            total_functions: metrics.len(),
            lcom,
            cohesion_score,
            function_cohesion: calc::mean_or(&cohesions, calc::PERFECT_COHESION),
            functions: metrics,
            message: file_message(cohesion_score).to_string(),
        }
    }
}

/// Per-file summary keyed on `cohesion_score`.
#[must_use]
pub fn file_message(score: f64) -> &'static str {
    if score >= MESSAGE_THRESHOLD_EXCELLENT {
        "Excellent cohesion - functions are well-focused and cohesive"
    } else if score >= MESSAGE_THRESHOLD_GOOD {
        "Good cohesion - functions have reasonable focus"
    } else if score >= MESSAGE_THRESHOLD_FAIR {
        "Fair cohesion - some functions could be more focused"
    } else {
        "Poor cohesion - functions lack focus and should be refactored"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(cohesion: f64, variable_count: usize, line_count: u64) -> FunctionMetrics {
        FunctionMetrics {
            name: "f".into(),
            start_line: 1,
            line_count,
            variable_count,
            cohesion,
        }
    }

    #[test]
    fn line_count_is_inclusive_span() {
        let f = Function {
            start_line: 3,
            end_line: 7,
            ..Function::default()
        };
        assert_eq!(f.line_count(), 5);
        assert_eq!(Function::default().line_count(), 1);
    }

    #[test]
    fn assessments_follow_the_documented_thresholds() {
        assert_eq!(
            metrics(0.6, 0, 1).cohesion_assessment(),
            "\u{1F7E2} Excellent"
        );
        assert_eq!(metrics(0.4, 0, 1).cohesion_assessment(), "\u{1F7E1} Good");
        assert_eq!(metrics(0.3, 0, 1).cohesion_assessment(), "\u{1F7E1} Fair");
        assert_eq!(metrics(0.29, 0, 1).cohesion_assessment(), "\u{1F534} Poor");
        assert_eq!(metrics(1.0, 3, 1).variable_assessment(), "\u{1F7E2} Few");
        assert_eq!(
            metrics(1.0, 7, 1).variable_assessment(),
            "\u{1F7E1} Moderate"
        );
        assert_eq!(metrics(1.0, 8, 1).variable_assessment(), "\u{1F534} Many");
        assert_eq!(metrics(1.0, 0, 10).size_assessment(), "\u{1F7E2} Small");
        assert_eq!(metrics(1.0, 0, 30).size_assessment(), "\u{1F7E1} Medium");
        assert_eq!(metrics(1.0, 0, 31).size_assessment(), "\u{1F534} Large");
    }

    #[test]
    fn file_messages_follow_the_documented_thresholds() {
        assert!(file_message(0.7).starts_with("Excellent"));
        assert!(file_message(0.4).starts_with("Good"));
        assert!(file_message(0.3).starts_with("Fair"));
        assert!(file_message(0.1).starts_with("Poor"));
    }

    #[test]
    fn empty_report_scores_perfectly_and_round_trips() {
        let empty = FileReport::empty();
        assert!(!empty.has_functions());
        let report = empty.to_report();
        assert_eq!(report.get("total_functions"), Some(&ReportValue::Int(0)));
        assert_eq!(
            report.get("cohesion_score").and_then(ReportValue::as_float),
            Some(1.0)
        );
        assert_eq!(
            report.get("message"),
            Some(&ReportValue::Str(EMPTY_FILE_MESSAGE.into()))
        );
    }
}
