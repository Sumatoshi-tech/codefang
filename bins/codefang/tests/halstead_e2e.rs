//! End-to-end coverage of `static/halstead`, driven through the **real
//! binary** over an on-disk fixture, asserting exact Halstead numbers computed
//! by hand from the standard definitions (see `specs/bugs/BUG-static-halstead-operators.md`).
//!
//! Fixture (two Go files):
//!
//! * `hal.go`   — `c := a + b; return c * 2`: operators `:=`, `+`, `*`,
//!   `return` (n1 = 4, N1 = 4); operands `a, b, c, c, 2` (n2 = 4, N2 = 5);
//!   V = 9·log2(8) = 27, D = (4/2)·(5/4) = 2.5, E = 67.5.
//! * `calls.go` — three `fmt.Println(...)` calls: operators `()`×3, `.`×3
//!   (n1 = 2, N1 = 6); operands `fmt`×3, `Println`×3, `"x"`×2, `"y"`×1
//!   (n2 = 4, N2 = 9); V = 15·log2(6), D = (2/2)·(9/4) = 2.25.
//!
//! Codebase aggregate = the UNION of both multisets: n1 = 6, N1 = 10,
//! n2 = 8, N2 = 14 → n = 14, N = 24, V = 24·log2(14), D = (6/2)·(14/8) = 5.25.

mod common;

use std::path::Path;

use serde_json::Value;

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");

const HAL_GO: &str =
    "package main\n\nfunc hal(a int, b int) int {\n\tc := a + b\n\treturn c * 2\n}\n";
const CALLS_GO: &str = "package main\n\nimport \"fmt\"\n\nfunc calls(x int) {\n\tfmt.Println(\"x\")\n\tfmt.Println(\"x\")\n\tfmt.Println(\"y\")\n}\n";

// Hand-computed expectations.
const HAL_VOLUME: f64 = 27.0;
const HAL_DIFFICULTY: f64 = 2.5;
const HAL_EFFORT: f64 = 67.5;
const CALLS_DIFFICULTY: f64 = 2.25;
const AGG_DISTINCT_OPERATORS: i64 = 6;
const AGG_DISTINCT_OPERANDS: i64 = 8;
const AGG_TOTAL_OPERATORS: i64 = 10;
const AGG_TOTAL_OPERANDS: i64 = 14;
const AGG_DIFFICULTY: f64 = 5.25;
const STATUS_LOW: &str = "Low Halstead complexity - well-structured code";
const STATUS_EMPTY: &str = "No functions found";
const FLOAT_TOLERANCE: f64 = 1e-9;

fn calls_volume() -> f64 {
    15.0 * 6f64.log2()
}

fn agg_volume() -> f64 {
    24.0 * 14f64.log2()
}

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

/// `codefang run -a static/halstead --format <format> [extra] <path>` →
/// (exit code, stdout, stderr).
fn halstead(path: &str, format: &str, extra: &[&str]) -> (i32, String, String) {
    let out = codefang(
        &[
            &["-a", "static/halstead", "--format", format],
            extra,
            &[path],
        ]
        .concat(),
    );
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("hal.go"), HAL_GO).unwrap();
    std::fs::write(dir.path().join("calls.go"), CALLS_GO).unwrap();
    dir
}

fn json_report(path: &str, extra: &[&str]) -> Value {
    let (rc, stdout, stderr) = halstead(path, "json", extra);
    assert_eq!(rc, 0, "json run failed: {stderr}");
    serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("invalid json ({e}):\n{stdout}"))
}

/// The one `HALSTEAD` section of a json report.
fn section(report: &Value) -> &Value {
    let sections = report["sections"].as_array().expect("sections");
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0]["title"], "HALSTEAD");
    &sections[0]
}

/// The `value` of the key metric labelled `label`.
fn metric<'a>(section: &'a Value, label: &str) -> &'a str {
    section["metrics"]
        .as_array()
        .expect("metrics")
        .iter()
        .find(|m| m["label"] == label)
        .unwrap_or_else(|| panic!("metric {label} missing in {section}"))["value"]
        .as_str()
        .expect("string value")
}

/// One decimal, the same formatting the report uses.
fn one_decimal(v: f64) -> String {
    format!("{v:.1}")
}

/// The yaml scalar following `key:` on the first line that starts with it
/// (after indentation), parsed as f64.
fn yaml_number(yaml: &str, key: &str) -> f64 {
    let prefix = format!("{key}: ");
    yaml.lines()
        .map(str::trim_start)
        .find_map(|l| l.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("{key} missing in:\n{yaml}"))
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("{key} is not a number ({e}) in:\n{yaml}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < FLOAT_TOLERANCE
}

/// `--format plot --output <dir>` writes `report.json`, whose `halstead`
/// entry carries every function with its operator / operand maps.
fn plot_report(path: &str) -> Value {
    let out_dir = tempfile::tempdir().expect("plot output dir");
    let out = codefang(&[
        "-a",
        "static/halstead",
        "--format",
        "plot",
        "--output",
        out_dir.path().to_str().unwrap(),
        path,
    ]);
    assert!(
        out.status.success(),
        "plot run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let raw = std::fs::read_to_string(out_dir.path().join("report.json")).expect("report.json");
    let report: Value = serde_json::from_str(&raw).expect("report.json is json");
    report["halstead"].clone()
}

fn function<'a>(raw: &'a Value, name: &str) -> &'a Value {
    raw["functions"]
        .as_array()
        .expect("functions")
        .iter()
        .find(|f| f["name"] == name)
        .unwrap_or_else(|| panic!("function {name} missing in {raw}"))
}

fn counts(v: &Value) -> Vec<(String, i64)> {
    let mut out: Vec<(String, i64)> = v
        .as_object()
        .expect("map")
        .iter()
        .map(|(k, c)| (k.clone(), c.as_i64().expect("count")))
        .collect();
    out.sort();
    out
}

fn pairs(items: &[(&str, i64)]) -> Vec<(String, i64)> {
    let mut out: Vec<(String, i64)> = items.iter().map(|(k, c)| ((*k).to_string(), *c)).collect();
    out.sort();
    out
}

/// The per-function operator / operand multisets are exactly the standard
/// Halstead tokens: no node type names, no statement text, no missing
/// literals, call parentheses and member access counted as operators.
#[test]
fn per_function_operators_and_operands_are_exact() {
    let dir = fixture();
    let raw = plot_report(dir.path().to_str().unwrap());

    let hal = function(&raw, "hal");
    assert_eq!(
        counts(&hal["operators"]),
        pairs(&[(":=", 1), ("+", 1), ("*", 1), ("return", 1)])
    );
    assert_eq!(
        counts(&hal["operands"]),
        pairs(&[("a", 1), ("b", 1), ("c", 2), ("2", 1)])
    );
    assert_eq!(hal["distinct_operators"], 4);
    assert_eq!(hal["distinct_operands"], 4);
    assert_eq!(hal["total_operators"], 4);
    assert_eq!(hal["total_operands"], 5);
    assert_eq!(hal["volume"], HAL_VOLUME);
    assert_eq!(hal["difficulty"], HAL_DIFFICULTY);
    assert_eq!(hal["effort"], HAL_EFFORT);
    assert_eq!(hal["_source_file"], "hal.go");
    assert_eq!(hal["_language"], "go");

    let calls = function(&raw, "calls");
    assert_eq!(counts(&calls["operators"]), pairs(&[("()", 3), (".", 3)]));
    assert_eq!(
        counts(&calls["operands"]),
        pairs(&[("fmt", 3), ("Println", 3), ("\"x\"", 2), ("\"y\"", 1)])
    );
    assert!(close(calls["volume"].as_f64().unwrap(), calls_volume()));
    assert_eq!(calls["difficulty"], CALLS_DIFFICULTY);

    // The raw aggregate is the union, with the per-function statistics.
    assert_eq!(raw["total_functions"], 2);
    assert_eq!(raw["distinct_operators"], AGG_DISTINCT_OPERATORS);
    assert_eq!(raw["total_operands"], AGG_TOTAL_OPERANDS);
    assert_eq!(raw["difficulty"], AGG_DIFFICULTY);
    assert_eq!(raw["max_effort"], calls["effort"]);
    assert_eq!(raw["high_effort_functions"], 0);
    assert_eq!(raw["message"], STATUS_LOW);
}

/// The json aggregate reports the UNION counts (never per-file averages),
/// and `--per-file` entries carry each file's own counts whose totals add up
/// to the aggregate totals.
#[test]
fn json_aggregate_is_the_union_and_per_file_entries_add_up() {
    let dir = fixture();
    let report = json_report(dir.path().to_str().unwrap(), &["--per-file"]);
    let sec = section(&report);

    assert_eq!(sec["status"], STATUS_LOW);
    assert_eq!(metric(sec, "Total Functions"), "2");
    assert_eq!(
        metric(sec, "Distinct Operators (n1)"),
        AGG_DISTINCT_OPERATORS.to_string()
    );
    assert_eq!(
        metric(sec, "Distinct Operands (n2)"),
        AGG_DISTINCT_OPERANDS.to_string()
    );
    assert_eq!(
        metric(sec, "Total Operators (N1)"),
        AGG_TOTAL_OPERATORS.to_string()
    );
    assert_eq!(
        metric(sec, "Total Operands (N2)"),
        AGG_TOTAL_OPERANDS.to_string()
    );
    assert_eq!(metric(sec, "Vocabulary"), "14");
    assert_eq!(metric(sec, "Length"), "24");
    assert_eq!(metric(sec, "Volume"), one_decimal(agg_volume()));
    assert_eq!(metric(sec, "Difficulty"), one_decimal(AGG_DIFFICULTY));
    assert_eq!(
        metric(sec, "Effort"),
        one_decimal(agg_volume() * AGG_DIFFICULTY)
    );
    assert_eq!(
        metric(sec, "Max Effort"),
        one_decimal(calls_volume() * CALLS_DIFFICULTY)
    );
    assert_eq!(metric(sec, "High-Effort Functions"), "0");
    assert_eq!(report["overall_score_label"], "10/10");
    assert_eq!(report["overall_score"], 1.0);

    // Distribution: both functions are Low (< 100).
    let dist = sec["distribution"].as_array().expect("distribution");
    assert_eq!(dist[0]["label"], "Low (<100)");
    assert_eq!(dist[0]["count"], 2);
    assert_eq!(dist[0]["percent"], 1.0);
    assert_eq!(dist[3]["label"], "Very High (>=5000)");
    assert_eq!(dist[3]["count"], 0);

    // Issues: effort descending — calls (87.2) before hal (67.5).
    let issues = sec["issues"].as_array().expect("issues");
    assert_eq!(issues[0]["name"], "calls");
    assert_eq!(issues[1]["name"], "hal");
    assert_eq!(issues[1]["value"], "effort=67.5 | vol=27.0 | bugs=0.0");
    assert_eq!(issues[1]["severity"], "good");

    // --per-file: one entry per file, each with its own union counts.
    let files = sec["files"].as_array().expect("files");
    assert_eq!(files.len(), 2);
    let by_path = |p: &str| {
        files
            .iter()
            .find(|f| f["file_path"] == p)
            .unwrap_or_else(|| panic!("{p} entry missing"))
    };
    let hal = by_path("hal.go");
    assert_eq!(metric(hal, "Distinct Operators (n1)"), "4");
    assert_eq!(metric(hal, "Total Operators (N1)"), "4");
    assert_eq!(metric(hal, "Total Operands (N2)"), "5");
    assert_eq!(metric(hal, "Volume"), one_decimal(HAL_VOLUME));
    let calls = by_path("calls.go");
    assert_eq!(metric(calls, "Distinct Operators (n1)"), "2");
    assert_eq!(metric(calls, "Total Operators (N1)"), "6");
    assert_eq!(metric(calls, "Total Operands (N2)"), "9");
    let sum = |label: &str| -> i64 {
        files
            .iter()
            .map(|f| metric(f, label).parse::<i64>().unwrap())
            .sum()
    };
    assert_eq!(sum("Total Operators (N1)"), AGG_TOTAL_OPERATORS);
    assert_eq!(sum("Total Operands (N2)"), AGG_TOTAL_OPERANDS);
    assert_eq!(sum("Total Functions"), 2);
}

/// json, yaml and text agree on every number, and yaml's aggregate carries
/// the real integer counts.
#[test]
fn json_yaml_and_text_agree() {
    let dir = fixture();
    let path = dir.path().to_str().unwrap();
    let report = json_report(path, &[]);
    let sec = section(&report);

    let (rc, yaml, stderr) = halstead(path, "yaml", &[]);
    assert_eq!(rc, 0, "yaml run failed: {stderr}");
    let agg = yaml.split("aggregate:").nth(1).expect("aggregate block");
    assert!(close(yaml_number(agg, "volume"), agg_volume()), "{yaml}");
    assert_eq!(yaml_number(agg, "difficulty"), AGG_DIFFICULTY);
    assert_eq!(
        yaml_number(agg, "distinct_operators") as i64,
        AGG_DISTINCT_OPERATORS
    );
    assert_eq!(
        yaml_number(agg, "distinct_operands") as i64,
        AGG_DISTINCT_OPERANDS
    );
    assert_eq!(
        yaml_number(agg, "total_operators") as i64,
        AGG_TOTAL_OPERATORS
    );
    assert_eq!(
        yaml_number(agg, "total_operands") as i64,
        AGG_TOTAL_OPERANDS
    );
    assert_eq!(yaml_number(agg, "vocabulary") as i64, 14);
    assert_eq!(yaml_number(agg, "length") as i64, 24);
    assert_eq!(yaml_number(agg, "health_score"), 100.0);
    assert!(agg.contains(&format!("message: {STATUS_LOW}")), "{yaml}");
    assert_eq!(
        one_decimal(yaml_number(agg, "volume")),
        metric(sec, "Volume"),
        "yaml volume vs json Volume"
    );
    assert_eq!(
        one_decimal(yaml_number(agg, "effort")),
        metric(sec, "Effort")
    );
    // Same distribution buckets as json.
    let dist = yaml
        .split("distribution:")
        .nth(1)
        .expect("distribution block");
    assert_eq!(yaml_number(dist, "low") as i64, 2);
    assert_eq!(yaml_number(dist, "very_high") as i64, 0);
    // function_halstead: effort descending, calls first, with its bucket.
    let funcs = yaml.split("function_halstead:").nth(1).unwrap();
    assert!(funcs.find("name: calls").unwrap() < funcs.find("name: hal").unwrap());
    assert!(funcs.contains("complexity_level: Low"));
    assert!(yaml.contains("high_effort_functions: []"), "{yaml}");

    let (rc, text, stderr) = halstead(path, "text", &[]);
    assert_eq!(rc, 0, "text run failed: {stderr}");
    assert!(text.contains(STATUS_LOW), "{text}");
    for (label, value) in [
        ("Total Functions", "2".to_string()),
        ("Total Operators (N1)", AGG_TOTAL_OPERATORS.to_string()),
        ("Total Operands (N2)", AGG_TOTAL_OPERANDS.to_string()),
        ("Volume", one_decimal(agg_volume())),
        ("Effort", metric(sec, "Effort").to_string()),
    ] {
        let after = &text[text
            .find(label)
            .unwrap_or_else(|| panic!("{label} in:\n{text}"))
            + label.len()..];
        let got: String = after
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        assert_eq!(got, value, "text {label} vs json");
    }
}

/// An empty folder, or one holding only unsupported files, produces the
/// empty report (zeros, `No functions found`) and exits 0 in every format.
#[test]
fn empty_folder_exits_zero_with_the_empty_report() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("notes.txt"), "this is not source\n").unwrap();
    let path = dir.path().to_str().unwrap();

    let report = json_report(path, &["--per-file"]);
    let sec = section(&report);
    assert_eq!(sec["status"], STATUS_EMPTY);
    assert_eq!(metric(sec, "Total Functions"), "0");
    assert_eq!(metric(sec, "Volume"), "0.0");
    assert!(
        sec.get("distribution").is_none(),
        "no buckets without functions"
    );
    assert_eq!(sec["issues"].as_array().unwrap().len(), 0);
    assert_eq!(sec["files"].as_array().unwrap().len(), 0);

    let (rc, yaml, stderr) = halstead(path, "yaml", &[]);
    assert_eq!(rc, 0, "yaml: {stderr}");
    assert!(yaml.contains("total_functions: 0"), "{yaml}");
    assert!(yaml.contains(&format!("message: {STATUS_EMPTY}")), "{yaml}");

    for format in ["text", "compact", "bin"] {
        let (rc, _, stderr) = halstead(path, format, &[]);
        assert_eq!(rc, 0, "{format}: {stderr}");
    }

    let empty = tempfile::tempdir().expect("empty");
    let (rc, stdout, stderr) = halstead(empty.path().to_str().unwrap(), "json", &[]);
    assert_eq!(rc, 0, "empty dir: {stderr}");
    assert!(stdout.contains(STATUS_EMPTY), "{stdout}");
}

/// Two runs over the same fixture produce byte-identical reports.
#[test]
fn reports_are_deterministic() {
    let dir = fixture();
    let path = dir.path().to_str().unwrap();
    for format in ["json", "yaml", "text"] {
        let (rc1, a, _) = halstead(path, format, &["--per-file"]);
        let (rc2, b, _) = halstead(path, format, &["--per-file"]);
        assert_eq!((rc1, rc2), (0, 0));
        assert_eq!(a, b, "{format} output differs between runs");
    }
}

/// Sanity: the fixture directory is what the tests assume.
#[test]
fn fixture_files_exist() {
    let dir = fixture();
    assert!(Path::new(&dir.path().join("hal.go")).is_file());
    assert!(Path::new(&dir.path().join("calls.go")).is_file());
}
