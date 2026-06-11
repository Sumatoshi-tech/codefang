//! `cf-commands` — Cobra→clap command wiring and analyzer registration for the
//! `codefang` binary, the Rust port of Go `cmd/codefang/commands`.
//!
//! This is the **tier-8 aggregation point** (specs/rust-rewrite/DESIGN.md §1
//! tier 8, §4): it registers every analyzer, builds the `run` / `render` /
//! `version` command tree with all of `run`'s literal flags and the dynamic
//! per-analyzer flags, and (in the `runtime` configuration) drives the analysis
//! pipeline and routes report serialization through the shared go-compat
//! serialization crates — never raw `serde`.
//!
//! # CLI parity (DESIGN §4)
//!
//! The command tree is built with clap's **builder API** (not derive) so command
//! and flag declaration order, help strings, defaults, and error wording can be
//! matched to cobra verbatim. The three subcommands wired by Go `main.go` are
//! [`build_run_command`], [`build_render_command`], and [`build_version_command`]
//! (Go `NewRunCommand` / `NewRenderCommand` / `versionCmd`). The Go `mcp`
//! command is `//go:build ignore`; it is mirrored behind the non-default `mcp`
//! Cargo feature and is not built by default.
//!
//! # Port status
//!
//! The self-contained pieces are implemented and unit-tested here against the
//! already-compiling `cf-version` and `cf-pipeline` crates:
//!
//! - [`formats`] — full port of Go `internal/analyzers/analyze/formats.go`
//!   (format constants, `NormalizeFormat`, `ValidateFormat`,
//!   `ValidateUniversalFormat`, plus the `ResolveFormats` / `ResolveInputFormat`
//!   conversion logic and the `--ndjson` + `--format timeseries` →
//!   `timeseries+ndjson` composition). The error string is the exact Go
//!   `unsupported format: <fmt>`.
//! - [`version`] — the `version` subcommand output (`codefang <v> (commit: <c>,
//!   built: <d>)\n`), via [`cf_version`].
//! - [`flags`] — the full `run`/`render` clap command tree: every literal flag
//!   from `run.go` (names, shorts, defaults, verbatim help), the tri-state
//!   `--checkpoint`/`--resume`, the deprecated `--skip-blacklist` /
//!   `--blacklisted-prefixes` (exact messages), and the **dynamic per-analyzer
//!   flag** registration driven by [`cf_pipeline::ConfigurationOption`] (Go
//!   `registerAnalyzerFlags` / `registerConfigFlag`).
//!
//! The actual run/render execution handlers (Go `RunCommand.run`,
//! `runHistoryAnalyzers`, `runRender`) depend on `cf-analyze` (the
//! format/conversion hub), `cf-gitlib`, `cf-framework`'s runner, and the 16
//! analyzer crates. Those crates are not yet building in this tree, so the
//! handlers live behind the `runtime` feature; their cross-crate contracts are
//! captured as the minimal traits in [`registry`]. See the crate `todos`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod flags;
pub mod formats;
pub mod handlers;
pub mod pipeline;
pub mod registry;
pub mod version;

pub use flags::{
    build_completion_command, build_render_command, build_run_command, deprecated_flag_message,
};
// `FORMAT_JSON` is part of the public format-constant API (used by the binary
// and golden harness); rustc emits a spurious unused-import warning for the
// re-export because it is also referenced internally via `crate::formats`.
#[allow(unused_imports)]
pub use formats::{
    apply_ndjson_modifier, normalize_format, resolve_formats, resolve_input_format, validate_format,
    validate_universal_format, FormatError, FORMAT_BINARY, FORMAT_BIN_ALIAS, FORMAT_COMPACT,
    FORMAT_JSON, FORMAT_NDJSON, FORMAT_PLOT, FORMAT_TEXT, FORMAT_TIMESERIES,
    FORMAT_TIMESERIES_NDJSON, FORMAT_YAML, INPUT_FORMAT_AUTO, INPUT_FORMAT_BINARY,
    INPUT_FORMAT_JSON,
};
pub use registry::{ConfigOptionProvider, RegistrationError};
pub use version::{build_version_command, version_output};

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-commands";

/// Build the top-level `codefang` [`clap::Command`] with the `run`, `render`,
/// and `version` subcommands wired in declaration order, mirroring Go
/// `cmd/codefang/main.go`.
///
/// The root carries the persistent flags `--verbose`/`-v`, `--quiet`/`-q`, and
/// `--profile` (all default `false`), matching cobra's persistent flags. The
/// binary-level error-handling asymmetry (codefang sets `SilenceErrors` +
/// `SilenceUsage`) is configured by the `codefang` binary crate, not here.
#[must_use]
pub fn build_codefang_command() -> clap::Command {
    clap::Command::new("codefang")
        .about("Codefang Code Analysis - Unified code analysis tool")
        .arg(
            clap::Arg::new("verbose")
                .long("verbose")
                .short('v')
                .global(true)
                .action(clap::ArgAction::SetTrue)
                .help("Enable verbose output"),
        )
        .arg(
            clap::Arg::new("quiet")
                .long("quiet")
                .short('q')
                .global(true)
                .action(clap::ArgAction::SetTrue)
                .help("Suppress non-error output"),
        )
        .arg(
            clap::Arg::new("profile")
                .long("profile")
                .global(true)
                .action(clap::ArgAction::SetTrue)
                .help("Enable profiling server and memory watchdog"),
        )
        .subcommand(build_run_command())
        .subcommand(build_render_command())
        .subcommand(build_version_command())
        .subcommand(build_completion_command())
}

/// Sentinel error message for run/render dispatch when no registered handler can
/// produce a report for the requested `(analyzer, format)` selection. Mirrors
/// the codefang error path (`Error: <msg>\n`, exit 1, no usage).
const DISPATCH_BLOCKED_MSG: &str =
    "command dispatch is blocked on cf-commands (tier 8); see DESIGN.md \u{00A7}4.1";

/// The single `codefang` entry point: build the command tree, parse `args`
/// (which MUST start with the program name, like `std::env::args`), and dispatch
/// `run` / `render` / `version` through the general pipeline + registry.
/// Returns the process exit code (Go `RunCommand.run` → cobra `Execute` exit).
///
/// This is the thin shell the `codefang` binary calls; all dispatch flows
/// through [`pipeline::run_pipeline`] over [`handlers::default_registry`] — there
/// is no per-`(analyzer, format)` branching here.
#[must_use]
pub fn run<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let matches = match build_codefang_command().try_get_matches_from(args) {
        Ok(m) => m,
        Err(e) => {
            // clap prints help/usage/version itself. cobra exits 1 on a usage
            // error (bad flag / unknown command / missing arg) and 0 when it
            // merely displayed help/version; mirror that exit-code contract
            // (the cli-surface error-path probes assert rc==1, not clap's 2).
            e.print().ok();
            return i32::from(e.use_stderr());
        }
    };

    match matches.subcommand() {
        Some(("version", _)) => {
            print!("{}", cf_version::codefang_version_line());
            0
        }
        Some(("run", sub)) => run_subcommand(sub),
        Some(("render", sub)) => render_subcommand(sub),
        Some(("completion", sub)) => completion_subcommand(sub),
        _ => {
            build_codefang_command().print_help().ok();
            println!();
            0
        }
    }
}

/// Dispatches `codefang completion <shell>`; generates the shell-completion
/// script to stdout, the Rust analogue of the bytes cobra's auto-registered
/// completion command emits (`bash`/`fish`/`powershell`/`zsh`). The script bytes
/// are clap-vs-cobra cosmetic (Layer-D informational), but a real generator (not
/// a help stub) is required so the command behaves like Go's. With no shell
/// subcommand cobra prints the completion command's help (rc 0).
fn completion_subcommand(sub: &clap::ArgMatches) -> i32 {
    use clap_complete::Shell;

    let shell = match sub.subcommand_name() {
        Some("bash") => Shell::Bash,
        Some("fish") => Shell::Fish,
        Some("powershell") => Shell::PowerShell,
        Some("zsh") => Shell::Zsh,
        _ => {
            build_completion_command().print_help().ok();
            println!();
            return 0;
        }
    };

    let mut cmd = build_codefang_command();
    clap_complete::generate(shell, &mut cmd, "codefang", &mut std::io::stdout());
    0
}

/// Dispatches `codefang run` through the general pipeline. Emits each phase's
/// report bytes to stdout in dispatch order; on an unsatisfiable selection it
/// surfaces the codefang error path (`Error: <msg>\n`, exit 1).
fn run_subcommand(sub: &clap::ArgMatches) -> i32 {
    use std::io::Write;

    let registry = handlers::default_registry();

    if sub.get_flag("list-analyzers") {
        let mut out = std::io::stdout();
        for id in registry.ids() {
            let _ = writeln!(out, "{id}");
        }
        return 0;
    }

    let ctx = pipeline::RunContext::from_matches(sub);
    let ids = ctx.analyzer_ids();

    // Special case preserved from Go FormatPerAnalyzer: a static-only glob/id set
    // with --format bin emits the concatenated registry-ordered per-analyzer CFB1
    // envelopes. This is a multi-id (glob) concern, not a single-id dispatch, so
    // it is handled before the per-id pipeline (which dispatches literal ids).
    let raw_format = ctx.raw_format();
    let analyzer_strs: Vec<&str> = ids.iter().map(String::as_str).collect();

    // --format plot routes to the multi-page HTML renderer (Go run.go: the
    // static/history phases each call validatePlotFlags then the plot
    // executor). The --output precheck fires for ANY plot selection (the exact
    // Go ErrPlotOutputRequired wording, rc 1); a static-only selection then
    // renders pages + index + report.json into the output dir with empty
    // stdout. History/mixed plot selections are not yet ported and surface the
    // dispatch-blocked diagnostic AFTER the flag validation, preserving Go's
    // error ordering.
    if formats::normalize_format(&raw_format) == formats::FORMAT_PLOT {
        let output = sub.get_one::<String>("output").map(String::as_str).unwrap_or("");
        if output.is_empty() {
            eprintln!("Error: --output flag is required when --format plot");
            return 1;
        }
        let (plot_static, plot_history) = handlers::expand_combined_ids(&analyzer_strs);
        if !plot_static.is_empty() && plot_history.is_empty() {
            if let Some(code) = handlers::plot::run_static_plot(&ctx, &plot_static, output) {
                return code;
            }
        }
        eprintln!("Error: {DISPATCH_BLOCKED_MSG}");
        return 1;
    }
    if raw_format == "bin"
        && !analyzer_strs.is_empty()
        && analyzer_strs.iter().any(|a| a.contains(['*', '?', '[']))
        && analyzer_strs.iter().all(|a| handlers::is_static_id_or_glob(a))
    {
        if let Some(bytes) = handlers::static_multi_bin(&analyzer_strs, &ctx.path) {
            std::io::stdout().write_all(&bytes).expect("write stdout");
            return 0;
        }
    }

    // Special case preserved from Go renderer.SectionsToJSON: a STATIC-ONLY
    // selection (no history analyzer) of MORE THAN ONE static analyzer (a literal
    // multi-id list or a glob like `static/*`) with `--format json` renders ONE
    // merged JSONReport — sections in registry order, overall_score the average
    // of the scored sections. A single static analyzer keeps its own one-section
    // document (the per-id pipeline below); a selection that also matches a
    // history analyzer (e.g. `*`) uses the UnifiedModel path, not this merge.
    if raw_format == "json"
        && !analyzer_strs.is_empty()
        && analyzer_strs.iter().all(|a| handlers::is_static_id_or_glob(a))
        && handlers::static_json_selects_multiple(&analyzer_strs)
    {
        if let Some(bytes) = handlers::static_multi_json(&analyzer_strs, &ctx.path) {
            std::io::stdout().write_all(&bytes).expect("write stdout");
            return 0;
        }
    }

    // Mixed static+history selection (both phases non-empty) renders the single
    // `codefang.run.v1` unified-model envelope, mirroring Go runDirect →
    // renderCombinedDirect. This is NOT a per-analyzer concatenation: every
    // selected analyzer's raw report is gathered (via its bin payload) into one
    // model with run metadata and re-serialized in the requested format by the
    // serializer layer (cf-analyze::write_converted_output). Plot is excluded
    // (Go isMixedPlot keeps the separate-phase path). On any unported analyzer
    // the helper returns None and we fall through to the per-id pipeline.
    let (combined_static, combined_history) = handlers::expand_combined_ids(&analyzer_strs);
    if !combined_static.is_empty()
        && !combined_history.is_empty()
        && raw_format != "plot"
        && raw_format != "compact"
    {
        if let Some(bytes) =
            handlers::render_combined(&ctx, &combined_static, &combined_history, &raw_format)
        {
            std::io::stdout().write_all(&bytes).expect("write stdout");
            return 0;
        }
    }

    // Expand globs / literal ids to the concrete registry ids the per-id pipeline
    // dispatches (Go `registry.Split` resolves patterns before the phase loop).
    // `run_pipeline` matches literal ids only, so an unexpanded glob like
    // `history/*` would otherwise miss every handler. The static phase uses the
    // registry static order; the history phase uses Go's SEPARATE-phase emit order
    // (`HISTORY_PHASE_EMIT_ORDER` — `runHistoryPhase` over the glob-expanded leaf
    // set), which differs from the combined-model order, so a history-only glob's
    // per-analyzer reports concatenate in the same sequence Go writes them. Fall
    // back to the raw selection when nothing expands (preserves Go's unknown-id
    // error). A glob in `analyzer_strs` forces the expanded ordering even for a
    // mixed selection; a purely literal selection keeps its request order.
    let resolved_ids = {
        let is_glob = |p: &&str| p.contains(['*', '?', '[']);
        if analyzer_strs.iter().any(is_glob) {
            let (s, _h) = handlers::expand_combined_ids(&analyzer_strs);
            let mut v = s;
            v.extend(handlers::expand_history_phase_ids(&analyzer_strs));
            if v.is_empty() { ids.clone() } else { v }
        } else {
            ids.clone()
        }
    };

    // Centralized history-phase formats (Go OutputHistoryResults / StreamingSink):
    // text, ndjson, and timeseries are NOT per-analyzer encodings — Go routes the
    // whole selected leaf set through one history output function (header + per-
    // leaf sections for text; per-commit TC lines for ndjson; one merged document
    // for timeseries). A history-only selection in one of those formats dispatches
    // here; `None` (an unported leaf) falls through to the per-id pipeline and its
    // existing dispatch-blocked diagnostic.
    let all_history = !resolved_ids.is_empty()
        && resolved_ids.iter().all(|id| {
            registry
                .lookup(id)
                .is_some_and(|e| matches!(e.mode, pipeline::Mode::History))
        });
    if all_history {
        let normalized = formats::normalize_format(&raw_format);
        let history_format = formats::apply_ndjson_modifier(&normalized, ctx.ndjson());
        let special = match history_format.as_str() {
            formats::FORMAT_TEXT => handlers::history_formats::history_text(&ctx, &resolved_ids),
            formats::FORMAT_NDJSON => handlers::history_formats::history_ndjson(&ctx, &resolved_ids),
            formats::FORMAT_TIMESERIES => {
                handlers::history_formats::history_timeseries(&ctx, &resolved_ids, false)
            }
            // `timeseries+ndjson` (the --ndjson modifier) is NOT the merged
            // document as lines: Go routes it through the per-chunk
            // TimeSeriesChunkFlusher (DrainCommitStats), which devs/burndown
            // reproduce in their per-analyzer handlers — fall through.
            _ => None,
        };
        match special {
            Some(Ok(bytes)) => {
                std::io::stdout().write_all(&bytes).expect("write stdout");
                return 0;
            }
            Some(Err(fail)) => {
                // Go streams the partial bytes to stdout BEFORE the serializer
                // fails; cobra then prints `Error: <msg>` to stderr and exits 1.
                std::io::stdout().write_all(&fail.partial).expect("write stdout");
                eprintln!("Error: {}", fail.message);
                return 1;
            }
            None => {}
        }
    }

    match pipeline::run_pipeline(&registry, &ctx, &resolved_ids) {
        Ok(outputs) => {
            let mut out = std::io::stdout();
            for phase in &outputs {
                out.write_all(&phase.bytes).expect("write stdout");
            }
            0
        }
        // An unknown analyzer id surfaces the specific Go diagnostic
        // ("unknown analyzer id: <id>", Go ErrUnknownAnalyzer) so the error
        // path is byte-class-identical to cobra (the cli-surface runtime probe
        // requires the "analyzer" diagnostic, not a generic stub message).
        Err(pipeline::PipelineError::UnknownAnalyzer(id)) => {
            eprintln!("Error: unknown analyzer id: {id}");
            1
        }
        // Any other unsatisfiable selection (no handler for this analyzer/format)
        // routes to the same codefang error path the legacy dispatch fell through to.
        Err(_) => {
            eprintln!("Error: {DISPATCH_BLOCKED_MSG}");
            1
        }
    }
}

/// Dispatches `codefang render <store-dir>`; reproduces the `--output` precheck
/// (Go render.go `ErrNoOutputDir`) before the (still-blocked) render body.
fn render_subcommand(sub: &clap::ArgMatches) -> i32 {
    let output = sub.get_one::<String>("output").map(String::as_str).unwrap_or("");
    if output.is_empty() {
        eprintln!("Error: output directory is required (use --output)");
        return 1;
    }
    eprintln!("Error: {DISPATCH_BLOCKED_MSG}");
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_links() {
        assert_eq!(CRATE_NAME, "cf-commands");
    }

    #[test]
    fn root_has_subcommands_in_order() {
        let cmd = build_codefang_command();
        let names: Vec<&str> = cmd.get_subcommands().map(clap::Command::get_name).collect();
        // `completion` mirrors the command cobra auto-registers (cli-surface
        // parity); it is wired last, after the three explicit subcommands.
        assert_eq!(names, vec!["run", "render", "version", "completion"]);
    }

    #[test]
    fn root_has_persistent_flags() {
        let cmd = build_codefang_command();
        let longs: Vec<_> = cmd
            .get_arguments()
            .filter_map(clap::Arg::get_long)
            .collect();
        assert!(longs.contains(&"verbose"));
        assert!(longs.contains(&"quiet"));
        assert!(longs.contains(&"profile"));
    }

    #[test]
    fn root_parses_version_subcommand() {
        let cmd = build_codefang_command();
        let m = cmd.try_get_matches_from(["codefang", "version"]).unwrap();
        assert_eq!(m.subcommand_name(), Some("version"));
    }
}
