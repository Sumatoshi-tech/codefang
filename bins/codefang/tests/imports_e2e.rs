//! End-to-end coverage of `static/imports`, driven through the **real binary**
//! over an on-disk fixture tree.
//!
//! The contract (see `specs/bugs/BUG-static-imports-extraction.md` and
//! `site/analyzers/imports.md`): one entry per imported module/package/file,
//! aliases and specifier lists never count, the aggregate value of an import
//! is the number of files importing it, an empty root yields the empty report
//! with exit 0, a file root analyzes that one file, and output is
//! deterministic.

mod common;

use serde_json::Value;
use std::path::Path;

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");

/// Go: plain, aliased, dot, blank, duplicate, external and relative specs.
const GO_SRC: &str = "package main\n\nimport \"fmt\"\n\nimport (\n\t\"os\"\n\tstr \"strings\"\n\t. \"math\"\n\t_ \"embed\"\n\t\"fmt\"\n\t\"github.com/example/lib/v2\"\n\t\"github.com/example/lib/v2\"\n\t\"../relative/pkg\"\n)\n\nfunc main() {\n\tfmt.Println(os.Args, str.ToUpper(\"x\"), Pi, lib.X)\n}\n";
/// Python: comma list, dotted module, `from` form, aliases, relative forms.
const PY_SRC: &str =
    "import os, sys\nimport os.path\nfrom typing import List, Dict\nfrom x import y as z\nfrom . import sibling\nfrom ..pkg.sub import thing\nimport os\nimport numpy as np\n";
/// JavaScript: `require`, default/named/namespace/side-effect/mixed ES imports,
/// and a dynamic `import()`.
const JS_SRC: &str = "const fs = require('fs');\nconst path = require(\"path\");\nimport React from 'react';\nimport { useState, useEffect } from 'react';\nimport * as ns from 'ns-lib';\nimport './side-effect.css';\nimport def, { named } from './local/mod';\nasync function f() { const m = await import('dyn-mod'); return m; }\n";
/// Go stdlib packages that the old 17-name table classified as external.
const GO_STDLIB_SRC: &str = "package main\n\nimport (\n\t\"errors\"\n\t\"log\"\n\t\"strconv\"\n\t\"testing\"\n)\n\nfunc f() { _ = errors.New(\"x\"); log.Println(strconv.Itoa(1)); var _ testing.T }\n";
/// Python stdlib modules that the old 7-name table classified as external.
const PY_STDLIB_SRC: &str = "import datetime\nimport subprocess\nimport math\n";

/// The expected `(import, files importing it)` pairs for [`fixture`], in the
/// report's order: count descending, then name ascending.
const EXPECTED_AGGREGATE: &[(&str, &str)] = &[
    ("os", "2"),
    ("../relative/pkg", "1"),
    ("..pkg.sub", "1"),
    ("./local/mod", "1"),
    ("./side-effect.css", "1"),
    (".sibling", "1"),
    ("dyn-mod", "1"),
    ("embed", "1"),
    ("fmt", "1"),
    ("fs", "1"),
    ("github.com/example/lib/v2", "1"),
    ("math", "1"),
    ("ns-lib", "1"),
    ("numpy", "1"),
    ("os.path", "1"),
    ("path", "1"),
    ("react", "1"),
    ("strings", "1"),
    ("sys", "1"),
    ("typing", "1"),
    ("x", "1"),
];

/// Per-file expectations for [`fixture`] (`--per-file` entries list a file's
/// imports alphabetically, each valued `1`).
const EXPECTED_PER_FILE: &[(&str, &[&str])] = &[
    (
        "a.go",
        &[
            "../relative/pkg",
            "embed",
            "fmt",
            "github.com/example/lib/v2",
            "math",
            "os",
            "strings",
        ],
    ),
    (
        "b.py",
        &[
            "..pkg.sub",
            ".sibling",
            "numpy",
            "os",
            "os.path",
            "sys",
            "typing",
            "x",
        ],
    ),
    (
        "c.js",
        &[
            "./local/mod",
            "./side-effect.css",
            "dyn-mod",
            "fs",
            "ns-lib",
            "path",
            "react",
        ],
    ),
];

/// Runs `static/imports` with a deterministic environment.
fn codefang(args: &[&str]) -> std::process::Output {
    common::command(BIN)
        .args(["run", "-a", "static/imports"])
        .args(args)
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .output()
        .expect("run the codefang binary")
}

/// Runs the json format over `path` (+ `extra` flags) and returns
/// `(exit code, parsed stdout, stderr)`.
fn run_json(path: &Path, extra: &[&str]) -> (i32, Value, String) {
    let path = path.to_str().expect("utf-8 path");
    let out = codefang(&[&["--format", "json"], extra, &[path]].concat());
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {stdout}\nstderr: {stderr}"));
    (out.status.code().unwrap_or(-1), json, stderr)
}

/// Writes the three-language fixture into a fresh tempdir.
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.go"), GO_SRC).expect("write a.go");
    std::fs::write(dir.path().join("b.py"), PY_SRC).expect("write b.py");
    std::fs::write(dir.path().join("c.js"), JS_SRC).expect("write c.js");
    dir
}

/// The single IMPORTS section of a `static/imports` json report.
fn section(report: &Value) -> &Value {
    let sections = report["sections"].as_array().expect("sections array");
    assert_eq!(sections.len(), 1, "one section expected: {report}");
    &sections[0]
}

/// The `(name, value)` pairs of a section's or file entry's `issues`.
fn issues(node: &Value) -> Vec<(String, String)> {
    node["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .map(|i| {
            (
                i["name"].as_str().expect("issue name").to_owned(),
                i["value"].as_str().expect("issue value").to_owned(),
            )
        })
        .collect()
}

/// The value of the metric labelled `label`.
fn metric(node: &Value, label: &str) -> String {
    node["metrics"]
        .as_array()
        .expect("metrics array")
        .iter()
        .find(|m| m["label"] == label)
        .unwrap_or_else(|| panic!("metric {label} missing: {node}"))["value"]
        .as_str()
        .expect("metric value")
        .to_owned()
}

/// Each module/package/file is listed exactly once with the number of files
/// importing it; aliases, specifier lists and keywords never appear.
#[test]
fn aggregate_lists_each_module_once_with_file_counts() {
    let dir = fixture();
    let (rc, report, stderr) = run_json(dir.path(), &[]);
    assert_eq!(rc, 0, "run failed: {stderr}");
    let sec = section(&report);

    let expected: Vec<(String, String)> = EXPECTED_AGGREGATE
        .iter()
        .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
        .collect();
    assert_eq!(issues(sec), expected, "issues differ: {sec}");
    assert_eq!(
        metric(sec, "Unique Imports"),
        EXPECTED_AGGREGATE.len().to_string()
    );
    assert_eq!(metric(sec, "Total Files"), "3");
    assert_eq!(
        sec["status"],
        format!("Found {} unique imports", EXPECTED_AGGREGATE.len())
    );
}

/// An empty root (or one with no supported files) is an empty report, not an
/// error: exit 0, zero metrics, no issues.
#[test]
fn empty_root_yields_the_empty_report_with_exit_zero() {
    let empty = tempfile::tempdir().expect("tempdir");
    let unsupported = tempfile::tempdir().expect("tempdir");
    std::fs::write(unsupported.path().join("notes.txt"), "no code here\n").expect("write");

    for dir in [empty.path(), unsupported.path()] {
        let (rc, report, stderr) = run_json(dir, &[]);
        assert_eq!(rc, 0, "empty root must exit 0: {stderr}");
        let sec = section(&report);
        assert_eq!(sec["status"], "No import data available");
        assert_eq!(metric(sec, "Unique Imports"), "0");
        assert_eq!(metric(sec, "Total Files"), "0");
        assert!(issues(sec).is_empty(), "no issues expected: {sec}");
        assert_eq!(report["overall_score_label"], "Info");
    }
}

/// A file given as the root analyzes that single file.
#[test]
fn a_file_root_analyzes_that_single_file() {
    let dir = fixture();
    let (rc, report, stderr) = run_json(&dir.path().join("c.js"), &["--per-file"]);
    assert_eq!(rc, 0, "file root must exit 0: {stderr}");
    let sec = section(&report);
    assert_eq!(metric(sec, "Total Files"), "1");
    let names: Vec<String> = issues(sec).into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        names, EXPECTED_PER_FILE[2].1,
        "file-root imports differ: {sec}"
    );
    let files = sec["files"].as_array().expect("files array");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0]["file_path"], "c.js");
}

/// `--per-file` adds one entry per analyzed file listing exactly that file's
/// imports, each located at the file and counted once per file.
#[test]
fn per_file_entries_list_exactly_each_files_imports() {
    let dir = fixture();
    let (rc, report, stderr) = run_json(dir.path(), &["--per-file"]);
    assert_eq!(rc, 0, "run failed: {stderr}");
    let files = section(&report)["files"].as_array().expect("files array");
    assert_eq!(files.len(), EXPECTED_PER_FILE.len(), "{files:?}");

    for (entry, (path, imports)) in files.iter().zip(EXPECTED_PER_FILE) {
        assert_eq!(entry["file_path"], *path);
        assert_eq!(metric(entry, "Total Files"), "1", "per-file entry: {entry}");
        assert_eq!(metric(entry, "Unique Imports"), imports.len().to_string());
        let got = issues(entry);
        let want: Vec<(String, String)> = imports
            .iter()
            .map(|i| ((*i).to_owned(), "1".to_owned()))
            .collect();
        assert_eq!(got, want, "imports of {path} differ: {entry}");
        for issue in entry["issues"].as_array().expect("issues") {
            assert_eq!(issue["location"], *path);
        }
    }
}

/// The yaml metrics classify stdlib packages by the importing file's language.
#[test]
fn yaml_classifies_stdlib_per_language() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("a.go"), GO_STDLIB_SRC).expect("write a.go");
    std::fs::write(dir.path().join("b.py"), PY_STDLIB_SRC).expect("write b.py");
    let path = dir.path().to_str().expect("utf-8 path");

    let out = codefang(&["--format", "yaml", path]);
    assert_eq!(out.status.code(), Some(0));
    let yaml = String::from_utf8_lossy(&out.stdout);

    for name in [
        "errors",
        "log",
        "strconv",
        "testing",
        "datetime",
        "subprocess",
        "math",
    ] {
        assert!(
            yaml.contains(&format!("- path: {name}\n      category: stdlib\n")),
            "{name} must be stdlib:\n{yaml}"
        );
    }
    assert!(yaml.contains("external_imports: 0\n"), "{yaml}");
    assert!(yaml.contains("internal_imports: 7\n"), "{yaml}");
    assert!(yaml.contains("external_ratio: 0\n"), "{yaml}");
}

/// Two runs over the same tree emit identical bytes.
#[test]
fn two_runs_emit_identical_bytes() {
    let dir = fixture();
    let path = dir.path().to_str().expect("utf-8 path");
    let first = codefang(&["--format", "json", "--per-file", path]);
    let second = codefang(&["--format", "json", "--per-file", path]);
    assert_eq!(first.status.code(), Some(0));
    assert_eq!(
        first.stdout, second.stdout,
        "json output must be deterministic"
    );
}
