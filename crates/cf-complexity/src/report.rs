//! The aggregated, multi-file report view (`--format yaml|bin` payload, and
//! the numbers behind the JSON sections report).
//!
//! The static pipeline stamps every function with its file and language and
//! hands the whole list here; this module owns the aggregate math, the
//! deterministic ordering, and the `ComputedMetrics` value tree:
//!
//! * `function_complexity`: every function with density and risk level, in
//!   [`function_order`];
//! * `distribution`: `simple` / `moderate` / `complex` counts;
//! * `high_risk_functions`: functions crossing a high threshold, by risk
//!   priority then [`function_order`];
//! * `aggregate`: the [`Aggregate`] scalars.

use std::cmp::Ordering;

use cf_gojson::{GoMap, GoValue, MapOrigin};

use crate::thresholds::{
    complexity_message, distribution_bucket, health_score, risk_issues, risk_level, risk_priority,
};
use crate::FunctionMetrics;

/// One function's metrics stamped with its file, language and directory.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionInput {
    /// Function name.
    pub name: String,
    /// Source file relative to the analyzed root.
    pub source_file: String,
    /// Detected language.
    pub language: String,
    /// Directory of `source_file` (`.` at the root).
    pub directory: String,
    /// Cyclomatic complexity.
    pub cyclomatic_complexity: i64,
    /// Cognitive complexity.
    pub cognitive_complexity: i64,
    /// Maximum nesting depth.
    pub nesting_depth: i64,
    /// Lines of code.
    pub lines_of_code: i64,
    /// 1-based line where the function starts (0 when unknown).
    pub line: i64,
}

impl FunctionInput {
    /// Stamps per-file metrics with their file, language and directory.
    #[must_use]
    pub fn stamped(
        m: &FunctionMetrics,
        source_file: &str,
        language: &str,
        directory: &str,
    ) -> Self {
        FunctionInput {
            name: m.name.clone(),
            source_file: source_file.to_string(),
            language: language.to_string(),
            directory: directory.to_string(),
            cyclomatic_complexity: m.cyclomatic_complexity,
            cognitive_complexity: m.cognitive_complexity,
            nesting_depth: m.nesting_depth,
            lines_of_code: m.lines_of_code,
            line: m.line,
        }
    }

    /// Decision points (`cyclomatic - 1`, never negative).
    #[must_use]
    pub fn decision_points(&self) -> i64 {
        (self.cyclomatic_complexity - 1).max(0)
    }

    /// Cyclomatic complexity per line of code (0 without lines).
    #[must_use]
    pub fn density(&self) -> f64 {
        complexity_density(self.cyclomatic_complexity, self.lines_of_code)
    }

    /// Risk level of this function (see [`risk_level`]).
    #[must_use]
    pub fn risk(&self) -> &'static str {
        risk_level(
            self.cyclomatic_complexity,
            self.cognitive_complexity,
            self.nesting_depth,
        )
    }
}

/// Cyclomatic complexity per line of code; `0.0` when there are no lines.
#[must_use]
pub fn complexity_density(cyclomatic: i64, lines_of_code: i64) -> f64 {
    if lines_of_code > 0 {
        cyclomatic as f64 / lines_of_code as f64
    } else {
        0.0
    }
}

/// Report-level scalars over all functions.
#[derive(Debug, Clone, PartialEq)]
pub struct Aggregate {
    /// Number of functions.
    pub total_functions: i64,
    /// Function-weighted average cyclomatic complexity
    /// (`total_complexity / total_functions`; 0 without functions).
    pub average_complexity: f64,
    /// Maximum per-function cyclomatic complexity.
    pub max_complexity: i64,
    /// Sum of cyclomatic complexity.
    pub total_complexity: i64,
    /// Sum of cognitive complexity.
    pub cognitive_complexity: i64,
    /// Maximum per-function nesting depth.
    pub nesting_depth: i64,
    /// Sum of decision points.
    pub decision_points: i64,
    /// Health score (0..=100) from the average.
    pub health_score: f64,
    /// Message for the average.
    pub message: &'static str,
}

/// Computes the [`Aggregate`] scalars.
///
/// ```
/// use cf_complexity::report::{aggregate, FunctionInput};
///
/// let f = |name: &str, cc: i64, cog: i64, nest: i64| FunctionInput {
///     name: name.into(), source_file: "a.go".into(), language: "go".into(),
///     directory: ".".into(), cyclomatic_complexity: cc, cognitive_complexity: cog,
///     nesting_depth: nest, lines_of_code: 10, line: 1,
/// };
/// let agg = aggregate(&[f("a", 3, 2, 1), f("b", 5, 7, 3)]);
/// assert_eq!(agg.total_functions, 2);
/// assert_eq!(agg.total_complexity, 8);
/// assert_eq!(agg.cognitive_complexity, 9);
/// assert_eq!(agg.nesting_depth, 3);
/// assert_eq!(agg.max_complexity, 5);
/// assert_eq!(agg.decision_points, 6);
/// assert!((agg.average_complexity - 4.0).abs() < f64::EPSILON);
/// ```
#[must_use]
pub fn aggregate(functions: &[FunctionInput]) -> Aggregate {
    let total_functions = functions.len() as i64;
    let total_complexity: i64 = functions.iter().map(|f| f.cyclomatic_complexity).sum();
    let average_complexity = if total_functions > 0 {
        total_complexity as f64 / total_functions as f64
    } else {
        0.0
    };
    Aggregate {
        total_functions,
        average_complexity,
        max_complexity: functions
            .iter()
            .map(|f| f.cyclomatic_complexity)
            .max()
            .unwrap_or(0),
        total_complexity,
        cognitive_complexity: functions.iter().map(|f| f.cognitive_complexity).sum(),
        nesting_depth: functions.iter().map(|f| f.nesting_depth).max().unwrap_or(0),
        decision_points: functions.iter().map(FunctionInput::decision_points).sum(),
        health_score: health_score(average_complexity),
        message: complexity_message(average_complexity),
    }
}

/// The report ordering: cyclomatic descending, cognitive descending, nesting
/// descending, then name, file and line ascending — a total order, so ties
/// are deterministic.
///
/// ```
/// use cf_complexity::report::{function_order, FunctionInput};
/// use std::cmp::Ordering;
///
/// let f = |name: &str, file: &str, cc: i64| FunctionInput {
///     name: name.into(), source_file: file.into(), language: "go".into(),
///     directory: ".".into(), cyclomatic_complexity: cc, cognitive_complexity: 0,
///     nesting_depth: 0, lines_of_code: 1, line: 1,
/// };
/// assert_eq!(function_order(&f("a", "x.go", 5), &f("b", "x.go", 2)), Ordering::Less);
/// assert_eq!(function_order(&f("b", "x.go", 2), &f("a", "x.go", 2)), Ordering::Greater);
/// assert_eq!(function_order(&f("a", "a/x.go", 2), &f("a", "b/x.go", 2)), Ordering::Less);
/// ```
#[must_use]
pub fn function_order(a: &FunctionInput, b: &FunctionInput) -> Ordering {
    b.cyclomatic_complexity
        .cmp(&a.cyclomatic_complexity)
        .then(b.cognitive_complexity.cmp(&a.cognitive_complexity))
        .then(b.nesting_depth.cmp(&a.nesting_depth))
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| a.source_file.cmp(&b.source_file))
        .then(a.line.cmp(&b.line))
}

/// Inserts a string field only when non-empty.
fn insert_omitempty_str(m: &mut GoMap, key: &str, value: &str) {
    if !value.is_empty() {
        m.insert(key, GoValue::Str(value.to_string()));
    }
}

fn stamp_fields(o: &mut GoMap, f: &FunctionInput) {
    o.insert("name", GoValue::Str(f.name.clone()));
    insert_omitempty_str(o, "source_file", &f.source_file);
    insert_omitempty_str(o, "language", &f.language);
    insert_omitempty_str(o, "directory", &f.directory);
    o.insert(
        "cyclomatic_complexity",
        GoValue::Int(f.cyclomatic_complexity),
    );
    o.insert("cognitive_complexity", GoValue::Int(f.cognitive_complexity));
}

/// `function_complexity`: every function with density and risk, in
/// [`function_order`].
fn build_function_complexity(functions: &[FunctionInput]) -> Vec<GoValue> {
    let mut sorted: Vec<&FunctionInput> = functions.iter().collect();
    sorted.sort_by(|a, b| function_order(a, b));
    sorted
        .into_iter()
        .map(|f| {
            let mut o = GoMap::new(MapOrigin::Struct);
            stamp_fields(&mut o, f);
            o.insert("nesting_depth", GoValue::Int(f.nesting_depth));
            o.insert("lines_of_code", GoValue::Int(f.lines_of_code));
            o.insert("complexity_density", GoValue::Float(f.density()));
            o.insert("risk_level", GoValue::Str(f.risk().to_string()));
            GoValue::Map(o)
        })
        .collect()
}

/// `distribution`: the non-zero buckets (byte-sorted keys on encode).
fn build_distribution(functions: &[FunctionInput]) -> GoMap {
    let mut dist = GoMap::new(MapOrigin::Map);
    for bucket in ["complex", "moderate", "simple"] {
        let count = functions
            .iter()
            .filter(|f| distribution_bucket(f.cyclomatic_complexity) == bucket)
            .count() as i64;
        if count > 0 {
            dist.insert(bucket, GoValue::Int(count));
        }
    }
    dist
}

/// `high_risk_functions`: functions with at least one issue, by risk
/// priority then [`function_order`].
fn build_high_risk_functions(functions: &[FunctionInput]) -> Vec<GoValue> {
    let mut rows: Vec<(&FunctionInput, Vec<&'static str>)> = functions
        .iter()
        .map(|f| {
            (
                f,
                risk_issues(
                    f.cyclomatic_complexity,
                    f.cognitive_complexity,
                    f.nesting_depth,
                ),
            )
        })
        .filter(|(_, issues)| !issues.is_empty())
        .collect();
    rows.sort_by(|(a, _), (b, _)| {
        risk_priority(a.risk())
            .cmp(&risk_priority(b.risk()))
            .then_with(|| function_order(a, b))
    });
    rows.into_iter()
        .map(|(f, issues)| {
            let mut o = GoMap::new(MapOrigin::Struct);
            stamp_fields(&mut o, f);
            o.insert("risk_level", GoValue::Str(f.risk().to_string()));
            o.insert(
                "issues",
                GoValue::Array(
                    issues
                        .into_iter()
                        .map(|s| GoValue::Str(s.to_string()))
                        .collect(),
                ),
            );
            GoValue::Map(o)
        })
        .collect()
}

/// `aggregate`: the [`Aggregate`] scalars as a struct-origin map.
fn build_aggregate(agg: &Aggregate) -> GoMap {
    let mut m = GoMap::new(MapOrigin::Struct);
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
    m.insert("health_score", GoValue::Float(agg.health_score));
    m.insert("message", GoValue::Str(agg.message.to_string()));
    m
}

/// Builds the `ComputedMetrics` value tree (the yaml/bin payload root):
/// `function_complexity`, `distribution`, `high_risk_functions`, `aggregate`
/// in that declaration order.
///
/// ```
/// use cf_complexity::report::{computed_metrics, FunctionInput};
/// use cf_gojson::GoValue;
///
/// let funcs = vec![FunctionInput {
///     name: "foo".into(), source_file: "a.go".into(), language: "go".into(),
///     directory: ".".into(), cyclomatic_complexity: 2, cognitive_complexity: 1,
///     nesting_depth: 1, lines_of_code: 10, line: 3,
/// }];
/// let value = computed_metrics(&funcs);
/// let m = value.as_map().unwrap();
/// for key in ["function_complexity", "distribution", "high_risk_functions", "aggregate"] {
///     assert!(m.contains_key(key), "missing {key}");
/// }
/// let agg = m.get("aggregate").unwrap().as_map().unwrap();
/// assert_eq!(agg.get("cognitive_complexity"), Some(&GoValue::Int(1)));
/// assert_eq!(agg.get("nesting_depth"), Some(&GoValue::Int(1)));
/// ```
#[must_use]
pub fn computed_metrics(functions: &[FunctionInput]) -> GoValue {
    let mut root = GoMap::new(MapOrigin::Struct);
    root.insert(
        "function_complexity",
        GoValue::Array(build_function_complexity(functions)),
    );
    root.insert("distribution", GoValue::Map(build_distribution(functions)));
    root.insert(
        "high_risk_functions",
        GoValue::Array(build_high_risk_functions(functions)),
    );
    root.insert(
        "aggregate",
        GoValue::Map(build_aggregate(&aggregate(functions))),
    );
    GoValue::Map(root)
}

#[cfg(test)]
mod tests {
    use super::{aggregate, computed_metrics, function_order, FunctionInput};
    use crate::thresholds::{MSG_EXCELLENT, RISK_CRITICAL, RISK_LOW};
    use cf_gojson::GoValue;

    fn f(name: &str, file: &str, cc: i64, cog: i64, nest: i64) -> FunctionInput {
        FunctionInput {
            name: name.into(),
            source_file: file.into(),
            language: "go".into(),
            directory: ".".into(),
            cyclomatic_complexity: cc,
            cognitive_complexity: cog,
            nesting_depth: nest,
            lines_of_code: 4,
            line: 1,
        }
    }

    fn arr<'a>(v: &'a GoValue, key: &str) -> &'a [GoValue] {
        match v.as_map().unwrap().get(key) {
            Some(GoValue::Array(a)) => a,
            other => panic!("{key}: not an array: {other:?}"),
        }
    }

    fn name_of(v: &GoValue) -> &str {
        v.as_map().unwrap().get("name").unwrap().as_str().unwrap()
    }

    #[test]
    fn empty_input_yields_zero_aggregate_and_empty_lists() {
        let agg = aggregate(&[]);
        assert_eq!(agg.total_functions, 0);
        assert!(agg.average_complexity.abs() < f64::EPSILON);
        assert_eq!(agg.message, MSG_EXCELLENT);
        let v = computed_metrics(&[]);
        assert!(arr(&v, "function_complexity").is_empty());
        assert!(arr(&v, "high_risk_functions").is_empty());
        assert!(v
            .as_map()
            .unwrap()
            .get("distribution")
            .unwrap()
            .as_map()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn function_complexity_is_sorted_by_the_total_order() {
        let input = vec![
            f("same", "b/x.go", 2, 1, 1),
            f("same", "a/x.go", 2, 1, 1),
            f("zed", "a/x.go", 2, 1, 1),
            f("top", "a/x.go", 12, 20, 6),
        ];
        let v = computed_metrics(&input);
        let names: Vec<(&str, &str)> = arr(&v, "function_complexity")
            .iter()
            .map(|row| {
                (
                    name_of(row),
                    row.as_map()
                        .unwrap()
                        .get("source_file")
                        .unwrap()
                        .as_str()
                        .unwrap(),
                )
            })
            .collect();
        assert_eq!(
            names,
            vec![
                ("top", "a/x.go"),
                ("same", "a/x.go"),
                ("same", "b/x.go"),
                ("zed", "a/x.go")
            ]
        );
        let top = &arr(&v, "function_complexity")[0];
        assert_eq!(
            top.as_map().unwrap().get("risk_level").unwrap().as_str(),
            Some(RISK_CRITICAL)
        );
        assert_eq!(
            arr(&v, "function_complexity")[1]
                .as_map()
                .unwrap()
                .get("risk_level")
                .unwrap()
                .as_str(),
            Some(RISK_LOW)
        );
        assert_eq!(arr(&v, "high_risk_functions").len(), 1);
        assert_eq!(name_of(&arr(&v, "high_risk_functions")[0]), "top");
    }

    #[test]
    fn ordering_is_total_and_antisymmetric() {
        let a = f("a", "x.go", 3, 3, 1);
        let b = f("a", "x.go", 3, 2, 1);
        assert_eq!(function_order(&a, &b), std::cmp::Ordering::Less);
        assert_eq!(function_order(&b, &a), std::cmp::Ordering::Greater);
        assert_eq!(function_order(&a, &a), std::cmp::Ordering::Equal);
    }

    #[test]
    fn distribution_counts_only_non_empty_buckets() {
        let v = computed_metrics(&[f("a", "x.go", 1, 0, 0), f("b", "x.go", 11, 0, 0)]);
        let dist = v
            .as_map()
            .unwrap()
            .get("distribution")
            .unwrap()
            .as_map()
            .unwrap();
        assert_eq!(dist.get("simple"), Some(&GoValue::Int(1)));
        assert_eq!(dist.get("complex"), Some(&GoValue::Int(1)));
        assert!(dist.get("moderate").is_none());
    }
}
