//! UAST node inspection for the cohesion analyzer.
//!
//! Everything the analyzer needs to know about a [`Node`] lives here: which
//! nodes are functions, which are identifiers, how an identifier's name is
//! resolved, and the small structural predicates (type-role identifiers,
//! declarations, calls, class-like scopes) the variable rules are written in.
//!
//! # Name resolution
//!
//! Mappings differ in where an identifier's text ends up. Most grammars put it
//! in the node's `token`; some carry it only as `props["name"]`; and a few
//! (the Rust mapping in its current form) leave identifier nodes token-less
//! while an ancestor — the function, a field access — carries the full source
//! text of its span. The resolver tries, in order: `props["name"]`, the token,
//! and finally the identifier's source text sliced out of the nearest enclosing
//! [`Anchor`] by byte offsets. An anchor is only taken from a node whose token
//! is exactly its source span, so slicing can never produce text from
//! somewhere else.

use cf_uast_node::Node;

/// UAST node *type* names referenced by cohesion.
pub mod ty {
    /// `Function` type.
    pub const FUNCTION: &str = "Function";
    /// `Method` type.
    pub const METHOD: &str = "Method";
    /// `Variable` type.
    pub const VARIABLE: &str = "Variable";
    /// `Parameter` type.
    pub const PARAMETER: &str = "Parameter";
    /// `Field` type.
    pub const FIELD: &str = "Field";
    /// `Identifier` type.
    pub const IDENTIFIER: &str = "Identifier";
    /// `Call` type.
    pub const CALL: &str = "Call";
    /// `Class` type.
    pub const CLASS: &str = "Class";
    /// `Struct` type.
    pub const STRUCT: &str = "Struct";
    /// `Interface` type.
    pub const INTERFACE: &str = "Interface";
    /// `Enum` type.
    pub const ENUM: &str = "Enum";
}

/// UAST *role* names referenced by cohesion.
pub mod role {
    /// `Function` role.
    pub const FUNCTION: &str = "Function";
    /// `Declaration` role.
    pub const DECLARATION: &str = "Declaration";
    /// `Variable` role.
    pub const VARIABLE: &str = "Variable";
    /// `Name` role.
    pub const NAME: &str = "Name";
    /// `Reference` role.
    pub const REFERENCE: &str = "Reference";
    /// `Type` role.
    pub const TYPE: &str = "Type";
    /// `Call` role.
    pub const CALL: &str = "Call";
}

/// The `props` key carrying a node's declared name.
pub const NAME_PROP: &str = "name";

/// Identifier spellings of the implicit method receiver. They are shared by
/// every method by construction, so counting them would inflate cohesion
/// uniformly; they are never variables.
pub const RECEIVER_KEYWORDS: [&str; 2] = ["self", "this"];

/// Source text of a node whose token is exactly its span, used to recover the
/// text of token-less identifiers below it by byte offsets.
#[derive(Debug, Clone, Copy)]
pub struct Anchor<'a> {
    text: &'a str,
    start: u64,
}

impl<'a> Anchor<'a> {
    /// An anchor over `n`, when `n` has a position and its token is exactly
    /// as long as its byte span (i.e. the token *is* the source text).
    #[must_use]
    pub fn of(n: &'a Node) -> Option<Self> {
        let pos = n.pos.as_ref()?;
        let span = pos.end_offset.checked_sub(pos.start_offset)?;
        if n.token.is_empty() || n.token.len() as u64 != span {
            return None;
        }
        Some(Anchor {
            text: &n.token,
            start: pos.start_offset,
        })
    }

    /// The source text of `n` inside this anchor, when `n` lies within it.
    #[must_use]
    pub fn slice(&self, n: &Node) -> Option<&'a str> {
        let pos = n.pos.as_ref()?;
        let lo = usize::try_from(pos.start_offset.checked_sub(self.start)?).ok()?;
        let hi = usize::try_from(pos.end_offset.checked_sub(self.start)?).ok()?;
        self.text.get(lo..hi).filter(|s| !s.is_empty())
    }
}

/// Resolves the name an identifier node stands for: `props["name"]`, else
/// its token, else its source text under `anchor`. `None` when the node
/// carries no recoverable name.
#[must_use]
pub fn identifier_name(n: &Node, anchor: Option<Anchor<'_>>) -> Option<String> {
    if let Some(name) = n.props.get(NAME_PROP) {
        return Some(name.clone());
    }
    if !n.token.is_empty() {
        return Some(n.token.clone());
    }
    anchor.and_then(|a| a.slice(n)).map(str::to_owned)
}

/// The declared name a node carries in its props, if any.
#[must_use]
pub fn declared_name(n: &Node) -> Option<&str> {
    n.props.get(NAME_PROP).map(String::as_str)
}

/// True for function nodes: type `Function`/`Method`, or the role pair
/// `{Function, Declaration}`.
#[must_use]
pub fn is_function(n: &Node) -> bool {
    n.has_any_type(&[ty::FUNCTION, ty::METHOD])
        || n.has_all_roles(&[role::FUNCTION, role::DECLARATION])
}

/// True for `Method` nodes (functions that may carry a receiver).
#[must_use]
pub fn is_method(n: &Node) -> bool {
    n.has_any_type(&[ty::METHOD])
}

/// True for `Identifier` nodes.
#[must_use]
pub fn is_identifier(n: &Node) -> bool {
    n.has_any_type(&[ty::IDENTIFIER])
}

/// True for identifiers that name a type (role `Type`): `int`, `Config`,
/// `i32`, `Self`.
#[must_use]
pub fn is_type_identifier(n: &Node) -> bool {
    is_identifier(n) && n.has_any_role(&[role::TYPE])
}

/// True for identifiers in a variable position: role `Variable`, `Reference`
/// or `Name`, excluding type identifiers.
#[must_use]
pub fn is_variable_identifier(n: &Node) -> bool {
    is_identifier(n)
        && !n.has_any_role(&[role::TYPE])
        && n.has_any_role(&[role::VARIABLE, role::REFERENCE, role::NAME])
}

/// True for `Parameter` nodes.
#[must_use]
pub fn is_parameter(n: &Node) -> bool {
    n.has_any_type(&[ty::PARAMETER])
}

/// True for declaration nodes whose `props["name"]` names a variable:
/// `Variable`, `Parameter`, `Field`.
#[must_use]
pub fn is_named_declaration(n: &Node) -> bool {
    n.has_any_type(&[ty::VARIABLE, ty::PARAMETER, ty::FIELD])
}

/// True for call nodes (type `Call` or role `Call`): the first child is the
/// callee expression.
#[must_use]
pub fn is_call(n: &Node) -> bool {
    n.has_any_type(&[ty::CALL]) || n.has_any_role(&[role::CALL])
}

/// True for class-like scopes whose name qualifies the functions inside.
#[must_use]
pub fn is_scope(n: &Node) -> bool {
    n.has_any_type(&[ty::CLASS, ty::STRUCT, ty::INTERFACE, ty::ENUM])
}

/// The direct child carrying a node's own name: the first `Identifier` child
/// with role `Name` that is not a type identifier.
#[must_use]
pub fn name_child(n: &Node) -> Option<&Node> {
    n.children
        .iter()
        .find(|c| is_identifier(c) && c.has_any_role(&[role::NAME]) && !is_type_identifier(c))
}

/// A node's own name: `props["name"]`, else its name child resolved under
/// `anchor`.
#[must_use]
pub fn own_name(n: &Node, anchor: Option<Anchor<'_>>) -> Option<String> {
    declared_name(n)
        .map(str::to_owned)
        .or_else(|| name_child(n).and_then(|c| identifier_name(c, anchor)))
}

/// The first descendant (preorder, `n` excluded) matching `pred`.
#[must_use]
pub fn find_descendant<'a>(n: &'a Node, pred: &dyn Fn(&Node) -> bool) -> Option<&'a Node> {
    for child in &n.children {
        if pred(child) {
            return Some(child);
        }
        if let Some(found) = find_descendant(child, pred) {
            return Some(found);
        }
    }
    None
}

/// The 1-based `(start_line, end_line)` of a node; `(0, 0)` without position.
#[must_use]
pub fn line_span(n: &Node) -> (u64, u64) {
    n.pos
        .as_ref()
        .map_or((0, 0), |p| (p.start_line, p.end_line))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_uast_node::{Builder, Positions};

    fn positioned(node_type: &str, token: &str, start: u64, end: u64) -> Node {
        let mut n = Builder::new()
            .with_type(node_type)
            .with_token(token)
            .with_roles(vec![role::NAME.to_string()])
            .build();
        n.pos = Some(Positions {
            start_offset: start,
            end_offset: end,
            ..Positions::default()
        });
        n
    }

    #[test]
    fn anchor_requires_token_equal_to_span() {
        let exact = positioned("Field", "self.a", 10, 16);
        assert!(Anchor::of(&exact).is_some());
        let shorter = positioned("Variable", "total", 10, 25);
        assert!(
            Anchor::of(&shorter).is_none(),
            "token shorter than span must not anchor"
        );
        let no_pos = Builder::new()
            .with_type("Field")
            .with_token("self.a")
            .build();
        assert!(Anchor::of(&no_pos).is_none());
    }

    #[test]
    fn token_less_identifier_resolves_through_the_anchor() {
        let field = positioned("Field", "self.a", 10, 16);
        let anchor = Anchor::of(&field);
        let ident = positioned(ty::IDENTIFIER, "", 15, 16);
        assert_eq!(identifier_name(&ident, anchor), Some("a".to_string()));
        let outside = positioned(ty::IDENTIFIER, "", 2, 4);
        assert_eq!(identifier_name(&outside, anchor), None);
    }

    #[test]
    fn props_name_wins_over_token_and_anchor() {
        let mut ident = positioned(ty::IDENTIFIER, "tok", 0, 3);
        ident.props.insert(NAME_PROP.into(), "declared".into());
        assert_eq!(identifier_name(&ident, None), Some("declared".to_string()));
    }

    #[test]
    fn type_role_identifiers_are_not_variables() {
        let t = Builder::new()
            .with_type(ty::IDENTIFIER)
            .with_token("int")
            .with_roles(vec![role::NAME.into(), role::TYPE.into()])
            .build();
        assert!(is_type_identifier(&t));
        assert!(!is_variable_identifier(&t));
        let v = Builder::new()
            .with_type(ty::IDENTIFIER)
            .with_token("x")
            .with_roles(vec![role::REFERENCE.into()])
            .build();
        assert!(is_variable_identifier(&v));
    }
}
