//! The cohesion analyzer over REAL parsed trees (`cf_uast::Parser`), asserting
//! the hand-computed values of `site/explanation/cohesion.md` and the
//! variable-identification rules of `cf_cohesion::visitor`.

use std::collections::BTreeSet;

use cf_cohesion::{Analyzer, FileReport, ReportValue};
use cf_uast_node::Node;

const EPS: f64 = 1e-9;

fn parse(name: &str, src: &str) -> Node {
    cf_uast::Parser::new()
        .parse(name, src.as_bytes())
        .unwrap_or_else(|e| panic!("parse {name}: {e}"))
}

fn analyze(name: &str, src: &str) -> FileReport {
    Analyzer::new().analyze_file(&parse(name, src))
}

fn cohesion_of(report: &FileReport, name: &str) -> f64 {
    report
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no function {name} in {report:?}"))
        .cohesion
}

fn variables_of(name: &str, src: &str, function: &str) -> BTreeSet<String> {
    let root = parse(name, src);
    cf_cohesion::visitor::collect_functions(&root, None)
        .into_iter()
        .find(|f| f.name == function)
        .unwrap_or_else(|| panic!("no function {function}"))
        .variables
}

fn assert_close(got: f64, want: f64, what: &str) {
    assert!((got - want).abs() < EPS, "{what}: got {got}, want {want}");
}

const GO_SHARE: &str = "package a

func f() int {
	x := 1
	y := 2
	return x + y
}

func g() int {
	x := 3
	return x
}
";

/// f uses {x, y}, g uses {x}: LCOM = 1 - 3/4 = 0.25, score 0.75;
/// f shares x only (0.5), g shares x (1.0); mean 0.75.
#[test]
fn go_share_matches_the_documented_formula() {
    let report = analyze("a.go", GO_SHARE);
    assert_eq!(report.total_functions, 2);
    assert_close(report.lcom, 0.25, "lcom");
    assert_close(report.cohesion_score, 0.75, "cohesion_score");
    assert_close(cohesion_of(&report, "f"), 0.5, "f");
    assert_close(cohesion_of(&report, "g"), 1.0, "g");
    assert_close(report.function_cohesion, 0.75, "function_cohesion");
}

/// Variables are lexical identifier references only: no type names, no
/// function names, no whole declaration tokens.
#[test]
fn go_share_variables_are_identifiers_only() {
    let f: Vec<String> = variables_of("a.go", GO_SHARE, "f").into_iter().collect();
    assert_eq!(f, ["x", "y"]);
    let g: Vec<String> = variables_of("a.go", GO_SHARE, "g").into_iter().collect();
    assert_eq!(g, ["x"]);
}

const GO_DISJOINT: &str = "package a

func f() int {
	x := 1
	x = x + 1
	return x
}

func g() int {
	y := 2
	return y
}
";

/// A variable repeated inside ONE function is not shared: both functions
/// score 0.0 and LCOM = 1 - 2/4 = 0.5.
#[test]
fn go_disjoint_scores_zero_sharing() {
    let report = analyze("a.go", GO_DISJOINT);
    assert_close(report.lcom, 0.5, "lcom");
    assert_close(cohesion_of(&report, "f"), 0.0, "f");
    assert_close(cohesion_of(&report, "g"), 0.0, "g");
    assert_eq!(
        report
            .functions
            .iter()
            .find(|f| f.name == "f")
            .unwrap()
            .variable_count,
        1,
        "variable_count counts distinct variables"
    );
}

/// Functions without variables score 1.0 and the file is perfectly cohesive.
#[test]
fn go_novars_scores_perfectly() {
    let report = analyze("a.go", "package a\n\nfunc f() {\n}\n\nfunc g() {\n}\n");
    assert_eq!(report.total_functions, 2);
    assert_close(report.lcom, 0.0, "lcom");
    assert_close(report.cohesion_score, 1.0, "cohesion_score");
    assert_close(cohesion_of(&report, "f"), 1.0, "f");
    assert_close(cohesion_of(&report, "g"), 1.0, "g");
    assert_close(report.function_cohesion, 1.0, "function_cohesion");
}

/// A single-function file is perfectly cohesive by definition.
#[test]
fn single_function_file_scores_perfectly() {
    let report = analyze(
        "a.go",
        "package a\n\nfunc f() int {\n\tx := 1\n\treturn x\n}\n",
    );
    assert_eq!(report.total_functions, 1);
    assert_close(report.lcom, 0.0, "lcom");
    assert_close(report.cohesion_score, 1.0, "cohesion_score");
    assert_close(cohesion_of(&report, "f"), 1.0, "f");
}

/// Callees are not variables: `caller` calling `helper()` uses nothing.
#[test]
fn go_calls_exclude_callees() {
    let src = "package a\n\nfunc helper() int {\n\treturn 1\n}\n\nfunc caller() int {\n\treturn helper()\n}\n";
    assert!(variables_of("a.go", src, "caller").is_empty());
    let report = analyze("a.go", src);
    assert_close(report.lcom, 0.0, "lcom");
    assert_close(cohesion_of(&report, "caller"), 1.0, "caller");
}

/// Method calls through a member chain exclude only the method name:
/// `cfg.Validate()` uses `cfg`; `fmt.Println(n)` uses `fmt` and `n`.
#[test]
fn go_member_calls_keep_the_receiver_expression() {
    let src = "package main

import \"fmt\"

func use(cfg *Config, items []string) error {
	if cfg == nil {
		return nil
	}
	cfg.Name = items[0]
	fmt.Println(cfg)
	return cfg.Validate()
}
";
    let vars: Vec<String> = variables_of("a.go", src, "use").into_iter().collect();
    assert_eq!(vars, ["Name", "cfg", "fmt", "items"]);
}

/// Go receivers are the implicit `self`: excluded from the variables, and the
/// receiver type qualifies the method name.
#[test]
fn go_struct_receiver_is_excluded_and_qualifies_the_name() {
    let src = "package a

type S struct {
	a int
	b int
}

func (s *S) M1() int {
	return s.a
}

func (s S) M2() int {
	return s.a
}

func (s *S) M3() int {
	return s.b
}
";
    let m1: Vec<String> = variables_of("a.go", src, "S.M1").into_iter().collect();
    assert_eq!(m1, ["a"]);
    let report = analyze("a.go", src);
    // {a}, {a}, {b}: m = 3, a = 2, sum(mA) = 3 -> 1 - 3/6.
    assert_close(report.lcom, 0.5, "lcom");
    assert_close(cohesion_of(&report, "S.M1"), 1.0, "S.M1");
    assert_close(cohesion_of(&report, "S.M3"), 0.0, "S.M3");
}

/// Same-named methods on different types are all kept, qualified by their
/// receiver type.
#[test]
fn go_dup_keeps_every_same_named_function() {
    let src = "package a

type A struct{ v int }
type B struct{ w int }

func (a A) String() string {
	v := a.v
	return string(rune(v))
}

func (b B) String() string {
	w := b.w
	return string(rune(w))
}

func (a A) Len() int {
	v := a.v
	return v
}
";
    let report = analyze("a.go", src);
    let names: Vec<&str> = report.functions.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["A.String", "B.String", "A.Len"]);
    assert_eq!(report.total_functions, 3);
}

/// Rust: `m1` reads `self.a`, `m2` reads `self.b` — disjoint fields, `self`
/// excluded: LCOM = 0.5, both methods 0.0.
#[test]
fn rs_impl_resolves_token_less_identifiers() {
    let src = "pub struct S {
    a: i32,
    b: i32,
}

impl S {
    pub fn m1(&self) -> i32 {
        self.a
    }

    pub fn m2(&self) -> i32 {
        self.b
    }
}
";
    let m1: Vec<String> = variables_of("a.rs", src, "m1").into_iter().collect();
    assert_eq!(m1, ["a"]);
    let m2: Vec<String> = variables_of("a.rs", src, "m2").into_iter().collect();
    assert_eq!(m2, ["b"]);
    let report = analyze("a.rs", src);
    assert_eq!(report.total_functions, 2);
    assert_close(report.lcom, 0.5, "lcom");
    assert_close(cohesion_of(&report, "m1"), 0.0, "m1");
    assert_close(cohesion_of(&report, "m2"), 0.0, "m2");
}

/// Python: `self` is excluded, attribute names count, methods are qualified
/// by their class.
#[test]
fn py_class_counts_attributes_not_self() {
    let src = "class C:
    def __init__(self):
        self.x = 1
        self.y = 2

    def m1(self):
        return self.x

    def m2(self):
        return self.y
";
    let init: Vec<String> = variables_of("a.py", src, "C.__init__")
        .into_iter()
        .collect();
    assert_eq!(init, ["x", "y"]);
    let report = analyze("a.py", src);
    // {x, y}, {x}, {y}: m = 3, a = 2, sum(mA) = 4 -> 1 - 4/6.
    assert_close(report.lcom, 1.0 / 3.0, "lcom");
    assert_close(cohesion_of(&report, "C.__init__"), 1.0, "__init__");
}

/// Every supported language the docs name yields variables for a function
/// that uses some.
#[test]
fn variable_count_is_positive_across_languages() {
    let cases: [(&str, &str, &str); 6] = [
        ("a.go", "package a\n\nfunc f(n int) int {\n\tm := n + 1\n\treturn m\n}\n", "f"),
        ("a.py", "def f(n):\n    m = n + 1\n    return m\n", "f"),
        ("a.js", "function f(n) {\n  const m = n + 1;\n  return m;\n}\n", "f"),
        ("a.rs", "fn f(n: i32) -> i32 {\n    let m = n + 1;\n    m\n}\n", "f"),
        (
            "A.java",
            "public class A {\n    public int f(int n) {\n        int m = n + 1;\n        return m;\n    }\n}\n",
            "A.f",
        ),
        ("a.ts", "function f(n: number): number {\n  const m = n + 1;\n  return m;\n}\n", "f"),
    ];
    for (file, src, function) in cases {
        let report = analyze(file, src);
        let f = report
            .functions
            .iter()
            .find(|f| f.name == function)
            .unwrap_or_else(|| panic!("{file}: no function {function} in {report:?}"));
        assert!(f.variable_count > 0, "{file}: {function} has no variables");
        let vars = variables_of(file, src, function);
        assert!(vars.contains("n") && vars.contains("m"), "{file}: {vars:?}");
        assert!(
            !vars.contains("int") && !vars.contains("i32") && !vars.contains("number"),
            "{file}: type names leaked into {vars:?}"
        );
    }
}

/// `line_count` is the inclusive line span of the function node, never the
/// sum over its descendants.
#[test]
fn line_count_is_the_function_span() {
    let report = analyze("a.go", GO_SHARE);
    let f = report.functions.iter().find(|f| f.name == "f").unwrap();
    assert_eq!((f.start_line, f.line_count), (3, 5));
    let g = report.functions.iter().find(|f| f.name == "g").unwrap();
    assert_eq!((g.start_line, g.line_count), (9, 4));
}

/// A closure is not a function: its variables belong to the enclosing one.
#[test]
fn go_nested_closure_variables_belong_to_the_outer_function() {
    let src = "package a

func outer() int {
	x := 1
	inner := func() int {
		y := 2
		return y
	}
	return x + inner()
}
";
    let report = analyze("a.go", src);
    assert_eq!(report.total_functions, 1);
    let vars: Vec<String> = variables_of("a.go", src, "outer").into_iter().collect();
    assert_eq!(vars, ["inner", "x", "y"]);
}

/// A file without functions is the empty report.
#[test]
fn struct_only_file_has_no_functions() {
    let report = analyze(
        "a.go",
        "package a\n\ntype Config struct {\n\tName string\n\tPort int\n}\n",
    );
    assert_eq!(report, FileReport::empty());
}

/// The discovery depth cap of the history `quality` consumer: functions below
/// the cap are not found.
#[test]
fn find_depth_caps_function_discovery() {
    let root = parse("a.go", GO_SHARE);
    let capped = Analyzer::new()
        .analyze_file_capped(&root, Some(0))
        .to_report();
    assert_eq!(capped.get("total_functions"), Some(&ReportValue::Int(0)));
    let found = Analyzer::new()
        .analyze_file_capped(&root, Some(1))
        .to_report();
    assert_eq!(found.get("total_functions"), Some(&ReportValue::Int(2)));
    assert_eq!(
        found.get("cohesion_score").and_then(ReportValue::as_float),
        Some(0.75)
    );
}
