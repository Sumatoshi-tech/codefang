//! End-to-end coverage of the UAST shape contract through the **real
//! binary**: `uast parse --format json` over two on-disk fixtures, asserting
//! the JSON an analyzer would consume (`site/architecture/uast.md`, "Shape
//! contract"; `specs/bugs/BUG-uast-mapping-shapes.md`).

use std::path::PathBuf;
use std::process::Command;

use serde_json::Value;

/// Path to the freshly built `uast` binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_uast");

/// A C fixture: two definitions (one pointer-returning), a prototype, a
/// ternary and an else-if chain.
const C_SOURCE: &str = "int f(int a) { return a > 0 && a < 9 ? 1 : 2; }\n\
char *g(void);\n\
char *g(void) { if (x) { y(); } else if (z) { w(); } else { v(); } return 0; }\n";

/// A Go fixture: a named struct, a func-typed parameter, a three-clause for,
/// literals and operators.
const GO_SOURCE: &str = "package p\n\n\
type Config struct {\n\tN int\n}\n\n\
func apply(f func(int) int, n int) int {\n\
\tr := 0\n\
\tfor i := 0; i < n; i++ {\n\
\t\tr += f(i) * 2\n\
\t}\n\
\treturn r\n\
}\n";

/// Runs `uast parse --format json` on `path` and decodes the document.
fn parse_json(path: &PathBuf) -> Value {
    let out = Command::new(BIN)
        .args(["parse", "--format", "json"])
        .arg(path)
        .env("NO_COLOR", "1")
        .output()
        .expect("run the uast binary");
    assert!(
        out.status.success(),
        "uast parse failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stdout is one JSON document")
}

/// Writes `source` to a unique scratch file with the given extension.
fn fixture(name: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("uast-parse-shapes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write fixture");
    path
}

/// All nodes (pre-order) whose `type` equals `node_type`.
fn of_type<'a>(node: &'a Value, node_type: &str, out: &mut Vec<&'a Value>) {
    if node["type"] == node_type {
        out.push(node);
    }
    if let Some(children) = node["children"].as_array() {
        for c in children {
            of_type(c, node_type, out);
        }
    }
}

fn find<'a>(root: &'a Value, node_type: &str) -> Vec<&'a Value> {
    let mut out = Vec::new();
    of_type(root, node_type, &mut out);
    out
}

fn prop<'a>(node: &'a Value, key: &str) -> &'a str {
    node["props"][key].as_str().unwrap_or("")
}

fn has_role(node: &Value, role: &str) -> bool {
    node["roles"]
        .as_array()
        .is_some_and(|rs| rs.iter().any(|r| r == role))
}

#[test]
fn c_functions_are_named_once_with_operators_and_else_chain() {
    let root = parse_json(&fixture("shapes.c", C_SOURCE));

    let fns = find(&root, "Function");
    let names: Vec<&str> = fns.iter().map(|f| prop(f, "name")).collect();
    assert_eq!(
        names,
        ["f", "g"],
        "one Function per definition, none for the prototype"
    );
    for f in &fns {
        assert_eq!(
            f["token"],
            prop(f, "name"),
            "token is the name, not the source"
        );
        assert_eq!(find(f, "Function").len(), 1, "no nested Function");
    }

    let ops: Vec<&str> = find(&root, "BinaryOp")
        .iter()
        .map(|n| prop(n, "operator"))
        .collect();
    assert_eq!(ops, ["&&", ">", "<"]);
    let ternary = find(&root, "If");
    assert_eq!(ternary.len(), 3, "ternary + if + else-if");
    assert_eq!(prop(ternary[0], "operator"), "?:");

    let outer = ternary[1];
    let link = outer["children"]
        .as_array()
        .expect("children")
        .iter()
        .find(|c| c["type"] == "If")
        .expect("else-if is a direct If child");
    assert!(has_role(link, "Else"));
    let else_block = link["children"]
        .as_array()
        .expect("children")
        .iter()
        .find(|c| c["type"] == "Block" && has_role(c, "Else"))
        .expect("else is a direct Block child with Else");
    assert!(has_role(else_block, "Body"));

    assert!(find(&root, "Literal")
        .iter()
        .all(|n| !n["token"].as_str().unwrap_or("").is_empty()));
}

#[test]
fn go_struct_name_single_loop_literal_tokens_and_operators() {
    let root = parse_json(&fixture("shapes.go", GO_SOURCE));

    let structs = find(&root, "Struct");
    assert_eq!(structs.len(), 1);
    assert_eq!(prop(structs[0], "name"), "Config");
    assert_eq!(find(structs[0], "Field").len(), 1);

    let fns = find(&root, "Function");
    assert_eq!(fns.len(), 1, "func(int) int is a type, not a Function");
    assert_eq!(prop(fns[0], "name"), "apply");

    assert_eq!(
        find(&root, "Loop").len(),
        1,
        "a three-clause for is one Loop"
    );

    let lits: Vec<&str> = find(&root, "Literal")
        .iter()
        .map(|n| n["token"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(lits, ["0", "0", "2"]);

    let ops: Vec<&str> = find(&root, "BinaryOp")
        .iter()
        .map(|n| prop(n, "operator"))
        .collect();
    assert_eq!(ops, ["<", "*"]);
    let assigns: Vec<&str> = find(&root, "Assignment")
        .iter()
        .map(|n| prop(n, "operator"))
        .collect();
    assert_eq!(assigns, ["+="]);
    let unary: Vec<&str> = find(&root, "UnaryOp")
        .iter()
        .map(|n| prop(n, "operator"))
        .collect();
    assert_eq!(unary, ["++"]);
}

/// Runs `uast validate` on `json` written to a scratch file; the embedded
/// schema must accept everything `uast parse` emits.
fn assert_validates(name: &str, json: &Value) {
    let path = fixture(name, &serde_json::to_string(json).expect("re-encode"));
    let out = Command::new(BIN)
        .arg("validate")
        .arg(&path)
        .env("NO_COLOR", "1")
        .output()
        .expect("run the uast binary");
    assert!(
        out.status.success(),
        "uast validate rejected {name}: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn parsed_shapes_pass_uast_validate() {
    for (name, source) in [("valid.c", C_SOURCE), ("valid.go", GO_SOURCE)] {
        let root = parse_json(&fixture(name, source));
        assert_validates(&format!("{name}.json"), &root);
    }
}
