//! `uast mapping` — UAST mapping helpers (port of `cmd/uast/mapping.go`).
//!
//! Analyzes `node-types.json`, classifies nodes, computes mapping coverage,
//! generates `.uastmap` DSL, or shows the raw tree-sitter JSON structure of an
//! input file. JSON output (`{node_count, categories, nodes[, coverage]}`) routes
//! through [`cf_textutil::write_json`] (DESIGN rule 1); `text` is human output.

use std::io::{self, Write};

use clap::{Arg, ArgAction, ArgMatches, Command};
use cf_textutil::{GoMap, GoValue};
use cf_uast_mapping::grammar_analysis::{
    apply_heuristic_classification, coverage_analysis, generate_mapping_dsl, parse_node_types,
};
use cf_uast_mapping::{NodeTypeInfo, Parser as MappingParser, Rule};

use crate::{COVERAGE_PERCENT, FORMAT_JSON};

/// Builds the `mapping` subcommand (mapping.go:38-69).
pub fn command() -> Command {
    Command::new("mapping")
        .about("UAST mapping helpers: grammar analysis, classification, coverage")
        .long_about(
            "Analyze node-types.json, classify nodes, compute mapping coverage, and show tree-sitter JSON structure.",
        )
        .arg(Arg::new("files").num_args(0..).index(1))
        .arg(long_opt("node-types", "", "Path to node-types.json (required for non-treesitter operations)"))
        .arg(long_opt("mapping", "", "Path to mapping DSL file (optional)"))
        .arg(long_opt("format", "text", "Output format: text or json"))
        .arg(long_flag("coverage", "Compute mapping coverage if mapping is provided"))
        .arg(long_flag("generate", "Generate .uastmap DSL from node-types.json"))
        .arg(long_flag("show-treesitter", "Show original tree-sitter JSON structure for input files"))
        .arg(long_opt("language", "", "Language for tree-sitter parsing (language name or grammar file path)"))
        .arg(long_opt("extensions", "", "Comma-separated list of file extensions for language declaration"))
}

/// Runs `mapping` (mapping.go `runMappingHelper`).
pub fn run(m: &ArgMatches) -> Result<(), String> {
    let node_types_path = m.get_one::<String>("node-types").map(String::as_str).unwrap_or("");
    let mapping_path = m.get_one::<String>("mapping").map(String::as_str).unwrap_or("");
    let format = m.get_one::<String>("format").map(String::as_str).unwrap_or("text");
    let coverage = m.get_flag("coverage");
    let generate = m.get_flag("generate");
    let show_treesitter = m.get_flag("show-treesitter");
    let language = m.get_one::<String>("language").map(String::as_str).unwrap_or("");
    let extensions = m.get_one::<String>("extensions").map(String::as_str).unwrap_or("");
    let files: Vec<String> =
        m.get_many::<String>("files").map(|v| v.cloned().collect()).unwrap_or_default();

    if show_treesitter {
        return show_tree_sitter(&files, language);
    }

    let nodes = load_node_types(node_types_path)?;

    if generate {
        let exts: Vec<String> = if extensions.is_empty() {
            Vec::new()
        } else {
            extensions.split(',').map(|s| s.trim().to_string()).collect()
        };
        let dsl = generate_mapping_dsl(&nodes, language, &exts);
        print!("{dsl}");
        return Ok(());
    }

    let rules = load_mapping_rules(mapping_path)?;

    if format == FORMAT_JSON {
        return output_json(&nodes, &rules, coverage);
    }
    output_text(&nodes, &rules, coverage)
}

/// Loads + classifies `node-types.json` (mapping.go `loadNodeTypes`). Errors
/// `--node-types is required for non-treesitter operations` when the path is
/// empty.
fn load_node_types(path: &str) -> Result<Vec<NodeTypeInfo>, String> {
    if path.is_empty() {
        return Err("--node-types is required for non-treesitter operations".to_string());
    }
    let data = std::fs::read(path).map_err(|e| format!("failed to read node-types.json: {e}"))?;
    let nodes = parse_node_types(&data).map_err(|e| format!("failed to parse node-types.json: {e}"))?;
    Ok(apply_heuristic_classification(nodes))
}

/// Loads mapping rules from a DSL file (mapping.go `loadMappingRules`). The Go
/// code parses the DSL only for validation and returns an empty rule list, which
/// is reproduced here.
fn load_mapping_rules(path: &str) -> Result<Vec<Rule>, String> {
    if path.is_empty() {
        return Ok(Vec::new());
    }
    let data = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to open mapping DSL: {e}"))?;
    MappingParser::new()
        .parse_mapping(&data)
        .map_err(|e| format!("failed to load mapping DSL: {e}"))?;
    Ok(Vec::new())
}

/// Outputs the JSON summary (mapping.go `outputMappingJSON`).
fn output_json(nodes: &[NodeTypeInfo], rules: &[Rule], coverage: bool) -> Result<(), String> {
    let mut entries: Vec<(String, GoValue)> = vec![
        ("node_count".to_string(), GoValue::Int(nodes.len() as i64)),
        ("categories".to_string(), categories_value(nodes)),
        ("nodes".to_string(), nodes_value(nodes)),
    ];

    if coverage && !rules.is_empty() {
        let cov = coverage_analysis(rules, nodes).map_err(|e| e.to_string())?;
        entries.push(("coverage".to_string(), GoValue::Float(cov)));
    }

    // Go builds `map[string]any{...}` (map-origin, byte-sorted keys).
    let value = GoValue::object(GoMap::from_map(entries));
    let mut out = io::stdout();
    cf_textutil::write_json(&mut out, &value, true).map_err(|e| e.to_string())
}

/// Outputs the text summary (mapping.go `outputMappingText`).
fn output_text(nodes: &[NodeTypeInfo], rules: &[Rule], coverage: bool) -> Result<(), String> {
    let out = io::stdout();
    let mut out = out.lock();
    let _ = writeln!(out, "Node types: {}", nodes.len());
    for (cat, count) in summarize_categories(nodes) {
        let _ = writeln!(out, "  {cat}: {count}");
    }
    if coverage && !rules.is_empty() {
        let cov = coverage_analysis(rules, nodes).map_err(|e| e.to_string())?;
        let _ = writeln!(out, "Coverage: {:.2}%", cov * COVERAGE_PERCENT);
    }
    Ok(())
}

/// Summarizes nodes by category (mapping.go `summarizeCategories`).
///
/// Go does `cats[fmt.Sprintf("%v", nodeInfo.Category)]++`, and `NodeCategory` is
/// a bare `int` with no `String()` method, so `%v` renders the integer value
/// (`0`/`1`/`2`). The category key is therefore the integer rendered as a string.
fn summarize_categories(nodes: &[NodeTypeInfo]) -> Vec<(String, i64)> {
    use std::collections::BTreeMap;
    let mut cats: BTreeMap<String, i64> = BTreeMap::new();
    for n in nodes {
        *cats.entry(category_key(n.category)).or_insert(0) += 1;
    }
    cats.into_iter().collect()
}

/// Renders a [`NodeCategory`] as Go's `fmt.Sprintf("%v", category)` would: the
/// underlying `int` value (`Leaf`=0, `Container`=1, `Composite/Operator`=2).
fn category_key(c: cf_uast_mapping::NodeCategory) -> String {
    (c as i64).to_string()
}

/// Builds the `categories` map value (map-origin, byte-sorted).
fn categories_value(nodes: &[NodeTypeInfo]) -> GoValue {
    GoValue::object(GoMap::from_map(
        summarize_categories(nodes)
            .into_iter()
            .map(|(k, v)| (k, GoValue::Int(v)))
            .collect::<Vec<_>>(),
    ))
}

/// Builds the `nodes` array value: one struct-origin object per [`NodeTypeInfo`]
/// (mapping_types.go JSON tags: `name`, `children` omitempty, `fields` omitempty,
/// `category` as the integer iota value, `is_named`). Field declaration order is
/// preserved (struct-origin); `children`/`fields` are omitted when empty.
fn nodes_value(nodes: &[NodeTypeInfo]) -> GoValue {
    GoValue::Array(
        nodes
            .iter()
            .map(|n| {
                let mut entries = GoMap::new_struct();
                entries.push("name", GoValue::Str(n.name.clone()));
                if !n.children.is_empty() {
                    let kids = n
                        .children
                        .iter()
                        .map(|c| {
                            let mut ce = GoMap::new_struct();
                            ce.push("type", GoValue::Str(c.r#type.clone()));
                            ce.push("named", GoValue::Bool(c.named));
                            GoValue::Object(ce)
                        })
                        .collect();
                    entries.push("children", GoValue::Array(kids));
                }
                if !n.fields.is_empty() {
                    // fields is a map[string]FieldInfo (map-origin, byte-sorted).
                    let field_entries: Vec<(String, GoValue)> = n
                        .fields
                        .iter()
                        .map(|(k, fi)| {
                            let mut fe = GoMap::new_struct();
                            fe.push("name", GoValue::Str(fi.name.clone()));
                            fe.push(
                                "types",
                                GoValue::Array(
                                    fi.types.iter().cloned().map(GoValue::Str).collect(),
                                ),
                            );
                            fe.push("required", GoValue::Bool(fi.required));
                            fe.push("multiple", GoValue::Bool(fi.multiple));
                            (k.clone(), GoValue::Object(fe))
                        })
                        .collect();
                    entries.push("fields", GoValue::object(GoMap::from_map(field_entries)));
                }
                entries.push("category", GoValue::Int(n.category as i64));
                entries.push("is_named", GoValue::Bool(n.is_named));
                GoValue::Object(entries)
            })
            .collect(),
    )
}

/// Shows the raw tree-sitter JSON for input files (mapping.go
/// `showTreeSitterJSON`). The grammar wiring lives in `cf-uast` (DESIGN §5); this
/// reproduces the control flow and sentinel errors. Until grammars are vendored
/// the parse step is unavailable, so an unset/unknown language yields the same
/// `unsupported language` sentinel.
fn show_tree_sitter(files: &[String], language: &str) -> Result<(), String> {
    if files.is_empty() {
        return Err("no input files provided".to_string());
    }
    for filename in files {
        let _ = std::fs::read(filename)
            .map_err(|e| format!("failed to process {filename}: failed to read file: {e}"))?;
        // Language resolution mirrors mapping.go: an unknown/empty language is an
        // `unsupported language` error (the grammar dispatch is owned by cf-uast
        // and not yet wired; see cf-uast todos / DESIGN §5).
        if language.is_empty() {
            return Err(format!(
                "failed to process {filename}: tree-sitter parsing requires a language to be set"
            ));
        }
        return Err(format!("failed to process {filename}: unsupported language: {language}"));
    }
    Ok(())
}

fn long_opt(name: &'static str, default: &'static str, help: &'static str) -> Arg {
    Arg::new(name).long(name).help(help).default_value(default).action(ArgAction::Set)
}

fn long_flag(name: &'static str, help: &'static str) -> Arg {
    Arg::new(name).long(name).help(help).action(ArgAction::SetTrue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_types_required_error() {
        let err = load_node_types("").unwrap_err();
        assert_eq!(err, "--node-types is required for non-treesitter operations");
    }

    #[test]
    fn show_treesitter_no_files_error() {
        let err = show_tree_sitter(&[], "go").unwrap_err();
        assert_eq!(err, "no input files provided");
    }

    #[test]
    fn empty_mapping_path_is_empty_rules() {
        assert!(load_mapping_rules("").unwrap().is_empty());
    }
}
