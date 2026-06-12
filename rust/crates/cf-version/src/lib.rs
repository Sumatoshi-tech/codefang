//! Build/version metadata for codefang.
//!
//! Holds the build-metadata strings and renders the one-line banner both
//! binaries print (CLI compatibility contract; pinned by the `version` CLI
//! golden):
//!
//! ```text
//! <name> <Version> (commit: <Commit>, built: <Date>)
//! ```
//!
//! # Injection model (build script)
//!
//! `Version`/`Commit`/`Date` are injected at build time (DESIGN.md §2.8)
//! through a build script (`build.rs`) that reads environment variables and
//! re-exports them as `rustc-env` entries; the library reads those with
//! [`option_env!`], falling back to the defaults `dev` / `none` / `unknown`
//! when unset. A plain `cargo build` with no environment therefore produces
//! exactly `dev` / `none` / `unknown`.
//!
//! Recognized build-time inputs (see `build.rs`):
//! - `Version`: `CF_VERSION`, then `GIT_VERSION`
//! - `Commit`:  `CF_COMMIT`,  then `GIT_COMMIT`
//! - `Date`:    `CF_DATE`,    then `SOURCE_DATE_EPOCH` (epoch → RFC3339 UTC)
//!
//! `SOURCE_DATE_EPOCH` support keeps the `built:` date reproducible so the
//! `version` CLI golden (DESIGN §6, Layer D) is stable across builds.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt::Write as _;

/// Default version when none is injected.
pub const DEFAULT_VERSION: &str = "dev";
/// Default commit when none is injected.
pub const DEFAULT_COMMIT: &str = "none";
/// Default build date when none is injected.
pub const DEFAULT_DATE: &str = "unknown";
/// Default git hash of the running binary. There is no build-time injection
/// for this field, so it is a plain default.
pub const DEFAULT_BINARY_GIT_HASH: &str = "<unknown>";
/// Default API version. See [`binary_api_version`].
pub const DEFAULT_BINARY: i64 = 0;

/// The build version string.
///
/// Injected via `CF_VERSION` / `GIT_VERSION` at build time; defaults to
/// [`DEFAULT_VERSION`] (`"dev"`).
pub const VERSION: &str = match option_env!("CF_VERSION_INJECTED") {
    Some(v) => v,
    None => DEFAULT_VERSION,
};

/// The build commit hash.
///
/// Injected via `CF_COMMIT` / `GIT_COMMIT` at build time; defaults to
/// [`DEFAULT_COMMIT`] (`"none"`).
pub const COMMIT: &str = match option_env!("CF_COMMIT_INJECTED") {
    Some(v) => v,
    None => DEFAULT_COMMIT,
};

/// The build date.
///
/// Injected via `CF_DATE` / `SOURCE_DATE_EPOCH` at build time; defaults to
/// [`DEFAULT_DATE`] (`"unknown"`).
pub const DATE: &str = match option_env!("CF_DATE_INJECTED") {
    Some(v) => v,
    None => DEFAULT_DATE,
};

/// The git hash of the running binary.
///
/// Nothing in the binaries reassigns this, so the constant default is
/// exposed directly.
pub const BINARY_GIT_HASH: &str = DEFAULT_BINARY_GIT_HASH;

/// Render the one-line version banner for a binary named `name`, using the
/// compile-time-injected [`VERSION`], [`COMMIT`], and [`DATE`].
///
/// The returned string carries **no trailing newline**; the `version`
/// subcommands of both binaries print it with a trailing `\n` (the banner
/// format is a frozen CLI contract). Use [`println!`] (or write the string
/// then `\n`) at the call site to emit the final newline.
///
/// # Examples
///
/// ```
/// // With no build-time injection, the defaults are used:
/// assert_eq!(
///     cf_version::banner("codefang"),
///     "codefang dev (commit: none, built: unknown)"
/// );
/// assert_eq!(
///     cf_version::banner("uast"),
///     "uast dev (commit: none, built: unknown)"
/// );
/// ```
#[must_use]
pub fn banner(name: &str) -> String {
    banner_with(name, VERSION, COMMIT, DATE)
}

/// Returns the `codefang version` line — the banner plus a trailing newline
/// (frozen CLI contract, pinned by `rust/tests/compat`).
#[must_use]
pub fn codefang_version_line() -> String {
    let mut s = banner("codefang");
    s.push('\n');
    s
}

/// Returns the `uast version` line — the banner plus a trailing newline
/// (frozen CLI contract, pinned by `rust/tests/compat`).
#[must_use]
pub fn uast_version_line() -> String {
    let mut s = banner("uast");
    s.push('\n');
    s
}

/// Render the version banner from explicit fields. Exposed for testing and for
/// callers that need to format a banner from values not baked into this crate.
///
/// Produces `"<name> <version> (commit: <commit>, built: <date>)"` with no
/// trailing newline.
#[must_use]
pub fn banner_with(name: &str, version: &str, commit: &str, date: &str) -> String {
    let mut s =
        String::with_capacity(name.len() + version.len() + commit.len() + date.len() + 24);
    // `write!` to a String is infallible; `let _ =` discards the Ok(()).
    let _ = write!(s, "{name} {version} (commit: {commit}, built: {date})");
    s
}

/// Compute codefang's integer API version from a dotted package-path-style
/// identifier.
///
/// The API version is derived from the *last dot-separated component*,
/// stripping its first character and parsing the remainder as an integer.
/// The convention is a component like `v0`, `v3`, … so the leading `v` is
/// dropped. Callers pass the path explicitly (e.g. the binary's module path
/// or a configured identifier).
///
/// The parsing rule is frozen (reference-implementation behavior):
/// 1. take the substring after the last `'.'` (or the whole string if none);
/// 2. drop the first character (the ASCII `vN` convention makes the byte/char
///    distinction moot);
/// 3. parse the remainder as `i64`; on failure return [`DEFAULT_BINARY`]
///    (`0`).
///
/// # Examples
///
/// ```
/// assert_eq!(cf_version::binary_api_version("github.com/x/pkg/version.v3"), 3);
/// assert_eq!(cf_version::binary_api_version("v0"), 0);
/// assert_eq!(cf_version::binary_api_version("version"), 0); // "ersion" -> 0
/// assert_eq!(cf_version::binary_api_version(""), 0);        // empty -> 0
/// ```
#[must_use]
pub fn binary_api_version(pkg_path: &str) -> i64 {
    let last = match pkg_path.rsplit_once('.') {
        Some((_, tail)) => tail,
        None => pkg_path,
    };
    // Drop the first character of the component (the `vN` prefix convention).
    let rest = match last.char_indices().nth(1) {
        Some((idx, _)) => &last[idx..],
        None => "", // 0- or 1-char component -> empty remainder
    };
    rest.parse::<i64>().unwrap_or(DEFAULT_BINARY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_frozen() {
        // Frozen defaults: Version="dev", Commit="none", Date="unknown",
        // BinaryGitHash="<unknown>", Binary=0.
        assert_eq!(DEFAULT_VERSION, "dev");
        assert_eq!(DEFAULT_COMMIT, "none");
        assert_eq!(DEFAULT_DATE, "unknown");
        assert_eq!(DEFAULT_BINARY_GIT_HASH, "<unknown>");
        assert_eq!(DEFAULT_BINARY, 0);
    }

    #[test]
    fn banner_with_explicit_fields_matches_frozen_format() {
        // "<name> <version> (commit: <commit>, built: <date>)" sans newline.
        assert_eq!(
            banner_with("codefang", "1.2.3", "abc123", "2024-01-02T03:04:05Z"),
            "codefang 1.2.3 (commit: abc123, built: 2024-01-02T03:04:05Z)"
        );
    }

    #[test]
    fn banner_uses_binary_name() {
        assert_eq!(
            banner_with("uast", "1.2.3", "abc123", "2024-01-02T03:04:05Z"),
            "uast 1.2.3 (commit: abc123, built: 2024-01-02T03:04:05Z)"
        );
    }

    #[test]
    fn banner_with_no_injection_uses_defaults() {
        // In the test build no CF_*_INJECTED env is set, so the consts default.
        assert_eq!(VERSION, "dev");
        assert_eq!(COMMIT, "none");
        assert_eq!(DATE, "unknown");
        assert_eq!(BINARY_GIT_HASH, "<unknown>");
        assert_eq!(banner("codefang"), "codefang dev (commit: none, built: unknown)");
        assert_eq!(banner("uast"), "uast dev (commit: none, built: unknown)");
    }

    #[test]
    fn banner_has_no_trailing_newline() {
        // The newline is the caller's responsibility.
        assert!(!banner("codefang").ends_with('\n'));
    }

    #[test]
    fn banner_handles_empty_fields() {
        assert_eq!(banner_with("", "", "", ""), "  (commit: , built: )");
    }

    #[test]
    fn binary_api_version_strips_leading_char_and_parses() {
        // Split on '.', take last component, parse it minus its first char.
        assert_eq!(binary_api_version("github.com/x/pkg/version.v3"), 3);
        assert_eq!(binary_api_version("v0"), 0);
        assert_eq!(binary_api_version("v42"), 42);
        assert_eq!(binary_api_version("x.v100"), 100);
    }

    #[test]
    fn binary_api_version_falls_back_to_zero_on_parse_error() {
        // Non-numeric remainder -> parse error -> 0.
        assert_eq!(binary_api_version("version"), 0); // "ersion" -> err -> 0
        assert_eq!(binary_api_version(""), 0);
        assert_eq!(binary_api_version("v"), 0); // remainder "" -> err -> 0
        assert_eq!(binary_api_version("pkg.vX"), 0);
    }
}
