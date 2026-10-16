//! End-to-end coverage of a MIXED static+history selection
//! (`-a static/complexity,history/devs`) in the formats whose content is
//! per-phase: `timeseries`, `timeseries --ndjson`, `text` and `compact`.
//!
//! A mixed run must carry each phase's own rendering: the history time
//! series (static analyzers have no per-commit data), the static text
//! sections followed by the history text, and one compact line per analyzer.

mod common;

use cf_gitlib::testutil::{commit_files, init_repo, TestRepo};

/// The binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture history: 2021-01-01T00:00:00Z.
const T0: i64 = 1_609_459_200;
/// One day between fixture commits.
const DAY: i64 = 86_400;
/// Number of fixture commits.
const N_COMMITS: usize = 3;
/// The mixed selection under test.
const MIXED: &str = "static/complexity,history/devs";

/// Three daily commits, each adding one Go function with one branch.
fn fixture() -> TestRepo {
    let test = init_repo().expect("init fixture repo");
    for i in 0..N_COMMITS {
        let source =
            format!("package main\n\nfunc f{i}(a int) int {{\n\tif a > {i} {{\n\t\treturn a\n\t}}\n\treturn 0\n}}\n");
        commit_files(
            &test,
            &format!("commit {i}"),
            T0 + i64::try_from(i).expect("index fits i64") * DAY,
            &[(format!("f{i}.go").as_str(), source.as_bytes())],
        )
        .expect("fixture commit");
    }
    test
}

/// Runs `codefang run <args>` and returns `(exit code, stdout, stderr)`.
fn run(args: &[&str]) -> (i32, String, String) {
    let out = common::command(BIN)
        .arg("run")
        .args(args)
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .output()
        .expect("run the codefang binary");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Runs a selection in a format and asserts a clean exit.
fn stdout_of(selection: &str, format_args: &[&str], path: &str) -> (String, String) {
    let args = [&["-a", selection], format_args, &[path]].concat();
    let (rc, stdout, stderr) = run(&args);
    assert_eq!(rc, 0, "{selection} {format_args:?} failed: {stderr}");
    (stdout, stderr)
}

/// The mixed time series is the history phase's time series; the static
/// analyzers, which have no per-commit data, are named on stderr.
#[test]
fn mixed_timeseries_is_the_history_time_series() {
    let test = fixture();
    let p = test.path().to_str().expect("utf-8 path");
    let (history, _) = stdout_of("history/devs", &["--format", "timeseries"], p);
    let (mixed, stderr) = stdout_of(MIXED, &["--format", "timeseries"], p);
    assert_eq!(
        mixed, history,
        "mixed timeseries must carry the history commits"
    );
    let doc: serde_json::Value = serde_json::from_str(&mixed).expect("timeseries json");
    assert_eq!(doc["commits"].as_array().map(Vec::len), Some(N_COMMITS));
    assert!(stderr.contains("static/complexity"), "{stderr}");
}

/// `--format timeseries --ndjson` in a mixed run: one line per commit, equal
/// to the history-only lines.
#[test]
fn mixed_timeseries_ndjson_is_the_history_lines() {
    let test = fixture();
    let p = test.path().to_str().expect("utf-8 path");
    let ts = ["--format", "timeseries", "--ndjson"];
    let (history, _) = stdout_of("history/devs", &ts, p);
    let (mixed, _) = stdout_of(MIXED, &ts, p);
    assert_eq!(mixed, history);
    assert_eq!(mixed.lines().count(), N_COMMITS);
}

/// Mixed text: the static sections, then the history text.
#[test]
fn mixed_text_renders_both_phases() {
    let test = fixture();
    let p = test.path().to_str().expect("utf-8 path");
    let (history, _) = stdout_of("history/devs", &["--format", "text"], p);
    let (mixed, _) = stdout_of(MIXED, &["--format", "text"], p);
    assert!(
        mixed.contains("COMPLEXITY"),
        "static section missing:\n{mixed}"
    );
    assert!(mixed.ends_with(&history), "history text missing:\n{mixed}");
}

/// Mixed compact: each phase's compact output, static first.
#[test]
fn mixed_compact_renders_both_phases() {
    let test = fixture();
    let p = test.path().to_str().expect("utf-8 path");
    let (static_only, _) = stdout_of("static/complexity", &["--format", "compact"], p);
    let (history, _) = stdout_of("history/devs", &["--format", "compact"], p);
    let (mixed, _) = stdout_of(MIXED, &["--format", "compact"], p);
    assert_eq!(mixed, format!("{static_only}{history}"));
}
