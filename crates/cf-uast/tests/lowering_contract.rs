//! The language-agnostic lowering contract (`site/architecture/uast.md`,
//! "Shape contract"): behaviors the lowering guarantees for EVERY language
//! regardless of what its mapping table says — operator text on operator
//! nodes, source-text tokens on identifiers and literals, leaf comments with
//! line-exact spans, and the `Else` role on an `If`'s alternative branch.
//!
//! Each test drives the real parser over an embedded language chosen because
//! its table does NOT spell the behavior out, so a green test proves the
//! lowering supplies it.

use cf_uast::{Node, Parser};

const PROP_OPERATOR: &str = "operator";
const ROLE_ELSE: &str = "Else";

fn parse(filename: &str, source: &str) -> Node {
    Parser::new()
        .parse(filename, source.as_bytes())
        .unwrap_or_else(|e| panic!("{filename}: parse failed: {e}"))
}

fn of_type<'a>(root: &'a Node, node_type: &str) -> Vec<&'a Node> {
    root.find(|n| n.node_type == node_type)
}

fn operators(root: &Node, node_type: &str) -> Vec<String> {
    of_type(root, node_type)
        .iter()
        .map(|n| n.props.get(PROP_OPERATOR).cloned().unwrap_or_default())
        .collect()
}

#[test]
fn identifiers_and_literals_default_to_their_source_text() {
    // Kotlin's table declares `identifier` / `integer_literal` without a
    // token source; the lowering fills in the source text anyway.
    let root = parse("t.kt", "val x = 1\nval s = \"s\"\n");
    let idents: Vec<&str> = of_type(&root, "Identifier")
        .iter()
        .map(|n| n.token.as_str())
        .collect();
    assert!(idents.contains(&"x") && idents.contains(&"s"), "{idents:?}");
    let lits: Vec<&str> = of_type(&root, "Literal")
        .iter()
        .map(|n| n.token.as_str())
        .collect();
    assert_eq!(lits, ["1", "\"s\""]);
}

#[test]
fn operator_text_comes_from_the_operator_field() {
    let root = parse("t.go", "package p\n\nvar x = a + b\nvar y = -a\n");
    assert_eq!(operators(&root, "BinaryOp"), ["+"]);
    assert_eq!(operators(&root, "UnaryOp"), ["-"]);
}

#[test]
fn operator_text_comes_from_the_op_field() {
    // tree-sitter-swift names the field `op`.
    let root = parse("t.swift", "let x = a + b\n");
    assert_eq!(operators(&root, "BinaryOp"), ["+"]);
}

#[test]
fn chained_comparison_operators_are_space_joined() {
    // tree-sitter-python's `comparison_operator` carries a repeated
    // `operators` field; the chain `a < b <= c` reports both.
    let root = parse("t.py", "x = a < b <= c\n");
    assert_eq!(operators(&root, "BinaryOp"), ["< <="]);
}

#[test]
fn operator_text_falls_back_to_the_anonymous_operator_token() {
    // Rust's `assignment_expression` has no `operator` field; `=` is an
    // anonymous child.
    let root = parse("t.rs", "fn f() { c = 1; }\n");
    assert_eq!(operators(&root, "Assignment"), ["="]);
    // Python's `not_operator` likewise.
    let root = parse("t.py", "x = not a\n");
    assert_eq!(operators(&root, "UnaryOp"), ["not"]);
}

#[test]
fn explicit_literal_operator_props_win() {
    // The C ternary has no single operator token; its table pins `?:`.
    let root = parse("t.c", "int f(int a) { return a ? 1 : 2; }\n");
    assert_eq!(operators(&root, "If"), ["?:"]);
}

#[test]
fn comments_are_leaves_with_line_exact_spans() {
    // tree-sitter-rust's doc comments nest marker/text children and swallow
    // the line terminator; the lowering flattens and trims them, for LF and
    // CRLF alike.
    for (src, want_token) in [
        ("/// Doc.\nfn f() {}\n", "/// Doc."),
        ("/// Doc.\r\nfn f() {}\r\n", "/// Doc."),
        ("//! Inner.\n\nfn f() {}\n", "//! Inner."),
    ] {
        let root = parse("t.rs", src);
        let comments = of_type(&root, "Comment");
        assert_eq!(comments.len(), 1, "{src:?}");
        let c = comments[0];
        assert_eq!(c.token, want_token);
        assert!(c.children.is_empty());
        let pos = c.pos.expect("position");
        assert_eq!((pos.start_line, pos.end_line), (1, 1), "{src:?}");
        assert_eq!(
            pos.end_col,
            u64::try_from(want_token.len()).expect("fits") + 1
        );
        assert_eq!(
            pos.end_offset,
            u64::try_from(want_token.len()).expect("fits")
        );
    }
}

#[test]
fn alternative_field_children_carry_the_else_role() {
    // Go and Java have no `else_clause` node: the alternative is a direct
    // field of the if statement, so the role must come from the lowering.
    let go = parse(
        "t.go",
        "package p\n\nfunc f(a, b bool) {\n\tif a {\n\t} else if b {\n\t} else {\n\t}\n}\n",
    );
    assert_else_roles(&go);
    let java = parse(
        "A.java",
        "class A { void f(boolean a, boolean b) { if (a) {} else if (b) {} else {} } }\n",
    );
    assert_else_roles(&java);
}

fn assert_else_roles(root: &Node) {
    let ifs = of_type(root, "If");
    assert_eq!(ifs.len(), 2);
    let outer = ifs[0];
    assert!(!outer.roles.iter().any(|r| r == ROLE_ELSE));
    let with_else: Vec<&str> = outer
        .children
        .iter()
        .filter(|c| c.roles.iter().any(|r| r == ROLE_ELSE))
        .map(|c| c.node_type.as_str())
        .collect();
    assert_eq!(
        with_else,
        ["If"],
        "the else-if link is the only Else child of the outer if"
    );
    let inner = ifs[1];
    let with_else: Vec<&str> = inner
        .children
        .iter()
        .filter(|c| c.roles.iter().any(|r| r == ROLE_ELSE))
        .map(|c| c.node_type.as_str())
        .collect();
    assert_eq!(
        with_else,
        ["Block"],
        "the final else is the only Else child of the inner if"
    );
    let dup = inner
        .children
        .iter()
        .filter(|c| c.roles.iter().filter(|r| *r == ROLE_ELSE).count() > 1)
        .count();
    assert_eq!(dup, 0, "Else is never duplicated");
}

#[test]
fn parent_conditions_select_rules_by_enclosing_node() {
    // `arrow_function` lowers to a Lambda on its own, but is transparent
    // under the declarator that already lowered to a named Function.
    let root = parse("t.js", "const f = () => 1;\ng(() => 2);\n");
    assert_eq!(of_type(&root, "Lambda").len(), 1);
    assert_eq!(of_type(&root, "Function").len(), 1);
}
