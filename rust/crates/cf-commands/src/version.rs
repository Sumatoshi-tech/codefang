//! The `codefang version` subcommand.
//!
//! Prints, on STDOUT, with exit code 0 and no flags:
//!
//! ```text
//! codefang %s (commit: %s, built: %s)\n
//! ```
//!
//! (`version.Version`, `version.Commit`, `version.Date`). The exact bytes — with
//! the trailing newline — come from [`cf_version::codefang_version_line`], which
//! reproduces the reference implementation defaults `dev` / `none` / `unknown` when nothing is
//! injected at build time. This subcommand is a Layer-D CLI golden target
//! (DESIGN §6), so the output must be byte-identical to the reference binary.

/// Returns the exact bytes the `codefang version` subcommand writes to STDOUT,
/// including the trailing newline: `codefang <version> (commit: <commit>,
/// built: <date>)\n`. mirrors the reference implementation `fmt.Fprintf(os.Stdout, "codefang %s (commit:
/// %s, built: %s)\n", ...)`.
#[must_use]
pub fn version_output() -> String {
    cf_version::codefang_version_line()
}

/// Build the `version` [`clap::Command`].
///
/// The reference implementation's `versionCmd` is `Use: "version"`, `Short: "Show codefang version"`, with
/// a `Run` that prints [`version_output`] and exits 0. It declares no flags.
/// The printing happens in the binary's dispatch (so this builder only models
/// the command surface for parity with cobra's `--help`/usage output).
#[must_use]
pub fn build_version_command() -> clap::Command {
    clap::Command::new("version").about("Show codefang version")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_output_matches_go_format_with_defaults() {
        // With no build-time injection cf-version uses the reference implementation's dev/none/unknown.
        assert_eq!(
            version_output(),
            "codefang dev (commit: none, built: unknown)\n"
        );
    }

    #[test]
    fn version_output_has_trailing_newline() {
        assert!(version_output().ends_with('\n'));
    }

    #[test]
    fn version_command_has_no_flags() {
        let cmd = build_version_command();
        // Only the implicit help arg may be present; no user-facing flags.
        let user_flags: Vec<_> = cmd
            .get_arguments()
            .filter_map(clap::Arg::get_long)
            .filter(|l| *l != "help")
            .collect();
        assert!(user_flags.is_empty(), "version should declare no flags");
    }
}
