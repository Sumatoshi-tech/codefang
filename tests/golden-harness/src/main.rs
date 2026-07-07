//! Runnable golden-harness (ROADMAP Step 14 / DESIGN §6).
//!
//! `cargo run -p golden-harness` runs **every BINDING capture** (a capture is
//! binding when `nonBinding != true`) from `tests/golden/MANIFEST.json`
//! under the exact golden environment
//! (`TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800`), captures
//! STDOUT only, byte-compares it against the golden at `relPath`, prints a
//! per-capture `IDENTICAL`/`DIFFER` line and a final `N/M identical`, and exits
//! nonzero if any binding capture differs.
//!
//! The manifest's `argv[0]` is the Go reference binary used to capture the
//! golden; the harness swaps it for the freshly built Rust binary of the same
//! name (`codefang`/`uast`) under `target/release` (building both first if they
//! are absent) and keeps the rest of argv verbatim. Because argv is passed
//! straight to `Command` (no shell), selectors like `*` / `static/*` reach the
//! binary literally — the `set -f` (noglob) requirement of the manual protocol
//! is satisfied structurally.
//!
//! For `run/*` captures (the kubernetes repo is large) `--no-cache` is always
//! ensured on argv so no cross-run cache state can leak in.
//!
//! Usage:
//!   cargo run -p golden-harness                  # run all binding captures
//!   cargo run -p golden-harness -- -k <substr>   # only captures whose id or
//!   cargo run -p golden-harness -- <substr>...   # relPath contains a substr

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Manifest {
    env: BTreeMap<String, String>,
    captures: Vec<Capture>,
}

#[derive(Debug, Deserialize)]
struct Capture {
    id: String,
    argv: Vec<String>,
    #[serde(rename = "relPath")]
    rel_path: String,
    #[serde(default, rename = "nonBinding")]
    non_binding: bool,
}

/// `tests/golden` — holds MANIFEST.json and the goldens.
fn golden_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = tests/golden-harness
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // tests/
    p.push("golden");
    p
}

/// `target` — workspace target directory.
fn target_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // tests/
    p.pop(); // rust/
    p.push("target");
    p
}

/// Locate a built binary, preferring `release`, then `debug`.
fn binary_path(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let t = target_dir();
    for profile in ["release", "debug"] {
        let cand = t.join(profile).join(&exe);
        if cand.exists() {
            return Some(cand);
        }
    }
    None
}

/// Build the two release binaries if either is missing.
fn ensure_binaries_built() {
    if binary_path("codefang").is_some() && binary_path("uast").is_some() {
        return;
    }
    eprintln!("golden-harness: building release binaries (codefang, uast)…");
    let status = Command::new(env!("CARGO"))
        .args(["build", "--release", "-p", "codefang", "-p", "uast"])
        .status()
        .expect("spawn cargo build");
    assert!(status.success(), "cargo build --release failed");
}

/// Which Rust binary handles this capture, from the manifest argv[0] basename.
fn binary_for(cap: &Capture) -> &'static str {
    let arg0 = cap.argv.first().map(String::as_str).unwrap_or("");
    if arg0.ends_with("uast") {
        "uast"
    } else {
        "codefang"
    }
}

/// First differing byte offset, or `None` when identical.
fn first_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    let n = a.len().min(b.len());
    (0..n)
        .find(|&i| a[i] != b[i])
        .or(if a.len() == b.len() { None } else { Some(n) })
}

fn main() {
    let gdir = golden_dir();
    let manifest_path = gdir.join("MANIFEST.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display()));
    let manifest: Manifest =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse MANIFEST.json: {e}"));

    // Optional id/relPath substring filters: bare args or `-k <substr>` / `--id <substr>`.
    let mut filters: Vec<String> = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "-k" | "--id" | "--filter" => {
                if let Some(v) = it.next() {
                    filters.push(v);
                }
            }
            other => filters.push(other.to_string()),
        }
    }
    let selected = |cap: &Capture| -> bool {
        if filters.is_empty() {
            return true;
        }
        filters
            .iter()
            .any(|f| cap.id.contains(f.as_str()) || cap.rel_path.contains(f.as_str()))
    };

    ensure_binaries_built();

    // All BINDING captures (nonBinding != true), in manifest order, after filter.
    let binding: Vec<&Capture> = manifest
        .captures
        .iter()
        .filter(|c| !c.non_binding && selected(c))
        .collect();

    let total = binding.len();
    let mut identical = 0usize;
    let mut failures = 0usize;

    println!("== golden-harness: {total} binding captures ==");
    for cap in &binding {
        let rel = &cap.rel_path;

        let bin = binary_for(cap);
        let bin_path = match binary_path(bin) {
            Some(p) => p,
            None => {
                println!("ERROR     {rel}: binary `{bin}` not found under target/");
                failures += 1;
                continue;
            }
        };

        let golden_path = gdir.join(&cap.rel_path);
        let golden = match std::fs::read(&golden_path) {
            Ok(b) => b,
            Err(e) => {
                println!("ERROR     {rel}: missing golden {}: {e}", cap.rel_path);
                failures += 1;
                continue;
            }
        };

        // Swap argv[0] (Go binary) for the Rust binary; keep the rest verbatim.
        let mut args: Vec<String> = cap.argv.iter().skip(1).cloned().collect();
        // For run/* captures the kubernetes repo is large: always ensure
        // --no-cache so no cross-run cache state leaks in.
        if rel.starts_with("run/") && !args.iter().any(|a| a == "--no-cache") {
            args.insert(0, "--no-cache".to_string());
        }

        let mut cmd = Command::new(&bin_path);
        cmd.args(&args);
        // Match the manual protocol `env TZ=… NO_COLOR=… … <cmd>`: inherit the
        // ambient environment and OVERRIDE only the pinned golden vars (so the
        // dynamic loader / libgit2 keep PATH/HOME, while TZ/LANG/LC_ALL/NO_COLOR/
        // SOURCE_DATE_EPOCH are forced to the golden values).
        for (k, v) in &manifest.env {
            cmd.env(k, v);
        }

        let output = cmd
            .output()
            .unwrap_or_else(|e| panic!("spawn {}: {e}", bin_path.display()));

        match first_diff(&output.stdout, &golden) {
            None => {
                identical += 1;
                println!("IDENTICAL {rel}");
            }
            Some(off) => {
                failures += 1;
                println!(
                    "DIFFER    {rel} (first diff at byte {off}; rust={} golden={})",
                    output.stdout.len(),
                    golden.len(),
                );
                if !output.stderr.is_empty() {
                    let tail = String::from_utf8_lossy(&output.stderr);
                    if let Some(last) = tail.lines().last() {
                        println!("            stderr: {last}");
                    }
                }
            }
        }
    }

    println!("---");
    println!("{identical}/{total} identical");

    if failures > 0 {
        std::process::exit(1);
    }
}
