//! `uast parse [files...]` — parse source files into UAST (port of
//! `cmd/uast/parse.go`).
//!
//! Resolves files (`--all` walks `.` skipping hidden dirs), parses each into a
//! UAST, and serializes via [`cf_textutil::write_json`] (DESIGN rule 1). With no
//! files, or for a `-` file argument, it reads stdin (`stdin.go`, or
//! `stdin.<lang>` when `--language` is set) and assigns stable IDs before
//! output.
//!
//! Output formats (parse.go `outputNode`): `json` (pretty), `compact`, `none`
//! (parse only, no serialization). Any other value (the Go help also listed
//! `tree`, which no code path renders) is `unsupported format: <fmt>`.

use std::io::{self, Read, Write};
use std::path::Path;

use cf_uast::Parser;
use clap::{Arg, ArgAction, ArgMatches, Command};

use crate::govalue_bridge::node_to_value;
use crate::{FORMAT_COMPACT, FORMAT_JSON, FORMAT_NONE, STDIN_ARG};

/// Builds the `parse` subcommand (parse.go:39-66).
pub fn command() -> Command {
    Command::new("parse")
        .about("Parse source code files into UAST")
        .override_usage("uast parse [files...] [flags]")
        .arg(Arg::new("files").num_args(0..).index(1))
        .arg(opt("language", 'l', "", "force language detection"))
        .arg(opt("output", 'o', "", "output file (default: stdout)"))
        .arg(opt(
            "format",
            'f',
            FORMAT_JSON,
            "output format (json, compact, none)",
        ))
        .arg(flag(
            "all",
            "parse all source files in the codebase recursively",
        ))
}

/// Runs `parse` (parse.go `runParse`).
pub fn run(m: &ArgMatches) -> Result<(), String> {
    let files: Vec<String> = m
        .get_many::<String>("files")
        .map(|v| v.cloned().collect())
        .unwrap_or_default();
    let lang = m
        .get_one::<String>("language")
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
    let all = m.get_flag("all");

    let parser = Parser::new();

    let files = resolve_files(files, all, &parser)?;

    let mut writer = crate::open_output(output)?;
    if files.is_empty() {
        return parse_stdin(lang, &mut *writer, format);
    }

    // Go has a parallel path for `>1 file && format==none`; pooling is
    // behavioral-only (DESIGN), so the sequential path is byte-identical.
    for file in &files {
        if file == STDIN_ARG {
            parse_stdin(lang, &mut *writer, format)?;
            continue;
        }
        parse_one(&parser, file, lang, &mut *writer, format)
            .map_err(|e| format!("failed to parse {file}: {e}"))?;
    }
    Ok(())
}

/// Resolves the file list, walking `.` when `--all` (parse.go `resolveFiles` +
/// `collectSourceFiles`). Errors `no source files found in the codebase` when
/// the walk yields nothing.
fn resolve_files(files: Vec<String>, all: bool, parser: &Parser) -> Result<Vec<String>, String> {
    if !all {
        return Ok(files);
    }
    let mut collected = Vec::new();
    collect_source_files(Path::new("."), parser, &mut collected)
        .map_err(|e| format!("failed to collect source files: {e}"))?;
    if collected.is_empty() {
        return Err("no source files found in the codebase".to_string());
    }
    Ok(collected)
}

/// Recursively collects supported source files, skipping hidden directories
/// (parse.go `collectSourceFiles` / `isHiddenDir`).
fn collect_source_files(dir: &Path, parser: &Parser, out: &mut Vec<String>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if is_hidden_dir(&name) {
                continue;
            }
            collect_source_files(&path, parser, out)?;
        } else if let Some(s) = path.to_str() {
            if parser.is_supported(s) {
                out.push(s.to_string());
            }
        }
    }
    Ok(())
}

/// Hidden-directory test (parse.go `isHiddenDir`): a name longer than one byte
/// that starts with a dot.
fn is_hidden_dir(name: &str) -> bool {
    name.len() > 1 && name.starts_with('.')
}

/// Parses stdin into UAST and outputs it (parse.go `parseStdin`).
fn parse_stdin(lang: &str, writer: &mut dyn Write, format: &str) -> Result<(), String> {
    let mut code = Vec::new();
    io::stdin()
        .read_to_end(&mut code)
        .map_err(|e| format!("failed to read stdin: {e}"))?;

    let parser = Parser::new();
    let filename = if lang.is_empty() {
        "stdin.go".to_string()
    } else {
        format!("stdin.{lang}")
    };

    let mut node = parser
        .parse(&filename, &code)
        .map_err(|e| format!("parse error: {e}"))?;
    node.assign_stable_ids();
    output_node(&node, writer, format)
}

/// Parses a single file and outputs it (parse.go `parseFileWithParser`).
fn parse_one(
    parser: &Parser,
    file: &str,
    lang: &str,
    writer: &mut dyn Write,
    format: &str,
) -> Result<(), String> {
    let mut node = parser.parse_file(file, lang).map_err(|e| e.to_string())?;

    if format == FORMAT_NONE {
        return Ok(());
    }
    node.assign_stable_ids();
    output_node(&node, writer, format)
}

/// Serializes a node to `writer` in `format` (parse.go `outputNode`).
fn output_node(node: &cf_uast::Node, writer: &mut dyn Write, format: &str) -> Result<(), String> {
    match format {
        FORMAT_JSON => write_node(writer, node, true),
        FORMAT_COMPACT => write_node(writer, node, false),
        FORMAT_NONE => Ok(()),
        // Go: default => `unsupported format: <fmt>` (ErrUnsupportedParseFmt).
        other => Err(format!("unsupported format: {other}")),
    }
}

/// Writes `node.to_map()` through the shared go-compat encoder (DESIGN rule 1).
fn write_node(writer: &mut dyn Write, node: &cf_uast::Node, pretty: bool) -> Result<(), String> {
    // Go parity: on an empty source the reference parser returns a NIL node
    // and `json.Marshal(nil)` emits `null`. The Rust loader surfaces that
    // collapsed root as `Node::default()` (empty type, no children, zero
    // positions) — a shape no real parse produces, since every lowered node
    // carries a type.
    if node.node_type.is_empty() && node.children.is_empty() && node.token.is_empty() {
        return writeln!(writer, "null").map_err(|e| e.to_string());
    }
    let value = node_to_value(node);
    cf_textutil::write_json(writer, &value, pretty).map_err(|e| e.to_string())
}

// --- small builders shared with other subcommands via local copies ---

fn opt(name: &'static str, short: char, default: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .short(short)
        .help(help)
        .default_value(default)
        .action(ArgAction::Set)
}

fn flag(name: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .help(help)
        .action(ArgAction::SetTrue)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_dir_detection() {
        assert!(is_hidden_dir(".git"));
        assert!(is_hidden_dir(".github"));
        assert!(!is_hidden_dir("."));
        assert!(!is_hidden_dir("src"));
    }

    #[test]
    fn tree_format_is_unsupported() {
        // parse.go's switch has no `tree` arm, so `tree` hits the default error.
        let node = cf_uast::Node::with_token("File", "");
        let err = output_node(&node, &mut io::sink(), "tree").unwrap_err();
        assert_eq!(err, "unsupported format: tree");
    }

    #[test]
    fn none_format_writes_nothing() {
        let node = cf_uast::Node::with_token("File", "");
        // `none` returns Ok without touching the writer.
        assert!(output_node(&node, &mut io::sink(), "none").is_ok());
    }
}
