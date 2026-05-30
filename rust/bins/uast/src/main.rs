//! `uast` binary — clap builder mirroring the cobra CLI (DESIGN §4.2).
//!
//! Root: `Use "uast"`, Short "UAST (Universal Abstract Syntax Tree) parser and
//! analyzer". Persistent flags --config, --verbose/-v, --quiet/-q. 11
//! subcommands in main.go AddCommand order: parse, diff, query, explore,
//! analyze, completion, version, validate, mapping, lsp, server. Short/Use
//! strings, flag long/short/default, and valid-value lists are copied verbatim
//! from cmd/uast/*.go.
//!
//! ERROR-HANDLING ASYMMETRY (DESIGN §4): uast does NOT set
//! SilenceErrors/SilenceUsage, so on a runtime error cobra prints usage + the
//! error. We let clap surface usage on parse errors, and on a body error we
//! print `Error: <msg>\n` and the command usage, matching cobra's flow.
//!
//! `uast validate` uses os.Exit 0/1/2 directly (validate.go), with --no-color
//! winning over --color. Un-ported subcommand bodies print the stub marker and
//! exit 1 so the golden harness can SKIP.

use std::process::exit;

use clap::{Arg, ArgAction, Command};

/// Marker on stderr for not-yet-ported bodies (golden harness SKIP signal).
const UNIMPLEMENTED_MARKER: &str = "uast: not yet implemented in the Rust port";

/// validate exit codes (validate.go: exitCodeValidationFailure = 2).
const VALIDATE_EXIT_OK: i32 = 0;
const VALIDATE_EXIT_INVALID: i32 = 1;
const VALIDATE_EXIT_ERROR: i32 = 2;

fn build_cli() -> Command {
    Command::new("uast")
        .about("UAST (Universal Abstract Syntax Tree) parser and analyzer")
        .long_about("UAST is a tool for parsing source code into Universal Abstract Syntax Trees.")
        // Persistent flags (uast main.go:29-31).
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
        .subcommand(build_parse())
        .subcommand(build_diff())
        .subcommand(build_query())
        .subcommand(build_explore())
        .subcommand(build_analyze())
        .subcommand(build_completion())
        .subcommand(Command::new("version").about("Show version information"))
        .subcommand(build_validate())
        .subcommand(build_mapping())
        .subcommand(Command::new("lsp").about("Start language server for mapping and query DSL (LSP)"))
        .subcommand(build_server())
}

fn build_parse() -> Command {
    Command::new("parse")
        .about("Parse source code files into UAST")
        .override_usage("uast parse [files...] [flags]")
        .arg(Arg::new("files").num_args(0..).index(1))
        .arg(opt("language", 'l', "", "force language detection"))
        .arg(opt("output", 'o', "", "output file (default: stdout)"))
        .arg(opt("format", 'f', "json", "output format (json, compact, tree, none)"))
        .arg(flag("progress", Some('p'), "show progress for multiple files"))
        .arg(flag("all", None, "parse all source files in the codebase recursively"))
        .arg(
            Arg::new("workers")
                .long("workers")
                .short('w')
                .help("number of parallel workers (default: number of CPUs)")
                .default_value("0")
                .value_parser(clap::value_parser!(i64))
                .action(ArgAction::Set),
        )
}

fn build_diff() -> Command {
    Command::new("diff")
        .about("Compare two files and detect changes")
        .arg(Arg::new("file1").required(true).index(1))
        .arg(Arg::new("file2").required(true).index(2))
        .arg(opt("output", 'o', "", "output file (default: stdout)"))
        .arg(opt("format", 'f', "unified", "output format (unified, summary, json)"))
}

fn build_query() -> Command {
    Command::new("query")
        .about("Query UAST with DSL expressions")
        .override_usage("uast query [query] [files...] [flags]")
        .arg(Arg::new("args").num_args(0..).index(1))
        .arg(opt("input", 'i', "", "input file (UAST JSON or source code)"))
        .arg(opt("output", 'o', "", "output file (default: stdout)"))
        .arg(opt("format", 'f', "json", "output format (json, compact, count)"))
        .arg(flag("interactive", Some('t'), "interactive query mode"))
}

fn build_explore() -> Command {
    Command::new("explore")
        .about("Interactive UAST exploration")
        .arg(Arg::new("file").num_args(0..=1).index(1))
        .arg(opt("language", 'l', "", "force language detection"))
}

fn build_analyze() -> Command {
    Command::new("analyze")
        .about("Analyze UAST tree structure and composition")
        .override_usage("uast analyze [files...] [flags]")
        .arg(Arg::new("files").num_args(0..).index(1))
        .arg(opt("output", 'o', "", "output file (default: stdout)"))
        .arg(opt("format", 'f', "text", "output format (text, json, html)"))
}

fn build_completion() -> Command {
    Command::new("completion")
        .about("Generate shell completion scripts")
        .arg(
            Arg::new("shell")
                .required(true)
                .index(1)
                .value_parser(["bash", "zsh", "fish", "powershell"]),
        )
}

fn build_validate() -> Command {
    Command::new("validate")
        .about("Validate a UAST JSON file against the UAST schema")
        .override_usage("uast validate <file.json|-> [flags]")
        .arg(Arg::new("file").required(true).index(1))
        .arg(
            Arg::new("schema")
                .long("schema")
                .help("path to UAST JSON schema")
                .default_value("pkg/uast/spec/uast-schema.json")
                .action(ArgAction::Set),
        )
        .arg(flag("color", None, "force colored output"))
        .arg(flag("no-color", None, "disable colored output"))
}

fn build_mapping() -> Command {
    Command::new("mapping")
        .about("UAST mapping helpers: grammar analysis, classification, coverage")
        .arg(opt_long("node-types", "", "Path to node-types.json (required for non-treesitter operations)"))
        .arg(opt_long("mapping", "", "Path to mapping DSL file (optional)"))
        .arg(opt_long("format", "text", "Output format: text or json"))
        .arg(flag("coverage", None, "Compute mapping coverage if mapping is provided"))
        .arg(flag("generate", None, "Generate .uastmap DSL from node-types.json"))
        .arg(flag("show-treesitter", None, "Show original tree-sitter JSON structure for input files"))
        .arg(opt_long("language", "", "Language for tree-sitter parsing (language name or grammar file path)"))
        .arg(opt_long("extensions", "", "Comma-separated list of file extensions for language declaration"))
}

fn build_server() -> Command {
    Command::new("server")
        .about("Start UAST development server")
        .arg(opt("port", 'p', "8080", "port to listen on"))
        .arg(opt("static", 's', "", "directory to serve static files from"))
}

// --- small builders to keep flag declarations readable ---

fn opt(name: &'static str, short: char, default: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .short(short)
        .help(help)
        .default_value(default)
        .action(ArgAction::Set)
}

fn opt_long(name: &'static str, default: &'static str, help: &'static str) -> Arg {
    Arg::new(name).long(name).help(help).default_value(default).action(ArgAction::Set)
}

fn flag(name: &'static str, short: Option<char>, help: &'static str) -> Arg {
    let mut a = Arg::new(name).long(name).help(help).action(ArgAction::SetTrue);
    if let Some(s) = short {
        a = a.short(s);
    }
    a
}

fn main() {
    let matches = build_cli().get_matches();

    match matches.subcommand() {
        Some(("version", _)) => print!("{}", cf_version::uast_version_line()),
        Some(("validate", sub)) => validate_dispatch(sub),
        Some((name, _)) => stub(name),
        None => {
            build_cli().print_help().ok();
            println!();
        }
    }
}

/// uast prints usage on error (asymmetry vs codefang). For an un-ported body we
/// print the stub marker, then the command usage, then exit 1.
fn stub(cmd_name: &str) -> ! {
    eprintln!("Error: {UNIMPLEMENTED_MARKER}");
    if let Some(mut sub) = build_cli().find_subcommand(cmd_name).cloned() {
        let _ = sub.print_help();
        eprintln!();
    }
    exit(1);
}

/// validate uses os.Exit 0/1/2 directly; --no-color wins over --color.
fn validate_dispatch(sub: &clap::ArgMatches) -> ! {
    let no_color = sub.get_flag("no-color");
    let _color = if no_color { false } else { sub.get_flag("color") };
    // Body not ported: report engine-not-available as exit code 2 (the "error"
    // class), matching validate's exit semantics rather than the cobra path.
    eprintln!("Error: {UNIMPLEMENTED_MARKER}");
    let _ = (VALIDATE_EXIT_OK, VALIDATE_EXIT_INVALID);
    exit(VALIDATE_EXIT_ERROR);
}

/// Keep tree-sitter in the dependency graph until cf-uast lands.
#[allow(dead_code)]
fn _treesitter_link_anchor() -> usize {
    tree_sitter::MIN_COMPATIBLE_LANGUAGE_VERSION
}
