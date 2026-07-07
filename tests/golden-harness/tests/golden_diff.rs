//! Golden-diff integration harness (DESIGN §6).
//!
//! Drives `tests/golden/MANIFEST.json`: for each capture it builds the
//! correct binary (codefang/uast), runs the invocation with the pinned env
//! (`env` block in the manifest), and byte-compares stdout against the golden
//! file at `outPath`, reporting the FIRST differing byte offset on mismatch.
//!
//! The manifest's `argv[0]` is the path to the Go binary used to capture the
//! golden; the harness replaces it with the freshly built Rust binary of the
//! matching name (codefang/uast) and keeps the rest of argv verbatim.
//!
//! SKIP policy: a capture is SKIPPED (not failed) when the Rust subcommand body
//! is still a stub. The scaffold's only implemented stdout paths are `version`
//! and `--help`; every `run`/`uast <subcmd>` capture in the current manifest is
//! therefore treated as stubbed until its body is ported. The set of
//! already-ported commands is listed in `IMPLEMENTED_PREFIXES`; everything else
//! is SKIPPED. Implemented binding captures (machine && !nonBinding) HARD-GATE;
//! non-binding captures are reported informationally and never gated.

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
    #[serde(rename = "outPath")]
    out_path: String,
    #[serde(default)]
    machine: bool,
    #[serde(rename = "nonBinding", default)]
    non_binding: bool,
}

/// argv subcommand chains whose Rust bodies are ported and should RUN (not skip).
/// Each entry is matched against the capture's argv after argv[0]. As bodies are
/// ported, add their leading tokens here (e.g. "run", "uast parse").
const IMPLEMENTED_PREFIXES: &[&[&str]] = &[
    &["version"], // codefang version
                  // uast version is dispatched via the uast binary; see `is_implemented`.
];

/// Directory holding MANIFEST.json and the goldens.
fn golden_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = tests/golden-harness
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // tests/
    p.push("golden");
    p
}

/// Locate a built binary in the target directory. The test executable lives in
/// `target/<profile>/deps/`, so the binary is one or two levels up.
fn binary_path(name: &str) -> Option<PathBuf> {
    let test_exe = std::env::current_exe().ok()?;
    let deps_dir = test_exe.parent()?; // target/<profile>/deps
    let profile_dir = deps_dir.parent()?; // target/<profile>
    let exe_name = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let mut candidates = vec![profile_dir.join(&exe_name), deps_dir.join(&exe_name)];
    if let Some(target_dir) = profile_dir.parent() {
        candidates.push(target_dir.join("debug").join(&exe_name));
        candidates.push(target_dir.join("release").join(&exe_name));
    }
    candidates.into_iter().find(|c| c.exists())
}

/// Which Rust binary handles this capture, from the original argv[0] basename.
fn binary_for(capture: &Capture) -> &'static str {
    let arg0 = capture.argv.first().map(String::as_str).unwrap_or("");
    if arg0.ends_with("uast") {
        "uast"
    } else {
        "codefang"
    }
}

/// Is the subcommand chain for this capture already ported (so it should RUN)?
fn is_implemented(capture: &Capture, bin: &str) -> bool {
    // argv[1..] is the codefang/uast subcommand + flags.
    let rest: Vec<&str> = capture.argv.iter().skip(1).map(String::as_str).collect();
    // uast version / codefang version are implemented.
    if rest.first() == Some(&"version") {
        return true;
    }
    let _ = bin;
    IMPLEMENTED_PREFIXES
        .iter()
        .any(|prefix| rest.len() >= prefix.len() && rest[..prefix.len()] == **prefix)
}

/// First differing byte offset between two slices, or None if identical.
fn first_diff_offset(a: &[u8], b: &[u8]) -> Option<usize> {
    let n = a.len().min(b.len());
    for i in 0..n {
        if a[i] != b[i] {
            return Some(i);
        }
    }
    if a.len() != b.len() {
        Some(n)
    } else {
        None
    }
}

#[test]
fn golden_diff() {
    let gdir = golden_dir();
    let manifest_path = gdir.join("MANIFEST.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", manifest_path.display()));
    let manifest: Manifest =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse MANIFEST.json: {e}"));

    let mut skipped = Vec::new();
    let mut hard_failures = Vec::new();
    let mut informational = Vec::new();
    let mut passed = Vec::new();

    for cap in &manifest.captures {
        let bin = binary_for(cap);

        if !is_implemented(cap, bin) {
            skipped.push(format!("SKIP (stubbed body) {}", cap.id));
            continue;
        }

        let bin_path = match binary_path(bin) {
            Some(p) => p,
            None => {
                hard_failures.push(format!("{}: built binary `{bin}` not found", cap.id));
                continue;
            }
        };

        let golden_path = gdir.join(&cap.out_path);
        let golden = match std::fs::read(&golden_path) {
            Ok(b) => b,
            Err(e) => {
                let msg = format!("{}: missing golden {}: {e}", cap.id, cap.out_path);
                if cap.machine && !cap.non_binding {
                    hard_failures.push(msg);
                } else {
                    informational.push(msg);
                }
                continue;
            }
        };

        // Replace argv[0] (the Go binary) with the Rust binary; keep the rest.
        let args: Vec<&str> = cap.argv.iter().skip(1).map(String::as_str).collect();
        let mut cmd = Command::new(&bin_path);
        cmd.args(&args).current_dir(&gdir);
        for (k, v) in &manifest.env {
            cmd.env(k, v);
        }
        let output = cmd
            .output()
            .unwrap_or_else(|e| panic!("spawn {}: {e}", bin_path.display()));

        match first_diff_offset(&output.stdout, &golden) {
            None => passed.push(cap.id.clone()),
            Some(off) => {
                let msg = format!(
                    "{}: stdout differs from {} at byte offset {} (rust_len={}, golden_len={})",
                    cap.id,
                    cap.out_path,
                    off,
                    output.stdout.len(),
                    golden.len(),
                );
                if cap.machine && !cap.non_binding {
                    hard_failures.push(msg);
                } else {
                    informational.push(msg);
                }
            }
        }
    }

    eprintln!("== golden harness report ==");
    for s in &skipped {
        eprintln!("  {s}");
    }
    for p in &passed {
        eprintln!("  PASS {p}");
    }
    for i in &informational {
        eprintln!("  INFO {i}");
    }
    for f in &hard_failures {
        eprintln!("  FAIL {f}");
    }
    eprintln!(
        "skipped={} passed={} informational={} hard_failures={}",
        skipped.len(),
        passed.len(),
        informational.len(),
        hard_failures.len()
    );

    assert!(
        hard_failures.is_empty(),
        "{} binding golden(s) failed; see report above",
        hard_failures.len()
    );
}

/// Sanity: the goldens directory and manifest must exist and parse.
#[test]
fn manifest_loads() {
    let gdir = golden_dir();
    assert!(gdir.exists(), "golden dir missing: {}", gdir.display());
    let raw = std::fs::read_to_string(gdir.join("MANIFEST.json")).expect("read manifest");
    let m: Manifest = serde_json::from_str(&raw).expect("parse manifest");
    assert!(!m.captures.is_empty(), "manifest has no captures");
}
