//! `uast` binary — the standalone UAST CLI (port of Go `cmd/uast`).
//!
//! A clap **builder**-API mirror of the cobra CLI in `cmd/uast/*.go`
//! (DESIGN §4.2). Root: `Use "uast"`, Short "UAST (Universal Abstract Syntax
//! Tree) parser and analyzer". Persistent flags `--config`, `--verbose`/`-v`,
//! `--quiet`/`-q`. The 11 subcommands are registered in the same order as
//! `cmd/uast/main.go`'s `AddCommand` calls: parse, diff, query, explore,
//! analyze, completion, version, validate, mapping, lsp, server. Every
//! subcommand's `Use`/`Short`, flag long/short/default/help string, and valid
//! format-value list is copied verbatim from the Go source so help/usage and
//! error wording match cobra (the Layer-D CLI golden, DESIGN §6).
//!
//! ## Error-handling asymmetry (DESIGN §4)
//!
//! `cmd/uast/main.go` does **not** set cobra's `SilenceErrors`/`SilenceUsage`,
//! and its `main` prints `Error: %v\n` to stderr then `os.Exit(1)` on any
//! subcommand error. We reproduce that: a subcommand body error is printed as
//! `Error: <msg>\n` to stderr and the process exits 1. (This is the opposite of
//! the `codefang` binary, which silences usage.)
//!
//! `uast validate` exits via `os.Exit` with codes 0/1/2 (validate.go), so it is
//! dispatched specially and never goes through the generic error path.
//!
//! ## Serialization (DESIGN rule 1)
//!
//! Every machine-format report (`parse`/`query`/`analyze`/`diff`/`mapping`
//! `--format json|compact`, and the `server` HTTP responses) is serialized
//! through [`cf_textutil::write_json`], which wraps the shared `cf-gojson`
//! Go-byte-compatible encoder — never `serde_json`. `serde_json` is used only to
//! *decode* input (UAST JSON files/stdin, `node-types.json`, the validate
//! input), which DESIGN §2 permits.

mod analyze;
mod completion;
mod diff;
mod explore;
mod govalue_bridge;
mod mapping;
mod parse;
mod query;
mod server;
mod validate;

use std::process::exit;

use clap::{Arg, ArgAction, Command};

/// The `"json"` output-format constant (Go `formatJSON`, main.go:14).
pub const FORMAT_JSON: &str = "json";
/// The `"compact"` output-format constant (Go `formatCompact`, parse.go:29).
pub const FORMAT_COMPACT: &str = "compact";
/// The `"none"` output-format constant (Go `formatNone`, parse.go:28).
pub const FORMAT_NONE: &str = "none";

/// The percent multiplier used by `analyze`/`mapping` (Go `coveragePercent`).
pub const COVERAGE_PERCENT: f64 = 100.0;

/// Builds the root `uast` command with all 11 subcommands (main.go:23-43).
fn build_cli() -> Command {
    Command::new("uast")
        .about("UAST (Universal Abstract Syntax Tree) parser and analyzer")
        .long_about("UAST is a tool for parsing source code into Universal Abstract Syntax Trees.")
        // Persistent flags (main.go:29-31).
        .arg(
            Arg::new("config")
                .long("config")
                .help("config file (default is $HOME/.uast.yaml)")
                .global(true)
                .default_value("")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .short('v')
                .help("verbose output")
                .global(true)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("quiet")
                .long("quiet")
                .short('q')
                .help("suppress output")
                .global(true)
                .action(ArgAction::SetTrue),
        )
        // Subcommands in main.go AddCommand order.
        .subcommand(parse::command())
        .subcommand(diff::command())
        .subcommand(query::command())
        .subcommand(explore::command())
        .subcommand(analyze::command())
        .subcommand(completion::command())
        .subcommand(Command::new("version").about("Show version information"))
        .subcommand(validate::command())
        .subcommand(mapping::command())
        .subcommand(
            Command::new("lsp")
                .about("Start language server for mapping and query DSL (LSP)")
                .long_about(
                    "Start a language server (LSP) for .uastmap and query DSL files (stdio mode).",
                ),
        )
        .subcommand(server::command())
}

fn main() {
    // cobra exits 1 on a usage error (bad flag / unknown command / missing arg)
    // and 0 when it merely printed help/version; clap's own `get_matches` would
    // exit 2 on a usage error, so parse explicitly and map the exit code to
    // cobra's contract (the cli-surface error-path probes assert rc==1).
    let matches = match build_cli().try_get_matches() {
        Ok(m) => m,
        Err(e) => {
            e.print().ok();
            exit(i32::from(e.use_stderr()));
        }
    };

    // version uses cobra `Run` (no error path), exit 0 (main.go:56-58).
    let result: Result<(), String> = match matches.subcommand() {
        Some(("version", _)) => {
            print!("{}", cf_version::uast_version_line());
            Ok(())
        }
        Some(("parse", sub)) => parse::run(sub),
        Some(("diff", sub)) => diff::run(sub),
        Some(("query", sub)) => query::run(sub),
        Some(("explore", sub)) => explore::run(sub),
        Some(("analyze", sub)) => analyze::run(sub),
        Some(("completion", sub)) => completion::run(sub),
        // validate exits via process::exit 0/1/2 itself; it never returns Ok/Err.
        Some(("validate", sub)) => validate::run(sub),
        Some(("mapping", sub)) => mapping::run(sub),
        Some(("lsp", _)) => {
            run_lsp();
            Ok(())
        }
        Some(("server", sub)) => server::run(sub),
        _ => {
            // No subcommand: print help (cobra root with no args, exit 0).
            build_cli().print_help().ok();
            println!();
            Ok(())
        }
    };

    // uast does NOT silence errors/usage: print `Error: <msg>` and exit 1
    // (main.go:46-49). cobra also prints usage on a RunE error; clap's parse
    // errors already include usage, and here we surface the body error the same
    // way Go's `fmt.Fprintf(os.Stderr, "Error: %v\n", err)` does.
    if let Err(msg) = result {
        eprintln!("Error: {msg}");
        exit(1);
    }
}

/// Serves the mapping-DSL LSP over stdio (lsp.go `lsp.NewServer().Run()`).
///
/// `cf_uast_lsp::run_stdio` is async (tower-lsp); Go's `Run()` blocks, so we
/// drive it to completion on a fresh tokio multi-thread runtime.
fn run_lsp() {
    let rt = tokio::runtime::Runtime::new().expect("create tokio runtime for lsp");
    rt.block_on(cf_uast_lsp::run_stdio());
}

#[cfg(test)]
mod tests {
    use super::build_cli;

    #[test]
    fn cli_has_all_eleven_subcommands_in_order() {
        let cli = build_cli();
        let names: Vec<&str> = cli
            .get_subcommands()
            .map(clap::Command::get_name)
            .collect();
        assert_eq!(
            names,
            vec![
                "parse",
                "diff",
                "query",
                "explore",
                "analyze",
                "completion",
                "version",
                "validate",
                "mapping",
                "lsp",
                "server",
            ]
        );
    }

    #[test]
    fn root_about_matches_go() {
        let about = build_cli().get_about().map(|s| s.to_string()).unwrap_or_default();
        assert_eq!(about, "UAST (Universal Abstract Syntax Tree) parser and analyzer");
    }

    #[test]
    fn help_renders() {
        // `--help` must succeed (cobra root help). clap returns a DisplayHelp
        // "error" kind for --help; assert it is that kind, not a real failure.
        let err = build_cli().try_get_matches_from(["uast", "--help"]).unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn unknown_subcommand_is_error() {
        let err = build_cli().try_get_matches_from(["uast", "no-such-cmd"]).unwrap_err();
        // cobra prints "unknown command"; clap reports an invalid subcommand.
        assert!(matches!(
            err.kind(),
            clap::error::ErrorKind::InvalidSubcommand
                | clap::error::ErrorKind::UnknownArgument
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
        ));
    }
}
