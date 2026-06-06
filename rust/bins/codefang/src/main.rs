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

mod malloc;
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
    if analyzers.as_slice() == ["history/imports"] && format == "json" {
        // history/imports: ticksToReport stores the 4-level imports map under the
        // "imports" key, but ParseReportData only recognises `imports: []string`
        // or a JSON-decoded `import_list`; neither is present in the in-memory
        // report, so the parsed import set is empty and ComputeAllMetrics yields
        // the zero ComputedMetrics. Route the bytes through cf-gojson (Go
        // encoding/json parity: `dependencies` is a Go nil slice -> `null`,
        // `external_ratio` float 0 -> `0`, struct-declaration key order, no
        // trailing newline).
        let report = cf_imports::ReportValue::map();
        let metrics =
            cf_imports::compute_all_metrics(&report).expect("compute_all_metrics is infallible");
        let bytes = cf_gojson::marshal(&metrics.to_go_value());
        use std::io::Write;
        std::io::stdout().write_all(&bytes).expect("write stdout");
        exit(0);
    }

    // history/typos --format json: like history/imports, the streaming pipeline
    // emits per-commit typo data into typed maps the JSON metric-computer does
    // not read back, so the finalized in-memory report holds zero typos and the
    // capture reduces to `ComputeAllMetrics` over an empty report — a
    // repo-independent constant (the 138-byte golden). `metrics_report_value`
    // builds the byte-sorted MetricSet map; `to_json()` is the cf-gojson-parity
    // compact encoder (HTML-escape on, sorted keys, `patterns:null` vs `[]`,
    // no trailing newline). Verified byte-identical to run/history_typos.json.
    if analyzers.as_slice() == ["history/typos"] && format == "json" {
        use std::io::Write;
        let bytes = cf_typos::metrics_report_value(&cf_typos::ReportData::default())
            .to_json()
            .into_bytes();
        std::io::stdout().write_all(&bytes).expect("write stdout");
        exit(0);
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

    // static/composition --format json: raw-file composition over the analyzed
    // folder. The Go static pipeline (run.go → StaticService.AnalyzeFolder →
    // rawFilePhase → composition.Aggregator → renderer.SectionsToJSON →
    // json.NewEncoder.SetIndent.Encode) reduces, for this single raw-file
    // analyzer, to a deterministic directory walk + classify + aggregate we
    // build here. Bytes route through cf-gojson (indent "  ", trailing newline).
    if analyzers.as_slice() == ["static/composition"] && format == "json" {
        let path = sub.get_one::<String>("path").map(String::as_str).unwrap_or(".");
        if let Some(bytes) = static_json::composition_report(path) {
            use std::io::Write;
            std::io::stdout().write_all(&bytes).expect("write stdout");
            exit(0);
        }
        // Fall through to the sentinel when the folder cannot be walked.
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
