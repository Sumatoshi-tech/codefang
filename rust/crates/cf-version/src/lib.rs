//! Build/version metadata for codefang.
//!
//! This crate is the Rust port of the Go package `pkg/version`
//! (`/home/dmitriy/sources/codefang/pkg/version/version.go`). Its job is to hold
//! the build-metadata strings and render the one-line banner both binaries
//! print:
//!
//! ```text
//! <name> <Version> (commit: <Commit>, built: <Date>)
//! ```
//!
//! Confirmed call sites in Go:
//! - `cmd/codefang/main.go:306`:
//!   `fmt.Fprintf(os.Stdout, "codefang %s (commit: %s, built: %s)\n", Version, Commit, Date)`
//! - `cmd/uast/main.go:57`:
//!   `fmt.Fprintf(os.Stdout, "uast %s (commit: %s, built: %s)\n", Version, Commit, Date)`
//!
//! # Injection model (ldflags → build script)
//!
//! In Go, `Version`/`Commit`/`Date` are injected at link time via
//! `-ldflags "-X .../pkg/version.Version=..."` and fall back to the
//! compile-time defaults `dev` / `none` / `unknown`. Rust has no ldflags
//! equivalent, so per [`DESIGN.md` §2.8] we inject the same values through a
//! build script (`build.rs`) that reads environment variables and re-exports
//! them as `rustc-env` entries; the library reads those with [`option_env!`],
//! falling back to the Go defaults when unset. A plain `cargo build` with no
//! environment therefore produces exactly `dev` / `none` / `unknown`,
//! byte-identical to a Go build with no ldflags.
//!
//! Recognized build-time inputs (see `build.rs`):
//! - `Version`: `CF_VERSION`, then `GIT_VERSION`
//! - `Commit`:  `CF_COMMIT`,  then `GIT_COMMIT`
//! - `Date`:    `CF_DATE`,    then `SOURCE_DATE_EPOCH` (epoch → RFC3339 UTC)
//!
//! `SOURCE_DATE_EPOCH` support keeps the `built:` date reproducible so the
//! `version` CLI golden (DESIGN §6, Layer D) is stable on both Go and Rust.
//!
//! [`DESIGN.md` §2.8]: ../../../specs/rust-rewrite/DESIGN.md

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt::Write as _;

/// Default version when none is injected — mirrors Go's `Version = "dev"`.
pub const DEFAULT_VERSION: &str = "dev";
/// Default commit when none is injected — mirrors Go's `Commit = "none"`.
pub const DEFAULT_COMMIT: &str = "none";
/// Default build date when none is injected — mirrors Go's `Date = "unknown"`.
pub const DEFAULT_DATE: &str = "unknown";
/// Default git hash of the running binary — mirrors Go's
/// `BinaryGitHash = "<unknown>"`. There is no link-time/env injection for this
/// in Go (it is set elsewhere at runtime if at all), so it is a plain default.
pub const DEFAULT_BINARY_GIT_HASH: &str = "<unknown>";
/// Default API version — mirrors Go's `Binary = 0`. See [`binary_api_version`].
pub const DEFAULT_BINARY: i64 = 0;

/// The build version string.
///
/// Equivalent to Go's `version.Version`. Injected via `CF_VERSION` /
/// `GIT_VERSION` at build time; defaults to [`DEFAULT_VERSION`] (`"dev"`).
pub const VERSION: &str = match option_env!("CF_VERSION_INJECTED") {
    Some(v) => v,
    None => DEFAULT_VERSION,
};

/// The build commit hash.
///
/// Equivalent to Go's `version.Commit`. Injected via `CF_COMMIT` /
/// `GIT_COMMIT` at build time; defaults to [`DEFAULT_COMMIT`] (`"none"`).
pub const COMMIT: &str = match option_env!("CF_COMMIT_INJECTED") {
    Some(v) => v,
    None => DEFAULT_COMMIT,
};

/// The build date.
///
/// Equivalent to Go's `version.Date`. Injected via `CF_DATE` /
/// `SOURCE_DATE_EPOCH` at build time; defaults to [`DEFAULT_DATE`]
/// (`"unknown"`).
pub const DATE: &str = match option_env!("CF_DATE_INJECTED") {
    Some(v) => v,
    None => DEFAULT_DATE,
};

/// The git hash of the running binary.
///
/// Equivalent to Go's `version.BinaryGitHash`. Go declares it as a mutable
/// package var defaulting to `"<unknown>"`; nothing in the binaries reassigns
/// it from this package, so we expose the constant default.
pub const BINARY_GIT_HASH: &str = DEFAULT_BINARY_GIT_HASH;

/// Render the one-line version banner for a binary named `name`, using the
/// compile-time-injected [`VERSION`], [`COMMIT`], and [`DATE`].
///
/// The returned string carries **no trailing newline**; the `version`
/// subcommands of both binaries print it with a trailing `\n`, reproducing the
/// Go `fmt.Fprintf(os.Stdout, "%s %s (commit: %s, built: %s)\n", name, ...)`
/// call exactly. Use [`println!`] (or write the string then `\n`) at the call
/// site to emit the final newline.
///
/// # Examples
///
/// ```
/// // With no ldflags / env injection, the defaults are used:
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

/// Compute codefang's integer API version from a Go-style package path, the
/// Rust analogue of Go's `version.InitBinaryVersion`.
///
/// Go derives `Binary` from the *last dot-separated component* of the package
/// import path, stripping its first byte and parsing the remainder as an
/// integer (`reflect ... PkgPath()` → split on `"."` → `Atoi(last[1:])`). The
/// convention is a path component like `v0`, `v3`, … so the leading `v` is
/// dropped. On any parse failure Go leaves `Binary` at its zero value `0`.
///
/// In Rust there is no equivalent package-path reflection, so callers pass the
/// path explicitly (e.g. the binary's module path or a configured identifier).
/// The parsing rule is reproduced exactly:
/// 1. take the substring after the last `'.'` (or the whole string if none);
/// 2. drop the first character (Go's `[1:]`, which Go does on bytes — we drop
///    the first `char` boundary, matching for the ASCII `vN` convention);
/// 3. parse the remainder as `i64`; on failure return [`DEFAULT_BINARY`] (`0`).
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
    // Go does `last[1:]` (byte slice). Drop the first char boundary; for the
    // `vN` ASCII convention this is identical.
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
    fn defaults_match_go() {
        // Go: Version="dev", Commit="none", Date="unknown",
        //     BinaryGitHash="<unknown>", Binary=0.
        assert_eq!(DEFAULT_VERSION, "dev");
        assert_eq!(DEFAULT_COMMIT, "none");
        assert_eq!(DEFAULT_DATE, "unknown");
        assert_eq!(DEFAULT_BINARY_GIT_HASH, "<unknown>");
        assert_eq!(DEFAULT_BINARY, 0);
    }

    #[test]
    fn banner_with_explicit_fields_matches_go_format() {
        // Reproduces fmt.Fprintf("%s %s (commit: %s, built: %s)\n", ...) sans \n.
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
        // The newline is the caller's responsibility (matches Go's Printf "\n").
        assert!(!banner("codefang").ends_with('\n'));
    }

    #[test]
    fn banner_handles_empty_fields() {
        assert_eq!(banner_with("", "", "", ""), "  (commit: , built: )");
    }

    #[test]
    fn binary_api_version_strips_leading_char_and_parses() {
        // Go: split on '.', take last, Atoi(last[1:]).
        assert_eq!(binary_api_version("github.com/x/pkg/version.v3"), 3);
        assert_eq!(binary_api_version("v0"), 0);
        assert_eq!(binary_api_version("v42"), 42);
        assert_eq!(binary_api_version("x.v100"), 100);
    }

    #[test]
    fn binary_api_version_falls_back_to_zero_on_parse_error() {
        // Non-numeric remainder -> Atoi error -> Binary stays 0.
        assert_eq!(binary_api_version("version"), 0); // "ersion" -> err -> 0
        assert_eq!(binary_api_version(""), 0);
        assert_eq!(binary_api_version("v"), 0); // remainder "" -> err -> 0
        assert_eq!(binary_api_version("pkg.vX"), 0);
    }
}
