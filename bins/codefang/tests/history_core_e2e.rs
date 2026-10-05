//! End-to-end coverage of the SHARED history-analysis core, driven through the
//! **real binary** over real on-disk git fixtures
//! (`specs/bugs/BUG-history-core-pipeline.md`).
//!
//! Every assertion here is a contract from `site/guide/cli-reference.md` or
//! plain git semantics: the commit window (`--head`, `--limit`, `--since`),
//! first-parent isolation, rename identity, host-independence of the output,
//! the shared change filter, identity merging, tick sizing, the one-document
//! rule for multi-analyzer JSON, and the named error messages.

mod common;

use std::process::Command;

use cf_gitlib::testutil::{
    commit_files, commit_files_as, init_repo, rename_file, CommitBuilder, TestRepo,
};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture histories: 2021-01-01T00:00:00Z.
const T0: i64 = 1_609_459_200;
/// One hour.
const HOUR: i64 = 3_600;
/// One day between fixture commits.
const DAY: i64 = 24 * HOUR;
/// The name of the fixture's second developer.
const BOB: (&str, &str) = ("Bob", "bob@example.com");
/// Number of files in the "big commit" fixture — above the 32-file spill
/// threshold the old pipeline silently applied.
const BIG_COMMIT_FILES: usize = 40;
/// The default commit-count a `--head` run must report.
const ONE_COMMIT: i64 = 1;

/// Runs the binary with a deterministic environment.
fn codefang(args: &[&str]) -> std::process::Output {
    codefang_with(common::command(BIN), args)
}

/// Runs the binary under `taskset -c <cpus>` (pins the CPU set the process
/// sees), with the same deterministic environment.
fn codefang_pinned(cpus: &str, args: &[&str]) -> std::process::Output {
    let mut cmd = common::command("taskset");
    cmd.args(["-c", cpus, BIN]);
    codefang_with(cmd, args)
}

/// The pinned clock of every run: multi-analyzer documents carry the
/// analysis time (`metadata.analyzed_at`), so two runs a second apart would
/// otherwise differ.
const PINNED_NOW: &str = "2030-01-01T00:00:00Z";

fn codefang_with(mut cmd: Command, args: &[&str]) -> std::process::Output {
    cmd.arg("run")
        .args(args)
        .env("CODEFANG_NOW", PINNED_NOW)
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .output()
        .expect("run the codefang binary")
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

/// stdout of a successful run, parsed as ONE JSON document.
fn json_ok(args: &[&str]) -> serde_json::Value {
    let text = stdout_ok(args);
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}):\n{text}"))
}

/// `(exit code, stderr)` of a run expected to fail.
fn fail(args: &[&str]) -> (i32, String, String) {
    let out = codefang(args);
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn path(test: &TestRepo) -> &str {
    test.path().to_str().expect("utf-8 path")
}

/// A Go source body with `n` one-line functions.
fn go_source(n: usize) -> Vec<u8> {
    let mut s = String::from("package main\n\n");
    for i in 0..n {
        s.push_str(&format!("func f{i}() int {{ return {i} }}\n"));
    }
    s.into_bytes()
}

/// A 100-line text body.
fn hundred_lines() -> Vec<u8> {
    (1..=100)
        .map(|i| format!("line {i}\n"))
        .collect::<String>()
        .into_bytes()
}

/// Alice's main line plus a side line by Bob merged into it:
/// `c1 (alice) ← side (bob) ; c3 (alice) ; merge (alice)`.
fn merge_fixture() -> TestRepo {
    let test = init_repo().expect("init");
    let c1 = commit_files(&test, "c1", T0, &[("a.txt", b"a1\n")]).expect("c1");
    let side = CommitBuilder::new(&test, "side", T0 + DAY)
        .author(BOB.0, BOB.1)
        .parent(c1)
        .detached()
        .write("b.txt", b"b1\n")
        .write("c.txt", b"c1\n")
        .commit()
        .expect("side");
    commit_files(&test, "c3", T0 + 2 * DAY, &[("a.txt", b"a1\na2\n")]).expect("c3");
    CommitBuilder::new(&test, "merge", T0 + 3 * DAY)
        .merge_parent(side)
        .write("b.txt", b"b1\n")
        .write("c.txt", b"c1\n")
        .commit()
        .expect("merge");
    test
}

/// The `developers` array of a devs report (standalone or inside the unified
/// document).
fn developers(report: &serde_json::Value) -> &Vec<serde_json::Value> {
    report["developers"]
        .as_array()
        .unwrap_or_else(|| panic!("no developers array in {report}"))
}

/// The report of analyzer `id` inside a unified `codefang.run.v1` document.
fn analyzer_report<'a>(doc: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    doc["analyzers"]
        .as_array()
        .unwrap_or_else(|| panic!("no analyzers array in {doc}"))
        .iter()
        .find(|a| a["id"] == id)
        .map(|a| &a["report"])
        .unwrap_or_else(|| panic!("no analyzer {id} in {doc}"))
}

/// Burndown is the only analyzer that walks first-parent; selecting it next
/// to devs must NOT drop Bob's side-line commit from the devs report.
#[test]
fn burndown_coselection_does_not_force_first_parent_on_devs() {
    let test = merge_fixture();
    let p = path(&test);

    let standalone = json_ok(&["-a", "history/devs", "--format", "json", p]);
    assert_eq!(
        developers(&standalone).len(),
        2,
        "standalone devs: {standalone}"
    );

    let combined = json_ok(&["-a", "history/devs,history/burndown", "--format", "json", p]);
    let devs = analyzer_report(&combined, "history/devs");
    assert_eq!(
        developers(devs).len(),
        2,
        "devs co-selected with burndown must still see Bob: {combined}"
    );

    let first_parent = json_ok(&[
        "-a",
        "history/devs",
        "--first-parent",
        "--format",
        "json",
        p,
    ]);
    assert_eq!(
        developers(&first_parent).len(),
        1,
        "the user's --first-parent still applies: {first_parent}"
    );
}

/// The report is a pure function of the repository and the flags: pinning the
/// process to two CPUs must give byte-identical output.
#[test]
fn output_does_not_depend_on_the_host_cpu_count() {
    let test = merge_fixture();
    let p = path(&test);
    let args_ndjson = [
        "-a",
        "history/couples,history/file-history,history/typos,history/devs",
        "--format",
        "ndjson",
        p,
    ];
    let args_json = [
        "-a",
        "history/couples,history/file-history",
        "--format",
        "json",
        p,
    ];

    let pinned = codefang_pinned("0-1", &args_ndjson);
    if !pinned.status.success() {
        eprintln!(
            "skipping: taskset -c 0-1 is not usable here: {}",
            String::from_utf8_lossy(&pinned.stderr)
        );
        return;
    }
    let unpinned = codefang(&args_ndjson);
    assert!(unpinned.status.success());
    assert_eq!(
        pinned.stdout, unpinned.stdout,
        "ndjson must be byte-identical under taskset -c 0-1"
    );

    let pinned = codefang_pinned("0-1", &args_json);
    let unpinned = codefang(&args_json);
    assert!(pinned.status.success() && unpinned.status.success());
    assert_eq!(
        pinned.stdout, unpinned.stdout,
        "json must be byte-identical under taskset -c 0-1"
    );

    // A merged-in change couples b.txt and c.txt exactly once: they were
    // co-changed in ONE commit (the side commit); the merge re-introduces
    // them on the main line but must not double count.
    let doc: serde_json::Value = serde_json::from_slice(&unpinned.stdout).expect("json");
    let couples = analyzer_report(&doc, "history/couples");
    let pair = couples["file_coupling"]
        .as_array()
        .expect("file_coupling")
        .iter()
        .find(|e| {
            (e["file1"] == "b.txt" && e["file2"] == "c.txt")
                || (e["file1"] == "c.txt" && e["file2"] == "b.txt")
        })
        .unwrap_or_else(|| panic!("no b.txt/c.txt coupling in {couples}"));
    assert_eq!(
        pair["co_changes"], 1,
        "merge double-counted the pair: {pair}"
    );
}

/// A pure `git mv` is a rename, not 100 deleted + 100 added lines.
#[test]
fn pure_rename_adds_and_removes_zero_lines_in_devs() {
    let test = init_repo().expect("init");
    let body = hundred_lines();
    commit_files(&test, "add", T0, &[("a.txt", &body)]).expect("add");
    let mv = rename_file(&test, "mv", T0 + DAY, "a.txt", "b.txt").expect("mv");
    let p = path(&test);

    let ndjson = stdout_ok(&["-a", "history/devs", "--format", "ndjson", p]);
    let line = ndjson
        .lines()
        .find(|l| l.contains(&mv.to_string()))
        .unwrap_or_else(|| panic!("no devs record for the rename commit in:\n{ndjson}"));
    assert!(
        line.contains(r#""lines_added":0,"lines_removed":0"#),
        "a pure rename must add/remove zero lines: {line}"
    );

    let json = json_ok(&["-a", "history/devs", "--format", "json", p]);
    assert_eq!(json["aggregate"]["total_lines_removed"], 0, "{json}");
    assert_eq!(json["aggregate"]["total_lines_added"], 100, "{json}");
}

/// `--since` is an author-time filter: a commit authored after the cutoff but
/// committed before it is included, one authored before the cutoff but
/// committed after it is excluded — regardless of walk order.
#[test]
fn since_keeps_every_commit_authored_after_the_cutoff() {
    let test = init_repo().expect("init");
    let cutoff = T0 + 10 * DAY;
    let a = CommitBuilder::new(&test, "a", cutoff + DAY)
        .committer_time(cutoff - 9 * DAY)
        .write("a.txt", b"a\n")
        .commit()
        .expect("a");
    let b = CommitBuilder::new(&test, "b", cutoff - DAY)
        .committer_time(cutoff + DAY)
        .write("b.txt", b"b\n")
        .commit()
        .expect("b");
    let c = commit_files(&test, "c", cutoff + 5 * DAY, &[("c.txt", b"c\n")]).expect("c");
    let p = path(&test);

    let ndjson = stdout_ok(&[
        "-a",
        "history/burndown",
        "--format",
        "ndjson",
        "--since",
        "2021-01-11T00:00:00Z",
        p,
    ]);
    let hashes: Vec<&str> = ndjson
        .lines()
        .filter_map(|l| l.find("\"hash\":\"").map(|i| &l[i + 8..i + 48]))
        .collect();
    assert_eq!(
        hashes,
        vec![a.to_string(), c.to_string()],
        "a (authored after, committed before) must be kept and b dropped; got {hashes:?}, b={b}"
    );
}

/// `--head` is the single-commit window `[HEAD]` for every analyzer and
/// format — including a non-merge HEAD for devs, and burndown's streaming
/// formats.
#[test]
fn head_is_a_single_commit_window_for_every_format() {
    let test = init_repo().expect("init");
    for i in 0..3_i64 {
        commit_files(
            &test,
            &format!("c{i}"),
            T0 + i * DAY,
            &[("f.go", go_source(i as usize + 1).as_slice())],
        )
        .expect("commit");
    }
    let head = test.repo.head().expect("head").to_string();
    let p = path(&test);

    let ndjson = stdout_ok(&["-a", "history/burndown", "--format", "ndjson", "--head", p]);
    let lines: Vec<&str> = ndjson.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "burndown ndjson --head must emit one record:\n{ndjson}"
    );
    assert!(lines[0].contains(&head));

    let ts = json_ok(&[
        "-a",
        "history/burndown",
        "--format",
        "timeseries",
        "--head",
        p,
    ]);
    assert_eq!(ts["commits"].as_array().map(Vec::len), Some(1), "{ts}");
    assert_eq!(ts["commits"][0]["hash"], head.as_str());

    let head_raw = stdout_ok(&["-a", "history/devs", "--format", "json", "--head", p]);
    let devs: serde_json::Value = serde_json::from_str(&head_raw).expect("json");
    assert_eq!(devs["aggregate"]["total_commits"], ONE_COMMIT, "{devs}");
    assert_eq!(developers(&devs).len(), 1, "{devs}");

    // `--head` ≡ `--limit 1`, byte for byte.
    let limit_one = stdout_ok(&["-a", "history/devs", "--format", "json", "--limit", "1", p]);
    assert_eq!(limit_one, head_raw);
}

/// Several history analyzers in `--format json` emit ONE document.
#[test]
fn multi_analyzer_history_json_is_one_document() {
    let test = merge_fixture();
    let p = path(&test);
    let doc = json_ok(&["-a", "history/devs,history/burndown", "--format", "json", p]);
    assert_eq!(doc["version"], "codefang.run.v1");
    let ids: Vec<&str> = doc["analyzers"]
        .as_array()
        .expect("analyzers")
        .iter()
        .filter_map(|a| a["id"].as_str())
        .collect();
    assert_eq!(ids, vec!["history/devs", "history/burndown"]);
    assert!(analyzer_report(&doc, "history/burndown")["aggregate"].is_object());
}

/// Every user error names its cause and exits 1 with empty stdout.
#[test]
fn user_errors_are_named() {
    let (rc, out, err) = fail(&["-a", "history/devs", "--format", "json", "/no/such/path"]);
    assert_eq!((rc, out.as_str()), (1, ""));
    assert!(
        err.contains("Error: path not found: /no/such/path"),
        "{err}"
    );

    let dir = tempfile::tempdir().expect("tempdir");
    let d = dir.path().to_str().unwrap();
    let (rc, out, err) = fail(&["-a", "history/devs", "--format", "json", d]);
    assert_eq!((rc, out.as_str()), (1, ""));
    assert!(
        err.contains(&format!("Error: cannot open repository: {d}")),
        "{err}"
    );

    let test = merge_fixture();
    let p = path(&test);
    let (rc, _out, err) = fail(&["-a", "history/devs", "--format", "bogus", p]);
    assert_eq!(rc, 1);
    assert!(err.contains("Error: unsupported format: bogus"), "{err}");

    let (rc, _out, err) = fail(&["-a", "static/complexity", "--format", "ndjson", p]);
    assert_eq!(rc, 1);
    assert!(
        err.contains("Error: format ndjson is not supported for static/complexity"),
        "{err}"
    );

    let (rc, _out, err) = fail(&["-a", "history/devs", "--format", "json", "--limit=-5", p]);
    assert_eq!(rc, 1);
    assert!(
        err.contains("Error: invalid --limit: must be >= 0, got -5"),
        "{err}"
    );

    let (rc, _out, err) = fail(&["-a", "history/devs", "--tick-size", "0", p]);
    assert_eq!(rc, 1);
    assert!(
        err.contains("Error: invalid --tick-size: must be > 0, got 0"),
        "{err}"
    );

    for (_, _, err) in [
        fail(&["-a", "history/devs", "/no/such/path"]),
        fail(&["-a", "history/devs", "--format", "bogus", p]),
    ] {
        assert!(!err.contains("dispatch is blocked"), "{err}");
    }
}

/// `--tick-size 1` buckets commits by the hour: the documented
/// `[0, 1, 11, 31, 73]` over the ticks fixture, and the timeseries document
/// reports the configured size.
#[test]
fn tick_size_one_hour_follows_the_documented_fixture() {
    // 2020-01-01T00:00:00Z.
    const START_2020: i64 = 1_577_836_800;
    let times = [
        START_2020 + 23 * HOUR + 59 * 60, // 2020-01-01T23:59:00Z
        START_2020 + DAY + 60,            // 2020-01-02T00:01:00Z
        START_2020 + DAY + 10 * HOUR,     // 2020-01-02T12:00:00+02:00
        START_2020 + 2 * DAY + 6 * HOUR,  // 2020-01-03T01:00:00-05:00
        START_2020 + 4 * DAY,             // 2020-01-05T00:00:00Z
    ];
    let test = init_repo().expect("init");
    for (i, t) in times.iter().enumerate() {
        commit_files(
            &test,
            &format!("c{i}"),
            *t,
            &[(&format!("{i}.go"), go_source(1).as_slice())],
        )
        .expect("commit");
    }
    let p = path(&test);

    let ndjson = stdout_ok(&[
        "-a",
        "history/burndown",
        "--format",
        "ndjson",
        "--tick-size",
        "1",
        p,
    ]);
    let ticks: Vec<i64> = ndjson
        .lines()
        .filter_map(|l| {
            let i = l.find("\"tick\":")? + 7;
            l[i..].split(',').next()?.parse().ok()
        })
        .collect();
    assert_eq!(ticks, vec![0, 1, 11, 31, 73], "{ndjson}");

    let ts = json_ok(&[
        "-a",
        "history/burndown",
        "--format",
        "timeseries",
        "--tick-size",
        "1",
        p,
    ]);
    assert_eq!(ts["tick_size_hours"], 1, "{ts}");
    let daily = json_ok(&["-a", "history/burndown", "--format", "timeseries", p]);
    assert_eq!(daily["tick_size_hours"], 24, "{daily}");
}

/// `--languages ""` means no filter in the history phase, like the static
/// phase.
#[test]
fn empty_languages_value_is_no_filter_in_history() {
    let test = merge_fixture();
    let p = path(&test);
    let unfiltered = stdout_ok(&["-a", "history/devs", "--format", "json", p]);
    let empty = stdout_ok(&[
        "-a",
        "history/devs",
        "--format",
        "json",
        "--languages",
        "",
        p,
    ]);
    assert_eq!(unfiltered, empty);
    let json: serde_json::Value = serde_json::from_str(&empty).expect("json");
    assert_eq!(json["aggregate"]["total_commits"], 4, "{json}");
}

/// A file whose header carries a generated-file marker is excluded from the
/// history by default and included with `--include-generated`.
#[test]
fn generated_marker_is_applied_in_history() {
    let test = init_repo().expect("init");
    let marked = b"// Code generated by protoc-gen-go. DO NOT EDIT.\npackage main\n\nfunc g() {}\n";
    commit_files(
        &test,
        "init",
        T0,
        &[
            ("a.go", go_source(2).as_slice()),
            ("gen.go", marked.as_slice()),
        ],
    )
    .expect("init");
    let p = path(&test);

    let default = json_ok(&["-a", "history/devs", "--format", "json", p]);
    assert_eq!(
        default["aggregate"]["total_lines_added"], 4,
        "a.go only: {default}"
    );

    let with_generated = json_ok(&[
        "-a",
        "history/devs",
        "--format",
        "json",
        "--include-generated",
        p,
    ]);
    assert_eq!(
        with_generated["aggregate"]["total_lines_added"], 8,
        "a.go + gen.go: {with_generated}"
    );
}

/// Loose identity merges every alias that shares a token with a known
/// person; `--exact-signatures` keeps them apart; `--anonymize` hides names.
#[test]
fn alias_identities_are_merged_and_flags_shape_names() {
    let test = init_repo().expect("init");
    commit_files_as(
        &test,
        "alice",
        "alice@x.com",
        "c1",
        T0,
        &[("a.txt", b"1\n")],
    )
    .expect("c1");
    commit_files_as(
        &test,
        "Alice Smith",
        "alice@x.com",
        "c2",
        T0 + DAY,
        &[("a.txt", b"1\n2\n")],
    )
    .expect("c2");
    commit_files_as(
        &test,
        "Alice Smith",
        "alice@other.org",
        "c3",
        T0 + 2 * DAY,
        &[("a.txt", b"1\n2\n3\n")],
    )
    .expect("c3");
    let p = path(&test);

    let loose = json_ok(&["-a", "history/devs", "--format", "json", p]);
    assert_eq!(
        developers(&loose).len(),
        1,
        "three aliases, one person: {loose}"
    );
    assert_eq!(developers(&loose)[0]["commits"], 3);

    let exact = json_ok(&[
        "-a",
        "history/devs",
        "--format",
        "json",
        "--exact-signatures",
        p,
    ]);
    assert_eq!(developers(&exact).len(), 3, "{exact}");

    let anon = json_ok(&["-a", "history/devs", "--format", "json", "--anonymize", p]);
    assert_eq!(developers(&anon)[0]["name"], "Developer-A", "{anon}");

    // Burndown and devs agree on the author ids (one identity detector).
    let burndown = stdout_ok(&["-a", "history/burndown", "--format", "ndjson", p]);
    assert_eq!(
        burndown.matches(r#""author_id":0"#).count(),
        3,
        "every commit resolves to author 0: {burndown}"
    );

    let dict = test.path().join("people.txt");
    std::fs::write(&dict, "Alice|alice@x.com|alice@other.org\n").expect("dict");
    let with_dict = json_ok(&[
        "-a",
        "history/devs",
        "--format",
        "json",
        "--people-dict",
        dict.to_str().unwrap(),
        p,
    ]);
    assert_eq!(developers(&with_dict)[0]["name"], "Alice", "{with_dict}");
}

/// A commit touching more than 32 files contributes every file: no spill
/// gate.
#[test]
fn large_commits_are_fully_analyzed_by_quality() {
    let test = init_repo().expect("init");
    let files: Vec<(String, Vec<u8>)> = (0..BIG_COMMIT_FILES)
        .map(|i| (format!("f{i}.go"), go_source(1)))
        .collect();
    let refs: Vec<(&str, &[u8])> = files
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    commit_files(&test, "big", T0, &refs).expect("big");
    let p = path(&test);

    let quality = json_ok(&["-a", "history/quality", "--format", "json", p]);
    assert_eq!(
        quality["aggregate"]["total_files_analyzed"], BIG_COMMIT_FILES as i64,
        "{quality}"
    );
}

/// The whole-selection history `--format compact` renders one section per
/// analyzer.
#[test]
fn history_compact_renders_one_section_per_analyzer() {
    let test = merge_fixture();
    let p = path(&test);
    let out = stdout_ok(&[
        "-a",
        "history/devs,history/burndown",
        "--format",
        "compact",
        p,
    ]);
    assert!(out.contains("history/devs"), "{out}");
    assert!(out.contains("history/burndown"), "{out}");
}
