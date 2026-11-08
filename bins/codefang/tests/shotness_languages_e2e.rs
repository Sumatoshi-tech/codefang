//! BUG: specs/bugs/BUG-shotness-language-names.md
//! Named declarations must survive the default Shotness DSL and track body edits.
mod common;

use cf_gitlib::testutil::{commit_files, init_repo};
use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_codefang");
const BASE: i64 = 1_609_459_200;
const NEXT: i64 = BASE + 86_400;
const BEFORE: &str = "111";
const AFTER: &str = "222";

fn check(filename: &str, source: &str, expected: &[&str]) {
    let repo = init_repo().expect("init fixture");
    commit_files(&repo, "introduce", BASE, &[(filename, source.as_bytes())])
        .expect("commit source");
    let edited = source.replace(BEFORE, AFTER);
    assert_ne!(edited, source, "fixture has no body edit: {filename}");
    commit_files(&repo, "edit body", NEXT, &[(filename, edited.as_bytes())]).expect("commit edit");
    let output = common::command(BIN)
        .args([
            "run",
            "-a",
            "history/shotness",
            "--limit",
            "200",
            "--format",
            "json",
        ])
        .arg(repo.path())
        .output()
        .expect("run Shotness");
    assert!(
        output.status.success(),
        "{filename}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("JSON report");
    let rows = report["node_hotness"].as_array().expect("entity rows");
    let mut actual: Vec<_> = rows
        .iter()
        .map(|row| row["name"].as_str().expect("name"))
        .collect();
    actual.sort_unstable();
    let mut expected = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected, "{filename}: {report}");
    for row in rows {
        assert_eq!(
            row["change_count"], 2,
            "body edit missing in {filename}: {report}"
        );
    }
    assert_eq!(
        report["aggregate"]["total_changes"],
        serde_json::json!(expected.len() * 2),
        "{filename}: {report}"
    );
    let rendered = common::command(BIN)
        .args([
            "run",
            "-a",
            "history/shotness",
            "--limit",
            "200",
            "--format",
            "text",
        ])
        .env("NO_COLOR", "1")
        .arg(repo.path())
        .output()
        .expect("render Shotness");
    assert!(
        rendered.status.success(),
        "{filename}: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let text = String::from_utf8(rendered.stdout).expect("text report");
    for (label, wanted) in [
        ("Total Nodes", expected.len()),
        ("Total Changes", expected.len() * 2),
    ] {
        let shown = text
            .lines()
            .find_map(|line| line.trim_start().strip_prefix(label))
            .expect("summary metric");
        assert_eq!(shown.trim(), wanted.to_string(), "{filename}: {text}");
    }
}

macro_rules! language {
    ($test:ident, $filename:literal $(, $expected:literal)*) => {
        #[test]
        fn $test() {
            check(
                $filename,
                include_str!(concat!("fixtures/shotness_languages/", $filename)),
                &[$($expected),*],
            );
        }
    };
}
language!(bash, "bash.sh", "probe");
language!(c, "c.c", "probe");
language!(c_sharp, "c_sharp.cs", "Box.probe");
language!(clojure, "clojure.clj", "probe");
language!(cmake, "cmake.cmake", "probe");
language!(commonlisp, "commonlisp.lisp", "probe");
language!(cpp, "cpp.cpp", "probe");
language!(crystal, "crystal.cr", "probe");
language!(dart, "dart.dart", "probe");
language!(elixir, "elixir.ex", "Box.probe");
language!(elm, "elm.elm", "probe");
language!(fish, "fish.fish", "probe");
language!(fortran, "fortran.f90", "probe");
language!(go, "go.go", "probe");
language!(gotmpl, "gotmpl.gotmpl", "\"probe\"");
language!(graphql, "graphql.graphql", "probe");
language!(groovy, "groovy.groovy", "probe");
language!(haskell, "haskell.hs", "probe");
language!(java, "java.java", "Box.probe");
language!(javascript, "javascript.js", "probe");
language!(kotlin, "kotlin.kt", "probe");
language!(lua, "lua.lua", "probe");
language!(make, "make.makefile", "probe");
language!(nim, "nim.nim", "probe");
language!(perl, "perl.pl", "probe");
language!(php, "php.php", "probe");
language!(powershell, "powershell.ps1", "probe");
language!(proto, "proto.proto", "Box.probe");
language!(prql, "prql.prql", "probe");
language!(r, "r.r", "probe");
language!(rego, "rego.rego", "probe");
language!(ruby, "ruby.rb", "probe");
language!(rust, "rust.rs", "probe");
language!(scala, "scala.scala", "probe");
language!(sql, "sql.sql", "probe");
language!(swift, "swift.swift", "probe");
language!(tcl, "tcl.tcl", "probe");
language!(tsx, "tsx.tsx", "probe");
language!(typescript, "typescript.ts", "probe");
language!(zig, "zig.zig", "probe");
language!(
    clojure_forms,
    "clojure_forms.clj",
    "private-probe",
    "macro-probe",
    "test-probe",
    "method-probe"
);
language!(
    elixir_forms,
    "elixir_forms.ex",
    "Box.private_probe",
    "Box.macro_probe",
    "Box.private_macro",
    "Box.zero_probe"
);
language!(cmake_macro, "cmake_macro.cmake", "probe");
language!(
    commonlisp_forms,
    "commonlisp_forms.lisp",
    "macro-probe",
    "generic-probe",
    "method-probe"
);
language!(fortran_subroutine, "fortran_subroutine.f90", "probe");
language!(fortran_program, "fortran_program.f90", "probe");
language!(graphql_fragment, "graphql_fragment.graphql", "probe");
language!(graphql_anonymous, "graphql_anonymous.graphql");
language!(powershell_class, "powershell_class.ps1", "Box.probe");
language!(groovy_class, "groovy_class.groovy", "Box.probe");
language!(r_equal, "r_equal.r", "probe");
language!(r_super_left, "r_super_left.r", "probe");
language!(r_right, "r_right.r", "probe");
language!(r_super_right, "r_super_right.r", "probe");
language!(r_anonymous, "r_anonymous.r");

#[test]
fn r_named_function_body_is_measured_by_static_complexity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("probe.r");
    std::fs::write(&file, include_str!("fixtures/shotness_languages/r.r")).expect("write fixture");
    let output = common::command(BIN)
        .args(["run", "-a", "static/complexity", "--format", "json"])
        .arg(file)
        .output()
        .expect("run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    let section = &report["sections"][0];
    assert_eq!(section["metrics"][0]["value"], "1");
    assert_eq!(section["metrics"][2]["value"], "2");
    assert_eq!(section["issues"][0]["name"], "probe");
}
language!(python, "python.py", "probe");
language!(css, "css.css");
language!(latex, "latex.tex", "\\probe");
language!(latex_environment, "latex_environment.tex", "probe");
language!(latex_theorem, "latex_theorem.tex", "probe");
language!(latex_delimiter, "latex_delimiter.tex", "\\probe");

#[test]
fn latex_old_definition_is_named_when_introduced() {
    let repo = init_repo().expect("fixture");
    let source = include_bytes!("fixtures/shotness_languages/latex_old.tex");
    commit_files(&repo, "introduce definition", BASE, &[("old.tex", source)]).expect("commit");
    let output = common::command(BIN)
        .args([
            "run",
            "-a",
            "history/shotness",
            "--limit",
            "200",
            "--format",
            "json",
        ])
        .arg(repo.path())
        .output()
        .expect("run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).expect("JSON");
    assert_eq!(
        report["node_hotness"].as_array().expect("entities").len(),
        1
    );
    assert_eq!(report["node_hotness"][0]["name"], "\\probe");
    assert_eq!(report["node_hotness"][0]["change_count"], 1);
}

language!(
    elixir_guarded,
    "elixir_guarded.ex",
    "Box.guarded",
    "Box.private_guarded",
    "Box.macro_guarded",
    "Box.private_macro_guarded"
);
