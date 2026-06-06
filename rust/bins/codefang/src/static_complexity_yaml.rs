//! Static-analysis YAML report path for the UAST `static/complexity` analyzer.
//!
//! Reproduces `codefang run --analyzers static/complexity --format yaml` over a
//! Go source tree (`static/static_complexity.yaml`). The Go static pipeline
//! (run.go → `StaticService.AnalyzeFolder` → per-file UAST parse →
//! `complexity.Analyzer.Analyze` → `StampSourceFile`/`StampLanguage` →
//! `complexity.Aggregator` (detailed `functions` collection in file-walk order +
//! summed totals) → `StaticService.FormatPerAnalyzer` →
//! `complexity.FormatReportYAML` = `yaml.Marshal(ComputeAllMetrics(report))`)
//! reduces, for this single UAST analyzer with `--static-workers 1`, to:
//!
//!  1. a lexical directory walk (mirroring `filepath.WalkDir`), parsing every
//!     parser-supported file with [`cf_uast::Parser`];
//!  2. per file: run the ported [`cf_complexity::Analyzer`] (bridged from the
//!     canonical `cf_uast_node::Node` to the analyzer's minimal node model) to
//!     obtain the per-function metrics, stamped with the file's relative path,
//!     language, and directory;
//!  3. concatenating those per-file function lists **in walk order** (the
//!     `DetailedDataCollector` appends without re-sorting) plus summing the
//!     aggregate totals;
//!  4. `ComputeAllMetrics` over the assembled report — `function_complexity`
//!     (Go `sort.Slice` by cyclomatic desc, reproduced via [`crate::go_sort`]),
//!     `distribution`, `high_risk_functions` (Go `sort.Slice` by risk priority),
//!     `aggregate`;
//!  5. marshaling the `ComputedMetrics` struct through cf-goyaml
//!     (`gopkg.in/yaml.v3` parity; the static YAML path writes the marshaled
//!     metrics directly, with no version header).
//!
//! The aggregate `cognitive_complexity`/`nesting_depth` are emitted as `0`
//! because the aggregator stores them as float64 *averages*; `ParseReportData`'s
//! `int` type assertion fails on a float, leaving the zero value (a faithful Go
//! quirk — see `metrics.go` `parseReportScalars`).

use std::fs;
use std::path::{Path, PathBuf};

use cf_complexity::node::{Node as CNode, Positions as CPos};
use cf_complexity::{Analyzer, FunctionMetrics};
use cf_gojson::{GoMap, GoValue, MapOrigin};
use cf_goyaml::marshal;
use cf_uast::Parser;
use cf_uast_node::Node as UNode;

use crate::go_sort;

// Risk thresholds (metrics.go).
const CYCLOMATIC_THRESHOLD_HIGH: i64 = 10;
const CYCLOMATIC_THRESHOLD_MODERATE: i64 = 5;
const COGNITIVE_THRESHOLD_HIGH: i64 = 15;
const COGNITIVE_THRESHOLD_MODERATE: i64 = 7;
const NESTING_THRESHOLD_HIGH: i64 = 5;
const NESTING_THRESHOLD_MODERATE: i64 = 3;
const RISK_SCORE_CRITICAL: i64 = 5;
const RISK_SCORE_HIGH: i64 = 3;

/// One function's data as carried in the merged `functions` collection, in walk
/// order (mirrors `complexity.FunctionData` after `Stamp*`).
struct FunctionData {
    name: String,
    source_file: String,
    language: String,
    directory: String,
    cyclomatic: i64,
    cognitive: i64,
    nesting: i64,
    loc: i64,
}

/// Builds the `static/complexity --format yaml` report bytes for `root_path`.
/// Returns `None` when the path does not exist (caller falls through to the
/// blocked-dependency sentinel).
#[must_use]
pub fn complexity_report_yaml(root_path: &str) -> Option<Vec<u8>> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let parser = Parser::new();
    let analyzer = Analyzer;

    let mut functions: Vec<FunctionData> = Vec::new();
    let mut total_functions: i64 = 0;
    let mut total_complexity: i64 = 0;
    let mut decision_points: i64 = 0;
    let mut max_complexity: i64 = 0;

    walk(
        root,
        root_path,
        &parser,
        &analyzer,
        &mut functions,
        &mut total_functions,
        &mut total_complexity,
        &mut decision_points,
        &mut max_complexity,
    );

    let average_complexity = if total_functions > 0 {
        total_complexity as f64 / total_functions as f64
    } else {
        0.0
    };

    let metrics = build_computed_metrics(
        &functions,
        total_functions,
        average_complexity,
        max_complexity,
        total_complexity,
        decision_points,
    );

    Some(marshal(&metrics))
}

/// Recursively walks `dir` in lexical order (mirroring `filepath.WalkDir`).
/// `.git` is skipped; every parser-supported file is parsed and analyzed.
#[allow(clippy::too_many_arguments)]
fn walk(
    dir: &Path,
    root_path: &str,
    parser: &Parser,
    analyzer: &Analyzer,
    functions: &mut Vec<FunctionData>,
    total_functions: &mut i64,
    total_complexity: &mut i64,
    decision_points: &mut i64,
    max_complexity: &mut i64,
) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    for entry in entries {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if entry.file_name() == ".git" {
                continue;
            }
            walk(
                &path,
                root_path,
                parser,
                analyzer,
                functions,
                total_functions,
                total_complexity,
                decision_points,
                max_complexity,
            );
            continue;
        }

        let path_str = path.to_string_lossy();
        if !parser.is_supported(&path_str) {
            continue;
        }
        let Ok(content) = fs::read(&path) else { continue };
        let Ok(uroot) = parser.parse(&path_str, &content) else {
            continue;
        };

        let croot = bridge(&uroot);
        let metrics: Vec<FunctionMetrics> = analyzer.function_metrics(Some(&croot));
        if metrics.is_empty() {
            // A per-file report with no functions ("No functions found")
            // contributes nothing to the aggregator's count keys.
            continue;
        }

        let stamped = make_relative_path(&path, root_path);
        let dir_rel = parent_dir(&stamped);
        let language = parser.get_language(&path_str);

        let mut file_max: i64 = 0;
        *total_functions += metrics.len() as i64;
        for m in &metrics {
            *total_complexity += m.cyclomatic_complexity;
            *decision_points += m.decision_points;
            if m.cyclomatic_complexity > file_max {
                file_max = m.cyclomatic_complexity;
            }
        }
        if file_max > *max_complexity {
            *max_complexity = file_max;
        }

        for m in metrics {
            functions.push(FunctionData {
                name: m.name,
                source_file: stamped.clone(),
                language: language.clone(),
                directory: dir_rel.clone(),
                cyclomatic: m.cyclomatic_complexity,
                cognitive: m.cognitive_complexity,
                nesting: m.nesting_depth,
                loc: m.lines_of_code,
            });
        }
    }
}

/// Converts the canonical `cf_uast_node::Node` tree into the complexity
/// analyzer's minimal node model (`cf_complexity::node::Node`).
fn bridge(u: &UNode) -> CNode {
    let mut c = CNode::new(u.node_type.clone());
    c.token = u.token.clone();
    c.roles = u.roles.clone();
    for (k, v) in &u.props {
        c.props.insert(k.clone(), v.clone());
    }
    if let Some(p) = &u.pos {
        c.pos = Some(CPos {
            start_line: p.start_line as u32,
            start_col: p.start_col as u32,
            start_offset: p.start_offset as u32,
            end_line: p.end_line as u32,
            end_col: p.end_col as u32,
            end_offset: p.end_offset as u32,
        });
    }
    c.children = u.children.iter().map(bridge).collect();
    c
}

/// Go `analyze.MakeRelativePath(filePath, rootPath)`: path relative to root.
/// When the file is directly under the analyzed dir, this is the bare filename.
fn make_relative_path(path: &Path, root_path: &str) -> String {
    let root = Path::new(root_path);
    match path.strip_prefix(root) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

/// Go `filepath.Dir(stamped)`: directory portion (`.` for a bare filename).
fn parent_dir(stamped: &str) -> String {
    let p = PathBuf::from(stamped);
    match p.parent() {
        Some(d) if !d.as_os_str().is_empty() => d.to_string_lossy().into_owned(),
        _ => ".".to_string(),
    }
}

/// Builds the `ComputedMetrics` GoValue (struct-origin map, declaration order:
/// `function_complexity`, `distribution`, `high_risk_functions`, `aggregate`).
fn build_computed_metrics(
    functions: &[FunctionData],
    total_functions: i64,
    average_complexity: f64,
    max_complexity: i64,
    total_complexity: i64,
    decision_points: i64,
) -> GoValue {
    let function_complexity = build_function_complexity(functions);
    let distribution = build_distribution(functions);
    let high_risk = build_high_risk(functions);
    let aggregate = build_aggregate(
        total_functions,
        average_complexity,
        max_complexity,
        total_complexity,
        decision_points,
    );

    let mut m = GoMap::new(MapOrigin::Struct);
    m.push("function_complexity", function_complexity);
    m.push("distribution", distribution);
    m.push("high_risk_functions", high_risk);
    m.push("aggregate", aggregate);
    GoValue::Map(m)
}

/// `FunctionComplexityMetric.Compute`: per-function density + risk, then
/// `sort.Slice` by cyclomatic complexity descending (unstable; pdqsort).
fn build_function_complexity(functions: &[FunctionData]) -> GoValue {
    let mut idx: Vec<usize> = (0..functions.len()).collect();
    go_sort::slice(&mut idx, |&a, &b| {
        functions[a].cyclomatic > functions[b].cyclomatic
    });

    let mut arr = Vec::with_capacity(functions.len());
    for &i in &idx {
        let f = &functions[i];
        let density = if f.loc > 0 {
            f.cyclomatic as f64 / f.loc as f64
        } else {
            0.0
        };
        let risk = classify_function_risk(f.cyclomatic, f.cognitive, f.nesting);

        let mut o = GoMap::new(MapOrigin::Struct);
        o.push("name", GoValue::Str(f.name.clone()));
        push_omitempty(&mut o, "source_file", &f.source_file);
        push_omitempty(&mut o, "language", &f.language);
        push_omitempty(&mut o, "directory", &f.directory);
        o.push("cyclomatic_complexity", GoValue::Int(f.cyclomatic));
        o.push("cognitive_complexity", GoValue::Int(f.cognitive));
        o.push("nesting_depth", GoValue::Int(f.nesting));
        o.push("lines_of_code", GoValue::Int(f.loc));
        o.push("complexity_density", GoValue::Float(density));
        o.push("risk_level", GoValue::Str(risk.to_string()));
        arr.push(GoValue::Map(o));
    }
    GoValue::Array(arr)
}

fn push_omitempty(m: &mut GoMap, key: &str, value: &str) {
    if !value.is_empty() {
        m.push(key, GoValue::Str(value.to_string()));
    }
}

/// `DistributionMetric.Compute` = `stats.Distribution`: counts per complexity
/// level. The resulting `map[string]int` is byte-sorted by yaml.v3.
fn build_distribution(functions: &[FunctionData]) -> GoValue {
    let mut simple = 0i64;
    let mut moderate = 0i64;
    let mut complex = 0i64;
    for f in functions {
        if f.cyclomatic <= CYCLOMATIC_THRESHOLD_MODERATE {
            simple += 1;
        } else if f.cyclomatic <= CYCLOMATIC_THRESHOLD_HIGH {
            moderate += 1;
        } else {
            complex += 1;
        }
    }
    let mut m = GoMap::new(MapOrigin::Map);
    if complex > 0 {
        m.push("complex", GoValue::Int(complex));
    }
    if moderate > 0 {
        m.push("moderate", GoValue::Int(moderate));
    }
    if simple > 0 {
        m.push("simple", GoValue::Int(simple));
    }
    GoValue::Map(m)
}

/// `HighRiskFunctionMetric.Compute`: functions with at least one issue, then
/// `sort.Slice` by risk priority ascending (unstable; pdqsort).
fn build_high_risk(functions: &[FunctionData]) -> GoValue {
    struct HighRisk {
        idx: usize,
        issues: Vec<&'static str>,
        risk: &'static str,
    }

    let mut items: Vec<HighRisk> = Vec::new();
    for (i, f) in functions.iter().enumerate() {
        let mut issues: Vec<&'static str> = Vec::new();
        if f.cyclomatic >= CYCLOMATIC_THRESHOLD_HIGH {
            issues.push("High cyclomatic complexity");
        }
        if f.cognitive >= COGNITIVE_THRESHOLD_HIGH {
            issues.push("High cognitive complexity");
        }
        if f.nesting >= NESTING_THRESHOLD_HIGH {
            issues.push("Deep nesting");
        }
        if issues.is_empty() {
            continue;
        }
        let risk = classify_function_risk(f.cyclomatic, f.cognitive, f.nesting);
        items.push(HighRisk { idx: i, issues, risk });
    }

    go_sort::slice(&mut items, |a, b| {
        risk_priority(a.risk) < risk_priority(b.risk)
    });

    let mut arr = Vec::with_capacity(items.len());
    for it in items {
        let f = &functions[it.idx];
        let mut o = GoMap::new(MapOrigin::Struct);
        o.push("name", GoValue::Str(f.name.clone()));
        push_omitempty(&mut o, "source_file", &f.source_file);
        push_omitempty(&mut o, "language", &f.language);
        push_omitempty(&mut o, "directory", &f.directory);
        o.push("cyclomatic_complexity", GoValue::Int(f.cyclomatic));
        o.push("cognitive_complexity", GoValue::Int(f.cognitive));
        o.push("risk_level", GoValue::Str(it.risk.to_string()));
        let issues: Vec<GoValue> =
            it.issues.iter().map(|s| GoValue::Str((*s).to_string())).collect();
        o.push("issues", GoValue::Array(issues));
        arr.push(GoValue::Map(o));
    }
    GoValue::Array(arr)
}

/// `AggregateMetric.Compute`: summary stats. `cognitive_complexity` and
/// `nesting_depth` are 0 (float averages fail the int assertion in
/// `ParseReportData`).
fn build_aggregate(
    total_functions: i64,
    average_complexity: f64,
    max_complexity: i64,
    total_complexity: i64,
    decision_points: i64,
) -> GoValue {
    let health = calculate_health_score(average_complexity);
    let message = build_complexity_message(average_complexity);

    let mut m = GoMap::new(MapOrigin::Struct);
    m.push("total_functions", GoValue::Int(total_functions));
    m.push("average_complexity", GoValue::Float(average_complexity));
    m.push("max_complexity", GoValue::Int(max_complexity));
    m.push("total_complexity", GoValue::Int(total_complexity));
    m.push("cognitive_complexity", GoValue::Int(0));
    m.push("nesting_depth", GoValue::Int(0));
    m.push("decision_points", GoValue::Int(decision_points));
    m.push("health_score", GoValue::Float(health));
    m.push("message", GoValue::Str(message.to_string()));
    GoValue::Map(m)
}

/// `classifyFunctionRisk` (metrics.go).
fn classify_function_risk(cyclomatic: i64, cognitive: i64, nesting: i64) -> &'static str {
    let mut score = 0i64;
    if cyclomatic >= CYCLOMATIC_THRESHOLD_HIGH {
        score += 2;
    } else if cyclomatic >= CYCLOMATIC_THRESHOLD_MODERATE {
        score += 1;
    }
    if cognitive >= COGNITIVE_THRESHOLD_HIGH {
        score += 2;
    } else if cognitive >= COGNITIVE_THRESHOLD_MODERATE {
        score += 1;
    }
    if nesting >= NESTING_THRESHOLD_HIGH {
        score += 2;
    } else if nesting >= NESTING_THRESHOLD_MODERATE {
        score += 1;
    }
    if score >= RISK_SCORE_CRITICAL {
        "CRITICAL"
    } else if score >= RISK_SCORE_HIGH {
        "HIGH"
    } else if score >= 1 {
        "MEDIUM"
    } else {
        "LOW"
    }
}

/// `metrics.RiskPriority`.
fn risk_priority(level: &str) -> i64 {
    match level {
        "CRITICAL" => 0,
        "HIGH" => 1,
        "MEDIUM" => 2,
        _ => 3,
    }
}

/// `calculateHealthScore` (metrics.go).
fn calculate_health_score(avg: f64) -> f64 {
    const PERFECT: f64 = 100.0;
    const GOOD_BASE: f64 = 80.0;
    const MODERATE_BASE: f64 = 50.0;
    const AVG_LOW: f64 = 1.0;
    const AVG_GOOD: f64 = 3.0;
    const AVG_HIGH: f64 = 7.0;
    if avg <= AVG_LOW {
        PERFECT
    } else if avg <= AVG_GOOD {
        GOOD_BASE + (AVG_GOOD - avg) * 10.0
    } else if avg <= AVG_HIGH {
        MODERATE_BASE + (AVG_HIGH - avg) * 7.5
    } else {
        (MODERATE_BASE - (avg - AVG_HIGH) * 5.0).max(0.0)
    }
}

/// `buildComplexityMessage` (aggregator.go).
fn build_complexity_message(score: f64) -> &'static str {
    if score <= 1.0 {
        "Excellent complexity - functions are simple and maintainable"
    } else if score <= 3.0 {
        "Good complexity - functions have reasonable complexity"
    } else if score <= 7.0 {
        "Fair complexity - some functions could be simplified"
    } else {
        "High complexity - functions are complex and should be refactored"
    }
}
