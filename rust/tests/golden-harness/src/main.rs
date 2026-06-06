//! Runnable golden-harness (ROADMAP Step 14 / DESIGN §6).
//!
//! `cargo run -p golden-harness` runs the **7 binding JSON captures** from
//! `rust/tests/golden/MANIFEST.json` under the exact golden environment
//! (`TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C SOURCE_DATE_EPOCH=315532800`), captures
//! STDOUT only, byte-compares it against the golden at `relPath`, prints a
//! per-capture `IDENTICAL`/`DIFFER` line and a final `N/7 identical`, and exits
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
//! Usage:
//!   cargo run -p golden-harness            # run all 7 binding captures
//!   cargo run -p golden-harness -- <id>... # run only the captures whose id or
//!                                          # relPath contains one of <id>...

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

use serde::Deserialize;

/// The 7 BINDING captures (machine && !nonBinding), by `relPath`. These are the
/// only captures that count toward the pass/fail tally (ROADMAP "Verification
/// protocol"). Order matches the ROADMAP table (uast first, then run).
const BINDING_REL_PATHS: &[&str] = &[
    "uast/parse.json",
    "uast/analyze.json",
    "uast/query.json",
    "run/history_typos.json",
    "run/history_imports.json",
    "run/history_anomaly.json",
    "run/history_devs.json",
];

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
}

/// `rust/tests/golden` — holds MANIFEST.json and the goldens.
fn golden_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = rust/tests/golden-harness
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // tests/
    p.push("golden");
    p
}

/// `rust/target` — workspace target directory (two levels up from the golden
/// dir: rust/tests/golden -> rust/tests -> rust, then `target`).
fn target_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // tests/
    p.pop(); // rust/
    p.push("target");
    p
}

/// Locate a built binary, preferring `release`, then `debug`.
fn binary_path(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
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
    if arg0.ends_with("uast") { "uast" } else { "codefang" }
}

/// First differing byte offset, or `None` when identical.
fn first_diff(a: &[u8], b: &[u8]) -> Option<usize> {
    let n = a.len().min(b.len());
    (0..n).find(|&i| a[i] != b[i]).or(if a.len() == b.len() { None } else { Some(n) })
}

fn main() {
    let gdir = golden_dir();
    let manifest_path = gdir.join("MANIFEST.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display()));
    let manifest: Manifest =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse MANIFEST.json: {e}"));

    // Optional id/relPath substring filters from argv.
    let filters: Vec<String> = std::env::args().skip(1).collect();
    let selected = |cap: &Capture| -> bool {
        if filters.is_empty() {
            return true;
        }
        filters
            .iter()
            .any(|f| cap.id.contains(f.as_str()) || cap.rel_path.contains(f.as_str()))
    };

    ensure_binaries_built();

    // Index captures by relPath for the fixed binding order.
    let by_rel: BTreeMap<&str, &Capture> =
        manifest.captures.iter().map(|c| (c.rel_path.as_str(), c)).collect();

    let mut identical = 0usize;
    let mut considered = 0usize;
    let mut failures = 0usize;

    println!("== golden-harness: 7 binding captures ==");
    for rel in BINDING_REL_PATHS {
        let cap = match by_rel.get(rel) {
            Some(c) => *c,
            None => {
                println!("MISSING   {rel} (not in MANIFEST.json)");
                failures += 1;
                continue;
            }
        };
        if !selected(cap) {
            continue;
        }
        considered += 1;

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
        let args: Vec<&str> = cap.argv.iter().skip(1).map(String::as_str).collect();
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

    let total = if filters.is_empty() { BINDING_REL_PATHS.len() } else { considered };
    println!("---");
    println!("{identical}/{total} identical");

    if failures > 0 {
        std::process::exit(1);
    }
}
