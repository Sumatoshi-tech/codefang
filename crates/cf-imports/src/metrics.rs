//! Import metrics computation.
//!
//! Given a parsed report ([`ReportData`]), computes a categorized import list,
//! a category distribution, dependency-risk findings, and aggregate statistics
//! ([`ComputedMetrics`]) — the structure the static analyzer marshals in the
//! json/yaml/bin formats.
//!
//! The classification helpers ([`categorize_import`], [`is_external_import`],
//! [`is_standard_library`]) are data-parity-critical: they decide which/what
//! values appear in machine output (pinned by the differential gate in
//! `tests/compat`), so the stdlib table and prefix rules are frozen
//! reference data.

use crate::report::ReportValue;

/// Threshold: `..` occurrences flagged as a deeply-nested relative import.
const DEEPLY_NESTED_THRESHOLD: usize = 3;
/// Threshold: `/` occurrences flagged as an overly-long import path.
const LONG_PATH_THRESHOLD: usize = 5;

/// Parsed input data for metrics computation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportData {
    /// The import identifiers found in the report.
    pub imports: Vec<String>,
    /// The reported import count.
    pub count: i64,
}

impl ReportData {
    /// Extracts [`ReportData`] from an analyzer report.
    ///
    /// The `imports` key is read as a string list when present; otherwise (the
    /// binary-encode -> JSON-decode round trip renames it to `import_list` with
    /// `{"path": ...}` objects) paths are pulled from the `path` field of each
    /// `import_list` entry. `count` accepts an int or a float (the latter from
    /// JSON decode).
    ///
    /// # Errors
    /// Never fails; the `Result` keeps call-site parity with the analyzer
    /// interface.
    pub fn parse(report: &ReportValue) -> Result<ReportData, std::convert::Infallible> {
        let mut data = ReportData::default();
        let Some(map) = report.as_map() else {
            return Ok(data);
        };

        match map.get("imports") {
            Some(ReportValue::List(items)) => {
                data.imports = items
                    .iter()
                    .filter_map(|v| match v {
                        ReportValue::Str(s) => Some(s.clone()),
                        _ => None,
                    })
                    .collect();
            }
            _ => {
                if let Some(items) = map.get("import_list") {
                    data.imports = extract_import_paths(items);
                }
            }
        }

        match map.get("count") {
            Some(ReportValue::Int(c)) => data.count = *c,
            Some(ReportValue::Float(c)) => data.count = *c as i64,
            _ => {}
        }

        Ok(data)
    }
}

/// Extracts string paths from a JSON-decoded `import_list` (a list of
/// `{"path": ...}` objects).
fn extract_import_paths(items: &ReportValue) -> Vec<String> {
    let ReportValue::List(list) = items else {
        return Vec::new();
    };
    let mut paths = Vec::with_capacity(list.len());
    for item in list {
        if let ReportValue::Map(m) = item {
            if let Some(ReportValue::Str(p)) = m.get("path") {
                paths.push(p.clone());
            }
        }
    }
    paths
}

/// Information about a single import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportData {
    /// The import path.
    pub path: String,
    /// `relative`, `stdlib`, or `external`.
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
    /// Converts the metrics to a [`ReportValue`] tree.
    ///
    /// Key names match the report contract exactly. Note that the top-level
    /// report object serializes with **declaration-order** keys (`import_list`,
    /// `categories`, `dependencies`, `aggregate`), which a byte-sorted
    /// [`ReportValue::Map`] cannot express — the authoritative encoder path is
    /// [`Self::to_go_value`]; this conversion remains as the legacy local-shim
    /// view.
    #[must_use]
    pub fn to_report_value(&self) -> ReportValue {
        let mut root = ReportValue::map();

        let import_list = self
            .import_list
            .iter()
            .map(|d| {
                let mut m = ReportValue::map();
                m.insert("path", ReportValue::Str(d.path.clone()));
                m.insert("category", ReportValue::Str(d.category.clone()));
                m.insert("is_external", ReportValue::Bool(d.is_external));
                m
            })
            .collect();
        root.insert("import_list", ReportValue::List(import_list));

        let categories = self
            .categories
            .iter()
            .map(|c| {
                let mut m = ReportValue::map();
                m.insert("category", ReportValue::Str(c.category.clone()));
                m.insert("count", ReportValue::Int(c.count));
                m
            })
            .collect();
        root.insert("categories", ReportValue::List(categories));

        let dependencies = self
            .dependencies
            .iter()
            .map(|d| {
                let mut m = ReportValue::map();
                m.insert("path", ReportValue::Str(d.path.clone()));
                m.insert("risk_level", ReportValue::Str(d.risk_level.clone()));
                m.insert("reason", ReportValue::Str(d.reason.clone()));
                m
            })
            .collect();
        root.insert("dependencies", ReportValue::List(dependencies));

        let mut agg = ReportValue::map();
        agg.insert("total_imports", ReportValue::Int(self.aggregate.total_imports));
        agg.insert(
            "external_imports",
            ReportValue::Int(self.aggregate.external_imports),
        );
        agg.insert(
            "internal_imports",
            ReportValue::Int(self.aggregate.internal_imports),
        );
        agg.insert(
            "unique_packages",
            ReportValue::Int(self.aggregate.unique_packages),
        );
        agg.insert(
            "external_ratio",
            ReportValue::Float(self.aggregate.external_ratio),
        );
        root.insert("aggregate", agg);

        root
    }

    /// Converts the metrics to a [`cf_gojson::GoValue`] for byte-exact machine
    /// JSON output. This is the authoritative encoder path;
    /// [`Self::to_report_value`] is the legacy local shim.
    ///
    /// Byte-parity details of the report contract (pinned by the differential
    /// gate in `tests/compat`):
    ///
    /// * The top-level object is **struct-origin**, so keys are emitted in
    ///   declaration order — `import_list`, `categories`, `dependencies`,
    ///   `aggregate` — **not** byte-sorted.
    /// * `import_list` and `categories` are always present slices, so they
    ///   serialize as `[]` when empty.
    /// * `dependencies` is a nil-when-empty slice in the report contract: when
    ///   no risky imports exist it serializes as **`null`**, not `[]`. This is
    ///   the first-byte distinction in the `run/history_imports.json` golden
    ///   (`"dependencies":null`).
    /// * `aggregate` is itself struct-origin: declaration order
    ///   `total_imports`, `external_imports`, `internal_imports`,
    ///   `unique_packages`, `external_ratio`; `external_ratio` is a float routed
    ///   through cf-gojson's shortest-round-trip formatter (`0` for the empty
    ///   report).
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

        // Nil-when-empty slice -> `null` (report-format contract): the
        // dependency metric only ever appends to an initially-nil slice, so an
        // empty result serializes as null, not [].
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
        agg.push("external_imports", GoValue::Int(self.aggregate.external_imports));
        agg.push("internal_imports", GoValue::Int(self.aggregate.internal_imports));
        agg.push("unique_packages", GoValue::Int(self.aggregate.unique_packages));
        agg.push("external_ratio", GoValue::Float(self.aggregate.external_ratio));

        let mut root = GoMap::new_struct();
        root.push("import_list", GoValue::Array(import_list));
        root.push("categories", GoValue::Array(categories));
        root.push("dependencies", dependencies);
        root.push("aggregate", GoValue::Map(agg));
        GoValue::Map(root)
    }

    /// Converts the metrics to a [`cf_gojson::GoValue`] for byte-exact machine
    /// YAML output (the `--format yaml` static path).
    ///
    /// Identical to [`Self::to_go_value`] (struct field-declaration key order —
    /// `import_list`, `categories`, `dependencies`, `aggregate`) with one
    /// encoder-specific difference in the report contract: the YAML encoding of
    /// a nil-when-empty slice is an empty sequence `[]`, not `null`. So when no
    /// risky imports exist, `dependencies` is `[]` here (vs `null` in the json
    /// encoder), matching the `static/static_imports.yaml` golden
    /// (`dependencies: []`). `external_ratio` routes through cf-goyaml's float
    /// formatter.
    #[must_use]
    pub fn to_go_value_yaml(&self) -> cf_gojson::GoValue {
        use cf_gojson::GoValue;

        let mut root = match self.to_go_value() {
            GoValue::Map(m) => m,
            other => return other,
        };
        // YAML contract: nil slice -> `[]` (json -> `null`). Replace a `null`
        // `dependencies` with an empty array.
        if matches!(root.get("dependencies"), Some(GoValue::Null)) {
            root.insert("dependencies", GoValue::Array(Vec::new()));
        }
        GoValue::Map(root)
    }
}

/// Runs all metrics over a report and returns the combined result.
///
/// # Errors
/// Never fails; the `Result` keeps call-site parity with the analyzer
/// interface.
pub fn compute_all_metrics(
    report: &ReportValue,
) -> Result<ComputedMetrics, std::convert::Infallible> {
    let input = ReportData::parse(report)?;
    Ok(ComputedMetrics {
        import_list: compute_import_list(&input),
        categories: compute_categories(&input),
        dependencies: compute_dependencies(&input),
        aggregate: compute_aggregate(&input),
    })
}

/// Computes the categorized import list, sorted by category then path.
#[must_use]
pub fn compute_import_list(input: &ReportData) -> Vec<ImportData> {
    let mut result: Vec<ImportData> = input
        .imports
        .iter()
        .map(|imp| ImportData {
            path: imp.clone(),
            category: categorize_import(imp),
            is_external: is_external_import(imp),
        })
        .collect();

    // The (category, path) key is a total order, so stable vs unstable sorting
    // cannot diverge here.
    result.sort_by(|a, b| {
        if a.category == b.category {
            a.path.cmp(&b.path)
        } else {
            a.category.cmp(&b.category)
        }
    });
    result
}

/// Computes the category distribution, sorted by count descending.
///
/// The reference implementation leaves equal-count tie order unspecified
/// (unstable sort over nondeterministic map iteration); this implementation
/// breaks ties by category name for determinism.
#[must_use]
pub fn compute_categories(input: &ReportData) -> Vec<ImportCategoryData> {
    use std::collections::BTreeMap;
    let mut categories: BTreeMap<String, i64> = BTreeMap::new();
    for imp in &input.imports {
        *categories.entry(categorize_import(imp)).or_insert(0) += 1;
    }
    let mut result: Vec<ImportCategoryData> = categories
        .into_iter()
        .map(|(category, count)| ImportCategoryData { category, count })
        .collect();
    result.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.category.cmp(&b.category)));
    result
}

/// Identifies potential dependency issues, in input order.
///
/// When both conditions hold, the long-path branch runs second and overwrites
/// the risk/reason — sequential `if`s, not `else if` (pinned classification
/// behavior).
#[must_use]
pub fn compute_dependencies(input: &ReportData) -> Vec<ImportDependencyData> {
    let mut result = Vec::new();
    for imp in &input.imports {
        let mut risk_level = String::new();
        let mut reason = String::new();

        if count_occurrences(imp, "..") >= DEEPLY_NESTED_THRESHOLD {
            risk_level = "MEDIUM".to_string();
            reason =
                "Deeply nested relative import may indicate poor module structure".to_string();
        }

        if count_occurrences(imp, "/") >= LONG_PATH_THRESHOLD {
            risk_level = "LOW".to_string();
            reason = "Long import path may indicate overly complex package structure".to_string();
        }

        if !risk_level.is_empty() {
            result.push(ImportDependencyData {
                path: imp.clone(),
                risk_level,
                reason,
            });
        }
    }
    result
}

/// Computes aggregate statistics.
#[must_use]
pub fn compute_aggregate(input: &ReportData) -> AggregateData {
    use std::collections::BTreeSet;
    let mut agg = AggregateData {
        total_imports: input.imports.len() as i64,
        ..Default::default()
    };
    let mut packages: BTreeSet<&str> = BTreeSet::new();
    for imp in &input.imports {
        if is_external_import(imp) {
            agg.external_imports += 1;
        } else {
            agg.internal_imports += 1;
        }
        packages.insert(base_package(imp));
    }
    agg.unique_packages = packages.len() as i64;
    if agg.total_imports > 0 {
        agg.external_ratio = agg.external_imports as f64 / agg.total_imports as f64;
    }
    agg
}

/// Counts non-overlapping occurrences of `sub` in `s`.
fn count_occurrences(s: &str, sub: &str) -> usize {
    if sub.is_empty() {
        // Degenerate case kept for contract completeness: rune count + 1.
        return s.chars().count() + 1;
    }
    s.matches(sub).count()
}

/// Returns the base package: the substring before the first `/`.
fn base_package(imp: &str) -> &str {
    imp.split_once('/').map_or(imp, |(head, _)| head)
}

/// Categorizes an import as `relative`, `stdlib`, or `external`.
#[must_use]
pub fn categorize_import(imp: &str) -> String {
    if imp.starts_with('.') || imp.starts_with('/') {
        return "relative".to_string();
    }
    if is_standard_library(imp) {
        "stdlib".to_string()
    } else {
        "external".to_string()
    }
}

/// Reports whether an import is external (not relative, not stdlib).
#[must_use]
pub fn is_external_import(imp: &str) -> bool {
    if imp.starts_with('.') || imp.starts_with('/') {
        return false;
    }
    !is_standard_library(imp)
}

/// Reports whether an import's base package is a known standard-library package.
///
/// The stdlib table is frozen reference data (it decides classification in
/// machine output); `path` and `http` appear twice, which is harmless for
/// membership.
#[must_use]
pub fn is_standard_library(imp: &str) -> bool {
    const STDLIBS: &[&str] = &[
        // Go.
        "fmt", "os", "io", "net", "http", "encoding", "sync", "context", "time", "strings",
        "bytes", "bufio", "path", "filepath", "regexp", "sort", "math",
        // Python.
        "sys", "typing", "collections", "itertools", "functools", "json", "re",
        // JavaScript/Node.
        "fs", "path", "util", "events", "stream", "crypto", "http", "https",
    ];
    let base = base_package(imp);
    STDLIBS.contains(&base)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_IMPORT_STDLIB: &str = "fmt";
    const TEST_IMPORT_EXTERNAL: &str = "github.com/user/repo";
    const TEST_IMPORT_RELATIVE: &str = "../utils";

    fn report_with_imports(imports: &[&str], count: i64) -> ReportValue {
        let mut r = ReportValue::map();
        r.insert(
            "imports",
            ReportValue::List(imports.iter().map(|s| ReportValue::Str(s.to_string())).collect()),
        );
        r.insert("count", ReportValue::Int(count));
        r
    }

    fn data(imports: &[&str]) -> ReportData {
        ReportData {
            imports: imports.iter().map(|s| s.to_string()).collect(),
            count: 0,
        }
    }

    #[test]
    fn test_parse_report_data_empty() {
        let result = ReportData::parse(&ReportValue::map()).unwrap();
        assert!(result.imports.is_empty());
        assert_eq!(result.count, 0);
    }

    /// Byte-for-byte parity of the empty-imports report against the binding
    /// golden `tests/golden/run/history_imports.json` (history/imports
    /// json, 10 commits with no extracted imports). Locks: declaration field
    /// order (not byte-sorted), `import_list`/`categories` => `[]`,
    /// `dependencies` => `null` (nil slice), `external_ratio` => `0`, and NO
    /// trailing newline (compact encoding).
    #[test]
    fn empty_report_matches_history_imports_golden() {
        // The tick-to-report path always emits a non-empty report
        // (imports/author_index/tick_size keys), so metrics computation runs
        // even when no imports exist.
        let report = ReportValue::map();
        let metrics = compute_all_metrics(&report).unwrap();
        let bytes = cf_gojson::marshal(&metrics.to_go_value());
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "{\"import_list\":[],\"categories\":[],\"dependencies\":null,\
             \"aggregate\":{\"total_imports\":0,\"external_imports\":0,\
             \"internal_imports\":0,\"unique_packages\":0,\"external_ratio\":0}}"
        );
    }

    #[test]
    fn test_parse_report_data_all_fields() {
        let report = report_with_imports(&[TEST_IMPORT_STDLIB, TEST_IMPORT_EXTERNAL], 5);
        let result = ReportData::parse(&report).unwrap();
        assert_eq!(result.imports.len(), 2);
        assert_eq!(result.imports[0], TEST_IMPORT_STDLIB);
        assert_eq!(result.imports[1], TEST_IMPORT_EXTERNAL);
        assert_eq!(result.count, 5);
    }

    #[test]
    fn test_categorize_import() {
        let cases = [
            ("./utils", "relative"),
            ("../utils", "relative"),
            ("/absolute/path", "relative"),
            ("fmt", "stdlib"),
            ("encoding/json", "stdlib"),
            ("github.com/user/repo", "external"),
            ("somepackage", "external"),
        ];
        for (imp, expected) in cases {
            assert_eq!(categorize_import(imp), expected, "imp={imp}");
        }
    }

    #[test]
    fn test_is_external_import() {
        let cases = [
            ("./utils", false),
            ("../utils", false),
            ("fmt", false),
            ("github.com/user/repo", true),
            ("somepackage", true),
        ];
        for (imp, expected) in cases {
            assert_eq!(is_external_import(imp), expected, "imp={imp}");
        }
    }

    #[test]
    fn test_is_standard_library() {
        let true_cases = [
            "fmt", "os", "io", "net", "net/http", "encoding/json", "sync", "context", "time",
            "strings", "bytes", "path/filepath", "regexp", "sort", "math/rand", "sys", "typing",
            "collections", "itertools", "functools", "json", "re", "fs", "util", "events",
            "stream", "crypto", "https",
        ];
        for imp in true_cases {
            assert!(is_standard_library(imp), "expected stdlib: {imp}");
        }
        for imp in ["github.com/user/repo", "somepackage"] {
            assert!(!is_standard_library(imp), "expected non-stdlib: {imp}");
        }
    }

    #[test]
    fn test_import_list_metric_empty() {
        assert!(compute_import_list(&ReportData::default()).is_empty());
    }

    #[test]
    fn test_import_list_metric_single_import() {
        let result = compute_import_list(&data(&[TEST_IMPORT_STDLIB]));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, TEST_IMPORT_STDLIB);
        assert_eq!(result[0].category, "stdlib");
        assert!(!result[0].is_external);
    }

    #[test]
    fn test_import_list_metric_multiple_imports_sorted() {
        let result = compute_import_list(&data(&[
            TEST_IMPORT_EXTERNAL,
            TEST_IMPORT_STDLIB,
            TEST_IMPORT_RELATIVE,
            "os",
        ]));
        assert_eq!(result.len(), 4);
        // external < relative < stdlib.
        assert_eq!(result[0].category, "external");
        assert_eq!(result[1].category, "relative");
        assert_eq!(result[2].category, "stdlib");
        assert_eq!(result[3].category, "stdlib");
        // Within stdlib, sorted by path: "fmt" < "os".
        assert_eq!(result[2].path, TEST_IMPORT_STDLIB);
        assert_eq!(result[3].path, "os");
    }

    #[test]
    fn test_import_category_metric_empty() {
        assert!(compute_categories(&ReportData::default()).is_empty());
    }

    #[test]
    fn test_import_category_metric_all_categories() {
        let result = compute_categories(&data(&[
            TEST_IMPORT_STDLIB,
            "os",
            "io",
            TEST_IMPORT_EXTERNAL,
            "github.com/other/pkg",
            TEST_IMPORT_RELATIVE,
        ]));
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].category, "stdlib");
        assert_eq!(result[0].count, 3);
        assert_eq!(result[1].category, "external");
        assert_eq!(result[1].count, 2);
        assert_eq!(result[2].category, "relative");
        assert_eq!(result[2].count, 1);
    }

    #[test]
    fn test_import_dependency_metric_empty() {
        assert!(compute_dependencies(&ReportData::default()).is_empty());
    }

    #[test]
    fn test_import_dependency_metric_no_issues() {
        let result = compute_dependencies(&data(&[
            TEST_IMPORT_STDLIB,
            TEST_IMPORT_EXTERNAL,
            TEST_IMPORT_RELATIVE,
        ]));
        assert!(result.is_empty());
    }

    #[test]
    fn test_import_dependency_metric_deeply_nested_relative() {
        let result = compute_dependencies(&data(&["../../../utils", "../../../../other/utils"]));
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].risk_level, "MEDIUM");
        assert!(result[0].reason.contains("Deeply nested"));
    }

    #[test]
    fn test_import_dependency_metric_long_path() {
        let result =
            compute_dependencies(&data(&["github.com/org/repo/pkg/internal/utils/helper"]));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].risk_level, "LOW");
        assert!(result[0].reason.contains("Long import path"));
    }

    #[test]
    fn test_aggregate_metric_empty() {
        let result = compute_aggregate(&ReportData::default());
        assert_eq!(result.total_imports, 0);
        assert_eq!(result.external_imports, 0);
        assert_eq!(result.internal_imports, 0);
        assert_eq!(result.unique_packages, 0);
        assert!((result.external_ratio - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_aggregate_metric_mixed_imports() {
        let result = compute_aggregate(&data(&[
            TEST_IMPORT_STDLIB,
            "os",
            TEST_IMPORT_EXTERNAL,
            "github.com/other",
            TEST_IMPORT_RELATIVE,
        ]));
        assert_eq!(result.total_imports, 5);
        assert_eq!(result.external_imports, 2);
        assert_eq!(result.internal_imports, 3);
        assert!(result.unique_packages >= 3);
        assert!((result.external_ratio - 2.0 / 5.0).abs() < 0.01);
    }

    #[test]
    fn test_aggregate_metric_all_external() {
        let result = compute_aggregate(&data(&[TEST_IMPORT_EXTERNAL, "github.com/other/pkg"]));
        assert_eq!(result.total_imports, 2);
        assert_eq!(result.external_imports, 2);
        assert_eq!(result.internal_imports, 0);
        assert!((result.external_ratio - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_compute_all_metrics_empty() {
        let result = compute_all_metrics(&ReportValue::map()).unwrap();
        assert!(result.import_list.is_empty());
        assert!(result.categories.is_empty());
        assert!(result.dependencies.is_empty());
        assert_eq!(result.aggregate.total_imports, 0);
    }

    #[test]
    fn test_compute_all_metrics_full() {
        let report = report_with_imports(
            &[
                TEST_IMPORT_STDLIB,
                TEST_IMPORT_EXTERNAL,
                TEST_IMPORT_RELATIVE,
                "../../../deep/nested",
            ],
            4,
        );
        let result = compute_all_metrics(&report).unwrap();
        assert_eq!(result.import_list.len(), 4);
        assert!(result.categories.len() >= 2);
        assert_eq!(result.dependencies.len(), 1);
        assert_eq!(result.dependencies[0].risk_level, "MEDIUM");
        assert_eq!(result.aggregate.total_imports, 4);
    }
}
