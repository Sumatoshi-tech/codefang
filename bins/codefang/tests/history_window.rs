//! End-to-end coverage of the `--limit` / `--since` commit window, driven
//! through the **real binary** over a real on-disk git repository.
//!
//! The window contract (see `specs/frds/FRD-newest-commit-window.md`):
//! `--limit N` takes the N NEWEST commits, `--since C` takes every commit with
//! author time >= C, and both are delivered to the analyzers oldest-first.
//! Reports are asserted in machine formats only (`ndjson` / `json`), which are
//! the byte-stable formats the golden harness also treats as binding.

use cf_gitlib::testutil::{commit_files, init_repo, TestRepo};
use cf_gitlib::Hash;

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture history: 2021-01-01T00:00:00Z.
const T0: i64 = 1_609_459_200;
/// One day between fixture commits, so every cutoff lands between two commits.
const DAY: i64 = 86_400;
/// Fixture length: five linear commits; `hashes[0]` is the root.
const N_COMMITS: usize = 5;
/// Cutoff naming the 3rd commit's instant; commits 1-2 fall outside it.
const CUTOFF_3RD: &str = "2021-01-03T00:00:00Z";
/// A newer-than-HEAD cutoff: excludes the whole history.
const CUTOFF_FUTURE: &str = "2030-01-01";

/// The ndjson key that carries a commit hash, and the byte offset of the hash
/// value behind it (`"hash":"` is 8 bytes, a SHA-1 is 40).
const NEEDLE_HASH: &str = "\"hash\":\"";

/// Runs the binary with a deterministic environment; state is disabled so the
/// run is self-contained (same discipline as the golden captures).
fn codefang(args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(["run", "--checkpoint=false", "--resume=false", "--no-cache"])
        .args(args)
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .output()
        .expect("run the codefang binary")
}

use std::process::Command;

/// Builds the five-commit fixture; returns it with its hashes, oldest-first.
fn fixture() -> (TestRepo, Vec<Hash>) {
    let test = init_repo().expect("init fixture repo");
    let mut hashes = Vec::new();
    for i in 0..N_COMMITS {
        let h = commit_files(
            &test,
            &format!("commit {i}"),
            T0 + i64::try_from(i).expect("index fits i64") * DAY,
            &[("f.txt", format!("content v{i}\n").as_bytes())],
        )
        .expect("fixture commit");
        hashes.push(h);
    }
    (test, hashes)
}

/// The commit hashes of a `burndown --format ndjson` run, in emitted order,
/// together with `(exit code, stdout, stderr)`.
fn ndjson_window(path: &str, extra: &[&str]) -> (i32, Vec<String>, String, String) {
    let out = codefang(
        &[
            &["-a", "history/burndown", "--workers", "1", "--format", "ndjson"],
            extra,
            &[path],
        ]
        .concat(),
    );
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let hashes = stdout
        .lines()
        .filter_map(|line| line.find(NEEDLE_HASH).map(|i| line[i + 8..i + 48].to_owned()))
        .collect();
    (
        out.status.code().unwrap_or(-1),
        hashes,
        stdout,
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `--limit N` selects the N NEWEST commits, delivered oldest-first.
#[test]
fn limit_selects_the_newest_commits() {
    let (test, hashes) = fixture();
    let path = test.path().to_str().expect("utf-8 path");
    let (rc, got, stdout, stderr) = ndjson_window(path, &["--limit", "2"]);
    assert_eq!(rc, 0, "run failed: {stderr}");
    assert_eq!(
        got,
        vec![hashes[3].to_string(), hashes[4].to_string()],
        "--limit 2 must take the two NEWEST commits oldest-first, got {got:?} in {stdout}"
    );
}

/// The user-visible commit count follows `--limit` and grows with it.
#[test]
fn limit_bound_shows_in_the_report() {
    let (test, _hashes) = fixture();
    let path = test.path().to_str().expect("utf-8 path");
    for (limit, want) in [("1", 1), ("2", 2), ("5", 5)] {
        let out = codefang(&[
            "-a", "history/devs", "--limit", limit, "--format", "json", path,
        ]);
        assert!(out.status.success(), "run failed for --limit {limit}");
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let needle = format!(r#""total_commits":{want}"#);
        assert!(
            stdout.contains(&needle),
            "--limit {limit} must report {want} commits ({needle}):\n{stdout}"
        );
    }
}

/// A `--since` cutoff in the MIDDLE of the history yields exactly that window.
/// This is the case the reference-parity loader aborted on with empty stdout.
#[test]
fn since_partial_window_succeeds_with_the_window() {
    let (test, hashes) = fixture();
    let path = test.path().to_str().expect("utf-8 path");
    let (rc, got, _stdout, stderr) = ndjson_window(path, &["--since", CUTOFF_3RD]);
    assert_eq!(rc, 0, "a partial --since window must exit 0, stderr: {stderr}");
    assert_eq!(
        got,
        vec![
            hashes[2].to_string(),
            hashes[3].to_string(),
            hashes[4].to_string()
        ],
        "--since must analyze exactly the in-window commits"
    );
}

/// `--since` also accepts a git revision.
#[test]
fn since_accepts_a_revision() {
    let (test, hashes) = fixture();
    let path = test.path().to_str().expect("utf-8 path");
    let (rc, got, _stdout, stderr) = ndjson_window(path, &["--since", "HEAD~2"]);
    assert_eq!(rc, 0, "run failed: {stderr}");
    assert_eq!(
        got.first().map(String::as_str),
        Some(hashes[2].to_string().as_str()),
        "`--since HEAD~2` must start the window at that commit"
    );
    assert_eq!(got.len(), 3, "window must hold HEAD~2..HEAD");
}

/// A cutoff newer than every commit analyzes nothing but still succeeds.
#[test]
fn since_after_head_is_empty_and_succeeds() {
    let (test, _hashes) = fixture();
    let path = test.path().to_str().expect("utf-8 path");
    let (rc, got, _stdout, stderr) = ndjson_window(path, &["--since", CUTOFF_FUTURE]);
    assert_eq!(rc, 0, "an empty window is not an error: {stderr}");
    assert!(got.is_empty(), "no commit is in window, got {got:?}");
}

/// An unresolvable `--since` aborts with empty stdout, exit 1, and a message
/// that names the flag rather than the internal dispatch placeholder.
#[test]
fn unresolvable_since_fails_with_a_named_diagnostic() {
    let (test, _hashes) = fixture();
    let path = test.path().to_str().expect("utf-8 path");
    let (rc, _got, stdout, stderr) = ndjson_window(path, &["--since", "not-a-time-or-ref"]);
    assert_eq!(rc, 1, "an unresolvable --since must exit 1");
    assert!(stdout.is_empty(), "stdout must stay empty, got {stdout}");
    assert!(
        stderr.contains("cannot resolve --since"),
        "the error must name the flag, stderr: {stderr}"
    );
    assert!(
        !stderr.contains("dispatch is blocked"),
        "the failure must not be misattributed to the port, stderr: {stderr}"
    );
}

/// `--head` means "exactly HEAD", so it must equal `--limit 1` (which now
/// selects HEAD) and must not be widened by a larger `--limit`.
#[test]
fn head_overrides_limit() {
    let (test, _hashes) = fixture();
    let path = test.path().to_str().expect("utf-8 path");
    let report = |extra: &[&str]| {
        let out = codefang(
            &[
                &["-a", "history/burndown", "--format", "json"],
                extra,
                &[path],
            ]
            .concat(),
        );
        assert!(out.status.success(), "run failed: {:?}", String::from_utf8_lossy(&out.stderr));
        out.stdout
    };
    let head = report(&["--head"]);
    let newest_one = report(&["--limit", "1"]);
    assert_eq!(
        head, newest_one,
        "--limit 1 must select exactly HEAD, so both reports must be identical bytes"
    );
    assert_eq!(
        head,
        report(&["--head", "--limit", "3"]),
        "--head must ignore --limit"
    );
}
