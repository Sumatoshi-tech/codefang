//! The operator / operand classification table.
//!
//! Halstead counts two kinds of tokens. This module is the single place that
//! says which UAST node is which, and how a node's operator text is recovered
//! when the mapping does not spell it out:
//!
//! | UAST node                                   | Class                | Name                                   |
//! |---------------------------------------------|----------------------|----------------------------------------|
//! | leaf `Identifier` / `Literal`               | operand              | `token`, else source span, else `props.name` / `props.value` |
//! | leaf `Identifier` with the `Type` role      | operator (type spec) | same text sources as an operand        |
//! | node with `props["operator"]`               | operator             | the property, verbatim (nothing else from that node) |
//! | `Call` / `Index` / `Slice`                  | operator             | fixed `()` / `[]` / `[:]`              |
//! | any other node inside a function body       | operator(s)          | the node's *residue* (see below)       |
//! | `Parameter` (type or role), `Comment`       | ignored subtree      | —                                      |
//! | function name / signature                   | ignored              | —                                      |
//!
//! The **residue** of a node is its source span with every child's span
//! blanked out: for `a + b` it is ` + `, for `c := a + b` it is ` := `, for
//! `return c * 2` it is `return `, for `fmt.Println` it is `.`, for `r++` it
//! is `++`. The residue is tokenized into runs of [`SYMBOL_CHARS`] (always
//! operators) and words (operators only when listed in
//! [`KEYWORD_OPERATORS`]); brackets, commas, semicolons, quotes and a lone
//! `:` are delimiters. A node TYPE name is never an operator, and a node's
//! whole text is never an operand.
//!
//! Without any recoverable source text, keyword statements fall back to the
//! [`CANONICAL_KEYWORDS`] spelling of their type (`Return` → `return`), and
//! symbol nodes are left uncounted rather than mislabelled.

/// Node types that open a function context (a per-function record).
pub const FUNCTION_TYPES: &[&str] = &["Function", "Method"];

/// Role pair that marks a function-declaration node whatever its type is.
pub const FUNCTION_ROLES: &[&str] = &["Function", "Declaration"];

/// Node types whose leaves are operands.
pub const OPERAND_TYPES: &[&str] = &["Identifier", "Literal"];

/// Role that turns a leaf identifier into a type specifier (an operator).
pub const TYPE_ROLE: &str = "Type";

/// Parameter nodes (by type or role) belong to the signature and are skipped.
pub const PARAMETER: &str = "Parameter";

/// Node types whose subtrees are never scanned.
pub const IGNORED_TYPES: &[&str] = &["Comment", "DocString"];

/// Role / type that marks a function's body among its children.
pub const BODY_ROLE: &str = "Body";
/// Block node type (a body without the `Body` role).
pub const BLOCK_TYPE: &str = "Block";

/// Property that carries a node's operator text when the mapping exposes it.
pub const OPERATOR_PROP: &str = "operator";
/// Property carrying a declaration / entity name.
pub const NAME_PROP: &str = "name";
/// Property carrying a literal value.
pub const VALUE_PROP: &str = "value";

/// Operators fixed by node type: a call's parentheses, an index, a slice.
pub const FIXED_OPERATORS: &[(&str, &str)] = &[("Call", "()"), ("Index", "[]"), ("Slice", "[:]")];

/// Keyword spelling used for a statement node when no source text can be
/// recovered for it.
pub const CANONICAL_KEYWORDS: &[(&str, &str)] = &[
    ("Await", "await"),
    ("Break", "break"),
    ("Case", "case"),
    ("Catch", "catch"),
    ("Continue", "continue"),
    ("Finally", "finally"),
    ("If", "if"),
    ("Loop", "loop"),
    ("Match", "match"),
    ("Return", "return"),
    ("Spread", "..."),
    ("Switch", "switch"),
    ("Throw", "throw"),
    ("Try", "try"),
    ("Yield", "yield"),
];

/// Characters that form operator symbols; a maximal run is one operator.
pub const SYMBOL_CHARS: &str = "+-*/%=<>!&|^~?:.@#$";

/// A lone colon is block / label punctuation, not an operator.
const LONE_COLON: &str = ":";
/// The conditional operator, assembled from the `?` and `:` of one node.
const TERNARY: &str = "?:";
const QUESTION: &str = "?";

/// Words that count as operators when they appear in a node's residue.
/// Sorted (binary search); language keywords that act as operators across
/// the supported grammars, including declaration and primitive-type keywords.
pub const KEYWORD_OPERATORS: &[&str] = &[
    "abstract",
    "alignof",
    "and",
    "as",
    "assert",
    "async",
    "await",
    "begin",
    "bool",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "chan",
    "char",
    "class",
    "const",
    "constexpr",
    "continue",
    "decltype",
    "def",
    "default",
    "defer",
    "del",
    "delete",
    "do",
    "double",
    "dyn",
    "elif",
    "else",
    "elsif",
    "end",
    "ensure",
    "enum",
    "except",
    "explicit",
    "extends",
    "extern",
    "f32",
    "f64",
    "fallthrough",
    "final",
    "finally",
    "float",
    "fn",
    "for",
    "foreach",
    "from",
    "func",
    "function",
    "global",
    "go",
    "goto",
    "i128",
    "i16",
    "i32",
    "i64",
    "i8",
    "if",
    "impl",
    "implements",
    "import",
    "in",
    "inline",
    "instanceof",
    "int",
    "interface",
    "is",
    "isize",
    "lambda",
    "let",
    "long",
    "loop",
    "match",
    "move",
    "mut",
    "namespace",
    "new",
    "nonlocal",
    "not",
    "of",
    "or",
    "override",
    "package",
    "pass",
    "private",
    "protected",
    "pub",
    "public",
    "raise",
    "range",
    "register",
    "repeat",
    "rescue",
    "return",
    "select",
    "short",
    "signed",
    "sizeof",
    "static",
    "str",
    "string",
    "struct",
    "switch",
    "template",
    "then",
    "throw",
    "throws",
    "trait",
    "try",
    "type",
    "typedef",
    "typename",
    "typeof",
    "u128",
    "u16",
    "u32",
    "u64",
    "u8",
    "unless",
    "unsafe",
    "unsigned",
    "until",
    "use",
    "using",
    "usize",
    "val",
    "var",
    "virtual",
    "void",
    "volatile",
    "when",
    "where",
    "while",
    "with",
    "xor",
    "yield",
];

/// True when `word` is a keyword operator.
#[must_use]
pub fn is_keyword_operator(word: &str) -> bool {
    KEYWORD_OPERATORS.binary_search(&word).is_ok()
}

/// The fixed operator for a node type, if it has one.
#[must_use]
pub fn fixed_operator(node_type: &str) -> Option<&'static str> {
    FIXED_OPERATORS
        .iter()
        .find(|(t, _)| *t == node_type)
        .map(|(_, op)| *op)
}

/// The canonical keyword for a statement node type, if it has one.
#[must_use]
pub fn canonical_keyword(node_type: &str) -> Option<&'static str> {
    CANONICAL_KEYWORDS
        .iter()
        .find(|(t, _)| *t == node_type)
        .map(|(_, kw)| *kw)
}

/// True when `token` is exactly one operator: a symbol run or a keyword.
#[must_use]
pub fn is_bare_operator(token: &str) -> bool {
    !token.is_empty()
        && (token.chars().all(|c| SYMBOL_CHARS.contains(c)) && token != LONE_COLON
            || is_keyword_operator(token))
}

/// Character classes of residue text.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CharClass {
    Word,
    Symbol,
    Delimiter,
}

fn class_of(c: char) -> CharClass {
    if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else if SYMBOL_CHARS.contains(c) {
        CharClass::Symbol
    } else {
        CharClass::Delimiter
    }
}

/// Tokenizes a node's residue into its operators: every symbol run, every
/// keyword word; a lone `:` is dropped unless it completes a `?` into the
/// conditional operator `?:`.
///
/// ```
/// use cf_halstead::classify::operator_tokens;
/// assert_eq!(operator_tokens(" := "), vec![":="]);
/// assert_eq!(operator_tokens("return "), vec!["return"]);
/// assert_eq!(operator_tokens("for , := range "), vec!["for", ":=", "range"]);
/// assert_eq!(operator_tokens(" ? : "), vec!["?:"]);
/// assert_eq!(operator_tokens("( ) ; Foo"), Vec::<String>::new());
/// ```
#[must_use]
pub fn operator_tokens(residue: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut run = String::new();
    let mut run_class = CharClass::Delimiter;
    for c in residue.chars().chain(std::iter::once(' ')) {
        let class = class_of(c);
        if class != run_class {
            push_run(&mut out, run_class, &run);
            run.clear();
            run_class = class;
        }
        if class != CharClass::Delimiter {
            run.push(c);
        }
    }
    out
}

fn push_run(out: &mut Vec<String>, class: CharClass, run: &str) {
    match class {
        CharClass::Word if is_keyword_operator(run) => out.push(run.to_string()),
        CharClass::Symbol if run == LONE_COLON => {
            if out.last().is_some_and(|last| last == QUESTION) {
                out.pop();
                out.push(TERNARY.to_string());
            }
        }
        CharClass::Symbol => out.push(run.to_string()),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_table_is_sorted_for_binary_search() {
        let mut sorted = KEYWORD_OPERATORS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted, KEYWORD_OPERATORS,
            "KEYWORD_OPERATORS must be sorted and unique"
        );
    }

    #[test]
    fn keywords_are_operators_identifiers_are_not() {
        assert!(is_keyword_operator("return"));
        assert!(is_keyword_operator("int"));
        assert!(!is_keyword_operator("Println"));
        assert!(!is_keyword_operator("Return"));
    }

    #[test]
    fn symbol_runs_are_split_by_delimiters_and_words() {
        assert_eq!(operator_tokens("a + b"), vec!["+"]);
        assert_eq!(operator_tokens(" += "), vec!["+="]);
        assert_eq!(operator_tokens("++;"), vec!["++"]);
        assert_eq!(operator_tokens("let   = ;"), vec!["let", "="]);
        assert_eq!(operator_tokens("if   else "), vec!["if", "else"]);
        assert_eq!(operator_tokens("::"), vec!["::"]);
        assert_eq!(operator_tokens("->"), vec!["->"]);
    }

    #[test]
    fn lone_colon_is_punctuation_but_completes_a_ternary() {
        assert_eq!(operator_tokens("for ( : )"), vec!["for"]);
        assert_eq!(operator_tokens("case 1:"), vec!["case"]);
        assert_eq!(operator_tokens(" ? : "), vec!["?:"]);
        assert_eq!(operator_tokens("x if  else "), vec!["if", "else"]);
    }

    #[test]
    fn quotes_and_brackets_never_become_operators() {
        assert!(operator_tokens("( \"x\" ) [ ] { }").is_empty());
        assert!(operator_tokens("Foo bar").is_empty());
    }

    #[test]
    fn bare_operator_tokens() {
        assert!(is_bare_operator("++"));
        assert!(is_bare_operator("defer"));
        assert!(is_bare_operator("mut"));
        assert!(!is_bare_operator(":"));
        assert!(!is_bare_operator(""));
        assert!(!is_bare_operator("fmt.Println(\"x\")"));
        assert!(!is_bare_operator("BinaryOp"));
    }

    #[test]
    fn fixed_and_canonical_tables() {
        assert_eq!(fixed_operator("Call"), Some("()"));
        assert_eq!(fixed_operator("Index"), Some("[]"));
        assert_eq!(fixed_operator("BinaryOp"), None);
        assert_eq!(canonical_keyword("Return"), Some("return"));
        assert_eq!(canonical_keyword("BinaryOp"), None);
    }
}
