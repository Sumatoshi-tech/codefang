//! End-to-end contract: the unified `codefang.run.v1` report describes every
//! field it emits.
//!
//! For each analyzer, the `schema` object declares exactly the keys of its
//! `report` object. A consumer that reads the schema to learn the report's
//! fields must never meet an undeclared field, nor a declared field the report
//! lacks. This suite runs the full analyzer selection (`-a '*'`) over a real
//! git fixture and checks both directions for every analyzer, so a renamed or
//! added report field without a schema-registry update fails here.

mod common;

use cf_gitlib::testutil::{commit_files, init_repo};
use std::collections::BTreeSet;

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// 2021-01-01T12:00:00Z, the fixture's first commit.
const T0: i64 = 1_609_502_400;
/// One day.
const DAY: i64 = 24 * 3_600;
/// Analyzers a full selection emits (static + history).
const ANALYZER_COUNT: usize = 17;

fn full_report(repo: &str) -> serde_json::Value {
    let out = common::command(BIN)
        .arg("run")
        .args(["-a", "*", "--format", "json", repo])
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .output()
        .expect("run the codefang binary");
    assert!(
        out.status.success(),
        "full run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stdout is one JSON document")
}

fn keys(value: &serde_json::Value, what: &str) -> BTreeSet<String> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{what} is not an object: {value}"))
        .keys()
        .cloned()
        .collect()
}

#[test]
fn every_analyzer_schema_declares_exactly_its_report_fields() {
    let test = init_repo().expect("init repo");
    let path = test.path().to_str().expect("utf-8 path").to_string();
    commit_files(
        &test,
        "init",
        T0,
        &[
            (
                "main.go",
                b"package main\n\nfunc add(a, b int) int {\n\treturn a + b\n}\n",
            ),
            ("util.py", b"def hi(x):\n    return x + 1\n"),
        ],
    )
    .expect("first commit");
    commit_files(
        &test,
        "refactor",
        T0 + DAY,
        &[(
            "main.go",
            b"package main\n\nfunc add(a, b int) int {\n\tc := a + b\n\treturn c\n}\n",
        )],
    )
    .expect("second commit");

    let doc = full_report(&path);
    let analyzers = doc["analyzers"].as_array().expect("analyzers array");
    assert_eq!(analyzers.len(), ANALYZER_COUNT, "full selection analyzer count");
    // Collect every mismatch so one run lists them all, not just the first.
    let mut mismatches = Vec::new();
    for analyzer in analyzers {
        let id = analyzer["id"].as_str().expect("analyzer id");
        let schema = keys(&analyzer["schema"], &format!("{id} schema"));
        let report = keys(&analyzer["report"], &format!("{id} report"));
        let undeclared: Vec<_> = report.difference(&schema).collect();
        let absent: Vec<_> = schema.difference(&report).collect();
        if !undeclared.is_empty() || !absent.is_empty() {
            mismatches.push(format!(
                "{id}: report fields undeclared in schema {undeclared:?}; \
                 schema fields absent from report {absent:?}"
            ));
        }
    }
    assert!(
        mismatches.is_empty(),
        "schema/report drift:\n{}",
        mismatches.join("\n")
    );
}
