//! The single-file [`Visitor`]: finds every function-like node in a UAST
//! (functions, methods and closures), names it, and builds its clone
//! candidate entry.
//!
//! Named functions keep their own name (`Foo.DoWork` for methods with a
//! receiver). Closures (UAST `Lambda`) are named by the nearest enclosing
//! candidate and their ordinal inside it — `Handle.func#2` — or `func#N` at
//! file level. Candidates nested inside another candidate remember it as
//! their parent so the engine never pairs a closure with the function it is
//! written in.

use cf_uast_node::Node;

use crate::engine::{build_entry, FuncEntry};
use crate::shingler::Shingler;
use crate::NUM_HASHES;

/// Function node type.
const UAST_FUNCTION: &str = "Function";
/// Method node type.
const UAST_METHOD: &str = "Method";
/// Closure / anonymous function node type.
const UAST_LAMBDA: &str = "Lambda";
/// Function role.
const ROLE_FUNCTION: &str = "Function";
/// Declaration role.
const ROLE_DECLARATION: &str = "Declaration";
/// Parameter role (receiver detection).
const ROLE_PARAMETER: &str = "Parameter";

/// Separator between a closure's enclosing scope and its ordinal label.
const SCOPE_SEPARATOR: &str = ".";
/// Label prefix of a closure's ordinal: `func#1`, `func#2`, …
const LAMBDA_LABEL: &str = "func#";

/// What one file contributes to clone detection.
#[derive(Debug, Clone, Default)]
pub struct FileScan {
    /// Every function-like node seen (closures included), size-gated or not.
    pub function_count: usize,
    /// The clone candidates (functions large enough to shingle), in
    /// pre-order.
    pub entries: Vec<FuncEntry>,
}

/// Scans a file's UAST for clone candidates.
#[derive(Debug, Clone)]
pub struct Visitor {
    shingler: Shingler,
    num_hashes: usize,
}

impl Default for Visitor {
    fn default() -> Self {
        Self::new()
    }
}

/// An open function-like scope during the traversal.
struct Scope {
    name: String,
    entry: Option<usize>,
    lambdas: usize,
}

/// One pending node of the explicit traversal stack.
struct Frame<'a> {
    node: &'a Node,
    next_child: usize,
    opened_scope: bool,
}

impl Visitor {
    /// Creates a visitor with the default shingle size and signature length.
    #[must_use]
    pub fn new() -> Self {
        Self {
            shingler: Shingler::default(),
            num_hashes: NUM_HASHES,
        }
    }

    /// Walks `root` in pre-order (explicit stack, so deep trees are safe) and
    /// returns the file's candidates.
    #[must_use]
    pub fn scan(&self, root: &Node) -> FileScan {
        let mut scan = FileScan::default();
        let mut scopes: Vec<Scope> = Vec::new();
        let mut file_lambdas = 0;

        let opened = self.enter(root, &mut scan, &mut scopes, &mut file_lambdas);
        let mut stack = vec![Frame {
            node: root,
            next_child: 0,
            opened_scope: opened,
        }];

        while let Some(frame) = stack.last_mut() {
            if let Some(child) = frame.node.children.get(frame.next_child) {
                frame.next_child += 1;
                let opened = self.enter(child, &mut scan, &mut scopes, &mut file_lambdas);
                stack.push(Frame {
                    node: child,
                    next_child: 0,
                    opened_scope: opened,
                });
                continue;
            }
            if frame.opened_scope {
                scopes.pop();
            }
            stack.pop();
        }
        scan
    }

    /// Handles entering `n`: records a function-like node, builds its entry
    /// and opens its scope. Returns whether a scope was opened.
    fn enter(
        &self,
        n: &Node,
        scan: &mut FileScan,
        scopes: &mut Vec<Scope>,
        file_lambdas: &mut usize,
    ) -> bool {
        if !is_candidate_node(n) {
            return false;
        }
        scan.function_count += 1;

        let name = if is_lambda(n) {
            lambda_name(scopes, file_lambdas)
        } else {
            extract_func_name(n)
        };
        let parent = scopes.iter().rev().find_map(|s| s.entry);
        let entry = build_entry(name.clone(), n, &self.shingler, self.num_hashes).map(|mut e| {
            e.parent = parent;
            scan.entries.push(e);
            scan.entries.len() - 1
        });
        scopes.push(Scope {
            name,
            entry,
            lambdas: 0,
        });
        true
    }
}

/// `true` for functions, methods and closures.
fn is_candidate_node(n: &Node) -> bool {
    n.has_any_type(&[UAST_FUNCTION, UAST_METHOD, UAST_LAMBDA])
        || n.has_all_roles(&[ROLE_FUNCTION, ROLE_DECLARATION])
}

fn is_lambda(n: &Node) -> bool {
    n.node_type == UAST_LAMBDA
}

/// Names a closure by its enclosing scope and ordinal: `<scope>.func#N`, or
/// `func#N` at file level.
fn lambda_name(scopes: &mut [Scope], file_lambdas: &mut usize) -> String {
    if let Some(scope) = scopes.last_mut() {
        scope.lambdas += 1;
        format!(
            "{}{SCOPE_SEPARATOR}{LAMBDA_LABEL}{}",
            scope.name, scope.lambdas
        )
    } else {
        *file_lambdas += 1;
        format!("{LAMBDA_LABEL}{file_lambdas}")
    }
}

/// Extracts a unique function name from a node: the entity name (see
/// [`extract_entity_name`]), falling back to the node token, then the type;
/// methods are qualified with the receiver type (`Foo.DoWork`).
fn extract_func_name(fn_node: &Node) -> String {
    let mut name = extract_entity_name(fn_node).unwrap_or_default();
    if name.is_empty() {
        name = if fn_node.token.is_empty() {
            fn_node.node_type.clone()
        } else {
            fn_node.token.clone()
        };
    }

    if fn_node.node_type == UAST_METHOD {
        let recv = extract_receiver_type(fn_node);
        if !recv.is_empty() {
            return format!("{recv}{SCOPE_SEPARATOR}{name}");
        }
    }
    name
}

/// Extracts the entity name from a node: the `name` prop, else the node's
/// own token, else the first child's token or `name` prop.
fn extract_entity_name(n: &Node) -> Option<String> {
    if let Some(name) = n.props.get("name") {
        return Some(name.clone());
    }
    if !n.token.is_empty() {
        return Some(n.token.clone());
    }
    let child = n.children.first()?;
    if !child.token.is_empty() {
        return Some(child.token.clone());
    }
    child.props.get("name").cloned()
}

/// Extracts the receiver type name from a method node: the first child with
/// the `Parameter` role whose token looks like `(v *T)` / `(v T)`; `T` is the
/// last whitespace-separated word after stripping the parens and a leading
/// `*`.
fn extract_receiver_type(fn_node: &Node) -> String {
    const MIN_RECEIVER_PARTS: usize = 2;

    for child in &fn_node.children {
        if !child.has_any_role(&[ROLE_PARAMETER]) {
            continue;
        }
        let tok = child.token.trim();
        let tok = tok.strip_prefix('(').unwrap_or(tok);
        let tok = tok.strip_suffix(')').unwrap_or(tok).trim();
        let parts: Vec<&str> = tok.split_whitespace().collect();
        if parts.len() < MIN_RECEIVER_PARTS {
            continue;
        }
        let last = parts[parts.len() - 1];
        let type_name = last.strip_prefix('*').unwrap_or(last);
        if !type_name.is_empty() {
            return type_name.to_string();
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_uast::Parser;

    const PROCESS_ITEMS_BODY: &str = r#"
	total := 0
	for i, v := range items {
		if v%2 == 0 {
			total += v * i
		} else {
			total -= v
		}
	}
	fmt.Println("total", total)
	return total
"#;

    fn go(src: &str) -> Node {
        Parser::new().parse("a.go", src.as_bytes()).expect("parses")
    }

    fn names(scan: &FileScan) -> Vec<&str> {
        scan.entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn named_functions_are_candidates() {
        let src = format!(
            "package main\n\nimport \"fmt\"\n\nfunc ProcessItems(items []int) int {{{PROCESS_ITEMS_BODY}}}\n"
        );
        let scan = Visitor::new().scan(&go(&src));
        assert_eq!(scan.function_count, 1);
        assert_eq!(names(&scan), vec!["ProcessItems"]);
    }

    #[test]
    fn methods_are_qualified_with_the_receiver_type() {
        let src = format!(
            "package main\n\nimport \"fmt\"\n\ntype A struct{{ items []int }}\n\nfunc (a *A) Process() int {{\n\titems := a.items{PROCESS_ITEMS_BODY}}}\n"
        );
        let scan = Visitor::new().scan(&go(&src));
        assert_eq!(names(&scan), vec!["A.Process"]);
    }

    #[test]
    fn small_functions_are_counted_but_not_candidates() {
        let scan = Visitor::new().scan(&go(
            "package main\n\nfunc Add(a, b int) int {\n\treturn a + b\n}\n",
        ));
        assert_eq!(scan.function_count, 1);
        assert!(scan.entries.is_empty());
    }

    #[test]
    fn file_level_closures_are_named_by_ordinal() {
        let src = format!(
            "package main\n\nimport \"fmt\"\n\nvar first = func(items []int) int {{{PROCESS_ITEMS_BODY}}}\n\nvar second = func(items []int) int {{{PROCESS_ITEMS_BODY}}}\n"
        );
        let scan = Visitor::new().scan(&go(&src));
        assert_eq!(scan.function_count, 2);
        assert_eq!(names(&scan), vec!["func#1", "func#2"]);
        assert!(scan.entries.iter().all(|e| e.parent.is_none()));
    }

    #[test]
    fn nested_closures_are_named_by_scope_and_remember_their_parent() {
        let src = format!(
            "package main\n\nimport \"fmt\"\n\nfunc Outer(items []int) int {{\n\tf := func() int {{{PROCESS_ITEMS_BODY}}}\n\treturn f()\n}}\n"
        );
        let scan = Visitor::new().scan(&go(&src));
        assert_eq!(scan.function_count, 2);
        assert_eq!(names(&scan), vec!["Outer", "Outer.func#1"]);
        assert_eq!(scan.entries[1].parent, Some(0));
    }

    #[test]
    fn a_closure_inside_a_small_function_has_no_parent_entry() {
        // The enclosing function is not a candidate (too small once the
        // closure is... no: the closure's nodes count for the parent too), so
        // nesting is tracked only among entries that exist.
        let src = format!(
            "package main\n\nimport \"fmt\"\n\nfunc Tiny() {{\n\tgo func(items []int) {{{PROCESS_ITEMS_BODY}}}(nil)\n}}\n"
        );
        let scan = Visitor::new().scan(&go(&src));
        assert_eq!(scan.function_count, 2);
        let parent_of_closure = scan
            .entries
            .iter()
            .find(|e| e.name == "Tiny.func#1")
            .expect("closure entry")
            .parent;
        let tiny_index = scan.entries.iter().position(|e| e.name == "Tiny");
        assert_eq!(parent_of_closure, tiny_index);
    }
}
