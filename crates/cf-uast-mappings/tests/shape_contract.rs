//! Per-language SHAPE contract: each test parses a small snippet through the
//! real parser (`cf_uast::Parser`, grammars + tables linked) and asserts the
//! lowered shape the analyzers depend on — one declaration node per
//! declaration, `name` on every declaration, `operator` on every operator
//! node, source-text tokens on identifiers and literals, the else-if chain
//! convention and leaf comments. `specs/bugs/BUG-uast-mapping-shapes.md`
//! lists the defects each assertion closes; `site/architecture/uast.md`
//! documents the contract itself.

use cf_uast::{Node, Parser};

/// The `props` key carrying an operator node's operator text.
const PROP_OPERATOR: &str = "operator";
/// The `props` key carrying a declaration's declared name.
const PROP_NAME: &str = "name";
/// The role marking an `If`'s alternative branch (else / else-if).
const ROLE_ELSE: &str = "Else";
/// The role marking a documentation comment.
const ROLE_DOC: &str = "Doc";
/// The role carried by conditional nodes (`If`, ternaries).
const ROLE_CONDITION: &str = "Condition";
/// The ternary operator spelling in C-family languages.
const TERNARY_C_FAMILY: &str = "?:";
/// The ternary operator spelling in Python (`x if c else y`).
const TERNARY_PYTHON: &str = "if-else";

fn parse(filename: &str, source: &str) -> Node {
    Parser::new()
        .parse(filename, source.as_bytes())
        .unwrap_or_else(|e| panic!("{filename}: parse failed: {e}"))
}

fn of_type<'a>(root: &'a Node, node_type: &str) -> Vec<&'a Node> {
    root.find(|n| n.node_type == node_type)
}

fn names(nodes: &[&Node]) -> Vec<String> {
    nodes
        .iter()
        .map(|n| n.props.get(PROP_NAME).cloned().unwrap_or_default())
        .collect()
}

fn operator(n: &Node) -> &str {
    n.props.get(PROP_OPERATOR).map_or("", String::as_str)
}

fn has_role(n: &Node, role: &str) -> bool {
    n.roles.iter().any(|r| r == role)
}

/// Asserts that a node of `node_type` with `operator` text `op` exists.
fn assert_operator(root: &Node, node_type: &str, op: &str, lang: &str) {
    let found = of_type(root, node_type)
        .into_iter()
        .any(|n| operator(n) == op);
    assert!(
        found,
        "{lang}: expected a {node_type} with operator {op:?}; got {:?}",
        of_type(root, node_type)
            .iter()
            .map(|n| operator(n).to_string())
            .collect::<Vec<_>>()
    );
}

/// The function-like node types a complexity analyzer counts.
const FUNCTION_LIKE: &[&str] = &["Function", "Method", "Lambda"];

fn function_like(root: &Node) -> Vec<&Node> {
    root.find(|n| FUNCTION_LIKE.contains(&n.node_type.as_str()))
}

// ---------------------------------------------------------------- C / C++

#[test]
fn c_one_function_per_definition_named_by_declarator() {
    let src = "int f(int a) { return a; }\nchar *g(void) { return 0; }\nstatic int **h(int x);\n";
    let root = parse("t.c", src);
    let fns = of_type(&root, "Function");
    assert_eq!(names(&fns), ["f", "g"], "one Function per definition");
    for f in &fns {
        assert_eq!(
            f.token, f.props[PROP_NAME],
            "token is the name, not the body"
        );
        assert!(
            f.find(|n| n.node_type == "Function").len() == 1,
            "no nested Function inside {}",
            f.props[PROP_NAME]
        );
    }
}

#[test]
fn c_pointer_depth_and_ternary_operator() {
    let src = "int **pp(void) { return 0; }\nint t(int a) { return a ? 1 : 2; }\n";
    let root = parse("t.c", src);
    assert_eq!(names(&of_type(&root, "Function")), ["pp", "t"]);
    let ternary = of_type(&root, "If");
    assert_eq!(ternary.len(), 1);
    assert_eq!(operator(ternary[0]), TERNARY_C_FAMILY);
    assert!(has_role(ternary[0], ROLE_CONDITION));
}

#[test]
fn c_else_if_chain_is_if_under_if_with_else_role() {
    let src = "void f(int a, int b) { if (a) { x(); } else if (b) { y(); } else { z(); } }\n";
    let root = parse("t.c", src);
    assert_else_if_chain(&root, "c");
}

#[test]
fn cpp_qualified_and_reference_declarators_are_named() {
    let src = "int Foo::bar(int a) { return a; }\nint& ref() { static int x; return x; }\nint t(int a) { return a ? 1 : 2; }\n";
    let root = parse("t.cpp", src);
    assert_eq!(names(&of_type(&root, "Function")), ["Foo::bar", "ref", "t"]);
    assert_operator(&root, "If", TERNARY_C_FAMILY, "cpp");
}

#[test]
fn cpp_else_if_chain() {
    let src = "void f(int a, int b) { if (a) { x(); } else if (b) { y(); } else { z(); } }\n";
    assert_else_if_chain(&parse("t.cpp", src), "cpp");
}

// ---------------------------------------------------------------- Go

#[test]
fn go_function_type_parameter_is_not_a_function() {
    let src = "package p\n\nfunc apply(f func(int) int, x int) int { return f(x) }\n";
    let root = parse("t.go", src);
    assert_eq!(names(&of_type(&root, "Function")), ["apply"]);
    assert!(
        !of_type(&root, "TypeAnnotation").is_empty(),
        "func(int) int lowers to a TypeAnnotation"
    );
}

#[test]
fn go_three_clause_for_is_one_loop() {
    let src = "package p\n\nfunc f(n int) {\n\tfor i := 0; i < n; i++ {\n\t}\n\tfor _, x := range []int{1} {\n\t\t_ = x\n\t}\n}\n";
    let root = parse("t.go", src);
    assert_eq!(
        of_type(&root, "Loop").len(),
        2,
        "one Loop per for statement"
    );
}

#[test]
fn go_literals_and_identifiers_carry_tokens() {
    let src = "package p\n\nvar a = 42\nvar b = 1.5\nvar c = \"s\"\nvar d = 'r'\nvar e = 2i\n";
    let root = parse("t.go", src);
    let tokens: Vec<&str> = of_type(&root, "Literal")
        .iter()
        .map(|n| n.token.as_str())
        .collect();
    assert_eq!(tokens, ["42", "1.5", "\"s\"", "'r'", "2i"]);
    assert!(of_type(&root, "Identifier")
        .iter()
        .all(|n| !n.token.is_empty()));
}

#[test]
fn go_named_type_declarations_carry_names() {
    let src = "package p\n\ntype Config struct {\n\tA int\n}\n\ntype Shape interface {\n\tArea() float64\n}\n\ntype ID int\n";
    let root = parse("t.go", src);
    let structs = of_type(&root, "Struct");
    assert_eq!(names(&structs), ["Config"]);
    assert_eq!(of_type(structs[0], "Field").len(), 1, "fields stay inside");
    assert_eq!(names(&of_type(&root, "Interface")), ["Shape"]);
    assert_eq!(names(&of_type(&root, "TypeDeclaration")), ["ID"]);
}

#[test]
fn go_else_if_chain_and_inc_dec() {
    let src = "package p\n\nfunc f(a, b bool, i int) {\n\tif a {\n\t\tx()\n\t} else if b {\n\t\ty()\n\t} else {\n\t\tz()\n\t}\n\ti++\n\ti--\n}\n";
    let root = parse("t.go", src);
    assert_else_if_chain(&root, "go");
    assert_operator(&root, "UnaryOp", "++", "go");
    assert_operator(&root, "UnaryOp", "--", "go");
}

// ---------------------------------------------------------------- Rust

#[test]
fn rust_identifiers_and_literals_carry_tokens() {
    let src = "fn f(x: i32) -> bool { let y = x + 1; let s = \"s\"; let c = 'c'; let b = true; y > 2.5 as i32 }\n";
    let root = parse("t.rs", src);
    let idents: Vec<&str> = of_type(&root, "Identifier")
        .iter()
        .map(|n| n.token.as_str())
        .collect();
    assert!(
        idents.contains(&"y") && idents.contains(&"x") && !idents.contains(&""),
        "identifiers: {idents:?}"
    );
    let lits: Vec<&str> = of_type(&root, "Literal")
        .iter()
        .map(|n| n.token.as_str())
        .collect();
    for want in ["1", "\"s\"", "'c'", "true", "2.5"] {
        assert!(
            lits.contains(&want),
            "literal {want:?} missing from {lits:?}"
        );
    }
}

#[test]
fn rust_doc_comments_are_flat_single_line_doc_comments() {
    let src = "//! Crate doc.\n\n/// Doc for f.\n/// Second line.\nfn f() {}\n// plain\nfn g() {}\n/** block doc */\nfn h() {}\n";
    let root = parse("t.rs", src);
    let comments = of_type(&root, "Comment");
    let tokens: Vec<&str> = comments.iter().map(|n| n.token.as_str()).collect();
    assert_eq!(
        tokens,
        [
            "//! Crate doc.",
            "/// Doc for f.",
            "/// Second line.",
            "// plain",
            "/** block doc */"
        ]
    );
    for c in &comments {
        assert!(c.children.is_empty(), "comments are leaves: {:?}", c.token);
        let pos = c.pos.expect("comment has a position");
        assert_eq!(
            pos.start_line, pos.end_line,
            "span is the comment line: {:?}",
            c.token
        );
    }
    let docs: Vec<bool> = comments.iter().map(|n| has_role(n, ROLE_DOC)).collect();
    assert_eq!(docs, [true, true, true, false, true]);
}

#[test]
fn rust_else_if_chain() {
    let src = "fn f(a: bool, b: bool) { if a { x() } else if b { y() } else { z() } }\n";
    assert_else_if_chain(&parse("t.rs", src), "rust");
}

// ---------------------------------------------------------------- Java

#[test]
fn java_switch_cases_single_catch_ternary_and_literals() {
    let src = "class A {\n  int f(int r) {\n    int t = r > 0 ? 1 : 2;\n    try { r = r / t; } catch (ArithmeticException e) { r = 0; }\n    switch (r) { case 1: r = 10; break; case 2: r = 20; break; default: r = \"s\".length(); }\n    return r;\n  }\n}\n";
    let root = parse("A.java", src);
    assert_eq!(
        of_type(&root, "Switch").len(),
        1,
        "the switch statement only"
    );
    assert_eq!(of_type(&root, "Case").len(), 3, "case 1, case 2, default");
    let catches = of_type(&root, "Catch");
    assert_eq!(catches.len(), 1, "one Catch per catch clause");
    let ternary = of_type(&root, "If");
    assert_eq!(ternary.len(), 1);
    assert_eq!(operator(ternary[0]), TERNARY_C_FAMILY);
    let lits: Vec<&str> = of_type(&root, "Literal")
        .iter()
        .map(|n| n.token.as_str())
        .collect();
    for want in ["0", "1", "2", "10", "20", "\"s\""] {
        assert!(
            lits.contains(&want),
            "literal {want:?} missing from {lits:?}"
        );
    }
}

#[test]
fn java_one_class_per_declaration_with_name() {
    let src = "/** Javadoc for A. */\npublic class A {\n    /** Javadoc for foo. */\n    public int foo() { return 1; }\n    public int bar() { return 2; }\n}\ninterface I { void m(); }\nenum E { X, Y }\n";
    let root = parse("A.java", src);
    assert_eq!(names(&of_type(&root, "Class")), ["A"]);
    assert_eq!(names(&of_type(&root, "Interface")), ["I"]);
    assert_eq!(names(&of_type(&root, "Enum")), ["E"]);
    assert_eq!(names(&of_type(&root, "Method")), ["foo", "bar", "m"]);
    assert_eq!(function_like(&root).len(), 3);
}

#[test]
fn java_else_if_chain() {
    let src = "class A { void f(boolean a, boolean b) { if (a) { x(); } else if (b) { y(); } else { z(); } } }\n";
    assert_else_if_chain(&parse("A.java", src), "java");
}

// ---------------------------------------------------------------- Python

#[test]
fn python_boolean_and_comparison_operators() {
    let src = "e = a and b\nf = a or b\ng = not a\nh = a < b\ni = a not in b\nj = x if c else y\n";
    let root = parse("t.py", src);
    assert_operator(&root, "BinaryOp", "and", "python");
    assert_operator(&root, "BinaryOp", "or", "python");
    assert_operator(&root, "UnaryOp", "not", "python");
    assert_operator(&root, "BinaryOp", "<", "python");
    assert_operator(&root, "BinaryOp", "not in", "python");
    assert_operator(&root, "If", TERNARY_PYTHON, "python");
    assert!(of_type(&root, "Synthetic")
        .iter()
        .all(|n| !n.token.contains(" and ") && !n.token.contains(" or ")));
}

#[test]
fn python_elif_else_structure_is_recoverable() {
    let src = "if a:\n    x()\nelif b:\n    y()\nelse:\n    z()\n";
    let root = parse("t.py", src);
    let outer = of_type(&root, "If")
        .into_iter()
        .find(|n| !has_role(n, ROLE_ELSE))
        .expect("the outer if");
    let elif = outer
        .children
        .iter()
        .find(|c| c.node_type == "If")
        .expect("elif lowers to an If child of the if");
    assert!(has_role(elif, ROLE_ELSE) && has_role(elif, ROLE_CONDITION));
    let else_block = outer
        .children
        .iter()
        .find(|c| c.node_type == "Block" && has_role(c, ROLE_ELSE))
        .expect("else lowers to a Block child with role Else");
    assert!(has_role(else_block, "Body"));
}

#[test]
fn python_docstrings_lower_to_doc_comments() {
    let src = "\"\"\"Module doc.\"\"\"\n\nclass K:\n    \"\"\"Class doc.\"\"\"\n\n    def m(self):\n        \"\"\"Method doc.\"\"\"\n        s = \"not a doc\"\n        return s\n";
    let root = parse("t.py", src);
    let docs = of_type(&root, "Comment");
    let tokens: Vec<&str> = docs.iter().map(|n| n.token.as_str()).collect();
    assert_eq!(
        tokens,
        [
            "\"\"\"Module doc.\"\"\"",
            "\"\"\"Class doc.\"\"\"",
            "\"\"\"Method doc.\"\"\""
        ]
    );
    for d in &docs {
        assert!(has_role(d, "Comment") && has_role(d, ROLE_DOC));
        assert!(d.children.is_empty(), "docstrings are leaf comments");
    }
    assert_eq!(
        of_type(&root, "Literal")
            .iter()
            .map(|n| n.token.as_str())
            .collect::<Vec<_>>(),
        ["\"not a doc\""],
        "a string used as a value stays a Literal"
    );
    let method = &of_type(&root, "Function")[0];
    assert_eq!(method.props[PROP_NAME], "m");
    assert_eq!(
        of_type(method, "Comment").len(),
        1,
        "the docstring sits inside its function"
    );
}

// ---------------------------------------------------------------- JavaScript / TypeScript

#[test]
fn javascript_else_if_chain_and_operators() {
    let src = "function f(a, b) {\n  if (a) { x(); } else if (b) { y(); } else { z(); }\n  let c = a && b || !a;\n  c = 1; c += 2; c++;\n}\n";
    let root = parse("t.js", src);
    assert_else_if_chain(&root, "javascript");
    assert_operator(&root, "BinaryOp", "&&", "javascript");
    assert_operator(&root, "BinaryOp", "||", "javascript");
    assert_operator(&root, "UnaryOp", "!", "javascript");
    assert_operator(&root, "Assignment", "=", "javascript");
    assert_operator(&root, "Assignment", "+=", "javascript");
    assert_operator(&root, "UnaryOp", "++", "javascript");
    assert_eq!(names(&of_type(&root, "Function")), ["f"]);
}

#[test]
fn javascript_named_arrow_is_one_function() {
    let src = "const arrow = (x) => { if (x) { return 1; } return 0; };\nconst fe = function named() { return 2; };\nclass C { handler = () => 1; }\n[1].map((v) => v * 2);\n";
    let root = parse("t.js", src);
    let fns = function_like(&root);
    let summary: Vec<(String, String)> = fns
        .iter()
        .map(|n| {
            (
                n.node_type.clone(),
                n.props.get(PROP_NAME).cloned().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("Function".into(), "arrow".into()),
            ("Function".into(), "fe".into()),
            ("Method".into(), "handler".into()),
            ("Lambda".into(), String::new()),
        ]
    );
    for f in &fns {
        assert_eq!(
            function_like(f).len(),
            1,
            "no nested function-like node under {:?}",
            f.props
        );
    }
    let arrow = fns[0];
    assert_eq!(
        of_type(arrow, "If").len(),
        1,
        "the body stays inside the named function"
    );
}

#[test]
fn typescript_else_if_chain_and_named_arrow() {
    let src = "function f(a: boolean, b: boolean): void {\n  if (a) { x(); } else if (b) { y(); } else { z(); }\n}\nconst g = (x: number): number => x + 1;\nclass C { h = (v: number) => v; }\n";
    let root = parse("t.ts", src);
    assert_else_if_chain(&root, "typescript");
    let summary: Vec<(String, String)> = function_like(&root)
        .iter()
        .map(|n| {
            (
                n.node_type.clone(),
                n.props.get(PROP_NAME).cloned().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("Function".into(), "f".into()),
            ("Function".into(), "g".into()),
            ("Method".into(), "h".into()),
        ]
    );
    assert_operator(&root, "BinaryOp", "+", "typescript");
    let tsx = parse("t.tsx", "const g = (x: number) => <div>{x}</div>;\n");
    assert_eq!(
        function_like(&tsx).len(),
        1,
        "tsx: one function-like node for a named arrow"
    );
}

#[test]
fn loop_kind_props_are_real_props() {
    let js = parse(
        "t.js",
        "for (;;) {} while (a) {} do {} while (a); for (k in o) {}\n",
    );
    let kinds: Vec<String> = of_type(&js, "Loop")
        .iter()
        .map(|n| n.props.get("kind").cloned().unwrap_or_default())
        .collect();
    assert_eq!(kinds, ["for", "while", "do-while", "for-in"]);
    assert!(of_type(&js, "Loop")
        .iter()
        .all(|n| !n.props.contains_key("props")));
    let rs = parse("t.rs", "fn f() { for x in 0..1 {} while a {} loop {} }\n");
    let kinds: Vec<String> = of_type(&rs, "Loop")
        .iter()
        .map(|n| n.props.get("kind").cloned().unwrap_or_default())
        .collect();
    assert_eq!(kinds, ["for", "while", "loop"]);
    let kt = parse(
        "t.kt",
        "fun f() {\n    for (x in xs) {}\n    while (a) {}\n    do {} while (a)\n}\n",
    );
    let kinds: Vec<String> = of_type(&kt, "Loop")
        .iter()
        .map(|n| n.props.get("kind").cloned().unwrap_or_default())
        .collect();
    assert_eq!(kinds, ["for", "while", "do-while"]);
}

// ---------------------------------------------------------------- shared assertions

/// The else-if convention: the outer `If` has an `If` child carrying `Else`
/// (the else-if link), and that inner `If` has a `Block` child carrying
/// `Else` (the final else); neither is wrapped in a `Synthetic`/`Block`.
fn assert_else_if_chain(root: &Node, lang: &str) {
    let ifs = of_type(root, "If");
    assert_eq!(
        ifs.len(),
        2,
        "{lang}: an if/else-if/else chain is two If nodes"
    );
    let outer = ifs[0];
    assert!(
        !has_role(outer, ROLE_ELSE),
        "{lang}: the outer if is not an else branch"
    );
    let link = outer
        .children
        .iter()
        .find(|c| c.node_type == "If")
        .unwrap_or_else(|| {
            panic!(
                "{lang}: else-if must be a direct If child; got {:?}",
                kinds(outer)
            )
        });
    assert!(
        has_role(link, ROLE_ELSE),
        "{lang}: the else-if If carries Else"
    );
    assert!(
        has_role(link, ROLE_CONDITION),
        "{lang}: the else-if If keeps Condition"
    );
    let else_block = link
        .children
        .iter()
        .find(|c| c.node_type == "Block" && has_role(c, ROLE_ELSE))
        .unwrap_or_else(|| {
            panic!(
                "{lang}: else must be a direct Block child with Else; got {:?}",
                kinds(link)
            )
        });
    assert!(has_role(else_block, "Body"));
    let bodies = outer
        .children
        .iter()
        .filter(|c| c.node_type == "Block" && !has_role(c, ROLE_ELSE))
        .count();
    assert_eq!(
        bodies, 1,
        "{lang}: exactly one consequence Block on the outer if"
    );
}

fn kinds(n: &Node) -> Vec<(String, Vec<String>)> {
    n.children
        .iter()
        .map(|c| (c.node_type.clone(), c.roles.clone()))
        .collect()
}

// ---------------------------------------------------------------- cross-language operator contract

/// One row per language: a snippet containing `a + b`, a logical AND, a
/// unary minus, a plain assignment and a compound assignment.
struct OperatorRow {
    file: &'static str,
    source: &'static str,
    and: &'static str,
}

const OPERATOR_ROWS: &[OperatorRow] = &[
    OperatorRow {
        file: "t.go",
        source: "package p\n\nfunc f(a, b int) int {\n\tc := a + b\n\tc = 1\n\tc += 2\n\td := -a\n\te := a > 0 && b > 0\n\t_, _ = d, e\n\treturn c\n}\n",
        and: "&&",
    },
    OperatorRow {
        file: "t.py",
        source: "c = a + b\nc = 1\nc += 2\nd = -a\ne = a and b\n",
        and: "and",
    },
    OperatorRow {
        file: "t.js",
        source: "let c = a + b; c = 1; c += 2; let d = -a; let e = a && b;\n",
        and: "&&",
    },
    OperatorRow {
        file: "t.ts",
        source: "let c = a + b; c = 1; c += 2; let d = -a; let e = a && b;\n",
        and: "&&",
    },
    OperatorRow {
        file: "t.rs",
        source: "fn f(a: i32, b: i32) { let mut c = a + b; c = 1; c += 2; let d = -a; let e = a > 0 && b > 0; }\n",
        and: "&&",
    },
    OperatorRow {
        file: "A.java",
        source: "class A { void f(int a, int b) { int c = a + b; c = 1; c += 2; int d = -a; boolean e = a > 0 && b > 0; } }\n",
        and: "&&",
    },
    OperatorRow {
        file: "t.c",
        source: "int f(int a, int b) { int c = a + b; c = 1; c += 2; int d = -a; int e = a > 0 && b > 0; return c; }\n",
        and: "&&",
    },
    OperatorRow {
        file: "t.cpp",
        source: "int f(int a, int b) { int c = a + b; c = 1; c += 2; int d = -a; bool e = a > 0 && b > 0; return c; }\n",
        and: "&&",
    },
    OperatorRow {
        file: "A.cs",
        source: "class A { void F(int a, int b) { int c = a + b; c = 1; c += 2; int d = -a; bool e = a > 0 && b > 0; } }\n",
        and: "&&",
    },
    OperatorRow {
        file: "t.rb",
        source: "c = a + b\nc = 1\nc += 2\nd = -a\ne = a && b\n",
        and: "&&",
    },
    OperatorRow {
        file: "t.php",
        source: "<?php\n$c = $a + $b; $c = 1; $c += 2; $d = -$a; $e = $a && $b;\n",
        and: "&&",
    },
    OperatorRow {
        file: "t.kt",
        source: "fun f(a: Int, b: Int) { var c = a + b; c = 1; c += 2; val d = -a; val e = a > 0 && b > 0 }\n",
        and: "&&",
    },
    OperatorRow {
        file: "t.swift",
        source: "func f(a: Int, b: Int) { var c = a + b; c = 1; c += 2; let d = -a; let e = a > 0 && b > 0 }\n",
        and: "&&",
    },
];

#[test]
fn every_language_exposes_operator_text_on_operator_nodes() {
    for row in OPERATOR_ROWS {
        let root = parse(row.file, row.source);
        assert_operator(&root, "BinaryOp", "+", row.file);
        assert_operator(&root, "BinaryOp", row.and, row.file);
        assert_operator(&root, "UnaryOp", "-", row.file);
        assert_operator(&root, "Assignment", "=", row.file);
        assert_operator(&root, "Assignment", "+=", row.file);
        for t in ["BinaryOp", "UnaryOp", "Assignment"] {
            for n in of_type(&root, t) {
                assert!(
                    !operator(n).is_empty(),
                    "{}: a {t} without operator: {:?}",
                    row.file,
                    n.token
                );
            }
        }
    }
}

#[test]
fn every_language_tokens_identifiers_and_literals() {
    for row in OPERATOR_ROWS {
        let root = parse(row.file, row.source);
        for t in ["Identifier", "Literal"] {
            let nodes = of_type(&root, t);
            assert!(!nodes.is_empty(), "{}: no {t} nodes", row.file);
            for n in nodes {
                assert!(
                    !n.token.is_empty(),
                    "{}: token-less {t} at {:?}",
                    row.file,
                    n.pos
                );
            }
        }
    }
}
