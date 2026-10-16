//! The per-file quality sample: the four static component analyzers run over
//! one parsed file, exactly as the `static/*` surfaces run them.
//!
//! [`analyze_file`] feeds the file's UAST root AND its source bytes to
//! `cf-complexity` (operator / name / line recovery) and `cf-halstead`
//! (token-less node recovery), so a file's quality numbers are the numbers
//! `static/complexity`, `static/halstead`, `static/comments` and
//! `static/cohesion` report for the same file. The only difference is the
//! discovery depth cap [`FIND_MAX_DEPTH`].

use cf_cohesion::report_value::ReportValue;
use cf_gojson::GoValue;
use cf_uast_node::Node;

/// Discovery depth cap of the component analyzers: only function / comment
/// nodes at depth `<= 10` below the file root are discovered (the static
/// surfaces run uncapped); the analysis of each discovered subtree is
/// unchanged.
pub const FIND_MAX_DEPTH: usize = 10;

/// One analyzed file's scalars, one per component metric.
#[derive(Debug, Clone, PartialEq)]
pub struct FileSample {
    /// Summed cyclomatic complexity (`total_complexity`).
    pub complexity: f64,
    /// Summed cognitive complexity (`cognitive_complexity`).
    pub cognitive: f64,
    /// Highest single-function cyclomatic complexity (`max_complexity`).
    pub max_complexity: i64,
    /// Function count (`total_functions`).
    pub functions: i64,
    /// Halstead volume of the file's functions (`volume`).
    pub halstead_volume: f64,
    /// Halstead effort (`effort`).
    pub halstead_effort: f64,
    /// Halstead delivered bugs (`delivered_bugs`).
    pub delivered_bugs: f64,
    /// Good-comment ratio (`overall_score`).
    pub comment_score: f64,
    /// Documented-declaration ratio (`documentation_coverage`).
    pub doc_coverage: f64,
    /// Cohesion score (`cohesion_score`); `None` for a file without
    /// functions — no cohesion data, as in the static analyzer.
    pub cohesion: Option<f64>,
}

/// Runs the four component analyzers over one parsed file (`root`, with its
/// `source` bytes) and returns its [`FileSample`].
///
/// ```
/// use cf_quality::analyze_file;
///
/// let src = b"package a\n\n// Add adds.\nfunc Add(a, b int) int {\n\tif a > b {\n\t\treturn a + b\n\t}\n\treturn b\n}\n";
/// let root = cf_uast::Parser::new().parse("a.go", src).unwrap();
/// let sample = analyze_file(&root, src);
/// assert_eq!(sample.functions, 1);
/// assert_eq!(sample.complexity, 2.0);
/// assert_eq!(sample.comment_score, 1.0);
/// assert_eq!(sample.cohesion, Some(1.0));
/// ```
#[must_use]
pub fn analyze_file(root: &Node, source: &[u8]) -> FileSample {
    let cx_root = cf_complexity::node::Node::from_uast(root);
    let cx = cf_complexity::Analyzer.analyze_with(
        Some(&cx_root),
        &cf_complexity::AnalysisOptions {
            source: Some(source),
            find_depth: Some(FIND_MAX_DEPTH),
        },
    );
    let halstead = cf_halstead::analyze_with_source(root, Some(source));
    let comments = cf_comments::Analyzer::new()
        .analyze_with_find_depth(Some(root), Some(FIND_MAX_DEPTH))
        .map_or_else(
            |_| (0.0, 0.0),
            |report| {
                (
                    govalue_float(&report, "overall_score"),
                    govalue_float(&report, "documentation_coverage"),
                )
            },
        );
    let cohesion = cf_cohesion::Analyzer::new().analyze_file_capped(root, Some(FIND_MAX_DEPTH));

    FileSample {
        complexity: govalue_int(&cx, "total_complexity") as f64,
        cognitive: govalue_int(&cx, "cognitive_complexity") as f64,
        max_complexity: govalue_int(&cx, "max_complexity"),
        functions: govalue_int(&cx, "total_functions"),
        halstead_volume: halstead.volume,
        halstead_effort: halstead.effort,
        delivered_bugs: halstead.delivered_bugs,
        comment_score: comments.0,
        doc_coverage: comments.1,
        cohesion: cohesion.has_functions().then(|| {
            cohesion
                .to_report()
                .get("cohesion_score")
                .and_then(ReportValue::as_float)
                .unwrap_or(0.0)
        }),
    }
}

/// Integer value at `key` in a cf-gojson report map, truncating floats toward
/// zero; `0` when absent or non-numeric.
fn govalue_int(v: &GoValue, key: &str) -> i64 {
    match v {
        GoValue::Map(m) => match m.get(key) {
            Some(GoValue::Int(n)) => *n,
            Some(GoValue::Float(f)) => *f as i64,
            _ => 0,
        },
        _ => 0,
    }
}

/// Float value at `key`; `0.0` when absent or non-numeric.
fn govalue_float(v: &GoValue, key: &str) -> f64 {
    match v {
        GoValue::Map(m) => match m.get(key) {
            Some(GoValue::Float(f)) => *f,
            Some(GoValue::Int(n)) => *n as f64,
            _ => 0.0,
        },
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    /// One documented function with one `if`, one undocumented function
    /// sharing no variable with it.
    const TWO_FUNCTIONS_GO: &[u8] = b"package main

// Add returns the sum of a and b.
func Add(a, b int) int {
\tif a > b {
\t\treturn a + b
\t}
\treturn b
}

func helper() int {
\tx := 1
\ty := 2
\treturn x + y
}
";

    /// A type declaration only.
    const NO_FUNCTIONS_GO: &[u8] =
        b"package main\n\n// Config holds settings.\ntype Config struct {\n\tName string\n}\n";

    /// The complexity crate's Python fixture: its UAST carries token-less
    /// operator nodes, so the Halstead counts depend on source recovery.
    const PY_FIXTURE: &[u8] = include_bytes!("../../cf-complexity/tests/fixtures/cc.py");

    fn parse(name: &str, src: &[u8]) -> Node {
        cf_uast::Parser::new()
            .parse(name, src)
            .unwrap_or_else(|e| panic!("parse {name}: {e}"))
    }

    /// The sample carries the static analyzers' numbers for the same file.
    #[test]
    fn sample_matches_the_component_analyzers() {
        let root = parse("a.go", TWO_FUNCTIONS_GO);
        let sample = analyze_file(&root, TWO_FUNCTIONS_GO);

        assert_eq!(sample.functions, 2);
        assert_eq!(sample.complexity, 3.0, "{sample:?}");
        assert_eq!(sample.cognitive, 1.0, "{sample:?}");
        assert_eq!(sample.max_complexity, 2);

        let halstead = cf_halstead::analyze_with_source(&root, Some(TWO_FUNCTIONS_GO));
        assert!((sample.halstead_volume - halstead.volume).abs() < EPS);
        assert!((sample.halstead_effort - halstead.effort).abs() < EPS);
        assert!((sample.delivered_bugs - halstead.delivered_bugs).abs() < EPS);
        assert!(sample.halstead_volume > 0.0);

        // One good comment of one; one of two declarations documented.
        assert!((sample.comment_score - 1.0).abs() < EPS, "{sample:?}");
        assert!((sample.doc_coverage - 0.5).abs() < EPS, "{sample:?}");

        let cohesion = cf_cohesion::Analyzer::new().analyze_file(&root);
        assert_eq!(sample.cohesion, Some(cohesion.cohesion_score));
    }

    /// The source bytes matter: the Halstead volume recovered from the
    /// source is the static analyzer's, not the token-only value.
    #[test]
    fn source_bytes_feed_the_token_recovery() {
        let root = parse("cc.py", PY_FIXTURE);
        let with_source = analyze_file(&root, PY_FIXTURE).halstead_volume;
        let token_only = cf_halstead::analyze(&root).volume;
        let static_value = cf_halstead::analyze_with_source(&root, Some(PY_FIXTURE)).volume;
        assert!((with_source - static_value).abs() < EPS);
        assert!(
            (with_source - token_only).abs() > EPS,
            "the fixture must exercise source recovery: {with_source} vs {token_only}"
        );
    }

    /// A function-free file has no cohesion data and zero function-based
    /// scalars; its comment scalars are still the static analyzer's.
    #[test]
    fn function_free_file_has_no_cohesion_sample() {
        let root = parse("c.go", NO_FUNCTIONS_GO);
        let sample = analyze_file(&root, NO_FUNCTIONS_GO);
        assert_eq!(sample.cohesion, None, "{sample:?}");
        assert_eq!(sample.functions, 0);
        assert_eq!(sample.complexity, 0.0);
        assert_eq!(sample.halstead_volume, 0.0);
        assert!((sample.comment_score - 1.0).abs() < EPS, "{sample:?}");
    }
}
