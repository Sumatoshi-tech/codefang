//! Static-analysis report path for the UAST `static/imports` analyzer.
//!
//! Reproduces the Go static pipeline for `codefang run --analyzers
//! static/imports` across its machine formats (yaml, bin, json).
//!
//! Pipeline (Go `StaticService.streamFiles` → per-file `uast.Parser.Parse` →
//! `imports.Analyzer.Analyze` (`extractImportsFromUAST`) → `imports.Aggregator`
//! → format-specific serialization):
//!
//!  1. `streamFiles` walks `rootPath` with `filepath.WalkDir` (lexical order,
//!     `.git` skipped), keeping every UAST-supported, non-vendor/-generated file
//!     (`ShouldSkipFolderNode` → `parser.IsSupported`, `pathpolicy.Exclude`).
//!  2. Each file is parsed to a real UAST and run through
//!     [`cf_imports::extract_imports_from_uast`] — the import path is taken from
//!     each `Import` node's token (or first literal/identifier child), cleaned
//!     (quotes/semicolons trimmed, statement forms parsed), and **deduplicated
//!     within the file in first-seen order**. Crucially the import token retains
//!     any alias prefix (e.g. `fwk "k8s.io/kube-scheduler/framework`), exactly
//!     as Go emits it — the previous text-based extractor incorrectly stripped
//!     aliases.
//!  3. The cross-file [`cf_imports::Aggregator`] increments `total_files` for
//!     EVERY analyzed file (including markdown READMEs that contribute no
//!     imports — Go parses every supported file and folds an empty report) and
//!     sums per-import occurrence counts.
//!  4. `ComputeAllMetrics(report)` derives the structured metrics; the machine
//!     formats serialize that value through cf-goyaml / cf-reportutil / cf-gojson
//!     byte-identically.
//!
//! ## Markdown / ungrammared-but-supported files
//!
//! Go's `IsSupported` is true for markdown (a wired tree-sitter grammar), so a
//! `README.md` is parsed (to a tree with no import nodes) and still counts as
//! one file in `total_files`. Rust may lack a wired grammar for some supported
//! extensions; a parse failure on a supported file is therefore folded as that
//! same empty report (`total_files += 1`, no imports) — keeping `total_files`
//! (and thus every derived metric and the JSON `Total Files` value) identical.

use std::path::Path;

use cf_gojson::{GoMap, GoValue, MapOrigin};
use cf_pathpolicy::{exclude, Options};
use cf_uast::{Node as UastNode, Parser};

/// Builds the `static/imports --format yaml` report bytes for `root_path`.
/// Returns `None` when the path does not exist (the caller falls through to the
/// blocked-dependency sentinel).
pub fn imports_report_yaml(root_path: &str) -> Option<Vec<u8>> {
    let report = aggregate_report_value(root_path)?;
    let metrics = cf_imports::compute_all_metrics(&report).expect("compute_all_metrics is infallible");
    Some(cf_goyaml::marshal(&metrics.to_go_value_yaml()))
}

/// Builds the `static/imports --format bin` report bytes for `root_path`.
///
/// Same aggregate report + [`cf_imports::compute_all_metrics`] as the yaml
/// sibling, but wrapped in a CFB1 envelope (`reportutil.EncodeBinaryEnvelope`).
pub fn imports_report_bin(root_path: &str) -> Option<Vec<u8>> {
    let report = aggregate_report_value(root_path)?;
    let metrics = cf_imports::compute_all_metrics(&report).expect("compute_all_metrics is infallible");
    Some(
        cf_reportutil::encode_binary_envelope(&metrics.to_go_value())
            .expect("imports metrics never exceed the CFB1 length cap"),
    )
}

/// Aggregates into the `cf_imports::ReportValue` aggregator report (the
/// `imports.Aggregator.GetResult` shape) consumed by `compute_all_metrics`.
fn aggregate_report_value(root_path: &str) -> Option<cf_imports::ReportValue> {
    let (all_imports, total_files) = walk_and_count(root_path)?;
    let imports: Vec<cf_imports::ReportValue> = all_imports
        .keys()
        .map(|k| cf_imports::ReportValue::Str(k.clone()))
        .collect();
    let mut import_counts = cf_imports::ReportValue::map();
    for (imp, c) in &all_imports {
        import_counts.insert(imp.clone(), cf_imports::ReportValue::Int(*c));
    }
    let mut report = cf_imports::ReportValue::map();
    report.insert("imports", cf_imports::ReportValue::List(imports));
    report.insert("import_counts", import_counts);
    report.insert("count", cf_imports::ReportValue::Int(all_imports.len() as i64));
    report.insert("total_files", cf_imports::ReportValue::Int(total_files));
    Some(report)
}

/// Builds the `static/imports --format json` structured-report bytes.
///
/// The Go JSON path is `StaticService.FormatJSON` →
/// `imports.CreateReportSection(aggregatedReport)` → `renderer.SectionToJSON` →
/// `json.NewEncoder(SetIndent("","  ")).Encode`. Imports is an INFO-only section
/// (`score = -1` ⇒ `score_label = "Info"`); the single-section overall is also
/// info-only. The `issues` list is every unique import, ordered by occurrence
/// count descending via Go's `sort.Slice` over a map iterated in random order —
/// so the tie order is intrinsically Go-nondeterministic (the project MANIFEST
/// marks `static_imports.json` nonBinding). We emit a DETERMINISTIC, correct
/// ordering: count descending, ties broken by the aggregator's sorted import
/// keys (the same set Go emits, just stably ordered).
pub fn imports_report_json(root_path: &str) -> Option<Vec<u8>> {
    let (all_imports, total_files) = walk_and_count(root_path)?;
    let count = all_imports.len() as i64;

    // --- status / score (info-only) ---
    let status = build_status_message(count);

    // --- key metrics (report_section.go KeyMetrics order) ---
    let metric = |label: &str, value: String| {
        let mut m = GoMap::new(MapOrigin::Struct);
        m.push("label", GoValue::Str(label.to_string()));
        m.push("value", GoValue::Str(value));
        GoValue::Map(m)
    };
    let metrics = vec![
        metric("Unique Imports", cf_reportutil::format_int(count)),
        metric("Total Files", cf_reportutil::format_int(total_files)),
    ];

    // --- issues: imports sorted by count desc (deterministic tie-break by key) ---
    // Go sorts `import_counts` (a map iterated in RANDOM order) by count desc via
    // an unstable sort.Slice, so its tie order is nondeterministic (the project
    // MANIFEST marks static_imports.json nonBinding). We emit a DETERMINISTIC,
    // correct ordering: count descending, ties broken by ascending import key.
    let mut entries: Vec<(String, i64)> =
        all_imports.iter().map(|(k, c)| (k.clone(), *c)).collect();
    // all_imports is a BTreeMap (keys already ascending); stable sort by count
    // desc keeps that ascending tie order.
    entries.sort_by(|a, b| b.1.cmp(&a.1));
    let issues: Vec<GoValue> = entries
        .iter()
        .map(|(name, c)| {
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("name", GoValue::Str(name.clone()));
            m.push("location", GoValue::Str(String::new()));
            m.push("value", GoValue::Str(cf_reportutil::format_int(*c)));
            m.push("severity", GoValue::Str("info".to_string()));
            GoValue::Map(m)
        })
        .collect();

    // --- section (JSONSection field order) ---
    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str("IMPORTS".to_string()));
    section.push("score_label", GoValue::Str("Info".to_string()));
    section.push("status", GoValue::Str(status));
    section.push("metrics", GoValue::Array(metrics));
    // Distribution() returns nil ⇒ omitempty omits it.
    section.push("issues", GoValue::Array(issues));
    section.push("score", GoValue::Float(-1.0));

    // --- top-level JSONReport (overall info-only) ---
    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("overall_score_label", GoValue::Str("Info".to_string()));
    root.push("sections", GoValue::Array(vec![GoValue::Map(section)]));
    root.push("overall_score", GoValue::Float(-1.0));

    Some(
        cf_gojson::Encoder::indented("  ")
            .with_trailing_newline(true)
            .encode(&GoValue::Map(root)),
    )
}

/// `imports.buildStatusMessage`.
fn build_status_message(count: i64) -> String {
    if count == 0 {
        "No import data available".to_string()
    } else {
        format!("Found {} unique imports", cf_reportutil::format_int(count))
    }
}

fn go_int(v: &GoValue) -> Option<i64> {
    match v {
        GoValue::Int(n) => Some(*n),
        _ => None,
    }
}

/// Walks `root_path`, parses each supported file to a real UAST, extracts its
/// imports, and folds them into a cross-file `import path -> file count` map
/// alongside the total analyzed-file count. Returns `None` when the path is
/// missing. Mirrors the per-file analyze + `imports.Aggregator` accumulation.
fn walk_and_count(root_path: &str) -> Option<(std::collections::BTreeMap<String, i64>, i64)> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let parser = Parser::new();
    let opts = Options::default();

    let mut files: Vec<String> = Vec::new();
    collect_files(root, &parser, &opts, &mut files);
    files.sort();

    // The Go aggregator increments total_files for every analyzed file and sums
    // per-import occurrence counts across files.
    let mut all_imports: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    let mut total_files: i64 = 0;

    for path in &files {
        let Ok(content) = std::fs::read(path) else { continue };
        total_files += 1;
        // Parse failures on supported files (markdown without a wired grammar)
        // fold as an empty report: +1 file, no imports (see module docs).
        let Ok(node) = parser.parse(path, &content) else { continue };

        // extract_imports_from_uast deduplicates within the file in first-seen
        // order; the cross-file aggregator then counts file occurrences.
        for imp in extract_imports_from_uast(&node) {
            *all_imports.entry(imp).or_insert(0) += 1;
        }
    }

    if total_files == 0 {
        return None;
    }
    Some((all_imports, total_files))
}

/// Recursively collects UAST-supported, non-excluded regular files under `dir`
/// (lexical order; `.git` skipped). Mirrors `streamFiles` /
/// `ShouldSkipFolderNode`.
fn collect_files(dir: &Path, parser: &Parser, opts: &Options, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    for entry in entries {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if entry.file_name() == ".git" {
                continue;
            }
            collect_files(&path, parser, opts, out);
            continue;
        }
        let path_str = path.to_string_lossy().to_string();
        if !parser.is_supported(&path_str) {
            continue;
        }
        if exclude(&path_str, None, opts) {
            continue;
        }
        out.push(path_str);
    }
}

/// Extracts deduplicated import strings from a real cf-uast tree.
///
/// Faithful port of Go `extractImportsFromUAST` (mirrored in
/// `cf_imports::extract_imports_from_uast`, here applied to `cf_uast::Node`):
/// pre-order traversal; a node contributes when its type is `Import` or it
/// carries the `Import` role. Duplicates (by extracted path) are dropped,
/// preserving first-seen order.
pub fn extract_imports_from_uast(root: &UastNode) -> Vec<String> {
    let mut imports: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    visit_pre_order(root, &mut |n: &UastNode| {
        if n.node_type == "Import" || has_role(n, "Import") {
            let p = extract_import_path(n);
            if !p.is_empty() && seen.insert(p.clone()) {
                imports.push(p);
            }
        }
    });
    imports
}

fn visit_pre_order<F: FnMut(&UastNode)>(n: &UastNode, f: &mut F) {
    f(n);
    for c in &n.children {
        visit_pre_order(c, f);
    }
}

fn has_role(n: &UastNode, role: &str) -> bool {
    n.roles.iter().any(|r| r == role)
}

/// `extractImportPath`: node token (cleaned), else search children.
fn extract_import_path(n: &UastNode) -> String {
    if !n.token.is_empty() {
        return clean_import_path(&n.token);
    }
    if n.children.is_empty() {
        return String::new();
    }
    extract_import_path_from_children(&n.children)
}

/// `extractImportPathFromChildren`: first literal-with-token, then
/// identifier-with-token, then recurse.
fn extract_import_path_from_children(children: &[UastNode]) -> String {
    for c in children {
        if c.node_type == "Literal" && !c.token.is_empty() {
            return clean_import_path(&c.token);
        }
    }
    for c in children {
        if c.node_type == "Identifier" && !c.token.is_empty() {
            return clean_import_path(&c.token);
        }
    }
    for c in children {
        let p = extract_import_path(c);
        if !p.is_empty() {
            return p;
        }
    }
    String::new()
}

/// `cleanImportPath`: trim `"`/`'`/`;` from both ends, skip empty/`{`/`}`, apply
/// the statement-form parser, else return the trimmed value.
fn clean_import_path(path: &str) -> String {
    let path = path.trim_matches(|c| c == '"' || c == '\'' || c == ';');
    if path.is_empty() || path == "{" || path == "}" {
        return String::new();
    }
    let parsed = parse_import_format(path);
    if !parsed.is_empty() {
        return parsed;
    }
    path.to_string()
}

/// `parseImportFormat`: Python/JS statement forms; empty for Go-style specs (so
/// `cleanImportPath` falls back to the bare token, retaining any alias prefix).
fn parse_import_format(path: &str) -> String {
    if path.starts_with("from ") {
        let parts: Vec<&str> = path.split_whitespace().collect();
        if parts.len() >= 2 {
            return parts[1].to_string();
        }
        return String::new();
    }
    if path.contains(" from ") {
        let parts: Vec<&str> = path.splitn(2, " from ").collect();
        if parts.len() >= 2 {
            return parts[1].trim_matches(|c| c == '"' || c == '\'').to_string();
        }
        return String::new();
    }
    if path.starts_with("import ") {
        let parts: Vec<&str> = path.split_whitespace().collect();
        if parts.len() >= 2 {
            return parts[1].trim_matches(|c| c == '"' || c == '\'').to_string();
        }
        return String::new();
    }
    if path.contains("import ") {
        let parts: Vec<&str> = path.splitn(2, "import ").collect();
        if parts.len() >= 2 {
            return parts[1].trim_matches(|c| c == '"' || c == '\'').to_string();
        }
        return String::new();
    }
    String::new()
}
