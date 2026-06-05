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
pub mod registry;
pub mod version;

pub use flags::{build_render_command, build_run_command, deprecated_flag_message};
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_name_links() {
        assert_eq!(CRATE_NAME, "cf-commands");
    }

    #[test]
    fn root_has_three_subcommands_in_order() {
        let cmd = build_codefang_command();
        let names: Vec<&str> = cmd.get_subcommands().map(clap::Command::get_name).collect();
        assert_eq!(names, vec!["run", "render", "version"]);
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
