//! The `uast` CLI surface contract, through the **real binary**: every flag
//! `uast <command> --help` lists does what its help says, and the flags that
//! never had a reader are rejected by the argument parser instead of being
//! accepted and ignored (`specs/bugs/BUG-cli-inert-surface.md`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Path to the freshly built `uast` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_uast");

/// A Go fixture with two function declarations.
const GO_SOURCE: &str = "package p\n\nfunc a() {}\n\nfunc b() {}\n";

/// The query every `--input` / positional comparison runs.
const FUNCTION_QUERY: &str = r#"filter(.type == "Function")"#;

/// The clap usage-error marker for an argument the command does not declare.
const UNEXPECTED_ARGUMENT: &str = "unexpected argument";

/// Runs the binary with `args` in `cwd`.
fn uast(cwd: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .env("NO_COLOR", "1")
        .output()
        .expect("run the uast binary")
}

/// Runs the binary and returns stdout, asserting a zero exit.
fn stdout_ok(cwd: &Path, args: &[&str]) -> String {
    let out = uast(cwd, args);
    assert!(
        out.status.success(),
        "uast {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

/// A scratch directory holding `m.go` (the Go fixture).
fn go_fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("scratch dir");
    let file = dir.path().join("m.go");
    std::fs::write(&file, GO_SOURCE).expect("write fixture");
    (dir, file)
}

#[test]
fn inert_flags_are_rejected() {
    let (dir, file) = go_fixture();
    let f = file.to_str().unwrap();
    let cases: &[&[&str]] = &[
        // Global flags with no reader.
        &["--config", "x.yaml", "version"],
        &["--verbose", "version"],
        &["-v", "version"],
        &["--quiet", "version"],
        &["-q", "version"],
        // `parse` flags with no reader.
        &["parse", "--progress", f],
        &["parse", "-p", f],
        &["parse", "--workers", "2", f],
        &["parse", "-w", "2", f],
        // `validate` color toggles (the output is plain text either way).
        &["validate", "--color", f],
        &["validate", "--no-color", f],
        // `mapping` coverage: the mapping rules were never loaded, so the
        // coverage was never computed.
        &["mapping", "--coverage", "--node-types", f],
        &["mapping", "--mapping", f, "--node-types", f],
    ];
    for args in cases {
        let out = uast(dir.path(), args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success() && stderr.contains(UNEXPECTED_ARGUMENT),
            "uast {args:?} must be rejected by the parser, got rc {:?}: {stderr}",
            out.status.code()
        );
    }
}

#[test]
fn query_input_flag_queries_the_file() {
    let (dir, file) = go_fixture();
    let f = file.to_str().unwrap();
    let positional = stdout_ok(dir.path(), &["query", FUNCTION_QUERY, f, "-f", "compact"]);
    let via_input = stdout_ok(
        dir.path(),
        &["query", FUNCTION_QUERY, "-i", f, "-f", "compact"],
    );
    assert!(
        positional.contains("\"results\":[{"),
        "the fixture has functions: {positional}"
    );
    assert_eq!(via_input, positional, "--input queries the same file");
}

#[test]
fn parse_format_help_lists_only_accepted_formats() {
    let (dir, file) = go_fixture();
    let help = stdout_ok(dir.path(), &["parse", "--help"]);
    assert!(help.contains("json, compact, none"), "{help}");
    assert!(!help.contains("tree"), "`tree` is rejected: {help}");
    let f = file.to_str().unwrap();
    for format in ["json", "compact", "none"] {
        stdout_ok(dir.path(), &["parse", "-f", format, f]);
    }
}

#[test]
fn show_treesitter_prints_the_raw_tree() {
    let (dir, file) = go_fixture();
    let f = file.to_str().unwrap();
    let out = stdout_ok(
        dir.path(),
        &["mapping", "--show-treesitter", "--language", "go", f],
    );
    let header = format!("=== Tree-sitter JSON for {f} (language: go) ===\n");
    let body = out
        .strip_prefix(header.as_str())
        .unwrap_or_else(|| panic!("header missing: {out}"));
    let tree: serde_json::Value = serde_json::from_str(body).expect("one JSON document");
    assert_eq!(tree["type"], "source_file");
    assert_eq!(tree["named"], true);
    assert_eq!(tree["start_byte"], 0);
    assert_eq!(tree["end_byte"], GO_SOURCE.len());
    let kinds: Vec<&str> = tree["children"]
        .as_array()
        .expect("named children")
        .iter()
        .filter_map(|c| c["type"].as_str())
        .collect();
    assert_eq!(
        kinds,
        [
            "package_clause",
            "function_declaration",
            "function_declaration"
        ]
    );
    assert_eq!(tree["children"][1]["text"], "func a() {}");
    assert_eq!(tree["children"][1]["start_pos"]["row"], 2);
}

#[test]
fn show_treesitter_names_a_missing_or_unknown_language() {
    let (dir, file) = go_fixture();
    let f = file.to_str().unwrap();
    let missing = uast(dir.path(), &["mapping", "--show-treesitter", f]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&missing.stderr)
        .contains("tree-sitter parsing requires a language to be set"));
    let unknown = uast(
        dir.path(),
        &["mapping", "--show-treesitter", "--language", "cobol", f],
    );
    assert_eq!(unknown.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&unknown.stderr).contains("unsupported language: cobol"),
        "{}",
        String::from_utf8_lossy(&unknown.stderr)
    );
}

#[test]
fn completion_scripts_cover_every_subcommand_and_parse() {
    let (dir, _file) = go_fixture();
    let bash = stdout_ok(dir.path(), &["completion", "bash"]);
    for sub in [
        "parse", "diff", "query", "explore", "analyze", "version", "validate", "mapping",
    ] {
        assert!(bash.contains(sub), "bash completion lacks `{sub}`");
    }
    assert!(
        !bash.contains("--progress"),
        "removed flags are not completed"
    );
    let script = dir.path().join("uast.bash");
    std::fs::write(&script, &bash).expect("write script");
    let syntax = Command::new("bash")
        .arg("-n")
        .arg(&script)
        .output()
        .expect("run bash -n");
    assert!(
        syntax.status.success(),
        "bash -n: {}",
        String::from_utf8_lossy(&syntax.stderr)
    );
    let zsh = stdout_ok(dir.path(), &["completion", "zsh"]);
    assert!(zsh.starts_with("#compdef uast"), "{zsh}");
    let fish = stdout_ok(dir.path(), &["completion", "fish"]);
    assert!(fish.contains("complete -c uast"), "{fish}");
}

#[test]
fn validate_uses_the_embedded_schema_by_default() {
    let (dir, file) = go_fixture();
    let help = stdout_ok(dir.path(), &["validate", "--help"]);
    assert!(
        !help.contains("pkg/uast/spec"),
        "the default is the embedded schema, not a Go-tree path: {help}"
    );
    let tree = stdout_ok(dir.path(), &["parse", "-f", "json", file.to_str().unwrap()]);
    let doc = dir.path().join("m.uast.json");
    std::fs::write(&doc, tree).expect("write UAST document");
    let report = stdout_ok(dir.path(), &["validate", doc.to_str().unwrap()]);
    assert!(report.starts_with("UAST is valid"), "{report}");
}

#[test]
fn output_file_keeps_every_input() {
    let (dir, a) = go_fixture();
    let b = dir.path().join("n.go");
    std::fs::write(&b, GO_SOURCE).expect("write second fixture");
    let (a, b) = (a.to_str().unwrap(), b.to_str().unwrap());
    let parsed = dir.path().join("parsed.jsonl");
    stdout_ok(
        dir.path(),
        &[
            "parse",
            "-f",
            "compact",
            "-o",
            parsed.to_str().unwrap(),
            a,
            b,
        ],
    );
    let parsed = std::fs::read_to_string(&parsed).expect("parse output file");
    assert_eq!(
        parsed.lines().count(),
        2,
        "one document per input: {parsed}"
    );
    let counted = dir.path().join("counts.txt");
    stdout_ok(
        dir.path(),
        &[
            "query",
            FUNCTION_QUERY,
            a,
            b,
            "-f",
            "count",
            "-o",
            counted.to_str().unwrap(),
        ],
    );
    let counts = std::fs::read_to_string(&counted).expect("query output file");
    assert_eq!(counts, "2\n2\n", "one count per input");
}

/// Runs the binary with `stdin` piped in and returns stdout (zero exit).
fn stdout_with_stdin(cwd: &Path, args: &[&str], stdin: &str) -> String {
    use std::io::Write as _;
    use std::process::Stdio;
    let mut child = Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the uast binary");
    child
        .stdin
        .take()
        .expect("stdin pipe")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    let out = child.wait_with_output().expect("wait for uast");
    assert!(
        out.status.success(),
        "uast {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

#[test]
fn interactive_query_explores_the_named_file() {
    let (dir, file) = go_fixture();
    let f = file.to_str().unwrap();
    let session = format!("{FUNCTION_QUERY}\nquit\n");
    let positional: &[&str] = &["query", "-t", f];
    let via_input: &[&str] = &["query", "-t", "-i", f];
    for args in [positional, via_input] {
        let out = stdout_with_stdin(dir.path(), args, &session);
        assert!(out.contains("Found 2 results:"), "uast {args:?}: {out}");
    }
    let two = uast(dir.path(), &["query", "-t", f, f]);
    assert_eq!(
        two.status.code(),
        Some(1),
        "interactive mode takes one file"
    );
}

#[test]
fn dash_reads_stdin() {
    let (dir, file) = go_fixture();
    let f = file.to_str().unwrap();
    let from_stdin = stdout_with_stdin(dir.path(), &["parse", "-f", "compact", "-"], GO_SOURCE);
    let from_file = stdout_ok(dir.path(), &["parse", "-f", "compact", f]);
    assert_eq!(from_stdin, from_file, "`-` parses stdin as the source");
    let tree = stdout_ok(dir.path(), &["parse", "-f", "json", f]);
    let piped = stdout_with_stdin(
        dir.path(),
        &["query", FUNCTION_QUERY, "-", "-f", "count"],
        &tree,
    );
    assert_eq!(piped, "2\n", "`-` queries the UAST JSON on stdin");
}
