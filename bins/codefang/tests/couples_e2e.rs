//! End-to-end coverage of `history/couples`, driven through the **real
//! binary** over real on-disk git fixtures
//! (`specs/bugs/BUG-history-couples-coupling.md`).
//!
//! Every number asserted here is computed by hand from the fixture per the
//! definitions in `site/explanation/couples.md`: a file pair's `co_changes`
//! is the number of walked commits in which BOTH files changed (symmetric,
//! the diagonal is the file's own change count); the developer matrix sums
//! `min(changes_i, changes_j)` over the files both developers changed; the
//! report lists the files present at the window head.

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
/// The documented changeset cap: a commit touching MORE files than this is
/// excluded from the coupling matrix (but still counts for ownership).
const CAP: usize = cf_couples::COUPLES_MAXIMUM_MEANINGFUL_CONTEXT_SIZE;
/// Tolerance for the hand-computed coupling strengths.
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

/// A `history/couples` run in `format` over `repo`, with `extra` flags.
fn couples(repo: &TestRepo, format: &str, extra: &[&str]) -> String {
    let mut args = vec!["-a", "history/couples", "--format", format];
    args.extend_from_slice(extra);
    args.push(path(repo));
    stdout_ok(&args)
}

/// The couples JSON report over `repo`.
fn couples_json(repo: &TestRepo, extra: &[&str]) -> serde_json::Value {
    let text = couples(repo, "json", extra);
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}):\n{text}"))
}

fn path(test: &TestRepo) -> &str {
    test.path().to_str().expect("utf-8 path")
}

/// The `file_coupling` entry for the unordered pair `{x, y}`, if any.
fn pair<'a>(report: &'a serde_json::Value, x: &str, y: &str) -> Option<&'a serde_json::Value> {
    report["file_coupling"]
        .as_array()?
        .iter()
        .find(|e| (e["file1"] == x && e["file2"] == y) || (e["file1"] == y && e["file2"] == x))
}

/// The `file_ownership` entry of `file`.
fn ownership<'a>(report: &'a serde_json::Value, file: &str) -> &'a serde_json::Value {
    report["file_ownership"]
        .as_array()
        .expect("file_ownership")
        .iter()
        .find(|e| e["file"] == file)
        .unwrap_or_else(|| panic!("no ownership entry for {file} in {report}"))
}

/// The sorted file names listed under `file_ownership`.
fn owned_files(report: &serde_json::Value) -> Vec<String> {
    let mut names: Vec<String> = report["file_ownership"]
        .as_array()
        .expect("file_ownership")
        .iter()
        .map(|e| e["file"].as_str().expect("file").to_string())
        .collect();
    names.sort();
    names
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < EPS,
        "{what}: expected {expected}, got {actual}"
    );
}

/// Four linear commits by two developers:
/// `c1 alice {a,b}`, `c2 bob {a,b,c}`, `c3 alice {c}`, `c4 bob {a}`.
fn linear_fixture() -> TestRepo {
    let test = init_repo().expect("init");
    commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c1 {a,b}",
        T0,
        &[("a", b"a1\na2\n"), ("b", b"b1\n")],
    )
    .expect("c1");
    commit_files_as(
        &test,
        BOB.0,
        BOB.1,
        "c2 {a,b,c}",
        T0 + DAY,
        &[("a", b"a1\na2\na3\n"), ("b", b"b1\nb2\n"), ("c", b"c1\n")],
    )
    .expect("c2");
    commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c3 {c}",
        T0 + 2 * DAY,
        &[("c", b"c1\nc2\n")],
    )
    .expect("c3");
    commit_files_as(
        &test,
        BOB.0,
        BOB.1,
        "c4 {a}",
        T0 + 3 * DAY,
        &[("a", b"a1\na2\na3\na4\n")],
    )
    .expect("c4");
    test
}

/// The file matrix, the developer matrix and the ownership of the linear
/// fixture, by hand: a-b 2, a-c 1, b-c 1, diagonal a 3 / b 2 / c 2; alice
/// {a:1,b:1,c:1} vs bob {a:2,b:1,c:1} share 3; every file has 2
/// contributors and its line count at the head.
#[test]
fn file_and_developer_matrices_follow_the_definition() {
    let test = linear_fixture();
    let report = couples_json(&test, &[]);

    let ab = pair(&report, "a", "b").expect("a-b");
    assert_eq!(ab["co_changes"], 2, "{report}");
    assert_close(ab["coupling_strength"].as_f64().unwrap(), 2.0 / 2.5, "a-b");
    let ac = pair(&report, "a", "c").expect("a-c");
    assert_eq!(ac["co_changes"], 1, "{report}");
    assert_close(ac["coupling_strength"].as_f64().unwrap(), 1.0 / 2.5, "a-c");
    let bc = pair(&report, "b", "c").expect("b-c");
    assert_eq!(bc["co_changes"], 1, "{report}");
    assert_close(bc["coupling_strength"].as_f64().unwrap(), 1.0 / 2.0, "b-c");
    assert_eq!(report["file_coupling"].as_array().unwrap().len(), 3);
    assert_eq!(
        report["file_coupling"][0]["co_changes"], 2,
        "pairs are co-change-descending: {report}"
    );

    let devs = report["developer_coupling"]
        .as_array()
        .expect("developer_coupling");
    assert_eq!(devs.len(), 1, "{report}");
    assert_eq!(devs[0]["developer1"], "alice");
    assert_eq!(devs[0]["developer2"], "bob");
    assert_eq!(devs[0]["shared_file_changes"], 3);
    assert_close(
        devs[0]["coupling_strength"].as_f64().unwrap(),
        3.0 / 3.5,
        "alice-bob",
    );

    assert_eq!(owned_files(&report), ["a", "b", "c"]);
    for (file, lines) in [("a", 4), ("b", 2), ("c", 2)] {
        let own = ownership(&report, file);
        assert_eq!(own["lines"], lines, "{file} lines: {report}");
        assert_eq!(own["contributors"], 2, "{file} contributors: {report}");
    }

    let agg = &report["aggregate"];
    assert_eq!(agg["total_files"], 3);
    assert_eq!(agg["total_developers"], 2);
    assert_eq!(agg["total_co_changes"], 4);
    assert_eq!(agg["highly_coupled_pairs"], 0);
    assert_close(
        agg["avg_coupling_strength"].as_f64().unwrap(),
        (0.8 + 0.4 + 0.5) / 3.0,
        "avg_coupling_strength is the mean of the pair strengths",
    );
}

/// A `git mv b d` carries b's history to d: one row `d` with b's co-changes
/// plus the rename commit, and no `b` row. A deleted file leaves the report.
#[test]
fn renames_merge_the_old_row_and_deleted_files_leave_the_report() {
    let test = linear_fixture();
    CommitBuilder::new(&test, "c5 rename b->d", T0 + 4 * DAY)
        .author(ALICE.0, ALICE.1)
        .rename("b", "d")
        .commit()
        .expect("c5");
    CommitBuilder::new(&test, "c6 delete c", T0 + 5 * DAY)
        .author(BOB.0, BOB.1)
        .delete("c")
        .commit()
        .expect("c6");
    let report = couples_json(&test, &[]);

    assert_eq!(
        owned_files(&report),
        ["a", "d"],
        "b merged into d, c deleted: {report}"
    );
    assert_eq!(report["aggregate"]["total_files"], 2, "{report}");

    // a: c1, c2, c4 = 3 changes; d: b's c1, c2 + the rename c5 = 3 changes;
    // a-d: c1, c2 = 2.
    let ad = pair(&report, "a", "d").expect("a-d");
    assert_eq!(ad["co_changes"], 2, "{report}");
    assert_close(ad["coupling_strength"].as_f64().unwrap(), 2.0 / 3.0, "a-d");
    assert_eq!(report["file_coupling"].as_array().unwrap().len(), 1);

    // Ownership follows the rename: alice (b in c1, rename c5) and bob (b in
    // c2) both own d; d has b's 2 lines at the head.
    let d = ownership(&report, "d");
    assert_eq!(d["contributors"], 2, "{report}");
    assert_eq!(d["lines"], 2, "{report}");

    // Developer matrix over the head files: alice {a:1, d:2}, bob {a:2, d:1}
    // share min(1,2) + min(2,1) = 2 out of 3 / 3 self changes.
    let devs = &report["developer_coupling"][0];
    assert_eq!(devs["shared_file_changes"], 2, "{report}");
    assert_close(
        devs["coupling_strength"].as_f64().unwrap(),
        2.0 / 3.0,
        "alice-bob after rename",
    );

    // The rename is reported on its commit's ndjson record.
    let ndjson = couples(&test, "ndjson", &[]);
    assert!(
        ndjson.contains(r#""Renames":[{"FromName":"b","ToName":"d"}]"#),
        "{ndjson}"
    );
}

/// A rename that also edits the file counts as one change of the new path.
#[test]
fn an_edited_rename_counts_the_modification_on_the_new_path() {
    let body: Vec<u8> = (1..=100)
        .map(|i| format!("line {i}\n"))
        .collect::<String>()
        .into_bytes();
    let test = init_repo().expect("init");
    commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c1",
        T0,
        &[("old.txt", body.as_slice()), ("peer.txt", b"p\n")],
    )
    .expect("c1");
    let mut edited = body.clone();
    edited.extend_from_slice(b"line 101\n");
    CommitBuilder::new(&test, "c2 mv+edit", T0 + DAY)
        .author(BOB.0, BOB.1)
        .delete("old.txt")
        .write("new.txt", &edited)
        .write("peer.txt", b"p\nq\n")
        .commit()
        .expect("c2");
    let report = couples_json(&test, &[]);

    assert_eq!(owned_files(&report), ["new.txt", "peer.txt"], "{report}");
    let pair = pair(&report, "new.txt", "peer.txt").expect("new-peer");
    assert_eq!(pair["co_changes"], 2, "c1 (as old.txt) and c2: {report}");
    assert_eq!(ownership(&report, "new.txt")["contributors"], 2, "{report}");
    assert_eq!(ownership(&report, "new.txt")["lines"], 101, "{report}");
}

/// Alice's main line plus Bob's side line merged into it:
/// `c1 alice {a}` ← `side bob {b,c}` ; `c3 alice {a}` ; `merge alice`.
fn merge_fixture() -> TestRepo {
    let test = init_repo().expect("init");
    let c1 = commit_files_as(&test, ALICE.0, ALICE.1, "c1", T0, &[("a", b"a1\n")]).expect("c1");
    let side = CommitBuilder::new(&test, "side", T0 + DAY)
        .author(BOB.0, BOB.1)
        .parent(c1)
        .detached()
        .write("b", b"b1\n")
        .write("c", b"c1\n")
        .commit()
        .expect("side");
    commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c3",
        T0 + 2 * DAY,
        &[("a", b"a1\na2\n")],
    )
    .expect("c3");
    CommitBuilder::new(&test, "merge", T0 + 3 * DAY)
        .author(ALICE.0, ALICE.1)
        .merge_parent(side)
        .write("b", b"b1\n")
        .write("c", b"c1\n")
        .commit()
        .expect("merge");
    test
}

/// A change merged in from a branch is counted once, at its own commit: the
/// merge neither re-couples b and c nor credits the merger with them. Under
/// `--first-parent` the side line is not walked, so the merge IS the one
/// commit that changes b and c.
#[test]
fn merged_in_changes_count_once() {
    let test = merge_fixture();
    let report = couples_json(&test, &[]);
    let bc = pair(&report, "b", "c").expect("b-c");
    assert_eq!(bc["co_changes"], 1, "{report}");
    assert_eq!(
        ownership(&report, "b")["contributors"],
        1,
        "bob only: {report}"
    );
    assert_eq!(
        ownership(&report, "c")["contributors"],
        1,
        "bob only: {report}"
    );
    assert_eq!(
        ownership(&report, "a")["contributors"],
        1,
        "alice only: {report}"
    );
    assert!(
        report["developer_coupling"].as_array().unwrap().is_empty(),
        "alice and bob changed disjoint files: {report}"
    );
    assert_eq!(report["aggregate"]["total_developers"], 2);

    let first_parent = couples_json(&test, &["--first-parent"]);
    let bc = pair(&first_parent, "b", "c").expect("b-c first-parent");
    assert_eq!(bc["co_changes"], 1, "{first_parent}");
    assert_eq!(
        first_parent["aggregate"]["total_developers"], 1,
        "{first_parent}"
    );
    assert_eq!(
        ownership(&first_parent, "b")["contributors"],
        1,
        "alice, via the merge: {first_parent}"
    );
}

/// A commit touching more files than the cap stays out of the coupling
/// matrix but still counts for ownership, keeps its real hash in ndjson and
/// lists its author files there.
#[test]
fn oversized_commits_keep_ownership_and_their_hash() {
    let test = init_repo().expect("init");
    commit_files_as(&test, ALICE.0, ALICE.1, "c1", T0, &[("x.txt", b"x\n")]).expect("c1");
    let mut big = CommitBuilder::new(&test, "c2 big", T0 + DAY).author(ALICE.0, ALICE.1);
    let names: Vec<String> = (1..=CAP + 1).map(|i| format!("f{i}.txt")).collect();
    for name in &names {
        big = big.write(name, b"f\n");
    }
    let big_hash = big.commit().expect("c2").to_string();
    commit_files_as(
        &test,
        BOB.0,
        BOB.1,
        "c3",
        T0 + 2 * DAY,
        &[("x.txt", b"x\ny\n"), ("f1.txt", b"f\nmod\n")],
    )
    .expect("c3");
    let report = couples_json(&test, &[]);

    // Coupling: only c3's pair.
    assert_eq!(
        report["file_coupling"].as_array().unwrap().len(),
        1,
        "{report}"
    );
    assert_eq!(pair(&report, "f1.txt", "x.txt").unwrap()["co_changes"], 1);
    // Ownership: every file at the head, with the oversized commit's author.
    assert_eq!(
        report["aggregate"]["total_files"].as_u64().unwrap() as usize,
        CAP + 2,
        "{}",
        report["aggregate"]
    );
    assert_eq!(ownership(&report, "f2.txt")["contributors"], 1);
    assert_eq!(ownership(&report, "f1.txt")["contributors"], 2);
    assert_eq!(ownership(&report, "x.txt")["contributors"], 2);

    let ndjson = couples(&test, "ndjson", &[]);
    let line = ndjson
        .lines()
        .find(|l| l.contains(&big_hash))
        .unwrap_or_else(|| panic!("no record with the real hash of the big commit:\n{ndjson}"));
    assert!(line.contains(r#""CouplingFiles":[]"#), "{line}");
    let record: serde_json::Value = serde_json::from_str(line).expect("ndjson line");
    assert_eq!(
        record["data"]["AuthorFiles"].as_object().unwrap().len(),
        CAP + 1,
        "{line}"
    );
    assert!(
        !ndjson.contains("0000000000000000000000000000000000000000"),
        "{ndjson}"
    );
}

/// Every timeseries commit carries its own tick (daily commits ⇒ 0, 1, 2, 3)
/// and the number of files its author touched.
#[test]
fn timeseries_reports_the_commit_tick() {
    let test = linear_fixture();
    let text = couples(&test, "timeseries", &[]);
    let doc: serde_json::Value = serde_json::from_str(&text).expect("timeseries json");
    let ticks: Vec<i64> = doc["commits"]
        .as_array()
        .expect("commits")
        .iter()
        .map(|c| c["tick"].as_i64().expect("tick"))
        .collect();
    assert_eq!(ticks, vec![0, 1, 2, 3], "{text}");
    let touched: Vec<i64> = doc["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            c["couples"]["files_touched"]
                .as_i64()
                .expect("files_touched")
        })
        .collect();
    assert_eq!(touched, vec![2, 3, 1, 1], "{text}");
}

/// Two runs are byte-identical; json and yaml carry the same numbers.
#[test]
fn output_is_deterministic_and_formats_agree() {
    let test = linear_fixture();
    let first = couples(&test, "json", &[]);
    let second = couples(&test, "json", &[]);
    assert_eq!(first, second);

    let yaml = couples(&test, "yaml", &[]);
    for line in [
        "co_changes: 2",
        "shared_file_changes: 3",
        "total_files: 3",
        "total_co_changes: 4",
        "lines: 4",
    ] {
        assert!(yaml.contains(line), "yaml lacks `{line}`:\n{yaml}");
    }
    let text = couples(&test, "text", &[]);
    assert!(text.contains("Total Co-Changes       4"), "{text}");
}
