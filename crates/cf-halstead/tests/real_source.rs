//! Halstead numbers asserted end-to-end from SOURCE: each fixture is parsed
//! with the production `cf_uast::Parser`, and the operator / operand
//! multisets plus the derived measures are compared against values computed
//! by hand from the standard definitions (n1 / n2 / N1 / N2, V = N·log2 n,
//! D = (n1/2)·(N2/n2), E = D·V).
//!
//! Every fixture is the same two-statement body — `c = a + b; return c * 2`
//! in the language's own spelling — so the expected operands are always
//! `a, b, c, c, 2` (n2 = 4, N2 = 5) and only the operator spelling varies.

use std::collections::BTreeMap;

use cf_halstead::{analyze_functions, analyze_with_source, summarize, FunctionMetrics};
use cf_uast::Parser;

/// Parses `src` as `filename` and measures its functions with the file bytes.
fn functions(filename: &str, src: &str) -> Vec<FunctionMetrics> {
    let root = Parser::new()
        .parse(filename, src.as_bytes())
        .expect("fixture parses");
    analyze_functions(&root, Some(src.as_bytes()))
}

fn map(pairs: &[(&str, i64)]) -> BTreeMap<String, i64> {
    pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
}

/// The operands shared by every fixture: `a, b, c, c, 2`.
fn body_operands() -> BTreeMap<String, i64> {
    map(&[("a", 1), ("b", 1), ("c", 2), ("2", 1)])
}

/// Asserts the one function of a fixture and its hand-computed measures.
fn assert_single(fns: &[FunctionMetrics], name: &str, operators: &[(&str, i64)]) {
    assert_eq!(fns.len(), 1, "exactly one function: {fns:#?}");
    let f = &fns[0];
    assert_eq!(f.name, name);
    assert_eq!(f.operators, map(operators), "operators of {name}");
    assert_eq!(f.operands, body_operands(), "operands of {name}");

    let n1 = i64::try_from(operators.len()).unwrap();
    let big_n1: i64 = operators.iter().map(|(_, c)| c).sum();
    assert_eq!(f.counts.distinct_operators, n1);
    assert_eq!(f.counts.total_operators, big_n1);
    assert_eq!(f.counts.distinct_operands, 4);
    assert_eq!(f.counts.total_operands, 5);

    #[allow(clippy::cast_precision_loss)]
    let (n, big_n) = ((n1 + 4) as f64, (big_n1 + 5) as f64);
    let volume = big_n * n.log2();
    #[allow(clippy::cast_precision_loss)]
    let difficulty = (n1 as f64 / 2.0) * (5.0 / 4.0);
    assert!((f.derived.volume - volume).abs() < 1e-9, "volume of {name}");
    assert!(
        (f.derived.difficulty - difficulty).abs() < 1e-9,
        "difficulty of {name}"
    );
    assert!(
        (f.derived.effort - volume * difficulty).abs() < 1e-9,
        "effort of {name}"
    );
}

const GO_HAL: &str =
    "package main\n\nfunc hal(a int, b int) int {\n\tc := a + b\n\treturn c * 2\n}\n";

/// Go: `:=`, `+`, `*`, `return` → n1 = 4, N1 = 4; V = 9·log2(8) = 27,
/// D = 2.5, E = 67.5.
#[test]
fn go_short_var_declaration_and_return() {
    let fns = functions("hal.go", GO_HAL);
    assert_single(&fns, "hal", &[(":=", 1), ("+", 1), ("*", 1), ("return", 1)]);
    assert_eq!(fns[0].derived.volume, 27.0);
    assert_eq!(fns[0].derived.difficulty, 2.5);
    assert_eq!(fns[0].derived.effort, 67.5);
}

/// Python: `=`, `+`, `*`, `return`.
#[test]
fn python_assignment_and_return() {
    let fns = functions(
        "hal.py",
        "def hal(a, b):\n    c = a + b\n    return c * 2\n",
    );
    assert_single(&fns, "hal", &[("=", 1), ("+", 1), ("*", 1), ("return", 1)]);
}

/// JavaScript: `const`, `=`, `+`, `*`, `return`.
#[test]
fn javascript_const_declaration_and_return() {
    let fns = functions(
        "hal.js",
        "function hal(a, b) {\n  const c = a + b;\n  return c * 2;\n}\n",
    );
    assert_single(
        &fns,
        "hal",
        &[("const", 1), ("=", 1), ("+", 1), ("*", 1), ("return", 1)],
    );
}

/// Rust: `let`, `=`, `+`, `*` (tail expression, no `return`); the
/// token-less identifiers and literal are read back by span.
#[test]
fn rust_let_binding_and_tail_expression() {
    let fns = functions(
        "hal.rs",
        "fn hal(a: i32, b: i32) -> i32 {\n    let c = a + b;\n    c * 2\n}\n",
    );
    assert_single(&fns, "hal", &[("let", 1), ("=", 1), ("+", 1), ("*", 1)]);
}

/// Java: the local declaration's type keyword `int`, `=`, `+`, `*`,
/// `return`; the method signature (modifiers, parameter types) is excluded.
#[test]
fn java_local_declaration_and_return() {
    let fns = functions(
        "Foo.java",
        "class Foo {\n    int hal(int a, int b) {\n        int c = a + b;\n        return c * 2;\n    }\n}\n",
    );
    assert_single(
        &fns,
        "hal",
        &[("int", 1), ("=", 1), ("+", 1), ("*", 1), ("return", 1)],
    );
}

/// C: `int`, `=`, `+`, `*`, `return`; the declarator is signature and the
/// function is named through it.
#[test]
fn c_local_declaration_and_return() {
    let fns = functions(
        "hal.c",
        "int hal(int a, int b) {\n    int c = a + b;\n    return c * 2;\n}\n",
    );
    assert_single(
        &fns,
        "hal",
        &[("int", 1), ("=", 1), ("+", 1), ("*", 1), ("return", 1)],
    );
}

/// Calls: each `fmt.Println("x")` is the call operator `()`, the member
/// access `.`, and the operands `fmt`, `Println`, `"x"`. The parameter `x`
/// of the signature is not an operand; the string `"x"` is.
#[test]
fn go_calls_count_parentheses_member_access_and_string_arguments() {
    let fns = functions(
        "calls.go",
        "package main\n\nimport \"fmt\"\n\nfunc calls(x int) {\n\tfmt.Println(\"x\")\n\tfmt.Println(\"x\")\n\tfmt.Println(\"y\")\n}\n",
    );
    assert_eq!(fns.len(), 1);
    assert_eq!(fns[0].name, "calls");
    assert_eq!(fns[0].operators, map(&[("()", 3), (".", 3)]));
    assert_eq!(
        fns[0].operands,
        map(&[("fmt", 3), ("Println", 3), ("\"x\"", 2), ("\"y\"", 1)])
    );
    // n1 = 2, n2 = 4, N1 = 6, N2 = 9 → V = 15·log2(6), D = 1·(9/4).
    assert!((fns[0].derived.volume - 15.0 * 6f64.log2()).abs() < 1e-9);
    assert_eq!(fns[0].derived.difficulty, 2.25);
}

/// A one-line function has a positive volume: `return 1` is one operator and
/// one operand → n = 2, N = 2, V = 2.
#[test]
fn go_single_return_has_volume_two() {
    let fns = functions("x.go", "package a\n\nfunc same() int { return 1 }\n");
    assert_eq!(fns[0].operators, map(&[("return", 1)]));
    assert_eq!(fns[0].operands, map(&[("1", 1)]));
    assert_eq!(fns[0].derived.volume, 2.0);
}

/// Comments inside a body contribute nothing, in Go and in Python.
#[test]
fn comments_inside_bodies_are_not_tokens() {
    let go = functions(
        "c.go",
        "package main\n\n// doc\nfunc f(a int) int {\n\t// a + b is not code\n\treturn a /* * 2 */\n}\n",
    );
    assert_eq!(go[0].operators, map(&[("return", 1)]));
    assert_eq!(go[0].operands, map(&[("a", 1)]));

    let py = functions("c.py", "def f(a):\n    # return a + b\n    return a\n");
    assert_eq!(py[0].operators, map(&[("return", 1)]));
    assert_eq!(py[0].operands, map(&[("a", 1)]));
}

/// Control-flow keywords are operators, and a closure (`Lambda`) belongs to
/// the function that contains it — the same function predicate the
/// complexity analyzer uses — so `withClosure` is one record: the closure's
/// `func` keyword, its `if` / `>` / `+=`, the loop's `for` / `:=` / `range`,
/// the call `()` and the `return`. Its parameter `x` is signature; the loop
/// variable `x` is an operand.
#[test]
fn go_control_flow_and_closure_attribution() {
    let fns = functions(
        "cl.go",
        "package main\n\nfunc withClosure(xs []int) int {\n\ttotal := 0\n\tf := func(x int) {\n\t\tif x > 0 {\n\t\t\ttotal += x\n\t\t}\n\t}\n\tfor _, x := range xs {\n\t\tf(x)\n\t}\n\treturn total\n}\n",
    );
    assert_eq!(fns.len(), 1, "the closure is part of withClosure: {fns:#?}");
    let outer = &fns[0];
    assert_eq!(outer.name, "withClosure");
    assert_eq!(
        outer.operators,
        map(&[
            (":=", 3),
            ("func", 1),
            ("if", 1),
            (">", 1),
            ("+=", 1),
            ("for", 1),
            ("range", 1),
            ("()", 1),
            ("return", 1),
        ])
    );
    assert_eq!(
        outer.operands,
        map(&[
            ("total", 3),
            ("0", 2),
            ("f", 2),
            ("x", 4),
            ("_", 1),
            ("xs", 1)
        ])
    );
}

/// Without the file bytes, Go recovers everything through the function's
/// verbatim token, so the quality path and the static path agree.
#[test]
fn go_without_source_bytes_matches_with_source_bytes() {
    let root = Parser::new()
        .parse("hal.go", GO_HAL.as_bytes())
        .expect("parse");
    let with = analyze_functions(&root, Some(GO_HAL.as_bytes()));
    let without = analyze_functions(&root, None);
    assert_eq!(with, without);
}

/// The quality entry point returns the union aggregate of the same functions.
#[test]
fn file_level_measures_are_the_union_of_the_functions() {
    let src = "package main\n\nfunc a(x int) int { return x + 1 }\n\nfunc b(y int) int { return y * 2 }\n";
    let root = Parser::new().parse("ab.go", src.as_bytes()).expect("parse");
    let fns = analyze_functions(&root, Some(src.as_bytes()));
    assert_eq!(fns.len(), 2);
    let summary = summarize(&fns);
    // Union: operators {return: 2, +: 1, *: 1} → n1 = 3, N1 = 4;
    // operands {x, 1, y, 2} → n2 = 4, N2 = 4.
    assert_eq!(summary.counts.distinct_operators, 3);
    assert_eq!(summary.counts.total_operators, 4);
    assert_eq!(summary.counts.distinct_operands, 4);
    assert_eq!(summary.counts.total_operands, 4);
    let file = analyze_with_source(&root, Some(src.as_bytes()));
    assert_eq!(file.volume, summary.derived.volume);
    assert_eq!(file.volume, 8.0 * 7f64.log2());
    assert_eq!(file.effort, summary.derived.effort);
    assert_eq!(file.delivered_bugs, summary.derived.delivered_bugs);
}
