//! `uast analyze [files...]` — UAST structural analysis (port of
//! `cmd/uast/analyze.go`).
//!
//! Walks each parsed tree once and reports shape metrics. The JSON form is an
//! array of `analysisResult` **structs**, so each object's keys emit in
//! declaration order (`file`, `total_nodes`, `leaf_nodes`, `leaf_ratio`,
//! `max_depth`, `avg_depth`, `max_children`, `avg_branching`, `type_diversity`,
//! `types`, `roles`, `role_coverage`, `pos_coverage`, `synthetic_nodes`) — built
//! as struct-origin objects. The nested `types`/`roles` are `map[string]int`, so
//! they are map-origin (byte-sorted). `text`/`html` are human-format.

use std::fs::File;
use std::io::{self, Write};

use cf_textutil::{GoMap, GoValue};
use cf_uast::{Node, Parser};
use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::{COVERAGE_PERCENT, FORMAT_JSON};

/// The node type marking a synthetic node (`node.UASTSynthetic`).
const UAST_SYNTHETIC: &str = "Synthetic";

/// Per-file structural analysis result (analyze.go `analysisResult`). Field
/// order is load-bearing for JSON struct-origin serialization.
#[derive(Default, Clone)]
struct AnalysisResult {
    file: String,
    total_nodes: i64,
    leaf_nodes: i64,
    leaf_ratio: f64,
    max_depth: i64,
    avg_depth: f64,
    max_children: i64,
    avg_branching: f64,
    type_diversity: i64,
    types: Vec<(String, i64)>,
    roles: Vec<(String, i64)>,
    role_coverage: f64,
    pos_coverage: f64,
    synthetic_nodes: i64,
}

/// Builds the `analyze` subcommand (analyze.go:57-79).
pub fn command() -> Command {
    Command::new("analyze")
        .about("Analyze UAST tree structure and composition")
        .override_usage("uast analyze [files...] [flags]")
        .arg(Arg::new("files").num_args(0..).index(1))
        .arg(
            Arg::new("output")
                .long("output")
                .short('o')
                .help("output file (default: stdout)")
                .default_value("")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("format")
                .long("format")
                .short('f')
                .help("output format (text, json, html)")
                .default_value("text")
                .action(ArgAction::Set),
        )
}

/// Runs `analyze` (analyze.go `runAnalyze`).
pub fn run(m: &ArgMatches) -> Result<(), String> {
    let files: Vec<String> = m
        .get_many::<String>("files")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();
    let output = m
        .get_one::<String>("output")
        .map(String::as_str)
        .unwrap_or("");
    let format = m
        .get_one::<String>("format")
        .map(String::as_str)
        .unwrap_or("text");

    run_analyze(files, output, format)
}

/// The body of `analyze`, separated so unit tests can call it directly
/// (mirrors the Go `runAnalyze` tests).
fn run_analyze(files: Vec<String>, output: &str, format: &str) -> Result<(), String> {
    if files.is_empty() {
        return Err("no files specified for analysis".to_string());
    }

    // filepath.Clean each (analyze.go).
    let files: Vec<String> = files.iter().map(|f| clean_path(f)).collect();

    let parser = Parser::new();

    let mut supported: Vec<String> = Vec::with_capacity(files.len());
    for file in &files {
        if !parser.is_supported(file) {
            eprintln!("Warning: Skipping unsupported file {file}");
            continue;
        }
        supported.push(file.clone());
    }

    if supported.is_empty() {
        return output_analysis(&[], output, format);
    }

    let mut results = Vec::with_capacity(supported.len());
    for file in &supported {
        let code = std::fs::read(file).map_err(|e| format!("failed to read file {file}: {e}"))?;
        let node = parser
            .parse(file, &code)
            .map_err(|e| format!("parse error in {file}: {e}"))?;
        results.push(analyze_node(&node, file));
    }

    output_analysis(&results, output, format)
}

/// Produces structural analysis for one tree (analyze.go `analyzeNode`).
fn analyze_node(root: &Node, filename: &str) -> AnalysisResult {
    let mut total_nodes = 0i64;
    let mut leaf_nodes = 0i64;
    let mut max_depth = 0i64;
    let mut total_depth = 0i64;
    let mut nodes_with_roles = 0i64;
    let mut nodes_with_pos = 0i64;
    let mut synthetic_nodes = 0i64;
    let mut max_children = 0i64;
    let mut total_children = 0i64;
    let mut inner_nodes = 0i64;

    use std::collections::BTreeMap;
    let mut types: BTreeMap<String, i64> = BTreeMap::new();
    let mut roles: BTreeMap<String, i64> = BTreeMap::new();

    // Iterative DFS, children pushed in reverse (analyze.go `collectTreeStats`).
    let mut stack: Vec<(&Node, i64)> = vec![(root, 0)];
    while let Some((node, depth)) = stack.pop() {
        total_nodes += 1;
        total_depth += depth;
        if depth > max_depth {
            max_depth = depth;
        }
        if !node.node_type.is_empty() {
            *types.entry(node.node_type.clone()).or_insert(0) += 1;
        }
        if node.node_type == UAST_SYNTHETIC {
            synthetic_nodes += 1;
        }
        for r in &node.roles {
            *roles.entry(r.clone()).or_insert(0) += 1;
        }
        if !node.roles.is_empty() {
            nodes_with_roles += 1;
        }
        if node.pos.is_some() {
            nodes_with_pos += 1;
        }
        let child_count = node.children.len() as i64;
        if child_count == 0 {
            leaf_nodes += 1;
        } else {
            inner_nodes += 1;
            total_children += child_count;
            if child_count > max_children {
                max_children = child_count;
            }
        }
        for child in node.children.iter().rev() {
            stack.push((child, depth + 1));
        }
    }

    let total_f = total_nodes as f64;
    AnalysisResult {
        file: filename.to_string(),
        total_nodes,
        leaf_nodes,
        leaf_ratio: safe_div(leaf_nodes as f64, total_f),
        max_depth,
        avg_depth: safe_div(total_depth as f64, total_f),
        max_children,
        avg_branching: safe_div(total_children as f64, inner_nodes as f64),
        type_diversity: types.len() as i64,
        types: types.into_iter().collect(),
        roles: roles.into_iter().collect(),
        role_coverage: safe_div(nodes_with_roles as f64, total_f),
        pos_coverage: safe_div(nodes_with_pos as f64, total_f),
        synthetic_nodes,
    }
}

/// `numerator/denominator`, or 0 when the denominator is 0 (analyze.go `safeDiv`).
fn safe_div(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

/// Lexically cleans a path (Go `filepath.Clean`), reusing the same lexical rules
/// `cf-iosafety` implements. A minimal local copy is used to avoid exporting an
/// internal helper; for the simple paths `analyze` sees it matches Go.
fn clean_path(p: &str) -> String {
    // Delegate to a normalization that matches filepath.Clean for the common
    // cases (collapse `//`, `.`, and `..`). For analyze's inputs this is exact.
    let mut out: Vec<&str> = Vec::new();
    let rooted = p.starts_with('/');
    for part in p.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if let Some(last) = out.last() {
                    if *last != ".." {
                        out.pop();
                        continue;
                    }
                }
                if !rooted {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    let joined = out.join("/");
    if rooted {
        format!("/{joined}")
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

/// Serializes results (analyze.go `outputAnalysis`).
fn output_analysis(results: &[AnalysisResult], output: &str, format: &str) -> Result<(), String> {
    let mut writer: Box<dyn Write> = if output.is_empty() {
        Box::new(io::stdout())
    } else {
        Box::new(File::create(output).map_err(|e| format!("failed to create output file: {e}"))?)
    };

    match format {
        FORMAT_JSON => {
            let value = results_to_value(results);
            cf_textutil::write_json(&mut writer, &value, true).map_err(|e| e.to_string())
        }
        "text" => {
            output_text(results, &mut writer);
            Ok(())
        }
        "html" => {
            output_html(results, &mut writer);
            Ok(())
        }
        other => Err(format!("unsupported format: {other}")),
    }
}

/// Builds the `[]analysisResult` JSON value (struct-origin objects).
fn results_to_value(results: &[AnalysisResult]) -> GoValue {
    GoValue::Array(results.iter().map(result_to_value).collect())
}

/// Builds one struct-origin `analysisResult` object (declaration order).
fn result_to_value(r: &AnalysisResult) -> GoValue {
    let types = GoValue::object(GoMap::from_map(
        r.types
            .iter()
            .map(|(k, v)| (k.clone(), GoValue::Int(*v)))
            .collect::<Vec<_>>(),
    ));
    let roles = GoValue::object(GoMap::from_map(
        r.roles
            .iter()
            .map(|(k, v)| (k.clone(), GoValue::Int(*v)))
            .collect::<Vec<_>>(),
    ));
    let mut m = GoMap::new_struct();
    m.push("file", GoValue::Str(r.file.clone()));
    m.push("total_nodes", GoValue::Int(r.total_nodes));
    m.push("leaf_nodes", GoValue::Int(r.leaf_nodes));
    m.push("leaf_ratio", GoValue::Float(r.leaf_ratio));
    m.push("max_depth", GoValue::Int(r.max_depth));
    m.push("avg_depth", GoValue::Float(r.avg_depth));
    m.push("max_children", GoValue::Int(r.max_children));
    m.push("avg_branching", GoValue::Float(r.avg_branching));
    m.push("type_diversity", GoValue::Int(r.type_diversity));
    m.push("types", types);
    m.push("roles", roles);
    m.push("role_coverage", GoValue::Float(r.role_coverage));
    m.push("pos_coverage", GoValue::Float(r.pos_coverage));
    m.push("synthetic_nodes", GoValue::Int(r.synthetic_nodes));
    GoValue::Object(m)
}

/// Human text rendering (analyze.go `outputAnalysisText`). Non-binding.
fn output_text(results: &[AnalysisResult], writer: &mut dyn Write) {
    for r in results {
        let _ = writeln!(writer, "File: {}", r.file);
        let _ = writeln!(writer, "  Tree shape:");
        let _ = writeln!(writer, "    Total nodes:    {}", r.total_nodes);
        let _ = writeln!(
            writer,
            "    Leaf nodes:     {} ({:.0}%)",
            r.leaf_nodes,
            r.leaf_ratio * COVERAGE_PERCENT
        );
        let _ = writeln!(writer, "    Max depth:      {}", r.max_depth);
        let _ = writeln!(writer, "    Avg depth:      {:.1}", r.avg_depth);
        let _ = writeln!(writer, "    Max children:   {}", r.max_children);
        let _ = writeln!(writer, "    Avg branching:  {:.1}", r.avg_branching);
        let _ = writeln!(writer, "  Coverage:");
        let _ = writeln!(
            writer,
            "    Role coverage:  {:.0}%",
            r.role_coverage * COVERAGE_PERCENT
        );
        let _ = writeln!(
            writer,
            "    Pos coverage:   {:.0}%",
            r.pos_coverage * COVERAGE_PERCENT
        );
        let _ = writeln!(writer, "    Synthetic:      {}", r.synthetic_nodes);
        let _ = writeln!(writer, "    Type diversity: {}", r.type_diversity);
        if !r.types.is_empty() {
            let _ = writeln!(writer, "  Node types:");
            print_sorted_by_value(writer, &r.types);
        }
        if !r.roles.is_empty() {
            let _ = writeln!(writer, "  Roles:");
            print_sorted_by_value(writer, &r.roles);
        }
        let _ = writeln!(writer);
    }
}

/// Prints `(key, value)` pairs sorted by descending value (analyze.go
/// `printSortedMap`).
fn print_sorted_by_value(writer: &mut dyn Write, m: &[(String, i64)]) {
    let mut sorted: Vec<&(String, i64)> = m.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    for (k, v) in sorted {
        let _ = writeln!(writer, "    {k:<20} {v}");
    }
}

/// HTML rendering (analyze.go `generateHTMLReport`). Non-binding (DESIGN §2.7);
/// emits the same table skeleton best-effort.
fn output_html(results: &[AnalysisResult], writer: &mut dyn Write) {
    let _ = writeln!(
        writer,
        "<!DOCTYPE html>\n<html>\n<head>\n<title>UAST Structure Report</title>"
    );
    let _ = writeln!(
        writer,
        "</head>\n<body>\n<h1>UAST Structure Report</h1>\n<table>"
    );
    let _ = writeln!(
        writer,
        "<tr><th>File</th><th>Nodes</th><th>Depth</th><th>Branching</th><th>Types</th><th>Role%</th><th>Pos%</th></tr>"
    );
    for r in results {
        let _ = writeln!(
            writer,
            "<tr>\n<td>{}</td>\n<td>{}</td>\n<td>{} (avg {:.1})</td>\n<td>max {} (avg {:.1})</td>\n<td>{}</td>\n<td>{:.0}%</td>\n<td>{:.0}%</td>\n</tr>",
            r.file,
            r.total_nodes,
            r.max_depth,
            r.avg_depth,
            r.max_children,
            r.avg_branching,
            r.type_diversity,
            r.role_coverage * COVERAGE_PERCENT,
            r.pos_coverage * COVERAGE_PERCENT,
        );
    }
    let _ = writeln!(writer, "</table>\n</body>\n</html>");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(t: &str) -> Node {
        Node::with_token(t, "")
    }

    fn n_roles(t: &str, roles: &[&str]) -> Node {
        let mut node = n(t);
        node.roles = roles.iter().map(|s| s.to_string()).collect();
        node
    }

    // Ported from analyze_test.go TestAnalyzeNode_BasicStructure.
    #[test]
    fn analyze_node_basic_structure() {
        let mut root = n("File");
        let pkg = n_roles("Package", &["Module"]);
        let mut fnn = n_roles("Function", &["Function"]);
        let mut ifn = n_roles("If", &["Branch"]);
        let call = n_roles("Call", &["Call"]);
        let mut method = n_roles("Method", &["Function"]);
        let loopn = n_roles("Loop", &["Loop"]);

        ifn.add_child(call);
        fnn.add_child(ifn);
        method.add_child(loopn);
        root.add_child(pkg);
        root.add_child(fnn);
        root.add_child(method);

        let r = analyze_node(&root, "test.go");
        assert_eq!(r.total_nodes, 7);
        assert_eq!(r.leaf_nodes, 3);
        assert_eq!(r.max_depth, 3);
        assert_eq!(r.max_children, 3);
        assert_eq!(r.file, "test.go");
        assert_eq!(types_get(&r, "Function"), 1);
        assert_eq!(types_get(&r, "Method"), 1);
        assert_eq!(types_get(&r, "If"), 1);
        assert_eq!(roles_get(&r, "Function"), 2);
        assert!(r.role_coverage > 0.85 && r.role_coverage < 0.87);
    }

    fn types_get(r: &AnalysisResult, k: &str) -> i64 {
        r.types
            .iter()
            .find(|(t, _)| t == k)
            .map(|(_, v)| *v)
            .unwrap_or(0)
    }
    fn roles_get(r: &AnalysisResult, k: &str) -> i64 {
        r.roles
            .iter()
            .find(|(t, _)| t == k)
            .map(|(_, v)| *v)
            .unwrap_or(0)
    }

    // Ported from analyze_test.go TestAnalyzeNode_EmptyTree.
    #[test]
    fn analyze_node_empty_tree() {
        let root = n("File");
        let r = analyze_node(&root, "empty.go");
        assert_eq!(r.total_nodes, 1);
        assert_eq!(r.leaf_nodes, 1);
        assert_eq!(r.max_depth, 0);
    }

    // Ported from analyze_test.go TestRunAnalyze_NoFiles.
    #[test]
    fn run_analyze_no_files_errors() {
        assert!(run_analyze(vec![], "", "text").is_err());
    }

    // Ported from analyze_test.go TestRunAnalyze_UnsupportedFormat (no
    // supported files → outputAnalysis(nil) path still validates format).
    #[test]
    fn unsupported_format_errors() {
        let err = output_analysis(&[], "", "xml").unwrap_err();
        assert_eq!(err, "unsupported format: xml");
    }

    #[test]
    fn json_struct_field_order_is_declaration_order() {
        let r = analyze_node(&n("File"), "a.go");
        let v = results_to_value(&[r]);
        let s = String::from_utf8(cf_textutil::marshal_json(&v, false).unwrap()).unwrap();
        // First keys must be in declaration order (struct-origin), not sorted.
        assert!(
            s.starts_with("[{\"file\":\"a.go\",\"total_nodes\":1,\"leaf_nodes\":1,"),
            "got {s}"
        );
        // A single File leaf: ratios are integer-valued floats -> "1"/"0".
        assert!(s.contains("\"leaf_ratio\":1,"), "got {s}");
        assert!(s.contains("\"avg_depth\":0,"), "got {s}");
    }

    #[test]
    fn safe_div_zero_denominator() {
        assert_eq!(safe_div(5.0, 0.0), 0.0);
        assert_eq!(safe_div(6.0, 2.0), 3.0);
    }

    #[test]
    fn clean_path_collapses() {
        assert_eq!(clean_path("a/b/../c"), "a/c");
        assert_eq!(clean_path("./x.go"), "x.go");
        assert_eq!(clean_path("/a/./b"), "/a/b");
    }
}
