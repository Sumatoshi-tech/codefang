//! Cross-file aggregation.
//!
//! Every function of every analyzed file is kept (no de-duplication by name);
//! files without functions carry no cohesion data and are excluded from every
//! denominator. `lcom` is the function-weighted mean of the per-file LCOM-HS
//! values (a variable is a per-file notion, so LCOM is never recomputed over
//! the union), `cohesion_score = 1 - lcom`, and `function_cohesion` is the
//! mean per-function cohesion over all functions.

use crate::analyzer::FileReport;
use crate::calc;

/// Overall message when no analyzed file has a function.
pub const NO_DATA_MESSAGE: &str = "No cohesion data available";

const MESSAGE_THRESHOLD_EXCELLENT: f64 = 0.7;
const MESSAGE_THRESHOLD_GOOD: f64 = 0.4;
const MESSAGE_THRESHOLD_FAIR: f64 = 0.3;

/// One analyzed file: its path relative to the analyzed root and its report.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalyzedFile {
    /// Path relative to the analyzed root.
    pub path: String,
    /// The file's cohesion report.
    pub report: FileReport,
}

/// One function in the cross-file table.
#[derive(Debug, Clone, PartialEq)]
pub struct AggregatedFunction {
    /// Source file (relative path).
    pub source_file: String,
    /// Function name.
    pub name: String,
    /// 1-based first source line.
    pub start_line: u64,
    /// Source lines spanned.
    pub line_count: u64,
    /// Distinct variables used.
    pub variable_count: usize,
    /// Per-function cohesion.
    pub cohesion: f64,
}

/// The cross-file aggregate.
#[derive(Debug, Clone, PartialEq)]
pub struct Aggregate {
    /// Files that were parsed and analyzed.
    pub files_analyzed: usize,
    /// Files with at least one function (the data-bearing files).
    pub files_with_functions: usize,
    /// Number of functions across all files.
    pub total_functions: usize,
    /// Function-weighted mean of the per-file LCOM-HS.
    pub lcom: f64,
    /// `1 - lcom`.
    pub cohesion_score: f64,
    /// Mean per-function cohesion over all functions.
    pub function_cohesion: f64,
    /// Every function, in walk order (file, then source position).
    pub functions: Vec<AggregatedFunction>,
    /// Overall message (see [`overall_message`]); [`NO_DATA_MESSAGE`] when
    /// there are no functions.
    pub message: String,
}

impl Aggregate {
    /// The aggregate of `files_analyzed` files none of which has a function.
    #[must_use]
    pub fn no_data(files_analyzed: usize) -> Self {
        Aggregate {
            files_analyzed,
            files_with_functions: 0,
            total_functions: 0,
            lcom: 0.0,
            cohesion_score: 0.0,
            function_cohesion: 0.0,
            functions: Vec::new(),
            message: NO_DATA_MESSAGE.to_string(),
        }
    }

    /// True when at least one function was found.
    #[must_use]
    pub fn has_data(&self) -> bool {
        self.total_functions > 0
    }
}

/// Aggregates per-file reports (see the module docs for the semantics).
#[must_use]
pub fn aggregate(files: &[AnalyzedFile]) -> Aggregate {
    let mut functions: Vec<AggregatedFunction> = Vec::new();
    let mut weighted_lcom = 0.0;
    let mut files_with_functions = 0;
    for file in files {
        if !file.report.has_functions() {
            continue;
        }
        files_with_functions += 1;
        weighted_lcom += file.report.lcom * file.report.total_functions as f64;
        functions.extend(file.report.functions.iter().map(|f| AggregatedFunction {
            source_file: file.path.clone(),
            name: f.name.clone(),
            start_line: f.start_line,
            line_count: f.line_count,
            variable_count: f.variable_count,
            cohesion: f.cohesion,
        }));
    }
    if functions.is_empty() {
        return Aggregate::no_data(files.len());
    }
    let total = functions.len();
    let lcom = weighted_lcom / total as f64;
    let cohesions: Vec<f64> = functions.iter().map(|f| f.cohesion).collect();
    let cohesion_score = calc::cohesion_score(lcom);
    Aggregate {
        files_analyzed: files.len(),
        files_with_functions,
        total_functions: total,
        lcom,
        cohesion_score,
        function_cohesion: calc::mean_or(&cohesions, 0.0),
        functions,
        message: overall_message(cohesion_score).to_string(),
    }
}

/// Overall message keyed on the aggregate `cohesion_score`.
#[must_use]
pub fn overall_message(score: f64) -> &'static str {
    if score >= MESSAGE_THRESHOLD_EXCELLENT {
        "Excellent overall cohesion across all analyzed code"
    } else if score >= MESSAGE_THRESHOLD_GOOD {
        "Good overall cohesion with room for improvement"
    } else if score >= MESSAGE_THRESHOLD_FAIR {
        "Fair overall cohesion - consider refactoring some functions"
    } else {
        "Poor overall cohesion - significant refactoring recommended"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyzer::FunctionMetrics;

    fn file(path: &str, lcom: f64, cohesions: &[f64]) -> AnalyzedFile {
        let functions: Vec<FunctionMetrics> = cohesions
            .iter()
            .enumerate()
            .map(|(i, c)| FunctionMetrics {
                name: format!("f{i}"),
                start_line: i as u64 + 1,
                line_count: 1,
                variable_count: 1,
                cohesion: *c,
            })
            .collect();
        AnalyzedFile {
            path: path.to_string(),
            report: FileReport {
                total_functions: functions.len(),
                lcom,
                cohesion_score: 1.0 - lcom,
                function_cohesion: calc::mean_or(cohesions, 1.0),
                functions,
                message: String::new(),
            },
        }
    }

    #[test]
    fn empty_input_is_no_data() {
        let agg = aggregate(&[]);
        assert!(!agg.has_data());
        assert_eq!(agg.message, NO_DATA_MESSAGE);
        assert_eq!(agg.cohesion_score, 0.0);
    }

    #[test]
    fn function_free_files_are_excluded_from_every_denominator() {
        let files = [
            file("poor.go", 0.5, &[0.0, 0.0]),
            file("t1.go", 0.0, &[]),
            file("t2.go", 0.0, &[]),
        ];
        let agg = aggregate(&files);
        assert_eq!(agg.files_analyzed, 3);
        assert_eq!(agg.files_with_functions, 1);
        assert_eq!(agg.total_functions, 2);
        assert!((agg.lcom - 0.5).abs() < 1e-9, "lcom {}", agg.lcom);
        assert!((agg.cohesion_score - 0.5).abs() < 1e-9);
        assert_eq!(agg.function_cohesion, 0.0);
    }

    #[test]
    fn scalars_are_function_weighted() {
        // 3 functions at LCOM 1/3, 2 functions at LCOM 0.5.
        let files = [
            file("a.go", 1.0 / 3.0, &[1.0, 1.0, 2.0 / 3.0]),
            file("b.go", 0.5, &[0.0, 0.0]),
        ];
        let agg = aggregate(&files);
        assert_eq!(agg.total_functions, 5);
        assert!((agg.lcom - 0.4).abs() < 1e-9, "lcom {}", agg.lcom);
        assert!((agg.cohesion_score - 0.6).abs() < 1e-9);
        assert!((agg.function_cohesion - (2.0 + 2.0 / 3.0) / 5.0).abs() < 1e-9);
        assert_eq!(agg.message, overall_message(0.6));
    }

    #[test]
    fn every_function_is_kept_even_with_equal_names() {
        let mut a = file("a.go", 0.0, &[1.0, 1.0]);
        a.report.functions[0].name = "String".into();
        a.report.functions[1].name = "String".into();
        let agg = aggregate(&[a]);
        assert_eq!(agg.functions.len(), 2);
        assert_eq!(agg.functions[0].name, agg.functions[1].name);
        assert_ne!(agg.functions[0].start_line, agg.functions[1].start_line);
    }

    #[test]
    fn overall_messages_follow_the_thresholds() {
        assert!(overall_message(0.7).starts_with("Excellent"));
        assert!(overall_message(0.4).starts_with("Good"));
        assert!(overall_message(0.3).starts_with("Fair"));
        assert!(overall_message(0.0).starts_with("Poor"));
    }
}
