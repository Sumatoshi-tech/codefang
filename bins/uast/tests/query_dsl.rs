//! End-to-end coverage of the `uast query` DSL through the **real binary**:
//! `filter` looks at the root's direct children, `rfilter` at the whole
//! tree, `rmap` applies an expression to every node, and `.props.name`
//! reads the declaration name the mapping contract guarantees.

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

/// Path to the freshly built `uast` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_uast");

/// A Java class with two methods: no function-role node is a direct child
/// of the file.
const JAVA_SOURCE: &str =
    "public class A {\n    public int f() { return 1; }\n    public int g() { return 2; }\n}\n";

/// Runs `uast query <query> <file> -f <format>` and returns stdout.
fn query(q: &str, file: &PathBuf, format: &str) -> String {
    let out = Command::new(BIN)
        .args(["query", q])
        .arg(file)
        .args(["-f", format])
        .env("NO_COLOR", "1")
        .output()
        .expect("run the uast binary");
    assert!(
        out.status.success(),
        "uast query failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

/// The `results` array of a JSON query output.
fn results(q: &str, file: &PathBuf) -> Vec<Value> {
    let doc: Value = serde_json::from_str(&query(q, file, "json")).expect("one JSON document");
    doc["results"].as_array().expect("results").clone()
}

/// Writes `source` to a unique scratch file.
fn fixture(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("uast-query-dsl-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write fixture");
    path
}

#[test]
fn filter_is_shallow_and_rfilter_is_recursive() {
    let file = fixture("A.java", JAVA_SOURCE);
    assert_eq!(
        query(r#"filter(.roles has "Function")"#, &file, "count").trim(),
        "0"
    );
    let methods = results(r#"rfilter(.roles has "Function")"#, &file);
    let names: Vec<&str> = methods
        .iter()
        .map(|n| n["props"]["name"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(names, vec!["f", "g"]);
    assert!(methods.iter().all(|n| n["type"] == "Method"));
}

/// Literal result nodes (field values) print as bare strings.
#[test]
fn rmap_and_props_name_read_every_declaration_name() {
    let file = fixture("A.java", JAVA_SOURCE);
    let names = results("rmap(.props.name)", &file);
    assert_eq!(
        names,
        vec![Value::from("A"), Value::from("f"), Value::from("g")]
    );
    let piped = results(r#"rfilter(.type == "Class") | map(.props.name)"#, &file);
    assert_eq!(piped, vec![Value::from("A")]);
}
