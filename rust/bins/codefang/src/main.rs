//! `codefang` binary — clap builder mirroring the cobra CLI (DESIGN §4.1).
//!
//! Root: `Use "codefang"`, Short "Codefang Code Analysis - Unified code analysis
//! tool". Persistent flags --verbose/-v, --quiet/-q, --profile. Subcommands
//! run / render / version (mcp is `//go:build ignore` -> feature-gated, not
//! shipped).
//!
//! ERROR-HANDLING ASYMMETRY (DESIGN §4): codefang sets cobra
//! SilenceErrors+SilenceUsage -> on a runtime error it prints ONLY
//! `Error: <msg>\n` to stderr and exits 1, with NO usage block. We reproduce
//! this by handling subcommand-body errors ourselves and printing exactly that.
//! (uast, by contrast, prints usage on error.)
//!
//! Subcommand BODIES that are not yet ported call the stub path (which prints the
//! UNIMPLEMENTED marker and exits 1). The CLI SURFACE (every command, flag
//! long/short, default, help text — copied verbatim from cmd/codefang) is
//! complete so `--help` / `--version` and dispatch work and the golden harness
//! can diff help/usage and SKIP stubbed run bodies.
//!
//! `git2` is wired as a dependency so the workspace links libgit2 (vendored, see
//! the workspace Cargo.toml); the touch below keeps the link visible until the
//! gitlib port lands.

use std::process::exit;

use clap::{Arg, ArgAction, Command};

/// Marker on stderr for not-yet-ported bodies (golden harness SKIP signal).
const UNIMPLEMENTED_MARKER: &str = "codefang: not yet implemented in the Rust port";

fn build_cli() -> Command {
    Command::new("codefang")
        .about("Codefang Code Analysis - Unified code analysis tool")
        .long_about(
            "Codefang provides comprehensive code analysis tools.\n\n\
             Commands:\n  \
             run       Unified static + history analysis entrypoint\n  \
             render    Render stored analysis results as multi-page HTML",
        )
        // Persistent flags (cobra PersistentFlags on root).
        .arg(
            Arg::new("verbose")
                .long("verbose")
                .short('v')
                .help("enable detailed output")
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
        .arg(
            Arg::new("profile")
                .long("profile")
                .help("enable pprof server (localhost:6060) and memory watchdog")
                .global(true)
                .action(ArgAction::SetTrue),
        )
        .subcommand(build_run_command())
        .subcommand(build_render_command())
        .subcommand(Command::new("version").about("Show version information"))
}

/// `codefang run [path]` — the ~45 literal flags (DESIGN §4.1), declared in the
/// same order as `run.go:268-320, 791-802, 1010-1020`. Help strings are copied
/// verbatim from the Go source.
fn build_run_command() -> Command {
    let str_flag = |name: &'static str, default: &'static str, help: &'static str| {
        Arg::new(name).long(name).help(help).default_value(default).action(ArgAction::Set)
    };
    let int_flag = |name: &'static str, help: &'static str| {
        Arg::new(name)
            .long(name)
            .help(help)
            .default_value("0")
            .value_parser(clap::value_parser!(i64))
            .action(ArgAction::Set)
    };
    let bool_flag = |name: &'static str, help: &'static str| {
        Arg::new(name).long(name).help(help).action(ArgAction::SetTrue)
    };

    Command::new("run")
        .about("Run static and history analyzers")
        .long_about("Run selected static and history analyzers.")
        .arg(
            Arg::new("path_arg")
                .help("Folder/repository path to analyze")
                .num_args(0..=1) // MaximumNArgs(1)
                .index(1),
        )
        .arg(
            Arg::new("analyzers")
                .long("analyzers")
                .short('a')
                .help("Analyzer IDs or glob patterns (example: static/complexity,history/*,*)")
                .value_delimiter(',')
                .action(ArgAction::Append),
        )
        .arg(str_flag(
            "format",
            "json",
            "Output format: json, yaml, plot, bin, timeseries, ndjson, text, compact",
        ))
        .arg(bool_flag(
            "ndjson",
            "With --format timeseries: emit one JSON line per commit (NDJSON)",
        ))
        .arg(str_flag("input", "", "Input report path for cross-format conversion"))
        .arg(str_flag("input-format", "auto", "Input format: auto, json, bin"))
        .arg(int_flag("gogc", "GC percent for history pipeline (0 = auto, >0 = exact)"))
        .arg(str_flag(
            "ballast-size",
            "0",
            "Optional GC ballast size for history pipeline (0 = disabled)",
        ))
        .arg(bool_flag("silent", "Disable progress output"))
        .arg(bool_flag("no-color", "Disable colored static output"))
        .arg(
            Arg::new("path")
                .long("path")
                .short('p')
                .help("Folder/repository path to analyze")
                .default_value(".")
                .action(ArgAction::Set),
        )
        .arg(bool_flag("debug-trace", "Enable 100% trace sampling for debugging"))
        .arg(str_flag("cpuprofile", "", "Write CPU profile to file"))
        .arg(str_flag("heapprofile", "", "Write heap profile to file"))
        .arg(int_flag("limit", "Limit number of commits to analyze (0 = no limit)"))
        .arg(bool_flag("first-parent", "Follow only first parent of merge commits"))
        .arg(bool_flag("head", "Analyze only HEAD commit"))
        .arg(str_flag(
            "since",
            "",
            "Only analyze commits after this time (e.g., '24h', '2024-01-01', RFC3339)",
        ))
        .arg(int_flag("workers", "Number of parallel workers (0 = use CPU count)"))
        .arg(int_flag(
            "static-workers",
            "Number of parallel static analysis workers (0 = min(CPU count, 8))",
        ))
        // registerExclusionFlags (run.go:1010-1020) — help verbatim.
        .arg(bool_flag(
            "include-vendored",
            "Re-include vendored dependencies (detected by enry / Linguist) in analysis. \
             Default: exclude vendor/, node_modules/, third_party/, testdata/, minified bundles, etc.",
        ))
        .arg(bool_flag(
            "include-generated",
            "Re-include generated files (detected by enry / Linguist) in analysis. \
             Default: exclude generated code (protobuf, code-gen output).",
        ))
        .arg(
            Arg::new("extra-excluded-prefixes")
                .long("extra-excluded-prefixes")
                .help("Additional path prefixes to exclude from analysis (comma-separated)")
                .value_delimiter(',')
                .action(ArgAction::Append),
        )
        .arg(
            Arg::new("per-file")
                .long("per-file")
                .short('F')
                .help("Include per-file breakdowns and summary statistics in static output")
                .action(ArgAction::SetTrue),
        )
        .arg(int_flag("buffer-size", "Size of internal pipeline channels (0 = workers*2)"))
        .arg(int_flag("commit-batch-size", "Commits per processing batch (0 = default 100)"))
        .arg(str_flag(
            "blob-cache-size",
            "",
            "Max blob cache size (e.g., '256MB', '1GB'; empty = default 1GB)",
        ))
        .arg(int_flag("diff-cache-size", "Max diff cache entries (0 = default 10000)"))
        .arg(str_flag(
            "blob-arena-size",
            "",
            "Memory arena size for blob loading (e.g., '4MB'; empty = default 4MB)",
        ))
        .arg(str_flag("memory-budget", "", "Memory budget for auto-tuning (e.g., '512MB', '2GB')"))
        .arg(int_flag(
            "max-changes-per-commit",
            "Skip commits whose tree diff exceeds this many changes (0 = default 10000). \
             Commits over the cap are silently dropped from history, which can desync \
             burndown's tracked state for affected files. Raise on monorepos with \
             legitimate large commits (Pods updates, generated code dumps).",
        ))
        // registerPersistenceFlags (run.go:791-802). --checkpoint/--resume are
        // tri-state in Go (default true, read via Flags().Changed); modeled here
        // as default-true value flags so the value source can be detected.
        .arg(
            Arg::new("checkpoint")
                .long("checkpoint")
                .help("Enable checkpointing for crash recovery")
                .default_value("true")
                .value_parser(clap::value_parser!(bool))
                .num_args(0..=1)
                .default_missing_value("true")
                .action(ArgAction::Set),
        )
        .arg(str_flag(
            "checkpoint-dir",
            "",
            "Checkpoint directory (default: ~/.codefang/checkpoints)",
        ))
        .arg(
            Arg::new("resume")
                .long("resume")
                .help("Resume from checkpoint if available")
                .default_value("true")
                .value_parser(clap::value_parser!(bool))
                .num_args(0..=1)
                .default_missing_value("true")
                .action(ArgAction::Set),
        )
        .arg(bool_flag("clear-checkpoint", "Clear existing checkpoint before run"))
        .arg(str_flag(
            "cache-dir",
            "",
            "Incremental analysis cache directory (skip already-processed commits)",
        ))
        .arg(bool_flag("no-cache", "Force full re-analysis, overwriting any existing cache"))
        .arg(str_flag(
            "config",
            "",
            "Configuration file path (default: .codefang.yaml in CWD or $HOME)",
        ))
        .arg(bool_flag("list-analyzers", "List all available analyzer IDs and exit"))
        .arg(str_flag(
            "diagnostics-addr",
            "",
            "Start diagnostics HTTP server (health/metrics) at this address (e.g., :6060)",
        ))
        .arg(
            Arg::new("output")
                .long("output")
                .short('o')
                .help("Output directory for plot HTML files (required with --format plot)")
                .default_value("")
                .action(ArgAction::Set),
        )
        .arg(bool_flag(
            "keep-store",
            "Keep temp ReportStore directory after rendering (with --format plot)",
        ))
        .arg(str_flag("tmp-dir", "", "Directory for temporary spill files (default: system temp)"))
        // Deprecated (hidden) flags — exact cobra MarkDeprecated messages.
        .arg(
            Arg::new("skip-blacklist")
                .long("skip-blacklist")
                .help("DEPRECATED: use --include-vendored=false and --include-generated=false (the new defaults). See CHANGELOG for migration.")
                .hide(true)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("blacklisted-prefixes")
                .long("blacklisted-prefixes")
                .help("DEPRECATED: use --extra-excluded-prefixes; the old flag name is preserved for back-compat but will be removed in the next minor release.")
                .value_delimiter(',')
                .hide(true)
                .action(ArgAction::Append),
        )
        // registerAnalyzerFlags: dynamic per-analyzer flags. The static
        // --languages flag is always present; the rest are built from the
        // analyzer registry once cf-commands lands (DESIGN §4.1).
        .arg(
            Arg::new("languages")
                .long("languages")
                .help("Languages to analyze (comma-separated; empty = all supported)")
                .value_delimiter(',')
                .action(ArgAction::Append),
        )
}

/// `codefang render <store-dir>` (DESIGN §4.1).
fn build_render_command() -> Command {
    Command::new("render")
        .about("Render stored analysis results as multi-page HTML")
        .arg(
            Arg::new("store-dir")
                .help("Directory containing stored analysis results")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("output")
                .long("output")
                .short('o')
                .help("output directory for HTML files")
                .default_value("")
                .action(ArgAction::Set),
        )
}

fn main() {
    let matches = build_cli().get_matches();

    match matches.subcommand() {
        Some(("version", _)) => {
            // version uses cobra `Run` (no error path), exit 0.
            print!("{}", cf_version::codefang_version_line());
        }
        Some(("run", _sub)) => run_dispatch(),
        Some(("render", sub)) => render_dispatch(sub),
        _ => {
            // No subcommand: print help (cobra root with no args).
            build_cli().print_help().ok();
            println!();
        }
    }
}

/// Reproduce the codefang error path: `Error: <msg>\n` to stderr, exit 1, NO
/// usage (SilenceErrors+SilenceUsage).
fn fail(msg: &str) -> ! {
    eprintln!("Error: {msg}");
    exit(1);
}

fn run_dispatch() -> ! {
    // Surface is complete; the body is not yet ported. Emit the stub marker
    // through the codefang error path so the golden harness SKIPs `stubbed`
    // records.
    fail(UNIMPLEMENTED_MARKER);
}

fn render_dispatch(sub: &clap::ArgMatches) -> ! {
    let output = sub.get_one::<String>("output").map(String::as_str).unwrap_or("");
    if output.is_empty() {
        // ErrNoOutputDir, exact wording (render.go:49).
        fail("output directory is required (use --output)");
    }
    fail(UNIMPLEMENTED_MARKER);
}

/// Keep git2 (and thus the vendored libgit2) in the dependency graph until
/// cf-gitlib lands. `Oid::from_bytes` links a core libgit2-backed code path.
#[allow(dead_code)]
fn _libgit2_link_anchor() -> bool {
    git2::Oid::from_bytes(&[0u8; 20]).is_ok()
}
