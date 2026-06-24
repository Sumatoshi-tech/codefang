//! `uast query [query] [files...]` — query UAST with the DSL (port of
//! `cmd/uast/query.go`).
//!
//! Runs a DSL query against parsed UAST (or a UAST JSON tree loaded from a file
//! / stdin) and emits the matching nodes wrapped in `{"results":[...]}`. The
//! wrapper is map-origin (single key). `nodesToMap` special-cases an all-Literal
//! result set to a list of the matched tokens; otherwise it emits each node's
//! `ToMap`.
//!
//! Formats (`outputResults`): `json` (pretty), `compact`, `count` (prints the
//! number of results). `interactive` opens a REPL (non-binding human output).
//! Input UAST JSON decoding uses `serde_json` (DESIGN §2 permits input decode).

use std::fs::File;
use std::io::{self, Read, Write};

use cf_textutil::{GoMap, GoValue};
use cf_uast::{Node, Parser};
use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::govalue_bridge::node_to_value;
use crate::{FORMAT_COMPACT, FORMAT_JSON};

/// Builds the `query` subcommand (query.go:33-61).
pub fn command() -> Command {
    Command::new("query")
        .about("Query UAST with DSL expressions")
        .override_usage("uast query [query] [files...] [flags]")
        .arg(Arg::new("args").num_args(0..).index(1))
        .arg(opt(
            "input",
            'i',
            "",
            "input file (UAST JSON or source code)",
        ))
        .arg(opt("output", 'o', "", "output file (default: stdout)"))
        .arg(opt(
            "format",
            'f',
            "json",
            "output format (json, compact, count)",
        ))
        .arg(flag("interactive", Some('t'), "interactive query mode"))
}

/// Runs `query` (query.go `RunE` + `runQuery`).
pub fn run(m: &ArgMatches) -> Result<(), String> {
    let args: Vec<String> = m
        .get_many::<String>("args")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();
    let input = m
        .get_one::<String>("input")
        .map(String::as_str)
        .unwrap_or("");
    let output = m
        .get_one::<String>("output")
        .map(String::as_str)
        .unwrap_or("");
    let format = m
        .get_one::<String>("format")
        .map(String::as_str)
        .unwrap_or(FORMAT_JSON);
    let interactive = m.get_flag("interactive");

    if interactive {
        return run_interactive(input);
    }

    // Go: `if len(args) == 0 { return ErrQueryExprRequired }`.
    if args.is_empty() {
        return Err("query expression required".to_string());
    }
    let query = &args[0];
    let files = &args[1..];

    if files.is_empty() && input.is_empty() {
        return query_stdin(query, output, format);
    }

    for file in files {
        query_file(file, query, output, format)
            .map_err(|e| format!("failed to query {file}: {e}"))?;
    }
    Ok(())
}

/// Queries a UAST tree decoded from stdin (query.go `queryStdin`).
fn query_stdin(query: &str, output: &str, format: &str) -> Result<(), String> {
    let mut buf = Vec::new();
    io::stdin()
        .read_to_end(&mut buf)
        .map_err(|e| format!("failed to decode UAST from stdin: {e}"))?;
    let node = decode_uast(&buf).map_err(|e| format!("failed to decode UAST from stdin: {e}"))?;
    let results = node
        .find_dsl(query)
        .map_err(|e| format!("query error: {e}"))?;
    output_results(&results, output, format)
}

/// Queries a single file (query.go `queryFile` / `parseFileForQuery`).
fn query_file(file: &str, query: &str, output: &str, format: &str) -> Result<(), String> {
    let node = load_query_node(file)?;
    let results = node
        .find_dsl(query)
        .map_err(|e| format!("query error: {e}"))?;
    output_results(&results, output, format)
}

/// Loads a UAST node from a file, auto-detecting JSON vs source (query.go
/// `parseFileForQuery`).
fn load_query_node(file: &str) -> Result<Node, String> {
    if is_json_file(file) {
        return load_uast_from_json(file).map_err(|e| format!("failed to query {file}: {e}"));
    }

    let parser = Parser::new();
    if !parser.is_supported(file) {
        return load_uast_from_json(file).map_err(|e| format!("failed to query {file}: {e}"));
    }

    let (code, resolved) =
        cf_iosafety::read_file(file).map_err(|e| format!("failed to read file {file}: {e}"))?;
    let resolved = resolved.to_string_lossy().into_owned();
    parser
        .parse(&resolved, &code)
        .map_err(|e| format!("parse error in {file}: {e}"))
}

/// Reads a UAST JSON file into a [`Node`] (query.go `loadUASTFromJSON`).
fn load_uast_from_json(file: &str) -> Result<Node, String> {
    let bytes = std::fs::read(file).map_err(|e| format!("failed to open file {file}: {e}"))?;
    decode_uast(&bytes).map_err(|e| format!("failed to decode UAST from {file}: {e}"))
}

/// The interactive REPL (query.go `runInteractiveQuery`). Human/non-binding.
fn run_interactive(input: &str) -> Result<(), String> {
    let node = if input.is_empty() {
        let mut buf = Vec::new();
        io::stdin()
            .read_to_end(&mut buf)
            .map_err(|e| format!("failed to read stdin: {e}"))?;
        Parser::new()
            .parse("stdin.go", &buf)
            .map_err(|e| format!("parse error: {e}"))?
    } else {
        load_query_node(input)?
    };

    let out = io::stdout();
    let mut out = out.lock();
    let _ = writeln!(out, "Interactive UAST Query Mode");
    let _ = writeln!(out, "Type 'help' for DSL syntax, 'quit' to exit");
    let _ = writeln!(out);

    let stdin = io::stdin();
    loop {
        let _ = write!(out, "uast> ");
        let _ = out.flush();
        let mut line = String::new();
        if stdin.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            break;
        }
        let q = line.trim();
        if q.is_empty() {
            continue;
        }
        if q == "quit" || q == "exit" {
            break;
        }
        if q == "help" {
            print_dsl_help(&mut out);
            continue;
        }
        match node.find_dsl(q) {
            Err(e) => {
                let _ = writeln!(out, "Error: {e}");
            }
            Ok(results) if results.is_empty() => {
                let _ = writeln!(out, "No results found");
            }
            Ok(results) => {
                let _ = writeln!(out, "Found {} results:", results.len());
                for (idx, n) in results.iter().enumerate() {
                    let _ = writeln!(
                        out,
                        "[{}] {}: {}",
                        idx + 1,
                        cf_iosafety::sanitize_for_terminal(&n.node_type),
                        cf_iosafety::sanitize_for_terminal(&n.token),
                    );
                }
            }
        }
        let _ = writeln!(out);
    }
    Ok(())
}

/// Serializes results (query.go `outputResults`).
fn output_results(results: &[Node], output: &str, format: &str) -> Result<(), String> {
    let mut writer: Box<dyn Write> = if output.is_empty() {
        Box::new(io::stdout())
    } else {
        Box::new(File::create(output).map_err(|e| format!("failed to create output file: {e}"))?)
    };

    let mapped = nodes_to_value(results);

    match format {
        FORMAT_JSON => {
            cf_textutil::write_json(&mut writer, &mapped, true).map_err(|e| e.to_string())
        }
        FORMAT_COMPACT => {
            cf_textutil::write_json(&mut writer, &mapped, false).map_err(|e| e.to_string())
        }
        "count" => {
            let count = result_count(&mapped);
            writeln!(writer, "{count}").map_err(|e| e.to_string())
        }
        other => Err(format!("unsupported format: {other}")),
    }
}

/// Builds the `{"results":[...]}` value (query.go `nodesToMap`).
///
/// Empty → `{"results":[]}`. If every node is a `Literal`, the results are the
/// tokens; otherwise each node's `ToMap`. The wrapper is map-origin (single key).
fn nodes_to_value(nodes: &[Node]) -> GoValue {
    let results: Vec<GoValue> = if nodes.is_empty() {
        Vec::new()
    } else if nodes.iter().all(|n| n.node_type == "Literal") {
        nodes
            .iter()
            .map(|n| GoValue::Str(n.token.clone()))
            .collect()
    } else {
        nodes.iter().map(node_to_value).collect()
    };
    GoValue::object(GoMap::from_map(vec![(
        "results".to_string(),
        GoValue::Array(results),
    )]))
}

/// Returns the number of entries in the `results` array of a built value.
fn result_count(value: &GoValue) -> usize {
    if let GoValue::Map(m) = value {
        if let Some(GoValue::Array(arr)) = m.get("results") {
            return arr.len();
        }
    }
    0
}

/// Decodes UAST JSON bytes into a [`Node`] (input decode; serde_json allowed).
fn decode_uast(bytes: &[u8]) -> Result<Node, String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    Ok(json_to_node(&v))
}

/// Converts a decoded UAST JSON value into a [`Node`], mirroring the field
/// names Go's `node.Node` `UnmarshalJSON` reads (`type`, `token`, `roles`,
/// `pos`, `props`, `children`).
fn json_to_node(v: &serde_json::Value) -> Node {
    let mut n = Node::default();
    let obj = match v.as_object() {
        Some(o) => o,
        None => return n,
    };
    if let Some(t) = obj.get("type").and_then(|x| x.as_str()) {
        n.node_type = t.to_string();
    }
    if let Some(tok) = obj.get("token").and_then(|x| x.as_str()) {
        n.token = tok.to_string();
    }
    if let Some(roles) = obj.get("roles").and_then(|x| x.as_array()) {
        n.roles = roles
            .iter()
            .filter_map(|r| r.as_str().map(str::to_string))
            .collect();
    }
    if let Some(props) = obj.get("props").and_then(|x| x.as_object()) {
        for (k, pv) in props {
            if let Some(s) = pv.as_str() {
                n.props.insert(k.clone(), s.to_string());
            }
        }
    }
    if let Some(pos) = obj.get("pos").and_then(|x| x.as_object()) {
        let g = |key: &str| {
            pos.get(key)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
        };
        n.pos = Some(cf_uast_node::Positions {
            start_line: g("start_line"),
            start_col: g("start_col"),
            start_offset: g("start_offset"),
            end_line: g("end_line"),
            end_col: g("end_col"),
            end_offset: g("end_offset"),
        });
    }
    if let Some(children) = obj.get("children").and_then(|x| x.as_array()) {
        n.children = children.iter().map(json_to_node).collect();
    }
    n
}

/// Prints DSL help (query.go `printDSLHelp`). Human/non-binding.
fn print_dsl_help(out: &mut dyn Write) {
    let _ = writeln!(out, "DSL Syntax:");
    let _ = writeln!(
        out,
        "  filter(.type == \"Function\")     - Filter by node type"
    );
    let _ = writeln!(
        out,
        "  filter(.type == \"Call\")         - Find function calls"
    );
    let _ = writeln!(
        out,
        "  filter(.type == \"Identifier\")   - Find identifiers"
    );
    let _ = writeln!(out, "  filter(.type == \"Literal\")      - Find literals");
    let _ = writeln!(out);
}

/// Case-insensitive `.json` suffix check (query.go `isJSONFile`).
fn is_json_file(file: &str) -> bool {
    file.to_lowercase().ends_with(".json")
}

fn opt(name: &'static str, short: char, default: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .short(short)
        .help(help)
        .default_value(default)
        .action(ArgAction::Set)
}

fn flag(name: &'static str, short: Option<char>, help: &'static str) -> Arg {
    let mut a = Arg::new(name)
        .long(name)
        .help(help)
        .action(ArgAction::SetTrue);
    if let Some(s) = short {
        a = a.short(s);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_results_value() {
        let v = nodes_to_value(&[]);
        let s = String::from_utf8(cf_textutil::marshal_json(&v, false).unwrap()).unwrap();
        assert_eq!(s, "{\"results\":[]}\n");
    }

    #[test]
    fn all_literal_results_are_tokens() {
        let nodes = vec![Node::literal("42"), Node::literal("foo")];
        let v = nodes_to_value(&nodes);
        let s = String::from_utf8(cf_textutil::marshal_json(&v, false).unwrap()).unwrap();
        assert_eq!(s, "{\"results\":[\"42\",\"foo\"]}\n");
    }

    #[test]
    fn mixed_results_use_to_map() {
        let nodes = vec![Node::with_token("Function", "f")];
        let v = nodes_to_value(&nodes);
        let s = String::from_utf8(cf_textutil::marshal_json(&v, false).unwrap()).unwrap();
        // Go `ToMap` always emits `pos` (all-zero here) and `roles` ([]).
        assert_eq!(
            s,
            "{\"results\":[{\"pos\":{\"end_col\":0,\"end_line\":0,\"end_offset\":0,\
\"start_col\":0,\"start_line\":0,\"start_offset\":0},\"roles\":[],\"token\":\"f\",\"type\":\"Function\"}]}\n"
        );
    }

    #[test]
    fn count_reads_results_len() {
        let v = nodes_to_value(&[Node::literal("a"), Node::literal("b"), Node::literal("c")]);
        assert_eq!(result_count(&v), 3);
    }

    #[test]
    fn json_roundtrip_through_decode() {
        // decode a UAST JSON tree then re-serialize via to_map; keys byte-sort.
        let json = br#"{"type":"File","children":[{"type":"Identifier","token":"x"}]}"#;
        let node = decode_uast(json).unwrap();
        let v = node_to_value(&node);
        let s = String::from_utf8(cf_textutil::marshal_json(&v, false).unwrap()).unwrap();
        // Go `ToMap` always emits `pos` (all-zero) and `roles` ([]) on each node.
        const ZERO_POS: &str = "\"pos\":{\"end_col\":0,\"end_line\":0,\"end_offset\":0,\
\"start_col\":0,\"start_line\":0,\"start_offset\":0}";
        assert_eq!(
            s,
            format!(
                "{{\"children\":[{{{ZERO_POS},\"roles\":[],\"token\":\"x\",\"type\":\"Identifier\"}}],\
{ZERO_POS},\"roles\":[],\"type\":\"File\"}}\n"
            )
        );
    }

    #[test]
    fn is_json_file_case_insensitive() {
        assert!(is_json_file("tree.json"));
        assert!(is_json_file("TREE.JSON"));
        assert!(!is_json_file("main.go"));
    }
}
