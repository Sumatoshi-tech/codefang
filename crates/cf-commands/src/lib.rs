//! `cf-commands` — command wiring and analyzer registration for the `codefang`
//! binary.
//!
//! This is the aggregation point: it registers every analyzer, builds the
//! `run` / `version` / `completion` command tree with all of `run`'s literal
//! flags and the dynamic per-analyzer flags, layers the configuration file
//! under the flags, drives the analysis pipeline, and routes report
//! serialization through the shared report-format crates (cf-gojson /
//! cf-goyaml) — never raw `serde`.
//!
//! The command tree is built with clap's **builder API** (not derive) so
//! command and flag declaration order, help strings and defaults are explicit.
//!
//! The major pieces:
//!
//! - [`formats`] — format constants and validation (`normalize_format`,
//!   `validate_format`, `validate_universal_format`, `resolve_formats` and the
//!   `--ndjson` + `--format timeseries` → `timeseries+ndjson` composition).
//! - [`version`] — the `version` subcommand output.
//! - [`flags`] — the `run` / `completion` clap command trees: every literal
//!   `run` flag, the dynamic per-analyzer flags driven by
//!   [`cf_pipeline::ConfigurationOption`], and the configuration-key → flag
//!   map ([`flags::CONFIG_KEY_FLAGS`]).
//! - [`handlers`] / [`pipeline`] — the run execution path: the shared run
//!   preflight, analyzer dispatch, the static folder walk and shared history
//!   revwalk, and per-format report serialization.
//!
//! # Configuration
//!
//! `codefang run` loads `--config <file>` (else `./.codefang.yaml`, else
//! `$HOME/.codefang.yaml`) plus the `CODEFANG_*` environment through
//! `cf-config`, installs every configured key as the default of its flag and
//! parses the command line again: an explicit flag > the environment > the
//! file > the compiled default. Unsupported keys print one
//! `warning: ignoring unsupported config keys: …` line.
//!
//! # Errors
//!
//! Every user error prints `Error: <message>` to stderr and exits 1, with
//! the message naming the cause ([`pipeline::HandlerError`]): `path not
//! found: <p>`, `cannot open repository: <p>`, `unsupported format: <f>`,
//! `format <f> is not supported for <analyzer>`, `invalid --limit: …`,
//! `cannot resolve --since "<v>": …`, `unknown analyzer id: <id>`,
//! `cannot read --config <p>: …`, `invalid config <p>: …`,
//! `cannot write --output <p>: …`.
//!
//! # Example: resolve the run-command output format
//!
//! ```
//! use cf_commands::{normalize_format, resolve_formats};
//!
//! assert_eq!(normalize_format(" BIN "), "binary");
//!
//! let (static_fmt, history_fmt) = resolve_formats("bin", true, false).unwrap();
//! assert_eq!(static_fmt, "binary");
//! assert_eq!(history_fmt, "");
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod flags;
pub mod formats;
pub mod handlers;
pub mod mcp;
pub mod pipeline;
pub mod registry;
pub mod version;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub use flags::{build_completion_command, build_run_command};
// `FORMAT_JSON` is part of the public format-constant API (used by the binary
// and tests); rustc emits a spurious unused-import warning for the re-export
// because it is also referenced internally via `crate::formats`.
#[allow(unused_imports)]
pub use formats::{
    apply_ndjson_modifier, normalize_format, resolve_formats, validate_format,
    validate_universal_format, FormatError, FORMAT_BINARY, FORMAT_BIN_ALIAS, FORMAT_COMPACT,
    FORMAT_JSON, FORMAT_NDJSON, FORMAT_PLOT, FORMAT_TEXT, FORMAT_TIMESERIES,
    FORMAT_TIMESERIES_NDJSON, FORMAT_YAML,
};
pub use pipeline::HandlerError;
pub use registry::{ConfigOptionProvider, RegistrationError};
pub use version::{build_version_command, version_output};

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-commands";

/// The `--verbose` flag id.
const FLAG_VERBOSE: &str = "verbose";
/// The `--quiet` flag id (`--silent` is its alias).
const FLAG_QUIET: &str = "quiet";

/// Build the top-level `codefang` [`clap::Command`] with the `run`,
/// `version` and `completion` subcommands wired in declaration order.
///
/// The root carries the global flags `--verbose`/`-v`, `--quiet`/`-q`
/// (alias `--silent`) and `--profile` (all default `false`).
#[must_use]
pub fn build_codefang_command() -> clap::Command {
    clap::Command::new("codefang")
        .about("Codefang Code Analysis - Unified code analysis tool")
        .after_help(
            "Getting started:\n  \
             codefang run .                       # analyze the current repo with every analyzer\n  \
             codefang run -a static/complexity .  # run a single analyzer\n  \
             codefang run --list-analyzers        # list every analyzer id\n  \
             codefang run --help                  # all run flags + available analyzers\n\n\
             Run 'codefang <command> --help' for details on any command.",
        )
        .arg(
            clap::Arg::new(FLAG_VERBOSE)
                .long(FLAG_VERBOSE)
                .short('v')
                .global(true)
                .action(clap::ArgAction::SetTrue)
                .help(
                    "Print run details on stderr: the configuration file used and the \
                     resolved analyzer ids",
                ),
        )
        .arg(
            clap::Arg::new(FLAG_QUIET)
                .long(FLAG_QUIET)
                .short('q')
                .visible_alias("silent")
                .global(true)
                .action(clap::ArgAction::SetTrue)
                .help("Suppress the progress lines on stderr (warnings and errors still print)"),
        )
        .arg(
            clap::Arg::new("profile")
                .long("profile")
                .global(true)
                .action(clap::ArgAction::SetTrue)
                .help(
                    "Log RSS and thread count to stderr at start and every 2 s; snapshot \
                     /proc/self/maps into the temp directory at start and on RSS spikes",
                ),
        )
        .subcommand(build_run_command())
        .subcommand(build_version_command())
        .subcommand(build_completion_command())
        .subcommand(mcp::build_mcp_command())
}

/// Parses `args` with `cmd`. clap prints help / usage / errors itself; a
/// usage error (bad flag / unknown command / missing arg) is exit code 1, a
/// displayed help or version is 0.
fn parse(cmd: clap::Command, args: &[OsString]) -> Result<clap::ArgMatches, i32> {
    cmd.try_get_matches_from(args).map_err(|e| {
        e.print().ok();
        i32::from(e.use_stderr())
    })
}

/// The single `codefang` entry point: build the command tree, parse `args`
/// (which MUST start with the program name, like `std::env::args`), and dispatch
/// `run` / `version` / `completion` through the general pipeline + registry.
/// Returns the process exit code.
#[must_use]
pub fn run<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let matches = match parse(build_codefang_command(), &args) {
        Ok(m) => m,
        Err(code) => return code,
    };

    install_signal_handlers();

    match matches.subcommand() {
        Some(("version", _)) => {
            print!("{}", version_output());
            0
        }
        Some(("run", sub)) => run_command(&args, sub),
        Some(("completion", sub)) => completion_subcommand(sub),
        Some(("mcp", _)) => mcp::mcp_subcommand(),
        _ => {
            build_codefang_command().print_help().ok();
            println!();
            0
        }
    }
}

/// `codefang run`: `--list-analyzers` answers without a configuration;
/// otherwise the configuration is loaded, installed as the flag defaults, the
/// command line parsed again, and the run dispatched.
fn run_command(args: &[OsString], first: &clap::ArgMatches) -> i32 {
    if first.get_flag("list-analyzers") {
        use std::io::Write as _;
        let mut out = std::io::stdout();
        for id in handlers::default_registry().ids() {
            let _ = writeln!(out, "{id}");
        }
        return 0;
    }
    if let Err(message) = check_verbosity(first) {
        return fail(&message);
    }
    let loaded = match load_config(first) {
        Ok(loaded) => loaded,
        Err(message) => return fail(&message),
    };
    if !loaded.unsupported_keys.is_empty() {
        eprintln!(
            "warning: ignoring unsupported config keys: {}",
            loaded.unsupported_keys.join(", ")
        );
    }
    let cmd = flags::with_config_defaults(build_codefang_command(), &loaded.config.settings());
    let matches = match parse(cmd, args) {
        Ok(m) => m,
        Err(code) => return code,
    };
    match matches.subcommand_matches("run") {
        Some(sub) => run_subcommand(sub, loaded.path.as_deref()),
        None => fail(&"the configured command line no longer selects `run`"),
    }
}

/// `--verbose` asks for more stderr, `--quiet` for less: both at once is a
/// usage error. (clap's `conflicts_with` misses a pair split across the root
/// and the subcommand, `codefang -v run -q`.)
fn check_verbosity(sub: &clap::ArgMatches) -> Result<(), &'static str> {
    if sub.get_flag(FLAG_VERBOSE) && sub.get_flag(FLAG_QUIET) {
        Err("--verbose and --quiet cannot be used together")
    } else {
        Ok(())
    }
}

/// Loads the configuration `--config` names (or discovers), with the
/// process environment as the `CODEFANG_*` layer.
fn load_config(sub: &clap::ArgMatches) -> Result<cf_config::Loaded, String> {
    let explicit = sub
        .get_one::<String>("config")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from);
    cf_config::load(
        explicit.as_deref(),
        &cf_config::default_search_dirs(),
        |name| std::env::var(name).ok(),
    )
    .map_err(|e| match (&e, &explicit) {
        (cf_config::LoadError::Read { path, cause }, Some(_)) => {
            format!("cannot read --config {}: {cause}", path.display())
        }
        _ => e.to_string(),
    })
}

/// Installs SIGINT/SIGTERM handlers for cooperative cancellation: the first
/// signal sets the shared cancel flag (the static folder walks poll it and
/// bail out), a second signal falls through to the default disposition and
/// kills the process — the standard double-Ctrl-C escape hatch.
fn install_signal_handlers() {
    for sig in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        let flag = std::sync::Arc::clone(handlers::cancel_flag());
        // Order matters: conditional-default first, so it only fires once the
        // flag from the plain register (below) is already set.
        let _ = signal_hook::flag::register_conditional_default(sig, std::sync::Arc::clone(&flag));
        let _ = signal_hook::flag::register(sig, flag);
    }
}

/// Exit code for a run interrupted by SIGINT/SIGTERM (128 + SIGINT), matching
/// the shell convention for signal death.
const EXIT_INTERRUPTED: i32 = 130;

/// Exit code for a closed downstream pipe (128 + SIGPIPE).
const EXIT_BROKEN_PIPE: i32 = 141;

/// Where a run writes its report: stdout, or the `-o` file (every format but
/// `plot`, whose `-o` is the page directory).
enum ReportSink {
    /// The process stdout.
    Stdout,
    /// The `-o` file, created (truncated) on the first write.
    File {
        /// The path as given.
        path: String,
        /// The open file once created.
        file: Option<std::fs::File>,
    },
}

impl ReportSink {
    /// The sink `-o` selects for a run whose format is (`plot`) or is not
    /// the plot directory renderer.
    fn new(sub: &clap::ArgMatches, plot: bool) -> Self {
        match sub.get_one::<String>("output").filter(|o| !o.is_empty()) {
            Some(path) if !plot => Self::File {
                path: path.clone(),
                file: None,
            },
            _ => Self::Stdout,
        }
    }

    /// Writes `bytes` and returns the exit code (0 on success). An
    /// interrupted run never emits a truncated report that looks complete
    /// (exit 130); a closed downstream pipe (`codefang run … | head`) exits
    /// with the SIGPIPE convention (141). Writing nothing still creates the
    /// `-o` file, so a successful run always leaves its report behind.
    fn emit(&mut self, bytes: &[u8]) -> i32 {
        use std::io::Write as _;
        if handlers::run_cancelled() {
            eprintln!("Error: interrupted");
            std::process::exit(EXIT_INTERRUPTED);
        }
        match self {
            Self::Stdout => match std::io::stdout().write_all(bytes) {
                Ok(()) => 0,
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                    std::process::exit(EXIT_BROKEN_PIPE)
                }
                Err(e) => fail(&format!("cannot write stdout: {e}")),
            },
            Self::File { path, file } => {
                if file.is_none() {
                    match std::fs::File::create(path.as_str()) {
                        Ok(f) => *file = Some(f),
                        Err(e) => return fail(&format!("cannot write --output {path}: {e}")),
                    }
                }
                match file.as_mut().map(|f| f.write_all(bytes)) {
                    Some(Err(e)) => fail(&format!("cannot write --output {path}: {e}")),
                    _ => 0,
                }
            }
        }
    }
}

/// Dispatches `codefang completion <shell>`; generates the shell-completion
/// script to stdout (`--no-descriptions` drops every help text from it).
/// With no shell subcommand it prints the completion command's help (rc 0).
fn completion_subcommand(sub: &clap::ArgMatches) -> i32 {
    use clap_complete::Shell;

    let (shell, shell_matches) = match sub.subcommand() {
        Some(("bash", m)) => (Shell::Bash, m),
        Some(("fish", m)) => (Shell::Fish, m),
        Some(("powershell", m)) => (Shell::PowerShell, m),
        Some(("zsh", m)) => (Shell::Zsh, m),
        _ => {
            build_completion_command().print_help().ok();
            println!();
            return 0;
        }
    };

    let mut cmd = build_codefang_command();
    if shell_matches.get_flag("no-descriptions") {
        cmd = without_descriptions(cmd);
    }
    clap_complete::generate(shell, &mut cmd, "codefang", &mut std::io::stdout());
    0
}

/// `cmd` with every command and argument description removed, recursively.
fn without_descriptions(cmd: clap::Command) -> clap::Command {
    cmd.about(None)
        .long_about(None)
        .mut_args(|a| a.help(None).long_help(None))
        .mut_subcommands(without_descriptions)
}

/// Stderr progress for a `run`: a startup line announces the resolved
/// selection immediately, and the Drop impl prints the elapsed wall time on
/// EVERY return path — so even a long static walk is visibly working rather
/// than looking hung. Suppressed by `--quiet` / `--silent`. The report bytes
/// themselves go to stdout (or `-o`), so progress never touches the output
/// contract.
struct RunProgress {
    start: std::time::Instant,
    quiet: bool,
}

impl RunProgress {
    fn begin(quiet: bool, static_n: usize, history_n: usize, output_format: &str) -> Self {
        if !quiet {
            eprintln!(
                "progress: resolved analyzers: static={static_n} history={history_n} output_format={output_format}"
            );
        }
        Self {
            start: std::time::Instant::now(),
            quiet,
        }
    }
}

impl Drop for RunProgress {
    fn drop(&mut self) {
        if self.quiet {
            return;
        }
        // Surface silent per-file skips (unreadable / oversized / parse
        // failure): without this line a permissions failure across half the
        // tree yields a plausible-looking but incomplete report.
        let skipped = handlers::skipped_file_count();
        if skipped > 0 {
            eprintln!("progress: skipped {skipped} unreadable/oversized/unparseable files");
        }
        let d = self.start.elapsed();
        let ms = d.as_millis();
        if ms < 1000 {
            eprintln!("progress: run finished in {ms}ms");
        } else {
            eprintln!("progress: run finished in {:.1}s", d.as_secs_f64());
        }
    }
}

/// The `--verbose` detail lines: the configuration file used and the
/// resolved analyzer ids (static phase first).
fn verbose_lines(config_path: Option<&Path>, static_ids: &[String], history_ids: &[String]) {
    let config = config_path.map_or_else(|| "none".to_owned(), |p| p.display().to_string());
    eprintln!("progress: config file: {config}");
    let ids: Vec<&str> = static_ids
        .iter()
        .chain(history_ids)
        .map(String::as_str)
        .collect();
    eprintln!("progress: analyzers: {}", ids.join(","));
}

/// Prints `Error: <message>` and returns the failure exit code.
fn fail(message: &dyn std::fmt::Display) -> i32 {
    eprintln!("Error: {message}");
    1
}

/// True when `pat` carries a glob metacharacter.
fn is_glob(pat: &str) -> bool {
    pat.contains(['*', '?', '['])
}

/// Dispatches `codefang run` through the general pipeline. Emits the report
/// bytes to stdout (or the `-o` file); on any user error it prints
/// `Error: <msg>` and exits 1. `config_path` is the configuration file the
/// flag defaults came from, for `--verbose`.
fn run_subcommand(sub: &clap::ArgMatches, config_path: Option<&Path>) -> i32 {
    // `--no-color` is the flag form of the `NO_COLOR` convention every text /
    // compact renderer reads.
    if sub.get_flag("no-color") {
        std::env::set_var("NO_COLOR", "1");
    }

    let ctx = pipeline::RunContext::from_matches(sub);
    let ids = ctx.analyzer_ids();
    let raw_format = ctx.raw_format();
    let analyzer_strs: Vec<&str> = ids.iter().map(String::as_str).collect();

    // Progress bracket: print the resolved selection now, and the elapsed
    // time on Drop — covering every dispatch branch's early return below.
    let quiet = sub.get_flag(FLAG_QUIET);
    let (prog_static, prog_history) = handlers::expand_combined_ids(&analyzer_strs);
    let _progress = RunProgress::begin(
        quiet,
        prog_static.len(),
        prog_history.len(),
        &formats::normalize_format(&raw_format),
    );
    if sub.get_flag(FLAG_VERBOSE) {
        verbose_lines(config_path, &prog_static, &prog_history);
    }

    // ONE validation of the path, the repository, the flags and the format
    // before any walk starts; every failure names its cause.
    if let Err(e) = handlers::preflight(&ctx, &prog_static, &prog_history) {
        return fail(&e);
    }

    if formats::normalize_format(&raw_format) == formats::FORMAT_PLOT {
        return run_plot(&ctx, sub, &analyzer_strs);
    }
    match render_report(&ctx, &analyzer_strs) {
        Ok(bytes) => ReportSink::new(sub, false).emit(&bytes),
        Err(e) => fail(&e),
    }
}

/// Renders a whole `codefang run …` command line in memory: the report bytes
/// the CLI would write, or the error message it would print. Validation is
/// the CLI's (the preflight); nothing is printed. `plot` is not supported
/// (its output is a page directory).
///
/// # Errors
///
/// The message for unparsable arguments, a failed preflight or a failed
/// render.
pub fn render_run(argv: &[&str]) -> Result<Vec<u8>, String> {
    let matches = build_codefang_command()
        .try_get_matches_from(std::iter::once("codefang").chain(argv.iter().copied()))
        .map_err(|e| e.to_string())?;
    let sub = matches
        .subcommand_matches("run")
        .ok_or_else(|| "not a run command".to_owned())?;
    let ctx = pipeline::RunContext::from_matches(sub);
    let ids = ctx.analyzer_ids();
    let analyzer_strs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let (static_ids, history_ids) = handlers::expand_combined_ids(&analyzer_strs);
    handlers::preflight(&ctx, &static_ids, &history_ids).map_err(|e| e.to_string())?;
    if formats::normalize_format(&ctx.raw_format()) == formats::FORMAT_PLOT {
        return Err("format plot is not supported here".to_owned());
    }
    render_report(&ctx, &analyzer_strs)
}

/// Renders the (non-plot) report of the selection into `sink` through the
/// matching path: the multi-analyzer static bin / json / text merges, the
/// unified mixed document, the whole-selection history formats, or the
/// per-id pipeline. Returns the exit code.
fn render_report(ctx: &pipeline::RunContext, analyzer_strs: &[&str]) -> Result<Vec<u8>, String> {
    let registry = handlers::default_registry();
    let raw_format = ctx.raw_format();
    let ids = ctx.analyzer_ids();
    // Validated by the preflight; a static-free selection never reads it.
    let static_filter = handlers::static_filter(ctx).unwrap_or_default();
    let all_static = !analyzer_strs.is_empty()
        && analyzer_strs
            .iter()
            .all(|a| handlers::is_static_id_or_glob(a));

    // A static-only glob with --format bin emits the concatenated
    // registry-ordered per-analyzer CFB1 envelopes.
    if raw_format == "bin" && all_static && analyzer_strs.iter().any(|a| is_glob(a)) {
        if let Some(bytes) = handlers::static_multi_bin(analyzer_strs, &ctx.path, &static_filter) {
            return Ok(bytes);
        }
    }

    // A STATIC-ONLY selection of MORE THAN ONE static analyzer with
    // `--format json` renders ONE merged report (sections in registry order,
    // overall_score the average of the scored sections); `text` renders all
    // selected sections through ONE text render with the executive summary.
    if (raw_format == "json" || raw_format == "text")
        && all_static
        && handlers::static_json_selects_multiple(analyzer_strs)
    {
        let bytes = if raw_format == "json" {
            handlers::static_multi_json(
                analyzer_strs,
                &ctx.path,
                &static_filter,
                ctx.matches.get_flag("per-file"),
            )
        } else {
            handlers::static_multi_text(analyzer_strs, &ctx.path, &static_filter)
        };
        if let Some(bytes) = bytes {
            return Ok(bytes);
        }
    }

    // Mixed static+history selection (both phases non-empty): the unified
    // `codefang.run.v1` document, or the per-phase text / compact /
    // time-series renderings.
    let (combined_static, combined_history) = handlers::expand_combined_ids(analyzer_strs);
    if !combined_static.is_empty() && !combined_history.is_empty() {
        return handlers::render_mixed(ctx, &combined_static, &combined_history, &raw_format)
            .map_err(|e| e.to_string());
    }

    // Expand globs / literal ids to the concrete registry ids the per-id
    // pipeline dispatches: the static phase uses the registry static order;
    // the history phase uses the separate-phase emit order. A purely literal
    // selection keeps its request order.
    let resolved_ids = if analyzer_strs.iter().any(|a| is_glob(a)) {
        let mut v = combined_static;
        v.extend(handlers::expand_history_phase_ids(analyzer_strs));
        if v.is_empty() {
            ids
        } else {
            v
        }
    } else {
        ids
    };

    let all_history = !resolved_ids.is_empty()
        && resolved_ids.iter().all(|id| {
            registry
                .lookup(id)
                .is_some_and(|e| matches!(e.mode, pipeline::Mode::History))
        });
    if all_history {
        if let Some(rendered) = render_history_whole_selection(ctx, &resolved_ids, &raw_format) {
            return rendered;
        }
    }

    pipeline::run_pipeline(&registry, ctx, &resolved_ids)
        .map(|outputs| outputs.into_iter().flat_map(|phase| phase.bytes).collect())
        .map_err(|e| e.to_string())
}

/// `--format plot`: the multi-page HTML renderer. `--output` is required. A
/// static-only selection renders pages + index + the static results
/// report.json; a history-only selection renders the per-flag pages + index +
/// report.json; a MIXED selection runs both phases into the SAME directory
/// and the final index is rebuilt across both page sets.
fn run_plot(ctx: &pipeline::RunContext, sub: &clap::ArgMatches, analyzer_strs: &[&str]) -> i32 {
    let output = sub.get_one::<String>("output").map_or("", String::as_str);
    if output.is_empty() {
        return fail(&"--output flag is required when --format plot");
    }
    // SELECTION-order expansion: pages (and index cards) render in the
    // resolved id order.
    let (plot_static, plot_history) = handlers::expand_selection_ids(analyzer_strs);
    let no_pages = || {
        fail(&format!(
            "no plot pages produced for {}",
            analyzer_strs.join(",")
        ))
    };
    match (plot_static.is_empty(), plot_history.is_empty()) {
        (false, true) => {
            handlers::plot::run_static_plot(ctx, &plot_static, output).unwrap_or_else(no_pages)
        }
        (true, false) => {
            handlers::history_plot::run_history_plot(ctx, &plot_history, output, false)
                .unwrap_or_else(no_pages)
        }
        (false, false) => {
            // Mixed run: static phase first, then the history phase into the
            // same directory.
            match handlers::plot::run_static_plot(ctx, &plot_static, output) {
                Some(0) => {
                    handlers::history_plot::run_history_plot(ctx, &plot_history, output, true)
                        .unwrap_or_else(no_pages)
                }
                Some(code) => code,
                None => no_pages(),
            }
        }
        (true, true) => no_pages(),
    }
}

/// The whole-selection history formats (`text`, `compact`, `ndjson`,
/// `timeseries`, `timeseries --ndjson`), and the one-document rule for a
/// multi-analyzer `json` / `yaml` selection, written into `sink`. Returns
/// `None` when the selection+format is a plain per-analyzer dispatch (handled
/// by the per-id pipeline).
fn render_history_whole_selection(
    ctx: &pipeline::RunContext,
    resolved_ids: &[String],
    raw_format: &str,
) -> Option<Result<Vec<u8>, String>> {
    let normalized = formats::normalize_format(raw_format);
    let history_format = formats::apply_ndjson_modifier(&normalized, ctx.ndjson());

    // Several history analyzers in json/yaml: ONE unified-model document
    // (`{version, metadata, analyzers: [{id, mode, report}]}`), never a
    // concatenation of per-analyzer documents.
    if resolved_ids.len() > 1
        && (history_format == formats::FORMAT_JSON || history_format == formats::FORMAT_YAML)
    {
        return Some(
            handlers::render_combined(ctx, &[], resolved_ids, raw_format)
                .map_err(|e| e.to_string()),
        );
    }

    // Every whole-selection renderer builds its complete output first: a
    // failure writes nothing to stdout.
    let rendered = match history_format.as_str() {
        formats::FORMAT_TEXT => handlers::history_formats::history_text(ctx, resolved_ids),
        formats::FORMAT_COMPACT => handlers::history_formats::history_compact(ctx, resolved_ids),
        formats::FORMAT_NDJSON => handlers::history_formats::history_ndjson(ctx, resolved_ids),
        formats::FORMAT_TIMESERIES => {
            handlers::history_formats::history_timeseries(ctx, resolved_ids, false)
        }
        // `--format timeseries --ndjson`: the merged document's commits, one
        // line per commit.
        formats::FORMAT_TIMESERIES_NDJSON => {
            handlers::history_formats::history_timeseries(ctx, resolved_ids, true)
        }
        _ => return None,
    };
    Some(rendered.map_err(|e| e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_links() {
        assert_eq!(CRATE_NAME, "cf-commands");
    }

    /// A whole `run` command renders in memory: the report bytes, or the
    /// error the CLI would print.
    #[test]
    fn a_run_renders_in_memory() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(
            dir.path().join("m.go"),
            "package m\n\nfunc f(x int) int {\n\tif x > 0 {\n\t\treturn 1\n\t}\n\treturn 0\n}\n",
        )
        .expect("write");
        let path = dir.path().to_str().expect("utf-8");
        let bytes = render_run(&["run", "-a", "static/complexity", "--format", "json", path])
            .expect("render");
        let report: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert!(report.get("sections").is_some(), "{report}");

        let err = render_run(&["run", "-a", "static/nope", path]).expect_err("unknown id");
        assert!(err.contains("nope"), "{err}");
        let err = render_run(&["run", "--format"]).expect_err("bad flags");
        assert!(!err.is_empty());
    }

    #[test]
    fn root_has_subcommands_in_order() {
        let cmd = build_codefang_command();
        let names: Vec<&str> = cmd.get_subcommands().map(clap::Command::get_name).collect();
        assert_eq!(names, vec!["run", "version", "completion", "mcp"]);
    }

    #[test]
    fn root_has_global_flags() {
        let cmd = build_codefang_command();
        let longs: Vec<_> = cmd
            .get_arguments()
            .filter_map(clap::Arg::get_long)
            .collect();
        assert_eq!(longs, vec!["verbose", "quiet", "profile"]);
    }

    #[test]
    fn root_parses_version_subcommand() {
        let cmd = build_codefang_command();
        let m = cmd.try_get_matches_from(["codefang", "version"]).unwrap();
        assert_eq!(m.subcommand_name(), Some("version"));
    }

    /// `render` printed "not available" for every input: it is gone.
    #[test]
    fn render_is_not_a_subcommand() {
        let parsed = build_codefang_command().try_get_matches_from(["codefang", "render", "x"]);
        assert!(parsed.is_err());
    }

    #[test]
    fn silent_is_an_alias_of_quiet_and_reaches_run() {
        for flag in ["--silent", "--quiet", "-q"] {
            let m = build_codefang_command()
                .try_get_matches_from(["codefang", "run", flag])
                .unwrap();
            let run = m.subcommand_matches("run").unwrap();
            assert!(run.get_flag(FLAG_QUIET), "{flag}");
        }
    }

    #[test]
    fn verbose_and_quiet_conflict_wherever_they_are_given() {
        for argv in [
            ["codefang", "-v", "run", "-q"],
            ["codefang", "run", "-v", "--silent"],
            ["codefang", "-q", "-v", "run"],
        ] {
            let m = build_codefang_command().try_get_matches_from(argv).unwrap();
            let run = m.subcommand_matches("run").unwrap();
            assert_eq!(
                check_verbosity(run),
                Err("--verbose and --quiet cannot be used together"),
                "{argv:?}"
            );
        }
        let m = build_codefang_command()
            .try_get_matches_from(["codefang", "run", "-v"])
            .unwrap();
        assert_eq!(
            check_verbosity(m.subcommand_matches("run").unwrap()),
            Ok(())
        );
    }

    #[test]
    fn descriptions_can_be_stripped_from_the_completion_tree() {
        let cmd = without_descriptions(build_codefang_command());
        let run = cmd.find_subcommand("run").unwrap();
        assert!(run.get_about().is_none());
        assert!(run.get_arguments().all(|a| a.get_help().is_none()));
    }
}
