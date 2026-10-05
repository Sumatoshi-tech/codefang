//! End-to-end coverage of `history/anomaly`, driven through the **real
//! binary** over on-disk git fixtures
//! (`specs/bugs/BUG-history-anomaly-files.md`).
//!
//! The contract (`site/analyzers/anomaly.md`): per tick, `files_changed` is
//! the number of DISTINCT files touched and `files` lists them (a delete
//! names the deleted file); the Z-score of a tick is computed over the
//! trailing window of the previous `--anomaly-window` ticks (population
//! standard deviation) and needs at least two observations before a
//! zero-variance window can flag anything; `anomalies` is always a list;
//! `--anomaly-window` is applied and validated.

mod common;

use cf_gitlib::testutil::{commit_files, delete_files, init_repo, TestRepo};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture histories: 2021-01-01T12:00:00Z.
const T0: i64 = 1_609_502_400;
/// One hour.
const HOUR: i64 = 3_600;
/// One day between quiet fixture commits.
const DAY: i64 = 24 * HOUR;
/// Number of quiet days before the busy day.
const QUIET_DAYS: i64 = 10;
/// Lines of a quiet-day file on even ticks.
const QUIET_LINES_EVEN: usize = 10;
/// Lines of a quiet-day file on odd ticks.
const QUIET_LINES_ODD: usize = 12;
/// New files added on the busy day.
const BURST_NEW_FILES: usize = 10;
/// Appends to the first burst file on the busy day.
const BURST_APPENDS: usize = 10;
/// Lines per burst write.
const BURST_LINES: usize = 10;
/// The busy day's tick index (ticks are 0-based, one per quiet day).
const BUSY_TICK: i64 = QUIET_DAYS;
/// Distinct files touched on the busy day: the new files plus the deleted
/// quiet file.
const BUSY_DISTINCT_FILES: i64 = BURST_NEW_FILES as i64 + 1;
/// Lines added on the busy day: `(new files + appends) * lines`.
const BUSY_LINES_ADDED: i64 = ((BURST_NEW_FILES + BURST_APPENDS) * BURST_LINES) as i64;
/// Lines removed on the busy day: the deleted `q1.txt`.
const BUSY_LINES_REMOVED: i64 = QUIET_LINES_EVEN as i64;
/// Mean of the ten quiet net-churn values `[10, 12, 10, 12, …]`.
const QUIET_MEAN: f64 = 11.0;
/// Population standard deviation of the quiet values.
const QUIET_STDDEV: f64 = 1.0;
/// The zero-variance sentinel magnitude.
const SENTINEL: f64 = 100.0;

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

/// stdout of a successful run.
fn stdout_ok(args: &[&str]) -> Vec<u8> {
    let out = codefang(args);
    assert!(
        out.status.success(),
        "run failed ({args:?}): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

/// stdout of a successful run, parsed as ONE JSON document.
fn json_ok(args: &[&str]) -> serde_json::Value {
    let bytes = stdout_ok(args);
    serde_json::from_slice(&bytes).unwrap_or_else(|e| {
        panic!(
            "stdout is not one JSON document ({e}):\n{}",
            String::from_utf8_lossy(&bytes)
        )
    })
}

fn path(test: &TestRepo) -> &str {
    test.path().to_str().expect("utf-8 path")
}

/// `n` numbered lines tagged with `tag`.
fn lines(tag: &str, n: usize) -> Vec<u8> {
    (1..=n)
        .map(|i| format!("line {i} of {tag}\n"))
        .collect::<String>()
        .into_bytes()
}

/// Ten quiet days (one commit each, alternating 10/12 added lines) and one
/// busy day: ten new files, ten appends to the first of them, and the
/// deletion of `q1.txt` — 21 commits touching 11 distinct files.
fn burst_fixture() -> TestRepo {
    let test = init_repo().expect("init");
    for d in 0..QUIET_DAYS {
        let n = if d % 2 == 0 {
            QUIET_LINES_EVEN
        } else {
            QUIET_LINES_ODD
        };
        let name = format!("q{}.txt", d + 1);
        commit_files(
            &test,
            &format!("quiet {d}"),
            T0 + d * DAY,
            &[(name.as_str(), lines(&name, n).as_slice())],
        )
        .expect("quiet");
    }
    let busy = T0 + QUIET_DAYS * DAY;
    let mut minute = 0;
    for k in 1..=BURST_NEW_FILES {
        let name = format!("burst{k}.txt");
        commit_files(
            &test,
            &format!("burst {k}"),
            busy + minute * 60,
            &[(name.as_str(), lines(&name, BURST_LINES).as_slice())],
        )
        .expect("burst");
        minute += 1;
    }
    let mut body = lines("burst1.txt", BURST_LINES);
    for k in 1..=BURST_APPENDS {
        body.extend_from_slice(&lines(&format!("append{k}"), BURST_LINES));
        commit_files(
            &test,
            &format!("append {k}"),
            busy + minute * 60,
            &[("burst1.txt", body.as_slice())],
        )
        .expect("append");
        minute += 1;
    }
    delete_files(&test, "delete q1", busy + minute * 60, &["q1.txt"]).expect("delete");
    test
}

/// Three identical quiet commits: nothing to flag.
fn quiet_fixture() -> TestRepo {
    let test = init_repo().expect("init");
    for d in 0..3_i64 {
        let name = format!("q{d}.txt");
        commit_files(
            &test,
            &format!("quiet {d}"),
            T0 + d * DAY,
            &[(name.as_str(), lines(&name, QUIET_LINES_EVEN).as_slice())],
        )
        .expect("quiet");
    }
    test
}

/// The busy day is the ONE flagged tick, with the hand-computed Z-scores over
/// the default window, distinct `files_changed`, and the deleted file named.
#[test]
fn the_busy_day_is_flagged_with_hand_computed_z_scores() {
    let test = burst_fixture();
    let report = json_ok(&["-a", "history/anomaly", "--format", "json", path(&test)]);

    let anomalies = report["anomalies"].as_array().expect("anomalies");
    assert_eq!(anomalies.len(), 1, "{report}");
    let busy = &anomalies[0];
    assert_eq!(busy["tick"], BUSY_TICK, "{report}");

    let churn = BUSY_LINES_ADDED - BUSY_LINES_REMOVED;
    let z_churn = (churn as f64 - QUIET_MEAN) / QUIET_STDDEV;
    let z_added = (BUSY_LINES_ADDED as f64 - QUIET_MEAN) / QUIET_STDDEV;
    assert_eq!(busy["z_scores"]["net_churn"], z_churn, "{report}");
    assert_eq!(busy["z_scores"]["lines_added"], z_added, "{report}");
    // Every quiet tick touched exactly one file: zero variance, so the busy
    // day's 11 distinct files score the sentinel.
    assert_eq!(busy["z_scores"]["files_changed"], SENTINEL, "{report}");
    assert_eq!(busy["z_scores"]["lines_removed"], SENTINEL, "{report}");
    assert_eq!(busy["max_abs_z_score"], z_added.max(z_churn), "{report}");

    assert_eq!(
        busy["metrics"]["files_changed"], BUSY_DISTINCT_FILES,
        "files_changed counts distinct files: {report}"
    );
    assert_eq!(busy["metrics"]["lines_added"], BUSY_LINES_ADDED, "{report}");
    assert_eq!(
        busy["metrics"]["lines_removed"], BUSY_LINES_REMOVED,
        "{report}"
    );
    assert_eq!(busy["metrics"]["net_churn"], churn, "{report}");

    let files: Vec<&str> = busy["files"]
        .as_array()
        .expect("files")
        .iter()
        .map(|f| f.as_str().expect("file name"))
        .collect();
    assert_eq!(files.len(), BUSY_DISTINCT_FILES as usize, "{report}");
    assert!(
        files.contains(&"q1.txt"),
        "a delete names the file: {files:?}"
    );
    assert!(files.contains(&"burst1.txt"), "{files:?}");
    assert!(!files.contains(&""), "{files:?}");
    assert_eq!(
        report["aggregate"]["total_ticks"],
        QUIET_DAYS + 1,
        "{report}"
    );
    assert_eq!(report["aggregate"]["total_anomalies"], 1, "{report}");
}

/// The second tick is NOT flagged: a one-sample window has no variance to
/// speak of, so the sentinel needs at least two observations.
#[test]
fn a_single_observation_never_triggers_the_sentinel() {
    let test = burst_fixture();
    let report = json_ok(&["-a", "history/anomaly", "--format", "json", path(&test)]);
    let series = report["time_series"].as_array().expect("time_series");
    assert_eq!(series[1]["tick"], 1, "{report}");
    assert_eq!(series[1]["is_anomaly"], false, "{report}");
    assert_eq!(series[1]["churn_z_score"], 0.0, "{report}");
    // The time-series entry carries the raw metrics once, under `metrics`.
    assert!(series[1].get("language_diversity").is_none(), "{report}");
    assert!(series[1].get("author_count").is_none(), "{report}");
    assert_eq!(series[1]["metrics"]["author_count"], 1, "{report}");
}

/// `--anomaly-window` changes the baseline the busy day is scored against and
/// is reported in `aggregate.window_size`; an invalid value is a named error.
#[test]
fn anomaly_window_is_applied_and_validated() {
    let test = burst_fixture();
    let p = path(&test);
    let default = json_ok(&["-a", "history/anomaly", "--format", "json", p]);
    let narrow = json_ok(&[
        "-a",
        "history/anomaly",
        "--format",
        "json",
        "--anomaly-window",
        "3",
        p,
    ]);
    assert_eq!(default["aggregate"]["window_size"], 20, "{default}");
    assert_eq!(narrow["aggregate"]["window_size"], 3, "{narrow}");
    assert_eq!(narrow["anomalies"][0]["tick"], BUSY_TICK, "{narrow}");
    assert_ne!(
        narrow["anomalies"][0]["z_scores"]["net_churn"],
        default["anomalies"][0]["z_scores"]["net_churn"],
        "a three-tick window scores against a different baseline"
    );

    let out = codefang(&[
        "-a",
        "history/anomaly",
        "--format",
        "json",
        "--anomaly-window",
        "1",
        p,
    ]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Error: invalid --anomaly-window: must be >= 2, got 1"),
        "{err}"
    );
}

/// `--anomaly-threshold` is a registered flag: a threshold above the busy
/// day's z-score leaves nothing flagged, and a non-positive one is a named
/// error before any walk.
#[test]
fn anomaly_threshold_is_applied_and_validated() {
    let test = burst_fixture();
    let p = path(&test);
    let default = json_ok(&["-a", "history/anomaly", "--format", "json", p]);
    assert_eq!(default["anomalies"][0]["tick"], BUSY_TICK, "{default}");
    let strict = json_ok(&[
        "-a",
        "history/anomaly",
        "--format",
        "json",
        "--anomaly-threshold",
        "1000",
        p,
    ]);
    assert_eq!(strict["anomalies"], serde_json::json!([]), "{strict}");

    let out = codefang(&[
        "-a",
        "history/anomaly",
        "--format",
        "json",
        "--anomaly-threshold",
        "0",
        p,
    ]);
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("Error: invalid --anomaly-threshold: must be > 0, got 0"),
        "{err}"
    );
}

/// With nothing flagged, `anomalies` is `[]` in json and yaml alike, and the
/// bin payload is the json value.
#[test]
fn no_anomaly_is_an_empty_list_in_every_format() {
    let test = quiet_fixture();
    let p = path(&test);
    let report = json_ok(&["-a", "history/anomaly", "--format", "json", p]);
    assert_eq!(report["anomalies"], serde_json::json!([]), "{report}");
    assert_eq!(report["time_series"].as_array().map(Vec::len), Some(3));

    let yaml = String::from_utf8(stdout_ok(&["-a", "history/anomaly", "--format", "yaml", p]))
        .expect("utf-8 yaml");
    assert!(yaml.contains("anomalies: []"), "{yaml}");

    let json = stdout_ok(&["-a", "history/anomaly", "--format", "json", p]);
    let bin = stdout_ok(&["-a", "history/anomaly", "--format", "bin", p]);
    assert_eq!(&bin[8..], &json[..], "bin payload is the json value");
}

/// Per-commit ndjson records name the deleted file, never an empty string.
#[test]
fn ndjson_records_name_deleted_files() {
    let test = burst_fixture();
    let ndjson = String::from_utf8(stdout_ok(&[
        "-a",
        "history/anomaly",
        "--format",
        "ndjson",
        path(&test),
    ]))
    .expect("utf-8");
    let delete = ndjson
        .lines()
        .last()
        .expect("the delete commit is the newest record");
    assert!(delete.contains(r#""files":["q1.txt"]"#), "{delete}");
    assert!(!ndjson.contains(r#""files":[""]"#), "{ndjson}");
}
