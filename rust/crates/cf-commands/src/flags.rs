//! The `run` and `render` clap command trees — Rust port of the flag wiring in
//! Go `cmd/codefang/commands/run.go` and `render.go`.
//!
//! Built with clap's **builder API** (not derive) so every flag's long name,
//! short, default, and help string matches cobra byte-for-byte (DESIGN §4).
//!
//! # `run` flags
//!
//! [`build_run_command`] registers the ~45 literal flags from `run.go`
//! (lines 268-320 + `registerPersistenceFlags` + `registerExclusionFlags`),
//! then the deprecated hidden flags ([`mark_deprecated_exclusion_flags`]), then
//! the dynamic per-analyzer flags ([`register_analyzer_flags`]). The help
//! strings are copied verbatim from the Go source.
//!
//! # Tri-state `--checkpoint` / `--resume`
//!
//! Both default `true`, but Go reads them via `Flags().Changed(name)` so a
//! config-file default only applies when the CLI flag was *not* supplied
//! (`parseBoolFlag`). clap models this with [`clap::ArgAction::SetTrue`]/`SetFalse`
//! is not enough for tri-state; instead these two flags take an explicit
//! `bool` value (`--checkpoint=false`) and the caller checks
//! [`clap::ArgMatches::value_source`] for [`clap::parser::ValueSource::CommandLine`]
//! to decide whether the user set it — the Rust analogue of `Changed`.
//!
//! # Dynamic per-analyzer flags
//!
//! [`register_analyzer_flags`] mirrors Go `registerAnalyzerFlags` /
//! `registerConfigFlag`: it walks the supplied
//! [`crate::registry::ConfigOptionProvider`]s, dedupes by flag name, and adds one
//! clap flag per [`cf_pipeline::ConfigurationOption`] keyed by its
//! [`cf_pipeline::ConfigurationOptionType`].

use cf_pipeline::{ConfigurationOption, ConfigurationOptionType, DefaultValue};
use clap::{Arg, ArgAction, Command};

use crate::formats::{FORMAT_JSON, INPUT_FORMAT_AUTO};
use crate::registry::ConfigOptionProvider;

/// Render-command constants, mirroring the Go `render.go` consts.
mod render_consts {
    /// `render <store-dir>` — the cobra `Use` string.
    pub const USE: &str = "render";
    /// Short help for the render command.
    pub const SHORT: &str = "Render stored analysis results as multi-page HTML";
    /// `--output` long flag name.
    pub const OUTPUT_FLAG: &str = "output";
    /// `-o` short flag.
    pub const OUTPUT_SHORT: char = 'o';
    /// `--output` help text.
    pub const OUTPUT_USAGE: &str = "output directory for HTML files";
}

/// Build the `run [path]` [`clap::Command`] with every literal flag, the
/// deprecated hidden flags, and the dynamic per-analyzer flags from
/// `providers`. Mirrors Go `newRunCommandWithAllDeps`.
///
/// `providers` supplies the analyzer configuration options (Go's
/// `buildPipeline(nil)` Core + Leaves). In the default (non-`runtime`) build the
/// caller passes [`default_analyzer_options`]; in the `runtime` build it passes
/// the real analyzer providers.
#[must_use]
pub fn build_run_command_with(providers: &[&dyn ConfigOptionProvider]) -> Command {
    let mut cmd = Command::new("run")
        .about("Run static and history analyzers")
        .long_about("Run selected static and history analyzers.")
        // Args: cobra.MaximumNArgs(1) -> 0..=1 positional [path].
        .arg(
            Arg::new("path-positional")
                .value_name("path")
                .num_args(0..=1)
                .help("Folder/repository path to analyze (overrides --path)"),
        );

    cmd = add_literal_run_flags(cmd);
    cmd = mark_deprecated_exclusion_flags(cmd);
    cmd = register_analyzer_flags(cmd, providers);
    cmd
}

/// Build the `run` command with the default analyzer option set
/// ([`default_analyzer_options`]). Convenience wrapper used by the default
/// (non-`runtime`) build and by tests.
#[must_use]
pub fn build_run_command() -> Command {
    let opts = default_analyzer_options();
    let provider: &dyn ConfigOptionProvider = &opts;
    build_run_command_with(&[provider])
}

/// Adds the ~45 literal `run` flags in Go declaration order, with verbatim help
/// strings and defaults (`run.go` lines 268-320, plus `registerPersistenceFlags`
/// and `registerExclusionFlags`).
#[allow(clippy::too_many_lines)]
fn add_literal_run_flags(cmd: Command) -> Command {
    cmd
        .arg(str_slice_arg("analyzers", Some('a'),
            "Analyzer IDs or glob patterns (example: static/complexity,history/*,*)"))
        .arg(str_arg("format", None, FORMAT_JSON,
            "Output format: json, yaml, plot, bin, timeseries, ndjson, text, compact"))
        .arg(bool_arg("ndjson",
            "With --format timeseries: emit one JSON line per commit (NDJSON)"))
        .arg(str_arg("input", None, "",
            "Input report path for cross-format conversion"))
        .arg(str_arg("input-format", None, INPUT_FORMAT_AUTO,
            "Input format: auto, json, bin"))
        .arg(int_arg("gogc", 0,
            "GC percent for history pipeline (0 = auto, >0 = exact)"))
        .arg(str_arg("ballast-size", None, "0",
            "Optional GC ballast size for history pipeline (0 = disabled)"))
        .arg(bool_arg("silent", "Disable progress output"))
        .arg(bool_arg("no-color", "Disable colored static output"))
        .arg(str_arg("path", Some('p'), ".",
            "Folder/repository path to analyze"))
        .arg(bool_arg("debug-trace", "Enable 100% trace sampling for debugging"))
        .arg(str_arg("cpuprofile", None, "", "Write CPU profile to file"))
        .arg(str_arg("heapprofile", None, "", "Write heap profile to file"))
        .arg(int_arg("limit", 0, "Limit number of commits to analyze (0 = no limit)"))
        .arg(bool_arg("first-parent", "Follow only first parent of merge commits"))
        .arg(bool_arg("head", "Analyze only HEAD commit"))
        .arg(str_arg("since", None, "",
            "Only analyze commits after this time (e.g., '24h', '2024-01-01', RFC3339)"))
        .arg(int_arg("workers", 0, "Number of parallel workers (0 = use CPU count)"))
        .arg(int_arg("static-workers", 0,
            "Number of parallel static analysis workers (0 = min(CPU count, 8))"))
        // registerExclusionFlags
        .arg(bool_arg("include-vendored",
            "Re-include vendored dependencies (detected by enry / Linguist) in analysis. \
Default: exclude vendor/, node_modules/, third_party/, testdata/, minified bundles, etc."))
        .arg(bool_arg("include-generated",
            "Re-include auto-generated files in analysis. \
Default: exclude *.pb.go, zz_generated_*.go, *_pb2.py, *.min.js, and any file whose \
first 512 bytes contain a generated-file marker (\"DO NOT EDIT\", \"Code generated\", etc.)."))
        .arg(str_slice_arg("extra-excluded-prefixes", None,
            "Additional UNIX path prefixes to exclude on top of enry heuristics (e.g. \
\".venv/,target/,build/\"). Applies to both static and history phases."))
        .arg(bool_arg("per-file",
            "Include per-file breakdowns and summary statistics in static output")
            .short('F'))
        .arg(int_arg("buffer-size", 0, "Size of internal pipeline channels (0 = workers*2)"))
        .arg(int_arg("commit-batch-size", 0, "Commits per processing batch (0 = default 100)"))
        .arg(str_arg("blob-cache-size", None, "",
            "Max blob cache size (e.g., '256MB', '1GB'; empty = default 1GB)"))
        .arg(int_arg("diff-cache-size", 0, "Max diff cache entries (0 = default 10000)"))
        .arg(str_arg("blob-arena-size", None, "",
            "Memory arena size for blob loading (e.g., '4MB'; empty = default 4MB)"))
        .arg(str_arg("memory-budget", None, "",
            "Memory budget for auto-tuning (e.g., '512MB', '2GB')"))
        .arg(int_arg("max-changes-per-commit", 0,
            "Skip commits whose tree diff exceeds this many changes (0 = default 10000). \
Commits over the cap are silently dropped from history, which can desync \
burndown's tracked state for affected files. Raise on monorepos with \
legitimate large commits (Pods updates, generated code dumps)."))
        // registerPersistenceFlags (tri-state checkpoint/resume default true)
        .arg(tristate_bool_arg("checkpoint", true,
            "Enable checkpointing for crash recovery"))
        .arg(str_arg("checkpoint-dir", None, "",
            "Checkpoint directory (default: ~/.codefang/checkpoints)"))
        .arg(tristate_bool_arg("resume", true,
            "Resume from checkpoint if available"))
        .arg(bool_arg("clear-checkpoint", "Clear existing checkpoint before run"))
        .arg(str_arg("cache-dir", None, "",
            "Incremental analysis cache directory (skip already-processed commits)"))
        .arg(bool_arg("no-cache", "Force full re-analysis, overwriting any existing cache"))
        .arg(str_arg("config", None, "",
            "Configuration file path (default: .codefang.yaml in CWD or $HOME)"))
        .arg(bool_arg("list-analyzers", "List all available analyzer IDs and exit"))
        .arg(str_arg("diagnostics-addr", None, "",
            "Start diagnostics HTTP server (health/metrics) at this address (e.g., :6060)"))
        .arg(str_arg("output", Some('o'), "",
            "Output directory for plot HTML files (required with --format plot)"))
        .arg(bool_arg("keep-store",
            "Keep temp ReportStore directory after rendering (with --format plot)"))
        .arg(str_arg("tmp-dir", None, "",
            "Directory for temporary spill files (default: system temp)"))
}

/// Marks the two legacy exclusion flags as deprecated and hidden, with the exact
/// Go deprecation messages (Go `markDeprecatedExclusionFlags`). clap has no
/// first-class "deprecated" attribute, so we register them hidden and surface
/// the message through [`deprecated_flag_message`] for the caller to emit when
/// the flag is used (mirroring cobra's deprecation warning behavior).
#[must_use]
pub fn mark_deprecated_exclusion_flags(cmd: Command) -> Command {
    cmd.arg(
        bool_arg("skip-blacklist", "DEPRECATED")
            .hide(true),
    )
    .arg(
        str_slice_arg("blacklisted-prefixes", None, "DEPRECATED")
            .hide(true),
    )
}

/// Returns the exact cobra deprecation message for a deprecated flag, or `None`
/// if the flag is not deprecated. Messages copied verbatim from
/// `markDeprecatedExclusionFlags` (`run.go`).
#[must_use]
pub fn deprecated_flag_message(flag: &str) -> Option<&'static str> {
    match flag {
        "skip-blacklist" => Some(
            "use --include-vendored=false and --include-generated=false \
(the new defaults). See CHANGELOG for migration.",
        ),
        "blacklisted-prefixes" => Some(
            "use --extra-excluded-prefixes; the old flag name is preserved \
for back-compat but will be removed in the next minor release.",
        ),
        _ => None,
    }
}

/// Registers one clap flag per configuration option exposed by `providers`,
/// deduplicating by flag name. Mirrors Go `registerAnalyzerFlags` +
/// `registerConfigFlag`: options whose declared kind does not match their
/// default value's type are skipped (Go's failed type assertion path).
#[must_use]
pub fn register_analyzer_flags(mut cmd: Command, providers: &[&dyn ConfigOptionProvider]) -> Command {
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

    for provider in providers {
        for opt in provider.list_configuration_options() {
            if !seen.insert(opt.flag.clone()) {
                continue;
            }
            if let Some(arg) = config_flag_arg(&opt) {
                cmd = cmd.arg(arg);
            }
        }
    }

    cmd
}

/// Builds the clap [`Arg`] for a single configuration option, or `None` when the
/// option's declared kind and default-value type disagree (Go's
/// `registerConfigFlag` skips via a failed `Default.(T)` assertion).
fn config_flag_arg(opt: &ConfigurationOption) -> Option<Arg> {
    // The workspace pins clap with `default-features = false` and no `string`
    // feature, so `Arg::new` / `.long` / `.default_value` need `&'static str`,
    // not owned `String`. Analyzer flag names/help/defaults are built once at
    // startup, so leaking them to `'static` is bounded and intentional.
    let flag: &'static str = string_to_static_leak(&opt.flag);
    let help: &'static str = string_to_static_leak(&opt.description);

    let arg = match (opt.option_type, &opt.default) {
        // cobra registers a BoolConfigurationOption as `Flags().Bool` — a
        // value-less boolean flag (`--anonymize`, no `<value>` placeholder in
        // help). `require_equals(true)` makes clap render `--anonymize[=<..>]`,
        // the `[=<` form the cli-surface extractor reads as value-less, while
        // still accepting an explicit `--anonymize=true/false` like cobra.
        (ConfigurationOptionType::Bool, DefaultValue::Bool(v)) => Arg::new(flag)
            .long(flag)
            .help(help)
            .action(ArgAction::Set)
            .default_value(if *v { "true" } else { "false" })
            .default_missing_value("true")
            .value_parser(clap::value_parser!(bool))
            .require_equals(true)
            .num_args(0..=1),
        (ConfigurationOptionType::Int, DefaultValue::Int(v)) => Arg::new(flag)
            .long(flag)
            .help(help)
            .action(ArgAction::Set)
            .default_value(string_to_static_leak(&v.to_string()))
            .value_parser(clap::value_parser!(i64)),
        (ConfigurationOptionType::String, DefaultValue::String(v))
        | (ConfigurationOptionType::Path, DefaultValue::Path(v)) => Arg::new(flag)
            .long(flag)
            .help(help)
            .action(ArgAction::Set)
            .default_value(string_to_static_leak(v)),
        (ConfigurationOptionType::Strings, DefaultValue::Strings(v)) => {
            let mut a = str_slice_arg(flag, None, help);
            if !v.is_empty() {
                // clap's multi-value default via a single comma-joined string;
                // value_delimiter(',') splits it back into the slice, matching
                // cobra's StringSlice default.
                a = a.default_value(string_to_static_leak(&v.join(",")));
            }
            a
        }
        (ConfigurationOptionType::Float, DefaultValue::Float(v)) => Arg::new(flag)
            .long(flag)
            .help(help)
            .action(ArgAction::Set)
            .default_value(string_to_static_leak(&format_go_float_default(*v)))
            .value_parser(clap::value_parser!(f64)),
        // Kind/default-type mismatch -> skipped (Go failed type assertion).
        _ => return None,
    };
    Some(arg)
}

/// Formats an f64 default for clap's string-based `default_value`. The runtime
/// value parsing uses `f64`, so this only needs to round-trip; report-bytes
/// float formatting is handled by the go-compat encoder elsewhere.
fn format_go_float_default(v: f64) -> String {
    // Use Rust's shortest round-trippable representation; clap re-parses to f64.
    let s = format!("{v}");
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        // Keep a decimal point so the value is unambiguously a float string.
        format!("{s}.0")
    }
}

/// `runtime.NumCPU()` analogue — the per-machine goroutine-count default the Go
/// analyzers derive their `*-goroutines` defaults from (`runtime.NumCPU()` and
/// `max(NumCPU()/4, 1)`). The live Go binary bakes the host CPU count into its
/// `--help` defaults, so the Rust surface must compute the same number for the
/// cli-surface comparison (and for runtime parity) to match.
fn num_cpu() -> i64 {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1) as i64
}

/// The default analyzer configuration options: the FULL Go analyzer option set
/// produced by `registerAnalyzerFlags` walking `buildPipeline(nil)` Core +
/// Leaves (every analyzer's `ListConfigurationOptions`). Each entry mirrors the
/// Go `ConfigurationOption` (Name / Flag / Type / Default / Description) so the
/// dynamic clap registration in [`register_analyzer_flags`] reproduces cobra's
/// flag surface byte-for-byte, and every value lands in the parsed matches that
/// `cf_commands::run` threads into the handlers via [`crate::pipeline::RunContext`].
///
/// The `*-goroutines` defaults are derived from [`num_cpu`] exactly as Go derives
/// them from `runtime.NumCPU()`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn default_analyzer_options() -> Vec<ConfigurationOption> {
    let ncpu = num_cpu();
    let uast_goroutines = (ncpu / 4).max(1);
    vec![
        // --- plumbing/tree_diff ---
        ConfigurationOption {
            name: "TreeDiff.FilteredRegexes".into(),
            flag: "whitelist".into(),
            description: "Whitelist regexp to determine which files to analyze.".into(),
            option_type: ConfigurationOptionType::String,
            default: DefaultValue::String(String::new()),
        },
        ConfigurationOption {
            name: "TreeDiff.Languages".into(),
            flag: "languages".into(),
            description: "Restrict analysis to these languages (comma-separated; 'all' for no filter)".into(),
            option_type: ConfigurationOptionType::Strings,
            default: DefaultValue::Strings(Vec::new()),
        },
        // --- plumbing/ticks ---
        ConfigurationOption {
            name: "TicksSinceStart.TickSize".into(),
            flag: "tick-size".into(),
            description: "How long each 'tick' represents in hours.".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(24),
        },
        // --- plumbing/identity ---
        ConfigurationOption {
            name: "IdentityDetector.PeopleDictPath".into(),
            flag: "people-dict".into(),
            description: "Path to the file with developer -> name|email associations.".into(),
            option_type: ConfigurationOptionType::Path,
            default: DefaultValue::Path(String::new()),
        },
        ConfigurationOption {
            name: "IdentityDetector.ExactSignatures".into(),
            flag: "exact-signatures".into(),
            description: "Disable separate name/email matching. This will lead to considerably more identities and should not be normally used.".into(),
            option_type: ConfigurationOptionType::Bool,
            default: DefaultValue::Bool(false),
        },
        // --- plumbing/blob_cache ---
        ConfigurationOption {
            name: "BlobCache.FailOnMissingSubmodules".into(),
            flag: "fail-on-missing-submodules".into(),
            description: "Specifies whether to panic if any referenced submodule does not exist in .gitmodules and thus the corresponding Git object cannot be loaded. Override this if you want to ensure that your repository is integral.".into(),
            option_type: ConfigurationOptionType::Bool,
            default: DefaultValue::Bool(false),
        },
        ConfigurationOption {
            name: "BlobCache.Goroutines".into(),
            flag: "blob-cache-goroutines".into(),
            description: "Number of goroutines to use for parallel blob loading.".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(ncpu),
        },
        // --- plumbing/file_diff ---
        ConfigurationOption {
            name: "FileDiff.NoCleanup".into(),
            flag: "no-diff-cleanup".into(),
            description: "Do not apply additional heuristics to improve diffs.".into(),
            option_type: ConfigurationOptionType::Bool,
            default: DefaultValue::Bool(false),
        },
        ConfigurationOption {
            name: "FileDiff.WhitespaceIgnore".into(),
            flag: "no-diff-whitespace".into(),
            description: "Ignore whitespace when computing diffs.".into(),
            option_type: ConfigurationOptionType::Bool,
            default: DefaultValue::Bool(false),
        },
        ConfigurationOption {
            name: "FileDiff.Timeout".into(),
            flag: "diff-timeout".into(),
            description: "Maximum time in milliseconds a single diff calculation may elapse.".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(1000),
        },
        ConfigurationOption {
            name: "FileDiff.Goroutines".into(),
            flag: "diff-goroutines".into(),
            description: "Number of goroutines to use for diff calculation.".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(ncpu),
        },
        // --- plumbing/uast ---
        ConfigurationOption {
            name: "UASTChanges.Goroutines".into(),
            flag: "uast-changes-goroutines".into(),
            description: "Number of goroutines to use for parallel UAST parsing (fallback when pipeline is not available).".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(uast_goroutines),
        },
        // --- burndown ---
        ConfigurationOption {
            name: "Burndown.Granularity".into(),
            flag: "granularity".into(),
            description: "How many time ticks there are in a single band.".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(30),
        },
        ConfigurationOption {
            name: "Burndown.Sampling".into(),
            flag: "sampling".into(),
            description: "How frequently to record the state in time ticks.".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(30),
        },
        ConfigurationOption {
            name: "Burndown.TrackFiles".into(),
            flag: "burndown-files".into(),
            description: "Record detailed statistics per each file.".into(),
            option_type: ConfigurationOptionType::Bool,
            default: DefaultValue::Bool(false),
        },
        ConfigurationOption {
            name: "Burndown.TrackPeople".into(),
            flag: "burndown-people".into(),
            description: "Record detailed statistics per each developer.".into(),
            option_type: ConfigurationOptionType::Bool,
            default: DefaultValue::Bool(false),
        },
        ConfigurationOption {
            name: "Burndown.HibernationThreshold".into(),
            flag: "burndown-hibernation-threshold".into(),
            description: "The minimum size for the allocated memory in each branch to be compressed.".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(1000),
        },
        ConfigurationOption {
            name: "Burndown.HibernationOnDisk".into(),
            flag: "burndown-hibernation-disk".into(),
            description: "If true, save hibernated state to disk (no-op with default treap timeline).".into(),
            option_type: ConfigurationOptionType::Bool,
            default: DefaultValue::Bool(true),
        },
        ConfigurationOption {
            name: "Burndown.HibernationDirectory".into(),
            flag: "burndown-hibernation-dir".into(),
            description: "Temporary directory for hibernated state (no-op with default treap timeline).".into(),
            option_type: ConfigurationOptionType::Path,
            default: DefaultValue::Path(String::new()),
        },
        ConfigurationOption {
            name: "Burndown.Debug".into(),
            flag: "burndown-debug".into(),
            description: "Validate the trees at each step.".into(),
            option_type: ConfigurationOptionType::Bool,
            default: DefaultValue::Bool(false),
        },
        ConfigurationOption {
            name: "Burndown.Goroutines".into(),
            flag: "burndown-goroutines".into(),
            description: "Number of goroutines to use for parallel processing.".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(ncpu),
        },
        // --- anomaly ---
        ConfigurationOption {
            name: "TemporalAnomaly.WindowSize".into(),
            flag: "anomaly-window".into(),
            description: "Sliding window size in ticks for computing rolling statistics.".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(20),
        },
        // --- devs ---
        ConfigurationOption {
            name: "Devs.ConsiderEmptyCommits".into(),
            flag: "empty-commits".into(),
            description: "Take into account empty commits such as trivial merges.".into(),
            option_type: ConfigurationOptionType::Bool,
            default: DefaultValue::Bool(false),
        },
        ConfigurationOption {
            name: "Devs.Anonymize".into(),
            flag: "anonymize".into(),
            description: "Anonymize developer names in output (e.g., Developer-A, Developer-B).".into(),
            option_type: ConfigurationOptionType::Bool,
            default: DefaultValue::Bool(false),
        },
        // --- shotness ---
        ConfigurationOption {
            name: "Shotness.DSLStruct".into(),
            flag: "shotness-dsl-struct".into(),
            description: "UAST DSL query to use for filtering the nodes.".into(),
            option_type: ConfigurationOptionType::String,
            default: DefaultValue::String("filter(.roles has \"Function\")".into()),
        },
        ConfigurationOption {
            name: "Shotness.DSLName".into(),
            flag: "shotness-dsl-name".into(),
            description: "UAST DSL query to determine the names of the filtered nodes.".into(),
            option_type: ConfigurationOptionType::String,
            default: DefaultValue::String(".props.name".into()),
        },
        // --- typos ---
        ConfigurationOption {
            name: "TyposDatasetBuilder.MaximumAllowedDistance".into(),
            flag: "typos-max-distance".into(),
            description: "Maximum Levenshtein distance between two identifiers to consider them a typo-fix pair.".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(4),
        },
        // --- sentiment ---
        ConfigurationOption {
            name: "CommentSentiment.MinLength".into(),
            flag: "min-comment-len".into(),
            description: "Minimum length of the comment to be analyzed.".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(20),
        },
    ]
}

/// Build the `render <store-dir>` [`clap::Command`]. Mirrors Go
/// `buildRenderCommand`: `Args = ExactArgs(1)` and the `--output`/`-o` flag.
#[must_use]
pub fn build_render_command() -> Command {
    Command::new(render_consts::USE)
        .about(render_consts::SHORT)
        .arg(
            Arg::new("store-dir")
                .value_name("store-dir")
                .required(true)
                .num_args(1)
                .help("Directory containing stored analysis results"),
        )
        .arg(
            Arg::new(render_consts::OUTPUT_FLAG)
                .long(render_consts::OUTPUT_FLAG)
                .short(render_consts::OUTPUT_SHORT)
                .action(ArgAction::Set)
                .default_value("")
                .help(render_consts::OUTPUT_USAGE),
        )
}

/// Build the `completion` [`clap::Command`] with the four shell subcommands
/// (`bash`, `fish`, `powershell`, `zsh`), mirroring the command cobra
/// auto-registers (`rootCmd.AddCommand`'s implicit completion command). The help
/// strings are copied verbatim from cobra's generated completion command so the
/// cli-surface comparison matches the live Go binary.
#[must_use]
pub fn build_completion_command() -> Command {
    Command::new("completion")
        .about("Generate the autocompletion script for the specified shell")
        .long_about(
            "Generate the autocompletion script for codefang for the specified shell.\n\
See each sub-command's help for details on how to use the generated script.",
        )
        .subcommand_required(false)
        // cobra's auto-registered completion command, run with NO shell argument,
        // prints its long help and exits 0; `arg_required_else_help` would make
        // clap exit 1, so we let the bare invocation flow to the handler, which
        // prints help and returns 0 (matching Go).
        .subcommand(
            Command::new("bash")
                .about("Generate the autocompletion script for bash")
                .arg(completion_no_descriptions_flag()),
        )
        .subcommand(
            Command::new("fish")
                .about("Generate the autocompletion script for fish")
                .arg(completion_no_descriptions_flag()),
        )
        .subcommand(
            Command::new("powershell")
                .about("Generate the autocompletion script for powershell")
                .arg(completion_no_descriptions_flag()),
        )
        .subcommand(
            Command::new("zsh")
                .about("Generate the autocompletion script for zsh")
                .arg(completion_no_descriptions_flag()),
        )
}

/// The `--no-descriptions` toggle cobra adds to every generated shell-completion
/// subcommand (bash/fish/powershell/zsh). A value-less boolean flag, matching
/// cobra's surface ("disable completion descriptions").
fn completion_no_descriptions_flag() -> Arg {
    bool_arg(
        "no-descriptions",
        "disable completion descriptions",
    )
}

// --- small flag-builder helpers (cobra-style: long name + short + default) ---

/// A boolean `--flag` (clap `SetTrue`), default `false`. Mirrors cobra
/// `Flags().BoolVar(..., false, help)`.
fn bool_arg(name: &'static str, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .help(help)
        .action(ArgAction::SetTrue)
        // cobra pflag accepts repeated flags (last occurrence wins); clap
        // errors on duplicates unless the arg overrides itself.
        .overrides_with(name)
}

/// A tri-state boolean flag, the Rust analogue of cobra's `Flags().Bool(..)`
/// read through `Changed` (`parseBoolFlag`): default `default`, the bare
/// `--flag` form sets `true`, `--flag=false` negates, and the caller
/// distinguishes "set by user" from "default" via
/// [`clap::ArgMatches::value_source`] (cobra `Changed`).
///
/// `require_equals(true)` is load-bearing for surface parity: a cobra `Bool`
/// flag advertises NO value placeholder in `--help` (it is `takes_value=false`
/// in the cli-surface comparison), and `require_equals` makes clap render
/// `--flag[=<flag>]` — the `[=<` form the surface extractor reads as
/// value-less — instead of the `[<flag>]` (value-taking) form a plain
/// `num_args(0..=1)` would print. The explicit `--flag=false` value still
/// parses, matching cobra (the analyzer matrix passes `--checkpoint=false`).
fn tristate_bool_arg(name: &'static str, default: bool, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .help(help)
        .action(ArgAction::Set)
        .num_args(0..=1)
        .require_equals(true)
        .default_value(if default { "true" } else { "false" })
        .default_missing_value("true")
        .value_parser(clap::value_parser!(bool))
        // cobra pflag last-wins on repeated flags.
        .overrides_with(name)
}

/// A string `--flag` (optionally with a short), with the given default. Mirrors
/// cobra `Flags().StringVar`/`StringVarP`.
fn str_arg(name: &'static str, short: Option<char>, default: &'static str, help: &'static str) -> Arg {
    let mut a = Arg::new(name)
        .long(name)
        .help(help)
        .action(ArgAction::Set)
        .default_value(default)
        // cobra pflag last-wins on repeated flags.
        .overrides_with(name);
    if let Some(s) = short {
        a = a.short(s);
    }
    a
}

/// An integer `--flag` with the given default. Mirrors cobra `Flags().IntVar`.
///
/// The workspace pins clap with `default-features = false` (no `string`
/// feature), so `default_value` needs `&'static str`; the small decimal default
/// is leaked to `'static` (bounded — built once at startup).
fn int_arg(name: &'static str, default: i64, help: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .help(help)
        .action(ArgAction::Set)
        .default_value(string_to_static_leak(&default.to_string()))
        .value_parser(clap::value_parser!(i64))
        // cobra pflag last-wins on repeated flags.
        .overrides_with(name)
}

/// A repeatable / comma-separated string-slice `--flag`. Mirrors cobra
/// `Flags().StringSliceVar`/`StringSliceVarP` (comma-split, append).
fn str_slice_arg(name: &'static str, short: Option<char>, help: &'static str) -> Arg {
    let mut a = Arg::new(name)
        .long(name)
        .help(help)
        .action(ArgAction::Append)
        .value_delimiter(',');
    if let Some(s) = short {
        a = a.short(s);
    }
    a
}

/// Leaks a `String` to `&'static str` so a dynamically-named clap arg can be
/// constructed. Used only for the small, fixed set of analyzer flag names built
/// once at startup; the leak is bounded and intentional (clap's `Arg::new`
/// historically wants `'static`; modern clap accepts owned ids, but the
/// `str_slice_arg` helper is `&'static`-typed for the literal flags).
fn string_to_static_leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run() -> Command {
        build_run_command()
    }

    // --- TestRunCommandConfig_Defaults ---
    #[test]
    fn run_defaults_format_gogc_ballast() {
        let m = run()
            .try_get_matches_from(["run"])
            .expect("defaults parse");
        assert_eq!(m.get_one::<String>("format").unwrap(), "json");
        assert_eq!(*m.get_one::<i64>("gogc").unwrap(), 0);
        assert_eq!(m.get_one::<String>("ballast-size").unwrap(), "0");
    }

    // --- TestRunCommandConfig_PathFlag ---
    #[test]
    fn run_default_path_is_dot() {
        let m = run().try_get_matches_from(["run"]).unwrap();
        assert_eq!(m.get_one::<String>("path").unwrap(), ".");
    }

    // --- TestRunCommandConfig_OutputFlag ---
    #[test]
    fn run_default_output_is_empty() {
        let m = run().try_get_matches_from(["run"]).unwrap();
        assert_eq!(m.get_one::<String>("output").unwrap(), "");
    }

    // --- TestRunCommandConfig_NDJSONFlag ---
    #[test]
    fn run_default_ndjson_false() {
        let m = run().try_get_matches_from(["run"]).unwrap();
        assert!(!m.get_flag("ndjson"));
    }

    // --- TestRunCommandConfig_PerFileFlag ---
    #[test]
    fn run_default_per_file_false_with_short_flag() {
        let m = run().try_get_matches_from(["run", "-F"]).unwrap();
        assert!(m.get_flag("per-file"));
        let m2 = run().try_get_matches_from(["run"]).unwrap();
        assert!(!m2.get_flag("per-file"));
    }

    // --- TestRunCommandConfig_ExclusionFlags ---
    #[test]
    fn run_default_exclusion_flags_false() {
        let m = run().try_get_matches_from(["run"]).unwrap();
        assert!(!m.get_flag("include-vendored"));
        assert!(!m.get_flag("include-generated"));
    }

    // --- TestRunCommandConfig_PersistenceFlags (defaults true) ---
    #[test]
    fn run_persistence_defaults_true() {
        let m = run().try_get_matches_from(["run"]).unwrap();
        assert!(*m.get_one::<bool>("checkpoint").unwrap());
        assert!(*m.get_one::<bool>("resume").unwrap());
    }

    // --- Tri-state: --checkpoint=false sets value AND marks as user-supplied ---
    #[test]
    fn checkpoint_tristate_distinguishes_user_set() {
        let m = run()
            .try_get_matches_from(["run", "--checkpoint=false"])
            .unwrap();
        assert!(!*m.get_one::<bool>("checkpoint").unwrap());
        assert_eq!(
            m.value_source("checkpoint"),
            Some(clap::parser::ValueSource::CommandLine),
            "explicit --checkpoint should report CommandLine (Go Changed==true)"
        );
        // Not supplied -> DefaultValue (Go Changed==false -> parseBoolFlag nil).
        let m2 = run().try_get_matches_from(["run"]).unwrap();
        assert_eq!(
            m2.value_source("checkpoint"),
            Some(clap::parser::ValueSource::DefaultValue)
        );
    }

    // --- TestRunCommandConfig_AnalyzerFlags (burndown --granularity registered) ---
    // Go renamed --burndown-granularity to --granularity (the cobra flag is
    // "granularity"); the dynamic registration must expose that name.
    #[test]
    fn run_registers_dynamic_burndown_flag() {
        let m = run()
            .try_get_matches_from(["run", "--granularity", "60"])
            .unwrap();
        assert_eq!(*m.get_one::<i64>("granularity").unwrap(), 60);
        // The old name must be gone (Go dropped it).
        assert!(
            run().try_get_matches_from(["run", "--burndown-granularity", "60"]).is_err(),
            "--burndown-granularity must no longer exist (renamed to --granularity)"
        );
    }

    // The new analyzer flags are registered with Go-exact names/defaults.
    #[test]
    fn run_registers_new_analyzer_flags() {
        let m = run().try_get_matches_from(["run"]).unwrap();
        assert_eq!(*m.get_one::<i64>("anomaly-window").unwrap(), 20);
        assert_eq!(*m.get_one::<i64>("sampling").unwrap(), 30);
        assert_eq!(*m.get_one::<i64>("tick-size").unwrap(), 24);
        assert_eq!(*m.get_one::<i64>("typos-max-distance").unwrap(), 4);
        assert_eq!(*m.get_one::<i64>("min-comment-len").unwrap(), 20);
        assert_eq!(*m.get_one::<i64>("diff-timeout").unwrap(), 1000);
        assert_eq!(*m.get_one::<i64>("burndown-hibernation-threshold").unwrap(), 1000);
        assert!(*m.get_one::<bool>("burndown-hibernation-disk").unwrap());
        assert!(!m.get_flag("anonymize"));
        assert_eq!(m.get_one::<String>("shotness-dsl-name").unwrap(), ".props.name");
        assert_eq!(m.get_one::<String>("people-dict").unwrap(), "");
        assert_eq!(m.get_one::<String>("whitelist").unwrap(), "");
    }

    // --checkpoint=false still parses (the analyzer matrix passes it) and reports
    // a CommandLine source (Go Changed==true) — surface is value-less.
    #[test]
    fn checkpoint_value_form_still_parses() {
        let m = run().try_get_matches_from(["run", "--checkpoint=false"]).unwrap();
        assert!(!*m.get_one::<bool>("checkpoint").unwrap());
        assert_eq!(
            m.value_source("checkpoint"),
            Some(clap::parser::ValueSource::CommandLine)
        );
    }

    // --- TestRunCommandConfig_DryRunOmitted ---
    #[test]
    fn run_has_no_dry_run_flag() {
        let parsed = run().try_get_matches_from(["run", "--dry-run"]);
        assert!(parsed.is_err(), "--dry-run must not exist");
    }

    // --- analyzers short -a, comma-split ---
    #[test]
    fn analyzers_short_and_comma_split() {
        let m = run()
            .try_get_matches_from(["run", "-a", "history/anomaly,history/devs"])
            .unwrap();
        let vals: Vec<&String> = m.get_many::<String>("analyzers").unwrap().collect();
        assert_eq!(vals, vec!["history/anomaly", "history/devs"]);
    }

    // --- positional [path] (MaximumNArgs(1)) ---
    #[test]
    fn run_accepts_one_positional_path() {
        let m = run().try_get_matches_from(["run", "/repo"]).unwrap();
        assert_eq!(
            m.get_one::<String>("path-positional").map(String::as_str),
            Some("/repo")
        );
    }

    #[test]
    fn run_rejects_two_positionals() {
        let parsed = run().try_get_matches_from(["run", "/a", "/b"]);
        assert!(parsed.is_err(), "MaximumNArgs(1) must reject 2 positionals");
    }

    // --- deprecated flags exist (hidden) with exact messages ---
    #[test]
    fn deprecated_flags_present_and_hidden() {
        let m = run()
            .try_get_matches_from(["run", "--skip-blacklist"])
            .unwrap();
        assert!(m.get_flag("skip-blacklist"));
    }

    #[test]
    fn deprecated_messages_are_verbatim() {
        assert_eq!(
            deprecated_flag_message("skip-blacklist"),
            Some(
                "use --include-vendored=false and --include-generated=false \
(the new defaults). See CHANGELOG for migration."
            )
        );
        assert_eq!(
            deprecated_flag_message("blacklisted-prefixes"),
            Some(
                "use --extra-excluded-prefixes; the old flag name is preserved \
for back-compat but will be removed in the next minor release."
            )
        );
        assert_eq!(deprecated_flag_message("nope"), None);
    }

    // --- register_analyzer_flags dedupes by flag name ---
    #[test]
    fn dynamic_flags_dedupe_by_flag_name() {
        let a = vec![ConfigurationOption {
            name: "Granularity".into(),
            flag: "g".into(),
            description: "first".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(1),
        }];
        let b = vec![ConfigurationOption {
            name: "Other".into(),
            flag: "g".into(), // duplicate flag -> ignored (Go registeredFlags set)
            description: "second".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(2),
        }];
        let pa: &dyn ConfigOptionProvider = &a;
        let pb: &dyn ConfigOptionProvider = &b;
        let cmd = register_analyzer_flags(Command::new("t"), &[pa, pb]);
        let m = cmd.try_get_matches_from(["t"]).unwrap();
        // First registration wins -> default 1.
        assert_eq!(*m.get_one::<i64>("g").unwrap(), 1);
    }

    // --- kind/default mismatch is skipped ---
    #[test]
    fn dynamic_flag_kind_mismatch_skipped() {
        let bad = vec![ConfigurationOption {
            name: "X".into(),
            flag: "x".into(),
            description: "mismatch".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::String("nope".into()),
        }];
        let p: &dyn ConfigOptionProvider = &bad;
        let cmd = register_analyzer_flags(Command::new("t"), &[p]);
        // The flag must not have been added.
        let parsed = cmd.try_get_matches_from(["t", "--x", "1"]);
        assert!(parsed.is_err());
    }

    // --- render command: ExactArgs(1) + --output/-o ---
    #[test]
    fn render_requires_store_dir_and_has_output() {
        let m = build_render_command()
            .try_get_matches_from(["render", "/store", "-o", "/out"])
            .unwrap();
        assert_eq!(m.get_one::<String>("store-dir").unwrap(), "/store");
        assert_eq!(m.get_one::<String>("output").unwrap(), "/out");
    }

    #[test]
    fn render_rejects_missing_store_dir() {
        let parsed = build_render_command().try_get_matches_from(["render"]);
        assert!(parsed.is_err(), "render requires exactly one store-dir arg");
    }

    #[test]
    fn render_rejects_two_args() {
        let parsed = build_render_command().try_get_matches_from(["render", "/a", "/b"]);
        assert!(parsed.is_err(), "render ExactArgs(1)");
    }

    #[test]
    fn render_default_output_empty() {
        let m = build_render_command()
            .try_get_matches_from(["render", "/store"])
            .unwrap();
        assert_eq!(m.get_one::<String>("output").unwrap(), "");
    }
}
