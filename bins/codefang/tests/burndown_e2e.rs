//! End-to-end coverage of `history/burndown` through the **real binary** over
//! on-disk git fixtures (`specs/bugs/BUG-history-burndown-survival.md`).
//!
//! Every number below is computed by hand from the fixture
//! (`site/explanation/burndown.md` defines the figures): lines written in a
//! band that still exist at each later sample, survival ≤ 100 %, gross lines
//! added / removed per commit, the merge on the first-parent line, the
//! window base, the two tracking flags, the age bands, the 16383-tick
//! history that used to panic.

mod common;

use cf_gitlib::testutil::{commit_files, init_repo, CommitBuilder, TestRepo};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// 2021-01-01T12:00:00Z, the fixture's first commit.
const T0: i64 = 1_609_502_400;
/// One day.
const DAY: i64 = 24 * 3_600;
/// Days between the fixture's main-line commits: every commit lands in its
/// own band of the default 30-tick grid.
const SPACING_DAYS: i64 = 31;
/// Fixture developers.
const ALICE: (&str, &str) = ("Alice", "alice@example.com");
const BOB: (&str, &str) = ("Bob", "bob@example.com");
const CAROL: (&str, &str) = ("Carol", "carol@example.com");
/// The tick that collided with the former merge-mark sentinel.
const FORMER_MERGE_MARK_TICK: i64 = 16_383;
/// The default sampling grid.
const DEFAULT_GRID: i64 = 30;

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

/// The burndown JSON report of `extra` flags over `repo`.
fn burndown_json(repo: &str, extra: &[&str]) -> serde_json::Value {
    json_ok(
        &[
            &["-a", "history/burndown", "--format", "json"],
            extra,
            &[repo],
        ]
        .concat(),
    )
}

/// The per-commit `(tick, LinesAdded, LinesRemoved, data)` of an ndjson run.
fn burndown_ndjson(repo: &str, extra: &[&str]) -> Vec<serde_json::Value> {
    let text = stdout_ok(
        &[
            &["-a", "history/burndown", "--format", "ndjson"],
            extra,
            &[repo],
        ]
        .concat(),
    );
    text.lines()
        .map(|l| serde_json::from_str(l).expect("ndjson line"))
        .collect()
}

fn path(test: &TestRepo) -> &str {
    test.path().to_str().expect("utf-8 path")
}

fn lines(items: &[&str]) -> Vec<u8> {
    let mut s = items.join("\n");
    s.push('\n');
    s.into_bytes()
}

/// The committer time of main-line commit `n` (day `n * SPACING_DAYS`).
fn day(n: i64) -> i64 {
    T0 + n * SPACING_DAYS * DAY
}

/// Fixture A — ten first-parent commits plus one side-line commit merged in:
///
/// | commit  | tick | author      | change                                   |
/// |---------|------|-------------|------------------------------------------|
/// | c1      |   0  | Alice       | a.go 10 lines                            |
/// | c2      |  31  | Bob         | a.go: delete l2,l5,l8; append n1..n5     |
/// | c3      |  62  | Alice       | pure rename a.go → b.go                  |
/// | c4      |  93  | Alice       | img.bin (binary), script.py 4 lines      |
/// | c5      | 124  | Bob         | script.py: p2→P2, append p5,p6           |
/// | c6      | 155  | Bob         | script.py: append p7                     |
/// | cb1     | 155  | Carol       | side line off c5: feat.go 6, delete b.go |
/// | merge   | 186  | Alice       | merge cb1 (first-parent diff: +feat.go, −b.go) |
/// | c7      | 217  | Alice       | vendor/lib/x.go (vendored, filtered out) |
/// | c8      | 248  | Alice Smith <alice@example.com> | feat.go +1           |
/// | c9      | 279  | Alice       | empty commit                             |
/// | c10     | 310  | Alice Smith <asmith@other.com>  | script.py +1         |
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

/// The dense global matrix of fixture A on the default grid, by hand:
/// bands are the 11 commit ticks / 30; sample `i` is the state after tick
/// `30·i + 29`.
fn fixture_a_global_survival() -> Vec<(i64, Vec<i64>)> {
    vec![
        (10, vec![10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        (12, vec![7, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        (12, vec![7, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        (16, vec![7, 5, 0, 4, 0, 0, 0, 0, 0, 0, 0]),
        (18, vec![7, 5, 0, 3, 3, 0, 0, 0, 0, 0, 0]),
        (19, vec![7, 5, 0, 3, 3, 1, 0, 0, 0, 0, 0]),
        (13, vec![0, 0, 0, 3, 3, 1, 6, 0, 0, 0, 0]),
        (13, vec![0, 0, 0, 3, 3, 1, 6, 0, 0, 0, 0]),
        (14, vec![0, 0, 0, 3, 3, 1, 6, 0, 1, 0, 0]),
        (14, vec![0, 0, 0, 3, 3, 1, 6, 0, 1, 0, 0]),
        (15, vec![0, 0, 0, 3, 3, 1, 6, 0, 1, 0, 1]),
    ]
}

/// Lines ever written that lived to a sample: 10 + 5 + 4 + 3 + 1 + 6 + 1 + 1.
const FIXTURE_A_PEAK: i64 = 31;
/// Lines alive at the end: script.py 8 + feat.go 7.
const FIXTURE_A_CURRENT: i64 = 15;

#[test]
fn global_survival_follows_the_hand_computed_matrix() {
    let test = fixture_a();
    let report = burndown_json(path(&test), &[]);
    let agg = &report["aggregate"];
    assert_eq!(agg["total_current_lines"], FIXTURE_A_CURRENT, "{report}");
    assert_eq!(agg["total_peak_lines"], FIXTURE_A_PEAK, "{report}");
    assert_eq!(
        agg["analysis_period_days"], 310,
        "the tick span, not the grid"
    );
    assert_eq!(agg["num_samples"], 11);
    assert_eq!(agg["num_bands"], 11);
    assert_eq!(agg["tracked_files"], 0);
    assert_eq!(agg["tracked_developers"], 0);
    let rate = agg["overall_survival_rate"].as_f64().unwrap();
    assert!((rate - 15.0 / 31.0).abs() < 1e-12, "{rate}");

    let samples = report["global_survival"].as_array().expect("samples");
    let want = fixture_a_global_survival();
    assert_eq!(samples.len(), want.len(), "{report}");
    for (i, (total, bands)) in want.iter().enumerate() {
        let s = &samples[i];
        assert_eq!(s["sample_index"], i as i64);
        assert_eq!(s["total_lines"], *total, "sample {i}: {s}");
        let got: Vec<i64> = s["band_breakdown"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_i64().unwrap())
            .collect();
        assert_eq!(got, *bands, "sample {i}");
        let r = s["survival_rate"].as_f64().unwrap();
        assert!(r <= 1.0 && (r - *total as f64 / FIXTURE_A_PEAK as f64).abs() < 1e-12);
    }
    assert_eq!(report["file_survival"], serde_json::json!([]));
    assert_eq!(report["developer_survival"], serde_json::json!([]));
    assert_eq!(report["interactions"], serde_json::json!([]));
}

#[test]
fn ndjson_reports_gross_lines_per_commit_including_the_merge() {
    let test = fixture_a();
    let records = burndown_ndjson(path(&test), &[]);
    let got: Vec<(i64, i64, i64)> = records
        .iter()
        .map(|r| {
            (
                r["tick"].as_i64().unwrap(),
                r["data"]["LinesAdded"].as_i64().unwrap(),
                r["data"]["LinesRemoved"].as_i64().unwrap(),
            )
        })
        .collect();
    // (tick, added, removed) per first-parent commit; the merge deletes the
    // 12 lines of b.go on the main line and adds feat.go's 6.
    assert_eq!(
        got,
        vec![
            (0, 10, 0),
            (31, 5, 3),
            (62, 0, 0),
            (93, 4, 0),
            (124, 3, 1),
            (155, 1, 0),
            (186, 6, 12),
            (217, 0, 0),
            (248, 1, 0),
            (279, 0, 0),
            (310, 1, 0),
        ]
    );
    let merge = &records[6]["data"];
    assert_eq!(
        merge["GlobalDeltas"],
        serde_json::json!({"186": {"0": -7, "31": -5, "186": 6}}),
        "the merge's deletions are debited at its own tick"
    );
    assert_eq!(merge["FileDeltas"], serde_json::Value::Null);
    assert_eq!(merge["PeopleDeltas"], serde_json::Value::Null);
    assert_eq!(merge["MatrixDeltas"], serde_json::Value::Null);
    assert_eq!(records[1]["author_id"], 1, "Bob");
    assert_eq!(records[1]["timestamp"], "2021-02-01T12:00:00Z");
}

#[test]
fn head_equals_limit_one_and_a_window_counts_only_in_window_changes() {
    let test = fixture_a();
    let p = path(&test);
    let head = stdout_ok(&["-a", "history/burndown", "--format", "json", "--head", p]);
    let limit_one = stdout_ok(&[
        "-a",
        "history/burndown",
        "--format",
        "json",
        "--limit",
        "1",
        p,
    ]);
    assert_eq!(head, limit_one, "--head ≡ --limit 1");

    // Window c8, c9, c10: the base tree (c7's) seeds script.py 7 + feat.go 6
    // at tick 0; c8 adds ONE line, not the whole 7-line file.
    let records = burndown_ndjson(p, &["--limit", "3"]);
    let got: Vec<(i64, i64, i64)> = records
        .iter()
        .map(|r| {
            (
                r["tick"].as_i64().unwrap(),
                r["data"]["LinesAdded"].as_i64().unwrap(),
                r["data"]["LinesRemoved"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(got, vec![(0, 1, 0), (31, 0, 0), (62, 1, 0)]);

    let report = burndown_json(p, &["--limit", "3"]);
    assert_eq!(report["aggregate"]["total_current_lines"], 15);
    assert_eq!(report["aggregate"]["total_peak_lines"], 15);
    assert_eq!(report["aggregate"]["analysis_period_days"], 62);
    let last = &report["global_survival"][2];
    assert_eq!(last["band_breakdown"], serde_json::json!([14, 0, 1]));

    // devs agrees on c8: one line added.
    let devs = stdout_ok(&[
        "-a",
        "history/devs",
        "--format",
        "ndjson",
        "--limit",
        "3",
        p,
    ]);
    let c8: serde_json::Value = serde_json::from_str(devs.lines().next().unwrap()).unwrap();
    assert_eq!(c8["data"]["lines_added"], 1, "{devs}");
}

#[test]
fn timeseries_stamps_every_commit_with_its_own_tick() {
    let test = fixture_a();
    let ts = json_ok(&[
        "-a",
        "history/burndown",
        "--format",
        "timeseries",
        path(&test),
    ]);
    let ticks: Vec<i64> = ts["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["tick"].as_i64().unwrap())
        .collect();
    assert_eq!(
        ticks,
        vec![0, 31, 62, 93, 124, 155, 186, 217, 248, 279, 310],
        "{ts}"
    );
    assert_eq!(ts["commits"][6]["burndown"]["lines_removed"], 12);
}

#[test]
fn granularity_and_sampling_shape_the_grid() {
    let test = fixture_a();
    let p = path(&test);
    let report = burndown_json(p, &["--granularity", "100", "--sampling", "50"]);
    let agg = &report["aggregate"];
    assert_eq!(agg["num_bands"], 4, "310 / 100 + 1: {agg}");
    assert_eq!(agg["num_samples"], 7, "310 / 50 + 1: {agg}");
    assert_eq!(agg["analysis_period_days"], 310);
    assert_eq!(agg["total_current_lines"], FIXTURE_A_CURRENT);
    // Final state by creation tick: 93 → 3 lines; 124, 155, 186 → 3 + 1 + 6;
    // 248 → 1; 310 → 1, grouped into bands of 100 ticks.
    let last = &report["global_survival"][6]["band_breakdown"];
    assert_eq!(last, &serde_json::json!([3, 10, 1, 1]));

    // The defaults (30 / 30) give one band and one sample per 31-day tick
    // step; a non-positive value is rejected (BUG-cli-inert-surface item 4).
    let dflt = burndown_json(p, &[]);
    assert_eq!(dflt["aggregate"]["num_bands"], 11);
    assert_eq!(dflt["aggregate"]["num_samples"], 11);
    for flag in ["--granularity", "--sampling"] {
        let out = codefang(&["-a", "history/burndown", flag, "0", p]);
        assert_eq!(out.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&out.stderr)
                .contains(&format!("Error: invalid {flag}: must be > 0, got 0")),
            "{flag}"
        );
    }
}

#[test]
fn text_age_bands_read_newest_to_oldest() {
    let test = fixture_a();
    let text = stdout_ok(&["-a", "history/burndown", "--format", "text", path(&test)]);
    let age: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("month"))
        .map(str::trim)
        .collect();
    // Last sample: band 10 (1 line, 1 month old), band 8 (1 line, 3 months),
    // bands 5+6 (1 + 6 lines, 5–6 months), bands 3+4 (3 + 3 lines, 7–8 months).
    assert_eq!(age.len(), 4, "{text}");
    assert!(
        age[0].starts_with("< 1 month") && age[0].ends_with("(1)"),
        "{text}"
    );
    assert!(
        age[1].starts_with("1-3 months") && age[1].ends_with("(1)"),
        "{text}"
    );
    assert!(
        age[2].starts_with("3-6 months") && age[2].ends_with("(7)"),
        "{text}"
    );
    assert!(
        age[3].starts_with("6-12 months") && age[3].ends_with("(6)"),
        "{text}"
    );
    assert!(text.contains("Burndown: "), "{text}");
    assert!(text.contains("310d"), "{text}");
    assert!(
        !text.contains("Top Developers"),
        "no people tracking: {text}"
    );
}

#[test]
fn per_file_survival_follows_renames_and_binaries_are_skipped() {
    let test = fixture_a();
    let report = burndown_json(path(&test), &["--burndown-files"]);
    assert_eq!(report["aggregate"]["tracked_files"], 3, "{report}");
    let files: Vec<(String, i64, i64)> = report["file_survival"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap().to_string(),
                f["current_lines"].as_i64().unwrap(),
                f["peak_lines"].as_i64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        files,
        vec![
            ("script.py".to_string(), 8, 9),
            ("feat.go".to_string(), 7, 7),
            ("b.go".to_string(), 0, 15),
        ]
    );
    assert!(report["file_survival"][0].get("ownership").is_none());
    // The global figures are unchanged by the flag.
    assert_eq!(
        report["aggregate"]["total_current_lines"],
        FIXTURE_A_CURRENT
    );

    // Per-commit file deltas are keyed by the file's final path.
    let records = burndown_ndjson(path(&test), &["--burndown-files"]);
    assert_eq!(
        records[1]["data"]["FileDeltas"],
        serde_json::json!({"b.go": {"31": {"0": -3, "31": 5}}})
    );
}

#[test]
fn per_developer_survival_and_interactions_follow_the_shared_identities() {
    let test = fixture_a();
    let report = burndown_json(path(&test), &["--burndown-people", "--burndown-files"]);
    assert_eq!(report["aggregate"]["tracked_developers"], 2, "{report}");
    let devs: Vec<(i64, String, i64, i64)> = report["developer_survival"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["id"].as_i64().unwrap(),
                d["name"].as_str().unwrap().to_string(),
                d["current_lines"].as_i64().unwrap(),
                d["peak_lines"].as_i64().unwrap(),
            )
        })
        .collect();
    // The loose identity detector lowercases names.
    // alice: 10 (c1) − 3 (c2) − 7 (merge) + 4 + 6 + 1 + 1 − 1 (c5) = 11 live, 22 peak.
    // bob: 5 (c2, gone at the merge) + 3 + 1 = 4 live, 9 peak.
    assert_eq!(
        devs,
        vec![
            (0, "alice".to_string(), 11, 22),
            (1, "bob".to_string(), 4, 9)
        ]
    );
    let inter: Vec<(String, String, i64, bool)> = report["interactions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["author_name"].as_str().unwrap().to_string(),
                i["modifier_name"].as_str().unwrap().to_string(),
                i["lines_modified"].as_i64().unwrap(),
                i["is_self_modify"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        inter,
        vec![
            ("alice".to_string(), "alice".to_string(), 7, true),
            ("bob".to_string(), "alice".to_string(), 5, false),
            ("alice".to_string(), "bob".to_string(), 4, false),
        ]
    );
    // Ownership of the live files.
    let script = report["file_survival"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "script.py")
        .expect("script.py");
    assert_eq!(script["ownership"], serde_json::json!({"0": 4, "1": 4}));
    assert_eq!(script["top_owner_id"], 0, "tie → smaller id");
    assert_eq!(script["top_owner_name"], "alice");
    assert_eq!(script["top_owner_percentage"], 50.0);
    let dead = report["file_survival"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "b.go")
        .expect("b.go");
    assert_eq!(dead["ownership"], serde_json::json!({}));
    assert!(dead.get("top_owner_id").is_none());

    // The text report lists the developers.
    let text = stdout_ok(&[
        "-a",
        "history/burndown",
        "--format",
        "text",
        "--burndown-people",
        path(&test),
    ]);
    assert!(
        text.contains("Top Developers (by surviving lines)"),
        "{text}"
    );
    let alice = text
        .lines()
        .find(|l| l.trim_start().starts_with("alice"))
        .unwrap();
    assert!(alice.contains("11") && alice.contains("50.0%"), "{alice}");

    // Per-commit people and matrix deltas.
    let records = burndown_ndjson(path(&test), &["--burndown-people"]);
    assert_eq!(
        records[1]["data"]["PeopleDeltas"],
        serde_json::json!({"0": {"31": {"0": -3}}, "1": {"31": {"31": 5}}})
    );
    assert_eq!(
        records[1]["data"]["MatrixDeltas"],
        serde_json::json!({"0": {"1": 3}})
    );
}

#[test]
fn a_history_spanning_the_former_merge_mark_tick_runs() {
    // 1981-01-01T12:00:00Z; c2 lands exactly 16383 days later, c3 one
    // sampling period after and deletes c2's lines (so the peak sees them).
    const START_1981: i64 = 347_155_200;
    const C3_TICK: i64 = FORMER_MERGE_MARK_TICK + DEFAULT_GRID;
    let test = init_repo().expect("init");
    commit_files(&test, "c1", START_1981, &[("f.txt", b"a\nb\n")]).expect("c1");
    commit_files(
        &test,
        "c2",
        START_1981 + FORMER_MERGE_MARK_TICK * DAY,
        &[("f.txt", b"a\nb\nc\nd\n")],
    )
    .expect("c2");
    commit_files(
        &test,
        "c3",
        START_1981 + C3_TICK * DAY,
        &[("f.txt", b"a\nb\n")],
    )
    .expect("c3");
    let p = path(&test);

    for extra in [&[][..], &["--burndown-people", "--burndown-files"][..]] {
        let report = burndown_json(p, extra);
        let agg = &report["aggregate"];
        assert_eq!(agg["total_current_lines"], 2, "{report}");
        assert_eq!(agg["total_peak_lines"], 4, "{report}");
        assert_eq!(agg["num_samples"], C3_TICK / DEFAULT_GRID + 1);
        assert_eq!(agg["analysis_period_days"], C3_TICK);
    }
    let records = burndown_ndjson(p, &[]);
    assert_eq!(records[1]["tick"], FORMER_MERGE_MARK_TICK);
    assert_eq!(records[1]["data"]["LinesAdded"], 2);
    assert_eq!(
        records[2]["data"]["GlobalDeltas"],
        serde_json::json!({"16413": {"16383": -2}})
    );
}

#[test]
fn oversized_commits_are_dropped_like_devs_does() {
    let test = fixture_a();
    let p = path(&test);
    // c4 (img.bin + script.py) and the merge (feat.go + b.go) have two raw
    // changes; with the cap at 1 both are dropped: c4's 4 lines never
    // appear, and c5 re-inserts script.py whole at its own tick.
    let records = burndown_ndjson(p, &["--max-changes-per-commit", "1"]);
    let ticks: Vec<i64> = records
        .iter()
        .map(|r| r["tick"].as_i64().unwrap())
        .collect();
    assert_eq!(
        ticks,
        vec![0, 31, 62, 124, 155, 217, 248, 279, 310],
        "{records:?}"
    );
    assert_eq!(
        records[3]["data"]["LinesAdded"], 6,
        "c5 re-inserts script.py whole"
    );
    let devs = stdout_ok(&[
        "-a",
        "history/devs",
        "--format",
        "ndjson",
        "--max-changes-per-commit",
        "1",
        p,
    ]);
    assert_eq!(
        devs.lines().count(),
        7,
        "devs keeps the same commits minus the empty ones: {devs}"
    );
}

#[test]
fn json_and_yaml_agree_and_runs_are_deterministic() {
    let test = fixture_a();
    let p = path(&test);
    let flags = ["--burndown-people", "--burndown-files"];
    let a = stdout_ok(
        &[
            &["-a", "history/burndown", "--format", "json"],
            &flags[..],
            &[p],
        ]
        .concat(),
    );
    let b = stdout_ok(
        &[
            &["-a", "history/burndown", "--format", "json"],
            &flags[..],
            &[p],
        ]
        .concat(),
    );
    assert_eq!(a, b, "two runs must be byte-identical");
    let yaml = stdout_ok(
        &[
            &["-a", "history/burndown", "--format", "yaml"],
            &flags[..],
            &[p],
        ]
        .concat(),
    );
    assert!(yaml.contains("    total_current_lines: 15"), "{yaml}");
    assert!(yaml.contains("    tracked_developers: 2"), "{yaml}");
    assert!(yaml.contains("    - path: script.py"), "{yaml}");
    assert!(yaml.contains("      lines_modified: 7"), "{yaml}");
}
