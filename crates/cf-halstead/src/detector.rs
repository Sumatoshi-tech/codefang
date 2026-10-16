//! Function discovery and operator / operand collection.
//!
//! One pre-order walk over the UAST with a stack of open function contexts:
//! every operator and operand inside a function body is attributed to the
//! innermost enclosing function (a closure's tokens never leak into its
//! parent), the function's own signature (name, parameters, return type,
//! modifiers) is skipped, and nodes outside every function are only searched
//! for further functions. The classification itself is the table in
//! [`crate::classify`]; text recovery goes through [`crate::source`].
//!
//! This is the ONE detector: the static `static/halstead` report and the
//! `history/quality` file-level measures both run [`collect_functions`].

use std::collections::BTreeMap;

use cf_uast_node::Node;

use crate::calculator::HalsteadCounts;
use crate::classify::{
    canonical_keyword, fixed_operator, is_bare_operator, operator_tokens, BLOCK_TYPE, BODY_ROLE,
    FUNCTION_ROLES, FUNCTION_TYPES, IGNORED_TYPES, NAME_PROP, OPERAND_TYPES, OPERATOR_PROP,
    PARAMETER, TYPE_ROLE, VALUE_PROP,
};
use crate::source::{residue, span, Window};

/// Name reported for a function whose name cannot be recovered.
pub const ANONYMOUS: &str = "anonymous";

/// UAST type of identifier nodes.
const IDENTIFIER: &str = "Identifier";
/// UAST type of literal nodes.
const LITERAL: &str = "Literal";
/// Role marking a declaration's name identifier.
const NAME_ROLE: &str = "Name";
/// Wrapper nodes with no children of their own that sit in a signature
/// (modifiers such as `public`, bare return types such as `int`).
const SYNTHETIC: &str = "Synthetic";
/// Type annotations are signature parts.
const TYPE_ANNOTATION: &str = "TypeAnnotation";

/// The raw operator / operand multisets of one function.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FunctionCounts {
    /// Function name (`props["name"]`, else its name identifier, else
    /// [`ANONYMOUS`]).
    pub name: String,
    /// Operator text → occurrences.
    pub operators: BTreeMap<String, i64>,
    /// Operand text → occurrences.
    pub operands: BTreeMap<String, i64>,
}

impl FunctionCounts {
    /// The four Halstead counts (n1, n2, N1, N2) of these multisets.
    #[must_use]
    pub fn totals(&self) -> HalsteadCounts {
        HalsteadCounts {
            distinct_operators: to_i64(self.operators.len()),
            distinct_operands: to_i64(self.operands.len()),
            total_operators: self.operators.values().sum(),
            total_operands: self.operands.values().sum(),
        }
    }
}

/// Collects every function of `root` with its operator / operand multisets,
/// in post-order (a function is emitted when its subtree is complete, so a
/// nested closure precedes its parent).
///
/// `source` is the file the tree was parsed from; with it, token-less nodes
/// are read back by span. Without it, a node's own verbatim token or the
/// enclosing function's verbatim token serves as the window, and a node whose
/// text cannot be recovered at all is left uncounted.
///
/// ```
/// use cf_halstead::detector::collect_functions;
/// use cf_uast_node::{Node, Positions};
///
/// fn at(t: &str, tok: &str, s: u64, e: u64) -> Node {
///     let mut n = Node::with_token(t, tok);
///     n.pos = Some(Positions { start_offset: s, end_offset: e, ..Positions::default() });
///     n
/// }
/// // func f() { return x + 1 }
/// let src = b"func f() { return x + 1 }";
/// let mut f = at("Function", "", 0, 25);
/// f.props.insert("name".into(), "f".into());
/// let mut body = at("Block", "", 9, 25);
/// body.roles = vec!["Body".into()];
/// let mut ret = at("Return", "", 11, 23);
/// let mut plus = at("BinaryOp", "", 18, 23);
/// plus.add_child(at("Identifier", "x", 18, 19));
/// plus.add_child(at("Literal", "", 22, 23));
/// ret.add_child(plus);
/// body.add_child(ret);
/// f.add_child(body);
/// let mut root = Node::with_token("File", "");
/// root.add_child(f);
///
/// let fns = collect_functions(&root, Some(src));
/// assert_eq!(fns.len(), 1);
/// assert_eq!(fns[0].name, "f");
/// assert_eq!(fns[0].operators.get("return"), Some(&1));
/// assert_eq!(fns[0].operators.get("+"), Some(&1));
/// assert_eq!(fns[0].operands.get("x"), Some(&1));
/// assert_eq!(fns[0].operands.get("1"), Some(&1));
/// ```
#[must_use]
pub fn collect_functions<'a>(root: &'a Node, source: Option<&'a [u8]>) -> Vec<FunctionCounts> {
    Collector {
        file: source.map(Window::file),
        contexts: Vec::new(),
        out: Vec::new(),
    }
    .run(root)
}

/// True when the node opens a function context.
#[must_use]
pub fn is_function_node(node: &Node) -> bool {
    FUNCTION_TYPES.contains(&node.node_type.as_str()) || node.has_all_roles(FUNCTION_ROLES)
}

struct Context<'a> {
    counts: FunctionCounts,
    /// The function's verbatim token, used as the text window when the walk
    /// has no file source.
    window: Option<Window<'a>>,
}

struct Frame<'a> {
    node: &'a Node,
    next: usize,
    opened: bool,
}

struct Collector<'a> {
    file: Option<Window<'a>>,
    contexts: Vec<Context<'a>>,
    out: Vec<FunctionCounts>,
}

impl<'a> Collector<'a> {
    fn run(mut self, root: &'a Node) -> Vec<FunctionCounts> {
        let mut stack: Vec<Frame<'a>> = vec![self.enter(root)];
        while let Some(top) = stack.last_mut() {
            let node: &'a Node = top.node;
            if let Some(child) = node.children.get(top.next) {
                top.next += 1;
                let frame = self.enter(child);
                stack.push(frame);
                continue;
            }
            let done = stack.pop().expect("stack is non-empty inside the loop");
            if done.opened {
                if let Some(ctx) = self.contexts.pop() {
                    self.out.push(ctx.counts);
                }
            }
        }
        self.out
    }

    /// Classifies `node` on entry and returns the frame that drives the
    /// descent into its children (none for skipped subtrees, the body only
    /// for a function).
    fn enter(&mut self, node: &'a Node) -> Frame<'a> {
        if is_function_node(node) {
            self.open_context(node);
            return Frame {
                node,
                next: body_start(node),
                opened: true,
            };
        }
        if self.contexts.is_empty() {
            return Frame {
                node,
                next: 0,
                opened: false,
            };
        }
        let descend = self.classify(node);
        Frame {
            node,
            next: if descend { 0 } else { node.children.len() },
            opened: false,
        }
    }

    fn open_context(&mut self, node: &'a Node) {
        let window = if self.file.is_some() {
            None
        } else {
            Window::of_token(node)
        };
        let name = self.function_name(node, window);
        self.contexts.push(Context {
            counts: FunctionCounts {
                name,
                ..FunctionCounts::default()
            },
            window,
        });
    }

    /// Records the node's tokens; returns whether to descend into children.
    fn classify(&mut self, node: &'a Node) -> bool {
        if is_ignored(node) {
            return false;
        }
        if is_operand_node(node) && counted_whole(node) {
            if let Some(text) = self.text_of(node) {
                if node.has_any_role(&[TYPE_ROLE]) {
                    self.record_operator(text);
                } else {
                    self.record_operand(text);
                }
            }
            return false;
        }
        for op in self.operators_of(node) {
            self.record_operator(op);
        }
        true
    }

    /// The operators a non-operand node contributes: the explicit
    /// `operator` property alone when present; otherwise the type's fixed
    /// operator plus the residue tokens, or — without recoverable residue —
    /// the node's own bare token or its canonical keyword.
    fn operators_of(&self, node: &'a Node) -> Vec<String> {
        if let Some(op) = node.props.get(OPERATOR_PROP).filter(|s| !s.is_empty()) {
            return vec![op.clone()];
        }
        let mut ops: Vec<String> = fixed_operator(&node.node_type)
            .map(str::to_string)
            .into_iter()
            .collect();
        match self.residue_of(node) {
            Some(res) => ops.extend(operator_tokens(&res)),
            None => ops.extend(token_fallback(node)),
        }
        ops
    }

    /// The node's residue through the first window that covers it: the file,
    /// the node's own verbatim token, the enclosing function's token.
    fn residue_of(&self, node: &'a Node) -> Option<String> {
        let context_window = self.contexts.last().and_then(|c| c.window);
        [self.file, Window::of_token(node), context_window]
            .iter()
            .flatten()
            .find_map(|w| residue(node, w))
    }

    /// A leaf's text: its token, else its span read through a window, else
    /// its `name` / `value` property.
    fn text_of(&self, node: &Node) -> Option<String> {
        let context_window = self.contexts.last().and_then(|c| c.window);
        self.text_through(node, context_window)
    }

    fn text_through(&self, node: &Node, context_window: Option<Window<'a>>) -> Option<String> {
        if !node.token.is_empty() {
            return Some(node.token.clone());
        }
        if let Some((start, end)) = span(node) {
            let found = [self.file, context_window]
                .iter()
                .flatten()
                .find_map(|w| w.slice(start, end));
            if let Some(bytes) = found {
                return Some(String::from_utf8_lossy(bytes).into_owned());
            }
        }
        node.props
            .get(NAME_PROP)
            .or_else(|| node.props.get(VALUE_PROP))
            .filter(|s| !s.is_empty())
            .cloned()
    }

    /// `props["name"]`, else the name identifier among the direct children,
    /// else the name of a nested declarator function (C), else
    /// [`ANONYMOUS`].
    fn function_name(&self, node: &Node, window: Option<Window<'a>>) -> String {
        if let Some(name) = node.props.get(NAME_PROP).filter(|s| !s.is_empty()) {
            return name.clone();
        }
        let named = node.children.iter().find(|c| {
            c.node_type == IDENTIFIER
                && c.has_any_role(&[NAME_ROLE])
                && !c.has_any_role(&[TYPE_ROLE])
        });
        if let Some(text) = named.and_then(|c| self.text_through(c, window)) {
            return text;
        }
        if let Some(inner) = node.children.iter().find(|c| is_function_node(c)) {
            return self.function_name(inner, window);
        }
        ANONYMOUS.to_string()
    }

    fn record_operator(&mut self, op: String) {
        if let Some(ctx) = self.contexts.last_mut() {
            *ctx.counts.operators.entry(op).or_insert(0) += 1;
        }
    }

    fn record_operand(&mut self, operand: String) {
        if let Some(ctx) = self.contexts.last_mut() {
            *ctx.counts.operands.entry(operand).or_insert(0) += 1;
        }
    }
}

/// Without residue: the node's own token when it is exactly one operator,
/// else the canonical keyword of its type.
fn token_fallback(node: &Node) -> Option<String> {
    if is_bare_operator(&node.token) {
        return Some(node.token.clone());
    }
    canonical_keyword(&node.node_type).map(str::to_string)
}

/// Index of the first body child of a function node; children before it are
/// the signature. Without an explicit body, everything after the last
/// signature part (name, parameters, modifiers, type annotations) is body.
fn body_start(node: &Node) -> usize {
    if let Some(i) = node
        .children
        .iter()
        .position(|c| c.has_any_role(&[BODY_ROLE]) || c.node_type == BLOCK_TYPE)
    {
        return i;
    }
    node.children
        .iter()
        .rposition(is_signature_part)
        .map_or(0, |i| i + 1)
}

fn is_signature_part(node: &Node) -> bool {
    node.node_type == IDENTIFIER
        || is_parameter(node)
        || node.node_type == TYPE_ANNOTATION
        || (node.node_type == SYNTHETIC && node.children.is_empty())
}

fn is_parameter(node: &Node) -> bool {
    node.node_type == PARAMETER || node.has_any_role(&[PARAMETER])
}

fn is_ignored(node: &Node) -> bool {
    IGNORED_TYPES.contains(&node.node_type.as_str()) || is_parameter(node)
}

fn is_operand_node(node: &Node) -> bool {
    OPERAND_TYPES.contains(&node.node_type.as_str())
}

/// An operand node is counted by its own text when it is a leaf, a quoted
/// literal wrapping its content literal, or a composite with no operand
/// parts; otherwise its parts are classified individually.
fn counted_whole(node: &Node) -> bool {
    if node.children.is_empty() {
        return true;
    }
    if node.node_type == LITERAL
        && node.children.len() == 1
        && node.children[0].node_type == LITERAL
    {
        return true;
    }
    !has_operand_descendant(node)
}

fn has_operand_descendant(node: &Node) -> bool {
    let mut stack: Vec<&Node> = node.children.iter().collect();
    while let Some(n) = stack.pop() {
        if is_operand_node(n) {
            return true;
        }
        stack.extend(n.children.iter());
    }
    false
}

fn to_i64(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_uast_node::Positions;

    /// A node positioned at `[start, end)` with the given token.
    fn at(node_type: &str, token: &str, start: u64, end: u64) -> Node {
        let mut n = Node::with_token(node_type, token);
        n.pos = Some(Positions {
            start_offset: start,
            end_offset: end,
            ..Positions::default()
        });
        n
    }

    fn with_roles(mut n: Node, roles: &[&str]) -> Node {
        n.roles = roles.iter().map(|r| (*r).to_string()).collect();
        n
    }

    fn with_children(mut n: Node, children: Vec<Node>) -> Node {
        for c in children {
            n.add_child(c);
        }
        n
    }

    fn counts(pairs: &[(&str, i64)]) -> BTreeMap<String, i64> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    }

    /// Mirrors the Go mapping's tree for
    /// `func hal(a int, b int) int {\n\tc := a + b\n\treturn c * 2\n}`
    /// (token-less BinaryOps and literal, whole-statement tokens on the
    /// declaration and the return, name identifiers on the signature).
    const HAL_SRC: &str = "func hal(a int, b int) int {\n\tc := a + b\n\treturn c * 2\n}";

    fn hal_tree(function_token: &str) -> Node {
        let ident = |tok: &str, s: u64, e: u64| with_roles(at("Identifier", tok, s, e), &["Name"]);
        let typ = |s: u64, e: u64| with_roles(at("Identifier", "int", s, e), &["Name", "Type"]);
        let param = |name: &str, s: u64, e: u64| {
            let mut p = with_roles(at("Parameter", "", s, e), &["Parameter"]);
            p.props.insert("name".into(), name.into());
            with_children(p, vec![ident(name, s, s + 1), typ(s + 2, e)])
        };
        let params = with_children(
            with_roles(at("Parameter", "(a int, b int)", 8, 22), &["Parameter"]),
            vec![param("a", 9, 14), param("b", 16, 21)],
        );
        let decl = with_children(
            with_roles(
                at("Variable", "c := a + b", 30, 40),
                &["Variable", "Declaration"],
            ),
            vec![
                with_children(at("List", "", 30, 31), vec![ident("c", 30, 31)]),
                with_children(
                    at("List", "", 35, 40),
                    vec![with_children(
                        with_roles(at("BinaryOp", "", 35, 40), &["Operator"]),
                        vec![ident("a", 35, 36), ident("b", 39, 40)],
                    )],
                ),
            ],
        );
        let ret = with_children(
            with_roles(at("Return", "return c * 2", 42, 54), &["Return"]),
            vec![with_children(
                at("List", "", 49, 54),
                vec![with_children(
                    with_roles(at("BinaryOp", "", 49, 54), &["Operator"]),
                    vec![
                        ident("c", 49, 50),
                        with_roles(at("Literal", "", 53, 54), &["Literal"]),
                    ],
                )],
            )],
        );
        let body = with_children(
            with_roles(at("Block", "", 27, 56), &["Body"]),
            vec![decl, ret],
        );
        let mut f = with_roles(
            at("Function", function_token, 0, 56),
            &["Function", "Declaration"],
        );
        f.props.insert("name".into(), "hal".into());
        let f = with_children(f, vec![ident("hal", 5, 8), params, typ(23, 26), body]);
        with_children(Node::with_token("File", ""), vec![f])
    }

    fn expected_hal() -> FunctionCounts {
        FunctionCounts {
            name: "hal".into(),
            operators: counts(&[(":=", 1), ("+", 1), ("*", 1), ("return", 1)]),
            operands: counts(&[("a", 1), ("b", 1), ("c", 2), ("2", 1)]),
        }
    }

    #[test]
    fn hal_with_file_source_recovers_every_operator_and_the_literal() {
        let fns = collect_functions(&hal_tree(""), Some(HAL_SRC.as_bytes()));
        assert_eq!(fns, vec![expected_hal()]);
    }

    #[test]
    fn hal_without_file_source_uses_the_function_token_window() {
        let fns = collect_functions(&hal_tree(HAL_SRC), None);
        assert_eq!(fns, vec![expected_hal()]);
    }

    #[test]
    fn without_any_source_unrecoverable_symbols_are_uncounted_never_placeholders() {
        let fns = collect_functions(&hal_tree(""), None);
        assert_eq!(fns.len(), 1);
        // `:=` and `return` come from the statements' own verbatim tokens;
        // the token-less `+` / `*` have no text anywhere and stay uncounted
        // (no "BinaryOp" placeholder).
        assert_eq!(fns[0].operators, counts(&[(":=", 1), ("return", 1)]));
        // The token-less literal `2` is lost too; the identifiers survive.
        assert_eq!(fns[0].operands, counts(&[("a", 1), ("b", 1), ("c", 2)]));
    }

    #[test]
    fn operator_property_wins_over_residue() {
        let mut tree = hal_tree("");
        let body = &mut tree.children[0].children[3];
        let plus = &mut body.children[0].children[1].children[0];
        plus.props.insert("operator".into(), "+".into());
        let fns = collect_functions(&tree, Some(HAL_SRC.as_bytes()));
        assert_eq!(fns[0].operators, expected_hal().operators);
    }

    #[test]
    fn totals_are_the_four_halstead_counts() {
        let t = expected_hal().totals();
        assert_eq!(
            t,
            HalsteadCounts {
                distinct_operators: 4,
                distinct_operands: 4,
                total_operators: 4,
                total_operands: 5,
            }
        );
    }

    #[test]
    fn empty_tree_has_no_functions() {
        assert!(collect_functions(&Node::with_token("File", ""), None).is_empty());
    }

    /// `func outer() { inner := func() { return 1 }; return 2 }`-shaped tree:
    /// the closure is its own record and its literal never reaches `outer`.
    #[test]
    fn closure_tokens_belong_to_the_innermost_function() {
        let lit = |tok: &str| with_roles(Node::with_token("Literal", tok), &["Literal"]);
        let ret = |tok: &str| {
            with_children(
                with_roles(Node::with_token("Return", ""), &["Return"]),
                vec![lit(tok)],
            )
        };
        let inner = with_children(
            with_roles(Node::with_token("Function", ""), &["Function"]),
            vec![with_children(
                with_roles(Node::with_token("Block", ""), &["Body"]),
                vec![ret("1")],
            )],
        );
        let mut outer = with_roles(Node::with_token("Function", ""), &["Function"]);
        outer.props.insert("name".into(), "outer".into());
        let outer = with_children(
            outer,
            vec![with_children(
                with_roles(Node::with_token("Block", ""), &["Body"]),
                vec![inner, ret("2")],
            )],
        );
        let fns = collect_functions(&outer, None);
        assert_eq!(fns.len(), 2, "closure first (post-order), then outer");
        assert_eq!(fns[0].name, ANONYMOUS);
        assert_eq!(fns[0].operands, counts(&[("1", 1)]));
        assert_eq!(fns[1].name, "outer");
        assert_eq!(fns[1].operands, counts(&[("2", 1)]));
        assert_eq!(fns[1].operators, counts(&[("return", 1)]));
    }

    #[test]
    fn parameters_comments_and_signature_are_never_counted() {
        let ident = |tok: &str| with_roles(Node::with_token("Identifier", tok), &["Name"]);
        let param = with_children(
            with_roles(Node::with_token("Synthetic", "(x)"), &["Parameter"]),
            vec![ident("x")],
        );
        let comment = with_roles(Node::with_token("Comment", "// uses y"), &["Comment"]);
        let body = with_children(
            with_roles(Node::with_token("Block", ""), &["Body"]),
            vec![comment, ident("y")],
        );
        let f = with_children(
            with_roles(
                Node::with_token("Function", ""),
                &["Function", "Declaration"],
            ),
            vec![ident("f"), param, ident("int"), body],
        );
        let fns = collect_functions(&f, None);
        assert_eq!(fns[0].name, "f");
        assert_eq!(fns[0].operands, counts(&[("y", 1)]));
        assert!(fns[0].operators.is_empty());
    }

    #[test]
    fn type_role_identifier_in_a_body_is_a_type_specifier_operator() {
        let decl = with_children(
            with_roles(
                Node::with_token("Variable", ""),
                &["Variable", "Declaration"],
            ),
            vec![
                with_roles(Node::with_token("Identifier", "int"), &["Type", "Name"]),
                with_roles(Node::with_token("Identifier", "r"), &["Name"]),
            ],
        );
        let f = with_children(
            with_roles(Node::with_token("Function", ""), &["Function"]),
            vec![with_children(
                with_roles(Node::with_token("Block", ""), &["Body"]),
                vec![decl],
            )],
        );
        let fns = collect_functions(&f, None);
        assert_eq!(fns[0].operators, counts(&[("int", 1)]));
        assert_eq!(fns[0].operands, counts(&[("r", 1)]));
    }

    /// C: `int f(void) { return 0; }` lowers to Function → [Identifier(int,
    /// Type), Function(declarator) → Identifier(f), Block]. The declarator is
    /// signature (no empty record), and the outer record takes its name.
    #[test]
    fn c_declarator_names_the_outer_function_without_an_extra_record() {
        let declarator = with_children(
            with_roles(
                Node::with_token("Function", ""),
                &["Function", "Declaration"],
            ),
            vec![with_roles(Node::with_token("Identifier", "f"), &["Name"])],
        );
        let body = with_children(
            with_roles(Node::with_token("Block", ""), &["Body"]),
            vec![with_children(
                with_roles(Node::with_token("Return", "return 0;"), &["Return"]),
                vec![with_roles(Node::with_token("Literal", "0"), &["Literal"])],
            )],
        );
        let f = with_children(
            with_roles(
                Node::with_token("Function", ""),
                &["Function", "Declaration"],
            ),
            vec![
                with_roles(Node::with_token("Identifier", "int"), &["Type", "Name"]),
                declarator,
                body,
            ],
        );
        let fns = collect_functions(&f, None);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "f");
        assert_eq!(fns[0].operators, counts(&[("return", 1)]));
        assert_eq!(fns[0].operands, counts(&[("0", 1)]));
    }

    #[test]
    fn quoted_literal_is_counted_once_with_its_quotes() {
        let src = b"f(\"x\")";
        let content = with_roles(at("Literal", "", 3, 4), &["Literal"]);
        let quoted = with_children(
            with_roles(at("Literal", "", 2, 5), &["Literal"]),
            vec![content],
        );
        let call = with_children(
            with_roles(at("Call", "", 0, 6), &["Call"]),
            vec![
                with_roles(at("Identifier", "f", 0, 1), &["Name"]),
                with_children(at("List", "", 1, 6), vec![quoted]),
            ],
        );
        let f = with_children(
            with_roles(at("Function", "", 0, 6), &["Function"]),
            vec![with_children(
                with_roles(at("Block", "", 0, 6), &["Body"]),
                vec![call],
            )],
        );
        let fns = collect_functions(&f, Some(src));
        assert_eq!(fns[0].operators, counts(&[("()", 1)]));
        assert_eq!(fns[0].operands, counts(&[("f", 1), ("\"x\"", 1)]));
    }

    #[test]
    fn function_without_a_body_contributes_nothing() {
        let f = with_children(
            with_roles(Node::with_token("Method", ""), &["Function", "Declaration"]),
            vec![
                Node::with_token("Synthetic", "int"),
                with_roles(Node::with_token("Identifier", "abs"), &["Name"]),
                with_roles(Node::with_token("Parameter", ""), &["Parameter"]),
            ],
        );
        let fns = collect_functions(&f, None);
        assert_eq!(fns[0].name, "abs");
        assert!(fns[0].operators.is_empty() && fns[0].operands.is_empty());
    }
}
