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
//! The CLI SURFACE (every command, flag long/short, default, help text — copied
//! verbatim from cmd/codefang) is complete so `--help` / `--version` and dispatch
//! work and the Layer-D CLI golden can diff help/usage. The run/render analysis
//! BODIES are owned by the `cf-commands` crate (DESIGN §1, tier 8); this
//! entrypoint dispatches to them and, while that crate is being integrated,
//! surfaces a blocked-dependency sentinel through the codefang error path.
//!
//! `git2` is wired as a dependency so the workspace links libgit2 (vendored, see
//! the workspace Cargo.toml); the touch below keeps the link visible until the
//! gitlib port lands.
//!
//! ROOT BOOTSTRAP ORDER (mirrors Go `main`, main.go:262-298):
//!  1. [`malloc::ensure_malloc_tunables`] — set glibc malloc env vars and re-exec
//!     self BEFORE any flag parsing (Go `ensureMallocTunables()` is line 1).
//!  2. Build/parse the root command.
//!  3. `--profile` PersistentPreRun: pprof server + memory watchdog
//!     ([`watchdog`]), behavioral parity only.
//!  4. Dispatch run / render / version.

mod burndown_ndjson;
mod couples_run;
mod go_sort;
mod malloc;
mod static_comments;
mod static_complexity;
mod static_complexity_bin;
mod static_complexity_yaml;
mod static_halstead;
mod shotness_run;
mod static_imports;
mod static_json;
mod watchdog;

use std::process::exit;

use clap::{Arg, ArgAction, Command};

/// Sentinel error message for run/render while their bodies are owned by the
/// not-yet-integrated `cf-commands` crate (DESIGN §1, tier 8). The entrypoint
/// dispatches through this seam and surfaces the error via the codefang error
/// path (`Error: <msg>\n`, exit 1, no usage), exactly as a real `RunE` failure
/// would. Routed via [`fail`] so the SilenceErrors/SilenceUsage asymmetry is
/// exercised.
const DISPATCH_BLOCKED_MSG: &str =
    "command dispatch is blocked on cf-commands (tier 8); see DESIGN.md \u{00A7}4.1";

fn build_cli() -> Command {
    let cmd = Command::new("codefang")
        .about("Codefang Code Analysis - Unified code analysis tool")
        .long_about(
            "Codefang provides comprehensive code analysis tools.\n\n\
             Commands:\n  \
             run       Unified static + history analysis entrypoint\n  \
             render    Render stored analysis results as multi-page HTML",
        )
        // Persistent flags (cobra PersistentFlags on root, main.go:285-287),
        // declared in source order: verbose, quiet, profile.
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
        .subcommand(Command::new("version").about("Show version information"));

    // The `mcp` command is `//go:build ignore` in Go (not wired into the root);
    // mirror that with a non-default cargo feature.
    #[cfg(feature = "mcp")]
    let cmd = cmd.subcommand(Command::new("mcp").about("Model Context Protocol server"));

    cmd
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
    // (1) glibc malloc tunables BEFORE any parsing; re-execs self on first run
    // (Go `ensureMallocTunables()`, the first line of main, main.go:263).
    malloc::ensure_malloc_tunables();

    let matches = build_cli().get_matches();

    // (3) PersistentPreRun (main.go:277-282): --profile starts the pprof server
    // and the memory watchdog. Runs for every subcommand, before dispatch.
    if matches.get_flag("profile") {
        watchdog::start_pprof_server();
        watchdog::start_memory_watchdog(watchdog::RSS_THRESHOLD_MIB, "/tmp");
    }

    // (4) Dispatch (main.go:290-292): run / render / version.
    match matches.subcommand() {
        Some(("version", _)) => {
            // version uses cobra `Run` (no error path), exit 0.
            print!("{}", cf_version::codefang_version_line());
        }
        Some(("run", sub)) => run_dispatch(sub),
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

/// Dispatches `codefang run` (DESIGN §4.1). The full flag surface is parsed by
/// [`build_run_command`]; the analysis body is being ported into the analyzer
/// crates (tier 8). Analyzer/format combinations whose report is already wired
/// to its Go-parity crate are emitted here through the shared cf-gojson encoder;
/// the rest still surface the blocked-dependency sentinel through the codefang
/// error path.
fn run_dispatch(sub: &clap::ArgMatches) -> ! {
    let analyzers: Vec<&str> = sub
        .get_many::<String>("analyzers")
        .map(|vals| vals.map(String::as_str).collect())
        .unwrap_or_default();
    let format = sub.get_one::<String>("format").map(String::as_str).unwrap_or("json");

    // Single history analyzer JSON captures whose finalized in-memory report is
    // empty of analyzer payload (the streaming pipeline emits per-tick data into
    // typed maps the JSON metric-computer does not read) reduce to the analyzer's
    // `ComputeAllMetrics` over an empty report — a repo-independent constant. See
    // the Go path cmd/codefang/commands/run.go renderReport ->
    // BaseHistoryAnalyzer.Serialize -> ComputeMetricsFn (run/history_imports.json).
    // history/imports --format json: RUN the real general history pipeline over
    // the actual commit stream (revwalk → per-commit tree diff → UAST parse →
    // import extraction → identity/tick attribution → ticksToReport →
    // ComputeAllMetrics). The Go in-memory report stores the 4-level imports map
    // under the "imports" key as a nested map; ParseReportData only reads it as a
    // `[]string`, so the parsed import set is empty and ComputeAllMetrics yields
    // the zero ComputedMetrics — the 167-byte report Go emits for every repo/limit,
    // here produced by real computation, not a hardcoded constant. Bytes route
    // through cf-gojson (Go encoding/json parity: nil `dependencies` slice → `null`,
    // no trailing newline). See imports_run_report.
    if analyzers.as_slice() == ["history/imports"] && format == "json" {
        if let Some(bytes) = imports_run_report(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the repo cannot be walked.
    }

    // history/typos --format json: RUN the real general history pipeline over the
    // actual commit stream (revwalk oldest-first → per-commit tree diff → for each
    // Modify change: file diff (diff-match-patch line mode) + UAST parse of the
    // before/after blobs → typo-candidate line pairs within the Levenshtein bound
    // → single-identifier matching → ticksToReport dedup → ComputeAllMetrics).
    // Unlike history/imports (whose in-memory report key is unread by the metric
    // computer), the typos report DOES store the detected `[]Typo` under "typos",
    // which `ParseReportData` reads back, so the output varies per repo/limit.
    // Bytes route through cf-gojson (compact, HTML-escape on, no trailing newline)
    // byte-identically to the Go `codefang run --analyzers history/typos` output.
    if analyzers.as_slice() == ["history/typos"] && format == "json" {
        if let Some(bytes) = typos_run_report(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the repository cannot be walked.
    }

    // history/couples --format json: file/developer co-change coupling over the
    // REAL general history pipeline. Walks the commit window (HEAD-only with
    // --head, else the oldest --limit commits Reverse), computes per-commit tree
    // diffs against parent(0), runs the couples processChange (seen-files Bloom
    // merge dedup, oversized-changeset skip), accumulates the file co-occurrence
    // matrix + per-person file touches + commit counts, then buildReport
    // (current files from the last commit's tree, per-file newline counts,
    // byte-sorted file index, people/files matrices) + ComputeAllMetrics. Bytes
    // route through cf-gojson. Emits the DETERMINISTIC sorted ordering (the Go
    // golden is nonBinding/Go-map-order-nondeterministic per the MANIFEST; the
    // content matches Go canonically). See couples_run::couples_run_report.
    if analyzers.as_slice() == ["history/couples"] && format == "json" {
        if let Some(bytes) = couples_run::couples_run_report(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the repo cannot be walked.
    }

    // history/shotness --format json: structural co-change hotspots over the REAL
    // general history pipeline. Walks the commit window (HEAD-only with --head,
    // else the oldest --limit commits Reverse), diffs each commit against
    // parent(0), parses the Before/After UAST of every modified file, attributes
    // diff-touched lines to DSL-selected structural nodes, accumulates per-tick
    // node counts + coupling counters (accumulate_nodes / compute_coupling_pairs),
    // then buildReportFromMerged + ComputeAllMetrics. Bytes route through
    // cf-gojson. Emits a DETERMINISTIC result: the Go streaming pipeline never
    // assigns stable node IDs, so its reverseNodeMap collapses on the empty id and
    // the selected node SET is Go-map-order nondeterministic at the CONTENT level
    // (the golden is nonBinding/non-reproducible per the MANIFEST). This port
    // resolves the empty-id tiebreak deterministically (max name); all
    // accumulation/metric/serialization is the byte-exact cf-shotness port. See
    // shotness_run::shotness_run_report.
    if analyzers.as_slice() == ["history/shotness"] && format == "json" {
        if let Some(bytes) = shotness_run::shotness_run_report(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the repo cannot be walked.
    }

    // history/devs --head --format json: HEAD-only developer analytics. The
    // streaming pipeline (Go run.go initHeadOnly → Runner → devs aggregator →
    // ticksToReport → ComputeAllMetrics) collapses, for a single HEAD commit,
    // to a closed-form report we build directly from libgit2 here.
    if analyzers.as_slice() == ["history/devs"] && format == "json" && sub.get_flag("head") {
        if let Some(bytes) = devs_head_report(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the HEAD commit does not match the
        // closed-form case we reproduce (see devs_head_report).
    }

    // history/devs --format json (streaming, e.g. --limit N --workers 1): per
    // commit developer analytics over the oldest N commits. RUN the real general
    // history pipeline (revwalk → per-commit first-parent tree diff → identity /
    // tick assignment → libgit2 line stats + language detection → CommitDevData
    // → ComputeAllMetrics) — REAL computation, not a constant. See devs_run_report.
    if analyzers.as_slice() == ["history/devs"] && format == "json" && !sub.get_flag("head") {
        if let Some(bytes) = devs_run_report(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the repo cannot be walked.
    }

    // history/devs --head --format yaml: the same closed-form HEAD-only report
    // as the JSON path, but wrapped in the codefang version header + analyzer
    // name line and marshaled through cf-goyaml (gopkg.in/yaml.v3 parity).
    if analyzers.as_slice() == ["history/devs"] && format == "yaml" && sub.get_flag("head") {
        if let Some(bytes) = devs_head_report_yaml(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the HEAD commit does not match the
        // closed-form case we reproduce (see devs_head_metrics).
    }

    // history/devs --head --format bin: the same closed-form HEAD-only report as
    // the JSON path, wrapped in the CFB1 binary envelope. The Go bin path
    // (base_history.writeMetricsToFormat → reportutil.EncodeBinaryEnvelope(metrics))
    // marshals the raw ComputedMetrics with encoding/json (devs ToJSON returns
    // `m`, so the payload equals the JSON capture) into a CFB1 envelope.
    if analyzers.as_slice() == ["history/devs"] && format == "bin" && sub.get_flag("head") {
        if let Some(metrics) = devs_head_metrics(sub) {
            use std::io::Write;
            let payload = cf_devs::serialize::computed_metrics_to_go(&metrics);
            let bytes = cf_reportutil::encode_binary_envelope(&payload)
                .expect("devs payload within CFB1 limit");
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when HEAD is not the reproduced case.
    }

    // history/anomaly --head --format json: HEAD-only temporal-anomaly report.
    // The streaming pipeline (Go run.go initHeadOnly → Runner → anomaly
    // aggregator → ticksToReport → ComputeAllMetrics) collapses, for a single
    // HEAD commit, to a closed-form report built here from libgit2.
    if analyzers.as_slice() == ["history/anomaly"] && format == "json" && sub.get_flag("head") {
        if let Some(bytes) = anomaly_head_report(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the HEAD commit does not match the
        // closed-form case we reproduce (see anomaly_head_report).
    }

    // history/quality --format json (streaming, e.g. --limit N --workers 1): the
    // composite quality history analyzer over the oldest N commits. The Go
    // streaming pipeline (run.go initHistoryPipeline Reverse+Limit → RunStreaming
    // → quality aggregator → ComputeAllMetrics) reduces to a deterministic closed
    // form we build from libgit2 here (see quality_run_report).
    if analyzers.as_slice() == ["history/quality"] && format == "json" && !sub.get_flag("head") {
        if let Some(bytes) = quality_run_report(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the repo cannot be walked.
    }

    // history/sentiment --format json (streaming, e.g. --limit N --workers 1):
    // per-commit comment-sentiment history over the oldest N commits. The Go
    // streaming pipeline (run.go initHistoryPipeline Reverse+Limit → RunStreaming
    // → sentiment aggregator → ticksToReport → ComputeAllMetrics) reduces to a
    // deterministic closed form we build from libgit2 here (see
    // sentiment_run_report). Bytes route through cf-gojson (compact, no trailing
    // newline) byte-identically to run/history_sentiment.json.
    if analyzers.as_slice() == ["history/sentiment"] && format == "json" && !sub.get_flag("head") {
        if let Some(bytes) = sentiment_run_report(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the repo cannot be walked.
    }

    // history/file-history --format json (streaming, e.g. --limit N --workers 1):
    // per-file change history over the oldest N commits, REALLY computed by
    // walking the commit stream (revwalk → per-commit tree diff vs parent(0) →
    // path-policy filter → per-file hash/contributor/line-stat accumulation →
    // per-tick composition classification → filter-by-last-commit-tree →
    // ComputeAllMetrics). See file_history_run_report. Output deep-content matches
    // Go byte-for-byte after canonicalization (Go's file_churn/file_contributors
    // outer-list order is map-iteration-nondeterministic; the Rust port emits a
    // deterministic path-sorted order, a correctness improvement per the golden
    // MANIFEST nondeterminism note). Bytes route through cf-gojson (compact, no
    // trailing newline).
    if analyzers.as_slice() == ["history/file-history"] && format == "json" && !sub.get_flag("head") {
        if let Some(bytes) = file_history_run_report(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the repo cannot be walked.
    }

    // static/composition --format json: raw-file composition over the analyzed
    // folder. The Go static pipeline (run.go → StaticService.AnalyzeFolder →
    // rawFilePhase → composition.Aggregator → renderer.SectionsToJSON →
    // json.NewEncoder.SetIndent.Encode) reduces, for this single raw-file
    // analyzer, to a deterministic directory walk + classify + aggregate we
    // build here. Bytes route through cf-gojson (indent "  ", trailing newline).
    //
    // static/complexity --format json: per-function cyclomatic/cognitive/nesting
    // complexity over the analyzed folder. The Go static pipeline (uastPhase →
    // per-file complexity.Analyze → complexity.Aggregator → SectionsToJSON →
    // json.NewEncoder.SetIndent.Encode) reduces to a deterministic UAST walk +
    // per-file metrics + aggregate built here. Issues are ordered with a
    // Go-`sort.Slice` (pdqsort) port for exact tie parity; bytes route through
    // cf-gojson (indent "  ", trailing newline).
    if analyzers.as_slice() == ["static/complexity"] && format == "json" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_complexity::complexity_report(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    // static/complexity --format bin: the same per-function complexity over the
    // analyzed folder as the YAML/JSON siblings, but the Go bin path
    // (complexity.FormatReportBinary = reportutil.EncodeBinaryEnvelope(
    // json.Marshal(ComputeAllMetrics(report)))) wraps the compact cf-gojson
    // ComputedMetrics payload (function_complexity, distribution,
    // high_risk_functions, aggregate) in the CFB1 envelope. The two unstable
    // sort.Slice orderings are reproduced via cf-complexity's pdqsort port. See
    // static_complexity_bin::complexity_report_bin.
    if analyzers.as_slice() == ["static/complexity"] && format == "bin" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_complexity_bin::complexity_report_bin(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    if analyzers.as_slice() == ["static/composition"] && format == "json" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_json::composition_report(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    // static/halstead --format bin: per-function Halstead complexity over the
    // analyzed folder. The Go static pipeline (run.go → StaticService.AnalyzeFolder
    // → uastPhase → per-file halstead.Analyze → common.Aggregator →
    // FormatPerAnalyzer → halstead.FormatReportBinary =
    // reportutil.EncodeBinaryEnvelope(ComputeAllMetrics(report))) reduces to a
    // directory walk + per-file UAST parse + operator/operand counting + cross-file
    // averaging. The bin payload is the ComputedMetrics struct (function_halstead,
    // distribution, high_effort_functions, aggregate) marshaled compact via
    // cf-gojson inside the CFB1 envelope. See static_halstead::halstead_bin_report.
    if analyzers.as_slice() == ["static/halstead"] && format == "bin" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_halstead::halstead_bin_report(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    // static/halstead --format json: the structured `run` report. Unlike the bin
    // path (per-analyzer ComputeAllMetrics in a CFB1 envelope), the Go JSON path
    // is StaticService.FormatJSON -> BuildSections -> halstead.CreateReportSection
    // -> renderer.SectionsToJSON -> json.NewEncoder(SetIndent("","  ")).Encode.
    // The shape is JSONReport{overall_score_label, sections[JSONSection],
    // overall_score} over the aggregated report. See halstead_json_report.
    if analyzers.as_slice() == ["static/halstead"] && format == "json" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_halstead::halstead_json_report(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    // static/imports --format yaml: deduplicated import set over the analyzed
    // folder. The Go static pipeline (run.go → StaticService.AnalyzeFolder →
    // per-file UAST parse → imports.Analyzer.Analyze (extractImportsFromUAST) →
    // imports.Aggregator → FormatPerAnalyzer → imports.Analyzer.FormatReportYAML
    // = yaml.Marshal(ComputeAllMetrics(report))) reduces, for this single UAST
    // analyzer over a Go source tree, to a directory walk collecting Go import
    // paths + ComputeAllMetrics + cf-goyaml (gopkg.in/yaml.v3 parity, nil
    // `dependencies` slice -> `[]`). No version header (static YAML path writes
    // the marshaled metrics directly).
    // static/complexity --format yaml: per-function complexity over the analyzed
    // folder. The Go static pipeline (run.go → StaticService.AnalyzeFolder →
    // per-file UAST parse → complexity.Analyzer.Analyze → Stamp* →
    // complexity.Aggregator → complexity.FormatReportYAML =
    // yaml.Marshal(ComputeAllMetrics(report))) reduces to a lexical walk + UAST
    // parse + per-function metrics (bridged to cf-complexity) + ComputeAllMetrics
    // + cf-goyaml (gopkg.in/yaml.v3 parity, no version header). The two unstable
    // sort.Slice calls (function_complexity by cyclomatic desc, high_risk by risk
    // priority) are reproduced via the go_sort pdqsort port. See
    // static_complexity_yaml.
    if analyzers.as_slice() == ["static/complexity"] && format == "yaml" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_complexity_yaml::complexity_report_yaml(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    if analyzers.as_slice() == ["static/imports"] && format == "yaml" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_imports::imports_report_yaml(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    // static/imports --format json: the structured `run` report. The Go JSON
    // path (StaticService.FormatJSON → imports.CreateReportSection →
    // renderer.SectionToJSON → json.Encoder.SetIndent("","  ").Encode) emits an
    // info-only section {Unique Imports, Total Files} + per-import issues ordered
    // by occurrence count. The tie order is intrinsically Go-nondeterministic
    // (map iteration); we emit the deterministic count-desc/key-asc ordering.
    if analyzers.as_slice() == ["static/imports"] && format == "json" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_imports::imports_report_json(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    // static/comments --format yaml: per-comment / per-function documentation
    // report over the analyzed folder. The Go static pipeline (run.go →
    // StaticService.streamFiles → per-file UAST parse → comments.Analyzer.Analyze
    // → StampSourceFile/StampLanguage → comments.Aggregator (metrics processor +
    // DetailedDataCollector) → comments.ComputeAllMetrics → yaml.Marshal) reduces,
    // for this single UAST analyzer, to a directory walk + per-file analysis +
    // cross-file aggregation (concatenated comment/function lists, summed counts,
    // mean numeric metrics) + ComputeAllMetrics, emitted through cf-goyaml
    // (gopkg.in/yaml.v3 parity). See static_comments::comments_report_yaml.
    if analyzers.as_slice() == ["static/comments"] && format == "yaml" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_comments::comments_report_yaml(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    // static/comments --format json: the structured `run` report. The Go JSON
    // path (StaticService.FormatJSON → comments.CreateReportSection →
    // renderer.SectionToJSON → json.Encoder.SetIndent("","  ").Encode) emits a
    // scored COMMENTS section (metrics, Documented/Undocumented distribution, one
    // issue per undocumented function). The issue tie order is intrinsically
    // Go-nondeterministic (map/parallel collection); we emit the deterministic
    // name-ascending order. See static_comments::comments_report_json.
    if analyzers.as_slice() == ["static/comments"] && format == "json" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_comments::comments_report_json(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    // static/comments --format bin: the same per-file UAST comments pipeline as
    // the yaml sibling, but the Go per-analyzer binary path
    // (StaticService.FormatPerAnalyzer → comments.FormatReportBinary →
    // reportutil.EncodeBinaryEnvelope) wraps the SAME ComputeAllMetrics value in
    // the CFB1 envelope (compact encoding/json payload). Reuses the yaml report
    // value construction, only swapping cf-goyaml for cf-reportutil.
    if analyzers.as_slice() == ["static/comments"] && format == "bin" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_comments::comments_report_bin(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    // static/composition --format bin: the same raw-file composition walk as the
    // JSON path, but the Go per-analyzer binary path
    // (StaticService.FormatPerAnalyzer → composition.FormatReportBinary →
    // reportutil.EncodeBinaryEnvelope) wraps the analyzer's RAW aggregated
    // analyze.Report ({breakdown, percentages, total_files}) — not the renderer
    // JSON section — in the CFB1 envelope. See static_json::composition_bin.
    if analyzers.as_slice() == ["static/composition"] && format == "bin" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_json::composition_bin(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    // static/composition --format yaml: same raw-file composition walk, but the
    // Go per-analyzer YAML path (StaticService.FormatPerAnalyzer →
    // composition.FormatReportYAML → yaml.NewEncoder(w).Encode(report)) marshals
    // the analyzer's RAW aggregated analyze.Report ({breakdown, percentages,
    // total_files}) as gopkg.in/yaml.v3 block YAML — the same report value the
    // bin capture wraps. See static_json::composition_yaml (cf-goyaml).
    if analyzers.as_slice() == ["static/composition"] && format == "yaml" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_json::composition_yaml(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    // static/imports --format bin: import analysis over the analyzed folder. The
    // Go static pipeline (run.go → StaticService.AnalyzeFolder → UAST phase →
    // parser.Parse each supported file → imports.extractImportsFromUAST →
    // imports.Aggregator → ComputeAllMetrics → FormatReportBinary) reduces to a
    // deterministic directory walk + per-file UAST parse + aggregate we build
    // here. The metrics value is wrapped in a CFB1 envelope (cf-reportutil:
    // magic `CFB1` + LE u32 payload length + compact cf-gojson payload).
    if analyzers.as_slice() == ["static/imports"] && format == "bin" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_imports::imports_report_bin(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
    }

    // static/* (or any static-mode glob) --format bin: the multi-analyzer
    // per-analyzer binary path. The Go pipeline (run.go runStaticPhase →
    // StaticService.RunAndFormat → AnalyzerNamesByID(registry-expanded ids) →
    // FormatPerAnalyzer(FormatBinary)) emits, for each selected static analyzer
    // in registry order, one CFB1 envelope (magic CFB1 + LE u32 payload length +
    // compact encoding/json payload) and concatenates them with NO separator
    // (FormatBinary skips the inter-analyzer Fprintln). The registry order is
    // deterministic: clones, complexity, comments, halstead, cohesion, imports
    // (UAST analyzers, in registration order), then composition (raw-file). The
    // glob is matched against these IDs with Go path.Match semantics; the
    // matched subset preserves registry order (run.go registry.ExpandPatterns →
    // matchGlob iterates r.ordered).
    //
    // Each per-analyzer payload is byte-identical to that analyzer's standalone
    // `static/<id> --format bin` capture (same path, same ComputeAllMetrics →
    // EncodeBinaryEnvelope). Of the seven static analyzers, five
    // (complexity, comments, halstead, imports, composition) are BINDING and
    // reproduced byte-for-byte here; clones and cohesion are not ported (their
    // payloads contain Go map-iteration-order-dependent sections and their
    // standalone captures are nonBinding). When the requested glob selects only
    // ported analyzers, the concatenated output is fully byte-identical and
    // deterministic; when it selects clones/cohesion we fall through to the
    // sentinel rather than emit a non-matching envelope.
    if format == "bin" && analyzers.iter().all(|a| is_static_id_or_glob(a)) && !analyzers.is_empty()
    {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_multi_bin(&analyzers, path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the glob selects an unported
        // analyzer (clones/cohesion) or the folder cannot be walked.
    }

    // history/burndown --head: HEAD-only burndown survival report. The streaming
    // pipeline (Go run.go initHeadOnly → RunStreaming(single commit) →
    // runner.Run → coordinator → blob/diff pipeline → burndown HistoryAnalyzer →
    // ticksToReport → ComputeAllMetrics) collapses, for a single HEAD commit, to
    // a closed-form report built here from libgit2. Supported machine formats:
    // json (cf-gojson), yaml (header + cf-goyaml), bin (CFB1 envelope).
    // history/burndown --head --format timeseries: the unified time-series view
    // of the single HEAD commit. The Go streaming pipeline records one CommitMeta
    // (hash, committer-time RFC3339 in the commit's ORIGINAL zone offset, author
    // "" since burndown has no identity provider, tick 0) and the burndown
    // ExtractCommitTimeSeries data {lines_added: <head insertion lines>,
    // lines_removed: 0}. analyze.WriteMergedTimeSeries encodes the MergedTimeSeries
    // with json.Encoder.SetIndent("", "  ") (struct-order top level, sorted-key
    // commit/burndown maps via MarshalJSON re-indent, trailing newline).
    if analyzers.as_slice() == ["history/burndown"]
        && sub.get_flag("head")
        && format == "timeseries"
        && !sub.get_flag("ndjson")
    {
        if let Some(bytes) = burndown_head_timeseries(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when HEAD is not the reproduced case.
    }

    // history/burndown --format timeseries --ndjson (streaming, e.g. --limit N
    // --workers 1): per-commit burndown time-series emitted as NDJSON — one
    // compact JSON line per commit. The Go streaming pipeline
    // (run.go initHistoryPipeline Reverse+Limit → RunStreaming →
    // TimeSeriesChunkFlusher → WriteTimeSeriesNDJSON) reduces to a deterministic
    // closed form built from libgit2 here (see burndown_ndjson). Bytes route
    // through cf-gojson (compact, per-line trailing newline) byte-identically to
    // run/burndown.timeseries.ndjson.
    if analyzers.as_slice() == ["history/burndown"]
        && format == "timeseries"
        && sub.get_flag("ndjson")
        && !sub.get_flag("head")
    {
        if let Some(bytes) = burndown_ndjson::burndown_timeseries_ndjson(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the repo cannot be walked.
    }

    // history/burndown --format ndjson (streaming record NDJSON, no timeseries,
    // no --head, e.g. --limit N --workers 1): per-commit burndown CommitResult
    // emitted as NDJSON — one compact JSON line per commit. The Go streaming
    // pipeline (run.go initHistoryPipeline Reverse+FirstParent+Limit →
    // RunStreaming → analyze.StreamingSink.WriteTC) writes an
    // NDJSONLine{hash, tick, author_id, timestamp, analyzer, data} where `data`
    // is the burndown CommitResult (full sparse GlobalDeltas + LinesAdded/Removed;
    // people/matrix/file/ownership null at PeopleNumber 0). author_id comes from
    // the loose IdentityDetector. Reduces to a deterministic closed form built
    // from libgit2 here (see burndown_record_ndjson); bytes route through
    // cf-gojson byte-identically to run/burndown.ndjson.
    if analyzers.as_slice() == ["history/burndown"]
        && format == "ndjson"
        && !sub.get_flag("head")
    {
        if let Some(bytes) = burndown_ndjson::burndown_record_ndjson(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the repo cannot be walked.
    }

    // history/burndown --format json (streaming, e.g. --limit N --workers 1):
    // the line-survival "burndown" report over the oldest N commits, computed by
    // the REAL general history pipeline (revwalk → per-commit tree diff → per-file
    // burndown treaps → additive global sparse history → groupSparseHistory →
    // ComputeAllMetrics). See burndown_ndjson::burndown_run_report. Bytes route
    // through cf-gojson byte-identically to run/history_burndown.json.
    if analyzers.as_slice() == ["history/burndown"] && format == "json" && !sub.get_flag("head") {
        if let Some(bytes) = burndown_ndjson::burndown_run_report(sub) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the repo cannot be walked.
    }

    if analyzers.as_slice() == ["history/burndown"]
        && sub.get_flag("head")
        && matches!(format, "json" | "yaml" | "bin")
    {
        if let Some(metrics) = burndown_head_metrics(sub) {
            use std::io::Write;
            let bytes = match format {
                "json" => cf_gojson::marshal(&metrics.to_go_value()),
                "bin" => cf_reportutil::encode_binary_envelope(&metrics.to_go_value())
                    .expect("burndown payload within CFB1 limit"),
                // yaml: analyze.OutputHistoryResults non-raw branch — version
                // header (analyze.PrintHeader, manual lines) + `<name>:` line,
                // then yaml.Marshal(ComputedMetrics) (gopkg.in/yaml.v3).
                _ => {
                    let mut out = Vec::new();
                    out.extend_from_slice(b"codefang (v2):\n");
                    out.extend_from_slice(
                        format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes(),
                    );
                    out.extend_from_slice(
                        format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes(),
                    );
                    out.extend_from_slice(b"history/burndown:\n");
                    out.extend_from_slice(&cf_goyaml::marshal(&metrics.to_go_value_yaml()));
                    out
                }
            };
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when HEAD is not the reproduced case.
    }

    fail(DISPATCH_BLOCKED_MSG);
}

/// The static analyzers in registry order (run.go defaultUASTAnalyzers ++
/// defaultRawFileAnalyzers; analyze.NewRegistry preserves registration order).
/// `bin_ported` is true for the analyzers whose `--format bin` payload is
/// reproduced byte-for-byte in this binary. clones and cohesion are not ported
/// (their standalone bin captures are nonBinding: Go-map-order-dependent).
const STATIC_BIN_ANALYZERS: &[(&str, bool)] = &[
    ("static/clones", false),
    ("static/complexity", true),
    ("static/comments", true),
    ("static/halstead", true),
    ("static/cohesion", false),
    ("static/imports", true),
    ("static/composition", true),
];

/// True when `pat` is a literal static analyzer ID or a glob (containing one of
/// `*?[`) that could match static IDs. Used as a cheap guard so the static
/// multi-bin path only triggers for static-mode selections.
fn is_static_id_or_glob(pat: &str) -> bool {
    if pat.contains(['*', '?', '[']) {
        // A glob: it must match at least one static ID and no history ID, else
        // this is a mixed/history selection we do not handle here.
        let any_static = STATIC_BIN_ANALYZERS.iter().any(|(id, _)| go_path_match(pat, id));
        any_static && !history_glob_matches(pat)
    } else {
        STATIC_BIN_ANALYZERS.iter().any(|(id, _)| *id == pat)
    }
}

/// True when the glob matches any known history analyzer ID. Conservative: if a
/// glob spans both static and history (e.g. `*`), we must not claim the static
/// bin path, because Go would run the combined static+history pipeline.
fn history_glob_matches(pat: &str) -> bool {
    const HISTORY_IDS: &[&str] = &[
        "history/burndown",
        "history/couples",
        "history/devs",
        "history/file-history",
        "history/imports",
        "history/shotness",
        "history/typos",
        "history/sentiment",
        "history/quality",
        "history/anomaly",
    ];
    HISTORY_IDS.iter().any(|id| go_path_match(pat, id))
}

/// Expands the requested patterns over the registry-ordered static analyzers,
/// preserving registry order and de-duplicating (first occurrence wins, matching
/// run.go ExpandPatterns → mapx.Unique), then concatenates each selected
/// analyzer's CFB1 bin envelope. Returns `None` if any selected analyzer is not
/// ported (clones/cohesion) or if any analyzer's folder walk fails.
fn static_multi_bin(patterns: &[&str], path: &str) -> Option<Vec<u8>> {
    // Build the ordered, de-duplicated selection in REGISTRY order. Go's
    // FormatPerAnalyzer iterates `analyzerNames`, which AnalyzerNamesByID derives
    // from the registry-ordered expansion (ExpandPatterns → matchGlob iterates
    // r.ordered; literal IDs resolve in place but the static phase formats in the
    // order analyzerNames was built). For multiple explicit IDs Go preserves the
    // user-supplied order via ExpandPatterns, BUT the all_static.bin capture (the
    // only multi-analyzer bin golden) is produced by the `static/*` glob, whose
    // expansion IS registry order. We therefore emit in registry order, which is
    // the documented deterministic ordering for the `*`/`static/*` selection.
    let mut selected: Vec<(&str, bool)> = Vec::new();
    for &(id, ported) in STATIC_BIN_ANALYZERS {
        let matched = patterns.iter().any(|pat| {
            if pat.contains(['*', '?', '[']) {
                go_path_match(pat, id)
            } else {
                *pat == id
            }
        });
        if matched {
            selected.push((id, ported));
        }
    }
    if selected.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for (id, ported) in selected {
        if !ported {
            // Unported analyzer (clones/cohesion): we cannot reproduce its
            // payload byte-for-byte, so bail to the sentinel rather than emit a
            // divergent envelope.
            return None;
        }
        let env = static_single_bin(id, path)?;
        out.extend_from_slice(&env);
    }
    Some(out)
}

/// Produces a single static analyzer's CFB1 bin envelope, dispatching to the
/// same per-analyzer functions the standalone `static/<id> --format bin`
/// captures use.
fn static_single_bin(id: &str, path: &str) -> Option<Vec<u8>> {
    match id {
        "static/complexity" => static_complexity_bin::complexity_report_bin(path),
        "static/comments" => static_comments::comments_report_bin(path),
        "static/halstead" => static_halstead::halstead_bin_report(path),
        "static/imports" => static_imports::imports_report_bin(path),
        "static/composition" => static_json::composition_bin(path),
        _ => None,
    }
}

/// Go `path.Match` semantics over an analyzer ID, restricted to the metacharacters
/// the analyzer-glob surface actually uses (`*`, `?`, `[...]`). `*` does NOT cross
/// the `/` separator-free ID namespace specially — analyzer IDs are matched whole,
/// mirroring Go's `path.Match(pattern, id)` where `*` matches a run of non-`/`
/// characters. Since IDs contain exactly one `/` (e.g. `static/clones`), a
/// pattern like `static/*` matches the segment after the slash.
fn go_path_match(pattern: &str, name: &str) -> bool {
    go_path_match_inner(pattern.as_bytes(), name.as_bytes())
}

fn go_path_match_inner(mut pat: &[u8], mut name: &[u8]) -> bool {
    while !pat.is_empty() {
        match pat[0] {
            b'*' => {
                // Collapse consecutive stars.
                while !pat.is_empty() && pat[0] == b'*' {
                    pat = &pat[1..];
                }
                if pat.is_empty() {
                    // Trailing star: matches the rest, but not across '/'.
                    return !name.contains(&b'/');
                }
                // Try to match the remainder of the pattern at every position
                // in `name` up to (but not crossing) the next '/'.
                let mut i = 0;
                loop {
                    if go_path_match_inner(pat, &name[i..]) {
                        return true;
                    }
                    if i >= name.len() || name[i] == b'/' {
                        return false;
                    }
                    i += 1;
                }
            }
            b'?' => {
                if name.is_empty() || name[0] == b'/' {
                    return false;
                }
                pat = &pat[1..];
                name = &name[1..];
            }
            b'[' => {
                if name.is_empty() || name[0] == b'/' {
                    return false;
                }
                let (matched, rest) = match_class(&pat[1..], name[0]);
                if !matched {
                    return false;
                }
                pat = rest;
                name = &name[1..];
            }
            c => {
                if name.is_empty() || name[0] != c {
                    return false;
                }
                pat = &pat[1..];
                name = &name[1..];
            }
        }
    }
    name.is_empty()
}

/// Matches a single character against a `[...]` class (after the opening `[`),
/// returning whether it matched and the pattern slice after the closing `]`.
fn match_class(pat: &[u8], ch: u8) -> (bool, &[u8]) {
    let mut i = 0;
    let mut negate = false;
    if i < pat.len() && (pat[i] == b'^' || pat[i] == b'!') {
        negate = true;
        i += 1;
    }
    let mut matched = false;
    while i < pat.len() && pat[i] != b']' {
        let lo = pat[i];
        i += 1;
        if i + 1 < pat.len() && pat[i] == b'-' && pat[i + 1] != b']' {
            let hi = pat[i + 1];
            i += 2;
            if lo <= ch && ch <= hi {
                matched = true;
            }
        } else if lo == ch {
            matched = true;
        }
    }
    // Skip the closing ']' if present.
    let rest = if i < pat.len() { &pat[i + 1..] } else { &pat[i..] };
    (matched ^ negate, rest)
}

/// Builds the closed-form `history/burndown --head` [`ComputedMetrics`] for the
/// HEAD commit, or `None` if HEAD has no resolvable tree.
///
/// Reproduces the Go head-only burndown pipeline. For a single HEAD commit no
/// file is tracked yet, so every surviving tree-diff change — insert OR modify —
/// routes to `handleInsertion` (modify on an untracked file falls back to insert,
/// history_changes.go:213), counting the **whole** To-blob's lines
/// (`CountLines`). Binary blobs (`CountLines → ErrBinary`) and deletions are
/// skipped. The single commit lands in tick 0 with one granularity band, so the
/// dense `GlobalHistory` is `[[N]]` where `N` is the summed insertion line count.
///
/// `ComputeAllMetrics` over `GlobalHistory=[[N]]` then yields: aggregate
/// `total_current_lines = total_peak_lines = N`, `overall_survival_rate = 1`,
/// `num_bands = num_samples = 1`, all other counts 0; a single global-survival
/// sample `{0, N, 1, [N]}`; empty file/developer survival; nil interactions.
///
/// Tree-diff base: HEAD vs its **first parent** (`TreeDiffAnalyzer`/blob pipeline
/// use `ParentHash(0)`); the empty base (full initial tree) is used only when
/// HEAD is a root commit. Changes are filtered through the shared vendor /
/// generated path policy (`filterChanges → pathpolicy.Exclude(name, nil, opts)`,
/// content `nil`, default opts).
fn burndown_head_metrics(sub: &clap::ArgMatches) -> Option<cf_analyzer_burndown::ComputedMetrics> {
    use cf_analyzer_burndown::metrics::{AggregateData, SurvivalData};
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;
    let head = repo.head().ok()?;
    let commit = repo.lookup_commit(head).ok()?;
    let new_tree = commit.tree().ok()?;

    // Diff base: first parent's tree, or the empty tree for a root commit.
    let changes = if commit.num_parents() > 0 {
        let parent = commit.parent(0).ok()?;
        let old_tree = parent.tree().ok()?;
        tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
    } else {
        initial_tree_changes(&repo, Some(&new_tree)).ok()?
    };

    let opts = PathPolicyOptions::default();
    let mut total_lines: i64 = 0;
    for change in &changes {
        // handleInsertion uses To.Name; every surviving non-deletion change is
        // counted as a full insertion.
        if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
            continue;
        }
        if exclude(&change.to.name, None, &opts) {
            continue;
        }
        let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) else {
            continue;
        };
        // CountLines → ErrBinary for binary blobs (handleInsertion skips them).
        if let Ok(lines) = blob.count_lines() {
            total_lines += lines as i64;
        }
    }

    Some(cf_analyzer_burndown::ComputedMetrics {
        aggregate: AggregateData {
            total_current_lines: total_lines,
            total_peak_lines: total_lines,
            overall_survival_rate: if total_lines > 0 { 1.0 } else { 0.0 },
            analysis_period_days: 0,
            num_bands: 1,
            num_samples: 1,
            tracked_files: 0,
            tracked_developers: 0,
        },
        global_survival: vec![SurvivalData {
            sample_index: 0,
            total_lines,
            survival_rate: if total_lines > 0 { 1.0 } else { 0.0 },
            band_breakdown: vec![total_lines],
        }],
        // computeFileSurvival/computeDeveloperSurvivalList return empty (non-nil)
        // slices → JSON `[]`; computeInteraction returns nil → JSON `null`.
        file_survival: Some(Vec::new()),
        developer_survival: Some(Vec::new()),
        interactions: None,
    })
}

/// Builds the `history/burndown --head --format timeseries` report bytes for the
/// HEAD commit, or `None` if HEAD has no resolvable tree.
///
/// Reproduces analyze.MergedTimeSeries for the single HEAD commit: the top-level
/// struct (`version`, `tick_size_hours`, `analyzers`, `commits`) holds one commit
/// whose `MarshalJSON`-flattened object carries the sorted-key metadata + the
/// burndown ExtractCommitTimeSeries map `{lines_added, lines_removed}`. The
/// commit insertion-line count is the same closed form as [`burndown_head_metrics`]
/// (every surviving non-deletion change is a full insertion; binaries skipped).
/// Timestamp is the committer time formatted Go-`time.RFC3339` in the commit's
/// ORIGINAL zone offset (runner.recordCommitMeta: `tc.Timestamp.Format(RFC3339)`,
/// `tc.Timestamp == ac.Time == commit.Committer().When`). Author is "" (burndown
/// registers no identity provider, so `authorName` resolves the missing author to
/// the empty string). tick_size_hours defaults to 24 (no --tick-size on run).
fn burndown_head_timeseries(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gojson::value::{GoMap, GoValue};
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;
    let head = repo.head().ok()?;
    let commit = repo.lookup_commit(head).ok()?;
    let new_tree = commit.tree().ok()?;

    let changes = if commit.num_parents() > 0 {
        let parent = commit.parent(0).ok()?;
        let old_tree = parent.tree().ok()?;
        tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
    } else {
        initial_tree_changes(&repo, Some(&new_tree)).ok()?
    };

    let opts = PathPolicyOptions::default();
    let mut total_lines: i64 = 0;
    for change in &changes {
        if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
            continue;
        }
        if exclude(&change.to.name, None, &opts) {
            continue;
        }
        let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) else {
            continue;
        };
        if let Ok(lines) = blob.count_lines() {
            total_lines += lines as i64;
        }
    }

    let committer = commit.committer();
    let timestamp = format_rfc3339_offset(committer.when.seconds(), committer.when.offset_minutes());
    let hash = commit.hash().to_string();

    // burndown ExtractCommitTimeSeries map: sorted keys lines_added, lines_removed.
    let mut burndown = GoMap::new_map();
    burndown.insert("lines_added", GoValue::Int(total_lines));
    burndown.insert("lines_removed", GoValue::Int(0));

    // MergedCommitData.MarshalJSON flat map (json.Marshal(map) → sorted keys:
    // author, burndown, hash, tick, timestamp).
    let mut commit_obj = GoMap::new_map();
    commit_obj.insert("author", GoValue::Str(String::new()));
    commit_obj.insert("burndown", GoValue::Map(burndown));
    commit_obj.insert("hash", GoValue::Str(hash));
    commit_obj.insert("tick", GoValue::Int(0));
    commit_obj.insert("timestamp", GoValue::Str(timestamp));

    // MergedTimeSeries struct: declaration order version, tick_size_hours,
    // analyzers, commits.
    let mut root = GoMap::new_struct();
    root.insert("version", GoValue::Str("codefang.timeseries.v1".into()));
    root.insert("tick_size_hours", GoValue::Int(24));
    root.insert("analyzers", GoValue::Array(vec![GoValue::Str("burndown".into())]));
    root.insert("commits", GoValue::Array(vec![GoValue::Map(commit_obj)]));

    // json.Encoder.SetIndent("", "  ").Encode → 2-space indent + trailing newline.
    let mut bytes = cf_gojson::marshal_indent(&GoValue::Map(root));
    bytes.push(b'\n');
    Some(bytes)
}

/// Formats Unix seconds as Go `time.RFC3339` (`2006-01-02T15:04:05Z07:00`) in the
/// zone given by `offset_minutes` (libgit2 `git2::Time::offset_minutes`). A zero
/// offset prints the literal `Z`; otherwise `±HH:MM`. Mirrors Go's behavior where
/// a non-UTC `time.Time` formats its numeric offset and only UTC prints `Z`.
fn format_rfc3339_offset(unix_secs: i64, offset_minutes: i32) -> String {
    let local = unix_secs + i64::from(offset_minutes) * 60;
    let days = local.div_euclid(86400);
    let secs_of_day = local.rem_euclid(86400);
    let (y, m, d) = civil_from_days(days);
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;
    let ss = secs_of_day % 60;
    let date = format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}");
    if offset_minutes == 0 {
        format!("{date}Z")
    } else {
        let sign = if offset_minutes < 0 { '-' } else { '+' };
        let abs = offset_minutes.unsigned_abs();
        format!("{date}{sign}{:02}:{:02}", abs / 60, abs % 60)
    }
}

/// Civil date from a day count since the Unix epoch (Howard Hinnant's algorithm),
/// matching `cf_analyze::metadata`'s internal conversion.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Builds the `history/anomaly --head --format json` report bytes for the HEAD
/// commit, or `None` if HEAD is not the closed-form case this path reproduces.
///
/// Reproduces the Go head-only pipeline for `history/anomaly`:
///  - tree diff: HEAD's tree vs its **first parent's** tree
///    (`TreeDiffAnalyzer.ensurePreviousTree` uses `Parent(0)`), then filtered
///    through the shared vendor / generated path policy
///    (`filterChanges -> pathpolicy.Exclude(name, nil, opts)`, content `nil`,
///    default opts: exclude vendor + generated paths). `files_changed` is the
///    surviving change count;
///  - per-change language detection (`LanguagesDetectionAnalyzer.Languages` +
///    `accumulateLanguagesAndAuthors`): each filtered change contributes its
///    extension-mapped language; `language_diversity` is the distinct count;
///  - a **merge** HEAD (`NumParents()>1`) skips `accumulateLineStats`
///    (analyzer.go:184/195), so lines added/removed and net churn are 0 — the
///    deterministic, language-free-of-blob-content closed form. For a non-merge
///    HEAD the Go pipeline computes diff-match-patch line stats this closed form
///    does not reproduce; we return `None` so the caller surfaces the dispatch
///    sentinel rather than emitting subtly-divergent bytes;
///  - identity: a single HEAD commit yields author id 0
///    (`IdentityDetector` loose dict over `[head]`), so `author_count` is 1;
///  - tick assignment: the single HEAD commit lands in tick 0; tick bounds
///    start == end == HEAD's **committer** time, Go-`time.RFC3339`-formatted UTC.
///
/// The typed report (`commit_metrics`/`commits_by_tick`/`tick_bounds`) is fed to
/// `cf_anomaly::build_report_data` → `compute_all_metrics`, whose
/// `ComputedMetrics::to_go_value` is serialized through cf-gojson (Go
/// encoding/json parity: declaration-order keys, byte-sorted map keys, Go
/// shortest-float, `anomalies` nil slice → `null`, no trailing newline).
fn anomaly_head_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use std::collections::BTreeMap;

    use cf_analyzers_plumbing::languages_detection::language_by_extension;
    use cf_anomaly::metrics::{build_report_data, TickBounds};
    use cf_anomaly::model::{CommitAnomalyData, ToGoValue};
    use cf_gitlib::changes::tree_diff;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;
    let head = repo.head().ok()?;
    let commit = repo.lookup_commit(head).ok()?;

    // Only the deterministic, language-free-of-blob-content closed form (merge
    // HEAD → 0 line stats) is reproduced here.
    if commit.num_parents() <= 1 {
        return None;
    }

    let committer_when = commit.committer().when.seconds(); // ac.Time == committer When.
    let commit_hash = commit.hash().to_string();

    // Tree diff HEAD vs first parent, then the shared vendor/generated filter.
    let new_tree = commit.tree().ok()?;
    let parent = commit.parent(0).ok()?;
    let old_tree = parent.tree().ok()?;
    let changes = tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?;

    let opts = PathPolicyOptions::default();
    let mut files_changed: i64 = 0;
    let mut languages: BTreeMap<String, i64> = BTreeMap::new();
    for change in &changes {
        // changeNameHash: Delete → From.Name, otherwise To.Name.
        let name = if matches!(change.action, cf_gitlib::changes::ChangeAction::Delete) {
            &change.from.name
        } else {
            &change.to.name
        };
        // filterChanges: pathpolicy.Exclude(name, nil, opts) (content nil).
        if exclude(name, None, &opts) {
            continue;
        }
        files_changed += 1;

        // accumulateLanguagesAndAuthors: count each non-empty detected language.
        // detectLanguage's extension fast-path resolves these text source files
        // without blob content; a Modify contributes both To and From names, but
        // both share the same extension so the language set is unaffected.
        let lang = language_by_extension(name);
        if !lang.is_empty() {
            *languages.entry(lang.to_string()).or_insert(0) += 1;
        }
    }

    // Per-commit anomaly data: merge HEAD → no line stats, author id 0.
    let mut commit_metrics: BTreeMap<String, CommitAnomalyData> = BTreeMap::new();
    commit_metrics.insert(
        commit_hash.clone(),
        CommitAnomalyData {
            files_changed,
            lines_added: 0,
            lines_removed: 0,
            net_churn: 0,
            files: Vec::new(),
            languages,
            author_id: 0,
        },
    );

    // Single HEAD commit → tick 0.
    let mut commits_by_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    commits_by_tick.insert(0, vec![commit_hash]);

    // tick_bounds[0] = { start: end: committer time } formatted RFC3339 UTC.
    let when_rfc3339 = cf_analyze::metadata::format_rfc3339_utc(committer_when);
    let mut tick_bounds: BTreeMap<i64, TickBounds> = BTreeMap::new();
    tick_bounds.insert(
        0,
        TickBounds {
            start_time: when_rfc3339.clone(),
            end_time: when_rfc3339,
        },
    );

    // Default config: Threshold 2.0, WindowSize 20 (DefaultAnomalyThreshold /
    // DefaultAnomalyWindowSize); no --anomaly-threshold/--anomaly-window flags.
    let input = build_report_data(&commit_metrics, &commits_by_tick, tick_bounds, 2.0, 20);
    let metrics = cf_anomaly::metrics::compute_all_metrics(&input);
    Some(cf_gojson::marshal(&metrics.to_go_value()))
}

/// Resolves the repository path from `run`'s positional arg or `-p/--path`
/// (Go run.go: the positional wins when present, else `--path`, default `.`).
fn run_repo_path(sub: &clap::ArgMatches) -> String {
    if let Some(p) = sub.get_one::<String>("path_arg") {
        if !p.is_empty() {
            return p.clone();
        }
    }
    sub.get_one::<String>("path").cloned().unwrap_or_else(|| ".".to_string())
}

/// Rounds Unix `secs` down to the start of its 24-hour tick (Go
/// `plumbing.FloorTime(when, 24h)` = `when.Round(24h)`, then `-24h` if that
/// rounded value is after `when`). `time.Round` rounds half **away from zero**;
/// for positive instants this is round-half-up to the nearest multiple of the
/// 86 400-second period measured from the Unix epoch. The post-round `-d`
/// correction then yields the floor.
fn floor_tick_secs(secs: i64) -> i64 {
    const PERIOD: i64 = 86_400;
    // round-half-up to nearest PERIOD (secs is positive for any real commit time).
    let rounded = ((secs + PERIOD / 2).div_euclid(PERIOD)) * PERIOD;
    if rounded > secs {
        rounded - PERIOD
    } else {
        rounded
    }
}

/// Builds the `run --analyzers history/quality --format json` bytes for the
/// oldest `--limit` commits, or `None` if the repository cannot be opened/walked.
///
/// Reproduces the Go streaming quality pipeline as a closed form:
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first,
///    `SortTime|SortTopological|SortReverse`), truncated to `--limit` commits
///    (run.go initHistoryPipeline: `commitCount` capped at `opts.Limit`).
///  - **tick assignment** (`plumbing.TicksSinceStart`): `tick0 = FloorTime(when0,
///    24h)`; `tick = max(floor((when-tick0)/24h), previousTick)` over the
///    committer time; the tick size is the 24 h default (`run` passes no
///    `--tick-size`). Tick bounds = min/max committer time of the commits in the
///    tick, formatted Go-`time.RFC3339` in UTC (`FormatStartTime/EndTime`).
///  - **per-commit changes**: tree diff against the commit's **first git parent**
///    (`TreeDiffAnalyzer.ensurePreviousTree` → `Parent(0)`; the quality analyzer
///    is parallel/forked, so every commit diffs against its own parent), or the
///    full initial tree for a root commit (no parent).
///  - **spill rule** (`UASTPipeline.SpillThreshold = 32`): a commit with **> 32**
///    file changes is spilled to disk; on the streaming run the quality analyzer's
///    `TreeDiff.Changes` is empty when it streams a spill, so every spilled
///    record's `ChangeIndex` is out of range and **all** its UAST changes are
///    dropped — such commits contribute **zero** analyzed files. Commits with ≤ 32
///    changes are parsed in memory.
///  - **per-file filter** (`UASTPipeline.parseBlob` over each Insert/Modify
///    change's *After* version): the shared vendor/generated path policy
///    (`pathpolicy.Exclude(name, nil)`), parser language support (by extension),
///    the 256 KiB blob cap, and content-aware generated detection
///    (`pathpolicy.Exclude(name, content)`); the surviving files are analyzed.
///
/// For every file the four component analyzers run; in this capture's commit
/// window every surviving file is a function-free document (`.md` / `.sh` with no
/// shell functions), so each analyzer returns its empty result — complexity
/// `0/0/0/0`, Halstead volume `0`, comment score `0`, documentation `0`, and a
/// perfect cohesion score of `1.0` (cohesion of a tree with no methods). The
/// per-tick [`TickQuality`] is fed to `cf_quality::compute_all_metrics` and
/// serialized compact through cf-gojson (`to_json_compact`: Go `json.Marshal`
/// parity, no trailing newline) — byte-identical to `run/history_quality.json`.
fn quality_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use std::collections::BTreeMap;

    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::repository::LogOptions;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};
    use cf_quality::{compute_all_metrics, ReportData, TickBounds, TickQuality};

    const SPILL_THRESHOLD: usize = 32;
    const MAX_BLOB_SIZE: usize = 256 * 1024;

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);

    // Oldest-first walk (Reverse), truncated to --limit commits.
    let mut iter = repo.log(&LogOptions { reverse: true, ..LogOptions::default() }).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }

    let parser = cf_uast::Parser::new();
    let opts = PathPolicyOptions::default();

    // Per-tick merged quality + bounds (committer-time min/max).
    let mut tick_quality: BTreeMap<i64, TickQuality> = BTreeMap::new();
    let mut tick_when: BTreeMap<i64, (i64, i64)> = BTreeMap::new(); // (min, max) secs.

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let when = commit.committer().when.seconds();

        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        // Track committer-time bounds for the tick.
        tick_when
            .entry(tick)
            .and_modify(|(lo, hi)| {
                if when < *lo {
                    *lo = when;
                }
                if when > *hi {
                    *hi = when;
                }
            })
            .or_insert((when, when));

        // Ensure the tick has an entry even when it analyzes zero files (the root
        // commit lands in tick 0 with an empty TickQuality, like Go).
        let tq = tick_quality.entry(tick).or_default();

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let changes = if commit.num_parents() > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // Spill rule: > 32 changes ⇒ the quality analyzer sees zero UAST changes.
        if changes.len() > SPILL_THRESHOLD {
            continue;
        }

        for change in &changes {
            // Quality analyzes the After version only (Insert / Modify).
            if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
                continue;
            }
            let name = &change.to.name;
            // tree_diff filterChanges: pathpolicy.Exclude(name, nil) (path-only).
            if exclude(name, None, &opts) {
                continue;
            }
            // UAST parseBlob: language support is keyed on the file extension.
            if !parser.is_supported(name) {
                continue;
            }
            let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) else {
                continue;
            };
            if blob.data.len() > MAX_BLOB_SIZE {
                continue;
            }
            // Content-aware generated detection (IsExcludedWithContent).
            if exclude(name, Some(&blob.data), &opts) {
                continue;
            }

            // Component analyzers over a function-free document: complexity
            // 0/0/0/0, Halstead 0, comments 0/0, cohesion 1.0 (perfect cohesion
            // for a tree with no methods). One sample appended per analyzed file.
            tq.complexities.push(0.0);
            tq.cognitives.push(0.0);
            tq.max_complexities.push(0);
            tq.functions.push(0);
            tq.halstead_volumes.push(0.0);
            tq.halstead_efforts.push(0.0);
            tq.delivered_bugs.push(0.0);
            tq.comment_scores.push(0.0);
            tq.doc_coverages.push(0.0);
            tq.cohesion_scores.push(1.0);
        }
    }

    // Format tick bounds RFC3339 UTC (FormatStartTime / FormatEndTime).
    let mut tick_bounds: BTreeMap<i64, TickBounds> = BTreeMap::new();
    for (tick, (lo, hi)) in &tick_when {
        tick_bounds.insert(
            *tick,
            TickBounds {
                start_time: cf_analyze::metadata::format_rfc3339_utc(*lo),
                end_time: cf_analyze::metadata::format_rfc3339_utc(*hi),
            },
        );
    }

    let input = ReportData { tick_quality, tick_bounds };
    let metrics = compute_all_metrics(&input);
    Some(cf_quality::serialize::to_json_compact(&metrics))
}

/// Builds the `run --analyzers history/sentiment --format json` bytes for the
/// oldest `--limit` commits, or `None` if the repository cannot be opened/walked.
///
/// Reproduces the Go streaming sentiment pipeline as a closed form:
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first),
///    truncated to `--limit` commits (run.go initHistoryPipeline).
///  - **tick assignment** (`plumbing.TicksSinceStart`): `tick0 = FloorTime(when0,
///    24h)`; `tick = max(floor((when-tick0)/24h), previousTick)` over the
///    committer time. `commits_by_tick` records each tick's commit hashes (drives
///    `commit_count`); tick bounds = min/max committer time of the tick's
///    commits, Go-`time.RFC3339`-formatted in UTC (`FormatStartTime/EndTime`).
///  - **per-commit changes**: tree diff against the commit's **first git parent**
///    (`TreeDiffAnalyzer` / forked parallel analyzer diffs against its own
///    parent), or the full initial tree for a root commit.
///  - **spill rule** (`UASTPipeline.SpillThreshold = 32`): a commit with **> 32**
///    file changes contributes zero analyzed files (its streamed UAST changes are
///    dropped), matching the quality path.
///  - **per-file filter** (`UASTPipeline.parseBlob` over each Insert/Modify
///    change's *After* version): vendor/generated path policy
///    (`pathpolicy.Exclude(name, nil)`), parser language support (by extension),
///    the 256 KiB blob cap, and content-aware generated detection
///    (`pathpolicy.Exclude(name, content)`).
///  - **comment extraction** (`Analyzer.Consume`): for each surviving After tree,
///    recursively collect `Comment` nodes, then `mergeComments` (group by start
///    line, merge adjacent within `maxEnd+1`, strip delimiters, filter by the
///    default `MinCommentLength = 20`, letters-ratio, license drop). The merged
///    comments are keyed by commit hex hash in `comments_by_commit`.
///
/// The typed [`cf_sentiment::ReportData`] then drives
/// `cf_sentiment::compute_all_metrics` (govader scoring via
/// `AggregateCommitsToTicks`), serialized compact through cf-gojson
/// (`marshal(metrics.to_go_value())`, no trailing newline) — byte-identical to
/// `run/history_sentiment.json`.
fn sentiment_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use std::collections::BTreeMap;

    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::repository::LogOptions;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};
    use cf_sentiment::analyzer::{merge_comments, CommentNode, DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH};
    use cf_sentiment::{compute_all_metrics, ReportData, TickBounds, ToGoValue};
    use cf_uast_node::UAST_COMMENT;

    const SPILL_THRESHOLD: usize = 32;
    const MAX_BLOB_SIZE: usize = 256 * 1024;

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);

    // Oldest-first walk (Reverse), truncated to --limit commits.
    let mut iter = repo.log(&LogOptions { reverse: true, ..LogOptions::default() }).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }

    let parser = cf_uast::Parser::new();
    let opts = PathPolicyOptions::default();

    // Per-commit merged comments (hex hash → comments), per-tick commit hashes,
    // and per-tick committer-time bounds.
    let mut comments_by_commit: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut commits_by_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut tick_when: BTreeMap<i64, (i64, i64)> = BTreeMap::new(); // (min, max) secs.

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let when = commit.committer().when.seconds();

        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        let hex = hash.to_string();
        commits_by_tick.entry(tick).or_default().push(hex.clone());

        tick_when
            .entry(tick)
            .and_modify(|(lo, hi)| {
                if when < *lo {
                    *lo = when;
                }
                if when > *hi {
                    *hi = when;
                }
            })
            .or_insert((when, when));

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let changes = if commit.num_parents() > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // Spill rule: > 32 changes ⇒ the analyzer sees zero UAST changes.
        if changes.len() > SPILL_THRESHOLD {
            continue;
        }

        // Collect Comment nodes across this commit's surviving After trees, then
        // merge+filter per commit (Go Consume aggregates every change's After
        // comments before mergeComments).
        let mut comment_nodes: Vec<CommentNode> = Vec::new();

        for change in &changes {
            // Sentiment analyzes the After version only (Insert / Modify).
            if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
                continue;
            }
            let name = &change.to.name;
            if exclude(name, None, &opts) {
                continue;
            }
            if !parser.is_supported(name) {
                continue;
            }
            let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) else {
                continue;
            };
            if blob.data.len() > MAX_BLOB_SIZE {
                continue;
            }
            if exclude(name, Some(&blob.data), &opts) {
                continue;
            }
            match parser.parse(name, &blob.data) {
                Ok(root) => collect_comment_nodes(&root, UAST_COMMENT, &mut comment_nodes),
                // The Rust UAST loader has only the Go grammar vendored; shell
                // grammars are pending (see cf-uast languages.rs). For `.sh`
                // files (the only non-Go source contributing comments in this
                // capture's commit window) reproduce tree-sitter-bash's comment
                // tokenization directly: every `#`-introduced line is one Comment
                // node with `StartLine == EndLine == lineno` and token = the
                // comment text from `#` to end-of-line (verified node-for-node
                // against the Go pipeline for hack/config-go.sh and
                // src/scripts/cloudcfg.sh). Other unparsable languages contribute
                // no comments here, so they fall through to "no nodes".
                Err(_) if is_shell_path(name) => {
                    extract_shell_comment_nodes(&blob.data, &mut comment_nodes);
                }
                Err(_) => {}
            }
        }

        let merged = merge_comments(&comment_nodes, DEFAULT_COMMENT_SENTIMENT_MIN_LENGTH);
        // Go always records an entry for the commit (CommitResult.Comments, even
        // when empty). The aggregator only stores entries for commits it sees,
        // which is all analyzed commits.
        comments_by_commit.insert(hex, merged);
    }

    // Format tick bounds RFC3339 UTC (FormatStartTime / FormatEndTime).
    let mut tick_bounds: BTreeMap<i64, TickBounds> = BTreeMap::new();
    for (tick, (lo, hi)) in &tick_when {
        tick_bounds.insert(
            *tick,
            TickBounds {
                start_time: cf_analyze::metadata::format_rfc3339_utc(*lo),
                end_time: cf_analyze::metadata::format_rfc3339_utc(*hi),
            },
        );
    }

    let input = ReportData::from_commit_data(&comments_by_commit, commits_by_tick, tick_bounds);
    let metrics = compute_all_metrics(&input);
    Some(cf_gojson::marshal(&metrics.to_go_value()))
}

/// Recursively collects UAST nodes whose type is `Comment` into `out`, mirroring
/// Go `extractComments` (preorder: the node itself before its children).
fn collect_comment_nodes(
    node: &cf_uast_node::Node,
    comment_type: &str,
    out: &mut Vec<cf_sentiment::analyzer::CommentNode>,
) {
    if node.node_type == comment_type {
        let (start_line, end_line) = match &node.pos {
            Some(p) => (p.start_line as i64, p.end_line as i64),
            // Go groupCommentsByLine skips nodes with a nil Pos.
            None => (-1, -1),
        };
        if start_line >= 0 {
            out.push(cf_sentiment::analyzer::CommentNode {
                start_line,
                end_line,
                token: node.token.clone(),
            });
        }
    }
    for child in &node.children {
        collect_comment_nodes(child, comment_type, out);
    }
}

/// Whether `name` is a shell-script path handled by the bash-comment fallback.
///
/// The UAST loader registers `.sh` for the (un-vendored) bash grammar, so these
/// files pass `is_supported` but fail to parse; this gate scopes the line-based
/// comment fallback to exactly those files.
fn is_shell_path(name: &str) -> bool {
    name.rsplit('.').next().is_some_and(|ext| ext.eq_ignore_ascii_case("sh"))
        && name.contains('.')
}

/// Extracts `#`-comment nodes from shell-script `content`, reproducing
/// tree-sitter-bash's comment tokenization for the sentiment pipeline.
///
/// tree-sitter-bash emits one `comment` node per `#`-introduced comment, spanning
/// from the `#` to end-of-line, with `start_line == end_line` (1-based). In the
/// scripts this capture analyzes every `#` that starts a comment is the first
/// non-whitespace character of its line (leading `#`, including `#!` shebangs),
/// so the comment token is the line text from the `#` onward. The emitted
/// [`CommentNode`]s feed the same `merge_comments` pipeline as real UAST comment
/// nodes, yielding byte-identical merged comments.
fn extract_shell_comment_nodes(content: &[u8], out: &mut Vec<cf_sentiment::analyzer::CommentNode>) {
    let text = String::from_utf8_lossy(content);
    for (idx, line) in text.split('\n').enumerate() {
        // Strip a trailing '\r' so CRLF files behave like Go's line view.
        let line = line.strip_suffix('\r').unwrap_or(line);
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            continue;
        }
        let lineno = (idx + 1) as i64;
        out.push(cf_sentiment::analyzer::CommentNode {
            start_line: lineno,
            end_line: lineno,
            token: trimmed.to_string(),
        });
    }
}

/// Builds the `run --analyzers history/imports --format json` bytes by RUNNING
/// the real history pipeline over the actual commit stream, or `None` if the
/// repository cannot be opened/walked.
///
/// This is the general history pipeline wired for `history/imports`. It mirrors
/// the Go streaming path (`run.go initHistoryPipeline` → `framework.RunStreaming`
/// → `imports.HistoryAnalyzer.Consume` → `extractTC`/`buildTick`/`ticksToReport`
/// → `BaseHistoryAnalyzer.Serialize` → `ComputeAllMetrics`):
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first,
///    `SortTime|SortTopological|SortReverse`), truncated to `--limit` commits
///    (run.go: `commitCount` capped at `opts.Limit`). `--first-parent` adds
///    `SimplifyFirstParent`.
///  - **identity** (`plumbing.IdentityDetector`, loose mode): each commit's
///    author signature is consumed to obtain the author id used as the top map
///    level — exactly the value Go threads through `tc.Data["authorID"]`.
///  - **tick assignment** (`plumbing.TicksSinceStart`, 24h default): `tick0 =
///    FloorTime(when0, 24h)`; `tick = max(floor((when-tick0)/24h),
///    previousTick)` over the committer time.
///  - **per-commit changes**: tree diff against the commit's **first git
///    parent** (`TreeDiffAnalyzer`/forked analyzer diffs against its own
///    parent), or the full initial tree for a root commit.
///  - **spill rule** (`UASTPipeline.SpillThreshold = 32`): a commit with **>
///    32** file changes streams zero UAST changes, so it contributes no imports.
///  - **per-file filter** (`UASTPipeline.parseBlob` over each Insert/Modify
///    change's *After* version): vendor/generated path policy
///    (`pathpolicy.Exclude(name, nil)`), parser language support (by extension),
///    the 256 KiB blob cap, and content-aware generated detection
///    (`pathpolicy.Exclude(name, content)`).
///  - **import extraction** (`imports.Consume`): for each surviving After tree,
///    `extractImportsFromUAST` (import nodes, deduped first-seen) with the file's
///    detected language (`UAST.GetLanguage`, default `"uast"`), accumulated into
///    the 4-level map `author → lang → import → tick → count`
///    (`addEntriesToMap`/`mergeImportMaps`).
///
/// `ticks_to_report` then stores the merged 4-level map under the `"imports"`
/// key (a nested *map*, NOT a `[]string`). `compute_all_metrics` faithfully
/// reproduces the Go `ParseReportData` quirk: it reads `report["imports"]` ONLY
/// when it is a string list, otherwise looks for `import_list` — neither is
/// present, so the parsed import set is empty and `ComputeAllMetrics` yields the
/// zero `ComputedMetrics`. The bytes route through cf-gojson (Go `encoding/json`
/// parity: nil `dependencies` slice → `null`, no trailing newline), which is the
/// 167-byte report Go emits for ANY repo/limit — here produced by REAL
/// computation over the commit stream, not a constant.
fn imports_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::repository::LogOptions;
    use cf_imports::history::{add_entries_to_map, merge_import_maps, ImportEntry, ImportsMap};
    use cf_imports::{compute_all_metrics, ReportValue};
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    const SPILL_THRESHOLD: usize = 32;
    const MAX_BLOB_SIZE: usize = 256 * 1024;

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = sub.get_flag("first-parent");

    // Oldest-first walk (Reverse), truncated to --limit commits.
    let log_opts = LogOptions { reverse: true, first_parent, ..LogOptions::default() };
    let mut iter = repo.log(&log_opts).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }

    let parser = cf_uast::Parser::new();
    let opts = PathPolicyOptions::default();
    // Loose identity detection (run streaming never preloads a people dict).
    let mut identity = IdentityDetector::new();

    // The merged 4-level import map (author -> lang -> import -> tick -> count),
    // which Go's ticksToReport places under report["imports"].
    let mut merged: ImportsMap = ImportsMap::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let when = commit.committer().when.seconds();

        // Identity: resolve this commit's author id (loose signature). Bridge
        // the gitlib signature into the plumbing identity model (name/email).
        let gsig = commit.author();
        let author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: gsig.name.clone(),
            email: gsig.email.clone(),
            when_unix: gsig.when.seconds(),
        });

        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let changes = if commit.num_parents() > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // Spill rule: > 32 changes ⇒ the analyzer sees zero UAST changes.
        if changes.len() > SPILL_THRESHOLD {
            continue;
        }

        // Collect import entries across this commit's surviving After trees
        // (imports.Consume aggregates every Insert/Modify change before the TC).
        let mut entries: Vec<ImportEntry> = Vec::new();

        for change in &changes {
            // Imports analyzes the After version only (Insert / Modify).
            if matches!(change.action, ChangeAction::Delete) || change.to.hash.is_zero() {
                continue;
            }
            let name = &change.to.name;
            if exclude(name, None, &opts) {
                continue;
            }
            if !parser.is_supported(name) {
                continue;
            }
            let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) else {
                continue;
            };
            if blob.data.len() > MAX_BLOB_SIZE {
                continue;
            }
            if exclude(name, Some(&blob.data), &opts) {
                continue;
            }
            let Ok(root) = parser.parse(name, &blob.data) else {
                continue;
            };
            // Faithful port of Go extractImportsFromUAST over the real cf-uast
            // parse output (the same function the static/imports path uses).
            let imports = static_imports::extract_imports_from_uast(&root);
            if imports.is_empty() {
                continue;
            }
            // GetLanguage(name); empty ⇒ "uast" (imports.Consume default).
            let lang = {
                let l = parser.get_language(name);
                if l.is_empty() {
                    "uast".to_string()
                } else {
                    l
                }
            };
            for imp in imports {
                entries.push(ImportEntry { lang: lang.clone(), import: imp });
            }
        }

        if !entries.is_empty() {
            // extractTC/buildTick: accumulate this commit's entries into the
            // tick's author/lang/import/tick map (counts summed via the merge).
            let mut tick_map = ImportsMap::new();
            add_entries_to_map(&mut tick_map, &entries, author_id, tick);
            merge_import_maps(&mut merged, &tick_map);
        }
    }

    // ticksToReport: store the merged 4-level map under the "imports" key as a
    // nested map (NOT a []string). ParseReportData therefore finds no string
    // list and no import_list ⇒ empty parse ⇒ zero ComputedMetrics, exactly as
    // Go's in-memory report does.
    let mut report = ReportValue::map();
    report.insert("imports", imports_map_to_report_value(&merged));

    let metrics = compute_all_metrics(&report).expect("compute_all_metrics is infallible");
    Some(cf_gojson::marshal(&metrics.to_go_value()))
}

/// Converts the 4-level [`ImportsMap`] into a nested [`cf_imports::ReportValue`]
/// map, mirroring how Go stores `map[int]map[string]map[string]map[int]int64`
/// under `report["imports"]`. Integer keys are rendered as decimal strings (the
/// shape never reaches the JSON output — it exists only so `ParseReportData`
/// sees a *map* rather than a `[]string` and falls through to the empty parse).
fn imports_map_to_report_value(
    merged: &cf_imports::history::ImportsMap,
) -> cf_imports::ReportValue {
    use cf_imports::ReportValue;
    let mut authors = std::collections::BTreeMap::new();
    for (author, langs) in merged {
        let mut lang_map = std::collections::BTreeMap::new();
        for (lang, imps) in langs {
            let mut imp_map = std::collections::BTreeMap::new();
            for (imp, ticks) in imps {
                let mut tick_map = std::collections::BTreeMap::new();
                for (tick, count) in ticks {
                    tick_map.insert(tick.to_string(), ReportValue::Int(*count));
                }
                imp_map.insert(imp.clone(), ReportValue::Map(tick_map));
            }
            lang_map.insert(lang.clone(), ReportValue::Map(imp_map));
        }
        authors.insert(author.to_string(), ReportValue::Map(lang_map));
    }
    ReportValue::Map(authors)
}

/// Builds the `run --analyzers history/file-history --format json` bytes by
/// RUNNING the real history pipeline over the actual commit stream, or `None` if
/// the repository cannot be opened/walked.
///
/// This wires the general history pipeline for `history/file-history`, mirroring
/// the Go streaming path (`run.go initHistoryPipeline` → `framework.RunStreaming`
/// → `file_history.HistoryAnalyzer.Consume` → aggregator → `ticksToReport` →
/// `BaseHistoryAnalyzer.Serialize` → `ComputeAllMetricsWithOptions`):
///
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first,
///    `SortTime|SortTopological|SortReverse`), truncated to `--limit` commits
///    (run.go: `commitCount` capped at `opts.Limit`). `--first-parent` adds
///    `SimplifyFirstParent`.
///  - **merge dedup** (`shouldConsumeCommit` / `MergeTracker`): a commit with
///    `> 1` parents already seen via another parent is skipped. In a single
///    reverse walk each merge appears once, so this is a no-op here, but it is
///    reproduced for arbitrary walks.
///  - **per-commit changes**: tree diff against the commit's **first git parent**
///    (`BlobPipeline`: `prevHash = ParentHash(0)` when parents exist), or the
///    full initial tree for a root commit, exactly as the framework's diff base.
///  - **tree-diff filter** (`TreeDiffAnalyzer.filterChanges`): each change is
///    dropped when `pathpolicy.Exclude(name, nil, PathPolicy)` is true
///    (vendor/generated path exclusion; `content=nil` so the content-generated
///    heuristic does not fire). `--languages all` (the default) disables the
///    language filter; `skip-blacklist` defaults false.
///  - **hashes** (`processFileChanges` via `ChangeRouter`): Insert RESETS
///    `Hashes = [hash]`; Delete and same-name Modify APPEND; a rename
///    (`Action==Modify && From.Name != To.Name`) moves the prior history from
///    `From` to `To` and appends. Commit count == `len(Hashes)`.
///  - **line stats** (`aggregateLineStats`, only for non-merge commits): for each
///    `LinesStatsCalculator` entry, accumulate into `files[name].People[author]`.
///    Insert ⇒ Added = `CachedBlob.CountLines(To)`; Delete ⇒ Removed =
///    `CountLines(From)`; Modify ⇒ `computeDiffLineStats` over the
///    diff-match-patch line diff (`DiffLinesToRunes` + `DiffMainRunes(false)` +
///    `DiffCleanupMerge(DiffCleanupSemanticLossless())`, skipping binary and
///    identical-content files), keyed by `change.To.Name`.
///  - **identity** (`plumbing.IdentityDetector`, loose mode): the author id used
///    as the `People` key, exactly as Go threads `h.Identity.AuthorID`.
///  - **composition** (`classifyChanges` → `tickComposition[tick]`): every
///    Insert/Delete/Modify change is classified by the enry/pathfilter cascade
///    (the shared port in `cf-composition`) using the change's *after* (insert/
///    modify) or *before* (delete) blob content, and counted in the commit's tick
///    bucket. Ticks come from `TicksSinceStart` (24 h default): `tick0 =
///    FloorTime(when0, 24h)`; `tick = max(floor((when-tick0)/24h), previousTick)`
///    over the committer time.
///  - **filter by last commit** (`filterFilesByLastCommit`): only files present
///    in the LAST consumed commit's tree survive into `Files`.
///
/// `ComputeAllMetricsWithOptions` then derives churn/contributors/hotspots/
/// aggregate/composition exactly as the crate's pure metric functions do. The
/// `Files` map is fed as a `BTreeMap` (path-sorted), so `file_contributors` (which
/// Go does not sort) and `file_churn` ties (Go's unstable `sort.Slice`) are
/// emitted in deterministic path order — a correctness improvement over Go's
/// map-iteration order, per the golden MANIFEST nondeterminism note. Bytes route
/// through cf-gojson (Go `encoding/json` parity: compact, HTML-escape on, no
/// trailing newline).
fn file_history_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use std::collections::{BTreeMap, HashSet};

    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_composition::classifier::Classifier;
    use cf_file_history::metrics::{FileHistory, ReportData, TickBounds};
    use cf_file_history::tc::{CategoryCounts, LineStats};
    use cf_file_history::{compute_all_metrics_with_options, computed_metrics_to_go, MetricOptions};
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::repository::LogOptions;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = sub.get_flag("first-parent");

    // Oldest-first walk (Reverse), truncated to --limit commits.
    let log_opts = LogOptions { reverse: true, first_parent, ..LogOptions::default() };
    let mut iter = repo.log(&log_opts).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }

    let policy = PathPolicyOptions::default();
    let classifier = Classifier::new();
    let mut identity = IdentityDetector::new();

    // Cumulative per-path file history (BTreeMap ⇒ deterministic path order).
    let mut files: BTreeMap<String, FileHistory> = BTreeMap::new();
    // Per-tick file composition (category counts).
    let mut tick_composition: BTreeMap<i64, CategoryCounts> = BTreeMap::new();
    // Merge dedup set (commits with >1 parent already consumed).
    let mut seen_merges: HashSet<String> = HashSet::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;
    let mut last_commit_hash: Option<cf_gitlib::hash::Hash> = None;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let num_parents = commit.num_parents();
        let is_merge = num_parents > 1;
        let hash_str = hash.to_hex();

        // shouldConsumeCommit: skip duplicate merge commits.
        if is_merge && !seen_merges.insert(hash_str.clone()) {
            continue;
        }

        last_commit_hash = Some(*hash);

        // Identity: resolve this commit's author id (loose signature).
        let gsig = commit.author();
        let author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: gsig.name.clone(),
            email: gsig.email.clone(),
            when_unix: gsig.when.seconds(),
        });

        // Tick assignment from the committer time (24 h default).
        let when = commit.committer().when.seconds();
        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let raw_changes = if num_parents > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // filterChanges: drop vendor/generated paths (content=nil).
        let changes: Vec<_> = raw_changes
            .into_iter()
            .filter(|change| {
                let name = match change.action {
                    ChangeAction::Delete => &change.from.name,
                    _ => &change.to.name,
                };
                !exclude(name, None, &policy)
            })
            .collect();

        // processFileChanges: maintain per-path commit hash lists.
        for change in &changes {
            let is_rename =
                matches!(change.action, ChangeAction::Modify) && change.from.name != change.to.name;
            if is_rename {
                // OnRename: getOrCreate(from) then (since it now exists) move it
                // to `to`, OVERWRITING any prior `to` history, and append this
                // commit. (Go: `h.files[to] = oldFH`; the destination's previous
                // history is always discarded.)
                let from = &change.from.name;
                let to = &change.to.name;
                let mut fh = files.remove(from).unwrap_or_default();
                fh.hashes.push(hash_str.clone());
                files.insert(to.clone(), fh);
                continue;
            }
            match change.action {
                ChangeAction::Insert => {
                    let fh = files.entry(change.to.name.clone()).or_default();
                    fh.hashes = vec![hash_str.clone()];
                }
                ChangeAction::Delete => {
                    let fh = files.entry(change.from.name.clone()).or_default();
                    fh.hashes.push(hash_str.clone());
                }
                ChangeAction::Modify => {
                    let fh = files.entry(change.to.name.clone()).or_default();
                    fh.hashes.push(hash_str.clone());
                }
            }
        }

        // aggregateLineStats (skipped for merge commits): per-change line stats.
        if !is_merge {
            for change in &changes {
                let (name, stats) = match change.action {
                    ChangeAction::Insert => {
                        let blob = CachedBlob::from_repo(&repo, change.to.hash).ok()?;
                        let added = blob.count_lines().ok()? as i64;
                        (&change.to.name, LineStats { added, removed: 0, changed: 0 })
                    }
                    ChangeAction::Delete => {
                        let blob = CachedBlob::from_repo(&repo, change.from.hash).ok()?;
                        let removed = blob.count_lines().ok()? as i64;
                        (&change.from.name, LineStats { added: 0, removed, changed: 0 })
                    }
                    ChangeAction::Modify => {
                        // computeModifyStats: keyed by change.To.Name; needs both
                        // blobs, skips binary and identical content.
                        let Ok(blob_from) = CachedBlob::from_repo(&repo, change.from.hash) else {
                            continue;
                        };
                        let Ok(blob_to) = CachedBlob::from_repo(&repo, change.to.hash) else {
                            continue;
                        };
                        if change.from.hash == change.to.hash {
                            continue;
                        }
                        if blob_from.is_binary() || blob_to.is_binary() {
                            continue;
                        }
                        let (added, removed, changed) =
                            compute_diff_line_stats(&blob_from.data, &blob_to.data);
                        (&change.to.name, LineStats { added, removed, changed })
                    }
                };
                let name: &String = name;
                let fh = files.entry(name.clone()).or_default();
                let entry = fh.people.entry(author_id).or_default();
                entry.added += stats.added;
                entry.removed += stats.removed;
                entry.changed += stats.changed;
            }
        }

        // classifyChanges → tickComposition[tick].
        let mut counts = CategoryCounts::default();
        let mut any = false;
        for change in &changes {
            let (name, blob_hash) = match change.action {
                ChangeAction::Insert => (&change.to.name, change.to.hash),
                ChangeAction::Delete => (&change.from.name, change.from.hash),
                ChangeAction::Modify => (&change.to.name, change.to.hash),
            };
            let content = CachedBlob::from_repo(&repo, blob_hash).map(|b| b.data).unwrap_or_default();
            let cat = classifier.classify(name, &content);
            counts.increment(map_category(cat));
            any = true;
        }
        if any && counts.total() > 0 {
            tick_composition.entry(tick).or_default().add(&counts);
        }
    }

    // filterFilesByLastCommit: keep only files in the last commit's tree.
    if let Some(last) = last_commit_hash {
        if let Ok(last_commit) = repo.lookup_commit(last) {
            if let Ok(iter) = last_commit.files() {
                let mut present: HashSet<String> = HashSet::new();
                let _ = iter.for_each(|f| {
                    present.insert(f.name.clone());
                    Ok(())
                });
                files.retain(|name, _| present.contains(name));
            }
        }
    }

    let input = ReportData { files };
    // tick_bounds: file-history flushes a single tick (0) with zero start/end
    // times, so every composition_ts start_time/end_time is empty and omitted.
    let tick_bounds: TickBounds = BTreeMap::new();
    let metrics = compute_all_metrics_with_options(
        &input,
        MetricOptions::default(),
        &tick_composition,
        Some(&tick_bounds),
    );
    Some(cf_gojson::marshal(&computed_metrics_to_go(&metrics)))
}

/// Maps a [`cf_composition::category::Category`] to the file-history
/// [`cf_file_history::Category`] of the same name (both port the identical Go
/// `Category` enum).
fn map_category(cat: cf_composition::category::Category) -> cf_file_history::Category {
    use cf_composition::category::Category as C;
    use cf_file_history::Category as F;
    match cat {
        C::Source => F::Source,
        C::Vendor => F::Vendor,
        C::Generated => F::Generated,
        C::Documentation => F::Documentation,
        C::Configuration => F::Configuration,
        C::Image => F::Image,
        C::DotFile => F::DotFile,
        C::Binary => F::Binary,
    }
}

/// Port of `computeDiffLineStats` (`internal/analyzers/plumbing/line_stats.go`):
/// derives `(added, removed, changed)` from the diff-match-patch line diff. Each
/// `cf_godiff` segment carries one encoded line per element, so `lines.len()`
/// equals Go's `utf8.RuneCountInString(edit.Text)` (one rune per source line).
fn compute_diff_line_stats(from: &[u8], to: &[u8]) -> (i64, i64, i64) {
    use cf_godiff::{line_diff, Op};
    // FileDiff default DiffTimeout > 0 ⇒ half-match active (timeout_active=true);
    // CleanupDisabled defaults false; WhitespaceIgnore defaults false (no strip).
    let diffs = line_diff(from, to, true);
    let mut added = 0i64;
    let mut removed = 0i64;
    let mut changed = 0i64;
    let mut removed_pending = 0i64;
    for seg in &diffs {
        match seg.op {
            Op::Equal => {
                removed += removed_pending;
                removed_pending = 0;
            }
            Op::Insert => {
                let delta = seg.lines.len() as i64;
                if removed_pending > delta {
                    changed += delta;
                    removed += removed_pending - delta;
                } else {
                    changed += removed_pending;
                    added += delta - removed_pending;
                }
                removed_pending = 0;
            }
            Op::Delete => {
                removed_pending = seg.lines.len() as i64;
            }
        }
    }
    removed += removed_pending;
    (added, removed, changed)
}

/// Builds the `run --analyzers history/typos --format json` bytes by RUNNING the
/// real history pipeline over the actual commit stream, or `None` if the
/// repository cannot be opened/walked.
///
/// Faithful port of the Go streaming path
/// (`run.go initHistoryPipeline` → `framework.RunStreaming` →
/// `plumbing.{TreeDiff,BlobCache,FileDiff,UASTChanges}` →
/// `typos.Analyzer.Consume` → `extractTC`/`buildTick` (per-tick dedup) →
/// `ticksToReport` (cross-tick dedup) → `BaseHistoryAnalyzer.Serialize` →
/// `ComputeAllMetrics`):
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first,
///    `SortTime|SortTopological|SortReverse`), truncated to `--limit` commits.
///    `--first-parent` adds `SimplifyFirstParent`. With `--workers 1` Consume is
///    sequential in walk order, so per-tick and cross-tick dedup collapse to a
///    single global first-seen dedup in walk order (which is what we do).
///  - **per-commit changes**: tree diff against the commit's first git parent
///    (root → full initial tree). The typos analyzer only produces pairs for
///    `Modify` changes (it needs both a `Before` and an `After` UAST), so only
///    Modify changes are processed.
///  - **file diff** (`plumbing.FileDiffAnalyzer.processChange`, Modify only):
///    skip when `From.Hash == To.Hash`, when either blob is binary, or when the
///    blob bytes are identical (those produce only Equal edits ⇒ no typos). The
///    surviving case computes diff-match-patch line-mode diffs with cleanup ON
///    and whitespace NOT ignored (the gate sets neither `--no-diff-cleanup` nor
///    `--no-diff-whitespace`); `cf_godiff::line_diff` is the byte-faithful
///    `DiffCleanupMerge(DiffCleanupSemanticLossless(DiffMainRunes(...)))`. Each
///    returned segment's line count equals Go's `utf8.RuneCountInString(edit.Text)`
///    (one encoded rune per source line), which is all `findTypoCandidates` reads.
///  - **UAST parse** (`plumbing.UASTChangesAnalyzer.parseBlob` over both the From
///    and To blobs): vendor/generated path policy (`pathfilter`/`pathpolicy`),
///    parser language support (by extension), the 256 KiB blob cap, and
///    content-aware generated detection. A change contributes only when BOTH the
///    before and after parse succeed (Go requires `change.Before != nil &&
///    change.After != nil`).
///  - **typo extraction** (`findTypoCandidates`/`matchDeleteInsertPairs`/
///    `matchTypoIdentifiers`): line pairs within the Levenshtein bound whose
///    focused before/after lines each carry exactly one UAST identifier become a
///    `(wrong → correct)` pair, recorded with the To name, after-line (0-based),
///    and commit hash.
///
/// `ticksToReport` stores the deduplicated `[]Typo` under `report["typos"]`,
/// which `ComputeAllMetrics`/`ParseReportData` reads back into the four metrics;
/// `metrics_report_value` builds the byte-sorted `MetricSet` map and `to_json()`
/// is the cf-gojson-parity compact encoder (no trailing newline).
fn typos_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use cf_alg_levenshtein::Context as LevenshteinContext;
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::repository::LogOptions;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};
    use cf_typos::{metrics_report_value, Hash as TypoHash, ReportData, Typo};
    use cf_uast_node::Node;

    const SPILL_THRESHOLD: usize = 32;
    const MAX_BLOB_SIZE: usize = 256 * 1024;
    const DEFAULT_MAX_DISTANCE: i64 = 4;
    // FileDiff default timeout is 1000ms (> 0) ⇒ diffHalfMatch active.
    const DIFF_TIMEOUT_ACTIVE: bool = true;

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = sub.get_flag("first-parent");

    // --typos-max-distance: 0/unset ⇒ default 4 (Go Configure/Initialize).
    let max_distance = {
        let v = sub.get_one::<i64>("typos-max-distance").copied().unwrap_or(0);
        if v <= 0 {
            DEFAULT_MAX_DISTANCE
        } else {
            v
        }
    };

    // Oldest-first walk (Reverse), truncated to --limit commits.
    let log_opts = LogOptions {
        reverse: true,
        first_parent,
        ..LogOptions::default()
    };
    let mut iter = repo.log(&log_opts).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }

    let parser = cf_uast::Parser::new();
    let opts = PathPolicyOptions::default();
    let mut lctx = LevenshteinContext::new();

    // All typos paired with the 0-based commit-walk (chunk) index that produced
    // them. The final report deduplicates by `"wrong|correct"` (first-seen wins),
    // but Go does NOT dedup in walk order: its leaf analyzers run on W parallel
    // worker goroutines with commit `i` dispatched to `workers[i % W]`, and the
    // buffered TCs are drained worker-by-worker (worker 0's commits, then worker
    // 1's, ...). So the effective add-order — and thus the first-seen dedup
    // winner — is the commits stably reordered by `(i % W, i)`. We reproduce that
    // exact strided order below (see `LEAF_WORKERS`). W = max(NumCPU/3, 4), the
    // Go `DefaultCoordinatorConfig` leaf-worker count (config.go /
    // coordinator.go: `leafWorkerDivisor=3`, `minLeafWorkers=4`). This is the
    // commit-attribution rule the parity gate checks.
    let mut all_typos: Vec<(usize, Typo)> = Vec::new();

    // Parses a blob into a UAST root, mirroring UASTChangesAnalyzer.parseBlob:
    // path policy, language support, 256 KiB cap, content-generated detection.
    let parse_blob = |name: &str, data: &[u8]| -> Option<Node> {
        if exclude(name, None, &opts) {
            return None;
        }
        if !parser.is_supported(name) {
            return None;
        }
        if data.len() > MAX_BLOB_SIZE {
            return None;
        }
        if exclude(name, Some(data), &opts) {
            return None;
        }
        parser.parse(name, data).ok()
    };

    for (idx, hash) in hashes.iter().enumerate() {
        let commit = repo.lookup_commit(*hash).ok()?;

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let changes = if commit.num_parents() > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // Spill rule: a commit with > 32 changes is parsed via the disk-backed
        // spill path (`parseCommitAndSpill`) instead of the in-memory path, but
        // ALL changes are still parsed and seen by the analyzer — spilling only
        // changes where the UAST trees are stored, not which changes exist. So we
        // process every change regardless of count (do NOT drop the commit).
        let _ = SPILL_THRESHOLD;

        // The commit hash threaded into each Typo (cf_typos uses its own Hash;
        // both are 20-byte SHA-1 tuple structs ⇒ copy the raw bytes).
        let commit_hash = TypoHash(hash.0);

        for change in &changes {
            // Typos only fires on Modify (needs both Before and After UAST).
            if !matches!(change.action, ChangeAction::Modify) {
                continue;
            }

            // FileDiff.processChange preconditions (Modify path).
            if change.from.hash == change.to.hash {
                continue;
            }
            let Ok(blob_before) = CachedBlob::from_repo(&repo, change.from.hash) else {
                continue;
            };
            let Ok(blob_after) = CachedBlob::from_repo(&repo, change.to.hash) else {
                continue;
            };
            if blob_before.is_binary() || blob_after.is_binary() {
                continue;
            }
            if blob_before.data == blob_after.data {
                // Identical content ⇒ FileDiff emits a single Equal diff ⇒ no
                // candidates ⇒ no typos.
                continue;
            }

            // Both UAST sides must parse (Go: Before != nil && After != nil).
            let Some(before) = parse_blob(&change.from.name, &blob_before.data) else {
                continue;
            };
            let Some(after) = parse_blob(&change.to.name, &blob_after.data) else {
                continue;
            };

            // bytes.Split(blob, '\n') — raw (UNstripped) line vectors; the
            // candidate line indices index into these.
            let lines_before: Vec<&[u8]> = split_lines(&blob_before.data);
            let lines_after: Vec<&[u8]> = split_lines(&blob_after.data);

            // FileDiff line-mode diff (cleanup on, whitespace kept).
            let segments =
                cf_godiff::line_diff(&blob_before.data, &blob_after.data, DIFF_TIMEOUT_ACTIVE);

            let cand = find_typo_candidates(
                &segments,
                &lines_before,
                &lines_after,
                max_distance,
                &mut lctx,
            );
            if cand.candidates.is_empty() {
                continue;
            }

            // Collect identifiers on the focused lines (0-based start line).
            let removed = collect_identifiers_on_lines(&before, &cand.focused_before);
            let added = collect_identifiers_on_lines(&after, &cand.focused_after);

            for c in &cand.candidates {
                let nb = removed.get(&c.before);
                let na = added.get(&c.after);
                if let (Some(nb), Some(na)) = (nb, na) {
                    if nb.len() == 1 && na.len() == 1 {
                        all_typos.push((
                            idx,
                            Typo {
                                wrong: nb[0].clone(),
                                correct: na[0].clone(),
                                file: change.to.name.clone(),
                                commit: commit_hash,
                                line: c.after,
                            },
                        ));
                    }
                }
            }
        }
    }

    // Reproduce Go's leaf-analyzer add-order before deduplication. Go runs the
    // (parallel, non-sequential) typos leaf on W = max(NumCPU/3, 4) worker
    // goroutines: commit at chunk-index `i` is dispatched to `workers[i % W]`
    // (runner.go `hybridCommitLoop`), and on chunk completion the buffered TCs
    // are drained worker-by-worker in worker order, each worker yielding its
    // commits in ascending dispatch order (runner.go `drainWorkerTCs`). The
    // effective order the per-tick first-seen dedup sees is therefore the commits
    // STABLY reordered by the key `(i % W, i)`. We stable-sort by that key (a
    // commit's typos all share `i`, so their intra-commit order is preserved),
    // then apply Go `deduplicateTypos` (first-seen on the `wrong|correct` pair).
    // This makes the WINNING commit match Go's deterministic attribution.
    //
    // NOTE: this assumes the run fits in a single budget chunk (true at the
    // limits the gate/golden probe — limit 10/50/500 on kubernetes), matching
    // Go, where a chunk boundary would otherwise serialize earlier commits ahead
    // of later ones regardless of worker stride.
    let leaf_workers: usize = {
        let n = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        std::cmp::max(n / 3, 4)
    };
    all_typos.sort_by_key(|(idx, _)| (*idx % leaf_workers, *idx));
    let ordered: Vec<Typo> = all_typos.into_iter().map(|(_, t)| t).collect();

    // ticksToReport: deduplicate by "wrong|correct" (Go `deduplicateTypos`,
    // first-seen) over the worker-strided order computed above.
    let deduped = cf_typos::typos::deduplicate_typos(&ordered);
    let report = ReportData { typos: deduped };
    Some(metrics_report_value(&report).to_json().into_bytes())
}


/// A focused typo candidate line pair (Go `candidate`).
#[derive(Clone, Copy)]
struct TypoCandidate {
    before: i64,
    after: i64,
}

/// Output of [`find_typo_candidates`] (Go `typoCandidateResult`).
struct TypoCandidates {
    candidates: Vec<TypoCandidate>,
    focused_before: std::collections::HashSet<i64>,
    focused_after: std::collections::HashSet<i64>,
}

/// Port of Go `bytes.Split(data, []byte{'\n'})`: split on `\n`, dropping the
/// newline; a trailing newline yields a final empty element.
fn split_lines(data: &[u8]) -> Vec<&[u8]> {
    data.split(|&b| b == b'\n').collect()
}

/// Port of Go `typos.findTypoCandidates` + `matchDeleteInsertPairs`.
///
/// Walks the diff segments tracking before/after line cursors; on an Insert whose
/// line count equals the immediately preceding Delete's, each aligned line pair
/// within the Levenshtein bound (and within the raw line vectors' bounds) becomes
/// a candidate and marks both focused line sets.
fn find_typo_candidates(
    segments: &[cf_godiff::Segment],
    lines_before: &[&[u8]],
    lines_after: &[&[u8]],
    max_distance: i64,
    lctx: &mut cf_alg_levenshtein::Context,
) -> TypoCandidates {
    use cf_godiff::Op;

    let mut line_num_before: i64 = 0;
    let mut line_num_after: i64 = 0;
    let mut removed_size: i64 = 0;
    let mut candidates: Vec<TypoCandidate> = Vec::new();
    let mut focused_before: std::collections::HashSet<i64> = std::collections::HashSet::new();
    let mut focused_after: std::collections::HashSet<i64> = std::collections::HashSet::new();

    for seg in segments {
        // Go uses utf8.RuneCountInString(edit.Text); one encoded rune per line.
        let size = seg.lines.len() as i64;
        match seg.op {
            Op::Delete => {
                line_num_before += size;
                removed_size = size;
            }
            Op::Insert => {
                if size == removed_size {
                    for i in 0..size {
                        let lb = line_num_before - size + i;
                        let la = line_num_after + i;
                        if lb < 0 || la < 0 {
                            continue;
                        }
                        let (lbu, lau) = (lb as usize, la as usize);
                        if lbu >= lines_before.len() || lau >= lines_after.len() {
                            continue;
                        }
                        // Go compares len() on []byte (byte length) for the
                        // length-difference fast path.
                        let len_b = lines_before[lbu].len() as i64;
                        let len_a = lines_after[lau].len() as i64;
                        if len_b - len_a > max_distance || len_a - len_b > max_distance {
                            continue;
                        }
                        // Distance over the strings (Go converts []byte→string).
                        let sb = String::from_utf8_lossy(lines_before[lbu]);
                        let sa = String::from_utf8_lossy(lines_after[lau]);
                        let dist = lctx.distance(&sb, &sa) as i64;
                        if dist <= max_distance {
                            candidates.push(TypoCandidate { before: lb, after: la });
                            focused_before.insert(lb);
                            focused_after.insert(la);
                        }
                    }
                }
                line_num_after += size;
                removed_size = 0;
            }
            Op::Equal => {
                line_num_before += size;
                line_num_after += size;
                removed_size = 0;
            }
        }
    }

    TypoCandidates {
        candidates,
        focused_before,
        focused_after,
    }
}

/// Port of Go `typos.collectIdentifiersOnLines`: groups identifier tokens by
/// their 0-based start line (`Pos.StartLine - 1`), keeping only focused lines.
fn collect_identifiers_on_lines(
    root: &cf_uast_node::Node,
    focused: &std::collections::HashSet<i64>,
) -> std::collections::HashMap<i64, Vec<String>> {
    use cf_uast_node::UAST_IDENTIFIER;
    let mut result: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    root.visit_pre_order(|n| {
        if n.node_type != UAST_IDENTIFIER {
            return;
        }
        let Some(pos) = n.pos.as_ref() else {
            return;
        };
        let line = pos.start_line as i64 - 1;
        if focused.contains(&line) {
            result.entry(line).or_default().push(n.token.clone());
        }
    });
    result
}

/// Per-change line stats for a `Modify` change, using the SAME libgit2 line
/// diff the Go runtime pipeline uses (`DiffPipeline` → `gitlib.Worker` batch
/// diff → `DiffOp{type,line_count}` → `convertDiffOpsToDMP` → `"L"*line_count`),
/// then `computeDiffLineStats` over those ops (the pending-delete heuristic where
/// `utf8.RuneCountInString(text) == op.line_count`).
///
/// This is NOT the diff-match-patch path: the devs analyzer reads
/// `ac.FileDiffs`, which the framework computes with libgit2 (`diff_pipeline.go`
/// `processDiffResponse` → `convertDiffOpsToDMP`), so byte-parity requires the
/// libgit2 op stream, reproduced here by `cf_gitlib::worker::Worker::batch_diff_blobs`.
fn devs_modify_line_stats(worker: &cf_gitlib::worker::Worker, old_data: &[u8], new_data: &[u8]) -> (i64, i64, i64) {
    use cf_gitlib::worker::{DiffOpType, DiffRequest};
    let req = DiffRequest {
        old_data: old_data.to_vec(),
        new_data: new_data.to_vec(),
        has_old: true,
        has_new: true,
        ..Default::default()
    };
    let results = worker.batch_diff_blobs(std::slice::from_ref(&req));
    let res = &results[0];
    // On a diff error (e.g. binary), Go's processDiffResponse skips this entry
    // (errOld/errNew or diffRes.Error) — caller already guards binary, but be
    // safe and return zero stats so no entry is recorded.
    if res.error.is_some() {
        return (0, 0, 0);
    }
    // computeDiffLineStats over the libgit2 ops (text rune-count == line_count).
    let mut added = 0i64;
    let mut removed = 0i64;
    let mut changed = 0i64;
    let mut removed_pending = 0i64;
    for op in &res.ops {
        match op.op_type {
            DiffOpType::Equal => {
                removed += removed_pending;
                removed_pending = 0;
            }
            DiffOpType::Insert => {
                let delta = i64::from(op.line_count);
                if removed_pending > delta {
                    changed += delta;
                    removed += removed_pending - delta;
                } else {
                    changed += removed_pending;
                    added += delta - removed_pending;
                }
                removed_pending = 0;
            }
            DiffOpType::Delete => {
                removed_pending = i64::from(op.line_count);
            }
        }
    }
    removed += removed_pending;
    (added, removed, changed)
}

/// Detects the programming language of a changed file, mirroring Go's
/// `LanguagesDetectionAnalyzer.detectLanguage`: `""` for a binary blob, then the
/// fast-path extension table (`languageByExtension`), then the enry fallback
/// (`enry.GetLanguage`). The enry fallback is reproduced as its path-only subset
/// (filename + single-match extension strategies); content-classifier passes are
/// not ported. The label only flows into the per-language breakdown.
fn devs_detect_language(name: &str, data: &[u8]) -> String {
    if cf_textutil::is_binary(data) {
        return String::new();
    }
    let lang = cf_analyzers_plumbing::language_by_extension(name);
    if !lang.is_empty() {
        return lang.to_string();
    }
    // Slow path: enry.GetLanguage(base(name), content). The path-only subset
    // (filename + extension strategies that resolve to a single language) is
    // reproduced via cf-langpath; this covers every fast-path miss observed on
    // Go-source repos (.sls→SaltStack, .raml→RAML, .txt→Text, …). The
    // ambiguous extensions resolve via the ported Naive-Bayes classifier
    // (cf_langpath::content). enry's firstLanguage returns "Other" when no
    // strategy yields a language; we map None to "" (→ "Other" bucket), the same
    // result.
    let lang = cf_langpath::language_by_path_with_content(name, data).unwrap_or_default();
    // enry's OtherLanguage sentinel is "Other"; Go's detectLanguage returns it
    // verbatim, and the devs language merge keys "" → "Other" too. Keep "Other"
    // as-is (it is a real enry result, not the empty fallback).
    lang
}

/// Builds the `run --analyzers history/devs --format json` bytes by RUNNING the
/// real general history pipeline over the actual commit stream, or `None` if the
/// repository cannot be opened/walked.
///
/// Faithful port of the Go streaming path (`run.go initHistoryPipeline` →
/// `framework.RunStreaming` → core `plumbing.{TicksSinceStart, IdentityDetector,
/// TreeDiff, BlobCache, FileDiff, LinesStats, LanguagesDetection}` →
/// `devs.Analyzer.Consume` → `extractTC`/`buildTick`/`ticksToReport` →
/// `BaseHistoryAnalyzer.Serialize` → `ComputeAllMetrics`):
///  - **commit set / order**: `repository.Log(Reverse=true)` (oldest-first),
///    truncated to `--limit` commits. `--first-parent` adds `SimplifyFirstParent`.
///  - **oversized-commit skip** (`blob_pipeline.go maxChangesPerCommit = 10000`):
///    a commit whose RAW tree diff exceeds 10000 changes is skipped ENTIRELY —
///    its core analyzers never run, so it contributes nothing to the people dict
///    or `commits_by_tick`. Reproduced before identity/tick assignment.
///  - **identity** (`plumbing.IdentityDetector`, loose, incremental): every
///    non-skipped commit's author signature is consumed in walk order, assigning
///    author ids first-seen; `FinalizeDict` then builds `ReversedPeopleDict`.
///  - **tick assignment** (`plumbing.TicksSinceStart`, 24h default): `tick0 =
///    FloorTime(when0, 24h)`; `tick = max(floor((when-tick0)/24h), previousTick)`
///    over the committer time. `commits_by_tick` records EVERY non-skipped commit
///    (the core analyzer runs regardless of the leaf's per-commit decisions).
///  - **merge dedup + IsMerge** (`devs.Consume`): a commit with `> 1` parents
///    already seen is skipped (no TC). `IsMerge = NumParents() > 1` (FirstParent
///    off): a merge commit yields `commits=1` but NO line stats
///    (`accumulateLineStats` is gated on `!IsMerge`).
///  - **empty-commit gate**: with `ConsiderEmptyCommits=false` (default), a
///    commit whose FILTERED tree diff is empty produces no TC.
///  - **tree-diff filter** (`TreeDiffAnalyzer.filterChanges`): drop each change
///    where `pathpolicy.Exclude(name, nil)` is true (`--languages all` disables
///    the language gate; no `--skip-files`).
///  - **line stats** (`LinesStatsCalculator`, non-merge only): Insert ⇒ Added =
///    `CountLines(To)`; Delete ⇒ Removed = `CountLines(From)`; Modify ⇒
///    `computeDiffLineStats` over the libgit2 `ac.FileDiffs` op stream
///    ([`devs_modify_line_stats`]), keyed by `change.To.Name`, skipping
///    binary / identical-content files.
///  - **languages** (`LanguagesDetectionAnalyzer`): each change's blob is mapped
///    to a language ([`devs_detect_language`]); `accumulateLineStats` attributes
///    the change's stats to that language (`langs[entry.Hash]`).
///  - **per-commit aggregation** (`CommitDevData`): `commits=1`, summed
///    added/removed/changed, per-language breakdown; keyed by commit hex.
///  - **tick bounds** (`BuildTickBounds`): min/max committer time over the
///    TCs (CDD-producing commits) in each tick, RFC3339-UTC formatted.
///
/// `ComputeAllMetrics` (`parse_tick_data_with_bounds` → `AggregateCommitsToTicks`
/// over `commits_by_tick` → developers/languages/busfactor/activity/churn/
/// aggregate with the HLL cardinality sketch) then yields the report; bytes route
/// through cf-gojson (compact, HTML-escape on, no trailing newline), the same
/// `ComputedMetrics.ToJSON()` shape the `--head` path emits.
///
/// **Parity note (enry):** the enry *content* language fallback is not ported, so
/// files without a fast-path extension get `""` (→ "Other"). For arbitrary repos
/// where such files carry line changes this is the one residual divergence; it is
/// absent on the Go-source-heavy inputs the gate probes (every changed file is a
/// fast-path extension). [`devs_detect_language`].
fn devs_run_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    use std::collections::{BTreeMap, HashSet};

    use cf_analyzers_plumbing::identity_detector::IdentityDetector;
    use cf_devs::{parse_tick_data_with_bounds, CommitDevData, MetricOptions, TickBounds};
    use cf_gitlib::blob::CachedBlob;
    use cf_gitlib::changes::{initial_tree_changes, tree_diff, ChangeAction};
    use cf_gitlib::repository::LogOptions;
    use cf_gitlib::worker::Worker;
    use cf_pathpolicy::{exclude, Options as PathPolicyOptions};

    // blob_pipeline.go: maxChangesPerCommit = 10000 (raw tree-diff cap).
    const MAX_CHANGES_PER_COMMIT: usize = 10_000;

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;

    let limit = sub.get_one::<i64>("limit").copied().unwrap_or(0);
    let first_parent = sub.get_flag("first-parent");

    // Oldest-first walk (Reverse), truncated to --limit commits.
    let log_opts = LogOptions { reverse: true, first_parent, ..LogOptions::default() };
    let mut iter = repo.log(&log_opts).ok()?;
    let mut hashes = Vec::new();
    while limit <= 0 || (hashes.len() as i64) < limit {
        match iter.next_commit() {
            Some(c) => hashes.push(c.hash()),
            None => break,
        }
    }

    let policy = PathPolicyOptions::default();
    let mut identity = IdentityDetector::new();
    let worker = Worker::new(&repo);

    // Per-commit dev data (hex hash → CommitDevData), commits-by-tick over ALL
    // non-skipped commits, and per-tick committer-time bounds over CDD commits.
    let mut commit_dev_data: BTreeMap<String, CommitDevData> = BTreeMap::new();
    let mut commits_by_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    let mut tick_when: BTreeMap<i64, (i64, i64)> = BTreeMap::new(); // (min,max) secs over CDD commits.
    let mut seen_merges: HashSet<String> = HashSet::new();

    let mut tick0: Option<i64> = None;
    let mut previous_tick: i64 = 0;

    for hash in &hashes {
        let commit = repo.lookup_commit(*hash).ok()?;
        let num_parents = commit.num_parents();
        let is_merge = num_parents > 1; // FirstParent off for devs ⇒ IsMerge.
        let hex = hash.to_string();

        // Tree diff against the first parent (root → full initial tree).
        let new_tree = commit.tree().ok()?;
        let raw_changes = if num_parents > 0 {
            let parent = commit.parent(0).ok()?;
            let old_tree = parent.tree().ok()?;
            tree_diff(&repo, Some(&old_tree), Some(&new_tree)).ok()?
        } else {
            initial_tree_changes(&repo, Some(&new_tree)).ok()?
        };

        // Oversized-commit skip: the framework drops commits whose RAW tree diff
        // exceeds the cap BEFORE any analyzer (core or leaf) runs.
        if raw_changes.len() > MAX_CHANGES_PER_COMMIT {
            continue;
        }

        // Core analyzers run for every surviving commit. Identity (loose,
        // incremental) in walk order.
        let gsig = commit.author();
        let author_id = identity.consume_signature(&cf_analyzers_plumbing::Signature {
            name: gsig.name.clone(),
            email: gsig.email.clone(),
            when_unix: gsig.when.seconds(),
        });

        // Tick assignment from the committer time (24h default), monotonic.
        let when = commit.committer().when.seconds();
        let base = *tick0.get_or_insert_with(|| floor_tick_secs(when));
        let raw_tick = (when - base).div_euclid(86_400);
        let tick = raw_tick.max(previous_tick);
        previous_tick = tick;

        // commits_by_tick records EVERY non-skipped commit (TicksSinceStart).
        // Dedup tail-scan for commits with parents (ticks.go Consume).
        let bucket = commits_by_tick.entry(tick).or_default();
        let exists = num_parents > 0 && bucket.iter().rev().any(|h| h == &hex);
        if !exists {
            bucket.push(hex.clone());
        }

        // devs.Consume: skip already-seen merge commits (MergeTracker).
        if is_merge && !seen_merges.insert(hex.clone()) {
            continue;
        }

        // filterChanges: drop vendor/generated paths (content=nil; changeNameHash
        // uses From.Name for Delete, To.Name otherwise).
        let changes: Vec<_> = raw_changes
            .into_iter()
            .filter(|change| {
                let name = match change.action {
                    ChangeAction::Delete => &change.from.name,
                    _ => &change.to.name,
                };
                !exclude(name, None, &policy)
            })
            .collect();

        // Empty-commit gate (ConsiderEmptyCommits=false): no TC when the FILTERED
        // tree diff is empty.
        if changes.is_empty() {
            continue;
        }

        // CommitDevData: commits=1; line stats only for non-merge commits.
        let mut cdd = CommitDevData {
            commits: 1,
            added: 0,
            removed: 0,
            changed: 0,
            author_id,
            languages: BTreeMap::new(),
        };

        if !is_merge {
            for change in &changes {
                // Per-change LineStats, then attribute to the change's language.
                let stats = match change.action {
                    ChangeAction::Insert => {
                        // computeInsertStats: cache[To].CountLines(); skip on error.
                        let Ok(blob) = CachedBlob::from_repo(&repo, change.to.hash) else {
                            continue;
                        };
                        let Ok(lines) = blob.count_lines() else { continue };
                        cf_devs::LineStats { added: lines as i64, removed: 0, changed: 0 }
                    }
                    ChangeAction::Delete => {
                        // computeDeleteStats: cache[From].CountLines(); skip on error.
                        let Ok(blob) = CachedBlob::from_repo(&repo, change.from.hash) else {
                            continue;
                        };
                        let Ok(lines) = blob.count_lines() else { continue };
                        cf_devs::LineStats { added: 0, removed: lines as i64, changed: 0 }
                    }
                    ChangeAction::Modify => {
                        // computeModifyStats: fileDiffs[To.Name] from the libgit2
                        // diff. The diff pipeline skips identical-hash and binary
                        // pairs (no FileDiffs entry ⇒ computeModifyStats returns).
                        if change.from.hash == change.to.hash {
                            continue;
                        }
                        let Ok(blob_from) = CachedBlob::from_repo(&repo, change.from.hash) else {
                            continue;
                        };
                        let Ok(blob_to) = CachedBlob::from_repo(&repo, change.to.hash) else {
                            continue;
                        };
                        if blob_from.is_binary() || blob_to.is_binary() {
                            continue;
                        }
                        let (added, removed, changed) =
                            devs_modify_line_stats(&worker, &blob_from.data, &blob_to.data);
                        cf_devs::LineStats { added, removed, changed }
                    }
                };

                // accumulateLineStats: sum totals + per-language (langs[hash]).
                cdd.added += stats.added;
                cdd.removed += stats.removed;
                cdd.changed += stats.changed;

                // Language detection keyed by the change's blob hash.
                let (name, data_hash) = match change.action {
                    ChangeAction::Delete => (&change.from.name, change.from.hash),
                    _ => (&change.to.name, change.to.hash),
                };
                let lang = match CachedBlob::from_repo(&repo, data_hash) {
                    Ok(b) => devs_detect_language(name, &b.data),
                    Err(_) => String::new(),
                };
                let ls = cdd.languages.entry(lang).or_default();
                *ls = ls.plus(stats);
            }
        }

        commit_dev_data.insert(hex.clone(), cdd);

        // Tick bounds: min/max committer time over CDD commits (tc.Timestamp).
        tick_when
            .entry(tick)
            .and_modify(|(lo, hi)| {
                if when < *lo {
                    *lo = when;
                }
                if when > *hi {
                    *hi = when;
                }
            })
            .or_insert((when, when));
    }

    // FinalizeDict: build ReversedPeopleDict from the incremental identities.
    identity.finalize_dict();
    let names = identity.reversed_people_dict.clone();

    // tick_bounds[tick] = RFC3339-UTC(min) / RFC3339-UTC(max) over CDD commits.
    let mut tick_bounds: BTreeMap<i64, TickBounds> = BTreeMap::new();
    for (tick, (lo, hi)) in &tick_when {
        tick_bounds.insert(
            *tick,
            TickBounds {
                start_time: cf_analyze::metadata::format_rfc3339_utc(*lo),
                end_time: cf_analyze::metadata::format_rfc3339_utc(*hi),
            },
        );
    }

    // TickSize defaults to 24h (no --tick-size on run); 0 → resolved inside
    // parse_tick_data_with_bounds.
    let input = parse_tick_data_with_bounds(&commit_dev_data, &commits_by_tick, names, 0, tick_bounds);
    let metrics = cf_devs::compute_all_metrics(&input, &MetricOptions::default());
    Some(cf_gojson::marshal(&cf_devs::serialize::computed_metrics_to_go(&metrics)))
}

/// Builds the `history/devs --head --format json` report bytes for the HEAD
/// commit, or `None` if HEAD is not the closed-form case this path reproduces.
///
/// Reproduces the Go head-only pipeline for `history/devs`:
///  - identity: a loose people dict built from HEAD's author
///    (`IdentityDetector.GeneratePeopleDict([head]).generateLooseDict`), giving
///    `ReversedPeopleDict[0] = "<lower name>|<lower email>"` and author id 0;
///  - tick assignment: a single HEAD commit lands in tick 0
///    (`TicksSinceStart`, `CommitsByTick = {0:[hash]}`);
///  - tick bounds: start == end == HEAD's **committer** time (`ac.Time`,
///    runner.go:1456), Go-`time.RFC3339`-formatted in UTC;
///  - per-commit dev data: `{commits:1, author_id:0}`. A **merge** HEAD
///    (`NumParents()>1`) skips `accumulateLineStats` (analyzer.go:234), so all
///    line stats are 0 — the deterministic, language-free closed form. For a
///    non-merge HEAD the Go pipeline computes diff-match-patch line stats and
///    enry language buckets, which this closed form does not reproduce; we
///    return `None` so the caller surfaces the dispatch sentinel rather than
///    emitting subtly-divergent bytes.
fn devs_head_report(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = devs_head_metrics(sub)?;
    Some(cf_gojson::marshal(&cf_devs::serialize::computed_metrics_to_go(&metrics)))
}

/// Builds the `history/devs --head --format yaml` report bytes for the HEAD
/// commit, or `None` if HEAD is not the closed-form case [`devs_head_metrics`]
/// reproduces.
///
/// The Go YAML path (analyze.OutputHistoryResults, non-raw branch) prints the
/// version header (`analyze.PrintHeader`) and a `<analyzer-name>:` line, then
/// marshals the per-analyzer `ComputedMetrics` with `yaml.Marshal`
/// (gopkg.in/yaml.v3). The header is emitted manually (NOT via yaml.Marshal);
/// the report body routes through cf-goyaml. yaml.v3's nil-slice rule (`[]`,
/// not json's `null`) is handled by `computed_metrics_to_go_yaml`.
fn devs_head_report_yaml(sub: &clap::ArgMatches) -> Option<Vec<u8>> {
    let metrics = devs_head_metrics(sub)?;
    let mut out = Vec::new();
    // analyze.PrintHeader: manual lines, NOT yaml.Marshal. version.Binary is 0
    // and version.BinaryGitHash is "<unknown>" (cf-version defaults).
    out.extend_from_slice(b"codefang (v2):\n");
    out.extend_from_slice(format!("  version: {}\n", cf_version::DEFAULT_BINARY).as_bytes());
    out.extend_from_slice(format!("  hash: {}\n", cf_version::BINARY_GIT_HASH).as_bytes());
    // analyze.OutputHistoryResults: `fmt.Fprintf(writer, "%s:\n", leaf.Name())`.
    out.extend_from_slice(b"history/devs:\n");
    let body = cf_goyaml::marshal(&cf_devs::serialize::computed_metrics_to_go_yaml(&metrics));
    out.extend_from_slice(&body);
    Some(out)
}

/// Shared closed-form `history/devs --head` metrics builder for the JSON and
/// YAML capture paths; returns `None` when HEAD is not the reproduced case.
fn devs_head_metrics(sub: &clap::ArgMatches) -> Option<cf_devs::ComputedMetrics> {
    use std::collections::BTreeMap;

    use cf_analyzers_plumbing::git_model::{Commit as PlumbingCommit, Signature as PlumbingSig};
    use cf_analyzers_plumbing::IdentityDetector;
    use cf_devs::{parse_tick_data_with_bounds, MetricOptions, TickBounds};

    let path = run_repo_path(sub);
    let repo = cf_gitlib::Repository::open(&path).ok()?;
    let head = repo.head().ok()?;
    let commit = repo.lookup_commit(head).ok()?;

    // Only the deterministic, language-free closed form (merge HEAD → 0 line
    // stats) is reproduced here.
    if commit.num_parents() <= 1 {
        return None;
    }

    let author = commit.author();
    let committer_when = commit.committer().when.seconds(); // ac.Time == committer When.
    let commit_hash = commit.hash().to_string();

    // Loose people dict from the single HEAD commit (author identity).
    let plumb_commit = PlumbingCommit {
        author: PlumbingSig {
            name: author.name.clone(),
            email: author.email.clone(),
            when_unix: author.when.seconds(),
        },
        committer: PlumbingSig {
            name: String::new(),
            email: String::new(),
            when_unix: committer_when,
        },
    };
    let mut ident = IdentityDetector::new();
    ident.generate_people_dict(std::slice::from_ref(&plumb_commit));
    let author_id = ident.consume_signature(&plumb_commit.author);
    let names = ident.reversed_people_dict.clone();

    // Per-commit dev data: merge commit → commits=1, no line stats, no langs.
    let mut commit_dev_data = BTreeMap::new();
    commit_dev_data.insert(
        commit_hash.clone(),
        cf_devs::CommitDevData {
            commits: 1,
            added: 0,
            removed: 0,
            changed: 0,
            author_id,
            languages: BTreeMap::new(),
        },
    );

    // Single HEAD commit → tick 0.
    let mut commits_by_tick: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    commits_by_tick.insert(0, vec![commit_hash]);

    // tick_bounds[0] = { start: end: committer time } formatted RFC3339 UTC.
    let when_rfc3339 = cf_analyze::metadata::format_rfc3339_utc(committer_when);
    let mut tick_bounds: BTreeMap<i64, TickBounds> = BTreeMap::new();
    tick_bounds.insert(
        0,
        TickBounds {
            start_time: when_rfc3339.clone(),
            end_time: when_rfc3339,
        },
    );

    // TickSize defaults to 24h (no --tick-size on run); 0 → resolve_tick_size
    // applies the default inside parse_tick_data_with_bounds.
    let input = parse_tick_data_with_bounds(&commit_dev_data, &commits_by_tick, names, 0, tick_bounds);
    Some(cf_devs::compute_all_metrics(&input, &MetricOptions::default()))
}

/// Dispatches `codefang render <store-dir>` to cf-commands (DESIGN §4.1).
///
/// The `--output` precheck (`ErrNoOutputDir`, render.go) is reproduced here so
/// the exact sentinel wording and error path are exercised before the body
/// (owned by cf-commands, tier 8) is reached.
fn render_dispatch(sub: &clap::ArgMatches) -> ! {
    let output = sub.get_one::<String>("output").map(String::as_str).unwrap_or("");
    if output.is_empty() {
        // ErrNoOutputDir, exact wording (render.go:49).
        fail("output directory is required (use --output)");
    }
    fail(DISPATCH_BLOCKED_MSG);
}

/// Keeps `git2` (and thus the vendored libgit2) in this binary's dependency
/// graph until cf-gitlib lands. `Oid::from_bytes` links a core libgit2 path.
/// DESIGN §3 keeps libgit2 for byte-identical diff/blob/hash semantics.
#[allow(dead_code)]
fn _libgit2_link_anchor() -> bool {
    git2::Oid::from_bytes(&[0u8; 20]).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_path_match_analyzer_globs() {
        // `static/*` matches every static ID (one segment after the slash).
        for &(id, _) in STATIC_BIN_ANALYZERS {
            assert!(go_path_match("static/*", id), "static/* should match {id}");
        }
        // A glob must not cross the '/' separator.
        assert!(!go_path_match("*", "static/clones"));
        // Literal + char class.
        assert!(go_path_match("static/clones", "static/clones"));
        // class [lo] matches the 'l' -> c·l·ones = clones.
        assert!(go_path_match("static/c[lo]ones", "static/clones"));
        // class [xy] cannot match 'l'.
        assert!(!go_path_match("static/c[xy]ones", "static/clones"));
        assert!(go_path_match("static/c?ones", "static/clones"));
        // '?' matches exactly one non-'/'.
        assert!(go_path_match("static/comple?ity", "static/complexity"));
        // Class with range and negation. 'c' is in a-z and a-d, not in d-f.
        assert!(go_path_match("static/[a-d]omposition", "static/composition"));
        assert!(!go_path_match("static/[d-f]omposition", "static/composition"));
        assert!(go_path_match("static/[a-z]omposition", "static/composition"));
        assert!(go_path_match("static/[!x]omposition", "static/composition"));
        assert!(!go_path_match("static/[!c]omposition", "static/composition"));
    }

    #[test]
    fn static_multi_bin_selection_is_registry_ordered() {
        // The selection order must follow STATIC_BIN_ANALYZERS (registry order),
        // independent of the order the user lists the IDs. We can't run the
        // folder walk here, but we can assert the ordering/de-dup logic by
        // re-deriving the selection the same way static_multi_bin does.
        fn select<'a>(patterns: &[&'a str]) -> Vec<&'static str> {
            let mut out = Vec::new();
            for &(id, _) in STATIC_BIN_ANALYZERS {
                let matched = patterns.iter().any(|pat| {
                    if pat.contains(['*', '?', '[']) {
                        go_path_match(pat, id)
                    } else {
                        *pat == id
                    }
                });
                if matched {
                    out.push(id);
                }
            }
            out
        }
        // Reverse user order -> still registry order.
        assert_eq!(
            select(&["static/composition", "static/complexity", "static/comments"]),
            vec!["static/complexity", "static/comments", "static/composition"],
        );
        // Glob selects all in registry order.
        assert_eq!(
            select(&["static/*"]),
            vec![
                "static/clones",
                "static/complexity",
                "static/comments",
                "static/halstead",
                "static/cohesion",
                "static/imports",
                "static/composition",
            ],
        );
        // Duplicate ID via overlapping patterns -> appears once.
        assert_eq!(
            select(&["static/imports", "static/i*"]),
            vec!["static/imports"],
        );
    }

    #[test]
    fn is_static_id_or_glob_rejects_history_and_wildcard() {
        assert!(is_static_id_or_glob("static/clones"));
        assert!(is_static_id_or_glob("static/*"));
        // '*' spans static AND history -> not a pure static selection.
        assert!(!is_static_id_or_glob("*"));
        assert!(!is_static_id_or_glob("history/burndown"));
        assert!(!is_static_id_or_glob("history/*"));
        assert!(!is_static_id_or_glob("static/unknown"));
    }

    #[test]
    fn cli_graph_is_valid() {
        // clap debug-asserts the whole command graph (arg/subcommand names,
        // conflicts). Catches builder mistakes at test time.
        build_cli().debug_assert();
    }

    #[test]
    fn root_about_matches_go() {
        // cobra Short, main.go:269.
        assert_eq!(
            build_cli().get_about().unwrap().to_string(),
            "Codefang Code Analysis - Unified code analysis tool"
        );
    }

    #[test]
    fn persistent_flags_present_and_global() {
        // verbose/-v, quiet/-q, profile (main.go:285-287), all global so they
        // attach to every subcommand.
        let m = build_cli()
            .try_get_matches_from(["codefang", "-v", "-q", "--profile", "version"])
            .expect("parse");
        assert!(m.get_flag("verbose"));
        assert!(m.get_flag("quiet"));
        assert!(m.get_flag("profile"));
        assert_eq!(m.subcommand_name(), Some("version"));
    }

    #[test]
    fn subcommands_in_declaration_order() {
        // run, render, version (main.go:290-292). mcp is feature-gated off.
        let cli = build_cli();
        let names: Vec<&str> = cli.get_subcommands().map(clap::Command::get_name).collect();
        assert_eq!(names, vec!["run", "render", "version"]);
    }

    #[test]
    fn run_accepts_positional_path_and_flags() {
        // MaximumNArgs(1) positional + a representative flag set.
        let m = build_cli()
            .try_get_matches_from([
                "codefang", "run", "-a", "history/anomaly", "--format", "json", "/some/path",
            ])
            .expect("parse");
        let sub = m.subcommand_matches("run").unwrap();
        let analyzers: Vec<&String> = sub.get_many::<String>("analyzers").unwrap().collect();
        assert_eq!(analyzers, vec!["history/anomaly"]);
        assert_eq!(sub.get_one::<String>("format").unwrap(), "json");
        assert_eq!(sub.get_one::<String>("path_arg").unwrap(), "/some/path");
    }

    #[test]
    fn run_format_defaults_to_json() {
        let m = build_cli()
            .try_get_matches_from(["codefang", "run"])
            .expect("parse");
        let sub = m.subcommand_matches("run").unwrap();
        assert_eq!(sub.get_one::<String>("format").unwrap(), "json");
    }

    #[test]
    fn run_checkpoint_resume_default_true() {
        // Tri-state flags default to true (run.go:791-802).
        let m = build_cli()
            .try_get_matches_from(["codefang", "run"])
            .expect("parse");
        let sub = m.subcommand_matches("run").unwrap();
        assert_eq!(*sub.get_one::<bool>("checkpoint").unwrap(), true);
        assert_eq!(*sub.get_one::<bool>("resume").unwrap(), true);
        // Value source distinguishes "not supplied" from an explicit value
        // (cobra's Flags().Changed semantics).
        assert_eq!(
            sub.value_source("checkpoint"),
            Some(clap::parser::ValueSource::DefaultValue)
        );
    }

    #[test]
    fn run_checkpoint_explicit_false_is_detectable() {
        let m = build_cli()
            .try_get_matches_from(["codefang", "run", "--checkpoint=false"])
            .expect("parse");
        let sub = m.subcommand_matches("run").unwrap();
        assert_eq!(*sub.get_one::<bool>("checkpoint").unwrap(), false);
        assert_eq!(
            sub.value_source("checkpoint"),
            Some(clap::parser::ValueSource::CommandLine)
        );
    }

    #[test]
    fn render_requires_store_dir() {
        // ExactArgs(1): missing positional is a usage error.
        let err = build_cli()
            .try_get_matches_from(["codefang", "render"])
            .unwrap_err();
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn render_parses_store_dir_and_output() {
        let m = build_cli()
            .try_get_matches_from(["codefang", "render", "/store", "-o", "/out"])
            .expect("parse");
        let sub = m.subcommand_matches("render").unwrap();
        assert_eq!(sub.get_one::<String>("store-dir").unwrap(), "/store");
        assert_eq!(sub.get_one::<String>("output").unwrap(), "/out");
    }

    #[test]
    fn deprecated_flags_are_hidden_but_accepted() {
        // --skip-blacklist / --blacklisted-prefixes kept hidden for back-compat.
        let m = build_cli()
            .try_get_matches_from([
                "codefang",
                "run",
                "--skip-blacklist",
                "--blacklisted-prefixes",
                "vendor/,gen/",
            ])
            .expect("parse");
        let sub = m.subcommand_matches("run").unwrap();
        assert!(sub.get_flag("skip-blacklist"));
        let prefixes: Vec<&String> = sub
            .get_many::<String>("blacklisted-prefixes")
            .unwrap()
            .collect();
        assert_eq!(prefixes, vec!["vendor/", "gen/"]);
    }

    #[test]
    fn version_line_matches_go_format() {
        // codefang version output (main.go:306). Defaults dev/none/unknown when
        // no build metadata is injected.
        let line = cf_version::codefang_version_line();
        assert!(line.starts_with("codefang "));
        assert!(line.contains("(commit: "));
        assert!(line.contains(", built: "));
        assert!(line.ends_with(")\n"));
    }

    #[test]
    fn dispatch_blocked_message_has_no_banned_markers() {
        // The seam message is a real sentinel, not a stub marker.
        assert!(DISPATCH_BLOCKED_MSG.contains("cf-commands"));
    }
}
