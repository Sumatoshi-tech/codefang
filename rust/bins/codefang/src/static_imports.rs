//! Static-analysis YAML report path for the UAST `static/imports` analyzer.
//!
//! Reproduces the `codefang run --analyzers static/imports --format yaml`
//! capture (`static/static_imports.yaml`). The Go static pipeline
//! (run.go → StaticService.AnalyzeFolder → UAST parse per file →
//! `imports.Analyzer.Analyze` (`extractImportsFromUAST`) → `imports.Aggregator`
//! (dedup across files into the `imports` report key) →
//! `StaticService.FormatPerAnalyzer` → `imports.Analyzer.FormatReportYAML`
//! = `yaml.Marshal(ComputeAllMetrics(report))`) reduces, for this single
//! UAST analyzer over a Go source tree, to:
//!
//!  1. a lexical directory walk (mirroring the composition path), collecting the
//!     deduplicated set of Go import paths across all `.go` files (the value the
//!     aggregator stores under the report `imports` key — order-independent, the
//!     metric computer re-sorts by category then path);
//!  2. `ComputedMetrics` via [`cf_imports::compute_all_metrics`];
//!  3. marshaling through cf-goyaml (`gopkg.in/yaml.v3` parity) — note yaml.v3
//!     renders the nil `dependencies` slice as `[]` (vs json `null`), handled by
//!     [`cf_imports::ComputedMetrics::to_go_value_yaml`].
//!
//! The set of imports is computed independent of map-iteration order, so the
//! sorted `import_list`/`categories`/`aggregate` output is deterministic.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Builds the `static/imports --format yaml` report bytes for `root_path`.
/// Returns `None` when the path does not exist (the caller falls through to the
/// blocked-dependency sentinel).
pub fn imports_report_yaml(root_path: &str) -> Option<Vec<u8>> {
    let metrics = compute_metrics(root_path)?;
    Some(cf_goyaml::marshal(&metrics.to_go_value_yaml()))
}

/// Builds the `static/imports --format bin` report bytes for `root_path`.
///
/// Same aggregate report + [`cf_imports::compute_all_metrics`] as the yaml
/// sibling, but wrapped in a CFB1 envelope (`reportutil.EncodeBinaryEnvelope`:
/// magic `CFB1` + little-endian u32 payload length + compact `encoding/json`
/// payload). Mirrors `imports.Analyzer.FormatReportBinary`. The bin payload
/// uses the json-shaped go-value (`dependencies` nil slice → `null`), distinct
/// from the yaml go-value (`[]`).
pub fn imports_report_bin(root_path: &str) -> Option<Vec<u8>> {
    let metrics = compute_metrics(root_path)?;
    Some(
        cf_reportutil::encode_binary_envelope(&metrics.to_go_value())
            .expect("imports metrics never exceed the CFB1 length cap"),
    )
}

/// Walks `root_path`, collects the deduplicated import set, and runs the imports
/// metric computer. Returns `None` when the path does not exist (the caller
/// falls through to the blocked-dependency sentinel).
fn compute_metrics(root_path: &str) -> Option<cf_imports::ComputedMetrics> {
    let root = Path::new(root_path);
    if !root.exists() {
        return None;
    }

    let mut imports: BTreeSet<String> = BTreeSet::new();
    walk(root, &mut imports);

    // The aggregator stores `imports` as a `[]string` (deduplicated across
    // files). compute_all_metrics re-sorts deterministically, so any order
    // works; a BTreeSet gives a stable, allocation-light dedup.
    let mut report = cf_imports::ReportValue::map();
    report.insert(
        "imports",
        cf_imports::ReportValue::List(
            imports.into_iter().map(cf_imports::ReportValue::Str).collect(),
        ),
    );

    Some(cf_imports::compute_all_metrics(&report).expect("compute_all_metrics is infallible"))
}

/// Recursively walks `dir` in lexical order (mirroring `filepath.WalkDir`),
/// extracting Go import paths from every `.go` file. `.git` is skipped.
fn walk(dir: &Path, imports: &mut BTreeSet<String>) {
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };

    let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    for entry in entries {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            if entry.file_name() == ".git" {
                continue;
            }
            walk(&path, imports);
            continue;
        }

        if path.extension().and_then(|e| e.to_str()) == Some("go") {
            if let Ok(src) = fs::read_to_string(&path) {
                extract_go_imports(&src, imports);
            }
        }
    }
}

/// Extracts import paths from Go source, mirroring `extractImportsFromUAST` +
/// `extractImportPath`/`cleanImportPath`: each import spec's quoted path string
/// is the import identifier; the optional alias (`alias`, `_`, `.`) and quotes
/// are stripped. Both grouped (`import ( ... )`) and single
/// (`import "path"` / `import alias "path"`) forms are handled.
fn extract_go_imports(src: &str, imports: &mut BTreeSet<String>) {
    let mut in_group = false;

    for raw in src.lines() {
        let line = strip_line_comment(raw).trim();
        if line.is_empty() {
            continue;
        }

        if in_group {
            if line == ")" {
                in_group = false;
                continue;
            }
            if let Some(p) = import_path_from_spec(line) {
                imports.insert(p);
            }
            continue;
        }

        if line == "import (" || line.starts_with("import(") {
            in_group = true;
            continue;
        }

        if let Some(rest) = line.strip_prefix("import ") {
            let rest = rest.trim();
            // `import ( "a"; "b" )` on one line is not generated by gofmt; the
            // grouped form opens on its own line. A single `import "path"` (with
            // optional alias) reduces to the same spec parser.
            if rest == "(" {
                in_group = true;
                continue;
            }
            if let Some(p) = import_path_from_spec(rest) {
                imports.insert(p);
            }
        }
    }
}

/// Parses an import spec (`"path"`, `alias "path"`, `_ "path"`, `. "path"`) into
/// the bare import path, returning `None` for non-import lines.
fn import_path_from_spec(spec: &str) -> Option<String> {
    let start = spec.find('"')?;
    let rest = &spec[start + 1..];
    let end = rest.find('"')?;
    let path = &rest[..end];
    if path.is_empty() {
        return None;
    }
    Some(path.to_string())
}

/// Removes a trailing `// ...` line comment outside of string literals (good
/// enough for import blocks, which contain only quoted paths and aliases).
fn strip_line_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut i = 0;
    while i + 1 < bytes.len() {
        match bytes[i] {
            b'"' => in_str = !in_str,
            b'/' if !in_str && bytes[i + 1] == b'/' => return &line[..i],
            _ => {}
        }
        i += 1;
    }
    line
}
