//! Placement rules: where a comment block sits relative to the declarations
//! of its file. Pure line arithmetic, independent of the UAST.
//!
//! A block is tested in this order:
//! 1. **docstring** — it is a leading documentation comment (the first
//!    statement of a declaration body, a Python docstring) and lies inside a
//!    declaration's span: it documents the innermost such declaration;
//! 2. **above** — it starts before a declaration's anchor line and its text
//!    reaches the line directly above that anchor (no blank line between);
//!    the anchor is the declaration's first line, extended upward over
//!    attribute / decorator lines that end directly above it;
//! 3. **inline** — it lies inside a declaration's span (the innermost one);
//! 4. **unassociated** — anything else.

/// A declaration's line geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetSpan {
    /// First line of the declaration.
    pub start: i64,
    /// Last line of the declaration.
    pub end: i64,
    /// The line a documenting comment must end directly above.
    pub anchor: i64,
}

/// A comment block's line geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockSpan {
    /// First line of the block.
    pub start: i64,
    /// Last line holding comment text.
    pub text_end: i64,
    /// Whether the block is a leading documentation comment of a declaration
    /// body (a docstring), as opposed to a comment found anywhere else.
    pub leading_doc: bool,
}

/// The outcome of placing a block; indices refer to the targets slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    /// The block documents the declaration at this index.
    Above(usize),
    /// The block is the docstring of the declaration at this index.
    Docstring(usize),
    /// The block is a body comment of the declaration at this index.
    Inline(usize),
    /// The block belongs to no declaration.
    Unassociated,
}

/// Places `block` against `targets`.
#[must_use]
pub fn resolve(block: BlockSpan, targets: &[TargetSpan]) -> Resolved {
    if block.leading_doc {
        if let Some(i) = enclosing_target(block, targets) {
            return Resolved::Docstring(i);
        }
    }
    if let Some(i) = documented_target(block, targets) {
        return Resolved::Above(i);
    }
    if let Some(i) = enclosing_target(block, targets) {
        return Resolved::Inline(i);
    }
    Resolved::Unassociated
}

/// The declaration whose anchor the block ends directly above (the earliest
/// such anchor when several qualify).
fn documented_target(block: BlockSpan, targets: &[TargetSpan]) -> Option<usize> {
    targets
        .iter()
        .enumerate()
        .filter(|(_, t)| block.start < t.anchor && block.text_end >= t.anchor - 1)
        .min_by_key(|(_, t)| t.anchor)
        .map(|(i, _)| i)
}

/// The innermost declaration whose span contains the block.
fn enclosing_target(block: BlockSpan, targets: &[TargetSpan]) -> Option<usize> {
    targets
        .iter()
        .enumerate()
        .filter(|(_, t)| t.start <= block.start && block.text_end <= t.end)
        .max_by_key(|(i, t)| (t.start, *i))
        .map(|(i, _)| i)
}

/// The anchor line of a declaration starting at `start`: `start`, extended
/// upward over attribute spans `(start, end)` that end directly above the
/// current anchor.
#[must_use]
pub fn anchor_line(start: i64, attributes: &[(i64, i64)]) -> i64 {
    let mut anchor = start;
    loop {
        let bridged = attributes
            .iter()
            .filter(|(_, end)| *end == anchor - 1)
            .map(|(s, _)| *s)
            .min();
        match bridged {
            Some(s) if s < anchor => anchor = s,
            _ => return anchor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(start: i64, end: i64) -> TargetSpan {
        TargetSpan {
            start,
            end,
            anchor: start,
        }
    }

    fn block(start: i64, text_end: i64) -> BlockSpan {
        BlockSpan {
            start,
            text_end,
            leading_doc: false,
        }
    }

    fn docstring(start: i64, text_end: i64) -> BlockSpan {
        BlockSpan {
            start,
            text_end,
            leading_doc: true,
        }
    }

    /// A docstring documents the declaration whose body holds it, even when
    /// it also ends directly above a nested declaration.
    #[test]
    fn docstring_documents_its_enclosing_declaration_not_the_next_one() {
        let targets = [target(1, 6), target(3, 6)];
        assert_eq!(resolve(docstring(2, 2), &targets), Resolved::Docstring(0));
    }

    /// A docstring outside every declaration (a module docstring) is
    /// unassociated.
    #[test]
    fn module_docstring_is_unassociated() {
        assert_eq!(
            resolve(docstring(1, 1), &[target(3, 5)]),
            Resolved::Unassociated
        );
    }

    #[test]
    fn directly_above_documents() {
        assert_eq!(resolve(block(1, 1), &[target(2, 4)]), Resolved::Above(0));
    }

    #[test]
    fn multi_line_block_directly_above_documents() {
        assert_eq!(resolve(block(1, 3), &[target(4, 6)]), Resolved::Above(0));
    }

    #[test]
    fn one_blank_line_breaks_documentation() {
        assert_eq!(
            resolve(block(1, 1), &[target(3, 5)]),
            Resolved::Unassociated
        );
    }

    #[test]
    fn inside_a_span_is_inline_for_the_innermost() {
        let targets = [target(1, 20), target(5, 10)];
        assert_eq!(resolve(block(7, 7), &targets), Resolved::Inline(1));
        assert_eq!(resolve(block(15, 15), &targets), Resolved::Inline(0));
    }

    #[test]
    fn above_a_member_wins_over_the_enclosing_class() {
        let targets = [target(1, 20), target(5, 10)];
        assert_eq!(resolve(block(4, 4), &targets), Resolved::Above(1));
    }

    #[test]
    fn same_line_as_the_declaration_is_inline() {
        assert_eq!(resolve(block(2, 2), &[target(2, 2)]), Resolved::Inline(0));
    }

    #[test]
    fn span_running_onto_the_declaration_line_still_documents() {
        assert_eq!(resolve(block(3, 4), &[target(4, 7)]), Resolved::Above(0));
    }

    #[test]
    fn after_the_last_declaration_is_unassociated() {
        assert_eq!(
            resolve(block(9, 9), &[target(2, 4)]),
            Resolved::Unassociated
        );
    }

    #[test]
    fn attributes_bridge_the_anchor() {
        assert_eq!(anchor_line(5, &[(4, 4), (3, 3)]), 3);
        assert_eq!(anchor_line(5, &[(2, 2)]), 5);
        assert_eq!(anchor_line(5, &[]), 5);
    }

    #[test]
    fn anchor_is_what_the_comment_must_touch() {
        let t = TargetSpan {
            start: 3,
            end: 5,
            anchor: 2,
        };
        assert_eq!(resolve(block(1, 1), &[t]), Resolved::Above(0));
    }
}
