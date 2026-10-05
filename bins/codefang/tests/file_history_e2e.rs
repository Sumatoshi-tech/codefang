//! End-to-end coverage of `history/file-history`, driven through the **real
//! binary** over real on-disk git fixtures
//! (`specs/bugs/BUG-history-file-history-lifecycle.md`).
//!
//! Every number asserted here is computed by hand from the fixture per
//! `site/explanation/file-history.md`: a file's commit list follows its
//! renames, every commit's line stats are attributed to its author, only the
//! files present at the window head are reported, the composition counts
//! distinct files (not path actions) by content-aware category, and the
//! output never depends on the host CPU count.

mod common;

use std::process::Command;

use cf_gitlib::testutil::{commit_files_as, init_repo, CommitBuilder, TestRepo};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture histories: 2021-01-01T00:00:00Z.
const T0: i64 = 1_609_459_200;
/// One day between fixture commits (one default tick each).
const DAY: i64 = 24 * 3_600;
/// The fixture developers.
const ALICE: (&str, &str) = ("alice", "alice@example.com");
const BOB: (&str, &str) = ("bob", "bob@example.com");
/// Number of modifications of the single file in the composition fixture.
const SINGLE_FILE_EDITS: i64 = 8;
/// Tolerance for the hand-computed churn scores.
const EPS: f64 = 1e-9;

fn codefang_with(mut cmd: Command, args: &[&str]) -> std::process::Output {
    cmd.arg("run")
        .args(args)
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .output()
        .expect("run the codefang binary")
}

/// Runs the binary with a deterministic environment.
fn codefang(args: &[&str]) -> std::process::Output {
    codefang_with(common::command(BIN), args)
}

/// Runs the binary under `taskset -c <cpus>`.
fn codefang_pinned(cpus: &str, args: &[&str]) -> std::process::Output {
    let mut cmd = common::command("taskset");
    cmd.args(["-c", cpus, BIN]);
    codefang_with(cmd, args)
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

fn path(test: &TestRepo) -> &str {
    test.path().to_str().expect("utf-8 path")
}

/// The file-history arguments in `format` over `repo` with `extra` flags.
fn args<'a>(repo: &'a TestRepo, format: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["-a", "history/file-history", "--format", format];
    args.extend_from_slice(extra);
    args.push(path(repo));
    args
}

/// A `history/file-history` run in `format` over `repo`.
fn file_history(repo: &TestRepo, format: &str, extra: &[&str]) -> String {
    stdout_ok(&args(repo, format, extra))
}

/// The file-history JSON report over `repo`.
fn file_history_json(repo: &TestRepo, extra: &[&str]) -> serde_json::Value {
    let text = file_history(repo, "json", extra);
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}):\n{text}"))
}

/// The `file_churn` entry of `path`.
fn churn<'a>(report: &'a serde_json::Value, path: &str) -> &'a serde_json::Value {
    report["file_churn"]
        .as_array()
        .expect("file_churn")
        .iter()
        .find(|e| e["path"] == path)
        .unwrap_or_else(|| panic!("no file_churn entry for {path} in {report}"))
}

/// The sorted paths listed under `file_churn`.
fn churn_paths(report: &serde_json::Value) -> Vec<String> {
    let mut paths: Vec<String> = report["file_churn"]
        .as_array()
        .expect("file_churn")
        .iter()
        .map(|e| e["path"].as_str().expect("path").to_string())
        .collect();
    paths.sort();
    paths
}

/// The `contributors` entry of developer `dev_id` on `path`.
fn contributor<'a>(
    report: &'a serde_json::Value,
    path: &str,
    dev_id: i64,
) -> &'a serde_json::Value {
    report["file_contributors"]
        .as_array()
        .expect("file_contributors")
        .iter()
        .find(|e| e["path"] == path)
        .and_then(|e| {
            e["contributors"]
                .as_array()?
                .iter()
                .find(|c| c["dev_id"] == dev_id)
        })
        .unwrap_or_else(|| panic!("no contributor {dev_id} on {path} in {report}"))
}

/// The ndjson record of commit `hash`.
fn record(ndjson: &str, hash: &str) -> serde_json::Value {
    let line = ndjson
        .lines()
        .find(|l| l.contains(hash))
        .unwrap_or_else(|| panic!("no record for {hash} in:\n{ndjson}"));
    serde_json::from_str(line).expect("ndjson line")
}

/// The `(added, removed, changed)` of the single line-stat update in `record`.
fn line_stats(record: &serde_json::Value) -> (i64, i64, i64) {
    let stats = &record["data"]["LineStatUpdates"][0]["Stats"];
    (
        stats["added"].as_i64().expect("added"),
        stats["removed"].as_i64().expect("removed"),
        stats["changed"].as_i64().expect("changed"),
    )
}

/// The lifecycle of one file next to a `keep.txt` bystander:
/// `c1 alice create f (3 lines)`, `c2 alice change l2 + add l4`,
/// `c3 bob remove l1`, `c4 alice git mv f g`, `c5 bob add l5`.
fn lifecycle_fixture() -> (TestRepo, Vec<String>) {
    let test = init_repo().expect("init");
    let c1 = commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c1 create f",
        T0,
        &[("keep.txt", b"k\n"), ("f.txt", b"l1\nl2\nl3\n")],
    )
    .expect("c1");
    let c2 = commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c2 change l2, add l4",
        T0 + DAY,
        &[("f.txt", b"l1\nl2x\nl3\nl4\n")],
    )
    .expect("c2");
    let c3 = commit_files_as(
        &test,
        BOB.0,
        BOB.1,
        "c3 remove l1",
        T0 + 2 * DAY,
        &[("f.txt", b"l2x\nl3\nl4\n")],
    )
    .expect("c3");
    let c4 = CommitBuilder::new(&test, "c4 rename f->g", T0 + 3 * DAY)
        .author(ALICE.0, ALICE.1)
        .rename("f.txt", "g.txt")
        .commit()
        .expect("c4");
    let c5 = commit_files_as(
        &test,
        BOB.0,
        BOB.1,
        "c5 add l5",
        T0 + 4 * DAY,
        &[("g.txt", b"l2x\nl3\nl4\nl5\n")],
    )
    .expect("c5");
    let hashes = [c1, c2, c3, c4, c5]
        .iter()
        .map(ToString::to_string)
        .collect();
    (test, hashes)
}

/// g.txt's commit list follows the rename (5 commits), its line stats are
/// attributed per author, and every commit's ndjson record carries that
/// commit's own line stats.
#[test]
fn commit_list_and_line_stats_follow_the_rename() {
    let (test, hashes) = lifecycle_fixture();
    let report = file_history_json(&test, &[]);

    assert_eq!(churn_paths(&report), ["g.txt", "keep.txt"], "{report}");
    let g = churn(&report, "g.txt");
    assert_eq!(g["commit_count"], 5, "{report}");
    assert_eq!(g["contributor_count"], 2);
    assert_eq!(g["total_lines_added"], 5, "3 + 1 + 1");
    assert_eq!(g["total_lines_removed"], 1);
    assert_eq!(g["total_lines_changed"], 1);
    let score = g["churn_score"].as_f64().unwrap();
    assert!((score - 5.07).abs() < EPS, "5 + 7/100: {score}");
    // alice (dev 0): c1 +3, c2 +1 / ~1; bob (dev 1): c3 -1, c5 +1.
    let alice = contributor(&report, "g.txt", 0);
    assert_eq!(
        (
            alice["added"].as_i64(),
            alice["removed"].as_i64(),
            alice["changed"].as_i64()
        ),
        (Some(4), Some(0), Some(1)),
        "{report}"
    );
    let bob = contributor(&report, "g.txt", 1);
    assert_eq!(
        (
            bob["added"].as_i64(),
            bob["removed"].as_i64(),
            bob["changed"].as_i64()
        ),
        (Some(1), Some(1), Some(0)),
        "{report}"
    );
    assert_eq!(churn(&report, "keep.txt")["commit_count"], 1);
    assert_eq!(report["aggregate"]["total_files"], 2);
    assert_eq!(report["aggregate"]["total_commits"], 6, "5 + 1");

    let ndjson = file_history(&test, "ndjson", &[]);
    let c2 = record(&ndjson, &hashes[1]);
    assert_eq!(line_stats(&c2), (1, 0, 1), "{c2}");
    let c3 = record(&ndjson, &hashes[2]);
    assert_eq!(line_stats(&c3), (0, 1, 0), "{c3}");
    let c4 = record(&ndjson, &hashes[3]);
    assert_eq!(c4["data"]["PathActions"][0]["FromPath"], "f.txt", "{c4}");
    assert_eq!(c4["data"]["PathActions"][0]["ToPath"], "g.txt", "{c4}");
    assert_eq!(line_stats(&c4), (0, 0, 0), "a pure rename: {c4}");
    let c5 = record(&ndjson, &hashes[4]);
    assert_eq!(line_stats(&c5), (1, 0, 0), "{c5}");
}

/// A file deleted at the window head is dropped from the report.
#[test]
fn deleted_files_leave_the_report() {
    let (test, _) = lifecycle_fixture();
    CommitBuilder::new(&test, "c6 delete g", T0 + 5 * DAY)
        .author(ALICE.0, ALICE.1)
        .delete("g.txt")
        .commit()
        .expect("c6");
    let report = file_history_json(&test, &[]);
    assert_eq!(churn_paths(&report), ["keep.txt"], "{report}");
    assert_eq!(report["aggregate"]["total_files"], 1);
    assert_eq!(report["aggregate"]["total_commits"], 1);

    let yaml = file_history(&test, "yaml", &[]);
    assert!(yaml.contains("- path: keep.txt"), "{yaml}");
    assert!(!yaml.contains("g.txt"), "{yaml}");
}

/// The composition counts FILES by content-aware category: a binary blob is
/// `binary` (not `source` by its path), and a single file edited eight times
/// is ONE source file in the aggregate while every tick lists it once; every
/// tick carries its time bounds.
#[test]
fn composition_counts_distinct_files_by_content_category() {
    let test = init_repo().expect("init");
    commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c0",
        T0,
        &[
            ("main.go", b"package main\n"),
            ("bin.dat", b"\x00\x01\x02\xff\x00\x00"),
        ],
    )
    .expect("c0");
    let report = file_history_json(&test, &[]);
    assert_eq!(report["composition"]["breakdown"]["binary"], 1, "{report}");
    assert_eq!(report["composition"]["breakdown"]["source"], 1, "{report}");
    assert_eq!(
        report["composition"]["percentages"]["binary"], 50.0,
        "{report}"
    );

    let single = init_repo().expect("init");
    for i in 0..SINGLE_FILE_EDITS {
        let body = format!("package main\n// edit {i}\n");
        commit_files_as(
            &single,
            ALICE.0,
            ALICE.1,
            &format!("edit {i}"),
            T0 + i * DAY,
            &[("main.go", body.as_bytes())],
        )
        .expect("edit");
    }
    let report = file_history_json(&single, &[]);
    assert_eq!(report["composition"]["breakdown"]["source"], 1, "{report}");
    assert_eq!(
        report["composition"]["percentages"]["source"], 100.0,
        "{report}"
    );
    let ts = report["composition_ts"].as_array().expect("composition_ts");
    assert_eq!(ts.len() as i64, SINGLE_FILE_EDITS, "{report}");
    for (i, entry) in ts.iter().enumerate() {
        assert_eq!(entry["tick"], i as i64, "{entry}");
        assert_eq!(entry["breakdown"]["source"], 1, "{entry}");
        let expected = format!("2021-01-{:02}T00:00:00Z", i + 1);
        assert_eq!(entry["start_time"], expected.as_str(), "{entry}");
        assert_eq!(entry["end_time"], expected.as_str(), "{entry}");
    }

    let text = file_history(&single, "text", &[]);
    assert!(text.contains("source               1 (100.0%)"), "{text}");
}

/// `h.txt` created, modified four times, deleted, then recreated after three
/// `other.txt` commits: the recreated h.txt has ONE commit, other.txt three
/// — independent of the host CPU count and of a window that drops the
/// oldest commit.
#[test]
fn head_files_and_consume_order_do_not_depend_on_the_cpu_count() {
    let test = init_repo().expect("init");
    let mut when = (1..).map(|n| T0 + n * DAY);
    commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c0",
        when.next().unwrap(),
        &[("h.txt", b"h0\n")],
    )
    .expect("c0");
    let mut body = b"h0\n".to_vec();
    for i in 1..=4 {
        body.extend_from_slice(format!("h{i}\n").as_bytes());
        commit_files_as(
            &test,
            ALICE.0,
            ALICE.1,
            "modify h",
            when.next().unwrap(),
            &[("h.txt", &body)],
        )
        .expect("modify");
    }
    CommitBuilder::new(&test, "c5 delete h", when.next().unwrap())
        .author(BOB.0, BOB.1)
        .delete("h.txt")
        .commit()
        .expect("c5");
    let mut other = Vec::new();
    for i in 6..=8 {
        other.extend_from_slice(format!("o{i}\n").as_bytes());
        commit_files_as(
            &test,
            BOB.0,
            BOB.1,
            "other",
            when.next().unwrap(),
            &[("other.txt", &other)],
        )
        .expect("other");
    }
    commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c9 recreate h",
        when.next().unwrap(),
        &[("h.txt", b"new\n")],
    )
    .expect("c9");

    let full = file_history_json(&test, &[]);
    assert_eq!(churn(&full, "h.txt")["commit_count"], 1, "{full}");
    assert_eq!(churn(&full, "other.txt")["commit_count"], 3, "{full}");
    assert_eq!(full["aggregate"]["total_files"], 2);

    let nine = file_history_json(&test, &["--limit", "9"]);
    assert_eq!(churn(&nine, "h.txt")["commit_count"], 1, "{nine}");
    assert_eq!(churn(&nine, "other.txt")["commit_count"], 3, "{nine}");

    let json_args = args(&test, "json", &[]);
    let pinned = codefang_pinned("0-1", &json_args);
    if !pinned.status.success() {
        eprintln!(
            "skipping the taskset comparison: {}",
            String::from_utf8_lossy(&pinned.stderr)
        );
        return;
    }
    let unpinned = codefang(&json_args);
    assert_eq!(
        pinned.stdout, unpinned.stdout,
        "json must not depend on the CPU count"
    );
    let ndjson_args = args(&test, "ndjson", &[]);
    let pinned = codefang_pinned("0-1", &ndjson_args);
    let unpinned = codefang(&ndjson_args);
    assert_eq!(
        pinned.stdout, unpinned.stdout,
        "ndjson must not depend on the CPU count"
    );
}
