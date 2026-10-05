//! End-to-end coverage of `history/devs` through the **real binary** over
//! on-disk git fixtures (`specs/bugs/BUG-history-devs-stats.md`).
//!
//! Every number is computed by hand from the fixture: commits and lines
//! added / removed / changed per developer with the per-language breakdown,
//! the bus factor, empty blobs, empty commits, the merged timeseries, and
//! determinism.

mod common;

use cf_gitlib::testutil::{commit_files, init_repo, CommitBuilder, TestRepo};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// 2021-01-01T12:00:00Z, the fixture's first commit.
const T0: i64 = 1_609_502_400;
/// One day.
const DAY: i64 = 24 * 3_600;
/// Days between the fixture's main-line commits.
const SPACING_DAYS: i64 = 31;
/// Fixture developers.
const ALICE: (&str, &str) = ("Alice", "alice@example.com");
const BOB: (&str, &str) = ("Bob", "bob@example.com");
const CAROL: (&str, &str) = ("Carol", "carol@example.com");

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

fn stdout_ok(args: &[&str]) -> String {
    let out = codefang(args);
    assert!(
        out.status.success(),
        "run failed ({args:?}): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

fn json_ok(args: &[&str]) -> serde_json::Value {
    let text = stdout_ok(args);
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}):\n{text}"))
}

fn devs_json(repo: &str, extra: &[&str]) -> serde_json::Value {
    json_ok(&[&["-a", "history/devs", "--format", "json"], extra, &[repo]].concat())
}

fn path(test: &TestRepo) -> &str {
    test.path().to_str().expect("utf-8 path")
}

fn lines(items: &[&str]) -> Vec<u8> {
    let mut s = items.join("\n");
    s.push('\n');
    s.into_bytes()
}

fn day(n: i64) -> i64 {
    T0 + n * SPACING_DAYS * DAY
}

/// The same fixture A as `burndown_e2e.rs` (see its table). The devs walk is
/// NOT first-parent, so Carol's side-line commit `cb1` is visited too.
fn fixture_a() -> TestRepo {
    let test = init_repo().expect("init");
    let a10 = lines(&["l1", "l2", "l3", "l4", "l5", "l6", "l7", "l8", "l9", "l10"]);
    let a12 = lines(&[
        "l1", "l3", "l4", "l6", "l7", "l9", "l10", "n1", "n2", "n3", "n4", "n5",
    ]);
    let commit = |msg: &str, n: i64, who: (&str, &str)| {
        CommitBuilder::new(&test, msg, day(n)).author(who.0, who.1)
    };
    commit("c1", 0, ALICE)
        .write("a.go", &a10)
        .commit()
        .expect("c1");
    commit("c2", 1, BOB)
        .write("a.go", &a12)
        .commit()
        .expect("c2");
    commit("c3", 2, ALICE)
        .rename("a.go", "b.go")
        .commit()
        .expect("c3");
    commit("c4", 3, ALICE)
        .write("img.bin", b"BIN\x00\x01\x02DATA\n")
        .write("script.py", &lines(&["p1", "p2", "p3", "p4"]))
        .commit()
        .expect("c4");
    let c5 = commit("c5", 4, BOB)
        .write("script.py", &lines(&["p1", "P2", "p3", "p4", "p5", "p6"]))
        .commit()
        .expect("c5");
    commit("c6", 5, BOB)
        .write(
            "script.py",
            &lines(&["p1", "P2", "p3", "p4", "p5", "p6", "p7"]),
        )
        .commit()
        .expect("c6");
    let feat6 = lines(&["f1", "f2", "f3", "f4", "f5", "f6"]);
    let cb1 = CommitBuilder::new(&test, "cb1", day(5) + 6 * 3_600)
        .author(CAROL.0, CAROL.1)
        .parent(c5)
        .detached()
        .write("feat.go", &feat6)
        .delete("b.go")
        .commit()
        .expect("cb1");
    commit("merge", 6, ALICE)
        .merge_parent(cb1)
        .write("feat.go", &feat6)
        .delete("b.go")
        .commit()
        .expect("merge");
    commit("c7", 7, ALICE)
        .write("vendor/lib/x.go", &lines(&["v1", "v2", "v3", "v4", "v5"]))
        .commit()
        .expect("c7");
    commit("c8", 8, ("Alice Smith", "alice@example.com"))
        .write(
            "feat.go",
            &lines(&["f1", "f2", "f3", "f4", "f5", "f6", "f7"]),
        )
        .commit()
        .expect("c8");
    commit("c9", 9, ALICE).commit().expect("c9");
    commit("c10", 10, ("Alice Smith", "asmith@other.com"))
        .write(
            "script.py",
            &lines(&["p1", "P2", "p3", "p4", "p5", "p6", "p7", "p8"]),
        )
        .commit()
        .expect("c10");
    test
}

/// `(name, commits, added, removed, changed, [(language, added, removed, changed)])`.
type DevRow = (String, i64, i64, i64, i64, Vec<(String, i64, i64, i64)>);

fn developer_rows(report: &serde_json::Value) -> Vec<DevRow> {
    report["developers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            let langs = d["languages"]
                .as_array()
                .map(|ls| {
                    ls.iter()
                        .map(|l| {
                            (
                                l["language"].as_str().unwrap().to_string(),
                                l["added"].as_i64().unwrap(),
                                l["removed"].as_i64().unwrap(),
                                l["changed"].as_i64().unwrap(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            (
                d["name"].as_str().unwrap().to_string(),
                d["commits"].as_i64().unwrap(),
                d["lines_added"].as_i64().unwrap(),
                d["lines_removed"].as_i64().unwrap(),
                d["lines_changed"].as_i64().unwrap(),
                langs,
            )
        })
        .collect()
}

#[test]
fn per_developer_stats_match_the_hand_computed_fixture() {
    // The loose identity detector lowercases names.
    let test = fixture_a();
    let report = devs_json(path(&test), &[]);
    let agg = &report["aggregate"];
    // c7 (vendored only) and c9 (empty) are not counted by default.
    assert_eq!(agg["total_commits"], 10, "{agg}");
    assert_eq!(agg["total_developers"], 3);
    assert_eq!(agg["total_lines_added"], 30);
    assert_eq!(agg["total_lines_removed"], 15);
    assert_eq!(agg["analysis_period_ticks"], 310);
    assert_eq!(agg["total_languages"], 2);
    assert_eq!(agg["estimated_total_developers"], 3);

    let rows = developer_rows(&report);
    assert_eq!(
        rows,
        vec![
            (
                "alice".to_string(),
                6,
                16,
                0,
                0,
                vec![
                    ("Go".to_string(), 11, 0, 0),
                    ("Python".to_string(), 5, 0, 0),
                ],
            ),
            (
                "bob".to_string(),
                3,
                8,
                3,
                1,
                vec![("Go".to_string(), 5, 3, 0), ("Python".to_string(), 3, 0, 1),],
            ),
            (
                "carol".to_string(),
                1,
                6,
                12,
                0,
                vec![("Go".to_string(), 6, 12, 0)],
            ),
        ]
    );
    let alice = &report["developers"][0];
    assert_eq!(alice["email"], "alice@example.com");
    assert_eq!(alice["net_lines"], 16);
    assert_eq!(alice["first_tick"], 0);
    assert_eq!(alice["last_tick"], 310);
    assert_eq!(alice["active_ticks"], 6);
}

#[test]
fn bus_factor_emits_a_secondary_developer_with_id_zero() {
    let test = fixture_a();
    let report = devs_json(path(&test), &[]);
    let go = report["busfactor"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["language"] == "Go")
        .expect("Go bus factor");
    // Go contributions (added + removed): Carol 18, Alice 11, Bob 8 of 37.
    assert_eq!(go["bus_factor"], 2, "{go}");
    assert_eq!(go["total_contributors"], 3);
    assert_eq!(go["primary_dev_id"], 2);
    assert_eq!(go["primary_dev_name"], "carol");
    assert_eq!(
        go["secondary_dev_id"], 0,
        "Alice is id 0 and must be present"
    );
    assert_eq!(go["secondary_dev_name"], "alice");
    assert_eq!(go["risk_level"], "LOW");
    let py = report["busfactor"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["language"] == "Python")
        .expect("Python bus factor");
    // Python: Alice 5, Bob 3 of 8 → 62.5 %.
    assert_eq!(py["bus_factor"], 1);
    assert_eq!(py["primary_dev_id"], 0);
    assert_eq!(py["secondary_dev_id"], 1);
    assert_eq!(py["primary_percentage"], 62.5);
    assert_eq!(py["risk_level"], "MEDIUM");
    assert_eq!(report["aggregate"]["project_bus_factor"], 2);
}

#[test]
fn empty_commits_flag_counts_commits_without_changes() {
    let test = fixture_a();
    let p = path(&test);
    let report = devs_json(p, &["--empty-commits"]);
    assert_eq!(report["aggregate"]["total_commits"], 12, "{report}");
    let alice = developer_rows(&report)
        .into_iter()
        .find(|r| r.0 == "alice")
        .unwrap();
    assert_eq!(alice.1, 8, "c7 (vendored only) and c9 (empty) count");
    assert_eq!((alice.2, alice.3), (16, 0), "lines are unchanged");
    let ndjson = stdout_ok(&[
        "-a",
        "history/devs",
        "--format",
        "ndjson",
        "--empty-commits",
        p,
    ]);
    assert_eq!(ndjson.lines().count(), 12);
}

#[test]
fn zero_byte_blobs_are_ordinary_empty_text() {
    let test = init_repo().expect("init");
    commit_files(&test, "c1", T0, &[("empty.txt", b""), ("a.txt", b"a1\n")]).expect("c1");
    commit_files(&test, "c2", T0 + DAY, &[("empty.txt", b"e1\ne2\n")]).expect("c2");
    commit_files(&test, "c3", T0 + 2 * DAY, &[("empty.txt", b"")]).expect("c3");
    let report = devs_json(path(&test), &[]);
    let dev = &report["developers"][0];
    assert_eq!(dev["commits"], 3, "{report}");
    assert_eq!(dev["lines_added"], 3, "a.txt 1 + empty.txt 0→2");
    assert_eq!(dev["lines_removed"], 2, "empty.txt 2→0");
    assert_eq!(dev["lines_changed"], 0);
}

#[test]
fn timeseries_lists_the_commits_in_tick_order() {
    let test = fixture_a();
    let p = path(&test);
    let ts = json_ok(&["-a", "history/devs", "--format", "timeseries", p]);
    assert_eq!(ts["analyzers"], serde_json::json!(["devs"]));
    let commits = ts["commits"].as_array().expect("commits");
    assert_eq!(commits.len(), 10, "{ts}");
    let ticks: Vec<i64> = commits
        .iter()
        .map(|c| c["tick"].as_i64().unwrap())
        .collect();
    assert_eq!(ticks, vec![0, 31, 62, 93, 124, 155, 155, 186, 248, 310]);
    assert_eq!(commits[1]["devs"]["lines_removed"], 3);
    assert_eq!(commits[1]["timestamp"], "2021-02-01T12:00:00Z");
    assert_eq!(commits[6]["devs"]["author_id"], 2, "cb1 is Carol's");

    let ndjson = stdout_ok(&[
        "-a",
        "history/devs",
        "--format",
        "timeseries",
        "--ndjson",
        p,
    ]);
    assert_eq!(ndjson.lines().count(), 10, "{ndjson}");
}

#[test]
fn json_and_yaml_agree_and_runs_are_deterministic() {
    let test = fixture_a();
    let p = path(&test);
    let a = stdout_ok(&["-a", "history/devs", "--format", "json", p]);
    let b = stdout_ok(&["-a", "history/devs", "--format", "json", p]);
    assert_eq!(a, b, "two runs must be byte-identical");
    let yaml = stdout_ok(&["-a", "history/devs", "--format", "yaml", p]);
    assert!(yaml.contains("    total_commits: 10"), "{yaml}");
    assert!(yaml.contains("name: alice"), "{yaml}");
    assert!(yaml.contains("      secondary_dev_id: 0"), "{yaml}");
    assert!(yaml.contains("    estimated_total_developers: 3"), "{yaml}");
}

/// Commits dropped by `--max-changes-per-commit` must not shift devs' ndjson
/// records: every analyzer keys its records on the commit's window position,
/// so a multi-analyzer ndjson stream stays in commit order.
#[test]
fn capped_commits_keep_the_merged_ndjson_in_commit_order() {
    let test = init_repo().expect("init fixture repo");
    let mut hashes = Vec::new();
    // c0 touches three files (dropped by a cap of 2); c1..c3 touch one each.
    hashes.push(
        commit_files(
            &test,
            "c0",
            T0,
            &[("a.txt", b"a\n"), ("b.txt", b"b\n"), ("c.txt", b"c\n")],
        )
        .expect("c0"),
    );
    for i in 1..4_i64 {
        let body = format!("line {i}\n");
        hashes.push(
            commit_files(
                &test,
                &format!("c{i}"),
                T0 + i * DAY,
                &[("a.txt", body.as_bytes())],
            )
            .expect("commit"),
        );
    }
    let order: Vec<String> = hashes.iter().map(ToString::to_string).collect();
    let out = stdout_ok(&[
        "-a",
        "history/devs,history/burndown",
        "--format",
        "ndjson",
        "--max-changes-per-commit",
        "2",
        path(&test),
    ]);
    let positions: Vec<usize> = out
        .lines()
        .map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).expect("ndjson line");
            let hash = v["hash"].as_str().expect("hash").to_string();
            order.iter().position(|h| *h == hash).expect("known hash")
        })
        .collect();
    assert!(
        !positions.contains(&0),
        "the capped commit must be dropped: {out}"
    );
    assert!(
        positions.windows(2).all(|w| w[0] <= w[1]),
        "records out of commit order {positions:?}:\n{out}"
    );
}
