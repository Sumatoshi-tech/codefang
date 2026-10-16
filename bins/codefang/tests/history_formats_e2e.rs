//! End-to-end coverage of the history OUTPUT layer, driven through the **real
//! binary** over a real on-disk git fixture
//! (`specs/bugs/BUG-history-text-format.md`).
//!
//! The contract: every `history/*` analyzer renders in every format
//! `codefang run --help` advertises — `json`, `yaml`, `bin`, `timeseries`,
//! `timeseries --ndjson`, `ndjson`, `text`, `compact`, `plot` — with exit 0
//! and a non-empty result, and a multi-analyzer selection renders ONE output
//! (one text section, one compact line, one timeseries line per commit) per
//! analyzer. A failing run never writes a partial report to stdout.

mod common;

use std::path::Path;

use cf_gitlib::testutil::{commit_files_as, init_repo, TestRepo};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture history: 2021-01-01T00:00:00Z.
const T0: i64 = 1_609_459_200;
/// One day between fixture commits (one default tick each).
const DAY: i64 = 24 * 3_600;
/// The fixture developers.
const ALICE: (&str, &str) = ("alice", "alice@example.com");
const BOB: (&str, &str) = ("bob", "bob@example.com");
/// Number of commits in [`fixture`].
const FIXTURE_COMMITS: usize = 3;

/// Every history analyzer id with its plot page name (`<page>.html`).
const ANALYZERS: [(&str, &str); 10] = [
    ("history/burndown", "burndown"),
    ("history/devs", "devs"),
    ("history/couples", "couples"),
    ("history/file-history", "file-history"),
    ("history/shotness", "shotness"),
    ("history/quality", "quality"),
    ("history/sentiment", "sentiment"),
    ("history/typos", "typos"),
    ("history/anomaly", "anomaly"),
    ("history/imports", "imports-per-dev"),
];

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

/// One requested output: a label and the flags that select it. The
/// `--ndjson` modifier turns `timeseries` into one JSON line per commit.
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

/// Go source before the typo fix: one documented function, one import.
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

/// Go source after Bob's commit: the typo `recieve` → `receive` is fixed on
/// the same lines, a second import and a negative comment appear.
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

/// The Python helper with a second function.
const UTIL_PY_V3: &[u8] = b"import os
# nice and happy code
def helper(a):
    if a:
        return os.getcwd()
    return None
def other(): return 1
";

/// Three commits by two developers over Go and Python sources: a typo fix,
/// imports, positive and negative comments, functions, co-changed files.
fn fixture() -> TestRepo {
    let test = init_repo().expect("init");
    commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c1",
        T0,
        &[("main.go", MAIN_GO_V1), ("util.py", b"lib\n")],
    )
    .expect("c1");
    commit_files_as(
        &test,
        BOB.0,
        BOB.1,
        "c2",
        T0 + 4 * DAY,
        &[("main.go", MAIN_GO_V2), ("util.py", UTIL_PY_V2)],
    )
    .expect("c2");
    commit_files_as(
        &test,
        ALICE.0,
        ALICE.1,
        "c3",
        T0 + 9 * DAY,
        &[("util.py", UTIL_PY_V3), ("notes.txt", b"x\n")],
    )
    .expect("c3");
    test
}

/// Runs `codefang run <args>` with a deterministic environment.
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

fn path(test: &TestRepo) -> &str {
    test.path().to_str().expect("utf-8 path")
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

/// A file under `dir` that exists and is non-empty.
fn non_empty_file(dir: &Path, name: &str) -> bool {
    std::fs::metadata(dir.join(name)).is_ok_and(|m| m.len() > 0)
}

/// What is wrong with one `(analyzer, format)` run, or `None` when it
/// rendered: exit 0, no `Error:` on stderr, and a non-empty result that
/// parses as the format promises (plot: the page and the index on disk).
fn format_defect(id: &str, page: &str, label: &str, flags: &[&str], repo: &str) -> Option<String> {
    let plot_dir = tempfile::tempdir().expect("plot dir");
    let plot_out = plot_dir.path().to_str().expect("utf-8 dir");
    let mut args = vec!["-a", id];
    args.extend_from_slice(flags);
    if label == "plot" {
        args.extend_from_slice(&["--output", plot_out]);
    }
    args.push(repo);
    let out = codefang(&args);
    let stderr = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() || stderr.contains("Error:") {
        return Some(format!(
            "{id} {label}: exit {:?}, stdout {} bytes, stderr: {stderr}",
            out.status.code(),
            out.stdout.len()
        ));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let ok = match label {
        "plot" => {
            non_empty_file(plot_dir.path(), "index.html")
                && non_empty_file(plot_dir.path(), &format!("{page}.html"))
        }
        "json" | "timeseries" => serde_json::from_str::<serde_json::Value>(&stdout).is_ok(),
        "ndjson" | "timeseries+ndjson" => {
            !stdout.is_empty()
                && stdout
                    .lines()
                    .all(|l| serde_json::from_str::<serde_json::Value>(l).is_ok())
        }
        "text" => stdout.contains(&format!("{id}:\n")),
        "compact" => stdout.lines().count() == 1 && stdout.starts_with(id),
        _ => !out.stdout.is_empty(),
    };
    (!ok).then(|| format!("{id} {label}: unexpected output:\n{stdout}"))
}

/// Runs `id` in every advertised format over the fixture and fails with the
/// full list of defects.
fn assert_every_format(id: &str) {
    let test = fixture();
    let page = ANALYZERS
        .iter()
        .find(|(a, _)| *a == id)
        .map(|(_, p)| *p)
        .expect("known analyzer");
    let defects: Vec<String> = FORMATS
        .iter()
        .filter_map(|(label, flags)| format_defect(id, page, label, flags, path(&test)))
        .collect();
    assert!(defects.is_empty(), "{}", defects.join("\n"));
}

#[test]
fn burndown_renders_every_advertised_format() {
    assert_every_format("history/burndown");
}

#[test]
fn devs_renders_every_advertised_format() {
    assert_every_format("history/devs");
}

#[test]
fn couples_renders_every_advertised_format() {
    assert_every_format("history/couples");
}

#[test]
fn file_history_renders_every_advertised_format() {
    assert_every_format("history/file-history");
}

#[test]
fn shotness_renders_every_advertised_format() {
    assert_every_format("history/shotness");
}

#[test]
fn quality_renders_every_advertised_format() {
    assert_every_format("history/quality");
}

#[test]
fn sentiment_renders_every_advertised_format() {
    assert_every_format("history/sentiment");
}

#[test]
fn typos_renders_every_advertised_format() {
    assert_every_format("history/typos");
}

#[test]
fn anomaly_renders_every_advertised_format() {
    assert_every_format("history/anomaly");
}

#[test]
fn imports_renders_every_advertised_format() {
    assert_every_format("history/imports");
}

/// The matrix above covers exactly the formats `--help` advertises (plus the
/// `--ndjson` modifier), so a newly advertised format cannot ship untested.
#[test]
fn the_matrix_covers_every_format_the_help_advertises() {
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
    assert!(help.contains("--ndjson"), "{help}");
}

/// The text body of each analyzer is a human summary of ITS report: the
/// four analyzers that had no text renderer name their key findings.
#[test]
fn text_summaries_name_the_key_findings() {
    let test = fixture();
    let p = path(&test);
    let text = |id: &str| stdout_ok(&["-a", id, "--format", "text", p]);

    let typos = text("history/typos");
    assert!(
        typos.contains("recieve") && typos.contains("receive"),
        "{typos}"
    );
    assert!(typos.contains("main.go:9"), "{typos}");

    let imports = text("history/imports");
    for needle in ["fmt", "os", "alice", "bob"] {
        assert!(
            imports.contains(needle),
            "imports lacks {needle}:\n{imports}"
        );
    }

    let quality = text("history/quality");
    assert!(quality.contains("Files Analyzed"), "{quality}");
    assert!(quality.contains("Complexity"), "{quality}");

    let anomaly = text("history/anomaly");
    assert!(anomaly.contains("Ticks Analyzed"), "{anomaly}");
    assert!(anomaly.contains("Anomalies"), "{anomaly}");
}

/// `-a 'history/*' --format text`: the header once, then one section per
/// analyzer, exit 0.
#[test]
fn multi_analyzer_text_renders_one_section_per_analyzer() {
    let test = fixture();
    let out = stdout_ok(&["-a", "history/*", "--format", "text", path(&test)]);
    assert_eq!(out.matches("codefang (v2):").count(), 1, "{out}");
    assert!(out.starts_with("codefang (v2):\n"), "{out}");
    for (id, _) in ANALYZERS {
        assert_eq!(
            out.lines().filter(|l| *l == format!("{id}:")).count(),
            1,
            "{id} section:\n{out}"
        );
    }
}

/// `-a 'history/*' --format compact`: exactly one line per analyzer, each
/// led by the analyzer id.
#[test]
fn multi_analyzer_compact_is_one_line_per_analyzer() {
    let test = fixture();
    let out = stdout_ok(&["-a", "history/*", "--format", "compact", path(&test)]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), ANALYZERS.len(), "{out}");
    for (id, _) in ANALYZERS {
        assert_eq!(
            lines.iter().filter(|l| l.starts_with(id)).count(),
            1,
            "{id}:\n{out}"
        );
    }
}

/// `--format timeseries --ndjson` is the timeseries document's `commits`
/// array, one line per commit with every selected analyzer merged into it.
#[test]
fn timeseries_ndjson_is_the_merged_commit_array() {
    let test = fixture();
    let p = path(&test);
    let selection = "history/devs,history/couples,history/anomaly";
    let doc: serde_json::Value =
        serde_json::from_str(&stdout_ok(&["-a", selection, "--format", "timeseries", p]))
            .expect("timeseries json");
    let lines: Vec<serde_json::Value> =
        stdout_ok(&["-a", selection, "--format", "timeseries", "--ndjson", p])
            .lines()
            .map(|l| serde_json::from_str(l).expect("ndjson line"))
            .collect();
    assert_eq!(lines.len(), FIXTURE_COMMITS, "{lines:?}");
    assert_eq!(&serde_json::Value::Array(lines.clone()), &doc["commits"]);
    for line in &lines {
        for key in ["hash", "tick", "timestamp", "devs", "couples", "anomaly"] {
            assert!(line.get(key).is_some(), "{key} missing in {line}");
        }
    }
}

/// A run that cannot produce its report writes NOTHING to stdout.
#[test]
fn a_failing_text_run_writes_nothing_to_stdout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = codefang(&[
        "-a",
        "history/*",
        "--format",
        "text",
        dir.path().to_str().expect("utf-8"),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty(), "{:?}", out.stdout);
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("Error: cannot open repository"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
