//! Function discovery and naming.
//!
//! A function is a `Function`/`Method` node (or any node carrying both the
//! `Function` and `Declaration` roles) that has a body. Declaration-only
//! nodes — C prototypes and declarators, Go function types and interface
//! methods, Rust trait signatures, Java abstract/interface methods — are
//! never reported.
//!
//! Nesting rules:
//!
//! * A *named* function nested inside another function (Python `def` in
//!   `def`, JS/Rust inner functions, `const g = function () {}`) is its own
//!   unit; its body is excluded from the enclosing function's metrics.
//! * An *anonymous* function literal nested inside a function (a JS callback
//!   `function (y) { ... }`, a Go/Rust closure) belongs to the enclosing
//!   function and raises its cognitive nesting, exactly like a `Lambda`.
//! * A `Function` whose body is itself an anonymous `Function`/`Lambda` child
//!   (the JS `const f = (x) => {}` / `const g = function () {}` shape) owns
//!   that child as its body: the wrapper never becomes a second function.

use crate::node::{role, uast, Node};

/// Display name for a function without a recoverable name.
pub const ANONYMOUS_FUNCTION_NAME: &str = "anonymous";

/// Longest plausible function name; longer "names" are source slices picked
/// up by a mapping gap and are rejected.
const MAX_NAME_LEN: usize = 200;

/// Node types that only ever appear in a signature.
const SIGNATURE_TYPES: &[&str] = &[
    uast::PARAMETER,
    uast::IDENTIFIER,
    uast::COMMENT,
    "TypeAnnotation",
    "Decorator",
    "Annotation",
    "Modifier",
];

/// Node types whose presence proves a function has executable content even
/// without a `Body`-role block (expression-bodied members).
const CODE_TYPES: &[&str] = &[
    uast::BLOCK,
    uast::IF,
    uast::LOOP,
    uast::SWITCH,
    uast::MATCH,
    uast::CASE,
    uast::TRY,
    uast::CATCH,
    uast::CALL,
    uast::BINARY_OP,
    uast::UNARY_OP,
    uast::LAMBDA,
    uast::BREAK,
    uast::CONTINUE,
    "Return",
    "Assignment",
    "Variable",
    "Literal",
    "Throw",
    "Yield",
    "Await",
    "Index",
    "Field",
];

/// Reports whether `n` is shaped like a function declaration.
pub(crate) fn is_function_candidate(n: &Node) -> bool {
    n.has_any_type(&[uast::FUNCTION, uast::METHOD])
        || n.has_all_roles(&[role::FUNCTION, role::DECLARATION])
}

/// The child that holds a function's executable body: the `Body`-role child,
/// else the first `Block`, else an anonymous `Function`/`Lambda` child that
/// the node merely wraps (JS arrow/function-expression declarations).
pub(crate) fn body_child(n: &Node) -> Option<&Node> {
    if let Some(body) = n.children.iter().find(|c| c.has_role(role::BODY)) {
        return Some(body);
    }
    if let Some(block) = n.children.iter().find(|c| c.is_type(uast::BLOCK)) {
        return Some(block);
    }
    n.children.iter().find(|c| {
        c.is_type(uast::LAMBDA)
            || (c.has_any_type(&[uast::FUNCTION, uast::METHOD])
                && !has_name(c, None)
                && !is_declaration_only(c))
    })
}

/// True when `n` declares a signature without any executable content.
pub(crate) fn is_declaration_only(n: &Node) -> bool {
    body_child(n).is_none() && !has_code_outside_signature(n)
}

/// Pre-order scan for a [`CODE_TYPES`] node, skipping signature subtrees
/// (parameter lists carry default-value literals that are not code).
fn has_code_outside_signature(n: &Node) -> bool {
    let mut stack: Vec<&Node> = n.children.iter().collect();
    while let Some(node) = stack.pop() {
        if node.has_any_type(SIGNATURE_TYPES) {
            continue;
        }
        if node.has_any_type(CODE_TYPES) {
            return true;
        }
        stack.extend(node.children.iter());
    }
    false
}

/// True when the name-role child of `n` is the function's own body wrapper
/// (the inner anonymous function of `const g = function () {}`).
pub(crate) fn is_body_wrapper(parent: &Node, child: &Node) -> bool {
    child.has_any_type(&[uast::FUNCTION, uast::METHOD, uast::LAMBDA])
        && body_child(parent).is_some_and(|b| std::ptr::eq(b, child))
}

/// Finds every reportable function below `root` in pre-order.
///
/// `find_depth` caps discovery: only candidates at depth `<= cap` below the
/// root (root at depth 0) are found; the history `quality` analyzer runs with
/// a cap, the static surfaces without one.
pub(crate) fn find_functions<'a>(
    root: &'a Node,
    source: Option<&[u8]>,
    find_depth: Option<usize>,
) -> Vec<&'a Node> {
    struct Frame<'a> {
        node: &'a Node,
        depth: usize,
        inside_function: bool,
        is_wrapper: bool,
    }

    let mut found: Vec<&'a Node> = Vec::new();
    let mut stack = vec![Frame {
        node: root,
        depth: 0,
        inside_function: false,
        is_wrapper: false,
    }];
    while let Some(frame) = stack.pop() {
        let n = frame.node;
        let within_cap = find_depth.map_or(true, |cap| frame.depth <= cap);
        let is_found = within_cap
            && !frame.is_wrapper
            && is_function_candidate(n)
            && !is_declaration_only(n)
            && (!frame.inside_function || has_name(n, source));
        if is_found {
            found.push(n);
        }
        for child in n.children.iter().rev() {
            stack.push(Frame {
                node: child,
                depth: frame.depth + 1,
                inside_function: frame.inside_function || is_found,
                is_wrapper: is_found && is_body_wrapper(n, child),
            });
        }
    }
    found
}

/// True when a plausible name can be recovered for `n`.
pub(crate) fn has_name(n: &Node, source: Option<&[u8]>) -> bool {
    recover_name(n, source).is_some()
}

/// The display name of a function: a `name`-like prop, else its `Name`-role
/// identifier (through a C-style declarator child), else an identifier in
/// the signature, else [`ANONYMOUS_FUNCTION_NAME`]. Multi-line or oversized
/// tokens are never names.
pub(crate) fn function_name(n: &Node, source: Option<&[u8]>) -> String {
    recover_name(n, source).unwrap_or_else(|| ANONYMOUS_FUNCTION_NAME.to_string())
}

fn recover_name(n: &Node, source: Option<&[u8]>) -> Option<String> {
    for key in ["name", "function_name", "method_name"] {
        if let Some(name) = n.prop(key).and_then(plausible_name) {
            return Some(name);
        }
    }
    if let Some(name) = direct_name_identifier(n, source) {
        return Some(name);
    }
    // C/C++ declarator: `Function{Identifier(Type), Function{Identifier(Name), List}, Block}`.
    for declarator in n
        .children
        .iter()
        .filter(|c| c.has_any_type(&[uast::FUNCTION, uast::METHOD]) && is_declaration_only(c))
    {
        if let Some(name) = direct_name_identifier(declarator, source) {
            return Some(name);
        }
    }
    signature_name_identifier(n, source)
}

/// The text of a `Name`-role, non-`Type` identifier among the direct children.
fn direct_name_identifier(n: &Node, source: Option<&[u8]>) -> Option<String> {
    n.children
        .iter()
        .filter(|c| is_name_identifier(c))
        .find_map(|c| identifier_text(c, source))
}

/// Last resort: the first `Name`-role identifier anywhere in the signature
/// part (every child before the body), skipping parameter lists (by type or
/// by role: JS wraps its parameters in a `Parameter`-role `Synthetic`).
fn signature_name_identifier(n: &Node, source: Option<&[u8]>) -> Option<String> {
    let body = body_child(n);
    let mut stack: Vec<&Node> = n
        .children
        .iter()
        .rev()
        .filter(|c| !body.is_some_and(|b| std::ptr::eq(b, *c)))
        .collect();
    while let Some(node) = stack.pop() {
        if node.is_type(uast::PARAMETER) || node.has_role(role::PARAMETER) {
            continue;
        }
        if is_name_identifier(node) {
            if let Some(name) = identifier_text(node, source) {
                return Some(name);
            }
        }
        stack.extend(node.children.iter().rev());
    }
    None
}

fn is_name_identifier(n: &Node) -> bool {
    n.is_type(uast::IDENTIFIER) && n.has_role(role::NAME) && !n.has_role(role::TYPE)
}

/// An identifier's text: its token, else its source span.
pub(crate) fn identifier_text(n: &Node, source: Option<&[u8]>) -> Option<String> {
    if let Some(name) = plausible_name(&n.token) {
        return Some(name);
    }
    n.span_text(source).and_then(plausible_name)
}

fn plausible_name(raw: &str) -> Option<String> {
    let name = raw.trim();
    if name.is_empty() || name.len() > MAX_NAME_LEN || name.contains('\n') {
        return None;
    }
    Some(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::{find_functions, function_name, is_declaration_only, ANONYMOUS_FUNCTION_NAME};
    use crate::node::{role, uast, Node};

    fn func(name: Option<&str>, children: Vec<Node>) -> Node {
        let mut f = Node::new(uast::FUNCTION)
            .with_roles(vec![role::FUNCTION, role::DECLARATION])
            .with_children(children);
        if let Some(n) = name {
            f = f.with_prop("name", n);
        }
        f
    }

    fn body() -> Node {
        Node::new(uast::BLOCK).with_roles(vec![role::BODY])
    }

    #[test]
    fn declaration_only_functions_are_not_found() {
        let proto = func(Some("proto"), vec![Node::new(uast::PARAMETER)]);
        assert!(is_declaration_only(&proto));
        let root = Node::new(uast::FILE).with_children(vec![proto]);
        assert!(find_functions(&root, None, None).is_empty());
    }

    #[test]
    fn expression_bodied_member_counts_as_a_function() {
        let member = func(
            Some("Expr"),
            vec![Node::new(uast::SYNTHETIC).with_children(vec![Node::new(uast::IF)])],
        );
        assert!(!is_declaration_only(&member));
    }

    #[test]
    fn c_declarator_is_not_a_second_function_and_names_the_outer() {
        let declarator = func(
            None,
            vec![
                Node::with_token(uast::IDENTIFIER, "c_decisions").with_roles(vec![role::NAME]),
                Node::new("List"),
            ],
        );
        let outer = func(
            None,
            vec![
                Node::with_token(uast::IDENTIFIER, "int").with_roles(vec![role::TYPE, role::NAME]),
                declarator,
                body(),
            ],
        );
        let root = Node::new(uast::FILE).with_children(vec![outer]);
        let found = find_functions(&root, None, None);
        assert_eq!(found.len(), 1);
        assert_eq!(function_name(found[0], None), "c_decisions");
    }

    #[test]
    fn multi_line_tokens_are_never_names() {
        let f = Node::with_token(uast::FUNCTION, "int f() {\n  return 1;\n}")
            .with_roles(vec![role::FUNCTION, role::DECLARATION])
            .with_children(vec![body()]);
        assert_eq!(function_name(&f, None), ANONYMOUS_FUNCTION_NAME);
    }

    #[test]
    fn name_comes_from_source_span_when_the_token_is_empty() {
        let ident = Node::new(uast::IDENTIFIER)
            .with_roles(vec![role::NAME])
            .with_pos(crate::node::Positions {
                start_offset: 3,
                end_offset: 8,
                ..Default::default()
            });
        let f = func(None, vec![ident, body()]);
        assert_eq!(function_name(&f, Some(b"fn outer() {}")), "outer");
        assert_eq!(function_name(&f, None), ANONYMOUS_FUNCTION_NAME);
    }

    #[test]
    fn named_nested_functions_are_separate_but_anonymous_ones_are_not() {
        let named_inner = func(Some("inner"), vec![body()]);
        let anonymous_inner = func(None, vec![body()]);
        let outer = func(
            Some("outer"),
            vec![body().with_children(vec![named_inner, anonymous_inner])],
        );
        let root = Node::new(uast::FILE).with_children(vec![outer]);
        let names: Vec<String> = find_functions(&root, None, None)
            .into_iter()
            .map(|f| function_name(f, None))
            .collect();
        assert_eq!(names, vec!["outer", "inner"]);
    }

    #[test]
    fn top_level_anonymous_function_is_found() {
        let root = Node::new(uast::FILE).with_children(vec![func(None, vec![body()])]);
        assert_eq!(find_functions(&root, None, None).len(), 1);
    }

    #[test]
    fn body_wrapper_function_is_not_a_second_function() {
        // const g = function () {} : Function(g){Identifier, Function{Block}}.
        let inner = func(None, vec![body()]);
        let g = func(
            Some("g"),
            vec![
                Node::with_token(uast::IDENTIFIER, "g").with_roles(vec![role::NAME]),
                inner,
            ],
        );
        let root = Node::new(uast::FILE).with_children(vec![g]);
        let found = find_functions(&root, None, None);
        assert_eq!(found.len(), 1);
        assert_eq!(function_name(found[0], None), "g");
    }

    #[test]
    fn find_depth_caps_discovery() {
        let deep = func(Some("deep"), vec![body()]);
        let wrapper = Node::new(uast::BLOCK).with_children(vec![deep]);
        let shallow = func(Some("shallow"), vec![body()]);
        let root = Node::new(uast::FILE).with_children(vec![shallow, wrapper]);
        assert_eq!(find_functions(&root, None, Some(1)).len(), 1);
        assert_eq!(find_functions(&root, None, Some(2)).len(), 2);
        assert_eq!(find_functions(&root, None, None).len(), 2);
    }
}
