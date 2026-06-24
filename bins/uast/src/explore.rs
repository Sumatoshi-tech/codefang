//! `uast explore [file]` — interactive UAST exploration (port of
//! `cmd/uast/explore.go`).
//!
//! Parses a file and opens a REPL (`tree`/`stats`/`find`/`query`/`help`/`quit`).
//! All output is human/non-binding (DESIGN §2.7); only the sentinel errors and
//! the prompt/help text are reproduced.

use std::io::{self, Write};

use cf_uast::{Node, Parser};
use clap::{Arg, ArgAction, ArgMatches, Command};

/// Builds the `explore` subcommand (explore.go:28-45).
pub fn command() -> Command {
    Command::new("explore")
        .about("Interactive UAST exploration")
        .arg(Arg::new("file").num_args(0..=1).index(1))
        .arg(
            Arg::new("language")
                .long("language")
                .short('l')
                .help("force language detection")
                .default_value("")
                .action(ArgAction::Set),
        )
}

/// Runs `explore` (explore.go `runExplore`).
pub fn run(m: &ArgMatches) -> Result<(), String> {
    let file = m
        .get_one::<String>("file")
        .map(String::as_str)
        .unwrap_or("");
    let lang = m
        .get_one::<String>("language")
        .map(String::as_str)
        .unwrap_or("");

    if file.is_empty() {
        return Err("no file specified for exploration".to_string());
    }

    let parser = Parser::new();
    if !parser.is_supported(file) {
        return Err(format!("unsupported file type: {file}"));
    }
    let node = parser
        .parse_file(file, lang)
        .map_err(|e| format!("failed to parse {file}: {e}"))?;

    let out = io::stdout();
    let mut out = out.lock();
    let _ = writeln!(
        out,
        "Exploring {}",
        cf_iosafety::sanitize_for_terminal(file)
    );
    let _ = writeln!(out, "Type 'help' for commands, 'quit' to exit");
    let _ = writeln!(out);

    repl(&node, &mut out)
}

/// The exploration REPL loop (explore.go `runExploreLoop`).
fn repl(node: &Node, out: &mut dyn Write) -> Result<(), String> {
    let stdin = io::stdin();
    loop {
        let _ = write!(out, "explore> ");
        let _ = out.flush();
        let mut line = String::new();
        if stdin.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            break;
        }
        let cmd = line.trim();
        if cmd.is_empty() {
            continue;
        }
        if cmd == "quit" || cmd == "exit" {
            break;
        }
        if cmd == "help" {
            print_help(out);
            continue;
        }
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        handle(&parts, node, out);
        let _ = writeln!(out);
    }
    Ok(())
}

/// Dispatches a parsed REPL command (explore.go `handleExploreParts`).
fn handle(parts: &[&str], node: &Node, out: &mut dyn Write) {
    match parts.first().copied() {
        Some("tree") => {
            let _ = writeln!(out, "Tree command is not available in this version.");
        }
        Some("stats") => print_stats(node, out),
        Some("find") => {
            if parts.len() < 2 {
                let _ = writeln!(out, "Usage: find <type>");
                return;
            }
            find_nodes(node, parts[1], out);
        }
        Some("query") => {
            if parts.len() < 2 {
                let _ = writeln!(out, "Usage: query <dsl-query>");
                return;
            }
            let q = parts[1..].join(" ");
            match node.find_dsl(&q) {
                Err(e) => {
                    let _ = writeln!(out, "Error: {e}");
                }
                Ok(results) => {
                    let _ = writeln!(out, "Found {} results", results.len());
                    for (idx, r) in results.iter().enumerate() {
                        let _ = writeln!(
                            out,
                            "[{}] {}: {}",
                            idx + 1,
                            cf_iosafety::sanitize_for_terminal(&r.node_type),
                            cf_iosafety::sanitize_for_terminal(&r.token),
                        );
                    }
                }
            }
        }
        Some(other) => {
            let _ = writeln!(
                out,
                "Unknown command: {}",
                cf_iosafety::sanitize_for_terminal(other)
            );
            let _ = writeln!(out, "Type 'help' for available commands");
        }
        None => {}
    }
}

/// Prints node-type statistics (explore.go `printStats`).
fn print_stats(root: &Node, out: &mut dyn Write) {
    use std::collections::BTreeMap;
    let mut stats: BTreeMap<String, i64> = BTreeMap::new();
    let mut total = 0i64;
    for n in root.pre_order() {
        *stats.entry(n.node_type.clone()).or_insert(0) += 1;
        total += 1;
    }
    let _ = writeln!(out, "Total nodes: {total}");
    let _ = writeln!(out, "By type:");
    for (t, c) in stats {
        let _ = writeln!(out, "  {t}: {c}");
    }
}

/// Finds nodes of a given type via the DSL (explore.go `findNodes`).
fn find_nodes(root: &Node, node_type: &str, out: &mut dyn Write) {
    let query = format!("filter(.type == {node_type:?})");
    match root.find_dsl(&query) {
        Err(e) => {
            let _ = writeln!(out, "Error: {e}");
        }
        Ok(results) => {
            let _ = writeln!(
                out,
                "Found {} nodes of type '{}':",
                results.len(),
                cf_iosafety::sanitize_for_terminal(node_type)
            );
            for (idx, r) in results.iter().enumerate() {
                let _ = writeln!(
                    out,
                    "[{}] {}: {}",
                    idx + 1,
                    cf_iosafety::sanitize_for_terminal(&r.node_type),
                    cf_iosafety::sanitize_for_terminal(&r.token),
                );
            }
        }
    }
}

/// Prints the exploration help (explore.go `printExploreHelp`).
fn print_help(out: &mut dyn Write) {
    let _ = writeln!(out, "Available commands:");
    let _ = writeln!(out, "  tree                    - Show AST tree structure");
    let _ = writeln!(out, "  stats                   - Show node statistics");
    let _ = writeln!(out, "  find <type>             - Find nodes by type");
    let _ = writeln!(out, "  query <dsl-query>       - Execute DSL query");
    let _ = writeln!(out, "  help                    - Show this help");
    let _ = writeln!(out, "  quit                    - Exit exploration");
    let _ = writeln!(out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::ArgMatches;

    fn matches(args: &[&str]) -> ArgMatches {
        command().get_matches_from(std::iter::once("explore").chain(args.iter().copied()))
    }

    #[test]
    fn no_file_errors() {
        let m = matches(&[]);
        assert_eq!(run(&m).unwrap_err(), "no file specified for exploration");
    }

    #[test]
    fn unsupported_file_type_errors() {
        let m = matches(&["nonexistent.unknownext"]);
        // Unsupported extension is reported before any file read.
        assert_eq!(
            run(&m).unwrap_err(),
            "unsupported file type: nonexistent.unknownext"
        );
    }
}
