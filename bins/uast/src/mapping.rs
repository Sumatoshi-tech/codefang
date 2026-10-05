//! `uast mapping` — UAST mapping helpers (port of `cmd/uast/mapping.go`).
//!
//! Analyzes and classifies `node-types.json`, generates `.uastmap` DSL from
//! it, or shows the raw tree-sitter JSON structure of input files. JSON output
//! (`{node_count, categories, nodes}`, the tree-sitter dump) routes through
//! [`cf_textutil::write_json`] (DESIGN rule 1); `text` is human output.
//!
//! The Go-era `--mapping` / `--coverage` pair is gone: the reference parsed
//! the mapping file and discarded the rules, so the coverage was never
//! computed (`specs/bugs/BUG-cli-inert-surface.md`).

use std::io::{self, Write};

use cf_textutil::{GoMap, GoValue};
use cf_uast_mapping::grammar_analysis::{
    apply_heuristic_classification, generate_mapping_dsl, parse_node_types,
};
use cf_uast_mapping::NodeTypeInfo;
use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::FORMAT_JSON;

/// Builds the `mapping` subcommand (mapping.go:38-69).
pub fn command() -> Command {
    Command::new("mapping")
        .about("UAST mapping helpers: grammar analysis, classification, DSL generation")
        .long_about(
            "Analyze and classify node-types.json, generate .uastmap DSL from it, and show the tree-sitter JSON structure of input files.",
        )
        // The positional files are the inputs of `--show-treesitter`.
        .arg(
            Arg::new("files")
                .num_args(0..)
                .index(1)
                .help("Input source files for --show-treesitter"),
        )
        .arg(long_opt("node-types", "", "Path to node-types.json (required for non-treesitter operations)"))
        .arg(long_opt("format", "text", "Output format of the node-types summary: text or json"))
        .arg(long_flag("generate", "Generate .uastmap DSL from node-types.json"))
        .arg(long_flag("show-treesitter", "Show original tree-sitter JSON structure for input files"))
        .arg(long_opt("language", "", "Tree-sitter language name (e.g. go) for --show-treesitter and the --generate header"))
        .arg(long_opt("extensions", "", "Comma-separated file extensions for the --generate language header"))
}

/// Runs `mapping` (mapping.go `runMappingHelper`).
pub fn run(m: &ArgMatches) -> Result<(), String> {
    let node_types_path = m
        .get_one::<String>("node-types")
        .map(String::as_str)
        .unwrap_or("");
    let format = m
        .get_one::<String>("format")
        .map(String::as_str)
        .unwrap_or("text");
    let generate = m.get_flag("generate");
    let show_treesitter = m.get_flag("show-treesitter");
    let language = m
        .get_one::<String>("language")
        .map(String::as_str)
        .unwrap_or("");
    let extensions = m
        .get_one::<String>("extensions")
        .map(String::as_str)
        .unwrap_or("");
    let files: Vec<String> = m
        .get_many::<String>("files")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();

    if show_treesitter {
        return show_tree_sitter(&files, language);
    }

    let nodes = load_node_types(node_types_path)?;

    if generate {
        let exts: Vec<String> = if extensions.is_empty() {
            Vec::new()
        } else {
            extensions
                .split(',')
                .map(|s| s.trim().to_string())
                .collect()
        };
        let dsl = generate_mapping_dsl(&nodes, language, &exts);
        print!("{dsl}");
        return Ok(());
    }

    if format == FORMAT_JSON {
        return output_json(&nodes);
    }
    output_text(&nodes)
}

/// Loads + classifies `node-types.json` (mapping.go `loadNodeTypes`). Errors
/// `--node-types is required for non-treesitter operations` when the path is
/// empty.
fn load_node_types(path: &str) -> Result<Vec<NodeTypeInfo>, String> {
    if path.is_empty() {
        return Err("--node-types is required for non-treesitter operations".to_string());
    }
    let data = std::fs::read(path).map_err(|e| format!("failed to read node-types.json: {e}"))?;
    let nodes =
        parse_node_types(&data).map_err(|e| format!("failed to parse node-types.json: {e}"))?;
    Ok(apply_heuristic_classification(nodes))
}

/// Outputs the JSON summary (mapping.go `outputMappingJSON`).
fn output_json(nodes: &[NodeTypeInfo]) -> Result<(), String> {
    let entries: Vec<(String, GoValue)> = vec![
        ("node_count".to_string(), GoValue::Int(nodes.len() as i64)),
        ("categories".to_string(), categories_value(nodes)),
        ("nodes".to_string(), nodes_value(nodes)),
    ];

    // Go builds `map[string]any{...}` (map-origin, byte-sorted keys).
    let value = GoValue::object(GoMap::from_map(entries));
    let mut out = io::stdout();
    cf_textutil::write_json(&mut out, &value, true).map_err(|e| e.to_string())
}

/// Outputs the text summary (mapping.go `outputMappingText`).
fn output_text(nodes: &[NodeTypeInfo]) -> Result<(), String> {
    let out = io::stdout();
    let mut out = out.lock();
    let _ = writeln!(out, "Node types: {}", nodes.len());
    for (cat, count) in summarize_categories(nodes) {
        let _ = writeln!(out, "  {cat}: {count}");
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
/// `showTreeSitterJSON`): for each file, a `=== Tree-sitter JSON for <file>
/// (language: <lang>) ===` header, the pretty-printed tree, and a blank line.
/// The first failing file aborts with `failed to process <file>: <cause>`.
fn show_tree_sitter(files: &[String], language: &str) -> Result<(), String> {
    if files.is_empty() {
        return Err("no input files provided".to_string());
    }
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for filename in files {
        write_tree_sitter_json(&mut out, filename, language)
            .map_err(|e| format!("failed to process {filename}: {e}"))?;
    }
    Ok(())
}

/// Parses `filename` with the vendored `language` grammar and writes its
/// header + tree JSON (mapping.go `processFileForTreeSitterJSON`).
fn write_tree_sitter_json(
    out: &mut dyn Write,
    filename: &str,
    language: &str,
) -> Result<(), String> {
    let content = std::fs::read(filename).map_err(|e| format!("failed to read file: {e}"))?;
    if language.is_empty() {
        return Err(
            "tree-sitter parsing requires a language to be set; use --language <name>".to_string(),
        );
    }
    let grammar = cf_uast::languages::get_language(language)
        .ok_or_else(|| format!("unsupported language: {language}"))?;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&grammar)
        .map_err(|e| format!("failed to load the {language} grammar: {e}"))?;
    let tree = parser
        .parse(&content, None)
        .ok_or_else(|| "failed to parse with tree-sitter".to_string())?;
    let value = tree_sitter_node_value(tree.root_node(), &content);
    writeln!(
        out,
        "=== Tree-sitter JSON for {filename} (language: {language}) ==="
    )
    .map_err(|e| e.to_string())?;
    cf_textutil::write_json(&mut *out, &value, true).map_err(|e| e.to_string())?;
    writeln!(out).map_err(|e| e.to_string())
}

/// One tree-sitter node as JSON (mapping.go `convertTreeSitterNodeToJSON`):
/// `type`, `named`, `start_pos` / `end_pos` (`row`, `column`), `start_byte` /
/// `end_byte`, `text` when non-empty, and `children` (the NAMED children) when
/// there are any. Keys are map-origin (byte-sorted), like the Go `map`.
fn tree_sitter_node_value(node: tree_sitter::Node<'_>, source: &[u8]) -> GoValue {
    let position = |p: tree_sitter::Point| {
        GoValue::object(GoMap::from_map(vec![
            ("row".to_string(), usize_value(p.row)),
            ("column".to_string(), usize_value(p.column)),
        ]))
    };
    let mut entries = vec![
        ("type".to_string(), GoValue::Str(node.kind().to_string())),
        ("named".to_string(), GoValue::Bool(node.is_named())),
        ("start_pos".to_string(), position(node.start_position())),
        ("end_pos".to_string(), position(node.end_position())),
        ("start_byte".to_string(), usize_value(node.start_byte())),
        ("end_byte".to_string(), usize_value(node.end_byte())),
    ];
    let text = source
        .get(node.start_byte()..node.end_byte())
        .map(String::from_utf8_lossy)
        .unwrap_or_default();
    if !text.is_empty() {
        entries.push(("text".to_string(), GoValue::Str(text.into_owned())));
    }
    let mut cursor = node.walk();
    let children: Vec<GoValue> = node
        .named_children(&mut cursor)
        .map(|child| tree_sitter_node_value(child, source))
        .collect();
    if !children.is_empty() {
        entries.push(("children".to_string(), GoValue::Array(children)));
    }
    GoValue::object(GoMap::from_map(entries))
}

/// A tree-sitter offset as a JSON integer.
fn usize_value(v: usize) -> GoValue {
    GoValue::Int(i64::try_from(v).unwrap_or(i64::MAX))
}

fn long_opt(name: &'static str, default: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .help(help)
        .default_value(default)
        .action(ArgAction::Set)
}

fn long_flag(name: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .help(help)
        .action(ArgAction::SetTrue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_types_required_error() {
        let err = load_node_types("").unwrap_err();
        assert_eq!(
            err,
            "--node-types is required for non-treesitter operations"
        );
    }

    #[test]
    fn show_treesitter_no_files_error() {
        let err = show_tree_sitter(&[], "go").unwrap_err();
        assert_eq!(err, "no input files provided");
    }
}
