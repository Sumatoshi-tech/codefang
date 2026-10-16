//! End-to-end coverage of `history/sentiment`, driven through the **real
//! binary** over real on-disk git fixtures
//! (`specs/bugs/BUG-history-sentiment-scoring.md`).
//!
//! Every assertion is a contract from `site/analyzers/sentiment.md`: only
//! the comments a commit introduced or changed are scored, only ticks with
//! such comments carry a score, scores and trend endpoints lie in `[0, 1]`,
//! Javadoc blocks count, `--min-comment-len` is applied, and the output is
//! deterministic across runs and formats.

mod common;

use cf_gitlib::testutil::{commit_files, init_repo, TestRepo};
use serde_json::Value;

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture histories: 2021-01-01T12:00:00Z.
const T0: i64 = 1_609_502_400;
/// One day between fixture commits (one tick at the default tick size).
const DAY: i64 = 24 * 3_600;
/// The documented positive / negative classification thresholds.
const POSITIVE_THRESHOLD: f64 = 0.6;
const NEGATIVE_THRESHOLD: f64 = 0.4;
/// A `--min-comment-len` longer than every fixture comment.
const LONGER_THAN_EVERY_COMMENT: &str = "200";

const POSITIVE_GO: &[u8] =
    b"package main\n\n// This is great, excellent work and really wonderful code\nfunc A() {}\n";
const POSITIVE_PLUS_CODE_GO: &[u8] =
    b"package main\n\n// This is great, excellent work and really wonderful code\nfunc A() {}\n\nfunc B() {}\n";
const NEGATIVE_GO: &[u8] =
    b"package main\n\n// This is a terrible awful hack, horrible nightmare mess\nfunc N() {}\n";
const JAVADOC_GO: &[u8] = b"package main

/**
 * This function is absolutely wonderful and great
 * and it works beautifully every single time
 */
func J() {}
";
const NEUTRAL_PY: &[u8] = b"# a python file whose comment describes the input validation\ndef receive(x):\n    return x\n";

/// Runs the binary with a deterministic environment.
fn codefang(args: &[&str]) -> std::process::Output {
    common::command(BIN)
        .arg("run")
        .args(args)
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .output()
        .expect("run the codefang binary")
}

/// stdout of a successful run, as text.
fn stdout_ok(args: &[&str]) -> String {
    let out = codefang(args);
    assert!(
        out.status.success(),
        "run failed ({args:?}): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

/// The sentiment report of `repo` as JSON, with extra flags.
fn sentiment_json(repo: &str, extra: &[&str]) -> Value {
    let args = [
        &["-a", "history/sentiment", "--format", "json"],
        extra,
        &[repo],
    ]
    .concat();
    let text = stdout_ok(&args);
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("stdout is not JSON ({e}):\n{text}"))
}

fn path(test: &TestRepo) -> &str {
    test.path().to_str().expect("utf-8 path")
}

fn series(report: &Value) -> &Vec<Value> {
    report["time_series"]
        .as_array()
        .unwrap_or_else(|| panic!("no time_series in {report}"))
}

fn ticks(report: &Value) -> Vec<i64> {
    series(report)
        .iter()
        .map(|e| e["tick"].as_i64().unwrap())
        .collect()
}

fn sentiment(entry: &Value) -> f64 {
    entry["sentiment"]
        .as_f64()
        .unwrap_or_else(|| panic!("no numeric sentiment in {entry}"))
}

/// The audit fixture: positive comment, code-only commit, negative comment,
/// Javadoc block, Python comment — one commit per day.
fn audit_fixture() -> TestRepo {
    let test = init_repo().expect("init");
    commit_files(&test, "positive", T0, &[("s.go", POSITIVE_GO)]).expect("s1");
    commit_files(
        &test,
        "code only",
        T0 + DAY,
        &[("s.go", POSITIVE_PLUS_CODE_GO)],
    )
    .expect("s2");
    commit_files(&test, "negative", T0 + 2 * DAY, &[("n.go", NEGATIVE_GO)]).expect("s3");
    commit_files(&test, "javadoc", T0 + 3 * DAY, &[("j.go", JAVADOC_GO)]).expect("s4");
    commit_files(&test, "python", T0 + 4 * DAY, &[("x.py", NEUTRAL_PY)]).expect("s5");
    test
}

/// Positive and negative comments give ordered scores in `[0, 1]` with the
/// documented classifications; the code-only commit's tick carries no score
/// and is not a low-sentiment period; the Javadoc block is scored.
#[test]
fn scores_are_ordered_and_only_commented_ticks_are_scored() {
    let test = audit_fixture();
    let report = sentiment_json(path(&test), &[]);

    assert_eq!(ticks(&report), vec![0, 2, 3, 4], "{report}");
    let by_tick = |t: i64| {
        series(&report)
            .iter()
            .find(|e| e["tick"] == t)
            .unwrap_or_else(|| panic!("no tick {t} in {report}"))
    };
    let positive = sentiment(by_tick(0));
    let negative = sentiment(by_tick(2));
    let javadoc = sentiment(by_tick(3));
    for s in [positive, negative, javadoc, sentiment(by_tick(4))] {
        assert!((0.0..=1.0).contains(&s), "score out of range: {report}");
    }
    assert!(positive > negative, "{report}");
    assert!(positive >= POSITIVE_THRESHOLD, "{report}");
    assert!(negative <= NEGATIVE_THRESHOLD, "{report}");
    assert_eq!(by_tick(0)["classification"], "positive");
    assert_eq!(by_tick(2)["classification"], "negative");
    assert_eq!(by_tick(3)["comment_count"], 1, "javadoc counted: {report}");
    assert!(javadoc >= POSITIVE_THRESHOLD, "{report}");

    let low = report["low_sentiment_periods"]
        .as_array()
        .unwrap_or_else(|| panic!("no low_sentiment_periods in {report}"));
    assert_eq!(low.len(), 1, "{report}");
    assert_eq!(low[0]["tick"], 2);
    assert_eq!(report["aggregate"]["total_ticks"], 4);
    assert_eq!(report["aggregate"]["total_commits"], 5);
    assert_eq!(report["aggregate"]["total_comments"], 4);
}

/// Unchanged comments are not re-counted: a commit that only adds code to a
/// commented file contributes zero comments, in the per-commit stream too.
#[test]
fn unchanged_comments_are_not_recounted() {
    let test = audit_fixture();
    let p = path(&test);
    let ndjson = stdout_ok(&["-a", "history/sentiment", "--format", "ndjson", p]);
    let lines: Vec<&str> = ndjson.lines().collect();
    assert_eq!(lines.len(), 5, "{ndjson}");
    assert!(
        lines[0].contains("This is great, excellent work"),
        "{}",
        lines[0]
    );
    assert!(
        lines[1].contains(r#""Comments":[]"#),
        "the code-only commit must carry no comment: {}",
        lines[1]
    );

    // Editing the comment counts it once as new text.
    let edited = b"package main\n\n// This is still great, excellent work and really wonderful code\nfunc A() {}\n\nfunc B() {}\n";
    commit_files(&test, "edit comment", T0 + 5 * DAY, &[("s.go", edited)]).expect("s6");
    let ndjson = stdout_ok(&["-a", "history/sentiment", "--format", "ndjson", p]);
    let last = ndjson.lines().last().expect("a record");
    assert!(last.contains("still great"), "{last}");
    assert_eq!(last.matches("excellent work").count(), 1, "{last}");
}

/// Trend endpoints lie in `[0, 1]` even when the fitted line overshoots.
#[test]
fn trend_endpoints_stay_in_the_unit_interval() {
    let test = init_repo().expect("init");
    commit_files(&test, "p1", T0, &[("a.go", POSITIVE_GO)]).expect("p1");
    commit_files(&test, "p2", T0 + DAY, &[("b.go", JAVADOC_GO)]).expect("p2");
    commit_files(&test, "n", T0 + 2 * DAY, &[("c.go", NEGATIVE_GO)]).expect("n");
    let report = sentiment_json(path(&test), &[]);
    let trend = &report["trend"];
    let start = trend["start_sentiment"].as_f64().unwrap();
    let end = trend["end_sentiment"].as_f64().unwrap();
    assert!((0.0..=1.0).contains(&start), "{trend}");
    assert!((0.0..=1.0).contains(&end), "{trend}");
    assert_eq!(trend["trend_direction"], "declining", "{trend}");
    let change = trend["change_percent"].as_f64().unwrap();
    assert!(change >= -100.0, "{trend}");
    assert!(
        ((end - start) / start * 100.0 - change).abs() < 1e-3,
        "{trend}"
    );
}

/// `--min-comment-len` is applied: a threshold above every comment length
/// leaves no scored tick; a negative value is a named error.
#[test]
fn min_comment_len_flag_is_applied_and_validated() {
    let test = audit_fixture();
    let p = path(&test);
    let default = sentiment_json(p, &[]);
    assert_eq!(default["aggregate"]["total_comments"], 4, "{default}");

    let strict = sentiment_json(p, &["--min-comment-len", LONGER_THAN_EVERY_COMMENT]);
    assert_eq!(strict["aggregate"]["total_comments"], 0, "{strict}");
    assert_eq!(strict["aggregate"]["total_ticks"], 0, "{strict}");
    assert_eq!(strict["aggregate"]["total_commits"], 5, "{strict}");
    assert!(ticks(&strict).is_empty(), "{strict}");

    let out = codefang(&[
        "-a",
        "history/sentiment",
        "--format",
        "json",
        "--min-comment-len=-1",
        p,
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Error: invalid --min-comment-len: must be >= 0, got -1"),
        "{err}"
    );
}

/// Runs are deterministic and `json` / `yaml` agree.
#[test]
fn runs_are_deterministic_and_formats_agree() {
    let test = audit_fixture();
    let p = path(&test);
    let first = stdout_ok(&["-a", "history/sentiment", "--format", "json", p]);
    let second = stdout_ok(&["-a", "history/sentiment", "--format", "json", p]);
    assert_eq!(first, second, "json output differs between runs");

    let json: Value = serde_json::from_str(&first).expect("json");
    let yaml = stdout_ok(&["-a", "history/sentiment", "--format", "yaml", p]);
    let agg = &json["aggregate"];
    for key in [
        "total_ticks",
        "total_comments",
        "total_commits",
        "positive_ticks",
    ] {
        let line = format!("{key}: {}", agg[key]);
        assert!(yaml.contains(&line), "yaml lacks {line:?}:\n{yaml}");
    }
    assert!(
        yaml.contains(&format!("average_sentiment: {}", agg["average_sentiment"])),
        "{yaml}"
    );
    for entry in series(&json) {
        let line = format!("sentiment: {}", entry["sentiment"]);
        assert!(yaml.contains(&line), "yaml lacks {line:?}:\n{yaml}");
    }
}
