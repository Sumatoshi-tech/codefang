//! End-to-end coverage of `static/comments`, driven through the **real
//! binary** over on-disk fixtures.
//!
//! The contract (see `site/analyzers/comments.md`): documentation coverage is
//! Σdocumented / Σfunctions over every analyzed file, the good-comment ratio
//! is Σgood / Σcomments, a function is documented iff a comment ends on the
//! line directly above it, and every output format reports the same counts.

mod common;

use std::path::Path;
use std::process::Output;

use serde_json::Value;

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");

/// Fixture file with one documented and one undocumented function; the
/// package comment is not directly above any declaration and the body
/// comment is inline.
const A_GO: &str = "// Package fixture is the comments e2e fixture.
package fixture

// Add adds.
func Add(a, b int) int {
	// inline comment
	return a + b
}

func Sub(a, b int) int {
	return a - b
}
";

/// Fixture file where a blank line separates `Mul` from its comment (not
/// documented), a block comment documents `Div`, and `Mod` has nothing.
const B_GO: &str = "package fixture

// Mul multiplies.

func Mul(a, b int) int {
	return a * b
}

/* Div divides. */
func Div(a, b int) int {
	return a / b
}

func Mod(a, b int) int {
	return a %% b
}
";

/// A parser-supported file with no declarations and no comments: it must not
/// dilute the ratios.
const README_MD: &str = "# Fixture\n\nProse only.\n";

/// Fixture totals: Add, Sub, Mul, Div, Mod.
const TOTAL_FUNCTIONS: i64 = 5;
/// Add (line comment directly above) and Div (block comment directly above).
const DOCUMENTED_FUNCTIONS: i64 = 2;
/// Package comment, Add doc, inline comment, Mul comment, Div doc.
const TOTAL_COMMENTS: i64 = 5;
/// Add doc + Div doc.
const GOOD_COMMENTS: i64 = 2;

/// Writes the multi-file fixture under `root`.
fn write_fixture(root: &Path) {
    std::fs::create_dir_all(root).expect("fixture dir");
    std::fs::write(root.join("a.go"), A_GO).expect("a.go");
    std::fs::write(root.join("b.go"), B_GO.replace("%%", "%")).expect("b.go");
    std::fs::write(root.join("README.md"), README_MD).expect("README.md");
}

/// Runs `codefang run -a static/comments` with a deterministic environment
/// from `cwd`.
fn run_comments(cwd: &Path, args: &[&str]) -> Output {
    common::command(BIN)
        .current_dir(cwd)
        .args(["run", "-a", "static/comments"])
        .args(args)
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .output()
        .expect("run the codefang binary")
}

/// Runs the json format and parses stdout.
fn run_json(cwd: &Path, args: &[&str]) -> Value {
    let out = run_comments(cwd, &[&["--format", "json"], args].concat());
    assert!(
        out.status.success(),
        "exit {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("json stdout")
}

fn section(report: &Value) -> &Value {
    &report["sections"][0]
}

/// The `value` of the key metric labelled `label`.
fn metric<'a>(section: &'a Value, label: &str) -> &'a str {
    section["metrics"]
        .as_array()
        .expect("metrics array")
        .iter()
        .find(|m| m["label"] == label)
        .unwrap_or_else(|| panic!("metric {label} missing in {section}"))["value"]
        .as_str()
        .expect("metric value")
}

fn distribution_count(section: &Value, label: &str) -> i64 {
    section["distribution"]
        .as_array()
        .expect("distribution array")
        .iter()
        .find(|d| d["label"] == label)
        .unwrap_or_else(|| panic!("distribution {label} missing in {section}"))["count"]
        .as_i64()
        .expect("count")
}

/// `(name, location)` of every issue, in emitted order.
fn issues(section: &Value) -> Vec<(String, String)> {
    section["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .map(|i| {
            (
                i["name"].as_str().expect("name").to_string(),
                i["location"].as_str().expect("location").to_string(),
            )
        })
        .collect()
}

#[test]
fn multi_file_go_fixture_reports_weighted_coverage() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_fixture(tmp.path());
    let report = run_json(tmp.path(), &["."]);
    let s = section(&report);

    assert_eq!(s["title"], "COMMENTS");
    assert_eq!(metric(s, "Total Functions"), TOTAL_FUNCTIONS.to_string());
    assert_eq!(metric(s, "Total Comments"), TOTAL_COMMENTS.to_string());
    assert_eq!(metric(s, "Good Comments"), GOOD_COMMENTS.to_string());
    assert_eq!(
        metric(s, "Bad Comments"),
        (TOTAL_COMMENTS - GOOD_COMMENTS).to_string()
    );
    // 2 of 5 functions, NOT the mean of per-file ratios (1/1, 1/4, README).
    assert_eq!(metric(s, "Doc Coverage"), "40.0%");
    assert_eq!(metric(s, "Good Ratio"), "40.0%");
    assert_eq!(distribution_count(s, "Documented"), DOCUMENTED_FUNCTIONS);
    assert_eq!(
        distribution_count(s, "Undocumented"),
        TOTAL_FUNCTIONS - DOCUMENTED_FUNCTIONS
    );
    assert_eq!(s["score"], 0.4);
    assert_eq!(s["score_label"], "4/10");
    assert_eq!(
        s["status"],
        "Fair comment quality - consider improving placement"
    );
    assert_eq!(
        issues(s),
        vec![
            ("Mod".to_string(), "b.go".to_string()),
            ("Mul".to_string(), "b.go".to_string()),
            ("Sub".to_string(), "a.go".to_string()),
        ]
    );
}

#[test]
fn per_file_paths_stay_root_relative_from_a_relative_cwd() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("fixture");
    write_fixture(&root);
    std::fs::create_dir_all(root.join("sub")).expect("sub dir");
    std::fs::write(root.join("sub").join("c.go"), A_GO).expect("sub/c.go");

    // Relative root, relative cwd: the bug emitted "../../a.go".
    let report = run_json(tmp.path(), &["--per-file", "fixture"]);
    let s = section(&report);
    let files: Vec<&str> = s["files"]
        .as_array()
        .expect("files array")
        .iter()
        .map(|f| f["file_path"].as_str().expect("file_path"))
        .collect();
    assert_eq!(files, vec!["README.md", "a.go", "b.go", "sub/c.go"]);

    let sub_entry = s["files"]
        .as_array()
        .expect("files array")
        .iter()
        .find(|f| f["file_path"] == "sub/c.go")
        .expect("sub/c.go entry");
    assert_eq!(
        issues(sub_entry),
        vec![("Sub".to_string(), "sub/c.go".to_string())]
    );
    assert!(
        issues(s).contains(&("Sub".to_string(), "sub/c.go".to_string())),
        "section issues keep the same relative location: {:?}",
        issues(s)
    );
}

#[test]
fn single_file_root_stamps_the_file_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_fixture(tmp.path());
    let report = run_json(tmp.path(), &["--per-file", "a.go"]);
    let s = section(&report);
    assert_eq!(s["files"][0]["file_path"], "a.go");
    assert_eq!(issues(s), vec![("Sub".to_string(), "a.go".to_string())]);
}

#[test]
fn empty_directory_reports_the_empty_report_with_exit_zero() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = run_comments(tmp.path(), &["--format", "json", "."]);
    assert_eq!(out.status.code(), Some(0));
    let report: Value = serde_json::from_slice(&out.stdout).expect("json stdout");
    let s = section(&report);
    assert_eq!(s["status"], "No comments found");
    assert_eq!(metric(s, "Total Functions"), "0");
    assert_eq!(metric(s, "Doc Coverage"), "0.0%");
    assert!(
        s.get("distribution").is_none(),
        "no functions, no distribution"
    );
    assert_eq!(issues(s), Vec::<(String, String)>::new());
}

#[test]
fn json_yaml_and_text_agree_on_the_counts() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_fixture(tmp.path());

    let json = run_json(tmp.path(), &["."]);
    let s = section(&json);
    assert_eq!(metric(s, "Total Functions"), TOTAL_FUNCTIONS.to_string());

    let yaml = run_comments(tmp.path(), &["--format", "yaml", "."]);
    assert!(yaml.status.success());
    let yaml = String::from_utf8(yaml.stdout).expect("utf-8 yaml");
    for needle in [
        &format!("total_functions: {TOTAL_FUNCTIONS}"),
        &format!("documented_functions: {DOCUMENTED_FUNCTIONS}"),
        &format!("total_comments: {TOTAL_COMMENTS}"),
        &format!("good_comments: {GOOD_COMMENTS}"),
        "documentation_coverage: 0.4",
        "good_comments_ratio: 0.4",
        "name: Add",
        "status: Documented",
        "name: Sub",
        "status: Undocumented",
    ] {
        assert!(yaml.contains(needle), "yaml lacks {needle:?}:\n{yaml}");
    }
    // Only undocumented functions are listed as such.
    let undocumented = yaml
        .split("undocumented_functions:")
        .nth(1)
        .and_then(|rest| rest.split("aggregate:").next())
        .expect("undocumented_functions block");
    assert_eq!(undocumented.matches("- name:").count(), 3, "{undocumented}");
    assert!(!undocumented.contains("name: Add"), "{undocumented}");

    let text = run_comments(tmp.path(), &["--format", "text", "."]);
    assert!(text.status.success());
    let text = String::from_utf8_lossy(&text.stdout)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for needle in [
        &format!("Total Functions {TOTAL_FUNCTIONS}"),
        &format!("Total Comments {TOTAL_COMMENTS}"),
        "Doc Coverage 40.0%",
        "Good Ratio 40.0%",
    ] {
        assert!(text.contains(needle), "text lacks {needle:?}:\n{text}");
    }
}
