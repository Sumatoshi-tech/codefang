//! End-to-end coverage of the `codefang` command surface, driven through the
//! **real binary** (`specs/bugs/BUG-cli-inert-surface.md`, items 2 and 3).
//!
//! The contract (`site/guide/cli-reference.md`): every flag and subcommand
//! `--help` lists does what its help says; a flag with no implementation is
//! rejected as an unknown argument instead of being accepted and ignored.

mod common;

use std::path::Path;
use std::process::{Command, Output};

use cf_gitlib::testutil::{commit_files, init_repo, TestRepo};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// Base instant of the fixture history: 2021-01-01T00:00:00Z.
const T0: i64 = 1_609_459_200;
/// One day between fixture commits.
const DAY: i64 = 86_400;
/// A Go source file with one branching function.
const GO_SOURCE: &str =
    "package main\n\nfunc f(x int) int {\n\tif x > 0 {\n\t\treturn 1\n\t}\n\treturn 0\n}\n";
/// The ANSI escape introducer.
const ANSI_ESCAPE: char = '\u{1b}';

/// Runs `codefang <args>` with an empty `HOME`, `envs`, and `TMPDIR` set to
/// `tmp` when given.
fn codefang_in(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let home = tempfile::tempdir().expect("home");
    let mut cmd = common::command(BIN);
    cmd.args(args)
        .env("HOME", home.path())
        .env("TZ", "UTC")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env_remove("NO_COLOR");
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.output().expect("run the codefang binary")
}

fn codefang(args: &[&str]) -> Output {
    codefang_in(args, &[])
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("utf-8 stdout")
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// stdout of a successful run.
fn stdout_ok(args: &[&str]) -> String {
    let out = codefang(args);
    assert!(out.status.success(), "{args:?}: {}", stderr(&out));
    stdout(&out)
}

/// A directory holding one Go file.
fn go_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("main.go"), GO_SOURCE).expect("write go");
    dir
}

/// Three daily commits of a Go file.
fn go_repo() -> TestRepo {
    let test = init_repo().expect("init");
    for d in 0..3_i64 {
        let body = format!("{GO_SOURCE}// revision {d}\n");
        commit_files(&test, "edit", T0 + d * DAY, &[("main.go", body.as_bytes())]).expect("commit");
    }
    test
}

fn utf8(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}

/// Flags and subcommands with no implementation behind them are rejected.
#[test]
fn inert_flags_and_render_are_rejected() {
    let dir = go_dir();
    let d = utf8(dir.path());
    for flag in [
        "--checkpoint=false",
        "--checkpoint",
        "--checkpoint-dir=x",
        "--resume=false",
        "--clear-checkpoint",
        "--cache-dir=x",
        "--no-cache",
        "--debug-trace",
        "--skip-blacklist",
        "--blacklisted-prefixes=x",
    ] {
        let out = codefang(&["run", "-a", "static/complexity", flag, d]);
        assert_eq!(out.status.code(), Some(1), "{flag} must be rejected");
        assert!(out.stdout.is_empty(), "{flag}");
        assert!(stderr(&out).contains("unexpected argument"), "{flag}");
    }
    let out = codefang(&["render", d, "-o", d]);
    assert_eq!(out.status.code(), Some(1), "render is not a subcommand");
    assert!(stderr(&out).contains("unrecognized subcommand"));
}

#[test]
fn quiet_and_silent_suppress_the_progress_lines() {
    let dir = go_dir();
    let d = utf8(dir.path());
    let loud = codefang(&["run", "-a", "static/complexity", d]);
    assert!(stderr(&loud).contains("progress: run finished"));
    for flag in ["-q", "--quiet", "--silent"] {
        let out = codefang(&["run", flag, "-a", "static/complexity", d]);
        assert!(out.status.success(), "{flag}");
        assert_eq!(stderr(&out), "", "{flag}");
        assert_eq!(stdout(&out), stdout(&loud), "{flag} keeps the report");
    }
    // The global flag also works before the subcommand.
    let out = codefang(&["-q", "run", "-a", "static/complexity", d]);
    assert_eq!(stderr(&out), "");
}

#[test]
fn quiet_keeps_warnings_and_errors() {
    let dir = go_dir();
    let d = utf8(dir.path());
    let out = codefang(&[
        "run",
        "-q",
        "-a",
        "static/complexity",
        "--format",
        "yaml",
        "-F",
        d,
    ]);
    assert!(out.status.success());
    assert_eq!(
        stderr(&out),
        "--per-file applies to --format json only; ignored for yaml\n"
    );
    let out = codefang(&["run", "-q", "-a", "static/complexity", "/no/such/dir"]);
    assert_eq!(stderr(&out), "Error: path not found: /no/such/dir\n");
}

#[test]
fn verbose_names_the_resolved_analyzers() {
    let test = go_repo();
    let p = utf8(test.path());
    let out = codefang(&["-v", "run", "-a", "history/devs,static/complexity", p]);
    assert!(out.status.success(), "{}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("progress: analyzers: static/complexity,history/devs"),
        "{err}"
    );
    assert!(err.contains("progress: config file: none"), "{err}");
    let quiet = codefang(&["run", "-a", "history/devs,static/complexity", p]);
    assert!(!stderr(&quiet).contains("progress: analyzers:"));
}

#[test]
fn verbose_and_quiet_together_is_a_usage_error() {
    let dir = go_dir();
    let out = codefang(&["-v", "run", "-q", utf8(dir.path())]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(stderr(&out).contains("Error: --verbose and --quiet cannot be used together"));
}

#[test]
fn output_writes_the_report_to_a_file() {
    let test = go_repo();
    let p = utf8(test.path());
    let out_dir = tempfile::tempdir().expect("out");
    for args in [
        vec!["-a", "static/complexity", "--format", "json"],
        vec!["-a", "history/devs", "--format", "yaml"],
        vec![
            "-a",
            "history/devs,history/anomaly",
            "--format",
            "timeseries",
        ],
        vec!["-a", "history/devs", "--format", "timeseries", "--ndjson"],
    ] {
        let file = out_dir.path().join("report.out");
        let mut with_output = vec!["run", "-q"];
        with_output.extend_from_slice(&args);
        with_output.extend_from_slice(&["-o", utf8(&file), p]);
        let out = codefang(&with_output);
        assert!(out.status.success(), "{args:?}: {}", stderr(&out));
        assert!(
            out.stdout.is_empty(),
            "{args:?}: the report went to the file"
        );

        let mut plain = vec!["run", "-q"];
        plain.extend_from_slice(&args);
        plain.push(p);
        let written = std::fs::read_to_string(&file).expect("report file");
        assert_eq!(written, stdout_ok(&plain), "{args:?}");
    }
}

#[test]
fn an_unwritable_output_is_an_error() {
    let dir = go_dir();
    let out = codefang(&[
        "run",
        "-a",
        "static/complexity",
        "-o",
        "/no/such/dir/report.json",
        utf8(dir.path()),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(
        stderr(&out).contains("Error: cannot write --output /no/such/dir/report.json: "),
        "{}",
        stderr(&out)
    );
}

#[test]
fn plot_still_takes_the_output_directory() {
    let test = go_repo();
    let out_dir = tempfile::tempdir().expect("out");
    let out = codefang(&[
        "run",
        "-a",
        "history/devs",
        "--format",
        "plot",
        "-o",
        utf8(out_dir.path()),
        utf8(test.path()),
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(out_dir.path().join("index.html").is_file());
}

#[test]
fn ndjson_without_timeseries_is_an_error() {
    let test = go_repo();
    let out = codefang(&["run", "-a", "history/devs", "--ndjson", utf8(test.path())]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert!(
        stderr(&out)
            .contains("Error: invalid --ndjson: requires --format timeseries, got --format json"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn typos_max_distance_must_be_positive() {
    let test = go_repo();
    let out = codefang(&[
        "run",
        "-a",
        "history/typos",
        "--typos-max-distance",
        "0",
        utf8(test.path()),
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("Error: invalid --typos-max-distance: must be > 0, got 0"));
}

#[test]
fn list_analyzers_prints_every_id_and_exits() {
    let text = stdout_ok(&["run", "--list-analyzers"]);
    let ids: Vec<&str> = text.lines().collect();
    assert!(ids.contains(&"static/complexity"), "{text}");
    assert!(ids.contains(&"history/devs"), "{text}");
    assert!(ids.iter().all(|id| id.contains('/')), "{text}");
    // It answers before any configuration is read.
    let out = codefang(&[
        "run",
        "--list-analyzers",
        "--config",
        "/no/such/config.yaml",
    ]);
    assert!(out.status.success());
}

#[test]
fn path_flag_and_positional_path_agree() {
    let dir = go_dir();
    let d = utf8(dir.path());
    let by_flag = stdout_ok(&["run", "-q", "-a", "static/complexity", "-p", d]);
    let by_positional = stdout_ok(&["run", "-q", "-a", "static/complexity", d]);
    assert_eq!(by_flag, by_positional);
    let empty = tempfile::tempdir().expect("empty");
    let other = stdout_ok(&[
        "run",
        "-q",
        "-a",
        "static/complexity",
        "-p",
        utf8(empty.path()),
    ]);
    assert_ne!(by_flag, other, "-p selects the analyzed folder");
}

#[test]
fn no_color_strips_ansi_from_text_output() {
    let dir = go_dir();
    let d = utf8(dir.path());
    let colored = stdout_ok(&[
        "run",
        "-q",
        "-a",
        "static/complexity",
        "--format",
        "text",
        d,
    ]);
    assert!(colored.contains(ANSI_ESCAPE), "the text renderer colors");
    let plain = stdout_ok(&[
        "run",
        "-q",
        "--no-color",
        "-a",
        "static/complexity",
        "--format",
        "text",
        d,
    ]);
    assert!(!plain.contains(ANSI_ESCAPE), "{plain}");
}

#[test]
fn version_names_the_crate_version() {
    let line = stdout_ok(&["version"]);
    let prefix = format!("codefang {} (commit: ", env!("CARGO_PKG_VERSION"));
    assert!(line.starts_with(&prefix), "{line}");
    assert!(line.ends_with(")\n"), "{line}");
    assert!(!line.contains("commit: none"), "{line}");
}

#[test]
fn profile_logs_memory_samples() {
    let dir = go_dir();
    let tmp = tempfile::tempdir().expect("tmp");
    let out = codefang_in(
        &[
            "--profile",
            "run",
            "-q",
            "-a",
            "static/complexity",
            utf8(dir.path()),
        ],
        &[("TMPDIR", utf8(tmp.path()))],
    );
    assert!(out.status.success());
    let err = stderr(&out);
    assert!(err.starts_with("MEM t=0 RSS="), "{err}");
    assert!(tmp.path().join("maps_baseline.txt").is_file());
}

/// The generated completion scripts parse and complete the live flag set.
#[test]
fn completion_scripts_are_valid_and_current() {
    for shell in ["bash", "zsh", "fish", "powershell"] {
        let script = stdout_ok(&["completion", shell]);
        assert!(script.contains("config"), "{shell}");
        assert!(script.contains("granularity"), "{shell}");
        assert!(!script.contains("checkpoint"), "{shell}");
        assert!(!script.contains("render"), "{shell}");
    }
    let bash = stdout_ok(&["completion", "bash"]);
    let check = Command::new("bash")
        .args(["-n", "-c", &bash])
        .output()
        .expect("bash -n");
    assert!(check.status.success(), "{}", stderr(&check));

    // --no-descriptions drops the help texts.
    let described = stdout_ok(&["completion", "zsh"]);
    let bare = stdout_ok(&["completion", "zsh", "--no-descriptions"]);
    assert!(described.contains("Analyzer IDs or glob patterns"));
    assert!(!bare.contains("Analyzer IDs or glob patterns"));
    assert!(bare.contains("--granularity"));
}
