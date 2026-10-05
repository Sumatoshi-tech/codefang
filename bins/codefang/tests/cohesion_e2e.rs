//! End-to-end coverage of `static/cohesion`, driven through the **real binary**
//! over an on-disk fixture tree.
//!
//! The contract under test (`site/analyzers/cohesion.md`,
//! `site/explanation/cohesion.md`, `specs/bugs/BUG-static-cohesion-variables.md`):
//!
//! * a variable is a lexical identifier reference, never a type name, the
//!   function's own name, a callee or a whole declaration token;
//! * LCOM-HS per file is `1 - sum(mA) / (m * a)` over the functions that use
//!   at least one variable; `cohesion_score = 1 - LCOM`;
//! * per-function cohesion is `shared / unique` where "shared" means used by
//!   at least one OTHER function of the same file;
//! * aggregation is function-weighted and function-free files are "no data";
//! * every function is kept (no `(file, name)` dedup), so the distribution
//!   always sums to `Total Functions`;
//! * the issues list holds only functions below the Excellent threshold.

mod common;

use std::path::Path;

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");

/// `f` uses {x, y}, `g` uses {x}, `h` uses {x, y, z}:
/// m = 3, a = 3, sum(mA) = 3 + 2 + 1 = 6, LCOM = 1 - 6/9 = 1/3.
/// f: x, y shared -> 1.0; g: x shared -> 1.0; h: x, y shared, z not -> 2/3.
const SHARE_GO: &str = "package a

func f() int {
	x := 1
	y := 2
	return x + y
}

func g() int {
	x := 3
	return x
}

func h() int {
	x := 1
	y := 2
	z := x + y
	return z
}
";

/// `f2` uses only {p} (twice), `g2` uses only {q}:
/// m = 2, a = 2, sum(mA) = 2, LCOM = 0.5; both functions share nothing -> 0.0.
const DISJOINT_GO: &str = "package a

func f2() int {
	p := 1
	p = p + 1
	return p
}

func g2() int {
	q := 2
	return q
}
";

/// A struct-only file: no functions, so no cohesion data.
const TYPES_GO: &str = "package a

type Config struct {
	Name string
	Port int
}
";

/// Expected per-fixture numbers (see the fixture docs above).
const SHARE_LCOM: f64 = 1.0 / 3.0;
const DISJOINT_LCOM: f64 = 0.5;
const SHARE_FUNCTIONS: f64 = 3.0;
const DISJOINT_FUNCTIONS: f64 = 2.0;
const TOTAL_FUNCTIONS: i64 = 5;
/// Function-weighted LCOM: (1/3 * 3 + 0.5 * 2) / 5 = 0.4.
const EXPECTED_LCOM: f64 =
    (SHARE_LCOM * SHARE_FUNCTIONS + DISJOINT_LCOM * DISJOINT_FUNCTIONS) / 5.0;
/// Mean per-function cohesion: (1 + 1 + 2/3 + 0 + 0) / 5.
const EXPECTED_AVG_COHESION: f64 = (1.0 + 1.0 + 2.0 / 3.0) / 5.0;
const EPS: f64 = 1e-9;

/// Runs `codefang run -a static/cohesion` with a deterministic environment.
fn cohesion(format: &str, extra: &[&str], path: &Path) -> std::process::Output {
    common::command(BIN)
        .arg("run")
        .args(["-a", "static/cohesion", "--format", format])
        .args(extra)
        .arg(path)
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .output()
        .expect("run the codefang binary")
}

/// The multi-file Go fixture plus two function-free files (an empty source
/// file and a Markdown file) that must not influence any number.
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("share.go"), SHARE_GO).unwrap();
    std::fs::write(dir.path().join("disjoint.go"), DISJOINT_GO).unwrap();
    std::fs::write(dir.path().join("types.go"), TYPES_GO).unwrap();
    std::fs::write(dir.path().join("empty.go"), "").unwrap();
    std::fs::write(dir.path().join("README.md"), "# fixture\n\nno code here\n").unwrap();
    dir
}

fn json_section(path: &Path, extra: &[&str]) -> serde_json::Value {
    let out = cohesion("json", extra, path);
    assert!(
        out.status.success(),
        "json run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout is a JSON report");
    report["sections"][0].clone()
}

fn metric<'a>(section: &'a serde_json::Value, label: &str) -> &'a str {
    section["metrics"]
        .as_array()
        .expect("metrics array")
        .iter()
        .find(|m| m["label"] == label)
        .unwrap_or_else(|| panic!("metric {label} missing in {section}"))["value"]
        .as_str()
        .expect("metric value is a string")
}

fn count_of(section: &serde_json::Value, label_prefix: &str) -> i64 {
    section["distribution"]
        .as_array()
        .expect("distribution array")
        .iter()
        .find(|d| d["label"].as_str().unwrap_or("").starts_with(label_prefix))
        .unwrap_or_else(|| panic!("bucket {label_prefix} missing in {section}"))["count"]
        .as_i64()
        .expect("count is an integer")
}

/// The headline numbers of the multi-file fixture are exactly the hand-computed
/// LCOM-HS values, aggregated function-weighted over the two files that have
/// functions.
#[test]
fn multi_file_go_fixture_reports_exact_numbers() {
    let dir = fixture();
    let section = json_section(dir.path(), &[]);

    assert_eq!(
        metric(&section, "Total Functions"),
        TOTAL_FUNCTIONS.to_string()
    );
    assert_eq!(
        metric(&section, "LCOM Score"),
        format!("{EXPECTED_LCOM:.2}")
    );
    assert_eq!(
        metric(&section, "Cohesion Score"),
        format!("{:.2}", 1.0 - EXPECTED_LCOM)
    );
    assert_eq!(
        metric(&section, "Avg Cohesion"),
        format!("{EXPECTED_AVG_COHESION:.2}")
    );

    let score = section["score"].as_f64().expect("score is a float");
    assert!(
        (score - (1.0 - EXPECTED_LCOM)).abs() < EPS,
        "score {score} != {}",
        1.0 - EXPECTED_LCOM
    );
    assert_eq!(section["score_label"], "6/10");
    assert_eq!(
        section["status"],
        "Good overall cohesion with room for improvement"
    );
}

/// Every function is kept: the distribution buckets sum to Total Functions
/// and land in the documented buckets (f, g, h excellent; f2, g2 poor).
#[test]
fn distribution_sums_to_total_functions() {
    let dir = fixture();
    let section = json_section(dir.path(), &[]);

    let excellent = count_of(&section, "Excellent");
    let good = count_of(&section, "Good");
    let fair = count_of(&section, "Fair");
    let poor = count_of(&section, "Poor");
    assert_eq!(excellent + good + fair + poor, TOTAL_FUNCTIONS);
    assert_eq!((excellent, good, fair, poor), (3, 0, 0, 2));
}

/// Only functions below the Excellent threshold are issues, ordered by
/// cohesion ascending then name, with two-decimal values.
#[test]
fn issues_list_only_the_non_excellent_functions() {
    let dir = fixture();
    let section = json_section(dir.path(), &[]);

    let issues: Vec<(String, String, String, String)> = section["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .map(|i| {
            (
                i["name"].as_str().unwrap().to_string(),
                i["location"].as_str().unwrap().to_string(),
                i["value"].as_str().unwrap().to_string(),
                i["severity"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        issues,
        vec![
            (
                "f2".into(),
                "disjoint.go".into(),
                "0.00".into(),
                "poor".into()
            ),
            (
                "g2".into(),
                "disjoint.go".into(),
                "0.00".into(),
                "poor".into()
            ),
        ]
    );
}

/// An existing but empty directory is "no data": the empty report, exit 0.
#[test]
fn empty_directory_yields_the_empty_report_with_exit_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = cohesion("json", &[], dir.path());
    assert_eq!(out.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let section = &report["sections"][0];
    assert_eq!(section["status"], "No cohesion data available");
    assert_eq!(metric(section, "Total Functions"), "0");
    assert_eq!(section["score"], 0.0);
    assert_eq!(section["score_label"], "0/10");
    assert_eq!(report["overall_score_label"], "0/10");
    assert!(section.get("distribution").is_none());
    assert_eq!(section["issues"].as_array().map(Vec::len), Some(0));
}

/// Function-free files (struct-only, zero-byte, Markdown) are "no data": a tree
/// holding only those yields the empty report, not a perfect score.
#[test]
fn function_free_files_are_no_data() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("types.go"), TYPES_GO).unwrap();
    std::fs::write(dir.path().join("empty.go"), "").unwrap();
    std::fs::write(dir.path().join("README.md"), "# fixture\n").unwrap();
    let section = json_section(dir.path(), &[]);
    assert_eq!(section["status"], "No cohesion data available");
    assert_eq!(metric(&section, "Total Functions"), "0");
    assert_eq!(section["score_label"], "0/10");
}

/// `--per-file` lists every analyzed file with its own LCOM; function-free
/// files report "No functions found".
#[test]
fn per_file_entries_carry_file_level_lcom() {
    let dir = fixture();
    let section = json_section(dir.path(), &["--per-file"]);
    let files = section["files"].as_array().expect("files array");
    let entry = |name: &str| {
        files
            .iter()
            .find(|f| f["file_path"] == name)
            .unwrap_or_else(|| panic!("no entry for {name}"))
    };

    let share = entry("share.go");
    assert_eq!(metric(share, "Total Functions"), "3");
    assert_eq!(metric(share, "LCOM Score"), format!("{SHARE_LCOM:.2}"));
    assert_eq!(
        metric(share, "Cohesion Score"),
        format!("{:.2}", 1.0 - SHARE_LCOM)
    );
    assert_eq!(count_of(share, "Excellent"), 3);

    let disjoint = entry("disjoint.go");
    assert_eq!(
        metric(disjoint, "LCOM Score"),
        format!("{DISJOINT_LCOM:.2}")
    );
    assert_eq!(count_of(disjoint, "Poor"), 2);

    let types = entry("types.go");
    assert_eq!(types["status"], "No functions found");
    assert_eq!(metric(types, "Total Functions"), "0");
}

/// JSON, YAML and text agree on the headline numbers; text renders the
/// Distribution and issue sections like the sibling analyzers do.
#[test]
fn json_yaml_and_text_agree() {
    let dir = fixture();
    let section = json_section(dir.path(), &[]);

    let yaml_out = cohesion("yaml", &[], dir.path());
    assert!(yaml_out.status.success());
    let yaml = String::from_utf8(yaml_out.stdout).unwrap();
    assert!(
        yaml.contains(&format!("total_functions: {TOTAL_FUNCTIONS}")),
        "yaml total_functions:\n{yaml}"
    );
    assert!(
        yaml.contains("lcom: 0.4\n"),
        "yaml lcom must match the json section:\n{yaml}"
    );
    assert!(
        yaml.contains("cohesion_score: 0.6\n"),
        "yaml cohesion_score must match the json section:\n{yaml}"
    );
    assert!(
        yaml.contains("excellent: 3\n") && yaml.contains("poor: 2\n"),
        "{yaml}"
    );
    assert!(
        yaml.contains("message: Good overall cohesion with room for improvement"),
        "{yaml}"
    );

    let text_out = cohesion("text", &[], dir.path());
    assert!(text_out.status.success());
    let text = String::from_utf8(text_out.stdout).unwrap();
    for label in [
        "Total Functions",
        "LCOM Score",
        "Cohesion Score",
        "Avg Cohesion",
    ] {
        let value = metric(&section, label);
        assert!(
            text.contains(label) && text.contains(value),
            "text must carry {label} = {value}:\n{text}"
        );
    }
    assert!(
        text.contains("Distribution"),
        "no Distribution block:\n{text}"
    );
    assert!(text.contains("Issues"), "no issues block:\n{text}");
    assert!(
        text.contains("f2") && text.contains("g2"),
        "issues missing:\n{text}"
    );
    assert!(
        !text.contains(" h "),
        "excellent functions must not be listed as issues:\n{text}"
    );
}
