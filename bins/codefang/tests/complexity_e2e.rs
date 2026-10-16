//! End-to-end coverage of `static/complexity`, driven through the **real
//! binary** over hermetic source trees written into a tempdir.
//!
//! The fixture sources are the ones the crate-level tests measure
//! (`crates/cf-complexity/tests/fixtures/`), so every number asserted here
//! was hand-computed against the documented counting rules
//! (`site/explanation/complexity.md`) and is checked twice: from the crate
//! API and from the CLI's machine formats.

mod common;

use std::fs;
use std::path::Path;

use serde_json::Value;

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");

const GO_FIXTURE: &str = include_str!("../../../crates/cf-complexity/tests/fixtures/cc.go");
const PY_FIXTURE: &str = include_str!("../../../crates/cf-complexity/tests/fixtures/cc.py");
const JS_FIXTURE: &str = include_str!("../../../crates/cf-complexity/tests/fixtures/cc.js");
const RS_FIXTURE: &str = include_str!("../../../crates/cf-complexity/tests/fixtures/cc.rs");
const JAVA_FIXTURE: &str = include_str!("../../../crates/cf-complexity/tests/fixtures/Foo.java");
const C_FIXTURE: &str = include_str!("../../../crates/cf-complexity/tests/fixtures/cc.c");
const GEN_FIXTURE: &str = include_str!("../../../crates/cf-complexity/tests/fixtures/gen.go");

/// A Go file without functions (a type declaration only).
const NOFUNC_GO: &str = "package main\n\ntype Config struct {\n\tName string\n}\n";
/// A one-function file, cyclomatic 1.
const SAME_A_GO: &str = "package a\n\nfunc same() int { return 1 }\n";
/// A one-function file with the same name, cyclomatic 2 (one `if`).
const SAME_B_GO: &str = "package b\n\nfunc same() int { if true { return 1 }; return 0 }\n";

/// Nesting depth of the generated deep fixture.
const DEEP_DEPTH: usize = 20_000;

/// Magic + LE u32 payload length of the `--format bin` envelope.
const BIN_MAGIC: &[u8; 4] = b"CFB1";
const BIN_HEADER_SIZE: usize = 8;

/// The Go fixture's functions in report order, as the `issues` rows render
/// them: `(name, cyclomatic, cognitive, nesting, severity)`.
const GO_EXPECTED: &[(&str, i64, i64, i64, &str)] = &[
    ("decisions", 8, 7, 1, "fair"),
    ("manyCases", 5, 1, 1, "good"),
    ("nested", 4, 6, 3, "good"),
    ("loopCond", 4, 3, 1, "good"),
    ("boolSeq", 4, 2, 0, "good"),
    ("elseNest", 3, 4, 2, "good"),
    ("withClosure", 3, 3, 1, "good"),
    ("fact", 2, 2, 1, "good"),
    ("Method", 2, 1, 1, "good"),
    ("simple", 1, 0, 0, "good"),
];

/// Runs the binary with a deterministic environment; state is disabled so
/// the run is self-contained.
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

/// `(exit code, stdout, stderr)` of `static/complexity --format <format>`.
fn complexity(path: &Path, format: &str, extra: &[&str]) -> (i32, Vec<u8>, String) {
    let p = path.to_str().expect("utf-8 path");
    let out = codefang(
        &[
            &["-a", "static/complexity", "--format", format],
            extra,
            &[p],
        ]
        .concat(),
    );
    (
        out.status.code().unwrap_or(-1),
        out.stdout,
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn write(dir: &Path, rel: &str, content: &str) {
    let path = dir.join(rel);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, content).expect("write fixture");
}

/// The Go fixture plus a function-free file and an empty file.
fn mixed_tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "cc.go", GO_FIXTURE);
    write(dir.path(), "nofunc.go", NOFUNC_GO);
    write(dir.path(), "empty.go", "");
    write(dir.path(), "notes.txt", "this is not source\n");
    dir
}

/// Two identically named functions in two directories.
fn dup_tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "a/x.go", SAME_A_GO);
    write(dir.path(), "b/x.go", SAME_B_GO);
    dir
}

/// Every language fixture in one tree.
fn polyglot_tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "cc.go", GO_FIXTURE);
    write(dir.path(), "cc.py", PY_FIXTURE);
    write(dir.path(), "cc.js", JS_FIXTURE);
    write(dir.path(), "cc.rs", RS_FIXTURE);
    write(dir.path(), "Foo.java", JAVA_FIXTURE);
    write(dir.path(), "cc.c", C_FIXTURE);
    write(dir.path(), "gen.go", GEN_FIXTURE);
    dir
}

fn deep_tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut src = String::from("package main\n\nfunc deep(x int) {\n");
    for _ in 0..DEEP_DEPTH {
        src.push_str("if true {\n");
    }
    for _ in 0..DEEP_DEPTH {
        src.push_str("}\n");
    }
    src.push_str("}\n");
    write(dir.path(), "deep.go", &src);
    dir
}

fn json_section(stdout: &[u8]) -> Value {
    let report: Value = serde_json::from_slice(stdout).expect("json report parses");
    report["sections"][0].clone()
}

/// `label -> value` of a section's (or file entry's) key metrics.
fn metrics(section: &Value) -> Vec<(String, String)> {
    section["metrics"]
        .as_array()
        .expect("metrics array")
        .iter()
        .map(|m| {
            (
                m["label"].as_str().expect("label").to_string(),
                m["value"].as_str().expect("value").to_string(),
            )
        })
        .collect()
}

fn metric(section: &Value, label: &str) -> String {
    metrics(section)
        .into_iter()
        .find(|(l, _)| l == label)
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("metric {label} missing in {section}"))
}

fn metric_i64(section: &Value, label: &str) -> i64 {
    metric(section, label).parse().expect("integer metric")
}

/// `(name, location, cyclomatic, cognitive, nesting, severity)` of each
/// issue row, parsing the `CC=.. | Cog=.. | Nest=..` value.
fn issue_rows(section: &Value) -> Vec<(String, String, i64, i64, i64, String)> {
    section["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .map(|row| {
            let value = row["value"].as_str().expect("value");
            let nums: Vec<i64> = value
                .split('|')
                .map(|part| {
                    part.trim()
                        .split('=')
                        .nth(1)
                        .expect("metric=value")
                        .parse()
                        .expect("integer")
                })
                .collect();
            (
                row["name"].as_str().expect("name").to_string(),
                row["location"].as_str().expect("location").to_string(),
                nums[0],
                nums[1],
                nums[2],
                row["severity"].as_str().expect("severity").to_string(),
            )
        })
        .collect()
}

/// Decodes the CFB1 envelope and parses its JSON payload.
fn bin_payload(stdout: &[u8]) -> Value {
    assert!(stdout.len() >= BIN_HEADER_SIZE, "envelope too short");
    assert_eq!(&stdout[..4], BIN_MAGIC, "envelope magic");
    let len = u32::from_le_bytes(stdout[4..8].try_into().expect("4 bytes")) as usize;
    assert_eq!(stdout.len(), BIN_HEADER_SIZE + len, "envelope length");
    serde_json::from_slice(&stdout[BIN_HEADER_SIZE..]).expect("bin payload parses as json")
}

/// The scalar after `<key>:` in the yaml `aggregate:` block.
fn yaml_aggregate(yaml: &str, key: &str) -> String {
    let agg = yaml
        .split("\naggregate:\n")
        .nth(1)
        .expect("aggregate block");
    agg.lines()
        .find_map(|line| line.trim().strip_prefix(&format!("{key}: ")))
        .unwrap_or_else(|| panic!("aggregate.{key} missing in:\n{yaml}"))
        .to_string()
}

/// The integer printed after a `Key Metrics` label in the text rendering.
fn text_metric(text: &str, label: &str) -> i64 {
    let at = text
        .find(label)
        .unwrap_or_else(|| panic!("{label} missing in:\n{text}"));
    let rest = &text[at + label.len()..];
    rest.trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .expect("digits after label")
}

#[test]
fn json_reports_exact_per_function_metrics_distribution_and_totals() {
    let dir = mixed_tree();
    let (rc, stdout, stderr) = complexity(dir.path(), "json", &[]);
    assert_eq!(rc, 0, "run failed: {stderr}");
    let section = json_section(&stdout);

    assert_eq!(section["title"], "COMPLEXITY");
    assert_eq!(
        metrics(&section),
        vec![
            ("Total Functions".to_string(), "10".to_string()),
            ("Avg Complexity".to_string(), "3.6".to_string()),
            ("Max Complexity".to_string(), "8".to_string()),
            ("Total Complexity".to_string(), "36".to_string()),
            ("Cognitive Total".to_string(), "29".to_string()),
            ("Max Nesting".to_string(), "3".to_string()),
            ("Decision Points".to_string(), "26".to_string()),
        ]
    );
    assert_eq!(
        section["status"],
        "Fair complexity - some functions could be simplified"
    );
    assert_eq!(section["score_label"], "6/10");

    let dist: Vec<(String, f64, i64)> = section["distribution"]
        .as_array()
        .expect("distribution")
        .iter()
        .map(|d| {
            (
                d["label"].as_str().expect("label").to_string(),
                d["percent"].as_f64().expect("percent"),
                d["count"].as_i64().expect("count"),
            )
        })
        .collect();
    assert_eq!(
        dist,
        vec![
            ("Simple (1-5)".to_string(), 0.9, 9),
            ("Moderate (6-10)".to_string(), 0.1, 1),
            ("Complex (11-20)".to_string(), 0.0, 0),
            ("Very Complex (>20)".to_string(), 0.0, 0),
        ]
    );

    let rows = issue_rows(&section);
    let want: Vec<(String, String, i64, i64, i64, String)> = GO_EXPECTED
        .iter()
        .map(|(n, cc, cog, nest, sev)| {
            (
                (*n).to_string(),
                "cc.go".to_string(),
                *cc,
                *cog,
                *nest,
                (*sev).to_string(),
            )
        })
        .collect();
    assert_eq!(
        rows, want,
        "issue rows (name, file, cc, cognitive, nesting, severity)"
    );
    assert!(
        section.get("files").is_none(),
        "no --per-file: no files array"
    );
}

#[test]
fn bin_envelope_carries_lines_of_code_density_and_risk() {
    let dir = mixed_tree();
    let (rc, stdout, stderr) = complexity(dir.path(), "bin", &[]);
    assert_eq!(rc, 0, "run failed: {stderr}");
    let payload = bin_payload(&stdout);

    let fc = payload["function_complexity"].as_array().expect("array");
    assert_eq!(fc.len(), 10);
    let top = &fc[0];
    assert_eq!(top["name"], "decisions");
    assert_eq!(top["source_file"], "cc.go");
    assert_eq!(top["language"], "go");
    assert_eq!(top["directory"], ".");
    assert_eq!(top["cyclomatic_complexity"], 8);
    assert_eq!(top["cognitive_complexity"], 7);
    assert_eq!(top["nesting_depth"], 1);
    assert_eq!(top["lines_of_code"], 22);
    assert!((top["complexity_density"].as_f64().expect("density") - 8.0 / 22.0).abs() < 1e-12);
    assert_eq!(top["risk_level"], "MEDIUM");
    let last = &fc[9];
    assert_eq!(last["name"], "simple");
    assert_eq!(last["lines_of_code"], 3);
    assert_eq!(last["risk_level"], "LOW");

    assert_eq!(payload["distribution"]["simple"], 9);
    assert_eq!(payload["distribution"]["moderate"], 1);
    assert!(payload["distribution"].get("complex").is_none());
    assert_eq!(
        payload["high_risk_functions"]
            .as_array()
            .expect("array")
            .len(),
        0
    );

    let agg = &payload["aggregate"];
    assert_eq!(agg["total_functions"], 10);
    assert_eq!(agg["total_complexity"], 36);
    assert_eq!(agg["cognitive_complexity"], 29);
    assert_eq!(agg["nesting_depth"], 3);
    assert_eq!(agg["max_complexity"], 8);
    assert_eq!(agg["decision_points"], 26);
    assert!((agg["average_complexity"].as_f64().expect("avg") - 3.6).abs() < 1e-12);
    assert_eq!(
        agg["message"],
        "Fair complexity - some functions could be simplified"
    );
}

#[test]
fn yaml_aggregate_carries_real_sums_and_max_nesting() {
    let dir = mixed_tree();
    let (rc, stdout, stderr) = complexity(dir.path(), "yaml", &[]);
    assert_eq!(rc, 0, "run failed: {stderr}");
    let yaml = String::from_utf8(stdout).expect("utf-8 yaml");
    assert_eq!(yaml_aggregate(&yaml, "total_functions"), "10");
    assert_eq!(yaml_aggregate(&yaml, "total_complexity"), "36");
    assert_eq!(yaml_aggregate(&yaml, "cognitive_complexity"), "29");
    assert_eq!(yaml_aggregate(&yaml, "nesting_depth"), "3");
    assert_eq!(yaml_aggregate(&yaml, "max_complexity"), "8");
    assert_eq!(yaml_aggregate(&yaml, "decision_points"), "26");
    assert_eq!(yaml_aggregate(&yaml, "average_complexity"), "3.6");
    assert!(yaml.starts_with("function_complexity:\n    - name: decisions\n"));
}

#[test]
fn per_file_entries_sum_to_the_aggregate() {
    let dir = mixed_tree();
    write(dir.path(), "sub/x.go", SAME_B_GO);
    let (rc, stdout, stderr) = complexity(dir.path(), "json", &["--per-file"]);
    assert_eq!(rc, 0, "run failed: {stderr}");
    let section = json_section(&stdout);
    let files = section["files"].as_array().expect("files array");
    let paths: Vec<&str> = files
        .iter()
        .map(|f| f["file_path"].as_str().expect("file_path"))
        .collect();
    assert_eq!(
        paths,
        vec!["cc.go", "empty.go", "nofunc.go", "sub/x.go"],
        "every parsed file in walk order, function-free files included"
    );

    for label in [
        "Total Functions",
        "Total Complexity",
        "Cognitive Total",
        "Decision Points",
    ] {
        let sum: i64 = files.iter().map(|f| metric_i64(f, label)).sum();
        assert_eq!(sum, metric_i64(&section, label), "{label}: per-file sum");
    }
    for label in ["Max Complexity", "Max Nesting"] {
        let max = files
            .iter()
            .map(|f| metric_i64(f, label))
            .max()
            .expect("max");
        assert_eq!(max, metric_i64(&section, label), "{label}: per-file max");
    }

    let nofunc = &files[2];
    assert_eq!(nofunc["status"], "No functions found");
    assert!(nofunc.get("distribution").is_none());
    assert!(nofunc["issues"].as_array().expect("issues").is_empty());
    let sub = &files[3];
    assert_eq!(issue_rows(sub)[0].0, "same");
    assert_eq!(issue_rows(sub)[0].1, "sub/x.go");
}

#[test]
fn json_yaml_bin_and_text_agree_on_the_totals() {
    let dir = mixed_tree();
    let (rc, json, _) = complexity(dir.path(), "json", &[]);
    assert_eq!(rc, 0);
    let (rc, yaml, _) = complexity(dir.path(), "yaml", &[]);
    assert_eq!(rc, 0);
    let (rc, bin, _) = complexity(dir.path(), "bin", &[]);
    assert_eq!(rc, 0);
    let (rc, text, _) = complexity(dir.path(), "text", &[]);
    assert_eq!(rc, 0);

    let section = json_section(&json);
    let yaml = String::from_utf8(yaml).expect("utf-8");
    let agg = bin_payload(&bin)["aggregate"].clone();
    let text = String::from_utf8(text).expect("utf-8");

    for (label, key) in [
        ("Total Functions", "total_functions"),
        ("Total Complexity", "total_complexity"),
        ("Cognitive Total", "cognitive_complexity"),
        ("Max Nesting", "nesting_depth"),
        ("Decision Points", "decision_points"),
        ("Max Complexity", "max_complexity"),
    ] {
        let from_json = metric_i64(&section, label);
        assert_eq!(
            yaml_aggregate(&yaml, key),
            from_json.to_string(),
            "yaml {key}"
        );
        assert_eq!(agg[key].as_i64().expect("int"), from_json, "bin {key}");
        assert_eq!(text_metric(&text, label), from_json, "text {label}");
    }
    assert!(
        text.contains("Top Issues"),
        "text prints the ranked function rows:\n{text}"
    );
    assert!(text.contains("decisions") && text.contains("CC=8 | Cog=7 | Nest=1"));
}

#[test]
fn deep_nesting_exits_zero_in_every_format() {
    let dir = deep_tree();
    let depth = i64::try_from(DEEP_DEPTH).expect("fits");
    let cognitive = depth * (depth + 1) / 2;
    for format in ["json", "yaml", "bin", "text", "compact"] {
        let (rc, stdout, stderr) = complexity(dir.path(), format, &[]);
        assert_eq!(rc, 0, "--format {format} failed: {stderr}");
        assert!(!stdout.is_empty(), "--format {format}: empty output");
    }
    let (_, json, _) = complexity(dir.path(), "json", &[]);
    let rows = issue_rows(&json_section(&json));
    assert_eq!(
        rows,
        vec![(
            "deep".to_string(),
            "deep.go".to_string(),
            depth + 1,
            cognitive,
            depth,
            "poor".to_string()
        )]
    );
    let (_, yaml, _) = complexity(dir.path(), "yaml", &[]);
    let yaml = String::from_utf8(yaml).expect("utf-8");
    assert_eq!(yaml_aggregate(&yaml, "nesting_depth"), depth.to_string());
}

#[test]
fn empty_directory_exits_zero_with_an_empty_report() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (rc, json, stderr) = complexity(dir.path(), "json", &[]);
    assert_eq!(rc, 0, "json: {stderr}");
    let section = json_section(&json);
    assert_eq!(metric(&section, "Total Functions"), "0");
    assert_eq!(section["status"], "No complexity data available");
    assert!(section.get("distribution").is_none());
    assert!(section["issues"].as_array().expect("issues").is_empty());

    let (rc, yaml, stderr) = complexity(dir.path(), "yaml", &[]);
    assert_eq!(rc, 0, "yaml: {stderr}");
    let yaml = String::from_utf8(yaml).expect("utf-8");
    assert!(yaml.starts_with("function_complexity: []\n"));
    assert_eq!(yaml_aggregate(&yaml, "total_functions"), "0");

    for format in ["bin", "text", "compact"] {
        let (rc, _, stderr) = complexity(dir.path(), format, &[]);
        assert_eq!(rc, 0, "--format {format}: {stderr}");
    }
}

#[test]
fn identically_named_functions_order_by_metrics_then_file() {
    let dir = dup_tree();
    let (rc, json, _) = complexity(dir.path(), "json", &[]);
    assert_eq!(rc, 0);
    let rows = issue_rows(&json_section(&json));
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (rows[0].0.as_str(), rows[0].1.as_str(), rows[0].2),
        ("same", "b/x.go", 2)
    );
    assert_eq!(
        (rows[1].0.as_str(), rows[1].1.as_str(), rows[1].2),
        ("same", "a/x.go", 1)
    );

    let (_, yaml, _) = complexity(dir.path(), "yaml", &[]);
    let yaml = String::from_utf8(yaml).expect("utf-8");
    assert_eq!(yaml_aggregate(&yaml, "cognitive_complexity"), "1");
    assert_eq!(yaml_aggregate(&yaml, "nesting_depth"), "1");
}

#[test]
fn every_language_reports_its_real_functions() {
    let dir = polyglot_tree();
    let (rc, json, stderr) = complexity(dir.path(), "json", &[]);
    assert_eq!(rc, 0, "run failed: {stderr}");
    let section = json_section(&json);
    // go 10 + py 3 + js 3 + rs 3 + java 2 + c 2 + gen.go 3
    assert_eq!(metric(&section, "Total Functions"), "26");
    let rows = issue_rows(&section);
    let find = |name: &str, file: &str| {
        rows.iter()
            .find(|r| r.0 == name && r.1 == file)
            .unwrap_or_else(|| panic!("{name} in {file} missing from {rows:?}"))
    };
    let c = find("c_decisions", "cc.c");
    assert_eq!((c.2, c.3, c.4), (10, 10, 1));
    assert_eq!(
        (
            find("jsDecisions", "cc.js").2,
            find("jsDecisions", "cc.js").3
        ),
        (11, 10)
    );
    assert_eq!(
        (
            find("py_decisions", "cc.py").2,
            find("py_decisions", "cc.py").3
        ),
        (8, 8)
    );
    assert_eq!(
        (
            find("rs_decisions", "cc.rs").2,
            find("rs_decisions", "cc.rs").3
        ),
        (9, 8)
    );
    assert_eq!(
        (find("nested", "Foo.java").2, find("nested", "Foo.java").3),
        (4, 6)
    );
    assert_eq!(find("outer", "gen.go").2, 3);
    for row in &rows {
        assert!(
            !row.0.contains('\n') && !row.0.contains('(') && row.0.len() < 40,
            "function name is an identifier, not source text: {:?}",
            row.0
        );
    }
}

#[test]
fn machine_formats_are_deterministic_across_runs() {
    let dir = polyglot_tree();
    for format in ["json", "yaml", "bin"] {
        let (rc, first, _) = complexity(dir.path(), format, &[]);
        assert_eq!(rc, 0);
        let (rc, second, _) = complexity(dir.path(), format, &[]);
        assert_eq!(rc, 0);
        assert_eq!(first, second, "--format {format} differs between runs");
    }
}
