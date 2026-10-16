//! Function discovery and variable attribution over a UAST.
//!
//! One preorder walk with a context stack: every function node opens a
//! context, every variable use below it is attributed to the **innermost**
//! open function, and the function is emitted when its subtree is done.
//!
//! # What counts as a variable use
//!
//! A node contributes a name to the innermost function when it is
//!
//! 1. an `Identifier` with role `Variable`, `Reference` or `Name` (see
//!    [`crate::uast::is_variable_identifier`]), or
//! 2. a `Variable` / `Parameter` / `Field` declaration carrying
//!    `props["name"]` (the declaration's own token — `x := 1` — is never
//!    used),
//!
//! and it is **not**
//!
//! * a type identifier (role `Type`: `int`, `Config`, `i32`, `Self`),
//! * a direct `Identifier` child of the function node (its own name and its
//!   return-type annotation),
//! * a callee: the first child of a `Call` node when it is an identifier, or
//!   the last identifier of a member chain in callee position
//!   (`helper()`, `fmt.Println(x)` → only `fmt` and `x` count),
//! * the method receiver: the spellings in [`crate::uast::RECEIVER_KEYWORDS`]
//!   (`self`, `this`) and a Go-style receiver parameter (`(s *S)`),
//! * a literal (literals are not identifiers).
//!
//! Names are resolved per [`crate::uast::identifier_name`]; identifiers with
//! no recoverable name are skipped. Nested function nodes open their own
//! context (their variables are theirs alone); lambdas do not, so a closure's
//! variables belong to the enclosing function.
//!
//! # Function names
//!
//! A function is named by `props["name"]` or its name child. Methods are
//! qualified `Type.name` when the UAST gives a receiver type (Go) or an
//! enclosing named `Class`/`Struct`/`Interface`/`Enum` (Python, Java,
//! TypeScript, …). A function whose name cannot be resolved is reported as
//! [`ANONYMOUS_FUNCTION_NAME`].

use std::collections::BTreeSet;

use cf_uast_node::Node;

use crate::analyzer::Function;
use crate::uast::{self, Anchor, RECEIVER_KEYWORDS};

/// Name reported for a function node without a resolvable name.
pub const ANONYMOUS_FUNCTION_NAME: &str = "<anonymous>";

/// Stack red zone for the growable-stack walk (64 KiB).
const STACK_RED_ZONE: usize = 64 * 1024;
/// Stack growth chunk for the growable-stack walk (4 MiB).
const STACK_CHUNK: usize = 4 * 1024 * 1024;

/// Why a node, or one of its children, must not count as a variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Exclusion {
    /// The node contributes normally.
    None,
    /// The node itself is never a variable (own name, callee, type).
    Node,
    /// The node is a member chain in callee position: its last direct
    /// identifier child is the method name and never a variable.
    LastIdentifier,
}

/// One open function on the context stack.
struct Ctx {
    function: Function,
    /// Names that are never variables of this function (the receiver).
    excluded: BTreeSet<String>,
}

struct Walker {
    stack: Vec<Ctx>,
    /// Names of the enclosing class-like scopes, innermost last.
    scopes: Vec<String>,
    out: Vec<Function>,
    /// Function nodes deeper than this (root at depth 0) are not discovered.
    max_depth: Option<usize>,
}

/// Collects every function below `root` (preorder, emitted on exit) with its
/// distinct variables. With `max_depth = Some(cap)` only function nodes at
/// depth `<= cap` open a context.
#[must_use]
pub fn collect_functions(root: &Node, max_depth: Option<usize>) -> Vec<Function> {
    let mut walker = Walker {
        stack: Vec::new(),
        scopes: Vec::new(),
        out: Vec::new(),
        max_depth,
    };
    walker.visit(root, 0, None, Exclusion::None);
    walker.out
}

impl Walker {
    fn visit(&mut self, n: &Node, depth: usize, anchor: Option<Anchor<'_>>, excl: Exclusion) {
        // Generated sources nest past 10k levels; grow the stack on demand
        // instead of overflowing a fixed OS stack.
        stacker::maybe_grow(STACK_RED_ZONE, STACK_CHUNK, || {
            self.visit_inner(n, depth, anchor, excl);
        });
    }

    fn visit_inner(&mut self, n: &Node, depth: usize, anchor: Option<Anchor<'_>>, excl: Exclusion) {
        let anchor = Anchor::of(n).or(anchor);
        let entered = self.enter_function(n, depth, anchor);
        let scoped = self.enter_scope(n, anchor);
        if excl != Exclusion::Node {
            self.contribute(n, anchor);
        }

        let last_identifier = (excl == Exclusion::LastIdentifier)
            .then(|| n.children.iter().rposition(uast::is_identifier))
            .flatten();
        let is_call = uast::is_call(n);
        for (i, child) in n.children.iter().enumerate() {
            let child_excl = child_exclusion(entered, is_call, last_identifier, i, child);
            self.visit(child, depth + 1, anchor, child_excl);
        }

        if scoped {
            self.scopes.pop();
        }
        if entered {
            if let Some(ctx) = self.stack.pop() {
                self.out.push(ctx.function);
            }
        }
    }

    /// Opens a context when `n` is a discoverable function; returns whether
    /// it did.
    fn enter_function(&mut self, n: &Node, depth: usize, anchor: Option<Anchor<'_>>) -> bool {
        if !uast::is_function(n) || self.max_depth.is_some_and(|cap| depth > cap) {
            return false;
        }
        let receiver = if uast::is_method(n) {
            receiver_parameter(n)
        } else {
            None
        };
        let receiver_name = receiver
            .and_then(|r| uast::find_descendant(r, &uast::is_variable_identifier))
            .and_then(|id| uast::identifier_name(id, anchor));
        let receiver_type = receiver
            .and_then(|r| uast::find_descendant(r, &uast::is_type_identifier))
            .and_then(|id| uast::identifier_name(id, anchor));

        let base = uast::own_name(n, anchor).unwrap_or_else(|| ANONYMOUS_FUNCTION_NAME.to_string());
        let name = match receiver_type.or_else(|| self.scopes.last().cloned()) {
            Some(qualifier) => format!("{qualifier}.{base}"),
            None => base,
        };
        let (start_line, end_line) = uast::line_span(n);
        self.stack.push(Ctx {
            function: Function {
                name,
                variables: BTreeSet::new(),
                start_line,
                end_line,
            },
            excluded: receiver_name.into_iter().collect(),
        });
        true
    }

    /// Pushes the name of a class-like scope; returns whether it did.
    fn enter_scope(&mut self, n: &Node, anchor: Option<Anchor<'_>>) -> bool {
        if !uast::is_scope(n) {
            return false;
        }
        match uast::own_name(n, anchor) {
            Some(name) => {
                self.scopes.push(name);
                true
            }
            None => false,
        }
    }

    /// Attributes `n`'s variable name, if it has one, to the innermost function.
    fn contribute(&mut self, n: &Node, anchor: Option<Anchor<'_>>) {
        let Some(ctx) = self.stack.last_mut() else {
            return;
        };
        let name = if uast::is_variable_identifier(n) {
            uast::identifier_name(n, anchor)
        } else if uast::is_named_declaration(n) {
            uast::declared_name(n).map(str::to_owned)
        } else {
            None
        };
        let Some(name) = name else {
            return;
        };
        if RECEIVER_KEYWORDS.contains(&name.as_str()) || ctx.excluded.contains(&name) {
            return;
        }
        ctx.function.variables.insert(name);
    }
}

/// The exclusion that applies to the `i`-th child of a node.
fn child_exclusion(
    parent_is_function: bool,
    parent_is_call: bool,
    last_identifier: Option<usize>,
    i: usize,
    child: &Node,
) -> Exclusion {
    if (parent_is_function && uast::is_identifier(child)) || last_identifier == Some(i) {
        return Exclusion::Node;
    }
    if parent_is_call && i == 0 {
        return if uast::is_identifier(child) {
            Exclusion::Node
        } else {
            Exclusion::LastIdentifier
        };
    }
    Exclusion::None
}

/// A Go-style receiver: a `Parameter` child of a method that precedes the
/// method's own name child.
fn receiver_parameter(method: &Node) -> Option<&Node> {
    let name_index = method
        .children
        .iter()
        .position(|c| uast::name_child(method).is_some_and(|nc| std::ptr::eq(nc, c)))
        .unwrap_or(method.children.len());
    method.children[..name_index]
        .iter()
        .find(|c| uast::is_parameter(c))
}
