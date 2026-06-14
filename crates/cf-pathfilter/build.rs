//! Build-time generator for the enry vendor-pattern table.
//!
//! # Why a generator
//!
//! `cf-pathfilter` reproduces `github.com/src-d/enry/v2` `IsVendor`, whose
//! decision changes *which* files are analysed and therefore which bytes appear
//! in machine-format reports (pinned by `tests/compat`). The design
//! (`specs/rust-rewrite/DESIGN.md` §2.6) mandates vendoring the **same** data
//! tables enry uses, byte-for-byte, instead of hand-translating a generated
//! artifact or swapping detectors.
//!
//! enry's vendor matchers live in its generated `data/vendor.go`
//! (`var VendorMatchers = substring.Or(substring.Regexp(`...`), ...)`). This
//! build script extracts the literal regular-expression source strings from that
//! file (when the enry source is available) and emits them as a Rust slice into
//! `$OUT_DIR/vendor_patterns.rs`, included by `lib.rs`. enry's regexp engine is
//! RE2-syntax, matched by the Rust [`regex`] crate, so the same source strings
//! yield identical (unanchored) match behaviour.
//!
//! ## Source location
//!
//! The path to enry's `data/vendor.go` is resolved, in order, from:
//!   1. the `CF_ENRY_VENDOR_GO` environment variable (explicit override), then
//!   2. `$GOMODCACHE/github.com/src-d/enry/v2@<ver>/data/vendor.go` for the
//!      pinned reference version (`v2.1.0`).
//!
//! If neither is found, the build script falls back to the checked-in
//! [`crate::vendor_data::VENDOR_PATTERNS`] table (a transcription of enry
//! v2.1.0) so offline builds still succeed; a `cargo::warning` is emitted so the
//! divergence risk is visible. The unit test `vendor_patterns_match_enry_source`
//! asserts parity when the enry source is present.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Pinned enry version (`github.com/src-d/enry/v2 v2.1.0`), matching the
/// vendored data snapshot.
const ENRY_VERSION: &str = "v2.1.0";

fn main() {
    println!("cargo::rerun-if-env-changed=CF_ENRY_VENDOR_GO");
    println!("cargo::rerun-if-env-changed=GOMODCACHE");

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR set by cargo");
    let dest = Path::new(&out_dir).join("vendor_patterns.rs");

    match locate_vendor_go() {
        Some(path) if path.exists() => {
            println!("cargo::rerun-if-changed={}", path.display());
            match fs::read_to_string(&path) {
                Ok(src) => {
                    let patterns = extract_patterns(&src);
                    if patterns.is_empty() {
                        emit_fallback(&dest, "extracted zero patterns from enry source");
                    } else {
                        emit_patterns(&dest, &patterns);
                    }
                }
                Err(e) => emit_fallback(&dest, &format!("could not read enry source: {e}")),
            }
        }
        _ => emit_fallback(&dest, "enry data/vendor.go not found"),
    }
}

/// Resolve the path to enry's `data/vendor.go`.
fn locate_vendor_go() -> Option<PathBuf> {
    if let Ok(p) = env::var("CF_ENRY_VENDOR_GO") {
        return Some(PathBuf::from(p));
    }
    let gomodcache = env::var("GOMODCACHE").ok().or_else(go_env_gomodcache)?;
    let rel = format!(
        "github.com/src-d/enry/v2@{ENRY_VERSION}/data/vendor.go"
    );
    Some(Path::new(&gomodcache).join(rel))
}

/// Ask the Go toolchain for `GOMODCACHE` if the env var is unset.
fn go_env_gomodcache() -> Option<String> {
    let out = Command::new("go").args(["env", "GOMODCACHE"]).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Extract the raw-string arguments of every `substring.Regexp(`...`)` in the
/// enry `vendor.go` source. enry's `VendorMatchers` is built as
/// `substring.Or(substring.Regexp(`p1`), substring.Regexp(`p2`), …)`, using
/// raw (backtick) string literals for the regex sources — which contain no
/// escape processing, so each pattern is the exact byte sequence between the
/// backticks.
fn extract_patterns(src: &str) -> Vec<String> {
    const NEEDLE: &str = "substring.Regexp(";
    let bytes = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(rel) = src[i..].find(NEEDLE) {
        let mut j = i + rel + NEEDLE.len();
        // Skip whitespace up to the opening backtick.
        while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b'`' {
            let start = j + 1;
            if let Some(end_rel) = src[start..].find('`') {
                let end = start + end_rel;
                out.push(src[start..end].to_string());
                i = end + 1;
                continue;
            }
        }
        i = j;
    }
    out
}

/// Write the extracted patterns as a Rust slice literal.
fn emit_patterns(dest: &Path, patterns: &[String]) {
    let mut body = String::new();
    body.push_str(
        "// @generated by build.rs from enry data/vendor.go. Do not edit.\n",
    );
    body.push_str("/// Vendor-path regex sources extracted from enry's `data/vendor.go`.\n");
    body.push_str("pub static GENERATED_VENDOR_PATTERNS: &[&str] = &[\n");
    for p in patterns {
        // Emit as a Rust raw string with enough `#` hashes to be unambiguous.
        let hashes = pick_raw_hashes(p);
        let pad = "#".repeat(hashes);
        let _ = writeln!(body, "    r{pad}\"{p}\"{pad},");
    }
    body.push_str("];\n");
    fs::write(dest, body).expect("write vendor_patterns.rs");
}

/// Choose a `#` count for a Rust raw string so the closing `"#...` delimiter
/// cannot appear inside the pattern. Patterns without a `"` need no hashes.
fn pick_raw_hashes(p: &str) -> usize {
    let mut n = 0usize;
    loop {
        let close = format!("\"{}", "#".repeat(n));
        if !p.contains(&close) {
            return n;
        }
        n += 1;
    }
}

/// Emit a stub that defers to the checked-in fallback table.
fn emit_fallback(dest: &Path, reason: &str) {
    println!(
        "cargo::warning=cf-pathfilter: using checked-in enry vendor table ({reason}); \
         set CF_ENRY_VENDOR_GO to enry {ENRY_VERSION} data/vendor.go for verified parity"
    );
    let body = "// @generated fallback: enry source unavailable at build time.\n\
        /// Empty marker; runtime falls back to `vendor_data::VENDOR_PATTERNS`.\n\
        pub static GENERATED_VENDOR_PATTERNS: &[&str] = &[];\n";
    fs::write(dest, body).expect("write vendor_patterns.rs fallback");
}
