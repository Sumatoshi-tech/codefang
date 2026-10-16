//! The `codefang version` subcommand.
//!
//! Prints, on STDOUT, with exit code 0 and no flags:
//!
//! ```text
//! codefang <version> (commit: <commit>, built: <date>)\n
//! ```
//!
//! * `<version>` — `CF_VERSION` / `GIT_VERSION` injected at build time
//!   (`cf-version`), else the crate version.
//! * `<commit>` — `CF_COMMIT` / `GIT_COMMIT` / `GIT_HASH` injected at build
//!   time, else the commit `HEAD` of the checkout the build ran in (found by
//!   `cf-version`'s build script), else `unknown`.
//! * `<date>` — `CF_DATE` / `SOURCE_DATE_EPOCH` injected at build time, else
//!   `unknown`; the wall clock is never read, so builds stay reproducible.

/// Returns the exact bytes the `codefang version` subcommand writes to STDOUT,
/// including the trailing newline.
///
/// ```
/// let line = cf_commands::version::version_output();
/// assert!(line.starts_with(&format!("codefang {} (commit: ", env!("CARGO_PKG_VERSION"))));
/// assert!(line.contains(", built: "));
/// assert!(line.ends_with(")\n"));
/// ```
#[must_use]
pub fn version_output() -> String {
    cf_version::codefang_version_line()
}

/// Build the `version` [`clap::Command`] (no flags; the dispatch prints
/// [`version_output`]).
#[must_use]
pub fn build_version_command() -> clap::Command {
    clap::Command::new("version").about("Show codefang version")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_names_the_crate_version_and_the_build_commit() {
        assert_eq!(
            version_output(),
            format!(
                "codefang {} (commit: {}, built: {})\n",
                env!("CARGO_PKG_VERSION"),
                cf_version::COMMIT,
                cf_version::DATE
            )
        );
    }

    #[test]
    fn version_command_has_no_flags() {
        let cmd = build_version_command();
        let user_flags: Vec<_> = cmd
            .get_arguments()
            .filter_map(clap::Arg::get_long)
            .filter(|l| *l != "help")
            .collect();
        assert!(user_flags.is_empty(), "version should declare no flags");
    }
}
