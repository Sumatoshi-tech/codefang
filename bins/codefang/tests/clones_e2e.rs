//! End-to-end coverage of `static/clones`, driven through the **real binary**
//! over hermetic on-disk fixtures.
//!
//! The contract under test (see `site/analyzers/clones.md`):
//!
//! * Type-1 = identical structure AND identical tokens; Type-2 = identical
//!   structure, renamed identifiers / literals; Type-3 = modified structure
//!   whose exact shingle Jaccard clears the 0.5 threshold;
//! * `similarity` is the exact shingle-set Jaccard of the pair;
//! * `clone_ratio` = functions in at least one pair / total functions, and the
//!   status message is keyed off that ratio;
//! * pairs are ordered by similarity descending, then by name;
//! * closures with a large-enough body are clone candidates;
//! * `--per-file` lists each file's functions that take part in a pair.

mod common;

use std::path::Path;

use serde_json::Value;

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");

/// Section title of the clones report.
const SECTION_TITLE: &str = "CLONE DETECTION";

/// Exact shingle Jaccard of [`PROCESS_ITEMS`] vs [`PROCESS_ITEMS_PLUS`]
/// (one statement and one call argument added): 41 shared 5-shingles over a
/// union of 64. Pinned with a tolerance that still rejects the MinHash
/// estimate the old engine reported (it never got past the LSH bands).
const T3_EXPECTED_SIMILARITY: f64 = 41.0 / 64.0;
const SIMILARITY_TOLERANCE: f64 = 0.02;

const PROCESS_ITEMS: &str = r#"package main

import "fmt"

func ProcessItems(items []int) int {
	total := 0
	for i, v := range items {
		if v%2 == 0 {
			total += v * i
		} else {
			total -= v
		}
	}
	fmt.Println("total", total)
	return total
}
"#;

/// [`PROCESS_ITEMS`] with every identifier and literal renamed.
const SUM_VALUES: &str = r#"package main

import "fmt"

func SumValues(values []int) int {
	acc := 0
	for idx, val := range values {
		if val%2 == 0 {
			acc += val * idx
		} else {
			acc -= val
		}
	}
	fmt.Println("acc", acc)
	return acc
}
"#;

/// [`PROCESS_ITEMS`] with one statement and one call argument added.
const PROCESS_ITEMS_PLUS: &str = r#"package main

import "fmt"

func ProcessItemsPlus(items []int) int {
	total := 0
	count := len(items)
	for i, v := range items {
		if v%2 == 0 {
			total += v * i
		} else {
			total -= v
		}
	}
	fmt.Println("total", total, count)
	return total
}
"#;

/// [`PROCESS_ITEMS`] with operators and literals changed, structure intact.
const PROCESS_ITEMS_V4: &str = r#"package main

import "fmt"

func ProcessItemsV4(items []int) int {
	total := 1
	for i, v := range items {
		if v%3 != 0 {
			total -= v / i
		} else {
			total += v
		}
	}
	fmt.Println("sum", total)
	return total
}
"#;

/// A function with nothing in common with [`PROCESS_ITEMS`].
const DESCRIBE: &str = r#"package main

import "strings"

type Server struct {
	name string
	tags map[string][]string
}

func (s *Server) Describe(prefix string) string {
	var b strings.Builder
	b.WriteString(prefix)
	b.WriteString(": ")
	b.WriteString(s.name)
	for k, vs := range s.tags {
		b.WriteString(" ")
		b.WriteString(k)
		b.WriteString("=")
		b.WriteString(strings.Join(vs, ","))
	}
	return strings.ToUpper(b.String())
}
"#;

/// Below the minimum function size: counted, never paired.
const TINY_ADD: &str = "package main\n\nfunc Add(a, b int) int {\n\treturn a + b\n}\n";

/// Two identical package-level closures.
const CLOSURES: &str = r#"package main

import "fmt"

var first = func(items []int) int {
	total := 0
	for i, v := range items {
		if v%2 == 0 {
			total += v * i
		} else {
			total -= v
		}
	}
	fmt.Println("total", total)
	return total
}

var second = func(items []int) int {
	total := 0
	for i, v := range items {
		if v%2 == 0 {
			total += v * i
		} else {
			total -= v
		}
	}
	fmt.Println("total", total)
	return total
}
"#;

/// Writes `files` (relative path, content) into a fresh temp dir.
fn fixture(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for (rel, content) in files {
        let path = dir.path().join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, content).expect("write fixture");
    }
    dir
}

/// Runs `codefang run -a static/clones <extra> <path>` with a pinned
/// environment and returns the raw stdout bytes.
fn run_clones(path: &Path, extra: &[&str]) -> Vec<u8> {
    let out = common::command(BIN)
        .arg("run")
        .args(["-a", "static/clones"])
        .args(extra)
        .arg(path)
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .output()
        .expect("run the codefang binary");
    assert!(
        out.status.success(),
        "run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

/// The single clones section of a `--format json` run.
fn clones_section(path: &Path, extra: &[&str]) -> Value {
    let stdout = run_clones(path, &[&["--format", "json"], extra].concat());
    let report: Value = serde_json::from_slice(&stdout).expect("valid JSON report");
    let sections = report["sections"].as_array().expect("sections array");
    assert_eq!(sections.len(), 1, "one clones section expected");
    assert_eq!(sections[0]["title"], SECTION_TITLE);
    sections[0].clone()
}

/// Looks up a `metrics[].value` by label.
fn metric(section: &Value, label: &str) -> String {
    section["metrics"]
        .as_array()
        .expect("metrics array")
        .iter()
        .find(|m| m["label"] == label)
        .unwrap_or_else(|| panic!("metric {label} missing in {section}"))["value"]
        .as_str()
        .expect("metric value string")
        .to_string()
}

/// `(name, clone_type, similarity)` of every issue, in emitted order.
fn issues(section: &Value) -> Vec<(String, String, f64)> {
    section["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .map(|i| {
            (
                i["name"].as_str().expect("issue name").to_string(),
                i["location"].as_str().expect("clone type").to_string(),
                i["value"]
                    .as_str()
                    .expect("similarity string")
                    .parse::<f64>()
                    .expect("similarity parses"),
            )
        })
        .collect()
}

/// `count` of the distribution row labelled `label`.
fn distribution_count(section: &Value, label: &str) -> i64 {
    section["distribution"]
        .as_array()
        .expect("distribution array")
        .iter()
        .find(|d| d["label"] == label)
        .unwrap_or_else(|| panic!("distribution row {label} missing"))["count"]
        .as_i64()
        .expect("count")
}

fn assert_close(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() <= SIMILARITY_TOLERANCE,
        "{what}: got {got}, want {want} ± {SIMILARITY_TOLERANCE}"
    );
}

/// Byte-identical copies across files are one Type-1 pair with similarity
/// 1.0; every function is cloned so the ratio is 1.0 and the status says so.
#[test]
fn identical_pair_is_type1_with_similarity_one() {
    let dir = fixture(&[("a.go", PROCESS_ITEMS), ("b.go", PROCESS_ITEMS)]);
    let section = clones_section(dir.path(), &[]);

    assert_eq!(metric(&section, "Total Functions"), "2");
    assert_eq!(metric(&section, "Clone Pairs"), "1");
    assert_eq!(metric(&section, "Clone Ratio"), "1.0");
    assert_eq!(
        section["status"],
        "High duplication - significant refactoring recommended"
    );
    assert_eq!(section["score_label"], "0/10");

    let got = issues(&section);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "a.go::ProcessItems <-> b.go::ProcessItems");
    assert_eq!(got[0].1, "Type-1");
    assert_close(got[0].2, 1.0, "Type-1 similarity");
    assert_eq!(distribution_count(&section, "Type-1 (Exact)"), 1);
}

/// Renaming every identifier and literal keeps the structure: Type-2, 1.0.
#[test]
fn renamed_pair_is_type2() {
    let dir = fixture(&[("a.go", PROCESS_ITEMS), ("c.go", SUM_VALUES)]);
    let section = clones_section(dir.path(), &[]);

    let got = issues(&section);
    assert_eq!(got.len(), 1, "one pair expected: {got:?}");
    assert_eq!(got[0].0, "a.go::ProcessItems <-> c.go::SumValues");
    assert_eq!(got[0].1, "Type-2");
    assert_close(got[0].2, 1.0, "Type-2 similarity");
    assert_eq!(distribution_count(&section, "Type-2 (Renamed)"), 1);
    assert_eq!(distribution_count(&section, "Type-1 (Exact)"), 0);
}

/// Changing operators and literals only is still identical structure: Type-2.
#[test]
fn operator_and_literal_changes_are_type2() {
    let dir = fixture(&[("a.go", PROCESS_ITEMS), ("b.go", PROCESS_ITEMS_V4)]);
    let section = clones_section(dir.path(), &[]);

    let got = issues(&section);
    assert_eq!(got.len(), 1, "one pair expected: {got:?}");
    assert_eq!(got[0].1, "Type-2");
    assert_close(got[0].2, 1.0, "Type-2 similarity");
}

/// One added statement + one added call argument modifies the structure: the
/// pair is a Type-3 clone found with its exact Jaccard (≈0.63), which the old
/// 16×8 LSH bands missed entirely.
#[test]
fn added_statement_pair_is_type3_above_threshold() {
    let dir = fixture(&[("a.go", PROCESS_ITEMS), ("d.go", PROCESS_ITEMS_PLUS)]);
    let section = clones_section(dir.path(), &[]);

    let got = issues(&section);
    assert_eq!(got.len(), 1, "Type-3 pair must be found: {got:?}");
    assert_eq!(got[0].0, "a.go::ProcessItems <-> d.go::ProcessItemsPlus");
    assert_eq!(got[0].1, "Type-3");
    assert_close(got[0].2, T3_EXPECTED_SIMILARITY, "Type-3 exact Jaccard");
    assert_eq!(distribution_count(&section, "Type-3 (Near-miss)"), 1);
}

/// Unrelated functions never pair; the status and score reflect zero clones.
#[test]
fn unrelated_functions_are_not_paired() {
    let dir = fixture(&[("a.go", PROCESS_ITEMS), ("e.go", DESCRIBE)]);
    let section = clones_section(dir.path(), &[]);

    assert_eq!(metric(&section, "Total Functions"), "2");
    assert_eq!(metric(&section, "Clone Pairs"), "0");
    assert_eq!(metric(&section, "Clone Ratio"), "0.0");
    assert_eq!(section["status"], "No code clones detected");
    assert_eq!(section["score_label"], "10/10");
    assert!(issues(&section).is_empty());
}

/// A lone function is never paired with itself.
#[test]
fn single_function_has_no_self_pair() {
    let dir = fixture(&[("a.go", PROCESS_ITEMS)]);
    let section = clones_section(dir.path(), &[]);
    assert_eq!(metric(&section, "Total Functions"), "1");
    assert_eq!(metric(&section, "Clone Pairs"), "0");
    assert!(issues(&section).is_empty());
}

/// Three identical copies form three pairs, listed in name order among equal
/// similarities (deterministic tie order).
#[test]
fn three_copies_yield_three_pairs_in_name_order() {
    let dir = fixture(&[
        ("a.go", PROCESS_ITEMS),
        ("b.go", PROCESS_ITEMS),
        ("g.go", PROCESS_ITEMS),
    ]);
    let section = clones_section(dir.path(), &[]);

    assert_eq!(metric(&section, "Clone Pairs"), "3");
    let names: Vec<String> = issues(&section).into_iter().map(|i| i.0).collect();
    assert_eq!(
        names,
        vec![
            "a.go::ProcessItems <-> b.go::ProcessItems",
            "a.go::ProcessItems <-> g.go::ProcessItems",
            "b.go::ProcessItems <-> g.go::ProcessItems",
        ]
    );
}

/// Functions below the minimum node count are counted but never paired.
#[test]
fn small_functions_are_counted_but_never_paired() {
    let dir = fixture(&[("a.go", TINY_ADD), ("b.go", TINY_ADD)]);
    let section = clones_section(dir.path(), &[]);
    assert_eq!(metric(&section, "Total Functions"), "2");
    assert_eq!(metric(&section, "Clone Pairs"), "0");
    assert_eq!(section["status"], "No code clones detected");
}

/// Closures (UAST `Lambda`) with a large-enough body are clone candidates,
/// named by their enclosing scope and ordinal.
#[test]
fn closures_are_clone_candidates() {
    let dir = fixture(&[("a.go", CLOSURES)]);
    let section = clones_section(dir.path(), &[]);

    assert_eq!(metric(&section, "Total Functions"), "2");
    let got = issues(&section);
    assert_eq!(got.len(), 1, "the two closures form one pair: {got:?}");
    assert_eq!(got[0].0, "a.go::func#1 <-> a.go::func#2");
    assert_eq!(got[0].1, "Type-1");
}

/// `--per-file` lists, per file, the functions that take part in a clone pair.
#[test]
fn per_file_lists_each_files_cloned_functions() {
    let dir = fixture(&[
        ("a.go", PROCESS_ITEMS),
        ("b.go", PROCESS_ITEMS),
        ("e.go", DESCRIBE),
    ]);
    let section = clones_section(dir.path(), &["--per-file"]);

    let files = section["files"].as_array().expect("files array");
    let paths: Vec<&str> = files
        .iter()
        .map(|f| f["file_path"].as_str().expect("file_path"))
        .collect();
    assert_eq!(paths, vec!["a.go", "b.go", "e.go"]);

    let a = &files[0];
    assert_eq!(metric(a, "Total Functions"), "1");
    assert_eq!(metric(a, "Cloned Functions"), "1");
    assert_eq!(a["status"], "1 of 1 functions in clone pairs");
    let a_issues = issues(a);
    assert_eq!(a_issues.len(), 1);
    assert_eq!(a_issues[0].0, "a.go::ProcessItems <-> b.go::ProcessItems");

    let e = &files[2];
    assert_eq!(metric(e, "Cloned Functions"), "0");
    assert_eq!(e["status"], "No functions in clone pairs");
    assert!(issues(e).is_empty());
}

/// Without `--per-file` the section carries no `files` key.
#[test]
fn no_per_file_flag_omits_files() {
    let dir = fixture(&[("a.go", PROCESS_ITEMS), ("b.go", PROCESS_ITEMS)]);
    let section = clones_section(dir.path(), &[]);
    assert!(section.get("files").is_none(), "files must be omitted");
}

/// Two runs over the same tree produce identical bytes in every format.
#[test]
fn two_runs_are_byte_identical() {
    let dir = fixture(&[
        ("a.go", PROCESS_ITEMS),
        ("b.go", PROCESS_ITEMS),
        ("c.go", SUM_VALUES),
        ("d.go", PROCESS_ITEMS_PLUS),
    ]);
    for format in ["json", "yaml", "text", "compact"] {
        let first = run_clones(dir.path(), &["--format", format]);
        let second = run_clones(dir.path(), &["--format", format]);
        assert_eq!(first, second, "--format {format} must be deterministic");
    }
}

/// The YAML metrics carry the exact pair list and the ratio-keyed message.
#[test]
fn yaml_carries_computed_metrics() {
    let dir = fixture(&[("a.go", PROCESS_ITEMS), ("d.go", PROCESS_ITEMS_PLUS)]);
    let yaml = String::from_utf8(run_clones(dir.path(), &["--format", "yaml"])).unwrap();
    assert!(yaml.contains("total_functions: 2\n"), "{yaml}");
    assert!(yaml.contains("total_clone_pairs: 1\n"), "{yaml}");
    assert!(yaml.contains("clone_ratio: 1\n"), "{yaml}");
    assert!(yaml.contains("clone_type: Type-3\n"), "{yaml}");
    assert!(
        yaml.contains("message: High duplication - significant refactoring recommended\n"),
        "{yaml}"
    );
}
