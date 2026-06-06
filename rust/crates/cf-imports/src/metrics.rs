//! Import metrics computation.
//!
//! Port of `internal/analyzers/imports/metrics.go`. Given a parsed report
//! ([`ReportData`]), it computes a categorized import list, a category
//! distribution, dependency-risk findings, and aggregate statistics
//! ([`ComputedMetrics`]) — the structure the static analyzer marshals in the
//! json/yaml/bin formats.
//!
//! The classification helpers ([`categorize_import`], [`is_external_import`],
//! [`is_standard_library`]) are data-parity-critical: they decide which/what
//! values appear in machine output, so the stdlib table and prefix rules are
//! reproduced verbatim from the Go source.

use crate::report::ReportValue;

/// Threshold: `..` occurrences flagged as a deeply-nested relative import.
const DEEPLY_NESTED_THRESHOLD: usize = 3;
/// Threshold: `/` occurrences flagged as an overly-long import path.
const LONG_PATH_THRESHOLD: usize = 5;

/// Parsed input data for metrics computation.
///
/// Mirrors Go `ReportData`.
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
    /// Mirrors Go `ParseReportData`. The `imports` key is read as a string list
    /// when present; otherwise (the binary-encode -> JSON-decode round trip
    /// renames it to `import_list` with `{"path": ...}` objects) paths are pulled
    /// from the `path` field of each `import_list` entry. `count` accepts an int
    /// or a float (the latter from JSON decode).
    ///
    /// # Errors
    /// Never fails; the `Result` matches the Go signature for call-site parity.
    pub fn parse(report: &ReportValue) -> Result<ReportData, std::convert::Infallible> {
        let mut data = ReportData::default();
        let map = match report.as_map() {
            Some(m) => m,
            None => return Ok(data),
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

/// Extracts string paths from a JSON-decoded `import_list`.
///
/// Mirrors Go `extractImportPaths`: the list is `[]any` of `{"path": ...}`.
fn extract_import_paths(items: &ReportValue) -> Vec<String> {
    let list = match items {
        ReportValue::List(l) => l,
        _ => return Vec::new(),
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

/// Information about a single import. Mirrors Go `ImportData`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportData {
    /// The import path.
    pub path: String,
    /// `relative`, `stdlib`, or `external`.
    pub category: String,
    /// Whether the import is external (not stdlib, not relative).
    pub is_external: bool,
}

/// Import count for a single category. Mirrors Go `ImportCategoryData`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportCategoryData {
    /// The category name.
    pub category: String,
    /// Number of imports in the category.
    pub count: i64,
}

/// A potential dependency issue. Mirrors Go `ImportDependencyData`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDependencyData {
    /// The offending import path.
    pub path: String,
    /// `MEDIUM` or `LOW`.
    pub risk_level: String,
    /// Human-readable reason.
    pub reason: String,
}

/// Aggregate summary statistics. Mirrors Go `AggregateData`.
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

/// All computed metric results. Mirrors Go `ComputedMetrics`.
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
    /// Converts the metrics to a [`ReportValue`] for JSON/YAML/bin marshaling.
    ///
    /// Field/key names match the Go `json:`/`yaml:` struct tags exactly so the
    /// go-compat encoders reproduce the original bytes. The top-level wrapper is
    /// a struct-origin object: in Go its key order is field declaration order
    /// (`import_list, categories, dependencies, aggregate`). The local
    /// [`ReportValue::Map`] byte-sorts keys, which is correct for `map`-origin
    /// objects but NOT for struct-origin wrappers — wiring the real cf-gojson
    /// dual-mode `GoMap` (declaration-order for structs) is required for true
    /// byte-identity here. See crate todos.
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

    /// Converts the metrics to a [`cf_gojson::GoValue`] for byte-identical Go
    /// `encoding/json` output (DESIGN §2/§3). This is the authoritative encoder
    /// path; [`Self::to_report_value`] is the legacy local shim.
    ///
    /// Byte-parity details reproduced from Go `*ComputedMetrics`:
    ///
    /// * The top-level object is **struct-origin** ([`MapOrigin::Struct`]), so
    ///   keys are emitted in Go field-declaration order — `import_list`,
    ///   `categories`, `dependencies`, `aggregate` — **not** byte-sorted.
    /// * `import_list` and `categories` come from `make([]T, 0, n)` in Go (never
    ///   nil), so they always serialize as `[]` when empty.
    /// * `dependencies` comes from a Go `var result []ImportDependencyData` (nil
    ///   until the first `append`), so when no risky imports exist it is a **nil
    ///   slice** and Go marshals it as `null`, not `[]`. This is the first-byte
    ///   distinction in the `run/history_imports.json` golden
    ///   (`"dependencies":null`).
    /// * `aggregate` is itself a struct: declaration order `total_imports`,
    ///   `external_imports`, `internal_imports`, `unique_packages`,
    ///   `external_ratio`; `external_ratio` is a `float64` routed through
    ///   cf-gojson's Go-`'g'` formatter (`0` for the empty report).
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

        // Go nil-slice -> `null`. The Dependency metric only ever `append`s to a
        // `var result []…` (nil) slice, so an empty result is nil, not `[]`.
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

    /// Converts the metrics to a [`cf_gojson::GoValue`] for byte-identical Go
    /// `gopkg.in/yaml.v3` output (the `--format yaml` static path:
    /// `imports.Analyzer.FormatReportYAML` = `yaml.Marshal(*ComputedMetrics)`).
    ///
    /// This is identical to [`Self::to_go_value`] (struct field-declaration key
    /// order — `import_list`, `categories`, `dependencies`, `aggregate`) with one
    /// encoder-specific difference: yaml.v3 marshals a **nil slice** as an empty
    /// sequence `[]`, not `null`. So when no risky imports exist, `dependencies`
    /// is `[]` here (vs `null` in the json encoder). This matches the
    /// `static/static_imports.yaml` golden (`dependencies: []`). `external_ratio`
    /// is a `float64` routed through cf-goyaml's go-`'g'` float formatter.
    #[must_use]
    pub fn to_go_value_yaml(&self) -> cf_gojson::GoValue {
        use cf_gojson::GoValue;

        let mut root = match self.to_go_value() {
            GoValue::Map(m) => m,
            other => return other,
        };
        // yaml.v3: nil slice -> `[]` (json -> `null`). Replace a `null`
        // `dependencies` with an empty array.
        if matches!(root.get("dependencies"), Some(GoValue::Null)) {
            root.insert("dependencies", GoValue::Array(Vec::new()));
        }
        GoValue::Map(root)
    }
}

/// Runs all metrics over a report and returns the combined result.
///
/// Mirrors Go `ComputeAllMetrics`.
///
/// # Errors
/// Never fails; the `Result` matches the Go signature.
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
///
/// Mirrors Go `ImportListMetric.Compute`.
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

    // Sort by category then path (Go sort.Slice is not stable, but the keys are
    // fully ordering here, so a stable sort yields the same result).
    result.sort_by(|a, b| {
        if a.category != b.category {
            a.category.cmp(&b.category)
        } else {
            a.path.cmp(&b.path)
        }
    });
    result
}

/// Computes the category distribution, sorted by count descending.
///
/// Mirrors Go `ImportCategoryMetric.Compute`. Go iterates a `map[string]int`
/// (nondeterministic order) then sorts by count descending with an unstable
/// sort; for ties the Go order is not defined. This port sorts by count
/// descending and, to be deterministic, breaks ties by category name.
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
/// Mirrors Go `ImportDependencyMetric.Compute`. When both conditions hold, the
/// long-path branch runs second and overwrites the risk/reason, exactly as in
/// the Go code (sequential `if`s, not `else if`).
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
///
/// Mirrors Go `AggregateMetric.Compute`.
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

/// Counts non-overlapping occurrences of `sub` in `s` (Go `strings.Count`).
fn count_occurrences(s: &str, sub: &str) -> usize {
    if sub.is_empty() {
        // Go strings.Count(s, "") == utf8.RuneCount(s)+1; not used here.
        return s.chars().count() + 1;
    }
    s.matches(sub).count()
}

/// Returns the base package: the substring before the first `/` (Go
/// `strings.Split(imp, "/")[0]`).
fn base_package(imp: &str) -> &str {
    match imp.split_once('/') {
        Some((head, _)) => head,
        None => imp,
    }
}

/// Categorizes an import as `relative`, `stdlib`, or `external`.
///
/// Mirrors Go `categorizeImport`.
pub fn categorize_import(imp: &str) -> String {
    if imp.starts_with('.') || imp.starts_with('/') {
        return "relative".to_string();
    }
    // Both remaining branches (with-slash and without-slash) reduce to: stdlib
    // if recognised, else external — exactly as the Go switch does.
    if is_standard_library(imp) {
        "stdlib".to_string()
    } else {
        "external".to_string()
    }
}

/// Reports whether an import is external (not relative, not stdlib).
///
/// Mirrors Go `isExternalImport`.
pub fn is_external_import(imp: &str) -> bool {
    if imp.starts_with('.') || imp.starts_with('/') {
        return false;
    }
    !is_standard_library(imp)
}

/// Reports whether an import's base package is a known standard-library package.
///
/// Mirrors Go `isStandardLibrary`. The stdlib table is reproduced verbatim and
/// in the same order as the Go source (data parity); `path` and `http` appear
/// twice in Go, which is harmless for membership.
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

    // --- ported from metrics_test.go ---

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
    /// golden `rust/tests/golden/run/history_imports.json` (history/imports json,
    /// 10 commits with no extracted imports). Locks: declaration field order
    /// (not byte-sorted), `import_list`/`categories` => `[]`, `dependencies` =>
    /// `null` (Go nil slice), `external_ratio` => `0`, and NO trailing newline
    /// (compact `json.Marshal`).
    #[test]
    fn empty_report_matches_history_imports_golden() {
        // ticksToReport always emits a non-empty report (imports/author_index/
        // tick_size keys), so ComputeAllMetrics runs even when no imports exist.
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
