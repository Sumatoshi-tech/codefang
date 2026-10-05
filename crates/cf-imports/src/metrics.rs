//! Import metrics computation.
//!
//! Given the classified import records ([`ReportData`]), computes a
//! categorized import list, a category distribution, dependency-risk findings,
//! and aggregate statistics ([`ComputedMetrics`]) — the structure the static
//! analyzer marshals in the json/yaml/bin formats.
//!
//! Classification is per import record: `relative` for file-relative targets
//! (`./x`, `../x`, `/x`, Rust `crate::`/`super::`/`self::`, local-file
//! includes), `stdlib` when [`crate::stdlib::is_stdlib`] says so for the
//! record's language, `external` otherwise.

use crate::language::Language;
use crate::report::ReportValue;
use crate::section::KEY_IMPORTS;
use crate::stdlib;

/// Threshold: `..` occurrences flagged as a deeply-nested relative import.
const DEEPLY_NESTED_THRESHOLD: usize = 3;
/// Threshold: `/` occurrences flagged as an overly-long import path.
const LONG_PATH_THRESHOLD: usize = 5;

/// Category of a file-relative import.
pub const CATEGORY_RELATIVE: &str = "relative";
/// Category of a standard-library import.
pub const CATEGORY_STDLIB: &str = "stdlib";
/// Category of a third-party import.
pub const CATEGORY_EXTERNAL: &str = "external";

/// Risk level of a deeply nested relative import.
const RISK_MEDIUM: &str = "MEDIUM";
/// Risk level of an overly long import path.
const RISK_LOW: &str = "LOW";
/// Reason attached to [`RISK_MEDIUM`] findings.
const REASON_DEEPLY_NESTED: &str =
    "Deeply nested relative import may indicate poor module structure";
/// Reason attached to [`RISK_LOW`] findings.
const REASON_LONG_PATH: &str = "Long import path may indicate overly complex package structure";

/// Rust path roots that name the importing crate itself.
const RUST_LOCAL_ROOTS: &[&str] = &["crate", "super", "self"];
/// Rust path separator.
const RUST_SEP: &str = "::";

/// One import to classify.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportRecord {
    /// The import path.
    pub path: String,
    /// Parser language name of the importing file (empty = unknown, which
    /// classifies against the union of all stdlib tables).
    pub language: String,
    /// Whether the import names a file next to the importing source.
    pub local_file: bool,
}

/// Parsed input data for metrics computation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportData {
    /// The import records found in the report.
    pub imports: Vec<ImportRecord>,
}

impl ReportData {
    /// Extracts [`ReportData`] from a dynamic report: the `imports` key read
    /// as a string list. Records carry no language (union classification) and
    /// no local-file flag; the static path builds [`ReportData`] directly from
    /// [`crate::Aggregate::report_data`] instead.
    #[must_use]
    pub fn parse(report: &ReportValue) -> ReportData {
        let imports = match report.as_map().and_then(|m| m.get(KEY_IMPORTS)) {
            Some(ReportValue::List(items)) => items
                .iter()
                .filter_map(|v| match v {
                    ReportValue::Str(s) => Some(ImportRecord {
                        path: s.clone(),
                        ..ImportRecord::default()
                    }),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        ReportData { imports }
    }
}

/// Information about a single import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportData {
    /// The import path.
    pub path: String,
    /// [`CATEGORY_RELATIVE`], [`CATEGORY_STDLIB`], or [`CATEGORY_EXTERNAL`].
    pub category: String,
    /// Whether the import is external (not stdlib, not relative).
    pub is_external: bool,
}

/// Import count for a single category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCategoryData {
    /// The category name.
    pub category: String,
    /// Number of imports in the category.
    pub count: i64,
}

/// A potential dependency issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDependencyData {
    /// The offending import path.
    pub path: String,
    /// `MEDIUM` or `LOW`.
    pub risk_level: String,
    /// Human-readable reason.
    pub reason: String,
}

/// Aggregate summary statistics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AggregateData {
    /// Total number of imports.
    pub total_imports: i64,
    /// Number of external imports.
    pub external_imports: i64,
    /// Number of internal (stdlib/relative) imports.
    pub internal_imports: i64,
    /// Number of unique base packages.
    pub unique_packages: i64,
    /// `external_imports / total_imports` (0 when there are no imports).
    pub external_ratio: f64,
}

/// All computed metric results.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComputedMetrics {
    /// Categorized import list (`import_list`).
    pub import_list: Vec<ImportData>,
    /// Category distribution (`categories`).
    pub categories: Vec<ImportCategoryData>,
    /// Dependency-risk findings (`dependencies`).
    pub dependencies: Vec<ImportDependencyData>,
    /// Aggregate statistics (`aggregate`).
    pub aggregate: AggregateData,
}

impl ComputedMetrics {
    /// Converts the metrics to a [`cf_gojson::GoValue`] for the machine JSON
    /// and bin formats.
    ///
    /// * The top-level object is **struct-origin**, so keys are emitted in
    ///   declaration order — `import_list`, `categories`, `dependencies`,
    ///   `aggregate`.
    /// * `import_list` and `categories` serialize as `[]` when empty.
    /// * `dependencies` serializes as **`null`** when no risky imports exist.
    /// * `aggregate` keeps declaration order `total_imports`,
    ///   `external_imports`, `internal_imports`, `unique_packages`,
    ///   `external_ratio`.
    #[must_use]
    pub fn to_go_value(&self) -> cf_gojson::GoValue {
        use cf_gojson::{GoMap, GoValue};

        let import_list = self
            .import_list
            .iter()
            .map(|d| {
                let mut m = GoMap::new_struct();
                m.push("path", GoValue::Str(d.path.clone()));
                m.push("category", GoValue::Str(d.category.clone()));
                m.push("is_external", GoValue::Bool(d.is_external));
                GoValue::Map(m)
            })
            .collect();

        let categories = self
            .categories
            .iter()
            .map(|c| {
                let mut m = GoMap::new_struct();
                m.push("category", GoValue::Str(c.category.clone()));
                m.push("count", GoValue::Int(c.count));
                GoValue::Map(m)
            })
            .collect();

        let dependencies = if self.dependencies.is_empty() {
            GoValue::Null
        } else {
            GoValue::Array(
                self.dependencies
                    .iter()
                    .map(|d| {
                        let mut m = GoMap::new_struct();
                        m.push("path", GoValue::Str(d.path.clone()));
                        m.push("risk_level", GoValue::Str(d.risk_level.clone()));
                        m.push("reason", GoValue::Str(d.reason.clone()));
                        GoValue::Map(m)
                    })
                    .collect(),
            )
        };

        let mut agg = GoMap::new_struct();
        agg.push("total_imports", GoValue::Int(self.aggregate.total_imports));
        agg.push(
            "external_imports",
            GoValue::Int(self.aggregate.external_imports),
        );
        agg.push(
            "internal_imports",
            GoValue::Int(self.aggregate.internal_imports),
        );
        agg.push(
            "unique_packages",
            GoValue::Int(self.aggregate.unique_packages),
        );
        agg.push(
            "external_ratio",
            GoValue::Float(self.aggregate.external_ratio),
        );

        let mut root = GoMap::new_struct();
        root.push("import_list", GoValue::Array(import_list));
        root.push("categories", GoValue::Array(categories));
        root.push("dependencies", dependencies);
        root.push("aggregate", GoValue::Map(agg));
        GoValue::Map(root)
    }

    /// Converts the metrics to a [`cf_gojson::GoValue`] for the machine YAML
    /// format: identical to [`Self::to_go_value`] except that an empty
    /// `dependencies` is an empty sequence `[]`, not `null`.
    #[must_use]
    pub fn to_go_value_yaml(&self) -> cf_gojson::GoValue {
        use cf_gojson::GoValue;

        let mut root = match self.to_go_value() {
            GoValue::Map(m) => m,
            other => return other,
        };
        if matches!(root.get("dependencies"), Some(GoValue::Null)) {
            root.insert("dependencies", GoValue::Array(Vec::new()));
        }
        GoValue::Map(root)
    }
}

/// Runs all metrics over a dynamic report (see [`ReportData::parse`]).
///
/// # Errors
/// Never fails; the `Result` keeps call-site parity with the analyzer
/// interface used by the history and plot paths.
pub fn compute_all_metrics(
    report: &ReportValue,
) -> Result<ComputedMetrics, std::convert::Infallible> {
    Ok(compute_metrics(&ReportData::parse(report)))
}

/// Runs all metrics over classified import records.
#[must_use]
pub fn compute_metrics(input: &ReportData) -> ComputedMetrics {
    ComputedMetrics {
        import_list: compute_import_list(input),
        categories: compute_categories(input),
        dependencies: compute_dependencies(input),
        aggregate: compute_aggregate(input),
    }
}

/// Categorizes an import record as `relative`, `stdlib`, or `external`.
///
/// ```
/// use cf_imports::metrics::{categorize, ImportRecord, CATEGORY_STDLIB, CATEGORY_EXTERNAL, CATEGORY_RELATIVE};
///
/// let rec = |path: &str, language: &str| ImportRecord { path: path.into(), language: language.into(), local_file: false };
/// assert_eq!(categorize(&rec("strconv", "go")), CATEGORY_STDLIB);
/// assert_eq!(categorize(&rec("github.com/x/y", "go")), CATEGORY_EXTERNAL);
/// assert_eq!(categorize(&rec("crate::foo", "rust")), CATEGORY_RELATIVE);
/// assert_eq!(categorize(&rec("../utils", "javascript")), CATEGORY_RELATIVE);
/// ```
#[must_use]
pub fn categorize(record: &ImportRecord) -> &'static str {
    if is_relative(record) {
        CATEGORY_RELATIVE
    } else if stdlib::is_stdlib(&record.language, &record.path) {
        CATEGORY_STDLIB
    } else {
        CATEGORY_EXTERNAL
    }
}

/// File-relative: a local-file include, a `.`/`/`-prefixed path, or a Rust
/// path rooted in the importing crate.
fn is_relative(record: &ImportRecord) -> bool {
    if record.local_file || record.path.starts_with('.') || record.path.starts_with('/') {
        return true;
    }
    Language::from_name(&record.language) == Language::Rust
        && RUST_LOCAL_ROOTS.contains(&record.path.split(RUST_SEP).next().unwrap_or(""))
}

/// Computes the categorized import list, sorted by category then path.
fn compute_import_list(input: &ReportData) -> Vec<ImportData> {
    let mut result: Vec<ImportData> = input
        .imports
        .iter()
        .map(|rec| {
            let category = categorize(rec);
            ImportData {
                path: rec.path.clone(),
                category: category.to_string(),
                is_external: category == CATEGORY_EXTERNAL,
            }
        })
        .collect();
    result.sort_by(|a, b| {
        a.category
            .cmp(&b.category)
            .then_with(|| a.path.cmp(&b.path))
    });
    result
}

/// Computes the category distribution, sorted by count descending then
/// category name.
fn compute_categories(input: &ReportData) -> Vec<ImportCategoryData> {
    use std::collections::BTreeMap;
    let mut categories: BTreeMap<&str, i64> = BTreeMap::new();
    for rec in &input.imports {
        *categories.entry(categorize(rec)).or_insert(0) += 1;
    }
    let mut result: Vec<ImportCategoryData> = categories
        .into_iter()
        .map(|(category, count)| ImportCategoryData {
            category: category.to_string(),
            count,
        })
        .collect();
    result.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.category.cmp(&b.category))
    });
    result
}

/// Identifies potential dependency issues, in input order. When both
/// conditions hold the long-path finding wins.
fn compute_dependencies(input: &ReportData) -> Vec<ImportDependencyData> {
    input
        .imports
        .iter()
        .filter_map(|rec| {
            let path = rec.path.as_str();
            let finding = if path.matches('/').count() >= LONG_PATH_THRESHOLD {
                Some((RISK_LOW, REASON_LONG_PATH))
            } else if path.matches("..").count() >= DEEPLY_NESTED_THRESHOLD {
                Some((RISK_MEDIUM, REASON_DEEPLY_NESTED))
            } else {
                None
            };
            finding.map(|(risk_level, reason)| ImportDependencyData {
                path: path.to_string(),
                risk_level: risk_level.to_string(),
                reason: reason.to_string(),
            })
        })
        .collect()
}

/// Computes aggregate statistics.
fn compute_aggregate(input: &ReportData) -> AggregateData {
    use std::collections::BTreeSet;
    let mut agg = AggregateData {
        total_imports: input.imports.len() as i64,
        ..Default::default()
    };
    let mut packages: BTreeSet<&str> = BTreeSet::new();
    for rec in &input.imports {
        if categorize(rec) == CATEGORY_EXTERNAL {
            agg.external_imports += 1;
        } else {
            agg.internal_imports += 1;
        }
        packages.insert(base_package(&rec.path));
    }
    agg.unique_packages = packages.len() as i64;
    if agg.total_imports > 0 {
        agg.external_ratio = agg.external_imports as f64 / agg.total_imports as f64;
    }
    agg
}

/// Returns the base package: the substring before the first `/`.
fn base_package(imp: &str) -> &str {
    imp.split_once('/').map_or(imp, |(head, _)| head)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GO_STDLIB: &str = "fmt";
    const GO_EXTERNAL: &str = "github.com/user/repo";
    const JS_RELATIVE: &str = "../utils";

    fn rec(path: &str, language: &str) -> ImportRecord {
        ImportRecord {
            path: path.to_string(),
            language: language.to_string(),
            local_file: false,
        }
    }

    fn data(records: &[(&str, &str)]) -> ReportData {
        ReportData {
            imports: records.iter().map(|(p, l)| rec(p, l)).collect(),
        }
    }

    fn report_with_imports(imports: &[&str]) -> ReportValue {
        let mut r = ReportValue::map();
        r.insert(
            KEY_IMPORTS,
            ReportValue::List(
                imports
                    .iter()
                    .map(|s| ReportValue::Str((*s).to_string()))
                    .collect(),
            ),
        );
        r
    }

    #[test]
    fn parse_reads_the_imports_list_without_language() {
        assert!(ReportData::parse(&ReportValue::map()).imports.is_empty());
        let parsed = ReportData::parse(&report_with_imports(&[GO_STDLIB, GO_EXTERNAL]));
        assert_eq!(parsed.imports.len(), 2);
        assert_eq!(parsed.imports[0].path, GO_STDLIB);
        assert_eq!(parsed.imports[0].language, "");
        assert!(!parsed.imports[1].local_file);
    }

    /// The empty report's compact JSON: declaration field order,
    /// `import_list`/`categories` => `[]`, `dependencies` => `null`,
    /// `external_ratio` => `0`.
    #[test]
    fn empty_report_json_shape() {
        let metrics = compute_all_metrics(&ReportValue::map()).unwrap();
        let bytes = cf_gojson::marshal(&metrics.to_go_value());
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "{\"import_list\":[],\"categories\":[],\"dependencies\":null,\
             \"aggregate\":{\"total_imports\":0,\"external_imports\":0,\
             \"internal_imports\":0,\"unique_packages\":0,\"external_ratio\":0}}"
        );
    }

    #[test]
    fn categorize_uses_the_record_language() {
        let cases = [
            ("./utils", "javascript", CATEGORY_RELATIVE),
            ("/absolute/path", "python", CATEGORY_RELATIVE),
            ("..pkg.sub", "python", CATEGORY_RELATIVE),
            ("crate::foo::bar", "rust", CATEGORY_RELATIVE),
            ("super::baz", "rust", CATEGORY_RELATIVE),
            ("self::x", "rust", CATEGORY_RELATIVE),
            ("crate::foo", "python", CATEGORY_EXTERNAL),
            ("fmt", "go", CATEGORY_STDLIB),
            ("errors", "go", CATEGORY_STDLIB),
            ("encoding/json", "go", CATEGORY_STDLIB),
            ("github.com/user/repo", "go", CATEGORY_EXTERNAL),
            ("datetime", "python", CATEGORY_STDLIB),
            ("numpy", "python", CATEGORY_EXTERNAL),
            ("fs", "javascript", CATEGORY_STDLIB),
            ("react", "javascript", CATEGORY_EXTERNAL),
            ("std::io::*", "rust", CATEGORY_STDLIB),
            ("serde::Serialize", "rust", CATEGORY_EXTERNAL),
            ("somepackage", "", CATEGORY_EXTERNAL),
            ("fmt", "", CATEGORY_STDLIB),
        ];
        for (path, language, expected) in cases {
            assert_eq!(
                categorize(&rec(path, language)),
                expected,
                "{language}: {path}"
            );
        }
        let local = ImportRecord {
            path: "local.h".into(),
            language: "c".into(),
            local_file: true,
        };
        assert_eq!(categorize(&local), CATEGORY_RELATIVE);
        assert_eq!(categorize(&rec("stdio.h", "c")), CATEGORY_STDLIB);
    }

    #[test]
    fn import_list_is_sorted_by_category_then_path() {
        let result = compute_import_list(&data(&[
            (GO_EXTERNAL, "go"),
            (GO_STDLIB, "go"),
            (JS_RELATIVE, "javascript"),
            ("os", "go"),
        ]));
        let got: Vec<(&str, &str, bool)> = result
            .iter()
            .map(|d| (d.category.as_str(), d.path.as_str(), d.is_external))
            .collect();
        assert_eq!(
            got,
            vec![
                (CATEGORY_EXTERNAL, GO_EXTERNAL, true),
                (CATEGORY_RELATIVE, JS_RELATIVE, false),
                (CATEGORY_STDLIB, GO_STDLIB, false),
                (CATEGORY_STDLIB, "os", false),
            ]
        );
    }

    #[test]
    fn categories_are_counted_and_ranked() {
        assert!(compute_categories(&ReportData::default()).is_empty());
        let result = compute_categories(&data(&[
            (GO_STDLIB, "go"),
            ("os", "go"),
            ("io", "go"),
            (GO_EXTERNAL, "go"),
            ("github.com/other/pkg", "go"),
            (JS_RELATIVE, "javascript"),
        ]));
        let got: Vec<(&str, i64)> = result
            .iter()
            .map(|c| (c.category.as_str(), c.count))
            .collect();
        assert_eq!(
            got,
            vec![
                (CATEGORY_STDLIB, 3),
                (CATEGORY_EXTERNAL, 2),
                (CATEGORY_RELATIVE, 1)
            ]
        );
    }

    #[test]
    fn dependency_findings() {
        assert!(compute_dependencies(&ReportData::default()).is_empty());
        assert!(compute_dependencies(&data(&[(GO_STDLIB, "go"), (GO_EXTERNAL, "go")])).is_empty());

        let nested = compute_dependencies(&data(&[
            ("../../../utils", "javascript"),
            ("../../../../other/utils", "javascript"),
        ]));
        assert_eq!(nested.len(), 2);
        assert_eq!(nested[0].risk_level, RISK_MEDIUM);
        assert_eq!(nested[0].reason, REASON_DEEPLY_NESTED);

        let long = compute_dependencies(&data(&[(
            "github.com/org/repo/pkg/internal/utils/helper",
            "go",
        )]));
        assert_eq!(long.len(), 1);
        assert_eq!(long[0].risk_level, RISK_LOW);
        assert_eq!(long[0].reason, REASON_LONG_PATH);

        // Both conditions: the long-path finding wins.
        let both = compute_dependencies(&data(&[("../../../a/b/c/d/e", "javascript")]));
        assert_eq!(both[0].risk_level, RISK_LOW);
    }

    #[test]
    fn aggregate_statistics() {
        let empty = compute_aggregate(&ReportData::default());
        assert_eq!(empty.total_imports, 0);
        assert!((empty.external_ratio - 0.0).abs() < f64::EPSILON);

        let mixed = compute_aggregate(&data(&[
            (GO_STDLIB, "go"),
            ("os", "go"),
            (GO_EXTERNAL, "go"),
            ("github.com/other", "go"),
            (JS_RELATIVE, "javascript"),
        ]));
        assert_eq!(mixed.total_imports, 5);
        assert_eq!(mixed.external_imports, 2);
        assert_eq!(mixed.internal_imports, 3);
        assert_eq!(mixed.unique_packages, 4); // fmt, os, github.com, ..
        assert!((mixed.external_ratio - 2.0 / 5.0).abs() < 0.01);

        let all_external = compute_aggregate(&data(&[
            (GO_EXTERNAL, "go"),
            ("github.com/other/pkg", "go"),
        ]));
        assert!((all_external.external_ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_all_metrics_full_report() {
        let report =
            report_with_imports(&[GO_STDLIB, GO_EXTERNAL, JS_RELATIVE, "../../../deep/nested"]);
        let result = compute_all_metrics(&report).unwrap();
        assert_eq!(result.import_list.len(), 4);
        assert_eq!(result.categories.len(), 3);
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(result.dependencies[0].risk_level, RISK_MEDIUM);
        assert_eq!(result.aggregate.total_imports, 4);
    }

    #[test]
    fn yaml_value_renders_empty_dependencies_as_sequence() {
        use cf_gojson::GoValue;
        let metrics = compute_metrics(&ReportData::default());
        let GoValue::Map(root) = metrics.to_go_value_yaml() else {
            panic!("map expected");
        };
        assert!(matches!(root.get("dependencies"), Some(GoValue::Array(a)) if a.is_empty()));
    }
}
