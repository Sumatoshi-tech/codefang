//! End-to-end coverage of `history/typos`, driven through the **real binary**
//! over on-disk git fixtures (`specs/bugs/BUG-history-typos-false-positives.md`).
//!
//! The contract (`site/analyzers/typos.md`): a typo is ONE identifier that a
//! commit changed into a DIFFERENT, Levenshtein-close identifier on the same
//! line; `typo_list` carries every distinct `wrong → correct` pair once (the
//! chronologically first fix wins), `patterns` counts every occurrence of a
//! pair, `line` is 1-based, the report is a pure function of the repository
//! and the flags, and `bin` carries the json value.

mod common;

use std::process::Command;

use cf_gitlib::testutil::{commit_files, init_repo, TestRepo};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture histories: 2021-01-01T00:00:00Z.
const T0: i64 = 1_609_459_200;
/// One day between fixture commits.
const DAY: i64 = 24 * 3_600;
/// Size of the CFB1 envelope header (`magic:4` + `length:4`).
const BIN_HEADER: usize = 8;
/// The 1-based line of `func receive()` in [`FIXED_GO`].
const RECEIVE_DECL_LINE: i64 = 3;

/// Before the fix: a misspelled function used on two lines, a function that
/// is about to be renamed to something unrelated, and a literal constant.
const TYPO_GO: &[u8] = b"package main

func recieve() int { return 1 }

func main() {
	v := recieve()
	_ = v
	foo()
}

func foo() {}

const numConTimes = 4
";

/// After the fix: `recieve` → `receive` on both lines, `foo` →
/// `completelyDifferent` (a rename, not a typo) and `4` → `17` (a literal
/// change that leaves the identifier untouched).
const FIXED_GO: &[u8] = b"package main

func receive() int { return 1 }

func main() {
	v := receive()
	_ = v
	completelyDifferent()
}

func completelyDifferent() {}

const numConTimes = 17
";

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

/// The typos report of `test` as JSON.
fn typos_json(test: &TestRepo) -> serde_json::Value {
    json_ok(&["-a", "history/typos", "--format", "json", path(test)])
}

/// A single-file history: the typo'd source, then the fixing commit.
fn fix_fixture() -> TestRepo {
    let test = init_repo().expect("init");
    commit_files(
        &test,
        "typo",
        T0,
        &[("a.go", TYPO_GO), ("README.md", b"# Hello\n")],
    )
    .expect("typo");
    commit_files(
        &test,
        "fix",
        T0 + DAY,
        &[("a.go", FIXED_GO), ("README.md", b"# Hallo\n")],
    )
    .expect("fix");
    test
}

/// The same pair fixed in two files, five commits apart, with filler commits
/// between them: `b.go` (walk index 1) is the chronologically first fix.
fn duplicate_pair_fixture() -> (TestRepo, String) {
    let test = init_repo().expect("init");
    commit_files(
        &test,
        "idx0 add",
        T0,
        &[
            ("b.go", b"package main\n\nfunc main() {\n\trecieve()\n}\n"),
            ("e.go", b"package main\n\nfunc other() {\n\trecieve()\n}\n"),
            ("z.go", b"package main\n\nfunc filler() {}\n"),
        ],
    )
    .expect("idx0");
    let first_fix = commit_files(
        &test,
        "idx1 fix b.go",
        T0 + DAY,
        &[("b.go", b"package main\n\nfunc main() {\n\treceive()\n}\n")],
    )
    .expect("idx1");
    for i in 2..4_i64 {
        commit_files(
            &test,
            &format!("idx{i}"),
            T0 + i * DAY,
            &[(
                "z.go",
                format!("package main\n\nfunc filler() {{}}\n// {i}\n").as_bytes(),
            )],
        )
        .expect("filler");
    }
    commit_files(
        &test,
        "idx4 fix e.go",
        T0 + 4 * DAY,
        &[("e.go", b"package main\n\nfunc other() {\n\treceive()\n}\n")],
    )
    .expect("idx4");
    commit_files(
        &test,
        "idx5",
        T0 + 5 * DAY,
        &[("z.go", b"package main\n\nfunc filler() {}\n// 5\n")],
    )
    .expect("idx5");
    (test, first_fix.to_string())
}

/// `recieve → receive` fixed on two lines is ONE `typo_list` entry (1-based
/// line of the first fix) and ONE pattern with frequency 2; the rename and
/// the literal change are not typos; `file_typos` and `aggregate` count
/// occurrences.
#[test]
fn a_pair_fixed_on_two_lines_is_reported_once_with_frequency_two() {
    let test = fix_fixture();
    let fix = test.repo.head().expect("head").to_string();
    let report = typos_json(&test);

    let list = report["typo_list"].as_array().expect("typo_list");
    assert_eq!(list.len(), 1, "{report}");
    assert_eq!(list[0]["wrong"], "recieve", "{report}");
    assert_eq!(list[0]["correct"], "receive", "{report}");
    assert_eq!(list[0]["file"], "a.go", "{report}");
    assert_eq!(
        list[0]["line"], RECEIVE_DECL_LINE,
        "line is 1-based: {report}"
    );
    assert_eq!(list[0]["commit"], fix.as_str(), "{report}");

    let patterns = report["patterns"].as_array().expect("patterns is an array");
    assert_eq!(patterns.len(), 1, "{report}");
    assert_eq!(patterns[0]["wrong"], "recieve");
    assert_eq!(patterns[0]["correct"], "receive");
    assert_eq!(patterns[0]["frequency"], 2, "{report}");

    let files = report["file_typos"].as_array().expect("file_typos");
    assert_eq!(files.len(), 1, "{report}");
    assert_eq!(files[0]["file"], "a.go");
    assert_eq!(files[0]["typo_count"], 2, "{report}");

    assert_eq!(report["aggregate"]["total_typos"], 2, "{report}");
    assert_eq!(report["aggregate"]["unique_patterns"], 1, "{report}");
    assert_eq!(report["aggregate"]["affected_files"], 1, "{report}");
    assert_eq!(report["aggregate"]["affected_commits"], 1, "{report}");
}

/// Neither an unrelated rename, nor a literal change next to an unchanged
/// identifier, nor a Markdown heading edit is a typo — and no reported pair
/// is ever empty or self-identical.
#[test]
fn renames_literal_changes_and_prose_are_not_typos() {
    let test = fix_fixture();
    let report = typos_json(&test);
    let list = report["typo_list"].as_array().expect("typo_list");
    for typo in list {
        let wrong = typo["wrong"].as_str().expect("wrong");
        let correct = typo["correct"].as_str().expect("correct");
        assert!(!wrong.is_empty() && !correct.is_empty(), "{report}");
        assert_ne!(wrong, correct, "{report}");
        assert_eq!(typo["file"], "a.go", "{report}");
    }
    assert!(
        !list.iter().any(|t| t["correct"] == "completelyDifferent"),
        "a rename is not a typo: {report}"
    );
    assert!(
        !list.iter().any(|t| t["wrong"] == "numConTimes"),
        "a literal change is not a typo: {report}"
    );
}

/// The chronologically first fix wins the `typo_list` slot, and the bytes do
/// not depend on the host CPU count.
#[test]
fn first_fix_wins_regardless_of_the_cpu_count() {
    let (test, first_fix) = duplicate_pair_fixture();
    let p = path(&test);
    let args = ["-a", "history/typos", "--format", "json", p];

    let unpinned = codefang(&args);
    assert!(unpinned.status.success());
    let report: serde_json::Value = serde_json::from_slice(&unpinned.stdout).expect("json");
    let list = report["typo_list"].as_array().expect("typo_list");
    assert_eq!(list.len(), 1, "{report}");
    assert_eq!(list[0]["file"], "b.go", "the first fix wins: {report}");
    assert_eq!(list[0]["commit"], first_fix.as_str(), "{report}");
    assert_eq!(report["patterns"][0]["frequency"], 2, "{report}");
    assert_eq!(report["aggregate"]["affected_commits"], 2, "{report}");

    let pinned = codefang_pinned("0-1", &args);
    if !pinned.status.success() {
        eprintln!(
            "skipping the pinned run: taskset -c 0-1 is not usable here: {}",
            String::from_utf8_lossy(&pinned.stderr)
        );
        return;
    }
    assert_eq!(
        pinned.stdout, unpinned.stdout,
        "json must be byte-identical under taskset -c 0-1"
    );
}

/// `--format bin` carries exactly the json value; `--format yaml` renders the
/// same report under the analyzer header.
#[test]
fn bin_and_yaml_carry_the_json_value() {
    let test = fix_fixture();
    let p = path(&test);
    let json = stdout_ok(&["-a", "history/typos", "--format", "json", p]);
    let bin = stdout_ok(&["-a", "history/typos", "--format", "bin", p]);
    assert_eq!(&bin[..4], b"CFB1");
    assert_eq!(
        &bin[BIN_HEADER..],
        &json[..],
        "bin payload is the json value"
    );

    let yaml = String::from_utf8(stdout_ok(&["-a", "history/typos", "--format", "yaml", p]))
        .expect("utf-8 yaml");
    assert!(yaml.starts_with("codefang (v2):\n"), "{yaml}");
    assert!(yaml.contains("history/typos:\n"), "{yaml}");
    assert!(yaml.contains("wrong: recieve"), "{yaml}");
    assert!(yaml.contains("frequency: 2"), "{yaml}");
}

/// A history without any fix yields the empty report with every list
/// present as `[]`.
#[test]
fn no_fix_is_the_empty_report() {
    let test = init_repo().expect("init");
    commit_files(&test, "only", T0, &[("a.go", TYPO_GO)]).expect("only");
    let report = typos_json(&test);
    assert_eq!(report["typo_list"], serde_json::json!([]), "{report}");
    assert_eq!(report["patterns"], serde_json::json!([]), "{report}");
    assert_eq!(report["file_typos"], serde_json::json!([]), "{report}");
    assert_eq!(report["aggregate"]["total_typos"], 0, "{report}");
}
