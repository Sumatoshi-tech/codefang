//! End-to-end coverage of function-free (data) files and the UAST parse size
//! limit, through the real `codefang` binary.
//!
//! - A data file (JSON, YAML, ...) has no function-like constructs. The
//!   function analyzers do not build its tree, but it must still appear in
//!   `--per-file` output with zero functions, exactly as before.
//! - A source file over the parse size limit is skipped with a stderr warning
//!   that names it; `--max-parse-file-size` raises the limit.

mod common;

use cf_gitlib::testutil::{commit_files, init_repo, TestRepo};

/// Path to the freshly built `codefang` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_codefang");
/// 2021-01-01T12:00:00Z.
const T0: i64 = 1_609_502_400;
/// One MiB in bytes.
const MIB: usize = 1024 * 1024;
/// Default parse limit, in MiB.
const DEFAULT_LIMIT_MIB: usize = 8;

fn run_json(args: &[&str]) -> (serde_json::Value, String) {
    let out = common::command(BIN)
        .arg("run")
        .args(args)
        .env("TZ", "UTC")
        .env("NO_COLOR", "1")
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .output()
        .expect("run the codefang binary");
    assert!(
        out.status.success(),
        "run failed ({args:?}): {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let doc = serde_json::from_slice(&out.stdout).expect("stdout is one JSON document");
    (doc, String::from_utf8_lossy(&out.stderr).into_owned())
}

/// The `Total Functions` metric value in a report section or file entry.
fn total_functions(entry: &serde_json::Value) -> String {
    entry["metrics"]
        .as_array()
        .expect("metrics array")
        .iter()
        .find(|m| m["label"] == "Total Functions")
        .and_then(|m| m["value"].as_str())
        .expect("Total Functions metric")
        .to_string()
}

/// A git repository whose working tree holds `files`: the static walk reads
/// the checkout, not the object database.
fn repo_with(files: &[(&str, &[u8])]) -> TestRepo {
    let test = init_repo().expect("init repo");
    for (name, body) in files {
        std::fs::write(test.path().join(name), body).expect("write fixture file");
    }
    commit_files(&test, "init", T0, files).expect("commit");
    test
}

#[test]
fn data_file_is_listed_per_file_with_zero_functions() {
    let test = repo_with(&[
        (
            "main.go",
            b"package main\n\nfunc add(a, b int) int {\n\treturn a + b\n}\n",
        ),
        ("config.json", b"{\"name\": \"x\", \"items\": [1, 2, 3]}\n"),
        ("settings.yaml", b"key: value\nlist:\n  - one\n"),
    ]);
    let path = test.path().to_str().expect("utf-8 path");
    let (doc, _) = run_json(&[
        "-a",
        "static/complexity",
        "--per-file",
        "--format",
        "json",
        path,
    ]);

    let files = doc["sections"][0]["files"]
        .as_array()
        .expect("per-file entries");
    for data in ["config.json", "settings.yaml"] {
        let entry = files
            .iter()
            .find(|f| f["file_path"] == data)
            .unwrap_or_else(|| panic!("{data} must appear per file: {files:?}"));
        assert_eq!(total_functions(entry), "0", "{data} has no functions");
        assert_eq!(entry["status"], "No functions found", "{data}");
    }
    let go = files
        .iter()
        .find(|f| f["file_path"] == "main.go")
        .expect("main.go entry");
    assert_eq!(total_functions(go), "1");
}

#[test]
fn source_file_over_the_parse_limit_is_skipped_with_a_warning() {
    let mut big = b"package main\n\nfunc big() int {\n\treturn 1\n}\n// ".to_vec();
    big.resize(DEFAULT_LIMIT_MIB * MIB + 1, b'x');
    let test = repo_with(&[
        (
            "main.go",
            b"package main\n\nfunc small() int {\n\treturn 2\n}\n",
        ),
        ("big.go", &big),
    ]);
    let path = test.path().to_str().expect("utf-8 path");
    let (doc, stderr) = run_json(&["-a", "static/complexity", "--format", "json", path]);

    assert!(
        stderr.contains("warning: skipped") && stderr.contains("big.go"),
        "the oversized file is named on stderr: {stderr}"
    );
    assert!(
        stderr.contains("parse size limit"),
        "the warning names the limit: {stderr}"
    );
    assert_eq!(
        total_functions(&doc["sections"][0]),
        "1",
        "only main.go is analyzed"
    );
}

#[test]
fn max_parse_file_size_flag_raises_the_limit() {
    let mut big = b"package main\n\nfunc big() int {\n\treturn 1\n}\n// ".to_vec();
    big.resize(DEFAULT_LIMIT_MIB * MIB + 1, b'x');
    let test = repo_with(&[
        (
            "main.go",
            b"package main\n\nfunc small() int {\n\treturn 2\n}\n",
        ),
        ("big.go", &big),
    ]);
    let path = test.path().to_str().expect("utf-8 path");
    let (doc, stderr) = run_json(&[
        "-a",
        "static/complexity",
        "--max-parse-file-size",
        "16",
        "--format",
        "json",
        path,
    ]);

    assert!(
        !stderr.contains("big.go"),
        "a file under the raised limit is not skipped: {stderr}"
    );
    assert_eq!(total_functions(&doc["sections"][0]), "2");
}

/// The `issues` names of a per-file entry, sorted.
fn issue_names(entry: &serde_json::Value) -> Vec<String> {
    let mut names: Vec<String> = entry["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .map(|i| i["name"].as_str().expect("issue name").to_string())
        .collect();
    names.sort();
    names
}

#[test]
fn proto_imports_are_still_reported_from_a_data_typed_file() {
    // enry types Protocol Buffer as data, but its mapping yields Import nodes,
    // so the import analyzer must still parse it.
    let test = repo_with(&[
        ("api.proto", b"syntax = \"proto3\";\nimport \"google/api.proto\";\nimport \"other.proto\";\nmessage M { int32 a = 1; }\n"),
        ("config.yaml", b"a: 1\n"),
    ]);
    let path = test.path().to_str().expect("utf-8 path");
    let (doc, _) = run_json(&[
        "-a",
        "static/imports",
        "--per-file",
        "--format",
        "json",
        path,
    ]);

    let files = doc["sections"][0]["files"]
        .as_array()
        .expect("per-file entries");
    let proto = files
        .iter()
        .find(|f| f["file_path"] == "api.proto")
        .expect("api.proto");
    assert_eq!(issue_names(proto), ["google/api.proto", "other.proto"]);
    let yaml = files
        .iter()
        .find(|f| f["file_path"] == "config.yaml")
        .expect("config.yaml");
    assert!(issue_names(yaml).is_empty(), "a YAML file has no imports");
}

#[test]
fn comments_in_data_files_are_still_counted() {
    // Comments are real nodes of the parsed tree, so a data file's comments
    // must not be dropped by the function-parse skip.
    let test = repo_with(&[(
        "settings.yaml",
        b"# top comment\nkey: value  # trailing\nlist:\n  - one\n",
    )]);
    let path = test.path().to_str().expect("utf-8 path");
    let (doc, _) = run_json(&[
        "-a",
        "static/comments",
        "--per-file",
        "--format",
        "json",
        path,
    ]);

    let files = doc["sections"][0]["files"]
        .as_array()
        .expect("per-file entries");
    let yaml = files
        .iter()
        .find(|f| f["file_path"] == "settings.yaml")
        .expect("entry");
    let total = yaml["metrics"]
        .as_array()
        .expect("metrics")
        .iter()
        .find(|m| m["label"] == "Total Comments")
        .and_then(|m| m["value"].as_str())
        .expect("Total Comments");
    assert_eq!(total, "2");
}

#[test]
fn sql_functions_are_still_counted_from_a_data_typed_file() {
    // enry types SQL as data, but its mapping declares CREATE FUNCTION as a
    // function, so the function analyzers must parse it.
    let test = repo_with(&[(
        "query.sql",
        b"CREATE FUNCTION add(a int, b int) RETURNS int AS $$ SELECT a + b $$ LANGUAGE sql;\n",
    )]);
    let path = test.path().to_str().expect("utf-8 path");
    let (doc, _) = run_json(&["-a", "static/complexity", "--format", "json", path]);
    assert_eq!(total_functions(&doc["sections"][0]), "1");
}
