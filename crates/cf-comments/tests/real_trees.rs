//! Placement and coverage rules proven on REAL parsed trees: every fixture
//! goes through `cf_uast::Parser` (the tree-sitter grammar + the language
//! mapping), so these tests pin what users get, not what hand-built nodes say.
//!
//! Rules under test (see `site/explanation/comments.md`):
//! * a function is documented iff a comment block ends on the line directly
//!   above its declaration (attribute / decorator lines belong to the
//!   declaration);
//! * a comment inside a declaration's span is `inline`;
//! * anything else is `unassociated`;
//! * documentation is keyed by declaration identity, never by name.

use cf_comments::Analyzer;
use cf_gojson::GoValue;
use cf_uast::Parser;

/// Analyzes `source` as `filename` and returns the per-file report map.
fn analyze(filename: &str, source: &str) -> GoValue {
    let root = Parser::new()
        .parse(filename, source.as_bytes())
        .expect("fixture parses");
    Analyzer::new().analyze(Some(&root)).expect("analysis")
}

fn int(report: &GoValue, key: &str) -> i64 {
    match report.as_map().and_then(|m| m.get(key)) {
        Some(GoValue::Int(n)) => *n,
        other => panic!("{key} is not an int: {other:?}"),
    }
}

fn rows<'a>(report: &'a GoValue, key: &str) -> &'a [GoValue] {
    match report.as_map().and_then(|m| m.get(key)) {
        Some(GoValue::Array(items)) => items,
        other => panic!("{key} is not an array: {other:?}"),
    }
}

fn str_field<'a>(row: &'a GoValue, key: &str) -> &'a str {
    match row.as_map().and_then(|m| m.get(key)) {
        Some(GoValue::Str(s)) => s,
        other => panic!("{key} is not a string: {other:?}"),
    }
}

fn int_field(row: &GoValue, key: &str) -> i64 {
    match row.as_map().and_then(|m| m.get(key)) {
        Some(GoValue::Int(n)) => *n,
        other => panic!("{key} is not an int: {other:?}"),
    }
}

/// `(function name, documented?)` for every row of the `functions` table.
fn function_status(report: &GoValue) -> Vec<(String, bool)> {
    rows(report, "functions")
        .iter()
        .map(|row| {
            (
                str_field(row, "function").to_string(),
                str_field(row, "assessment") == "✅ Well Documented",
            )
        })
        .collect()
}

/// `(line, placement, target)` for every row of the `comments` table.
fn comment_placements(report: &GoValue) -> Vec<(i64, String, String)> {
    rows(report, "comments")
        .iter()
        .map(|row| {
            (
                int_field(row, "line"),
                str_field(row, "placement").to_string(),
                str_field(row, "target").to_string(),
            )
        })
        .collect()
}

const GO_NO_COMMENTS: &str = "package a

func Add(a, b int) int {
	return a + b
}

func Sub(a, b int) int {
	return a - b
}
";

#[test]
fn zero_comment_file_keeps_its_functions() {
    let report = analyze("a.go", GO_NO_COMMENTS);
    assert_eq!(int(&report, "total_comments"), 0);
    assert_eq!(int(&report, "total_functions"), 2);
    assert_eq!(int(&report, "documented_functions"), 0);
    assert_eq!(
        function_status(&report),
        vec![("Add".to_string(), false), ("Sub".to_string(), false)]
    );
}

const GO_DUPLICATE_NAMES: &str = "package a

type A struct{}
type B struct{}

// String renders A.
func (A) String() string {
	return \"A\"
}

func (B) String() string {
	return \"B\"
}
";

#[test]
fn duplicate_names_are_documented_independently() {
    let report = analyze("a.go", GO_DUPLICATE_NAMES);
    assert_eq!(int(&report, "documented_functions"), 1);
    let strings: Vec<bool> = function_status(&report)
        .into_iter()
        .filter(|(name, _)| name == "String")
        .map(|(_, documented)| documented)
        .collect();
    assert_eq!(strings, vec![true, false], "only A.String is documented");
}

const RUST_DOC_COMMENT: &str = "/// Documents foo.
pub fn foo() -> i32 {
    1
}

pub fn bar() -> i32 {
    2
}
";

#[test]
fn rust_doc_comment_documents_the_next_item_exactly_once() {
    let report = analyze("a.rs", RUST_DOC_COMMENT);
    assert_eq!(int(&report, "total_comments"), 1);
    assert_eq!(int(&report, "good_comments"), 1);
    assert_eq!(int(&report, "total_functions"), 2);
    assert_eq!(int(&report, "documented_functions"), 1);
    assert_eq!(
        function_status(&report),
        vec![("foo".to_string(), true), ("bar".to_string(), false)]
    );
}

const RUST_ATTRIBUTE_BETWEEN: &str = "/// Doc f.
#[inline]
pub fn f() -> i32 {
    1
}
";

#[test]
fn rust_attribute_between_doc_and_item_keeps_the_item_documented() {
    let report = analyze("a.rs", RUST_ATTRIBUTE_BETWEEN);
    assert_eq!(int(&report, "total_comments"), 1);
    assert_eq!(int(&report, "documented_functions"), 1);
}

const GO_BODY_COMMENT: &str = "package a

func Add(a, b int) int {
	// inline comment
	return a + b
}

func Sub(a, b int) int {
	return a - b // trailing
}
";

#[test]
fn comments_inside_a_body_are_inline_for_that_function() {
    let report = analyze("a.go", GO_BODY_COMMENT);
    assert_eq!(
        comment_placements(&report),
        vec![
            (4, "inline".to_string(), "Add".to_string()),
            (9, "inline".to_string(), "Sub".to_string()),
        ]
    );
    assert_eq!(int(&report, "good_comments"), 0);
    assert_eq!(int(&report, "documented_functions"), 0);
}

const GO_BLANK_LINE_GAPS: &str = "package a

// Doc with one blank line below.

func A() int {
	return 1
}

// Doc with two blank lines below.


func B() int {
	return 2
}

// multi-line doc block line 1
// multi-line doc block line 2


func C() int {
	return 3
}

// Directly above D.
func D() int {
	return 4
}
";

#[test]
fn a_blank_line_between_comment_and_declaration_breaks_documentation() {
    let report = analyze("a.go", GO_BLANK_LINE_GAPS);
    assert_eq!(int(&report, "total_comments"), 5);
    assert_eq!(int(&report, "good_comments"), 1);
    assert_eq!(int(&report, "documented_functions"), 1);
    assert_eq!(
        function_status(&report),
        vec![
            ("A".to_string(), false),
            ("B".to_string(), false),
            ("C".to_string(), false),
            ("D".to_string(), true),
        ]
    );
    let placements: Vec<String> = comment_placements(&report)
        .into_iter()
        .map(|(_, placement, _)| placement)
        .collect();
    assert_eq!(
        placements,
        vec![
            "unassociated",
            "unassociated",
            "unassociated",
            "unassociated",
            "above"
        ]
    );
}

const JAVA_CLASS: &str = "/** Javadoc for A. */
public class A {
    /** Javadoc for foo. */
    public int foo() {
        // inside foo
        return 1;
    }

    public int bar() {
        return 2; // trailing
    }

    /* plain block */
    public int baz() {
        return 3;
    }
}
";

#[test]
fn java_class_body_is_not_a_declaration_and_member_docs_are_above() {
    let report = analyze("A.java", JAVA_CLASS);
    assert_eq!(int(&report, "total_functions"), 4, "A, foo, bar, baz");
    assert_eq!(
        function_status(&report),
        vec![
            ("A".to_string(), true),
            ("foo".to_string(), true),
            ("bar".to_string(), false),
            ("baz".to_string(), true),
        ]
    );
    assert_eq!(
        comment_placements(&report),
        vec![
            (1, "above".to_string(), "A".to_string()),
            (3, "above".to_string(), "foo".to_string()),
            (5, "inline".to_string(), "foo".to_string()),
            (10, "inline".to_string(), "bar".to_string()),
            (13, "above".to_string(), "baz".to_string()),
        ]
    );
}

const PYTHON_MODULE: &str = "# module comment
\"\"\"Module docstring.\"\"\"


# documents foo
def foo():
    \"\"\"Docstring of foo.\"\"\"
    return 1


def bar():
    # inside bar
    return 2


# documents K
@dataclass
class K:
    \"\"\"Class docstring.\"\"\"

    # documents m
    def m(self):
        return 3
";

#[test]
fn python_hash_comments_and_docstrings_document_their_declarations() {
    let report = analyze("a.py", PYTHON_MODULE);
    assert_eq!(
        int(&report, "total_comments"),
        8,
        "5 hash comments + module, foo and K docstrings"
    );
    assert_eq!(int(&report, "total_functions"), 4, "foo, bar, K, m");
    assert_eq!(
        function_status(&report),
        vec![
            ("foo".to_string(), true),
            ("bar".to_string(), false),
            ("K".to_string(), true),
            ("m".to_string(), true),
        ]
    );
    let placements: Vec<String> = comment_placements(&report)
        .into_iter()
        .map(|(_, placement, _)| placement)
        .collect();
    assert_eq!(
        placements,
        vec![
            "unassociated", // # module comment
            "unassociated", // module docstring: no enclosing declaration
            "above",        // # documents foo
            "docstring",    // foo's docstring
            "inline",       // # inside bar
            "above",        // # documents K
            "docstring",    // K's docstring
            "above",        // # documents m
        ]
    );
}

const PYTHON_DOCSTRING_ONLY: &str = "def foo():
    \"\"\"Documents foo.\"\"\"
    return 1


def bar():
    x = 1
    \"\"\"A stray string, not a docstring.\"\"\"
    return x


class K:
    \"\"\"Documents K.\"\"\"

    def m(self):
        \"\"\"Documents m.\"\"\"
        return 2
";

/// A docstring (the first statement of a body) documents its declaration; a
/// string literal later in the body is an inline comment of that body.
#[test]
fn python_docstrings_document_only_as_the_first_body_statement() {
    let report = analyze("d.py", PYTHON_DOCSTRING_ONLY);
    assert_eq!(int(&report, "total_functions"), 4, "foo, bar, K, m");
    assert_eq!(int(&report, "documented_functions"), 3);
    assert_eq!(
        function_status(&report),
        vec![
            ("foo".to_string(), true),
            ("bar".to_string(), false),
            ("K".to_string(), true),
            ("m".to_string(), true),
        ]
    );
    let placements: Vec<(i64, String, String)> = comment_placements(&report);
    let by_line: Vec<(i64, &str, &str)> = placements
        .iter()
        .map(|(line, p, t)| (*line, p.as_str(), t.as_str()))
        .collect();
    assert_eq!(
        by_line,
        vec![
            (2, "docstring", "foo"),
            (8, "inline", "bar"),
            (13, "docstring", "K"),
            (16, "docstring", "m"),
        ]
    );
}

const C_FUNCTIONS: &str = "/* adds */
int add(int a, int b) {
    return a + b;
}

int sub(int a, int b) {
    return a - b;
}
";

#[test]
fn c_function_names_are_identifiers_not_source_text() {
    let report = analyze("a.c", C_FUNCTIONS);
    let names: Vec<String> = function_status(&report)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, vec!["add".to_string(), "sub".to_string()]);
    assert_eq!(int(&report, "documented_functions"), 1);
}
