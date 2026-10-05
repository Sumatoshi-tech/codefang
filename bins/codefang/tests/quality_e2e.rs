//! End-to-end coverage of `history/quality`, driven through the **real
//! binary** over real on-disk git fixtures
//! (`specs/bugs/BUG-history-quality-ticks.md`).
//!
//! Every assertion is a contract from `site/analyzers/quality.md`: a tick's
//! statistics are the static analyzers' numbers over the files changed in
//! that tick, ticks without analyzed files are absent, the aggregate is
//! internally consistent (`min <= mean`), and a large commit is fully counted.

mod common;

use cf_gitlib::testutil::{commit_files, init_repo, TestRepo};
use serde_json::Value;

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture histories: 2021-01-01T12:00:00Z.
const T0: i64 = 1_609_502_400;
/// One day between fixture commits (one tick at the default tick size).
const DAY: i64 = 24 * 3_600;
/// Number of files in the "big commit" fixture.
const BIG_COMMIT_FILES: usize = 40;
/// Float tolerance for numbers that cross a YAML/JSON round trip.
const EPS: f64 = 1e-9;

/// `Add` with one `if`: cyclomatic 2.
const ADD_CC2_GO: &[u8] = b"package main

// Add returns the sum of a and b.
func Add(a, b int) int {
\tif a > b {
\t\treturn a + b
\t}
\treturn b
}
";

/// `Add` with three `if`s: cyclomatic 4.
const ADD_CC4_GO: &[u8] = b"package main

// Add returns the sum of a and b.
func Add(a, b int) int {
\tif a > b {
\t\treturn a + b
\t}
\tif a == 0 {
\t\treturn b
\t}
\tif b == 0 {
\t\treturn a
\t}
\treturn b
}
";

/// A type declaration only: no functions, no cohesion data.
const NO_FUNCTIONS_GO: &[u8] =
    b"package main\n\n// Config holds settings.\ntype Config struct {\n\tName string\n}\n";

/// The complexity crate's Python fixture: token-less operator nodes, so the
/// numbers depend on source recovery.
const PY_FIXTURE: &str = include_str!("../../../crates/cf-complexity/tests/fixtures/cc.py");

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

/// The quality report of `repo` as JSON.
fn quality_json(repo: &str) -> Value {
    let text = stdout_ok(&["-a", "history/quality", "--format", "json", repo]);
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("stdout is not JSON ({e}):\n{text}"))
}

/// A top-level `key: value` scalar of a YAML document, as `f64`.
fn yaml_scalar(yaml: &str, key: &str) -> f64 {
    let prefix = format!("{key}: ");
    yaml.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("no top-level {key:?} in:\n{yaml}"))
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("{key} is not a number ({e}) in:\n{yaml}"))
}

/// `aggregate.<key>` of the yaml report of static analyzer `id` over `path`.
fn static_aggregate(id: &str, path: &str, key: &str) -> f64 {
    let yaml = stdout_ok(&["-a", id, "--format", "yaml", path]);
    let aggregate = yaml
        .split_once("\naggregate:\n")
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| panic!("no aggregate in {id} yaml:\n{yaml}"));
    let unindented: String = aggregate
        .lines()
        .map(|l| l.trim_start())
        .collect::<Vec<_>>()
        .join("\n");
    yaml_scalar(&unindented, key)
}

fn path(test: &TestRepo) -> &str {
    test.path().to_str().expect("utf-8 path")
}

fn series(report: &Value) -> &Vec<Value> {
    report["time_series"]
        .as_array()
        .unwrap_or_else(|| panic!("no time_series in {report}"))
}

fn stat(entry: &Value, key: &str) -> f64 {
    entry["stats"][key]
        .as_f64()
        .unwrap_or_else(|| panic!("no numeric stats.{key} in {entry}"))
}

/// A Go source body with `n` one-line functions.
fn go_source(n: usize) -> Vec<u8> {
    let mut s = String::from("package main\n\n");
    for i in 0..n {
        s.push_str(&format!("func f{i}() int {{ return {i} }}\n"));
    }
    s.into_bytes()
}

/// Per-tick statistics follow the files changed in that tick: complexity
/// 2 → 4 over two ticks separated by an empty day; the gap day has no tick.
#[test]
fn per_tick_complexity_follows_the_changed_file() {
    let test = init_repo().expect("init");
    commit_files(&test, "cc2", T0, &[("a.go", ADD_CC2_GO)]).expect("cc2");
    commit_files(&test, "cc4", T0 + 2 * DAY, &[("a.go", ADD_CC4_GO)]).expect("cc4");
    let report = quality_json(path(&test));

    let ticks: Vec<i64> = series(&report)
        .iter()
        .map(|e| e["tick"].as_i64().unwrap())
        .collect();
    assert_eq!(ticks, vec![0, 2], "{report}");
    let (first, second) = (&series(&report)[0], &series(&report)[1]);
    assert_eq!(stat(first, "complexity_mean"), 2.0, "{first}");
    assert_eq!(stat(first, "files_analyzed"), 1.0);
    assert_eq!(stat(first, "total_functions"), 1.0);
    assert_eq!(stat(second, "complexity_mean"), 4.0, "{second}");
    assert_eq!(stat(second, "max_complexity"), 4.0);
    assert_eq!(first["start_time"], "2021-01-01T12:00:00Z");
    assert_eq!(second["end_time"], "2021-01-03T12:00:00Z");
    assert_eq!(report["aggregate"]["total_ticks"], 2);
    assert_eq!(report["aggregate"]["total_files_analyzed"], 2);
    assert_eq!(report["aggregate"]["complexity_median_mean"], 3.0);
}

/// A commit without analyzable files (a text file) and a commit whose only
/// Go file has no functions do not drag the aggregate means below the
/// aggregate minimums: `min <= mean` holds for cohesion and comment score,
/// and a tick without analyzed files is absent.
#[test]
fn aggregate_minimums_never_exceed_the_means() {
    let test = init_repo().expect("init");
    commit_files(&test, "code", T0, &[("a.go", ADD_CC2_GO)]).expect("code");
    commit_files(&test, "notes", T0 + DAY, &[("notes.txt", b"not source\n")]).expect("notes");
    commit_files(&test, "types", T0 + 2 * DAY, &[("c.go", NO_FUNCTIONS_GO)]).expect("types");
    let report = quality_json(path(&test));

    let ticks: Vec<i64> = series(&report)
        .iter()
        .map(|e| e["tick"].as_i64().unwrap())
        .collect();
    assert_eq!(
        ticks,
        vec![0, 2],
        "the text-only tick has no entry: {report}"
    );
    let agg = &report["aggregate"];
    let f = |k: &str| agg[k].as_f64().unwrap_or_else(|| panic!("no {k} in {agg}"));
    assert_eq!(agg["total_ticks"], 2, "{agg}");
    assert_eq!(agg["total_files_analyzed"], 2, "{agg}");
    assert!(f("min_cohesion") <= f("cohesion_mean_mean"), "{agg}");
    assert!(
        f("min_comment_score") <= f("comment_score_mean_mean"),
        "{agg}"
    );
    // The function-free file carries no cohesion data: both ticks' cohesion
    // aggregates come from the one function-bearing file.
    assert_eq!(f("min_cohesion"), 1.0, "{agg}");
    assert_eq!(f("cohesion_mean_mean"), 1.0, "{agg}");
    let types_tick = &series(&report)[1];
    assert_eq!(stat(types_tick, "files_analyzed"), 1.0, "{types_tick}");
    assert_eq!(stat(types_tick, "total_functions"), 0.0, "{types_tick}");
    assert_eq!(
        stat(types_tick, "cohesion_mean"),
        0.0,
        "no cohesion data: {types_tick}"
    );
}

/// A 40-file commit contributes every file.
#[test]
fn a_large_commit_is_fully_counted() {
    let test = init_repo().expect("init");
    let files: Vec<(String, Vec<u8>)> = (0..BIG_COMMIT_FILES)
        .map(|i| (format!("f{i}.go"), go_source(1)))
        .collect();
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    commit_files(&test, "big", T0, &refs).expect("big");
    let report = quality_json(path(&test));
    assert_eq!(
        report["aggregate"]["total_files_analyzed"], BIG_COMMIT_FILES as i64,
        "{report}"
    );
    assert_eq!(
        stat(&series(&report)[0], "files_analyzed"),
        BIG_COMMIT_FILES as f64
    );
    assert_eq!(
        stat(&series(&report)[0], "total_functions"),
        BIG_COMMIT_FILES as f64
    );
}

/// The per-file numbers of a one-commit, one-file repository are the static
/// analyzers' numbers for that file — for a Go file and for a Python file
/// whose Halstead counts depend on source recovery.
#[test]
fn quality_numbers_equal_the_static_analyzers() {
    for (name, body) in [("a.go", ADD_CC2_GO), ("cc.py", PY_FIXTURE.as_bytes())] {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(name), body).expect("write");
        let dir_path = dir.path().to_str().unwrap();

        let test = init_repo().expect("init");
        commit_files(&test, "one", T0, &[(name, body)]).expect("one");
        let report = quality_json(path(&test));
        let entry = &series(&report)[0];
        assert_eq!(stat(entry, "files_analyzed"), 1.0, "{name}: {entry}");

        let cx = |k: &str| static_aggregate("static/complexity", dir_path, k);
        assert_eq!(
            stat(entry, "complexity_mean"),
            cx("total_complexity"),
            "{name}"
        );
        assert_eq!(
            stat(entry, "max_complexity"),
            cx("max_complexity"),
            "{name}"
        );
        assert_eq!(
            stat(entry, "total_functions"),
            cx("total_functions"),
            "{name}"
        );

        let hal = |k: &str| static_aggregate("static/halstead", dir_path, k);
        assert!(
            (stat(entry, "halstead_vol_mean") - hal("volume")).abs() < EPS,
            "{name}: halstead volume {} vs static {}",
            stat(entry, "halstead_vol_mean"),
            hal("volume")
        );
        assert!(
            (stat(entry, "delivered_bugs_sum") - hal("delivered_bugs")).abs() < EPS,
            "{name}: delivered bugs"
        );

        let com = |k: &str| static_aggregate("static/comments", dir_path, k);
        assert_eq!(
            stat(entry, "comment_score_mean"),
            com("overall_score"),
            "{name}"
        );
        assert_eq!(
            stat(entry, "doc_coverage_mean"),
            com("documentation_coverage"),
            "{name}"
        );

        let coh = static_aggregate("static/cohesion", dir_path, "cohesion_score");
        assert_eq!(stat(entry, "cohesion_mean"), coh, "{name}");
    }
}

/// `json` and `yaml` carry the same numbers, and a run is deterministic.
#[test]
fn json_and_yaml_agree_and_runs_are_deterministic() {
    let test = init_repo().expect("init");
    commit_files(&test, "cc2", T0, &[("a.go", ADD_CC2_GO)]).expect("cc2");
    commit_files(&test, "cc4", T0 + DAY, &[("b.go", ADD_CC4_GO)]).expect("cc4");
    let p = path(&test);

    let first = stdout_ok(&["-a", "history/quality", "--format", "json", p]);
    let second = stdout_ok(&["-a", "history/quality", "--format", "json", p]);
    assert_eq!(first, second, "json output differs between runs");

    let json: Value = serde_json::from_str(&first).expect("json");
    let yaml = stdout_ok(&["-a", "history/quality", "--format", "yaml", p]);
    let agg = yaml
        .split_once("\naggregate:\n")
        .map(|(_, rest)| rest)
        .unwrap_or_else(|| panic!("no aggregate in yaml:\n{yaml}"));
    let unindented: String = agg
        .lines()
        .map(|l| l.trim_start())
        .collect::<Vec<_>>()
        .join("\n");
    for key in [
        "total_ticks",
        "total_files_analyzed",
        "complexity_median_mean",
        "min_cohesion",
        "cohesion_mean_mean",
    ] {
        let want = json["aggregate"][key].as_f64().expect("number");
        assert!(
            (yaml_scalar(&unindented, key) - want).abs() < EPS,
            "{key}: yaml {} vs json {want}",
            yaml_scalar(&unindented, key)
        );
    }
}
