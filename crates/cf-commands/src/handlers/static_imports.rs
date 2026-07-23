//! Static-analysis report path for the UAST `static/imports` analyzer.
//!
//! Reproduces the reference static pipeline for `codefang run --analyzers
//! static/imports` across its machine formats (yaml, bin, json).
//!
//! Pipeline (the reference `StaticService.streamFiles` → per-file `uast.Parser.Parse` →
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
//!     as the reference implementation emits it — the previous text-based extractor incorrectly stripped
//!     aliases.
//!  3. The cross-file [`cf_imports::Aggregator`] increments `total_files` for
//!     EVERY analyzed file (including markdown READMEs that contribute no
//!     imports — the reference implementation parses every supported file and folds an empty report) and
//!     sums per-import occurrence counts.
//!  4. `ComputeAllMetrics(report)` derives the structured metrics; the machine
//!     formats serialize that value through cf-goyaml / cf-reportutil / cf-gojson
//!     byte-identically.
//!
//! ## Markdown / ungrammared-but-supported files
//!
//! The reference implementation's `IsSupported` is true for markdown (a wired tree-sitter grammar), so a
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
    let metrics =
        cf_imports::compute_all_metrics(&report).expect("compute_all_metrics is infallible");
    Some(cf_goyaml::marshal(&metrics.to_go_value_yaml()))
}

/// Builds the `static/imports --format bin` report bytes for `root_path`.
///
/// Same aggregate report + [`cf_imports::compute_all_metrics`] as the yaml
/// sibling, but wrapped in a CFB1 envelope (`reportutil.EncodeBinaryEnvelope`).
pub fn imports_report_bin(root_path: &str) -> Option<Vec<u8>> {
    let report = aggregate_report_value(root_path)?;
    let metrics =
        cf_imports::compute_all_metrics(&report).expect("compute_all_metrics is infallible");
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
    report.insert(
        "count",
        cf_imports::ReportValue::Int(all_imports.len() as i64),
    );
    report.insert("total_files", cf_imports::ReportValue::Int(total_files));
    Some(report)
}

/// Builds the `static/imports --format json` structured-report bytes.
///
/// The reference JSON path is `StaticService.FormatJSON` →
/// `imports.CreateReportSection(aggregatedReport)` → `renderer.SectionToJSON` →
/// `json.NewEncoder(SetIndent("","  ")).Encode`. Imports is an INFO-only section
/// (`score = -1` ⇒ `score_label = "Info"`); the single-section overall is also
/// info-only. The `issues` list is every unique import, ordered by occurrence
/// count descending via the reference implementation's `sort.Slice` over a map iterated in random order —
/// so the tie order is intrinsically nondeterministic in the reference binary (the project MANIFEST
/// marks `static_imports.json` nonBinding). We emit a DETERMINISTIC, correct
/// ordering: count descending, ties broken by the aggregator's sorted import
/// keys (the same set the reference implementation emits, just stably ordered).
pub fn imports_report_json(root_path: &str) -> Option<Vec<u8>> {
    imports_report_json_flags(root_path, false)
}

/// [`imports_report_json`] with the `--per-file` flag applied: `per_file`
/// enables the reference implementation's section enrichment
/// (`StaticService.enrichWithPerFileData` → `JSONReport.EnrichWithPerFileData`
/// over the `PerFileRetainer` snapshots — one `JSONFileEntry` per ANALYZED
/// file, import-free files included, keyed into the section's `files` array).
#[must_use]
pub fn imports_report_json_flags(root_path: &str, per_file: bool) -> Option<Vec<u8>> {
    let root = imports_report_value_flags(root_path, per_file)?;
    Some(
        cf_gojson::Encoder::indented("  ")
            .with_trailing_newline(true)
            .encode(&root),
    )
}

/// Builds the `static/imports` `renderer.JSONReport` GoValue (single info-only
/// section), shared by the single-analyzer byte path and the multi-analyzer
/// static-JSON merge. `None` when the path cannot be walked.
#[must_use]
pub fn imports_report_value(root_path: &str) -> Option<GoValue> {
    imports_report_value_flags(root_path, false)
}

/// [`imports_report_value`] with the `--per-file` section enrichment.
#[must_use]
pub fn imports_report_value_flags(root_path: &str, per_file: bool) -> Option<GoValue> {
    let mut per_file_data: Vec<(String, Vec<String>)> = Vec::new();
    let (all_imports, total_files) = walk_and_count_opts(
        root_path,
        &Options::default(),
        per_file.then_some(&mut per_file_data),
    )?;
    let count = all_imports.len() as i64;

    // --- status / score (info-only) ---
    let status = build_status_message(count);

    // --- key metrics (reference KeyMetrics order) ---
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
    // the reference implementation sorts `import_counts` (a map iterated in RANDOM order) by count desc via
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

    // --- --per-file: one JSONFileEntry per ANALYZED file, in walk order (the
    // reference implementation ranges the retainer map here — run-to-run
    // random; the oracle's measured-variance canonicalization compares the set) ---
    let file_entries: Option<Vec<GoValue>> = if per_file {
        Some(
            per_file_data
                .iter()
                .map(|(rel, imports)| build_file_entry(rel, imports))
                .collect(),
        )
    } else {
        None
    };

    // --- section (JSONSection field order) ---
    let mut section = GoMap::new(MapOrigin::Struct);
    section.push("title", GoValue::Str("IMPORTS".to_string()));
    section.push("score_label", GoValue::Str("Info".to_string()));
    section.push("status", GoValue::Str(status));
    section.push("metrics", GoValue::Array(metrics));
    // Distribution() returns nil ⇒ omitempty omits it.
    section.push("issues", GoValue::Array(issues));
    // --per-file enrichment: `files` sits between `issues` and `score`
    // (renderer.JSONSection field order; omitempty without the flag).
    if let Some(entries) = file_entries {
        section.push("files", GoValue::Array(entries));
    }
    section.push("score", GoValue::Float(-1.0));

    // --- top-level JSONReport (overall info-only) ---
    let mut root = GoMap::new(MapOrigin::Struct);
    root.push("overall_score_label", GoValue::Str("Info".to_string()));
    root.push("sections", GoValue::Array(vec![GoValue::Map(section)]));
    root.push("overall_score", GoValue::Float(-1.0));

    Some(GoValue::Map(root))
}

/// Builds one `renderer.JSONFileEntry` for `--per-file` (the reference
/// `SectionToJSONFileEntry` over `imports.CreateReportSection(perFileReport)`):
/// the per-file report is the analyzer's `{imports, count}` map — no
/// `import_counts` — so the issues take the alphabetical-list fallback, each
/// valued `"1"` and located at the stamped relative path, and `Total Files`
/// reads the absent key as 0. Imports stays info-only per file (`score = -1`).
fn build_file_entry(rel_path: &str, imports: &[String]) -> GoValue {
    let count = imports.len() as i64;

    let metric = |label: &str, value: String| {
        let mut m = GoMap::new(MapOrigin::Struct);
        m.push("label", GoValue::Str(label.to_string()));
        m.push("value", GoValue::Str(value));
        GoValue::Map(m)
    };
    let metrics = vec![
        metric("Unique Imports", cf_reportutil::format_int(count)),
        metric("Total Files", cf_reportutil::format_int(0)),
    ];

    let mut sorted: Vec<&String> = imports.iter().collect();
    sorted.sort();
    let issues: Vec<GoValue> = sorted
        .iter()
        .map(|name| {
            let mut m = GoMap::new(MapOrigin::Struct);
            m.push("name", GoValue::Str((*name).clone()));
            m.push("location", GoValue::Str(rel_path.to_string()));
            m.push("value", GoValue::Str("1".to_string()));
            m.push("severity", GoValue::Str("info".to_string()));
            GoValue::Map(m)
        })
        .collect();

    let mut entry = GoMap::new(MapOrigin::Struct);
    entry.push("file_path", GoValue::Str(rel_path.to_string()));
    entry.push("score_label", GoValue::Str("Info".to_string()));
    entry.push("status", GoValue::Str(build_status_message(count)));
    entry.push("metrics", GoValue::Array(metrics));
    // Distribution() returns nil for imports ⇒ omitempty omits it.
    entry.push("issues", GoValue::Array(issues));
    entry.push("score", GoValue::Float(-1.0));
    GoValue::Map(entry)
}

/// Builds the AGGREGATED RAW `analyze.Report` GoValue for `static/imports` —
/// the value the reference implementation's `imports.Aggregator.GetResult()` returns,
/// which is what `--format plot` consumes and what `writeReportJSON`
/// serializes into `report.json`:
///
/// * `imports`: the unique import paths. the reference implementation materializes them from a
///   `map[string]int` in RANDOM iteration order (measured-nondeterministic;
///   the harness compares the multiset) — we emit the byte-sorted order.
/// * `import_counts`: import path → file-occurrence count.
/// * `count`: unique import count; `total_files`: every analyzed file.
#[must_use]
pub fn imports_raw_report_value(root_path: &str, opts: &Options) -> Option<GoValue> {
    let (all_imports, total_files) = walk_and_count_opts(root_path, opts, None)?;

    let imports: Vec<GoValue> = all_imports
        .keys()
        .map(|k| GoValue::Str(k.clone()))
        .collect();
    let mut import_counts = GoMap::new(MapOrigin::Map);
    for (imp, c) in &all_imports {
        import_counts.push(imp, GoValue::Int(*c));
    }

    let mut m = GoMap::new(MapOrigin::Map);
    m.push("imports", GoValue::Array(imports));
    m.push("import_counts", GoValue::Map(import_counts));
    m.push("count", GoValue::Int(all_imports.len() as i64));
    m.push("total_files", GoValue::Int(total_files));
    Some(GoValue::Map(m))
}

/// `imports.buildStatusMessage`.
fn build_status_message(count: i64) -> String {
    if count == 0 {
        "No import data available".to_string()
    } else {
        format!("Found {} unique imports", cf_reportutil::format_int(count))
    }
}

/// Walks `root_path`, parses each supported file to a real UAST, extracts its
/// imports, and folds them into a cross-file `import path -> file count` map
/// alongside the total analyzed-file count. Returns `None` when the path is
/// missing. Mirrors the per-file analyze + `imports.Aggregator` accumulation.
fn walk_and_count(root_path: &str) -> Option<(std::collections::BTreeMap<String, i64>, i64)> {
    walk_and_count_opts(root_path, &Options::default(), None)
}

/// [`walk_and_count`] with explicit path-policy options (the plot path passes
/// the run flags; the stdout formats keep the defaults). When `per_file_out`
/// is set, each analyzed file's deduplicated import list is also pushed as
/// `(relative path, imports)` in walk order — the reference `PerFileRetainer`
/// snapshot the `--per-file` section enrichment consumes.
fn walk_and_count_opts(
    root_path: &str,
    opts: &Options,
    mut per_file_out: Option<&mut Vec<(String, Vec<String>)>>,
) -> Option<(std::collections::BTreeMap<String, i64>, i64)> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let parser = Parser::new();

    let mut files: Vec<String> = Vec::new();
    collect_files(root, &parser, opts, &mut files);
    files.sort();

    // The reference aggregator increments total_files for every analyzed file and sums
    // per-import occurrence counts across files.
    let mut all_imports: std::collections::BTreeMap<String, i64> =
        std::collections::BTreeMap::new();
    let mut total_files: i64 = 0;

    for path in &files {
        let Ok(content) = std::fs::read(path) else {
            continue;
        };
        total_files += 1;
        // Parse failures on supported files (markdown without a wired grammar)
        // fold as an empty report: +1 file, no imports (see module docs).
        // extract_imports_from_uast deduplicates within the file in first-seen
        // order; the cross-file aggregator then counts file occurrences.
        let file_imports: Vec<String> = match parser.parse(path, &content) {
            Ok(node) => extract_imports_from_uast(&node),
            Err(_) => Vec::new(),
        };
        for imp in &file_imports {
            *all_imports.entry(imp.clone()).or_insert(0) += 1;
        }
        if let Some(out) = per_file_out.as_deref_mut() {
            out.push((make_relative_path(path, root_path), file_imports));
        }
    }

    if total_files == 0 {
        return None;
    }
    Some((all_imports, total_files))
}

/// The reference `filepath.Rel(rootPath, filePath)` (flat repos → path under
/// the root) — the `StampSourceFile` path stamped onto per-file reports.
fn make_relative_path(file_path: &str, root_path: &str) -> String {
    if root_path.is_empty() {
        return file_path.to_string();
    }
    match Path::new(file_path).strip_prefix(Path::new(root_path)) {
        Ok(rel) => rel.to_string_lossy().into_owned(),
        Err(_) => file_path.to_string(),
    }
}

/// Recursively collects UAST-supported, non-excluded regular files under `dir`
/// (lexical order; `.git` skipped). Mirrors `streamFiles` /
/// `ShouldSkipFolderNode`.
fn collect_files(dir: &Path, parser: &Parser, opts: &Options, out: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);

    for entry in entries {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            if super::should_skip_walk_dir(&entry.path(), &entry.file_name()) {
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
/// Faithful port of the reference `extractImportsFromUAST` (mirrored in
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

#[cfg(test)]
mod per_file_tests {
    use super::*;

    /// Fixture: one file with two imports, one import-free file.
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("a.go"),
            "package main\n\nimport (\n\t\"fmt\"\n\t\"os\"\n)\n\nfunc main() {\n\tfmt.Println(os.Args)\n}\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("types.go"),
            "package main\n\ntype Config struct {\n\tName string\n}\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn per_file_flag_emits_files_entries() {
        let dir = fixture();
        let bytes = imports_report_json_flags(dir.path().to_str().unwrap(), true).unwrap();
        let json = String::from_utf8(bytes).unwrap();
        // The section gains a `files` array with one JSONFileEntry per
        // ANALYZED file (import-free files included).
        assert!(json.contains("\"files\""), "files key missing:\n{json}");
        assert!(
            json.contains("\"file_path\": \"a.go\""),
            "a.go entry missing:\n{json}"
        );
        assert!(
            json.contains("\"file_path\": \"types.go\""),
            "types.go entry missing:\n{json}"
        );
        // Per-file status uses the per-file unique-import count.
        assert!(
            json.contains("\"status\": \"No import data available\""),
            "import-free per-file status missing:\n{json}"
        );
        // Per-file issues carry the stamped relative path as location.
        assert!(
            json.contains("\"location\": \"a.go\""),
            "per-file issue location missing:\n{json}"
        );
    }

    #[test]
    fn no_per_file_flag_omits_files() {
        let dir = fixture();
        let bytes = imports_report_json_flags(dir.path().to_str().unwrap(), false).unwrap();
        let json = String::from_utf8(bytes).unwrap();
        assert!(
            !json.contains("\"files\""),
            "files key must be omitted:\n{json}"
        );
        assert!(!json.contains("\"file_path\""));
    }
}
