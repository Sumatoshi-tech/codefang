//! The `run` and `completion` clap command trees.
//!
//! Built with clap's **builder API** (not derive) so every flag's long name,
//! short, default, and help string is declared explicitly. The help text is a
//! contract: every flag it lists is read by the pipeline (an accepted-and-
//! ignored flag is a bug), and `site/guide/cli-reference.md` documents each
//! one.
//!
//! # `run` flags
//!
//! [`build_run_command`] registers the `run` command's literal flags (the
//! selection / format / window / exclusion / configuration blocks), then the
//! dynamic per-analyzer flags ([`register_analyzer_flags`]).
//!
//! # Configuration defaults
//!
//! [`CONFIG_KEY_FLAGS`] maps every `cf-config` key to the `run` flag it
//! defaults, and [`with_config_defaults`] installs the configured values as
//! those flags' defaults. clap then resolves the precedence itself: a value
//! given on the command line wins over the configured default, which wins
//! over the compiled one.
//!
//! # Dynamic per-analyzer flags
//!
//! [`register_analyzer_flags`] walks the supplied
//! [`crate::registry::ConfigOptionProvider`]s, dedupes by flag name, and adds one
//! clap flag per [`cf_pipeline::ConfigurationOption`] keyed by its
//! [`cf_pipeline::ConfigurationOptionType`].

use cf_pipeline::{ConfigurationOption, ConfigurationOptionType, DefaultValue};
use clap::{Arg, ArgAction, Command};

use crate::formats::FORMAT_JSON;
use crate::registry::ConfigOptionProvider;

/// Build the `run [path]` [`clap::Command`] with every literal flag and the
/// dynamic per-analyzer flags from `providers`.
///
/// `providers` supplies the analyzer configuration options (the reference implementation's
/// `buildPipeline(nil)` Core + Leaves). In the default (non-`runtime`) build the
/// caller passes [`default_analyzer_options`]; in the `runtime` build it passes
/// the real analyzer providers.
#[must_use]
pub fn build_run_command_with(providers: &[&dyn ConfigOptionProvider]) -> Command {
    let mut cmd = Command::new("run")
        .about("Run static and history analyzers")
        .long_about("Run selected static and history analyzers.")
        .after_help(run_analyzers_after_help())
        // Args: cobra.MaximumNArgs(1) -> 0..=1 positional [path].
        .arg(
            Arg::new("path-positional")
                .value_name("path")
                .num_args(0..=1)
                .help("Folder/repository path to analyze (overrides --path)"),
        );

    cmd = add_literal_run_flags(cmd);
    register_analyzer_flags(cmd, providers)
}

/// The `run --help` trailer: the available analyzer ids (grouped by phase) plus
/// runnable examples.
///
/// This INTENTIONALLY DIVERGES from the reference `run` help — cobra lists no
/// analyzer ids there, expecting discovery via `--list-analyzers`. It is an
/// additive usability block: clap renders it after the `Options:` section, and
/// the cli-surface parity gate compares the STRUCTURED flag/command/positional
/// surface (not help prose), so this trailer adds no surface row. The ids are
/// read from the live [`crate::handlers::default_registry`] so the list can
/// never drift from what `--list-analyzers` prints or what dispatch accepts.
#[must_use]
fn run_analyzers_after_help() -> String {
    use std::fmt::Write as _;

    let ids = crate::handlers::default_registry().ids();
    let group = |prefix: &str| {
        ids.iter()
            .filter_map(|id| id.strip_prefix(prefix))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mut s = String::new();
    s.push_str("Available analyzers (pass to -a; omit -a to run them all):\n");
    let _ = writeln!(s, "  static/    {}", group("static/"));
    let _ = writeln!(s, "  history/   {}", group("history/"));
    s.push_str("  (full ids, one per line: codefang run --list-analyzers)\n\n");
    s.push_str("Examples:\n");
    s.push_str("  codefang run .                            # all analyzers, combined JSON\n");
    s.push_str("  codefang run -a static/complexity .       # one analyzer\n");
    s.push_str("  codefang run -a 'history/*' --limit 50 .  # all history analyzers, capped\n");
    s.push_str("  codefang run -a '*' --format yaml .       # everything, as YAML");
    s
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

/// Adds the literal `run` flags in declaration order: analyzer selection and
/// output format, the commit window (`--limit`, `--first-parent`, `--head`,
/// `--since`), the exclusion flags, `--max-changes-per-commit`, `--chunk-size`,
/// `--config`, `--list-analyzers` and `--output`.
#[allow(clippy::too_many_lines)]
fn add_literal_run_flags(cmd: Command) -> Command {
    cmd.arg(str_slice_arg(
        "analyzers",
        Some('a'),
        "Analyzer IDs or glob patterns (example: static/complexity,history/*,*)",
    ))
    .arg(str_arg(
        "format",
        None,
        FORMAT_JSON,
        "Output format: json, yaml, plot, bin, timeseries, ndjson, text, compact",
    ))
    .arg(bool_arg(
        "ndjson",
        "With --format timeseries: emit one JSON line per commit (NDJSON)",
    ))
    .arg(bool_arg("no-color", "Disable colored text/compact output"))
    .arg(str_arg(
        "path",
        Some('p'),
        ".",
        "Folder/repository path to analyze",
    ))
    .arg(int_arg(
        "limit",
        0,
        "Analyze only the N newest commits (0 = no limit)",
    ))
    .arg(bool_arg(
        "first-parent",
        "Follow only first parent of merge commits",
    ))
    .arg(bool_arg("head", "Analyze only HEAD commit"))
    .arg(str_arg(
        "since",
        None,
        "",
        "Only analyze commits authored at/after this time or revision (e.g., '24h', '2024-01-01', RFC3339, 'v1.2.0')",
    ))
    // Exclusion flags
    .arg(bool_arg(
        "include-vendored",
        "Re-include vendored dependencies (detected by enry / Linguist) in analysis. \
Default: exclude vendor/, node_modules/, third_party/, testdata/, minified bundles, etc.",
    ))
    .arg(bool_arg(
        "include-generated",
        "Re-include auto-generated files in analysis. \
Default: exclude *.pb.go, zz_generated_*.go, *_pb2.py, *.min.js, and any file whose \
first 512 bytes contain a generated-file marker (\"DO NOT EDIT\", \"Code generated\", etc.).",
    ))
    .arg(str_slice_arg(
        "extra-excluded-prefixes",
        None,
        "Additional UNIX path prefixes to exclude on top of enry heuristics (e.g. \
\".venv/,target/,build/\"). Applies to both static and history phases.",
    ))
    .arg(
        bool_arg(
            "per-file",
            "Include per-file breakdowns and summary statistics in static output",
        )
        .short('F'),
    )
    .arg(int_arg(
        "max-changes-per-commit",
        0,
        "Skip commits whose tree diff exceeds this many changes (0 = default 10000). \
Dropped commits are invisible to every analyzer; burndown re-inserts an affected \
file whole the next time it is touched. Raise on monorepos with legitimate large \
commits (Pods updates, generated code dumps).",
    ))
    .arg(
        Arg::new("chunk-size")
            .long("chunk-size")
            .help(
                "Commits per history chunk; per-commit work for at most this many commits is \
held in memory at once. Overrides the chunk size --memory-budget plans",
            )
            .action(ArgAction::Set)
            .value_parser(clap::value_parser!(i64))
            // cobra pflag last-wins on repeated flags.
            .overrides_with("chunk-size"),
    )
    .arg(
        Arg::new("memory-budget")
            .long("memory-budget")
            .value_name("SIZE")
            .help(
                "Memory a history run plans its chunk sizes and worker counts against, e.g. \
512MB, 2GB, 1.5GiB (default: half the physical RAM, at most 2 GiB; minimum 256 MiB)",
            )
            .action(ArgAction::Set)
            .overrides_with("memory-budget"),
    )
    .arg(str_arg(
        "config",
        None,
        "",
        "Configuration file (default: ./.codefang.yaml, else $HOME/.codefang.yaml); \
its keys default the matching flags",
    ))
    .arg(bool_arg(
        "list-analyzers",
        "List all available analyzer IDs and exit",
    ))
    .arg(str_arg(
        "output",
        Some('o'),
        "",
        "Write the report to this file instead of stdout; with --format plot, the \
output directory for the HTML pages (required)",
    ))
}

/// The `run` flag each `cf-config` key defaults, in [`cf_config::KEYS`]
/// order. Every key has exactly one flag (pinned by a test).
pub const CONFIG_KEY_FLAGS: &[(&str, &str)] = &[
    (cf_config::keys::ANALYZERS, "analyzers"),
    (
        cf_config::keys::MAX_CHANGES_PER_COMMIT,
        "max-changes-per-commit",
    ),
    (cf_config::keys::MEMORY_BUDGET, "memory-budget"),
    (cf_config::keys::CHUNK_SIZE, "chunk-size"),
    (cf_config::keys::TICK_SIZE, "tick-size"),
    (cf_config::keys::BURNDOWN_GRANULARITY, "granularity"),
    (cf_config::keys::BURNDOWN_SAMPLING, "sampling"),
    (cf_config::keys::BURNDOWN_TRACK_FILES, "burndown-files"),
    (cf_config::keys::BURNDOWN_TRACK_PEOPLE, "burndown-people"),
    (
        cf_config::keys::DEVS_CONSIDER_EMPTY_COMMITS,
        "empty-commits",
    ),
    (cf_config::keys::DEVS_ANONYMIZE, "anonymize"),
    (
        cf_config::keys::SENTIMENT_MIN_COMMENT_LENGTH,
        "min-comment-len",
    ),
    (cf_config::keys::SHOTNESS_DSL_STRUCT, "shotness-dsl-struct"),
    (cf_config::keys::SHOTNESS_DSL_NAME, "shotness-dsl-name"),
    (cf_config::keys::SHOTNESS_KEEP_ENDED, "shotness-keep-ended"),
    (cf_config::keys::TYPOS_MAX_DISTANCE, "typos-max-distance"),
    (cf_config::keys::ANOMALY_THRESHOLD, "anomaly-threshold"),
    (cf_config::keys::ANOMALY_WINDOW_SIZE, "anomaly-window"),
];

/// The `run` flag the configuration key `key` defaults.
///
/// ```
/// use cf_commands::flags::flag_for_config_key;
///
/// assert_eq!(flag_for_config_key("history.anomaly.window_size"), Some("anomaly-window"));
/// assert_eq!(flag_for_config_key("pipeline.workers"), None);
/// ```
#[must_use]
pub fn flag_for_config_key(key: &str) -> Option<&'static str> {
    CONFIG_KEY_FLAGS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, flag)| *flag)
}

/// Installs every configured `setting` as the default of its `run` flag in
/// the root `codefang` command `cmd`. A value given on the command line still
/// wins: clap only falls back to a default when the flag is absent.
#[must_use]
pub fn with_config_defaults(cmd: Command, settings: &[cf_config::Setting]) -> Command {
    let defaults: Vec<(&'static str, Vec<&'static str>)> = settings
        .iter()
        .filter_map(|s| {
            let flag = flag_for_config_key(s.key)?;
            let values = s
                .value
                .to_args()
                .iter()
                .map(|v| string_to_static_leak(v))
                .collect();
            Some((flag, values))
        })
        .collect();
    if defaults.is_empty() {
        return cmd;
    }
    cmd.mut_subcommand("run", |mut run| {
        for (flag, values) in defaults {
            run = run.mut_arg(flag, |a| a.default_values(values));
        }
        run
    })
}

/// Registers one clap flag per configuration option exposed by `providers`,
/// deduplicating by flag name. Mirrors the reference `registerAnalyzerFlags` +
/// `registerConfigFlag`: options whose declared kind does not match their
/// default value's type are skipped (the reference implementation's failed type assertion path).
#[must_use]
pub fn register_analyzer_flags(
    mut cmd: Command,
    providers: &[&dyn ConfigOptionProvider],
) -> Command {
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
/// option's declared kind and default-value type disagree (the reference implementation's
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
        // Kind/default-type mismatch -> skipped (reference: failed type assertion).
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

/// The default analyzer configuration options: every per-analyzer option
/// registered as a `run` flag (Name / Flag / Type / Default / Description).
/// Every value lands in the parsed matches that `cf_commands::run` threads
/// into the handlers via [`crate::pipeline::RunContext`].
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn default_analyzer_options() -> Vec<ConfigurationOption> {
    vec![
        // --- shared change filter ---
        ConfigurationOption {
            name: "TreeDiff.Languages".into(),
            flag: "languages".into(),
            description: "Restrict analysis to these languages (comma-separated; 'all' for no filter)".into(),
            option_type: ConfigurationOptionType::Strings,
            default: DefaultValue::Strings(Vec::new()),
        },
        // --- shared ticks ---
        ConfigurationOption {
            name: "TicksSinceStart.TickSize".into(),
            flag: "tick-size".into(),
            description: "How long each 'tick' represents in hours.".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(24),
        },
        // --- shared identity ---
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
        // --- anomaly ---
        ConfigurationOption {
            name: "TemporalAnomaly.WindowSize".into(),
            flag: "anomaly-window".into(),
            description: "Trailing window size in ticks for the rolling mean and standard deviation (>= 2).".into(),
            option_type: ConfigurationOptionType::Int,
            default: DefaultValue::Int(20),
        },
        ConfigurationOption {
            name: "TemporalAnomaly.Threshold".into(),
            flag: "anomaly-threshold".into(),
            description: "Z-score threshold, in standard deviations, above which a tick is flagged (> 0).".into(),
            option_type: ConfigurationOptionType::Float,
            default: DefaultValue::Float(2.0),
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
            default: DefaultValue::String("rfilter(.roles has \"Function\")".into()),
        },
        ConfigurationOption {
            name: "Shotness.DSLName".into(),
            flag: "shotness-dsl-name".into(),
            description: "UAST DSL query to determine the names of the filtered nodes.".into(),
            option_type: ConfigurationOptionType::String,
            default: DefaultValue::String(".props.name".into()),
        },
        ConfigurationOption {
            name: "Shotness.KeepEnded".into(),
            flag: "shotness-keep-ended".into(),
            description: "Keep entities whose declaration or file was removed in the report, \
marked ended, with their counts and couplings (default: drop them)."
                .into(),
            option_type: ConfigurationOptionType::Bool,
            default: DefaultValue::Bool(false),
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

/// Build the `completion` [`clap::Command`] with the four shell subcommands
/// (`bash`, `fish`, `powershell`, `zsh`), mirroring the command cobra
/// auto-registers (`rootCmd.AddCommand`'s implicit completion command). The help
/// strings are copied verbatim from cobra's generated completion command so the
/// cli-surface comparison matches the live reference binary.
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
        // prints help and returns 0 (matching the reference implementation).
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
    bool_arg("no-descriptions", "disable completion descriptions")
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

/// A string `--flag` (optionally with a short), with the given default. Mirrors
/// cobra `Flags().StringVar`/`StringVarP`.
fn str_arg(
    name: &'static str,
    short: Option<char>,
    default: &'static str,
    help: &'static str,
) -> Arg {
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

    #[test]
    fn run_defaults_format_json() {
        let m = run().try_get_matches_from(["run"]).expect("defaults parse");
        assert_eq!(m.get_one::<String>("format").unwrap(), "json");
    }

    /// Flags with no effect on any report are not accepted: an accepted-and-
    /// ignored flag is a bug.
    #[test]
    fn inert_flags_are_rejected() {
        for flag in [
            "--workers=1",
            "--static-workers=1",
            "--input=x",
            "--input-format=json",
            "--gogc=1",
            "--ballast-size=1",
            "--cpuprofile=x",
            "--heapprofile=x",
            "--buffer-size=1",
            "--commit-batch-size=1",
            "--blob-cache-size=1",
            "--diff-cache-size=1",
            "--blob-arena-size=1",
            "--diagnostics-addr=:1",
            "--keep-store",
            "--tmp-dir=x",
            "--whitelist=x",
            "--fail-on-missing-submodules",
            "--blob-cache-goroutines=1",
            "--no-diff-cleanup",
            "--no-diff-whitespace",
            "--diff-timeout=1",
            "--diff-goroutines=1",
            "--uast-changes-goroutines=1",
            "--burndown-hibernation-threshold=1",
            "--burndown-hibernation-disk",
            "--burndown-hibernation-dir=x",
            "--burndown-debug",
            "--burndown-goroutines=1",
            // No checkpoint store, incremental cache or trace exporter is
            // linked into the binary.
            "--checkpoint",
            "--checkpoint=false",
            "--checkpoint-dir=x",
            "--resume",
            "--resume=false",
            "--clear-checkpoint",
            "--cache-dir=x",
            "--no-cache",
            "--debug-trace",
            // The hidden deprecated spellings were accepted and ignored.
            "--skip-blacklist",
            "--blacklisted-prefixes=x",
        ] {
            assert!(
                run().try_get_matches_from(["run", flag]).is_err(),
                "{flag} must not be accepted"
            );
        }
    }

    #[test]
    fn run_default_path_is_dot() {
        let m = run().try_get_matches_from(["run"]).unwrap();
        assert_eq!(m.get_one::<String>("path").unwrap(), ".");
    }

    #[test]
    fn run_default_output_is_empty() {
        let m = run().try_get_matches_from(["run"]).unwrap();
        assert_eq!(m.get_one::<String>("output").unwrap(), "");
    }

    #[test]
    fn run_default_ndjson_false() {
        let m = run().try_get_matches_from(["run"]).unwrap();
        assert!(!m.get_flag("ndjson"));
    }

    #[test]
    fn run_default_per_file_false_with_short_flag() {
        let m = run().try_get_matches_from(["run", "-F"]).unwrap();
        assert!(m.get_flag("per-file"));
        let m2 = run().try_get_matches_from(["run"]).unwrap();
        assert!(!m2.get_flag("per-file"));
    }

    #[test]
    fn run_default_exclusion_flags_false() {
        let m = run().try_get_matches_from(["run"]).unwrap();
        assert!(!m.get_flag("include-vendored"));
        assert!(!m.get_flag("include-generated"));
    }

    /// `--granularity` is the flag name (the Go-era `--burndown-granularity`
    /// spelling is gone).
    #[test]
    fn run_registers_dynamic_burndown_flag() {
        let m = run()
            .try_get_matches_from(["run", "--granularity", "60"])
            .unwrap();
        assert_eq!(*m.get_one::<i64>("granularity").unwrap(), 60);
        assert!(
            run()
                .try_get_matches_from(["run", "--burndown-granularity", "60"])
                .is_err(),
            "--burndown-granularity must no longer exist (renamed to --granularity)"
        );
    }

    #[test]
    fn run_registers_analyzer_flags() {
        let m = run().try_get_matches_from(["run"]).unwrap();
        assert_eq!(*m.get_one::<i64>("anomaly-window").unwrap(), 20);
        assert!((*m.get_one::<f64>("anomaly-threshold").unwrap() - 2.0).abs() < f64::EPSILON);
        assert_eq!(*m.get_one::<i64>("sampling").unwrap(), 30);
        assert_eq!(*m.get_one::<i64>("tick-size").unwrap(), 24);
        assert_eq!(*m.get_one::<i64>("typos-max-distance").unwrap(), 4);
        assert_eq!(*m.get_one::<i64>("min-comment-len").unwrap(), 20);
        assert!(!m.get_flag("anonymize"));
        assert!(!*m.get_one::<bool>("exact-signatures").unwrap());
        assert_eq!(
            m.get_one::<String>("shotness-dsl-struct").unwrap(),
            "rfilter(.roles has \"Function\")"
        );
        assert_eq!(
            m.get_one::<String>("shotness-dsl-name").unwrap(),
            ".props.name"
        );
        assert_eq!(m.get_one::<String>("people-dict").unwrap(), "");
    }

    #[test]
    fn run_has_no_dry_run_flag() {
        let parsed = run().try_get_matches_from(["run", "--dry-run"]);
        assert!(parsed.is_err(), "--dry-run must not exist");
    }

    #[test]
    fn analyzers_short_and_comma_split() {
        let m = run()
            .try_get_matches_from(["run", "-a", "history/anomaly,history/devs"])
            .unwrap();
        let vals: Vec<&String> = m.get_many::<String>("analyzers").unwrap().collect();
        assert_eq!(vals, vec!["history/anomaly", "history/devs"]);
    }

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
        assert!(parsed.is_err(), "at most one positional path");
    }

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
            flag: "g".into(),
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
        let parsed = cmd.try_get_matches_from(["t", "--x", "1"]);
        assert!(parsed.is_err());
    }

    /// Every supported configuration key defaults exactly one existing `run`
    /// flag, in key order.
    #[test]
    fn every_config_key_maps_to_a_run_flag() {
        let keys: Vec<&str> = CONFIG_KEY_FLAGS.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, cf_config::KEYS);
        let cmd = run();
        for (key, flag) in CONFIG_KEY_FLAGS {
            assert!(
                cmd.get_arguments().any(|a| a.get_id() == *flag),
                "{key} maps to the missing flag --{flag}"
            );
        }
    }

    /// The root command with `settings` installed, parsed from `args`.
    fn root_with(settings: &[cf_config::Setting], args: &[&str]) -> clap::ArgMatches {
        let root = Command::new("codefang").subcommand(run());
        let m = with_config_defaults(root, settings)
            .try_get_matches_from(args)
            .unwrap();
        m.subcommand_matches("run").unwrap().clone()
    }

    #[test]
    fn configured_values_become_flag_defaults_and_explicit_flags_win() {
        use cf_config::{keys, Setting, Value};
        let settings = [
            Setting {
                key: keys::ANALYZERS,
                value: Value::List(vec!["history/devs".into(), "history/anomaly".into()]),
            },
            Setting {
                key: keys::ANOMALY_WINDOW_SIZE,
                value: Value::Int(3),
            },
            Setting {
                key: keys::ANOMALY_THRESHOLD,
                value: Value::Float(1.5),
            },
            Setting {
                key: keys::DEVS_ANONYMIZE,
                value: Value::Bool(true),
            },
            Setting {
                key: keys::SHOTNESS_DSL_NAME,
                value: Value::Text(".props.id".into()),
            },
        ];
        let m = root_with(&settings, &["codefang", "run"]);
        let ids: Vec<&String> = m.get_many::<String>("analyzers").unwrap().collect();
        assert_eq!(ids, vec!["history/devs", "history/anomaly"]);
        assert_eq!(*m.get_one::<i64>("anomaly-window").unwrap(), 3);
        assert!((*m.get_one::<f64>("anomaly-threshold").unwrap() - 1.5).abs() < f64::EPSILON);
        assert!(*m.get_one::<bool>("anonymize").unwrap());
        assert_eq!(
            m.get_one::<String>("shotness-dsl-name").unwrap(),
            ".props.id"
        );
        assert_eq!(
            m.value_source("anomaly-window"),
            Some(clap::parser::ValueSource::DefaultValue)
        );

        let m = root_with(
            &settings,
            &[
                "codefang",
                "run",
                "-a",
                "static/complexity",
                "--anomaly-window",
                "9",
                "--anonymize=false",
            ],
        );
        let ids: Vec<&String> = m.get_many::<String>("analyzers").unwrap().collect();
        assert_eq!(ids, vec!["static/complexity"]);
        assert_eq!(*m.get_one::<i64>("anomaly-window").unwrap(), 9);
        assert!(!*m.get_one::<bool>("anonymize").unwrap());
        assert_eq!(
            m.value_source("anomaly-window"),
            Some(clap::parser::ValueSource::CommandLine)
        );
    }

    #[test]
    fn no_settings_leave_the_command_unchanged() {
        let m = root_with(&[], &["codefang", "run"]);
        assert_eq!(*m.get_one::<i64>("anomaly-window").unwrap(), 20);
        assert!(m.get_many::<String>("analyzers").is_none());
    }
}
