//! End-to-end coverage of `history/shotness` through the **real binary** over
//! on-disk git fixtures (`specs/bugs/BUG-history-shotness-nested-entities.md`).
//!
//! The contract under test is the one `site/analyzers/shotness.md` documents:
//! an entity is a DSL-selected declaration anywhere in the tree (class
//! methods included), identified by its qualified name (`Class.method`,
//! `Receiver.method`) and file; a commit counts once per entity whose span
//! its hunks overlap; the entities a commit touches are coupled pairwise; a
//! file rename moves the live entities; a removed declaration or a deleted
//! file ends them (dropped by default; kept with their history and marked
//! `ended` under `--shotness-keep-ended`); merge commits are not re-counted;
//! the output is deterministic.

mod common;

use cf_gitlib::testutil::{commit_files, init_repo, rename_file, CommitBuilder, TestRepo};
use serde_json::Value;

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture histories: 2021-01-01T00:00:00Z.
const T0: i64 = 1_609_459_200;
/// One day between fixture commits.
const DAY: i64 = 24 * 3_600;
/// The analyzer under test.
const SHOTNESS: &str = "history/shotness";

/// `m.go` with two one-body functions.
const GO_F_G: &[u8] =
    b"package m\n\nfunc f() int {\n\treturn 1\n}\n\nfunc g() int {\n\treturn 2\n}\n";
/// `GO_F_G` with `f`'s body changed.
const GO_F_CHANGED: &[u8] =
    b"package m\n\nfunc f() int {\n\treturn 11\n}\n\nfunc g() int {\n\treturn 2\n}\n";
/// `GO_F_CHANGED` with `g`'s body changed too.
const GO_G_CHANGED: &[u8] =
    b"package m\n\nfunc f() int {\n\treturn 11\n}\n\nfunc g() int {\n\treturn 22\n}\n";
/// Both bodies changed again.
const GO_BOTH_CHANGED: &[u8] =
    b"package m\n\nfunc f() int {\n\treturn 111\n}\n\nfunc g() int {\n\treturn 222\n}\n";
/// `f` renamed to `f2` in place (its declaration line edited).
const GO_F_RENAMED: &[u8] =
    b"package m\n\nfunc f2() int {\n\treturn 111\n}\n\nfunc g() int {\n\treturn 222\n}\n";
/// `g` removed.
const GO_G_DELETED: &[u8] = b"package m\n\nfunc f2() int {\n\treturn 111\n}\n";
/// `h` added.
const GO_H_ADDED: &[u8] =
    b"package m\n\nfunc f2() int {\n\treturn 111\n}\n\nfunc h() int {\n\treturn 3\n}\n";

/// A Java class with two methods.
const JAVA_A: &[u8] = b"public class A {\n    public int f() {\n        return 1;\n    }\n\n    public int g() {\n        return 2;\n    }\n}\n";
/// `JAVA_A` with `f` changed.
const JAVA_A_CHANGED: &[u8] = b"public class A {\n    public int f() {\n        return 11;\n    }\n\n    public int g() {\n        return 2;\n    }\n}\n";
/// Two Python functions and a class with a method.
const PY_P: &[u8] =
    b"def pf():\n    return 1\n\ndef pg():\n    return 2\n\nclass K:\n    def km(self):\n        return 3\n";
/// `PY_P` with `K.km` changed.
const PY_P_CHANGED: &[u8] =
    b"def pf():\n    return 1\n\ndef pg():\n    return 2\n\nclass K:\n    def km(self):\n        return 33\n";
/// A TypeScript class with two methods and a top-level function.
const TS_T: &[u8] = b"class C {\n  m1(): number { return 1; }\n  m2(): number { return 2; }\n}\nfunction top(): number { return 3; }\n";
/// A C# namespace with a class with two methods.
const CS_P: &[u8] = b"namespace N {\n  public class C {\n    public int M1() { return 1; }\n    public int M2() { return 2; }\n  }\n}\n";
/// Nested Python declarations.
const PY_NESTED: &[u8] = b"def outer():\n    def inner():\n        return 1\n    return inner\n\nclass K:\n    def km(self):\n        return 3\n    class Inner:\n        def im(self):\n            return 4\n";
/// Two Go receivers with a same-named method.
const GO_RECEIVERS: &[u8] = b"package r\n\ntype A struct{}\ntype B struct{}\n\nfunc (a *A) String() string { return \"a\" }\n\nfunc (b *B) String() string { return \"b\" }\n";
/// `GO_RECEIVERS` with `A.String` changed.
const GO_RECEIVERS_CHANGED: &[u8] = b"package r\n\ntype A struct{}\ntype B struct{}\n\nfunc (a *A) String() string { return \"aa\" }\n\nfunc (b *B) String() string { return \"b\" }\n";

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

/// The shotness JSON report of `repo` with `extra` flags.
fn shotness(repo: &TestRepo, extra: &[&str]) -> Value {
    let mut args = vec!["-a", SHOTNESS, "--format", "json"];
    args.extend_from_slice(extra);
    args.push(path(repo));
    let text = stdout_ok(&args);
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("stdout is not one JSON document ({e}):\n{text}"))
}

fn path(test: &TestRepo) -> &str {
    test.path().to_str().expect("utf-8 path")
}

/// Commits `files` at `T0 + day * DAY`.
fn commit(repo: &TestRepo, day: i64, files: &[(&str, &[u8])]) {
    commit_files(repo, &format!("day {day}"), T0 + day * DAY, files).expect("commit");
}

/// `(name, type, file)` of every entity in the report, key-sorted.
fn entities(report: &Value) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = report["node_hotness"]
        .as_array()
        .unwrap_or_else(|| panic!("no node_hotness in {report}"))
        .iter()
        .map(|n| {
            (
                n["name"].as_str().unwrap().to_string(),
                n["type"].as_str().unwrap().to_string(),
                n["file"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    out.sort();
    out
}

/// The entity row for `(name, file)`.
fn row<'a>(report: &'a Value, name: &str, file: &str) -> &'a Value {
    report["node_hotness"]
        .as_array()
        .expect("node_hotness")
        .iter()
        .find(|n| n["name"] == name && n["file"] == file)
        .unwrap_or_else(|| panic!("no entity {name} in {file}: {report}"))
}

fn change_count(report: &Value, name: &str, file: &str) -> i64 {
    row(report, name, file)["change_count"].as_i64().unwrap()
}

fn coupled_nodes(report: &Value, name: &str, file: &str) -> i64 {
    row(report, name, file)["coupled_nodes"].as_i64().unwrap()
}

/// The co-change count of the pair, 0 when the pair is not listed.
fn co_changes(report: &Value, a: &str, b: &str) -> i64 {
    report["node_coupling"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|r| {
                    (r["node1_name"] == a && r["node2_name"] == b)
                        || (r["node1_name"] == b && r["node2_name"] == a)
                })
                .map(|r| r["co_changes"].as_i64().unwrap())
                .sum()
        })
        .unwrap_or(0)
}

fn total_nodes(report: &Value) -> i64 {
    report["aggregate"]["total_nodes"].as_i64().unwrap()
}

/// The sorted entity names of the report.
fn names(report: &Value) -> Vec<String> {
    entities(report).into_iter().map(|(n, _, _)| n).collect()
}

/// The sorted names of the entities the report marks ended.
fn ended(report: &Value) -> Vec<String> {
    let mut out: Vec<String> = report["node_hotness"]
        .as_array()
        .unwrap_or_else(|| panic!("no node_hotness in {report}"))
        .iter()
        .filter(|n| n["ended"].as_bool().expect("ended flag"))
        .map(|n| n["name"].as_str().unwrap().to_string())
        .collect();
    out.sort();
    out
}

/// One repository, asserted after every commit: counts accrue per touched
/// entity, an in-place rename ends the old name and starts the new one, a
/// removed declaration ends its entity (dropped with its couplings), and a
/// `git mv` moves the live entities under the new path with their counts.
#[test]
fn go_entity_lifecycle_counts_renames_and_deletions() {
    let repo = init_repo().expect("init");
    const M: &str = "m.go";

    commit(&repo, 0, &[(M, GO_F_G)]);
    let r = shotness(&repo, &[]);
    assert_eq!(names(&r), vec!["f", "g"], "{r}");
    assert_eq!(change_count(&r, "f", M), 1);
    assert_eq!(change_count(&r, "g", M), 1);
    assert_eq!(co_changes(&r, "f", "g"), 1, "created together: {r}");

    commit(&repo, 1, &[(M, GO_F_CHANGED)]);
    let r = shotness(&repo, &[]);
    assert_eq!(change_count(&r, "f", M), 2, "{r}");
    assert_eq!(change_count(&r, "g", M), 1);
    assert_eq!(co_changes(&r, "f", "g"), 1);

    commit(&repo, 2, &[(M, GO_G_CHANGED)]);
    let r = shotness(&repo, &[]);
    assert_eq!(change_count(&r, "f", M), 2);
    assert_eq!(change_count(&r, "g", M), 2, "{r}");

    commit(&repo, 3, &[(M, GO_BOTH_CHANGED)]);
    let r = shotness(&repo, &[]);
    assert_eq!(change_count(&r, "f", M), 3);
    assert_eq!(change_count(&r, "g", M), 3);
    assert_eq!(co_changes(&r, "f", "g"), 2, "{r}");
    assert_eq!(coupled_nodes(&r, "f", M), 1);

    commit(&repo, 4, &[(M, GO_F_RENAMED)]);
    let r = shotness(&repo, &[]);
    assert_eq!(names(&r), vec!["f2", "g"], "f ended, f2 born: {r}");
    assert_eq!(change_count(&r, "f2", M), 1);
    assert_eq!(change_count(&r, "g", M), 3);
    assert_eq!(co_changes(&r, "f2", "g"), 0);
    assert_eq!(coupled_nodes(&r, "g", M), 0, "f's coupling is gone: {r}");
    assert!(
        r["node_hotness"][0].get("ended").is_none(),
        "no ended field by default: {r}"
    );
    assert!(r["aggregate"].get("ended_nodes").is_none(), "{r}");

    commit(&repo, 5, &[(M, GO_G_DELETED)]);
    let r = shotness(&repo, &[]);
    assert_eq!(names(&r), vec!["f2"], "g ended: {r}");
    assert_eq!(change_count(&r, "f2", M), 1);

    commit(&repo, 6, &[(M, GO_H_ADDED)]);
    let r = shotness(&repo, &[]);
    assert_eq!(names(&r), vec!["f2", "h"], "{r}");
    assert_eq!(change_count(&r, "h", M), 1);
    assert_eq!(change_count(&r, "f2", M), 1);
    assert_eq!(co_changes(&r, "f2", "h"), 0);

    rename_file(&repo, "mv", T0 + 7 * DAY, M, "n.go").expect("git mv");
    let r = shotness(&repo, &[]);
    assert_eq!(
        entities(&r),
        vec![
            ("f2".to_string(), "Function".to_string(), "n.go".to_string()),
            ("h".to_string(), "Function".to_string(), "n.go".to_string()),
        ],
        "a git mv moves the entities, it does not duplicate them: {r}"
    );
    assert_eq!(change_count(&r, "f2", "n.go"), 1);
    assert_eq!(change_count(&r, "h", "n.go"), 1);
    assert_eq!(total_nodes(&r), 2);
}

/// The same history under `--shotness-keep-ended`: ended entities stay,
/// marked, with their counts and couplings; a `git mv` moves only the live
/// ones.
#[test]
fn keep_ended_reports_ended_entities_with_their_history() {
    let repo = init_repo().expect("init");
    const M: &str = "m.go";
    const KEEP: &[&str] = &["--shotness-keep-ended"];
    for (day, file) in [GO_F_G, GO_F_CHANGED, GO_G_CHANGED, GO_BOTH_CHANGED]
        .into_iter()
        .enumerate()
    {
        commit(&repo, day as i64, &[(M, file)]);
    }

    commit(&repo, 4, &[(M, GO_F_RENAMED)]);
    let r = shotness(&repo, KEEP);
    assert_eq!(names(&r), vec!["f", "f2", "g"], "f ended, f2 born: {r}");
    assert_eq!(ended(&r), vec!["f"], "{r}");
    assert_eq!(
        change_count(&r, "f", M),
        4,
        "the rename touched f once more"
    );
    assert_eq!(co_changes(&r, "f", "f2"), 1, "renamed in one commit: {r}");
    assert_eq!(
        coupled_nodes(&r, "g", M),
        1,
        "g keeps its history with f: {r}"
    );

    commit(&repo, 5, &[(M, GO_G_DELETED)]);
    commit(&repo, 6, &[(M, GO_H_ADDED)]);
    let r = shotness(&repo, KEEP);
    assert_eq!(names(&r), vec!["f", "f2", "g", "h"], "{r}");
    assert_eq!(ended(&r), vec!["f", "g"], "{r}");

    rename_file(&repo, "mv", T0 + 7 * DAY, M, "n.go").expect("git mv");
    let r = shotness(&repo, KEEP);
    assert_eq!(
        entities(&r),
        vec![
            ("f".to_string(), "Function".to_string(), "m.go".to_string()),
            ("f2".to_string(), "Function".to_string(), "n.go".to_string()),
            ("g".to_string(), "Function".to_string(), "m.go".to_string()),
            ("h".to_string(), "Function".to_string(), "n.go".to_string()),
        ],
        "the ended entities keep the path they ended under: {r}"
    );
    assert_eq!(total_nodes(&r), 4);
    assert_eq!(r["aggregate"]["ended_nodes"], 2, "{r}");
}

/// A file rename keeps every live entity's count and coupling under the new
/// path: three entities stay three (the old keys never linger).
#[test]
fn file_rename_keeps_counts_under_the_new_path() {
    let repo = init_repo().expect("init");
    const SRC: &[u8] = b"package m\n\nfunc f() int {\n\treturn 1\n}\n\nfunc g() int {\n\treturn 2\n}\n\nfunc h() int {\n\treturn 3\n}\n";
    const F_CHANGED: &[u8] = b"package m\n\nfunc f() int {\n\treturn 11\n}\n\nfunc g() int {\n\treturn 2\n}\n\nfunc h() int {\n\treturn 3\n}\n";
    const F_CHANGED_AGAIN: &[u8] = b"package m\n\nfunc f() int {\n\treturn 111\n}\n\nfunc g() int {\n\treturn 2\n}\n\nfunc h() int {\n\treturn 3\n}\n";
    commit(&repo, 0, &[("m.go", SRC)]);
    commit(&repo, 1, &[("m.go", F_CHANGED)]);
    commit(&repo, 2, &[("m.go", F_CHANGED_AGAIN)]);
    rename_file(&repo, "mv", T0 + 3 * DAY, "m.go", "pkg/n.go").expect("git mv");

    let r = shotness(&repo, &[]);
    assert_eq!(total_nodes(&r), 3, "{r}");
    assert!(
        entities(&r).iter().all(|(_, _, file)| file == "pkg/n.go"),
        "{r}"
    );
    assert_eq!(change_count(&r, "f", "pkg/n.go"), 3);
    assert_eq!(change_count(&r, "g", "pkg/n.go"), 1);
    assert_eq!(change_count(&r, "h", "pkg/n.go"), 1);
    assert_eq!(co_changes(&r, "f", "g"), 1);
    assert_eq!(co_changes(&r, "g", "h"), 1);
}

/// Class methods are entities, named by their class: Java, Python, TypeScript
/// and C# methods are all tracked, and a change inside a method counts for
/// that method only.
#[test]
fn class_methods_are_tracked_with_qualified_names() {
    let repo = init_repo().expect("init");
    commit(
        &repo,
        0,
        &[
            ("A.java", JAVA_A),
            ("p.py", PY_P),
            ("t.ts", TS_T),
            ("p.cs", CS_P),
        ],
    );
    commit(
        &repo,
        1,
        &[("A.java", JAVA_A_CHANGED), ("p.py", PY_P_CHANGED)],
    );

    let r = shotness(&repo, &[]);
    let expected: Vec<(String, String, String)> = [
        ("A.f", "Method", "A.java"),
        ("A.g", "Method", "A.java"),
        ("C.M1", "Method", "p.cs"),
        ("C.M2", "Method", "p.cs"),
        ("C.m1", "Method", "t.ts"),
        ("C.m2", "Method", "t.ts"),
        ("K.km", "Function", "p.py"),
        ("pf", "Function", "p.py"),
        ("pg", "Function", "p.py"),
        ("top", "Function", "t.ts"),
    ]
    .iter()
    .map(|(n, t, f)| ((*n).to_string(), (*t).to_string(), (*f).to_string()))
    .collect();
    assert_eq!(entities(&r), expected, "{r}");
    assert_eq!(change_count(&r, "A.f", "A.java"), 2);
    assert_eq!(change_count(&r, "A.g", "A.java"), 1);
    assert_eq!(change_count(&r, "K.km", "p.py"), 2);
    assert_eq!(change_count(&r, "pf", "p.py"), 1);
    assert_eq!(co_changes(&r, "A.f", "K.km"), 2, "{r}");
}

/// Nested declarations carry the whole enclosing chain.
#[test]
fn nested_declarations_are_qualified_by_every_enclosing_declaration() {
    let repo = init_repo().expect("init");
    commit(&repo, 0, &[("n.py", PY_NESTED)]);
    let r = shotness(&repo, &[]);
    assert_eq!(
        names(&r),
        vec!["K.Inner.im", "K.km", "outer", "outer.inner"],
        "{r}"
    );
}

/// Go methods are qualified by their receiver type: two `String()` methods
/// are two entities, and only the changed one counts.
#[test]
fn same_name_methods_on_two_receivers_are_distinct() {
    let repo = init_repo().expect("init");
    commit(&repo, 0, &[("r.go", GO_RECEIVERS)]);
    commit(&repo, 1, &[("r.go", GO_RECEIVERS_CHANGED)]);
    let r = shotness(&repo, &[]);
    assert_eq!(
        entities(&r),
        vec![
            (
                "A.String".to_string(),
                "Method".to_string(),
                "r.go".to_string()
            ),
            (
                "B.String".to_string(),
                "Method".to_string(),
                "r.go".to_string()
            ),
        ],
        "{r}"
    );
    assert_eq!(change_count(&r, "A.String", "r.go"), 2);
    assert_eq!(change_count(&r, "B.String", "r.go"), 1);
}

/// A merge commit re-shows the side line's changes against its first
/// parent; counting it would double every side-line touch. Under
/// `--first-parent` the merge is the only sight of the side line and counts
/// as one commit.
#[test]
fn merge_commits_are_not_counted_twice() {
    let repo = merge_fixture();

    let r = shotness(&repo, &[]);
    assert_eq!(
        change_count(&r, "f", "m.go"),
        3,
        "c1 + two side commits: {r}"
    );
    assert_eq!(change_count(&r, "g", "m.go"), 2, "c1 + c3: {r}");

    let r = shotness(&repo, &["--first-parent"]);
    assert_eq!(change_count(&r, "f", "m.go"), 2, "c1 + the merge: {r}");
    assert_eq!(change_count(&r, "g", "m.go"), 2);
}

/// `c1 ; side 1 ; side 2 (f changed twice on a side line) ; c3 (g changed) ;
/// merge` — every side-line change reaches the main line through the merge.
fn merge_fixture() -> TestRepo {
    let repo = init_repo().expect("init");
    const SIDE_1: &[u8] =
        b"package m\n\nfunc f() int {\n\treturn 11\n}\n\nfunc g() int {\n\treturn 2\n}\n";
    const SIDE_2: &[u8] =
        b"package m\n\nfunc f() int {\n\treturn 111\n}\n\nfunc g() int {\n\treturn 2\n}\n";
    const MAIN: &[u8] =
        b"package m\n\nfunc f() int {\n\treturn 1\n}\n\nfunc g() int {\n\treturn 22\n}\n";
    const MERGED: &[u8] =
        b"package m\n\nfunc f() int {\n\treturn 111\n}\n\nfunc g() int {\n\treturn 22\n}\n";
    let c1 = commit_files(&repo, "c1", T0, &[("m.go", GO_F_G)]).expect("c1");
    let side_1 = CommitBuilder::new(&repo, "side 1", T0 + DAY)
        .parent(c1)
        .detached()
        .write("m.go", SIDE_1)
        .commit()
        .expect("side 1");
    let side_2 = CommitBuilder::new(&repo, "side 2", T0 + 2 * DAY)
        .parent(side_1)
        .detached()
        .write("m.go", SIDE_2)
        .commit()
        .expect("side 2");
    commit(&repo, 3, &[("m.go", MAIN)]);
    CommitBuilder::new(&repo, "merge", T0 + 4 * DAY)
        .merge_parent(side_2)
        .write("m.go", MERGED)
        .commit()
        .expect("merge");
    repo
}

/// The shotness report is the same alone and co-selected with another UAST
/// analyzer (the shared multi-analyzer walk): `--first-parent` and both DSL
/// flags reach either walk.
#[test]
fn co_selection_honors_first_parent_and_the_dsl_flags() {
    let merges = merge_fixture();
    let classes = init_repo().expect("init");
    commit(&classes, 0, &[("A.java", JAVA_A), ("p.py", PY_P)]);
    commit(
        &classes,
        1,
        &[("A.java", JAVA_A_CHANGED), ("p.py", PY_P_CHANGED)],
    );
    let selection = format!("{SHOTNESS},history/quality");
    let cases: [(&TestRepo, &[&str]); 3] = [
        (&merges, &["--first-parent"]),
        (
            &classes,
            &["--shotness-dsl-struct", "rfilter(.roles has \"Class\")"],
        ),
        (&classes, &["--shotness-dsl-name", ".type"]),
    ];
    let mut defects = Vec::new();
    for (repo, extra) in cases {
        let alone = shotness(repo, extra);
        let mut args = vec!["-a", selection.as_str(), "--format", "json"];
        args.extend_from_slice(extra);
        args.push(path(repo));
        let doc: Value = serde_json::from_str(&stdout_ok(&args)).expect("one JSON document");
        let co_selected = doc["analyzers"]
            .as_array()
            .expect("analyzers array")
            .iter()
            .find(|a| a["id"] == SHOTNESS)
            .map(|a| &a["report"])
            .unwrap_or_else(|| panic!("no {SHOTNESS} in {doc}"));
        if &alone != co_selected {
            defects.push(format!(
                "{extra:?}:\n  alone {alone}\n  co-selected {co_selected}"
            ));
        }
    }
    assert!(defects.is_empty(), "{}", defects.join("\n"));
}

/// `--languages` restricts the tracked entities to the named languages;
/// binary and unparsable files never fail the run.
#[test]
fn languages_filter_is_respected_and_odd_files_are_skipped() {
    let repo = init_repo().expect("init");
    commit(
        &repo,
        0,
        &[
            ("m.go", GO_F_G),
            ("p.py", PY_P),
            ("bin.dat", b"bin\x00ary\n"),
            ("broken.go", b"package z\n\nfunc (((( {{{{ broken\n"),
        ],
    );
    let all = shotness(&repo, &[]);
    assert_eq!(names(&all), vec!["K.km", "f", "g", "pf", "pg"], "{all}");
    let py = shotness(&repo, &["--languages", "python"]);
    assert_eq!(names(&py), vec!["K.km", "pf", "pg"], "{py}");
    let go = shotness(&repo, &["--languages", "go"]);
    assert_eq!(names(&go), vec!["f", "g"], "{go}");
}

/// Two runs are byte-identical; json and yaml carry the same rows; the
/// multi-analyzer (shared) walk gives the same shotness report as the direct
/// walk; ndjson and timeseries list the node-touching commits.
#[test]
fn output_is_deterministic_and_every_format_agrees() {
    let repo = init_repo().expect("init");
    commit(&repo, 0, &[("A.java", JAVA_A)]);
    commit(&repo, 1, &[("A.java", JAVA_A_CHANGED)]);
    let p = path(&repo);

    let first = stdout_ok(&["-a", SHOTNESS, "--format", "json", p]);
    let second = stdout_ok(&["-a", SHOTNESS, "--format", "json", p]);
    assert_eq!(first, second, "two runs must be byte-identical");

    let yaml = stdout_ok(&["-a", SHOTNESS, "--format", "yaml", p]);
    assert!(yaml.contains("name: A.f"), "{yaml}");
    assert!(yaml.contains("change_count: 2"), "{yaml}");
    assert!(yaml.contains("name: A.g"), "{yaml}");

    let combined: Value = serde_json::from_str(&stdout_ok(&[
        "-a",
        "history/shotness,history/typos",
        "--format",
        "json",
        p,
    ]))
    .expect("one JSON document");
    let shared = combined["analyzers"]
        .as_array()
        .expect("analyzers")
        .iter()
        .find(|a| a["id"] == SHOTNESS)
        .map(|a| a["report"].clone())
        .expect("shotness in the combined document");
    let direct: Value = serde_json::from_str(&first).expect("json");
    assert_eq!(shared, direct, "the shared walk must match the direct walk");

    let ndjson = stdout_ok(&["-a", SHOTNESS, "--format", "ndjson", p]);
    let lines: Vec<Value> = ndjson
        .lines()
        .map(|l| serde_json::from_str(l).expect("ndjson line"))
        .collect();
    assert_eq!(
        lines.len(),
        2,
        "one line per node-touching commit: {ndjson}"
    );
    let touched = lines[1]["data"]["NodesTouched"]
        .as_object()
        .expect("NodesTouched");
    assert_eq!(touched.len(), 1, "the second commit touched A.f only");
    assert!(touched.contains_key("Method_A.f_A.java"), "{ndjson}");

    let ts: Value =
        serde_json::from_str(&stdout_ok(&["-a", SHOTNESS, "--format", "timeseries", p]))
            .expect("timeseries");
    let commits = ts["commits"].as_array().expect("commits");
    assert_eq!(commits[0]["shotness"]["nodes_touched"], 2, "{ts}");
    assert_eq!(commits[0]["shotness"]["coupling_pairs"], 1);
    assert_eq!(commits[1]["shotness"]["nodes_touched"], 1);
    assert_eq!(commits[1]["shotness"]["coupling_pairs"], 0);
}

/// `--shotness-dsl-struct` selects the tracked entities; an invalid
/// expression is a named flag error before any walk.
#[test]
fn dsl_flags_are_honored_and_validated() {
    let repo = init_repo().expect("init");
    commit(&repo, 0, &[("A.java", JAVA_A), ("p.py", PY_P)]);

    let classes = shotness(
        &repo,
        &["--shotness-dsl-struct", "rfilter(.roles has \"Class\")"],
    );
    assert_eq!(
        entities(&classes),
        vec![
            ("A".to_string(), "Class".to_string(), "A.java".to_string()),
            ("K".to_string(), "Class".to_string(), "p.py".to_string()),
        ],
        "{classes}"
    );

    let shallow = shotness(
        &repo,
        &["--shotness-dsl-struct", "filter(.roles has \"Function\")"],
    );
    assert_eq!(
        names(&shallow),
        vec!["pf", "pg"],
        "top-level only: {shallow}"
    );

    // The name query names the selected nodes AND their enclosing
    // declarations alike.
    let typed = shotness(&repo, &["--shotness-dsl-name", ".type"]);
    assert_eq!(
        names(&typed),
        vec!["Class.Function", "Class.Method", "Function"],
        "{typed}"
    );

    for (flag, value) in [
        ("--shotness-dsl-struct", "rfilter("),
        ("--shotness-dsl-name", "'unterminated"),
    ] {
        let out = codefang(&["-a", SHOTNESS, "--format", "json", flag, value, path(&repo)]);
        assert!(!out.status.success(), "{flag} {value} must fail");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(flag), "stderr must name the flag: {stderr}");
        assert!(out.stdout.is_empty(), "no report on a flag error");
    }
}

/// Two functions with names distinctive enough to search rendered pages for.
const KEPT_AND_REMOVED: &[u8] =
    b"package m\n\nfunc keptfunction() int {\n\treturn 1\n}\n\nfunc removedfunction() int {\n\treturn 2\n}\n";
/// The same file after `removedfunction` is deleted.
const ONLY_KEPT: &[u8] = b"package m\n\nfunc keptfunction() int {\n\treturn 1\n}\n";

/// Every file under `dir`, concatenated.
fn rendered_pages(dir: &std::path::Path) -> String {
    let mut out = String::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("read plot dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                out.push_str(&String::from_utf8_lossy(
                    &std::fs::read(&path).expect("read"),
                ));
            }
        }
    }
    out
}

/// Under `--shotness-keep-ended` a removed declaration stays in the machine
/// report, marked ended, and the text report marks it; the plot pages, a
/// view of the code that exists, leave it out.
#[test]
fn ended_entities_stay_in_the_report_and_leave_the_plot() {
    let repo = init_repo().expect("init");
    commit(&repo, 0, &[("m.go", KEPT_AND_REMOVED)]);
    commit(&repo, 1, &[("m.go", ONLY_KEPT)]);
    let p = path(&repo);

    let r = shotness(&repo, &["--shotness-keep-ended"]);
    assert_eq!(ended(&r), vec!["removedfunction"], "{r}");
    assert_eq!(r["aggregate"]["ended_nodes"], 1, "{r}");

    let text = stdout_ok(&[
        "-a",
        SHOTNESS,
        "--shotness-keep-ended",
        "--format",
        "text",
        p,
    ]);
    let removed_row = text
        .lines()
        .find(|l| l.contains("removedfunction"))
        .unwrap_or_else(|| panic!("no row for the ended entity:\n{text}"));
    assert!(removed_row.contains("ended"), "{text}");
    assert!(text.contains("Ended Nodes"), "{text}");

    let dir = tempfile::tempdir().expect("plot dir");
    let out = dir.path().to_str().expect("utf-8 dir");
    stdout_ok(&[
        "-a",
        SHOTNESS,
        "--shotness-keep-ended",
        "--format",
        "plot",
        "--output",
        out,
        p,
    ]);
    let pages = rendered_pages(dir.path());
    assert!(pages.contains("keptfunction"), "the live entity is plotted");
    assert!(
        !pages.contains("removedfunction"),
        "the ended entity is not plotted"
    );
}
