//! Hand-computed metrics asserted end-to-end from SOURCE: each fixture file
//! under `tests/fixtures/` (shared with the CLI e2e suite) is parsed with the
//! real `cf_uast::Parser`, bridged, and measured. The
//! expected numbers follow the counting rules documented in
//! `site/explanation/complexity.md` (`McCabe` cyclomatic, `SonarSource`
//! cognitive, block nesting depth, non-blank non-comment lines of code).

use cf_complexity::node::Node;
use cf_complexity::{AnalysisOptions, Analyzer, FunctionMetrics};
use cf_uast::Parser;

/// `(name, cyclomatic, cognitive, nesting, lines_of_code)`.
type Expected = (&'static str, i64, i64, i64, i64);

fn measure(file: &str, source: &str) -> Vec<FunctionMetrics> {
    let tree = Parser::new()
        .parse(file, source.as_bytes())
        .unwrap_or_else(|e| panic!("parse {file}: {e:?}"));
    let root = Node::from_uast(&tree);
    Analyzer.function_metrics_with(
        Some(&root),
        &AnalysisOptions {
            source: Some(source.as_bytes()),
            find_depth: None,
        },
    )
}

fn assert_functions(file: &str, source: &str, expected: &[Expected]) {
    let got = measure(file, source);
    let mut rows: Vec<(String, i64, i64, i64, i64)> = got
        .iter()
        .map(|m| {
            (
                m.name.clone(),
                m.cyclomatic_complexity,
                m.cognitive_complexity,
                m.nesting_depth,
                m.lines_of_code,
            )
        })
        .collect();
    rows.sort();
    let mut want: Vec<(String, i64, i64, i64, i64)> = expected
        .iter()
        .map(|(n, cc, cog, nest, loc)| ((*n).to_string(), *cc, *cog, *nest, *loc))
        .collect();
    want.sort();
    assert_eq!(rows, want, "{file}: (name, cc, cognitive, nesting, loc)");
    for m in &got {
        assert_eq!(
            m.decision_points,
            m.cyclomatic_complexity - 1,
            "{file}: decision points of {}",
            m.name
        );
        assert!(m.line > 0, "{file}: {} has no start line", m.name);
    }
}

const GO_FIXTURE: &str = include_str!("fixtures/cc.go");

#[test]
fn go_fixture() {
    assert_functions(
        "cc.go",
        GO_FIXTURE,
        &[
            ("decisions", 8, 7, 1, 22),
            ("nested", 4, 6, 3, 10),
            ("elseNest", 3, 4, 2, 10),
            ("loopCond", 4, 3, 1, 7),
            ("fact", 2, 2, 1, 6),
            ("withClosure", 3, 3, 1, 12),
            ("boolSeq", 4, 2, 0, 3),
            ("manyCases", 5, 1, 1, 14),
            ("simple", 1, 0, 0, 3),
            ("Method", 2, 1, 1, 6),
        ],
    );
}

const GO_EXTRAS: &str = include_str!("fixtures/extras.go");

#[test]
fn go_extras_jumps_select_type_switch_closures_recursion_and_declarations() {
    assert_functions(
        "extras.go",
        GO_EXTRAS,
        &[
            // loops 1 + 2, ifs at nesting 2: 3 each, two labelled jumps.
            ("labeled", 6, 14, 3, 18),
            // select: two non-default cases; no switch node, so no cognitive.
            ("sel", 3, 0, 0, 10),
            ("typeSwitch", 3, 1, 1, 9),
            // if inside a closure inside a closure: nesting 2 -> +3.
            ("nestedClosure", 2, 3, 1, 11),
            ("paren", 3, 2, 0, 3),
            ("chain", 4, 4, 1, 11),
            ("rec", 2, 2, 1, 6),
            ("Rec", 2, 2, 1, 6),
            ("commented", 1, 0, 0, 3),
        ],
    );
}

const PY_FIXTURE: &str = include_str!("fixtures/cc.py");

#[test]
fn python_fixture() {
    assert_functions(
        "cc.py",
        PY_FIXTURE,
        &[
            ("py_decisions", 8, 8, 1, 17),
            ("py_nested", 4, 6, 3, 6),
            ("method", 2, 1, 1, 4),
        ],
    );
}

const PY_EXTRAS: &str = include_str!("fixtures/extras.py");

#[test]
fn python_nested_def_is_its_own_function() {
    assert_functions(
        "extras.py",
        PY_EXTRAS,
        &[
            // if 1, elif 1, else 1, inner if at nesting 1: 2, `or` 1, while 1.
            ("outer", 6, 7, 2, 11),
            ("inner", 2, 1, 1, 4),
        ],
    );
}

const JS_FIXTURE: &str = include_str!("fixtures/cc.js");

#[test]
fn javascript_fixture() {
    assert_functions(
        "cc.js",
        JS_FIXTURE,
        &[
            ("jsDecisions", 11, 10, 1, 12),
            ("jsNested", 4, 6, 3, 10),
            ("arrow", 2, 1, 1, 1),
        ],
    );
}

const JS_EXTRAS: &str = include_str!("fixtures/extras.js");

#[test]
fn javascript_nested_functions_callbacks_and_labels() {
    assert_functions(
        "extras.js",
        JS_EXTRAS,
        &[
            // if 1, inner 2, else 1, inner 2, do 1, for 1, if 2, break label 1,
            // ternary in the arrow callback at nesting 1: 2.
            ("outer", 8, 13, 2, 11),
            ("inner", 2, 1, 1, 4),
            ("g", 2, 1, 0, 1),
        ],
    );
}

const RS_FIXTURE: &str = include_str!("fixtures/cc.rs");

#[test]
fn rust_fixture() {
    assert_functions(
        "cc.rs",
        RS_FIXTURE,
        &[
            ("rs_decisions", 9, 8, 1, 8),
            ("rs_nested", 4, 6, 3, 10),
            ("method", 2, 1, 1, 4),
        ],
    );
}

const RS_EXTRAS: &str = include_str!("fixtures/extras.rs");

#[test]
fn rust_nested_fn_closures_labels_and_trait_declarations() {
    assert_functions(
        "extras.rs",
        RS_EXTRAS,
        &[
            // closure if@1 2 + else 1, if-let 1, inner 2, else 1, inner 2,
            // for 1, for 2, if 3, continue label 1, match 1.
            ("outer", 10, 17, 3, 11),
            ("inner", 2, 2, 1, 3),
            ("d", 1, 0, 0, 1),
        ],
    );
}

const JAVA_FIXTURE: &str = include_str!("fixtures/Foo.java");

#[test]
fn java_nested_method() {
    let got = measure("Foo.java", JAVA_FIXTURE);
    let nested = got
        .iter()
        .find(|m| m.name == "nested")
        .expect("nested method found");
    assert_eq!(
        (
            nested.cyclomatic_complexity,
            nested.cognitive_complexity,
            nested.nesting_depth,
            nested.lines_of_code
        ),
        (4, 6, 3, 10)
    );
    assert_eq!(got.len(), 2, "two methods: {got:?}");
}

/// Java `switch` arms lower to `Case` nodes and the ternary to a
/// conditional `If` (the cf-uast-mappings shape contract), so every arm and
/// the `?:` count as decision points.
#[test]
fn java_decisions_method() {
    assert_functions(
        "Foo.java",
        JAVA_FIXTURE,
        &[("decisions", 11, 10, 1, 10), ("nested", 4, 6, 3, 10)],
    );
}

const JAVA_EXTRAS: &str = include_str!("fixtures/Probe.java");

#[test]
fn java_abstract_and_interface_methods_are_not_functions() {
    assert_functions(
        "Probe.java",
        JAVA_EXTRAS,
        &[
            // lambda if@1 2, if 1, inner 2, else 1, inner 2, do 1, for 1,
            // && 1, if@1 2, break label 1, if 1, sequence 2.
            ("outer", 12, 17, 2, 12),
        ],
    );
}

const C_FIXTURE: &str = include_str!("fixtures/cc.c");

#[test]
fn c_fixture() {
    assert_functions(
        "cc.c",
        C_FIXTURE,
        &[
            // goto is a jump to a label: +1 cognitive.
            ("c_decisions", 10, 10, 1, 11),
            ("c_nested", 4, 6, 3, 10),
        ],
    );
}

const C_EXTRAS: &str = include_str!("fixtures/extras.c");

#[test]
fn c_braceless_branches_prototypes_and_negation() {
    assert_functions(
        "extras.c",
        C_EXTRAS,
        &[
            // if 1, inner 2, else 1, inner 2, if 1, braceless inner if 2,
            // if 1, else-if 1, else 1, do 1, for 1, if@1 2, sequence 2.
            ("outer", 13, 18, 2, 12),
        ],
    );
}

const IFINIT_FIXTURE: &str = include_str!("fixtures/i.go");

#[test]
fn go_if_with_init_statement() {
    assert_functions(
        "i.go",
        IFINIT_FIXTURE,
        &[
            ("initIf", 2, 1, 1, 6),
            ("initIfNested", 3, 3, 2, 8),
            ("errIf", 2, 1, 1, 6),
            ("doIt", 1, 0, 0, 1),
        ],
    );
}

const DEFCASE_FIXTURE: &str = include_str!("fixtures/d.go");

#[test]
fn go_case_named_default_is_a_real_case() {
    assert_functions(
        "d.go",
        DEFCASE_FIXTURE,
        &[("caseNamedDefault", 4, 1, 1, 12)],
    );
}

const GEN_FIXTURE: &str = include_str!("fixtures/gen.go");

#[test]
fn go_generics_func_types_and_closures() {
    assert_functions(
        "gen.go",
        GEN_FIXTURE,
        &[
            ("Push", 2, 1, 1, 5),
            // `f func(T) U` is a parameter type, never a function.
            ("Map", 2, 1, 1, 7),
            // deferred/go closures: if@1 2, for@1 2.
            ("outer", 3, 4, 1, 11),
        ],
    );
}

/// 20k nested `if`s through parse -> bridge -> measure. This crate's walks
/// grow their stack on demand (proven on the default test stack by the unit
/// tests); the parser's own tree, however, drops recursively, so the whole
/// pipeline runs on a thread sized for that drop.
#[test]
fn deep_nesting_is_measured_without_overflowing() {
    const DEPTH: usize = 20_000;
    const STACK_FOR_PARSER_TREE_DROP: usize = 256 * 1024 * 1024;
    let mut src = String::from("package main\n\nfunc deep(x int) {\n");
    for _ in 0..DEPTH {
        src.push_str("if true {\n");
    }
    for _ in 0..DEPTH {
        src.push_str("}\n");
    }
    src.push_str("}\n");
    let got = std::thread::Builder::new()
        .stack_size(STACK_FOR_PARSER_TREE_DROP)
        .spawn(move || measure("deep.go", &src))
        .expect("spawn")
        .join()
        .expect("deep measurement thread");
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].name, "deep");
    let depth = i64::try_from(DEPTH).expect("depth fits i64");
    assert_eq!(got[0].cyclomatic_complexity, depth + 1);
    assert_eq!(got[0].nesting_depth, depth);
}
