//! Source-text windows used to recover operator and operand text by span.
//!
//! Many mappings leave `BinaryOp` / `Literal` / `Identifier` nodes without a
//! token; their text is still addressable through the node's byte span. A
//! [`Window`] maps a span onto a byte buffer that starts at a known offset:
//! the whole file (base 0) when the caller has it, or a node's own token when
//! that token is a verbatim slice of the file (its length equals its span).

use cf_uast_node::Node;

/// A byte buffer positioned at `base` in the original file.
#[derive(Debug, Clone, Copy)]
pub struct Window<'a> {
    base: usize,
    bytes: &'a [u8],
}

impl<'a> Window<'a> {
    /// The whole file, starting at byte 0.
    #[must_use]
    pub fn file(bytes: &'a [u8]) -> Self {
        Self { base: 0, bytes }
    }

    /// A window over `node.token` when that token is a verbatim slice of the
    /// file (`token.len()` equals the node's span length), else `None`.
    #[must_use]
    pub fn of_token(node: &'a Node) -> Option<Self> {
        let (start, end) = span(node)?;
        if node.token.is_empty() || node.token.len() != end - start {
            return None;
        }
        Some(Self {
            base: start,
            bytes: node.token.as_bytes(),
        })
    }

    /// The bytes of `[start, end)` (absolute file offsets) when they fall
    /// inside this window.
    #[must_use]
    pub fn slice(&self, start: usize, end: usize) -> Option<&'a [u8]> {
        let lo = start.checked_sub(self.base)?;
        let hi = end.checked_sub(self.base)?;
        self.bytes.get(lo..hi)
    }
}

/// The `[start, end)` byte span of a node, when it carries positions.
#[must_use]
pub fn span(node: &Node) -> Option<(usize, usize)> {
    let pos = node.pos?;
    let start = usize::try_from(pos.start_offset).ok()?;
    let end = usize::try_from(pos.end_offset).ok()?;
    (end >= start).then_some((start, end))
}

/// The residue of `node` inside `window`: its span text with every child's
/// span blanked. `None` when the node or any child lacks a span, or the span
/// is outside the window.
#[must_use]
pub fn residue(node: &Node, window: &Window<'_>) -> Option<String> {
    let (start, end) = span(node)?;
    let mut text = window.slice(start, end)?.to_vec();
    for child in &node.children {
        let (cs, ce) = span(child)?;
        let lo = cs.clamp(start, end) - start;
        let hi = ce.clamp(start, end) - start;
        text[lo..hi].fill(b' ');
    }
    Some(String::from_utf8_lossy(&text).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_uast_node::Positions;

    fn at(node_type: &str, token: &str, start: u64, end: u64) -> Node {
        let mut n = Node::with_token(node_type, token);
        n.pos = Some(Positions {
            start_offset: start,
            end_offset: end,
            ..Positions::default()
        });
        n
    }

    #[test]
    fn file_window_slices_absolute_offsets() {
        let w = Window::file(b"c := a + b");
        assert_eq!(w.slice(5, 10), Some(&b"a + b"[..]));
        assert_eq!(w.slice(5, 11), None);
    }

    #[test]
    fn token_window_requires_a_verbatim_token() {
        let full = at("Function", "func f() {}", 10, 21);
        let w = Window::of_token(&full).expect("verbatim token");
        assert_eq!(w.slice(16, 18), Some(&b"()"[..]));
        assert_eq!(w.slice(0, 2), None, "before the window");

        let trimmed = at("Function", "func", 10, 21);
        assert!(Window::of_token(&trimmed).is_none());
        let empty = at("Function", "", 10, 21);
        assert!(Window::of_token(&empty).is_none());
    }

    #[test]
    fn residue_blanks_children() {
        let src = b"c := a + b";
        let mut stmt = at("Variable", "", 0, 10);
        stmt.add_child(at("Identifier", "c", 0, 1));
        let mut plus = at("BinaryOp", "", 5, 10);
        plus.add_child(at("Identifier", "a", 5, 6));
        plus.add_child(at("Identifier", "b", 9, 10));
        stmt.add_child(plus.clone());
        let w = Window::file(src);
        assert_eq!(residue(&stmt, &w).as_deref(), Some("  :=      "));
        assert_eq!(residue(&plus, &w).as_deref(), Some("  +  "));
    }

    #[test]
    fn residue_is_none_without_positions() {
        let mut stmt = at("Return", "return x", 0, 8);
        stmt.add_child(Node::with_token("Identifier", "x"));
        assert!(residue(&stmt, &Window::file(b"return x")).is_none());
        assert!(residue(&Node::with_token("Return", "return"), &Window::file(b"")).is_none());
    }

    #[test]
    fn residue_clamps_children_outside_the_parent_span() {
        let mut outer = at("Synthetic", "", 2, 5);
        outer.add_child(at("Identifier", "", 0, 9));
        assert_eq!(
            residue(&outer, &Window::file(b"0123456789")).as_deref(),
            Some("   ")
        );
    }
}
