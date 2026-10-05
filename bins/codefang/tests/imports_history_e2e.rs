//! End-to-end coverage of `history/imports`, driven through the **real
//! binary** over on-disk git fixtures
//! (`specs/bugs/BUG-history-imports-report.md`).
//!
//! The contract (`site/analyzers/imports.md`, History section): the json /
//! yaml / bin report is `tick_size` (seconds), `author_index` (author id →
//! name) and `imports`: developer → language → import → tick → count, with
//! the parser's language names and the per-commit extraction the ndjson
//! records show; two runs emit identical bytes.

mod common;

use cf_gitlib::testutil::{commit_files_as, init_repo, TestRepo};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture histories: 2021-01-01T12:00:00Z.
const T0: i64 = 1_609_502_400;
/// One day between fixture commits.
const DAY: i64 = 24 * 3_600;
/// The default tick size in seconds.
const DEFAULT_TICK_SECS: i64 = 86_400;
/// Size of the CFB1 envelope header.
const BIN_HEADER: usize = 8;

const ALICE: (&str, &str) = ("Alice", "alice@x.io");
const BOB: (&str, &str) = ("Bob", "bob@x.io");

const A_GO_V1: &[u8] = b"package main

import (
	\"fmt\"
	\"os\"
)

func main() { fmt.Println(os.Args) }
";
const B_GO: &[u8] = b"package main

import (
	\"encoding/json\"
	\"fmt\"
)

func b() { fmt.Println(json.Valid(nil)) }
";
const C_PY: &[u8] = b"import os\nimport sys\n\nprint(os.getcwd(), sys.argv)\n";
const A_GO_V2: &[u8] = b"package main

import (
	\"fmt\"
	\"net/http\"
)

func main() { fmt.Println(http.StatusOK) }
";

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

/// Alice adds `a.go` (fmt, os) on day 0; Bob adds `b.go` (encoding/json,
/// fmt) and `c.py` (os, sys) on day 1; Alice rewrites `a.go` (fmt,
/// net/http) on day 2.
fn fixture() -> TestRepo {
    let test = init_repo().expect("init");
    commit_files_as(&test, ALICE.0, ALICE.1, "i1", T0, &[("a.go", A_GO_V1)]).expect("i1");
    commit_files_as(
        &test,
        BOB.0,
        BOB.1,
        "i2",
        T0 + DAY,
        &[("b.go", B_GO), ("c.py", C_PY)],
    )
    .expect("i2");
    commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "i3",
        T0 + 2 * DAY,
        &[("a.go", A_GO_V2)],
    )
    .expect("i3");
    test
}

/// Whether the identity detector's `names|emails` label names `who`.
fn is_developer(label: &str, who: &str) -> bool {
    label.to_lowercase().starts_with(&who.to_lowercase())
}

/// The developer entry of `report["imports"]` labelled `who`.
fn developer<'a>(report: &'a serde_json::Value, who: &str) -> &'a serde_json::Value {
    let imports = report["imports"].as_object().expect("imports object");
    imports
        .iter()
        .find(|(name, _)| is_developer(name, who))
        .map(|(_, v)| v)
        .unwrap_or_else(|| panic!("no developer {who} in {report}"))
}

/// `--format json` is the documented 4-level map with real language names
/// and per-tick counts, plus `tick_size` and `author_index`.
#[test]
fn json_carries_developer_language_import_tick_counts() {
    let test = fixture();
    let report = json_ok(&["-a", "history/imports", "--format", "json", path(&test)]);
    assert_eq!(report["tick_size"], DEFAULT_TICK_SECS, "{report}");

    let index = report["author_index"].as_array().expect("author_index");
    assert_eq!(index.len(), 2, "{report}");
    assert!(
        is_developer(index[0].as_str().unwrap(), ALICE.0),
        "{report}"
    );
    assert!(is_developer(index[1].as_str().unwrap(), BOB.0), "{report}");

    let alice = developer(&report, ALICE.0);
    assert_eq!(alice["go"]["fmt"]["0"], 1, "{report}");
    assert_eq!(alice["go"]["fmt"]["2"], 1, "{report}");
    assert_eq!(alice["go"]["os"]["0"], 1, "{report}");
    assert_eq!(alice["go"]["net/http"]["2"], 1, "{report}");
    assert!(alice["go"]["os"].get("2").is_none(), "{report}");

    let bob = developer(&report, BOB.0);
    assert_eq!(bob["go"]["encoding/json"]["1"], 1, "{report}");
    assert_eq!(bob["go"]["fmt"]["1"], 1, "{report}");
    assert_eq!(bob["python"]["os"]["1"], 1, "{report}");
    assert_eq!(bob["python"]["sys"]["1"], 1, "{report}");
    assert!(
        bob.get("uast").is_none(),
        "no placeholder language: {report}"
    );
}

/// yaml renders the same map under the analyzer header; bin carries the json
/// value; two runs are byte-identical.
#[test]
fn yaml_and_bin_carry_the_same_report_deterministically() {
    let test = fixture();
    let p = path(&test);
    let yaml = String::from_utf8(stdout_ok(&["-a", "history/imports", "--format", "yaml", p]))
        .expect("utf-8 yaml");
    assert!(yaml.starts_with("codefang (v2):\n"), "{yaml}");
    assert!(yaml.contains("ImportsPerDeveloper:\n"), "{yaml}");
    assert!(yaml.contains("tick_size: 86400"), "{yaml}");
    assert!(yaml.contains("net/http:"), "{yaml}");
    assert!(yaml.contains("python:"), "{yaml}");
    assert!(yaml.contains("author_index:"), "{yaml}");

    let json = stdout_ok(&["-a", "history/imports", "--format", "json", p]);
    let bin = stdout_ok(&["-a", "history/imports", "--format", "bin", p]);
    assert_eq!(&bin[..4], b"CFB1");
    assert_eq!(
        &bin[BIN_HEADER..],
        &json[..],
        "bin payload is the json value"
    );

    let again = stdout_ok(&["-a", "history/imports", "--format", "json", p]);
    assert_eq!(json, again, "two runs are byte-identical");
    let yaml_again = stdout_ok(&["-a", "history/imports", "--format", "yaml", p]);
    assert_eq!(yaml.as_bytes(), &yaml_again[..]);
}

/// The ndjson records carry the parser's language names.
#[test]
fn ndjson_entries_carry_real_language_names() {
    let test = fixture();
    let ndjson = String::from_utf8(stdout_ok(&[
        "-a",
        "history/imports",
        "--format",
        "ndjson",
        path(&test),
    ]))
    .expect("utf-8");
    assert!(
        ndjson.contains(r#"{"Lang":"go","Import":"fmt"}"#),
        "{ndjson}"
    );
    assert!(
        ndjson.contains(r#"{"Lang":"python","Import":"sys"}"#),
        "{ndjson}"
    );
    assert!(!ndjson.contains(r#""Lang":"uast""#), "{ndjson}");
}

/// A history without any supported file is the empty report, not an error.
#[test]
fn no_imports_is_the_empty_report() {
    let test = init_repo().expect("init");
    commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "txt",
        T0,
        &[("notes.txt", b"hello\n")],
    )
    .expect("txt");
    let report = json_ok(&["-a", "history/imports", "--format", "json", path(&test)]);
    assert_eq!(report["tick_size"], DEFAULT_TICK_SECS, "{report}");
    assert_eq!(report["imports"], serde_json::json!({}), "{report}");
    assert_eq!(
        report["author_index"].as_array().map(Vec::len),
        Some(1),
        "{report}"
    );
}
