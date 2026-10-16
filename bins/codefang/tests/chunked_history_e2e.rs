//! End-to-end coverage of the chunked history core, driven through the
//! **real binary** over real on-disk git fixtures
//! (`specs/frds/FRD-bounded-memory-history.md`).
//!
//! Contracts pinned here:
//!
//! * the `--max-changes-per-commit` drop gate is applied by EVERY analyzer,
//!   so an analyzer's report does not depend on which other analyzers are
//!   co-selected, and no per-commit stream names a dropped commit;
//! * merged ndjson streams stay in window order across a dropped commit,
//!   whichever leaf is selected first;
//! * `--chunk-size <n>` never changes a byte of any output format of the
//!   analyzers whose walks fold per chunk ([`CHUNKED_IDS`]) nor of the
//!   co-selected UAST analyzers' shared walk ([`UAST_CO_SELECTION`]), on a
//!   fixture with merges, renames, deletes, a binary file, an aliased author,
//!   an empty commit, an oversized commit at a chunk edge and a typo fix;
//! * an out-of-range `--chunk-size` is a named error before any walk.

mod common;

use std::path::Path;

use cf_gitlib::testutil::{commit_files_as, init_repo, CommitBuilder, TestRepo};
use cf_gitlib::Hash;

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture histories: 2021-01-01T00:00:00Z.
const T0: i64 = 1_609_459_200;
/// One day between fixture commits (one default tick each).
const DAY: i64 = 24 * 3_600;
/// The fixture developers.
const ALICE: (&str, &str) = ("alice", "alice@example.com");
const BOB: (&str, &str) = ("bob", "bob@example.com");
const CAROL: (&str, &str) = ("carol", "carol@example.com");
/// The `--max-changes-per-commit` cap the drop-gate fixture is sized
/// against: its oversized commit changes one file more than this.
const CAP: &str = "2";
/// The analyzers whose direct walks once ignored the drop gate.
const DROP_GATE_IDS: [&str; 4] = [
    "history/typos",
    "history/sentiment",
    "history/anomaly",
    "history/imports",
];
/// The analyzer co-selected with each [`DROP_GATE_IDS`] entry: a UAST
/// analyzer, so the typos / sentiment / imports runs take the shared walk.
const CO_SELECTED: &str = "history/quality";
/// Every history analyzer id.
const ALL_HISTORY_IDS: [&str; 10] = [
    "history/burndown",
    "history/devs",
    "history/couples",
    "history/file-history",
    "history/shotness",
    "history/quality",
    "history/sentiment",
    "history/typos",
    "history/anomaly",
    "history/imports",
];

/// Go source with a typo (`recieve`), a comment and one import.
const MAIN_GO_V1: &[u8] = b"package main

import \"fmt\"

// great work, this is a wonderful helper
func recieve(x int) int {
\tif x > 0 {
\t\treturn x
\t}
\treturn -x
}

func main() { fmt.Println(recieve(1)) }
";

/// The typo fixed on the same lines, a second import and a new comment.
const MAIN_GO_V2: &[u8] = b"package main

import (
\t\"fmt\"
\t\"os\"
)

// great work, this is a wonderful helper
func receive(x int) int {
\tif x > 0 {
\t\treturn x
\t}
\treturn -x
}

// terrible awful broken hack, I hate it
func main() { fmt.Println(receive(1)); os.Exit(0) }
";

/// Python helper with an import and a comment.
const UTIL_PY_V2: &[u8] = b"import os
# nice and happy code
def helper(a):
    if a:
        return os.getcwd()
    return None
";

/// A third Go file of the oversized commit.
const EXTRA_GO: &[u8] = b"package main

import \"strings\"

// dreadful sad code nobody likes
func upper(s string) string { return strings.ToUpper(s) }
";

/// A Go helper with a typo (`trimm`), an import and a comment.
const HELPER_GO_V1: &[u8] = b"package main

import \"strings\"

// lovely tidy helper for trimming
func trimm(s string) string {
\treturn strings.TrimSpace(s)
}
";

/// The helper's typo fixed.
const HELPER_GO_V2: &[u8] = b"package main

import \"strings\"

// lovely tidy helper for trimming
func trim(s string) string {
\treturn strings.TrimSpace(s)
}
";

/// A four-commit history whose second commit (Bob's) changes three files,
/// one above [`CAP`]; every other commit stays within it. The oversized
/// commit carries data every analyzer reports: a typo fix, new comments,
/// new imports, line churn.
struct DropGateFixture {
    test: TestRepo,
    /// The window, oldest-first.
    window: Vec<Hash>,
    /// The commit above the cap.
    oversized: Hash,
}

fn drop_gate_fixture() -> DropGateFixture {
    let test = init_repo().expect("init");
    let c1 = commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c1",
        T0,
        &[("main.go", MAIN_GO_V1), ("util.py", b"lib\n")],
    )
    .expect("c1");
    let c2 = commit_files_as(
        &test,
        BOB.0,
        BOB.1,
        "c2",
        T0 + DAY,
        &[
            ("main.go", MAIN_GO_V2),
            ("util.py", UTIL_PY_V2),
            ("extra.go", EXTRA_GO),
        ],
    )
    .expect("c2");
    let c3 = commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c3",
        T0 + 2 * DAY,
        &[("helper.go", HELPER_GO_V1)],
    )
    .expect("c3");
    let c4 = commit_files_as(
        &test,
        CAROL.0,
        CAROL.1,
        "c4",
        T0 + 3 * DAY,
        &[("helper.go", HELPER_GO_V2)],
    )
    .expect("c4");
    DropGateFixture {
        test,
        window: vec![c1, c2, c3, c4],
        oversized: c2,
    }
}

/// The pinned run clock (`CODEFANG_NOW`): a multi-analyzer document stamps
/// `analyzed_at`, which must not differ between two runs compared byte for
/// byte.
const PINNED_NOW: &str = "2021-06-01T00:00:00Z";

/// Runs `codefang run <args>` with a deterministic environment.
fn codefang(args: &[&str]) -> std::process::Output {
    common::command(BIN)
        .arg("run")
        .args(args)
        .env("CODEFANG_NOW", PINNED_NOW)
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .output()
        .expect("run the codefang binary")
}

fn path(test: &TestRepo) -> &str {
    test.path().to_str().expect("utf-8 path")
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
    let out = stdout_ok(args);
    serde_json::from_slice(&out).unwrap_or_else(|e| {
        panic!(
            "stdout is not one JSON document ({e}):\n{}",
            String::from_utf8_lossy(&out)
        )
    })
}

/// The report of analyzer `id` inside a multi-analyzer JSON document
/// (`{version, metadata, analyzers: [{id, mode, report}]}`).
fn analyzer_report<'a>(doc: &'a serde_json::Value, id: &str) -> &'a serde_json::Value {
    doc["analyzers"]
        .as_array()
        .unwrap_or_else(|| panic!("no analyzers array in {doc}"))
        .iter()
        .find(|a| a["id"] == id)
        .map(|a| &a["report"])
        .unwrap_or_else(|| panic!("no analyzer {id} in {doc}"))
}

/// The window positions of the hashes an ndjson stream names, line by line.
fn ndjson_positions(out: &[u8], window: &[Hash]) -> Vec<usize> {
    let order: Vec<String> = window.iter().map(ToString::to_string).collect();
    String::from_utf8_lossy(out)
        .lines()
        .map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).expect("ndjson line");
            let hash = v["hash"].as_str().expect("hash");
            order
                .iter()
                .position(|h| h == hash)
                .unwrap_or_else(|| panic!("unknown hash {hash}"))
        })
        .collect()
}

/// An analyzer's report is the same whether it runs alone or next to a
/// co-selected UAST analyzer, when a commit exceeds the cap: both walks drop
/// the oversized commit.
#[test]
fn drop_gate_reports_do_not_depend_on_the_co_selection() {
    let fx = drop_gate_fixture();
    let p = path(&fx.test);
    let mut defects = Vec::new();
    for id in DROP_GATE_IDS {
        let single = json_ok(&[
            "-a",
            id,
            "--format",
            "json",
            "--max-changes-per-commit",
            CAP,
            p,
        ]);
        let selection = format!("{id},{CO_SELECTED}");
        let multi = json_ok(&[
            "-a",
            &selection,
            "--format",
            "json",
            "--max-changes-per-commit",
            CAP,
            p,
        ]);
        let co_selected = analyzer_report(&multi, id);
        if &single != co_selected {
            defects.push(format!("{id}: alone {single}\n  co-selected {co_selected}"));
        }
    }
    assert!(defects.is_empty(), "{}", defects.join("\n"));
}

/// No analyzer's per-commit stream names the commit the cap dropped.
#[test]
fn no_analyzer_streams_a_dropped_commit() {
    let fx = drop_gate_fixture();
    let p = path(&fx.test);
    let dropped = fx.oversized.to_string();
    let offenders: Vec<&str> = ALL_HISTORY_IDS
        .into_iter()
        .filter(|id| {
            let out = stdout_ok(&[
                "-a",
                id,
                "--format",
                "ndjson",
                "--max-changes-per-commit",
                CAP,
                p,
            ]);
            String::from_utf8_lossy(&out).contains(&dropped)
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "these analyzers stream the dropped commit {dropped}: {offenders:?}"
    );
}

/// A selection whose FIRST leaf is shotness: on a position tie the merge
/// emits the first-selected leaf, so a shotness record keyed on its
/// kept-commit index would overtake the devs record of an earlier commit.
const SHOTNESS_FIRST: &str = "history/shotness,history/devs";

/// A merged ndjson stream over every drop-gate analyzer keeps window order
/// across the dropped commit: each analyzer keys its records on the window
/// position, never on its kept-commit index.
#[test]
fn merged_ndjson_keeps_window_order_across_a_dropped_commit() {
    let fx = drop_gate_fixture();
    let p = path(&fx.test);
    let selection = std::iter::once("history/devs")
        .chain(DROP_GATE_IDS)
        .collect::<Vec<_>>()
        .join(",");
    for selection in [
        selection.as_str(),
        "history/devs,history/sentiment",
        SHOTNESS_FIRST,
    ] {
        let out = stdout_ok(&[
            "-a",
            selection,
            "--format",
            "ndjson",
            "--max-changes-per-commit",
            CAP,
            p,
        ]);
        let positions = ndjson_positions(&out, &fx.window);
        assert!(
            positions.windows(2).all(|w| w[0] <= w[1]),
            "{selection}: records out of window order {positions:?}:\n{}",
            String::from_utf8_lossy(&out)
        );
    }
}

/// The analyzers whose walks fold per chunk: their output must not depend on
/// `--chunk-size`.
const CHUNKED_IDS: [&str; 10] = [
    "history/burndown",
    "history/devs",
    "history/anomaly",
    "history/file-history",
    "history/couples",
    "history/quality",
    "history/sentiment",
    "history/typos",
    "history/imports",
    "history/shotness",
];
/// The five UAST analyzers co-selected: ONE shared pass feeds every one of
/// them, so this selection exercises the shared walk's fold.
const UAST_CO_SELECTION: &str =
    "history/quality,history/sentiment,history/typos,history/imports,history/shotness";
/// The `--chunk-size` values every chunked analyzer is run with: one commit
/// per chunk, two, a size that puts the oversized commit last in the first
/// chunk, and a size above the window.
const CHUNK_SIZES: [&str; 4] = ["1", "2", "7", "1000"];
/// The chunk fixture's `--max-changes-per-commit` cap: its oversized commit
/// changes one file more than this.
const CHUNK_FIXTURE_CAP: &str = "3";
/// The `--format` values `codefang run --help` advertises.
const ADVERTISED_FORMATS: [&str; 8] = [
    "json",
    "yaml",
    "plot",
    "bin",
    "timeseries",
    "ndjson",
    "text",
    "compact",
];
/// One requested output: a label and the flags that select it.
const FORMATS: [(&str, &[&str]); 9] = [
    ("json", &["--format", "json"]),
    ("yaml", &["--format", "yaml"]),
    ("bin", &["--format", "bin"]),
    ("timeseries", &["--format", "timeseries"]),
    ("timeseries+ndjson", &["--format", "timeseries", "--ndjson"]),
    ("ndjson", &["--format", "ndjson"]),
    ("text", &["--format", "text"]),
    ("compact", &["--format", "compact"]),
    ("plot", &["--format", "plot"]),
];
/// The label of the format that writes files instead of stdout.
const PLOT: &str = "plot";
/// The name-aliased spelling of Alice (same name, another email).
const ALICE_ELSEWHERE: (&str, &str) = ("alice", "alice@elsewhere.org");
/// A blob the binary detector rejects (NUL bytes after a PNG signature).
const PNG_V1: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x01";
/// The binary blob, changed.
const PNG_V2: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x02";

/// `n` numbered lines.
fn lines(n: usize) -> Vec<u8> {
    (1..=n)
        .map(|i| format!("line {i}\n"))
        .collect::<String>()
        .into_bytes()
}

/// A Go source body with one import and `n` one-line functions, each under
/// its own comment (shotness entities, sentiment comments, imports).
fn go_source(n: usize) -> Vec<u8> {
    let mut s = String::from("package main\n\nimport \"strings\"\n\n");
    for i in 0..n {
        s.push_str(&format!(
            "// helper number {i} keeps the happy path tidy\nfunc f{i}() int {{ return {i} }}\n"
        ));
    }
    s.into_bytes()
}

/// A twelve-commit history exercising every walk edge:
///
/// | pos | commit | author | edit |
/// |-----|--------|--------|------|
/// | 0 | c1 | alice | `a.go`, `b.txt`, binary `logo.png` |
/// | 1 | side | bob | side line off c1: `c.txt`, `b.txt` |
/// | 2 | c3 | Alice Smith (email alias) | `a.go` |
/// | 3 | merge | alice | merges side |
/// | 4 | rename | bob | `a.go` → `src/a.go` |
/// | 5 | empty | bob | no change |
/// | 6 | oversized | bob | four new files (above the cap) |
/// | 7 | delete | alice (name alias) | deletes `b.txt` |
/// | 8 | binary | alice | `logo.png`, `src/a.go` |
/// | 9–11 | c10–c12 | alternating | small edits; `t.go` gains a typo (c10), the typo is fixed (c11) |
fn chunk_fixture() -> TestRepo {
    let test = init_repo().expect("init");
    let at = |n: i64| T0 + n * DAY;
    let c1 = CommitBuilder::new(&test, "c1", at(0))
        .author(ALICE.0, ALICE.1)
        .write("a.go", &go_source(2))
        .write("b.txt", &lines(3))
        .write("logo.png", PNG_V1)
        .commit()
        .expect("c1");
    let side = CommitBuilder::new(&test, "side", at(1))
        .author(BOB.0, BOB.1)
        .parent(c1)
        .detached()
        .write("c.txt", &lines(2))
        .write("b.txt", &lines(4))
        .commit()
        .expect("side");
    CommitBuilder::new(&test, "c3", at(2))
        .author("Alice Smith", ALICE.1)
        .write("a.go", &go_source(3))
        .commit()
        .expect("c3");
    CommitBuilder::new(&test, "merge", at(3))
        .author(ALICE.0, ALICE.1)
        .merge_parent(side)
        .write("c.txt", &lines(2))
        .write("b.txt", &lines(4))
        .commit()
        .expect("merge");
    CommitBuilder::new(&test, "rename", at(4))
        .author(BOB.0, BOB.1)
        .rename("a.go", "src/a.go")
        .commit()
        .expect("rename");
    CommitBuilder::new(&test, "empty", at(5))
        .author(BOB.0, BOB.1)
        .commit()
        .expect("empty");
    let mut oversized = CommitBuilder::new(&test, "oversized", at(6)).author(BOB.0, BOB.1);
    for i in 0..4 {
        oversized = oversized.write(&format!("d{i}.txt"), &lines(i + 1));
    }
    oversized.commit().expect("oversized");
    CommitBuilder::new(&test, "delete", at(7))
        .author(ALICE_ELSEWHERE.0, ALICE_ELSEWHERE.1)
        .delete("b.txt")
        .commit()
        .expect("delete");
    CommitBuilder::new(&test, "binary", at(8))
        .author(ALICE.0, ALICE.1)
        .write("logo.png", PNG_V2)
        .write("src/a.go", &go_source(4))
        .commit()
        .expect("binary");
    let helpers: [&[u8]; 3] = [HELPER_GO_V1, HELPER_GO_V2, HELPER_GO_V2];
    for (i, (author, helper)) in [ALICE, BOB, ALICE].into_iter().zip(helpers).enumerate() {
        let n = i as i64 + 9;
        CommitBuilder::new(&test, &format!("c{}", n + 1), at(n))
            .author(author.0, author.1)
            .write("c.txt", &lines(i + 3))
            .write("t.go", helper)
            .commit()
            .expect("tail commit");
    }
    test
}

/// Every output of one run: `stdout` for the stream formats, every written
/// file (sorted by name) for `plot`.
fn rendered(
    id: &str,
    label: &str,
    flags: &[&str],
    repo: &str,
    chunk: Option<&str>,
) -> Vec<(String, Vec<u8>)> {
    let plot_dir = tempfile::tempdir().expect("plot dir");
    let plot_out = plot_dir.path().to_str().expect("utf-8 dir");
    let mut args = vec!["-a", id, "--max-changes-per-commit", CHUNK_FIXTURE_CAP];
    args.extend_from_slice(flags);
    if let Some(size) = chunk {
        args.extend_from_slice(&["--chunk-size", size]);
    }
    if label == PLOT {
        args.extend_from_slice(&["--output", plot_out]);
    }
    args.push(repo);
    let stdout = stdout_ok(&args);
    if label == PLOT {
        return dir_files(plot_dir.path());
    }
    vec![("stdout".to_string(), stdout)]
}

/// Every file under `dir`, recursively, as `(relative path, bytes)` sorted by
/// path.
fn dir_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in std::fs::read_dir(&next).expect("read plot dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let name = path
                    .strip_prefix(dir)
                    .expect("under the plot dir")
                    .to_string_lossy()
                    .into_owned();
                files.push((name, std::fs::read(&path).expect("read plot file")));
            }
        }
    }
    files.sort();
    files
}

/// The `(selection, format, chunk size)` triples whose output differs from
/// the unchunked output of the same selection and format; every unchunked
/// output must be non-empty.
fn chunk_defects(selection: &str, repo: &str) -> Vec<String> {
    let mut defects = Vec::new();
    for (label, flags) in FORMATS {
        let whole = rendered(selection, label, flags, repo, None);
        assert!(
            whole.iter().all(|(_, bytes)| !bytes.is_empty()) && !whole.is_empty(),
            "{selection} {label}: empty output"
        );
        for size in CHUNK_SIZES {
            let chunked = rendered(selection, label, flags, repo, Some(size));
            if chunked != whole {
                defects.push(format!("{selection} {label} --chunk-size {size}"));
            }
        }
    }
    defects
}

/// For every chunked analyzer and every advertised format, the output with
/// each `--chunk-size` equals the unchunked output byte for byte. The
/// analyzers run concurrently, one thread each.
#[test]
fn chunked_output_equals_the_unchunked_output() {
    let test = chunk_fixture();
    let p = path(&test);
    let defects: Vec<String> = std::thread::scope(|s| {
        let runs: Vec<_> = CHUNKED_IDS
            .into_iter()
            .map(|id| s.spawn(move || chunk_defects(id, p)))
            .collect();
        runs.into_iter()
            .flat_map(|run| run.join().expect("chunk matrix thread"))
            .collect()
    });
    assert!(
        defects.is_empty(),
        "outputs changed by chunking:\n{}",
        defects.join("\n")
    );
}

/// The co-selected UAST analyzers share ONE chunked pass: every format of
/// the combined run is byte-identical for every `--chunk-size`.
#[test]
fn the_shared_uast_walk_output_does_not_depend_on_the_chunk_size() {
    let test = chunk_fixture();
    let defects = chunk_defects(UAST_CO_SELECTION, path(&test));
    assert!(
        defects.is_empty(),
        "outputs changed by chunking:\n{}",
        defects.join("\n")
    );
}

/// The format table above covers every format `--help` advertises (plus the
/// `--ndjson` modifier), and `--help` documents `--chunk-size`.
#[test]
fn the_chunk_matrix_covers_every_advertised_format() {
    let out = common::command(BIN)
        .args(["run", "--help"])
        .output()
        .expect("run --help");
    let help = String::from_utf8_lossy(&out.stdout);
    let line = help
        .lines()
        .find_map(|l| l.trim().strip_prefix("Output format: "))
        .unwrap_or_else(|| panic!("no `Output format:` line in:\n{help}"));
    let mut advertised: Vec<&str> = line.split(',').map(str::trim).collect();
    advertised.sort_unstable();
    let mut covered = ADVERTISED_FORMATS.to_vec();
    covered.sort_unstable();
    assert_eq!(advertised, covered, "{help}");
    for format in ADVERTISED_FORMATS {
        assert!(
            FORMATS.iter().any(|(label, _)| *label == format),
            "{format} is not in the matrix"
        );
    }
    assert!(help.contains("--chunk-size"), "{help}");
}

/// A `--chunk-size` below 1 is a named error before any walk: exit 1, empty
/// stdout.
#[test]
fn an_out_of_range_chunk_size_is_a_named_error() {
    let test = chunk_fixture();
    let p = path(&test);
    for bad in ["0", "-5"] {
        let flag = format!("--chunk-size={bad}");
        let out = codefang(&["-a", "history/devs", &flag, p]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{bad}: {stderr}");
        assert!(out.stdout.is_empty(), "{bad}: stdout must stay empty");
        assert!(
            stderr.contains(&format!(
                "Error: invalid --chunk-size: must be >= 1, got {bad}"
            )),
            "{bad}: {stderr}"
        );
    }
}

/// The smallest `--memory-budget`, which plans the smallest chunks and the
/// fewest workers.
const MIN_BUDGET: &str = "256MiB";
/// [`MIN_BUDGET`] in MiB.
const MIN_BUDGET_MIB: u64 = 256;

/// A `--memory-budget` that is not a size, or is below the minimum, is a
/// named error before any walk: exit 1, empty stdout.
#[test]
fn an_invalid_memory_budget_is_a_named_error() {
    let test = chunk_fixture();
    let p = path(&test);
    for (bad, why) in [
        ("1MB", "is below the minimum 256 MiB"),
        ("banana", "is not a size"),
        ("", "is not a size"),
    ] {
        let flag = format!("--memory-budget={bad}");
        let out = codefang(&["-a", "history/devs", &flag, p]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{bad}: {stderr}");
        assert!(out.stdout.is_empty(), "{bad}: stdout must stay empty");
        assert!(
            stderr.contains(&format!("Error: invalid --memory-budget: {bad:?} {why}")),
            "{bad}: {stderr}"
        );
    }
}

/// The budget only plans the walk: every chunked analyzer renders the same
/// bytes at the smallest budget as with the automatic one.
#[test]
fn the_memory_budget_never_changes_the_output() {
    let test = chunk_fixture();
    let p = path(&test);
    let mut defects = Vec::new();
    for id in CHUNKED_IDS {
        for (label, flags) in FORMATS.iter().filter(|(label, _)| *label != PLOT) {
            let auto = rendered(id, label, flags, p, None);
            let mut budgeted = flags.to_vec();
            budgeted.extend_from_slice(&["--memory-budget", MIN_BUDGET]);
            if rendered(id, label, &budgeted, p, None) != auto {
                defects.push(format!("{id} {label}"));
            }
        }
    }
    assert!(
        defects.is_empty(),
        "outputs changed by the budget:\n{}",
        defects.join("\n")
    );
}

/// `-v` prints each walk's plan on stderr: the chunk, the chunk count, the
/// workers, and the walk's share of the budget or the `--chunk-size` it
/// came from.
#[test]
fn verbose_runs_print_each_walk_plan() {
    let test = chunk_fixture();
    let p = path(&test);
    for id in CHUNKED_IDS {
        let budgeted = codefang(&["-v", "-a", id, "--memory-budget", MIN_BUDGET, p]);
        let stderr = String::from_utf8_lossy(&budgeted.stderr);
        assert!(budgeted.status.success(), "{id}: {stderr}");
        // The walk's share: the budget less libgit2's part, never more than
        // the budget.
        let share = stderr
            .lines()
            .filter(|l| l.starts_with(&format!("progress: {id} plan: ")))
            .find_map(|l| {
                l.strip_suffix(" MiB)")?
                    .rsplit_once("(budget ")?
                    .1
                    .parse::<u64>()
                    .ok()
            });
        assert!(
            matches!(share, Some(mib) if mib > 0 && mib <= MIN_BUDGET_MIB),
            "{id}: {stderr}"
        );
        let sized = codefang(&["-v", "-a", id, "--chunk-size", "2", p]);
        let stderr = String::from_utf8_lossy(&sized.stderr);
        assert!(
            stderr.lines().any(|l| l
                .starts_with(&format!("progress: {id} plan: 2 commits per chunk, "))
                && l.ends_with("(--chunk-size)")),
            "{id}: {stderr}"
        );
        let quiet = codefang(&["-a", id, "--chunk-size", "2", p]);
        assert!(
            !String::from_utf8_lossy(&quiet.stderr).contains(" plan: "),
            "{id}: the plan is printed only with -v"
        );
    }
}

/// The `"analyzer"` field of one ndjson record.
fn ndjson_analyzer(line: &str) -> String {
    let record: serde_json::Value = serde_json::from_str(line).expect("ndjson record");
    record["analyzer"]
        .as_str()
        .expect("analyzer field")
        .to_owned()
}

/// With another history walk selected, the UAST pass's results wait on disk
/// (`-v` names each spill) and every UAST analyzer still streams exactly
/// the records its own run streams; a UAST-only selection keeps its results
/// in memory.
#[test]
fn spilled_uast_results_stream_the_same_records() {
    let test = chunk_fixture();
    let p = path(&test);
    let mixed_ids = format!("{UAST_CO_SELECTION},history/devs");
    let mixed = codefang(&["-v", "-a", &mixed_ids, "--format", "ndjson", p]);
    let stderr = String::from_utf8_lossy(&mixed.stderr);
    assert!(mixed.status.success(), "{stderr}");
    assert!(stderr.contains("results spilled to disk"), "{stderr}");
    let mixed_out = String::from_utf8(mixed.stdout).expect("utf-8");
    for id in UAST_CO_SELECTION.split(',') {
        let alone =
            String::from_utf8(stdout_ok(&["-a", id, "--format", "ndjson", p])).expect("utf-8");
        let alone: Vec<&str> = alone.lines().collect();
        let Some(first) = alone.first() else {
            continue;
        };
        let name = ndjson_analyzer(first);
        let in_mixed: Vec<&str> = mixed_out
            .lines()
            .filter(|l| ndjson_analyzer(l) == name)
            .collect();
        assert_eq!(in_mixed, alone, "{id}");
    }
    let uast_only = codefang(&["-v", "-a", UAST_CO_SELECTION, p]);
    assert!(uast_only.status.success());
    assert!(
        !String::from_utf8_lossy(&uast_only.stderr).contains("spilled"),
        "a UAST-only run keeps its results in memory"
    );
}
