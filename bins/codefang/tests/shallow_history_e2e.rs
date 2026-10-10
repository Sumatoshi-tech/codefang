//! End-to-end coverage of history analysis on a shallow clone.
//!
//! libgit2 marks a shallow clone with `.git/shallow`, and `Repository::log`
//! yields no commits for such a repository (its history beyond the boundary
//! is absent, so the walk is not meaningful). Before this suite the history
//! analyzers then emitted their empty reports silently with exit 0. The
//! contract pinned here: the run still succeeds with the empty report, and
//! stderr carries one `warning:` line naming the cause and the remedy. A full
//! clone emits no such warning.

mod common;

use cf_gitlib::testutil::{commit_files, init_repo, TestRepo};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// 2021-01-01T12:00:00Z, the fixture's first commit.
const T0: i64 = 1_609_502_400;
/// One day.
const DAY: i64 = 24 * 3_600;
/// Commits in the fixture.
const COMMITS: usize = 3;
/// Stable prefix of the shallow-clone warning.
const SHALLOW_WARNING: &str = "warning: shallow clone";

/// A repository with `COMMITS` linear commits by one author.
fn three_commit_repo() -> TestRepo {
    let test = init_repo().expect("init repo");
    for i in 0..COMMITS {
        let body = format!("package main\n\nfunc f{i}() int {{\n\treturn {i}\n}}\n");
        commit_files(
            &test,
            &format!("commit {i}"),
            T0 + DAY * i as i64,
            &[("main.go", body.as_bytes())],
        )
        .expect("commit");
    }
    test
}

/// Marks HEAD as the shallow boundary, the on-disk form of `git clone --depth 1`.
fn make_shallow(test: &TestRepo) {
    let head = git2::Repository::open(test.path())
        .expect("open with git2")
        .head()
        .expect("head")
        .target()
        .expect("head target");
    std::fs::write(test.path().join(".git").join("shallow"), format!("{head}\n"))
        .expect("write .git/shallow");
}

/// Runs `codefang run -a history/devs --format json` and returns
/// `(total_commits, stderr)`.
fn devs_total_commits(repo: &str) -> (i64, String) {
    let out = common::command(BIN)
        .arg("run")
        .args(["-a", "history/devs", "--format", "json", repo])
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .output()
        .expect("run the codefang binary");
    assert!(
        out.status.success(),
        "run must still succeed on a shallow clone: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("stdout is one JSON document");
    let total = report["aggregate"]["total_commits"]
        .as_i64()
        .expect("total_commits");
    (total, String::from_utf8_lossy(&out.stderr).into_owned())
}

#[test]
fn full_clone_walks_every_commit_without_a_warning() {
    let test = three_commit_repo();
    let (total, stderr) = devs_total_commits(test.path().to_str().expect("utf-8 path"));
    assert_eq!(total, COMMITS as i64);
    assert!(
        !stderr.contains(SHALLOW_WARNING),
        "a full clone must not warn: {stderr}"
    );
}

#[test]
fn shallow_clone_warns_once_and_reports_no_commits() {
    let test = three_commit_repo();
    make_shallow(&test);
    let (total, stderr) = devs_total_commits(test.path().to_str().expect("utf-8 path"));
    assert_eq!(total, 0, "a shallow walk yields no commits");
    assert_eq!(
        stderr.matches(SHALLOW_WARNING).count(),
        1,
        "exactly one shallow warning per run: {stderr}"
    );
    assert!(
        stderr.contains("git fetch --unshallow"),
        "the warning names the remedy: {stderr}"
    );
}
