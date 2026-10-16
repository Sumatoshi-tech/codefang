//! Per-function metric computation: one walk over a function's subtree
//! produces its cyclomatic complexity, cognitive complexity, and nesting
//! depth; lines of code come from the source span.
//!
//! # Counting rules
//!
//! **Cyclomatic (`McCabe`)**: `1` + one per `if` / `else if` / conditional
//! expression, per loop, per non-default `case`/arm, per `catch`, and per
//! logical operator (`&&`, `||`, `and`, `or`).
//!
//! **Cognitive (`SonarSource`)**: structural increments for `if`, `else if`,
//! `else`, conditional expressions, `switch`/`match`, loops, `catch`,
//! labelled jumps (`break label`, `continue label`, `goto`) and direct
//! recursion; `+1` per sequence of like logical operators (`a && b && c || d`
//! is two); `if`, conditional expressions, `switch`/`match`, loops and
//! `catch` also add the current nesting level, and raise it for what they
//! contain. `else`/`else if` add a flat `+1` and never a nesting increment.
//! Lambdas and anonymous function literals raise nesting for their contents
//! without an increment of their own. `try` adds nothing. The function's own
//! body (including an arrow/function-expression wrapper) is nesting level 0.
//! A named nested function is a separate unit and is skipped here.
//!
//! **Nesting depth**: the maximum depth of nested control-flow blocks —
//! `if`/`else` chains (one level for the whole chain), loops,
//! `switch`/`match`, and `try`/`catch` (one level for the statement).
//! Conditional expressions and lambdas are not blocks.
//!
//! **Lines of code**: non-blank lines inside the function's span that carry
//! something other than comment text; the lines of a nested named function
//! belong to that function, not to the enclosing one.
//!
//! # Operator recovery
//!
//! A binary operator is read from `props["operator"]`, else from the node's
//! own token when that is a bare operator symbol, else from the source text
//! between its two operands. A `Synthetic` node with two operands whose gap
//! is a logical word/symbol (Python `and`/`or` before the mapping contract)
//! is treated as a logical operator too.

use crate::discover::{
    body_child, has_name, identifier_text, is_body_wrapper, is_declaration_only,
    is_function_candidate, ANONYMOUS_FUNCTION_NAME,
};
use crate::node::{role, uast, Node};

/// Stack red zone / growth chunk for the recursive walks (sources nest
/// beyond 10k levels; cf-uast builds those trees on a segmented stack).
const STACK_RED_ZONE: usize = 64 * 1024;
const STACK_GROWTH: usize = 16 * 1024 * 1024;

/// Longest token that can be a bare operator symbol (`and`, `<<=`).
const MAX_BARE_OPERATOR_LEN: usize = 3;
/// Characters a symbolic operator token is built from.
const OPERATOR_CHARS: &str = "&|!=<>+-*/%^~.?:";
/// Keywords that open an `if` statement (as opposed to a conditional
/// expression, whose text starts with an operand).
const IF_KEYWORDS: [&str; 4] = ["if", "elif", "unless", "when"];
/// Keyword that opens an `else` branch in every supported language.
const ELSE_KEYWORD: &str = "else";
/// Case-label keyword (`case _:` is Python's wildcard arm).
const CASE_KEYWORD: &str = "case";
/// Default-arm spellings: `default`, `_`, `else` (Kotlin `when`).
const DEFAULT_ARM_WORDS: [&str; 3] = ["default", "_", "else"];

/// The three structural metrics of one function.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Measured {
    pub cyclomatic: i64,
    pub cognitive: i64,
    pub nesting: i64,
}

/// Byte offsets of every line start, so per-function LOC counting never
/// rescans the file.
pub(crate) struct LineIndex {
    starts: Vec<usize>,
    len: usize,
}

impl LineIndex {
    pub(crate) fn new(source: &[u8]) -> Self {
        let mut starts = vec![0usize];
        starts.extend(
            source
                .iter()
                .enumerate()
                .filter(|(_, b)| **b == b'\n')
                .map(|(i, _)| i + 1),
        );
        LineIndex {
            starts,
            len: source.len(),
        }
    }

    /// Byte range of the 1-based `line` (without its newline), clipped to
    /// the source.
    fn line_range(&self, line: u32) -> Option<std::ops::Range<usize>> {
        let idx = (line as usize).checked_sub(1)?;
        let start = *self.starts.get(idx)?;
        let end = self.starts.get(idx + 1).map_or(self.len, |next| next - 1);
        Some(start..end.max(start))
    }
}

/// Logical operator kinds; a change of kind inside a sequence is a new
/// cognitive increment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Logical {
    And,
    Or,
}

/// Cognitive nesting level, structural depth, and whether the walk is inside
/// an already-counted logical-operator sequence.
#[derive(Debug, Clone, Copy)]
struct Frame {
    nesting: i64,
    depth: i64,
    in_sequence: bool,
}

impl Frame {
    const BASE: Frame = Frame {
        nesting: 0,
        depth: 0,
        in_sequence: false,
    };

    fn fresh(self) -> Frame {
        Frame {
            in_sequence: false,
            ..self
        }
    }

    fn nested(self) -> Frame {
        Frame {
            nesting: self.nesting + 1,
            depth: self.depth + 1,
            in_sequence: false,
        }
    }

    fn deeper_nesting(self) -> Frame {
        Frame {
            nesting: self.nesting + 1,
            in_sequence: false,
            ..self
        }
    }
}

/// How an `If` node's children split into condition, then-branch, and else
/// branches, or whether it is a conditional expression.
struct IfShape {
    ternary: bool,
    then_idx: Option<usize>,
}

/// One else branch: an `else if` continuation (its `If`, through at most one
/// wrapper) or a plain `else`.
enum ElseBranch<'a> {
    ElseIf(&'a Node),
    Else(&'a Node),
}

struct Walker<'s> {
    source: Option<&'s [u8]>,
    function_name: &'s str,
    out: Measured,
}

/// Computes the structural metrics of `fn_node`, named `name` (used for
/// recursion detection; [`ANONYMOUS_FUNCTION_NAME`] disables it).
pub(crate) fn measure(fn_node: &Node, name: &str, source: Option<&[u8]>) -> Measured {
    let mut walker = Walker {
        source,
        function_name: if name == ANONYMOUS_FUNCTION_NAME {
            ""
        } else {
            name
        },
        out: Measured {
            cyclomatic: 1,
            ..Measured::default()
        },
    };
    let own_body = body_child(fn_node);
    for child in &fn_node.children {
        let is_own_wrapper =
            own_body.is_some_and(|b| std::ptr::eq(b, child)) && is_body_wrapper(fn_node, child);
        if is_own_wrapper {
            // The function's own body literal: level 0, never a nested lambda.
            walker.walk_children(child, Frame::BASE);
        } else {
            walker.walk(child, Some(fn_node), Frame::BASE);
        }
    }
    walker.out
}

impl Walker<'_> {
    fn walk(&mut self, n: &Node, parent: Option<&Node>, frame: Frame) {
        stacker::maybe_grow(STACK_RED_ZONE, STACK_GROWTH, || {
            self.walk_inner(n, parent, frame);
        });
    }

    fn walk_children(&mut self, n: &Node, frame: Frame) {
        for child in &n.children {
            self.walk(child, Some(n), frame);
        }
    }

    fn walk_inner(&mut self, n: &Node, parent: Option<&Node>, frame: Frame) {
        match n.node_type.as_str() {
            uast::IF => self.process_if(n, frame, false),
            uast::LOOP => self.process_loop(n, parent, frame),
            uast::SWITCH | uast::MATCH => self.process_switch(n, frame),
            uast::CASE => self.process_case(n, parent, frame),
            uast::TRY => self.process_try(n, frame),
            uast::CATCH => self.process_catch(n, parent, frame),
            uast::LAMBDA => self.walk_children(n, frame.deeper_nesting()),
            uast::FUNCTION | uast::METHOD => self.process_nested_function(n, frame),
            uast::CALL => self.process_call(n, frame),
            uast::BREAK | uast::CONTINUE => self.process_jump(n),
            _ => self.process_expression(n, frame),
        }
    }

    fn enter_depth(&mut self, depth: i64) {
        if depth > self.out.nesting {
            self.out.nesting = depth;
        }
    }

    fn process_if(&mut self, n: &Node, frame: Frame, is_else_if: bool) {
        self.out.cyclomatic += 1;
        let shape = IfShape::classify(n, self.source);
        if shape.ternary {
            self.out.cognitive += frame.nesting + 1;
            self.walk_children(n, frame.deeper_nesting());
            return;
        }
        self.out.cognitive += if is_else_if { 1 } else { frame.nesting + 1 };
        let inner = frame.nested();
        self.enter_depth(inner.depth);
        let then_idx = shape.then_idx.unwrap_or(n.children.len());
        for (idx, child) in n.children.iter().enumerate() {
            match idx.cmp(&then_idx) {
                std::cmp::Ordering::Less => self.walk(child, Some(n), frame.fresh()),
                std::cmp::Ordering::Equal => self.walk(child, Some(n), inner),
                std::cmp::Ordering::Greater => self.process_else(n, child, frame, inner),
            }
        }
    }

    /// One branch after the then-branch: a flat `+1` for `else if`
    /// (recursing with the head's frame) or `else` (contents nested).
    fn process_else(&mut self, head: &Node, child: &Node, frame: Frame, inner: Frame) {
        match classify_else(child) {
            ElseBranch::ElseIf(inner_if) => self.process_if(inner_if, frame, true),
            ElseBranch::Else(branch) => {
                self.out.cognitive += 1;
                self.walk(branch, Some(head), inner);
            }
        }
    }

    fn process_loop(&mut self, n: &Node, parent: Option<&Node>, frame: Frame) {
        if is_for_clause_header(n, parent) {
            self.walk_children(n, frame.fresh());
            return;
        }
        self.out.cyclomatic += 1;
        self.out.cognitive += frame.nesting + 1;
        let inner = frame.nested();
        self.enter_depth(inner.depth);
        self.walk_children(n, inner);
    }

    fn process_switch(&mut self, n: &Node, frame: Frame) {
        self.out.cognitive += frame.nesting + 1;
        let inner = frame.nested();
        self.enter_depth(inner.depth);
        self.walk_children(n, inner);
    }

    fn process_case(&mut self, n: &Node, parent: Option<&Node>, frame: Frame) {
        let is_pattern = parent.is_some_and(|p| p.is_type(uast::CASE));
        if !is_pattern && !is_default_case(n, self.source) {
            self.out.cyclomatic += 1;
        }
        self.walk_children(n, frame.fresh());
    }

    fn process_try(&mut self, n: &Node, frame: Frame) {
        let inner = Frame {
            depth: frame.depth + 1,
            ..frame.fresh()
        };
        self.enter_depth(inner.depth);
        self.walk_children(n, inner);
    }

    fn process_catch(&mut self, n: &Node, parent: Option<&Node>, frame: Frame) {
        self.out.cyclomatic += 1;
        self.out.cognitive += frame.nesting + 1;
        // The enclosing `try` already opened the statement's nesting level.
        let depth = if parent.is_some_and(|p| p.is_type(uast::TRY)) {
            frame.depth
        } else {
            frame.depth + 1
        };
        let inner = Frame {
            nesting: frame.nesting + 1,
            depth,
            in_sequence: false,
        };
        self.enter_depth(inner.depth);
        self.walk_children(n, inner);
    }

    /// A named nested function is a separate unit; an anonymous function
    /// literal is a lambda of this function; a declaration-only node is
    /// signature noise and is walked through.
    fn process_nested_function(&mut self, n: &Node, frame: Frame) {
        if !is_function_candidate(n) || is_declaration_only(n) {
            self.walk_children(n, frame.fresh());
        } else if !has_name(n, self.source) {
            self.walk_children(n, frame.deeper_nesting());
        }
    }

    fn process_call(&mut self, n: &Node, frame: Frame) {
        if !self.function_name.is_empty()
            && callee_name(n, self.source).as_deref() == Some(self.function_name)
        {
            self.out.cognitive += 1;
        }
        self.walk_children(n, frame.fresh());
    }

    /// `break label` / `continue label` / `goto label` carry a target child;
    /// a plain `break`/`continue` has none and adds nothing.
    fn process_jump(&mut self, n: &Node) {
        if !n.children.is_empty() {
            self.out.cognitive += 1;
        }
    }

    /// Logical operators: `+1` cyclomatic each; `+1` cognitive per sequence
    /// of like operators, counted once at the sequence root. Wrappers
    /// (`Synthetic` parentheses, unary `!`) are transparent to a sequence.
    fn process_expression(&mut self, n: &Node, frame: Frame) {
        if logical_operator(n, self.source).is_some() {
            self.out.cyclomatic += 1;
            if !frame.in_sequence {
                self.out.cognitive += sequence_increments(n, self.source);
            }
            self.walk_children(
                n,
                Frame {
                    in_sequence: true,
                    ..frame
                },
            );
        } else if is_sequence_wrapper(n) {
            self.walk_children(n, frame);
        } else {
            self.walk_children(n, frame.fresh());
        }
    }
}

impl IfShape {
    fn classify(n: &Node, source: Option<&[u8]>) -> Self {
        let text = n
            .span_text(source)
            .map(str::trim_start)
            .filter(|t| !t.is_empty())
            .or_else(|| Some(n.token.trim_start()).filter(|t| !t.is_empty()));
        let has_block = n
            .children
            .iter()
            .any(|c| c.has_role(role::BODY) || c.is_type(uast::BLOCK));
        let ternary = match text {
            Some(t) => !starts_with_keyword(t, &IF_KEYWORDS),
            None => !has_block && n.children.len() == 3,
        };
        let then_idx = n
            .children
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, c)| {
                (c.has_role(role::BODY) || c.is_type(uast::BLOCK)) && !is_else_wrapper(c)
            })
            .map(|(idx, _)| idx)
            .or_else(|| (n.children.len() >= 2).then_some(1));
        IfShape { ternary, then_idx }
    }
}

/// True when `text` starts with one of `keywords` as a whole word.
fn starts_with_keyword(text: &str, keywords: &[&str]) -> bool {
    keywords.iter().any(|kw| {
        text.strip_prefix(kw)
            .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
    })
}

/// A wrapper whose token starts with `else` (C/JS lower braceless and
/// `else if` branches into a `Body`-role wrapper carrying that text).
fn is_else_wrapper(n: &Node) -> bool {
    starts_with_keyword(n.token.trim_start(), &[ELSE_KEYWORD])
}

/// Classifies one child after the then-branch. An `If` is an `else if`; a
/// one-child `Synthetic`/`Block` wrapper around an `If` is an `else if` when
/// it ends where the `If` ends (the `else` keyword is the only extra text),
/// and a braced `else { if ... }` otherwise.
fn classify_else(child: &Node) -> ElseBranch<'_> {
    if child.is_type(uast::IF) {
        return ElseBranch::ElseIf(child);
    }
    let is_wrapper = child.has_any_type(&[uast::SYNTHETIC, uast::BLOCK]);
    let only_if = child.children.len() == 1 && child.children[0].is_type(uast::IF);
    if !is_wrapper || !only_if {
        return ElseBranch::Else(child);
    }
    let inner = &child.children[0];
    let ends_together = match (&child.pos, &inner.pos) {
        (Some(outer), Some(inner)) => outer.end_offset == inner.end_offset,
        _ => is_else_wrapper(child),
    };
    if ends_together {
        ElseBranch::ElseIf(inner)
    } else {
        ElseBranch::Else(child)
    }
}

/// Go lowers a three-clause `for` as `Loop{Loop{init, cond, post}, Block}`:
/// the inner, body-less first child is the clause header, not a loop.
fn is_for_clause_header(n: &Node, parent: Option<&Node>) -> bool {
    let Some(parent) = parent else {
        return false;
    };
    parent.is_type(uast::LOOP)
        && parent.children.first().is_some_and(|c| std::ptr::eq(c, n))
        && parent.children.iter().any(|c| c.has_role(role::BODY))
        && !n.children.iter().any(|c| c.has_role(role::BODY))
}

/// True for a `default` / `_` / `else` arm, or a `case _:` wildcard. Reads
/// the arm's own token (never its body) and its pattern child.
fn is_default_case(n: &Node, source: Option<&[u8]>) -> bool {
    let token = n.token.trim();
    let mut words = token
        .split(|c: char| c.is_whitespace() || c == ':')
        .filter(|w| !w.is_empty());
    let first = words.next().unwrap_or("");
    if DEFAULT_ARM_WORDS
        .iter()
        .any(|w| first.eq_ignore_ascii_case(w))
    {
        return true;
    }
    if first.eq_ignore_ascii_case(CASE_KEYWORD) && words.next() == Some("_") {
        return true;
    }
    n.children.first().is_some_and(|c| {
        c.is_type(uast::PATTERN)
            && c.children.is_empty()
            && (c.token.trim() == "_" || c.span_text(source).map(str::trim) == Some("_"))
    })
}

/// The simple name a call targets: `props["name"]`, the identifier operand,
/// the last member of an access chain, or the call token up to `(`.
fn callee_name(call: &Node, source: Option<&[u8]>) -> Option<String> {
    if let Some(name) = call.prop("name").map(str::trim).filter(|s| !s.is_empty()) {
        return Some(name.to_string());
    }
    if let Some(target) = call.children.first() {
        if target.is_type(uast::IDENTIFIER) {
            return identifier_text(target, source);
        }
        if target.has_any_type(&[uast::FIELD, uast::SYNTHETIC]) {
            if let Some(last) = target
                .children
                .iter()
                .rev()
                .find(|c| c.is_type(uast::IDENTIFIER))
            {
                return identifier_text(last, source);
            }
        }
    }
    let head = call.token.split('(').next()?.trim();
    let simple = head.rsplit('.').next()?.trim();
    (!simple.is_empty() && !simple.contains(char::is_whitespace)).then(|| simple.to_string())
}

/// Nodes a logical-operator sequence looks through: parenthesised operands
/// and unary negation.
fn is_sequence_wrapper(n: &Node) -> bool {
    n.has_any_type(&[uast::SYNTHETIC, uast::UNARY_OP])
}

/// The logical operator of `n`, if it is a logical binary operation.
fn logical_operator(n: &Node, source: Option<&[u8]>) -> Option<Logical> {
    if n.children.len() < 2 {
        return None;
    }
    if n.is_type(uast::BINARY_OP) {
        return binary_operator(n, source).and_then(|op| classify_logical(&op));
    }
    if n.is_type(uast::SYNTHETIC) && n.children.len() == 2 {
        return operator_from_gap(n, source).and_then(|op| classify_logical(&op));
    }
    None
}

fn classify_logical(op: &str) -> Option<Logical> {
    match op {
        "&&" => Some(Logical::And),
        "||" => Some(Logical::Or),
        _ if op.eq_ignore_ascii_case("and") => Some(Logical::And),
        _ if op.eq_ignore_ascii_case("or") => Some(Logical::Or),
        _ => None,
    }
}

/// A `BinaryOp`'s operator: `props["operator"]`, else a bare-operator token,
/// else the source text between its operands.
pub(crate) fn binary_operator(n: &Node, source: Option<&[u8]>) -> Option<String> {
    if let Some(op) = n.prop("operator").map(str::trim).filter(|s| !s.is_empty()) {
        return Some(op.to_string());
    }
    let token = n.token.trim();
    if is_bare_operator(token) {
        return Some(token.to_string());
    }
    operator_from_gap(n, source)
}

/// True for a short token made only of operator characters, or a logical
/// word (`and`, `or`, `not`).
fn is_bare_operator(token: &str) -> bool {
    if token.is_empty() || token.len() > MAX_BARE_OPERATOR_LEN {
        return false;
    }
    token.chars().all(|c| OPERATOR_CHARS.contains(c))
        || ["and", "or", "not"]
            .iter()
            .any(|w| token.eq_ignore_ascii_case(w))
}

/// Recovers an operator from the source gap between the first two operands:
/// the gap stripped of whitespace and parentheses when that is a bare
/// operator, else the first logical operator found in the gap (comments
/// between operands are skipped over that way).
fn operator_from_gap(n: &Node, source: Option<&[u8]>) -> Option<String> {
    let source = source?;
    let left = n.children.first()?.pos.as_ref()?;
    let right = n.children.get(1)?.pos.as_ref()?;
    let (start, end) = (left.end_offset as usize, right.start_offset as usize);
    if start >= end || end > source.len() {
        return None;
    }
    let gap = std::str::from_utf8(&source[start..end]).ok()?;
    let compact: String = gap
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '(' && *c != ')')
        .collect();
    if is_bare_operator(&compact) {
        return Some(compact);
    }
    if let Some(sym) = ["&&", "||"].iter().find(|s| gap.contains(*s)) {
        return Some((*sym).to_string());
    }
    gap.split(|c: char| !c.is_alphanumeric() && c != '_')
        .find(|w| w.eq_ignore_ascii_case("and") || w.eq_ignore_ascii_case("or"))
        .map(str::to_string)
}

/// Cognitive increments of the logical-operator sequence rooted at `root`:
/// `1` for the sequence plus `1` per change of operator kind along the
/// in-order operator stream.
fn sequence_increments(root: &Node, source: Option<&[u8]>) -> i64 {
    let mut ops: Vec<Logical> = Vec::new();
    collect_sequence(root, source, &mut ops);
    if ops.is_empty() {
        return 0;
    }
    let changes = ops.windows(2).filter(|w| w[0] != w[1]).count();
    1 + changes as i64
}

fn collect_sequence(n: &Node, source: Option<&[u8]>, ops: &mut Vec<Logical>) {
    stacker::maybe_grow(STACK_RED_ZONE, STACK_GROWTH, || {
        if let Some(op) = logical_operator(n, source) {
            for (idx, child) in n.children.iter().enumerate() {
                if idx > 0 {
                    ops.push(op);
                }
                collect_sequence(child, source, ops);
            }
        } else if is_sequence_wrapper(n) {
            for child in &n.children {
                collect_sequence(child, source, ops);
            }
        }
    });
}

/// Non-blank, non-comment-only lines in `fn_node`'s span. Without source the
/// span's line count stands in; without positions the result is 0.
pub(crate) fn lines_of_code(
    fn_node: &Node,
    source: Option<&[u8]>,
    index: Option<&LineIndex>,
) -> i64 {
    let Some(pos) = &fn_node.pos else {
        return 0;
    };
    if pos.end_line < pos.start_line {
        return 0;
    }
    let (Some(source), Some(index)) = (source, index) else {
        return i64::from(pos.end_line - pos.start_line) + 1;
    };
    let excluded = excluded_ranges(fn_node, Some(source));
    let span = pos.start_offset as usize..pos.end_offset as usize;
    (pos.start_line..=pos.end_line)
        .filter_map(|line| index.line_range(line))
        .filter(|range| {
            let from = range.start.max(span.start);
            let to = range.end.min(span.end).min(source.len());
            (from..to).any(|i| !source[i].is_ascii_whitespace() && !in_ranges(&excluded, i))
        })
        .count() as i64
}

/// Sorted, merged byte ranges that do not count as this function's code:
/// every `Comment` node and every nested named function in the subtree.
fn excluded_ranges(fn_node: &Node, source: Option<&[u8]>) -> Vec<(usize, usize)> {
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut stack: Vec<&Node> = fn_node.children.iter().collect();
    while let Some(n) = stack.pop() {
        let is_nested_unit =
            is_function_candidate(n) && !is_declaration_only(n) && has_name(n, source);
        if n.is_type(uast::COMMENT) || is_nested_unit {
            if let Some(p) = &n.pos {
                ranges.push((p.start_offset as usize, p.end_offset as usize));
            }
            continue;
        }
        stack.extend(n.children.iter());
    }
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

fn in_ranges(ranges: &[(usize, usize)], offset: usize) -> bool {
    let idx = ranges.partition_point(|(start, _)| *start <= offset);
    idx > 0 && offset < ranges[idx - 1].1
}

#[cfg(test)]
mod tests {
    use super::{binary_operator, lines_of_code, measure, LineIndex, Measured};
    use crate::node::{role, uast, Node, Positions};

    fn func(children: Vec<Node>) -> Node {
        Node::new(uast::FUNCTION)
            .with_roles(vec![role::FUNCTION, role::DECLARATION])
            .with_prop("name", "f")
            .with_children(children)
    }

    fn block(children: Vec<Node>) -> Node {
        Node::new(uast::BLOCK)
            .with_roles(vec![role::BODY])
            .with_children(children)
    }

    fn cond() -> Node {
        Node::with_token(uast::IDENTIFIER, "x").with_roles(vec![role::NAME])
    }

    fn if_then(then: Vec<Node>) -> Node {
        Node::new(uast::IF).with_children(vec![cond(), block(then)])
    }

    fn m(cyclomatic: i64, cognitive: i64, nesting: i64) -> Measured {
        Measured {
            cyclomatic,
            cognitive,
            nesting,
        }
    }

    #[test]
    fn empty_function_is_one_zero_zero() {
        assert_eq!(measure(&func(vec![block(vec![])]), "f", None), m(1, 0, 0));
    }

    #[test]
    fn nested_ifs_add_nesting_increments() {
        let f = func(vec![block(vec![if_then(vec![if_then(vec![if_then(
            vec![],
        )])])])]);
        assert_eq!(measure(&f, "f", None), m(4, 6, 3));
    }

    #[test]
    fn else_if_is_flat_and_else_contents_are_nested() {
        // if x {} else if x {} else { if x {} }
        let else_block = block(vec![if_then(vec![])]);
        let else_if = Node::new(uast::IF).with_children(vec![cond(), block(vec![]), else_block]);
        let head = Node::new(uast::IF).with_children(vec![cond(), block(vec![]), else_if]);
        let f = func(vec![block(vec![head])]);
        // cyclomatic: 1 + if + else-if + inner if = 4
        // cognitive: if 1 + else-if 1 + else 1 + inner if (nesting 1) 2 = 5
        // nesting: chain 1, inner if 2
        assert_eq!(measure(&f, "f", None), m(4, 5, 2));
    }

    #[test]
    fn braced_else_with_one_if_is_an_else_not_an_else_if() {
        // Go: Block(else){If}: the block ends after the inner if.
        let inner_if = Node::new(uast::IF)
            .with_children(vec![cond(), block(vec![])])
            .with_pos(Positions {
                start_offset: 10,
                end_offset: 20,
                ..Default::default()
            });
        let else_block = block(vec![inner_if]).with_pos(Positions {
            start_offset: 8,
            end_offset: 22,
            ..Default::default()
        });
        let head = Node::new(uast::IF).with_children(vec![cond(), block(vec![]), else_block]);
        let f = func(vec![block(vec![head])]);
        assert_eq!(measure(&f, "f", None), m(3, 4, 2));
    }

    #[test]
    fn wrapped_else_if_sharing_the_end_offset_is_flat() {
        // C/JS: Block(else-if wrapper){If} ending exactly where the If ends.
        let inner_if = Node::new(uast::IF)
            .with_children(vec![cond(), block(vec![])])
            .with_pos(Positions {
                start_offset: 13,
                end_offset: 20,
                ..Default::default()
            });
        let wrapper = Node::with_token(uast::BLOCK, "else if (x) {}")
            .with_roles(vec![role::BODY])
            .with_children(vec![inner_if])
            .with_pos(Positions {
                start_offset: 8,
                end_offset: 20,
                ..Default::default()
            });
        let head = Node::new(uast::IF).with_children(vec![cond(), block(vec![]), wrapper]);
        let f = func(vec![block(vec![head])]);
        assert_eq!(measure(&f, "f", None), m(3, 2, 1));
    }

    #[test]
    fn ternary_is_one_increment_without_else_or_depth() {
        let ternary = Node::with_token(uast::IF, "x ? 1 : 2").with_children(vec![
            cond(),
            Node::new("Literal"),
            Node::new("Literal"),
        ]);
        let f = func(vec![block(vec![ternary])]);
        assert_eq!(measure(&f, "f", None), m(2, 1, 0));
    }

    #[test]
    fn loops_switch_and_cases() {
        let cases = vec![
            Node::with_token(uast::CASE, "case 1:"),
            Node::with_token(uast::CASE, "case 2:"),
            Node::with_token(uast::CASE, "default:"),
        ];
        let switch = Node::new(uast::SWITCH).with_children(cases);
        let lp = Node::new(uast::LOOP).with_children(vec![block(vec![switch])]);
        let f = func(vec![block(vec![lp])]);
        // cyclomatic: 1 + loop + 2 cases = 4; cognitive: loop 1 + switch 2 = 3
        assert_eq!(measure(&f, "f", None), m(4, 3, 2));
    }

    #[test]
    fn default_arm_spellings() {
        for token in ["default:", "DEFAULT:", "_ => 0", "case _:", "else ->"] {
            let sw =
                Node::new(uast::MATCH).with_children(vec![Node::with_token(uast::CASE, token)]);
            let f = func(vec![block(vec![sw])]);
            assert_eq!(measure(&f, "f", None).cyclomatic, 1, "{token}");
        }
        let named = Node::with_token(uast::CASE, "case defaultValue:");
        let sw = Node::new(uast::SWITCH).with_children(vec![named]);
        assert_eq!(
            measure(&func(vec![block(vec![sw])]), "f", None).cyclomatic,
            2
        );
    }

    #[test]
    fn pattern_case_inside_case_is_not_an_arm() {
        let arm = Node::with_token(uast::CASE, "case 1:")
            .with_children(vec![Node::new(uast::CASE), block(vec![])]);
        let mt = Node::new(uast::MATCH).with_children(vec![block(vec![arm])]);
        assert_eq!(
            measure(&func(vec![block(vec![mt])]), "f", None).cyclomatic,
            2
        );
    }

    #[test]
    fn try_adds_no_cognitive_and_catch_is_one_level() {
        let catch = Node::new(uast::CATCH).with_children(vec![block(vec![if_then(vec![])])]);
        let tr = Node::new(uast::TRY).with_children(vec![block(vec![if_then(vec![])]), catch]);
        let f = func(vec![block(vec![tr])]);
        // cyclomatic: 1 + if + catch + if = 4
        // cognitive: if-in-try 1 + catch 1 + if-in-catch (nesting 1) 2 = 4
        // nesting: try/catch 1, inner ifs 2
        assert_eq!(measure(&f, "f", None), m(4, 4, 2));
    }

    #[test]
    fn logical_sequences_count_once_per_run_of_like_operators() {
        let op = |o: &str, l: Node, r: Node| {
            Node::new(uast::BINARY_OP)
                .with_prop("operator", o)
                .with_children(vec![l, r])
        };
        // a && b && c || d
        let expr = op("||", op("&&", op("&&", cond(), cond()), cond()), cond());
        let ret = Node::new("Return").with_children(vec![expr]);
        let f = func(vec![block(vec![ret])]);
        assert_eq!(measure(&f, "f", None), m(4, 2, 0));

        // a && f(b || c): the call argument starts a new sequence.
        let call = Node::new(uast::CALL).with_children(vec![cond(), op("||", cond(), cond())]);
        let expr = op("&&", cond(), call);
        let f = func(vec![block(vec![
            Node::new("Return").with_children(vec![expr])
        ])]);
        assert_eq!(measure(&f, "f", None), m(3, 2, 0));

        // !(a || b) && c: the negation is transparent.
        let neg = Node::new(uast::UNARY_OP).with_children(vec![op("||", cond(), cond())]);
        let expr = op("&&", neg, cond());
        let f = func(vec![block(vec![
            Node::new("Return").with_children(vec![expr])
        ])]);
        assert_eq!(measure(&f, "f", None), m(3, 2, 0));
    }

    #[test]
    fn operator_recovery_order_is_prop_then_token_then_source() {
        let src = b"a > 0 && b > 0";
        let operand = |s: u32, e: u32| {
            Node::new(uast::BINARY_OP).with_pos(Positions {
                start_offset: s,
                end_offset: e,
                ..Default::default()
            })
        };
        let by_prop = Node::new(uast::BINARY_OP)
            .with_prop("operator", "||")
            .with_children(vec![operand(0, 5), operand(9, 14)]);
        assert_eq!(binary_operator(&by_prop, Some(src)).as_deref(), Some("||"));
        let by_token = Node::with_token(uast::BINARY_OP, "&&")
            .with_children(vec![operand(0, 5), operand(9, 14)]);
        assert_eq!(binary_operator(&by_token, None).as_deref(), Some("&&"));
        let by_source = Node::with_token(uast::BINARY_OP, "a > 0 && b > 0")
            .with_children(vec![operand(0, 5), operand(9, 14)]);
        assert_eq!(
            binary_operator(&by_source, Some(src)).as_deref(),
            Some("&&")
        );
        assert_eq!(binary_operator(&by_source, None), None);
        let paren = Node::new(uast::BINARY_OP).with_children(vec![operand(1, 5), operand(18, 19)]);
        assert_eq!(
            binary_operator(&paren, Some(b"(a||b) /* c */ && c")).as_deref(),
            Some("&&")
        );
    }

    #[test]
    fn python_word_operators_through_synthetic_nodes() {
        let src = b"if a and b or c:";
        let ident = |s: u32, e: u32| {
            Node::new(uast::IDENTIFIER).with_pos(Positions {
                start_offset: s,
                end_offset: e,
                ..Default::default()
            })
        };
        let inner = Node::new(uast::SYNTHETIC)
            .with_children(vec![ident(3, 4), ident(9, 10)])
            .with_pos(Positions {
                start_offset: 3,
                end_offset: 10,
                ..Default::default()
            });
        let outer = Node::new(uast::SYNTHETIC).with_children(vec![inner, ident(14, 15)]);
        let f = func(vec![block(vec![
            Node::new(uast::IF).with_children(vec![outer, block(vec![])])
        ])]);
        // cyclomatic: 1 + if + and + or = 4; cognitive: if 1 + sequence 1 + change 1 = 3
        assert_eq!(measure(&f, "f", Some(src)), m(4, 3, 1));
    }

    #[test]
    fn recursion_labelled_jumps_and_lambdas() {
        let call = Node::with_token(uast::CALL, "f(n-1)").with_children(vec![Node::with_token(
            uast::IDENTIFIER,
            "f",
        )
        .with_roles(vec![role::NAME])]);
        let other = Node::with_token(uast::CALL, "g(n)").with_children(vec![Node::with_token(
            uast::IDENTIFIER,
            "g",
        )
        .with_roles(vec![role::NAME])]);
        let labelled = Node::new(uast::BREAK).with_children(vec![Node::new(uast::IDENTIFIER)]);
        let plain = Node::new(uast::BREAK);
        let lambda = Node::new(uast::LAMBDA).with_children(vec![block(vec![if_then(vec![])])]);
        let f = func(vec![block(vec![call, other, labelled, plain, lambda])]);
        // cyclomatic: 1 + if = 2; cognitive: recursion 1 + label 1 + if@1 2 = 4; nesting 1
        assert_eq!(measure(&f, "f", None), m(2, 4, 1));
    }

    #[test]
    fn own_body_wrapper_is_level_zero_but_nested_literals_are_not() {
        let arrow_body = Node::new(uast::LAMBDA).with_children(vec![block(vec![if_then(vec![])])]);
        let arrow = func(vec![cond(), arrow_body]);
        assert_eq!(measure(&arrow, "f", None), m(2, 1, 1));

        let anon = Node::new(uast::FUNCTION)
            .with_roles(vec![role::FUNCTION, role::DECLARATION])
            .with_children(vec![block(vec![if_then(vec![])])]);
        let named = Node::new(uast::FUNCTION)
            .with_roles(vec![role::FUNCTION, role::DECLARATION])
            .with_prop("name", "inner")
            .with_children(vec![block(vec![if_then(vec![])])]);
        let f = func(vec![block(vec![anon, named])]);
        // anonymous literal: if at nesting 1 (+2); named inner: skipped.
        assert_eq!(measure(&f, "f", None), m(2, 2, 1));
    }

    #[test]
    fn go_for_clause_header_is_not_a_second_loop() {
        let header = Node::new(uast::LOOP).with_children(vec![cond()]);
        let lp = Node::new(uast::LOOP).with_children(vec![header, block(vec![])]);
        assert_eq!(measure(&func(vec![block(vec![lp])]), "f", None), m(2, 1, 1));
    }

    #[test]
    fn lines_of_code_skip_blank_and_comment_only_lines() {
        let src = b"func f() {\n\t// note\n\n\tx := 1 // trailing\n\t/* a\n\t   b */\n}\n";
        let comment = |s: u32, e: u32, line: u32, end_line: u32| {
            Node::new(uast::COMMENT).with_pos(Positions {
                start_offset: s,
                end_offset: e,
                start_line: line,
                end_line,
                ..Default::default()
            })
        };
        let f = Node::new(uast::FUNCTION)
            .with_pos(Positions {
                start_line: 1,
                end_line: 7,
                start_offset: 0,
                end_offset: u32::try_from(src.len()).unwrap() - 1,
                ..Default::default()
            })
            .with_children(vec![
                comment(12, 19, 2, 2),
                comment(29, 40, 4, 4),
                comment(42, 55, 5, 6),
            ]);
        let index = LineIndex::new(src);
        // Lines: `func f() {`, `x := 1`, `}` = 3.
        assert_eq!(lines_of_code(&f, Some(src), Some(&index)), 3);
        assert_eq!(
            lines_of_code(&f, None, None),
            7,
            "span fallback without source"
        );
        assert_eq!(lines_of_code(&Node::new(uast::FUNCTION), None, None), 0);
    }

    #[test]
    fn deep_nesting_does_not_overflow_the_stack() {
        const DEPTH: usize = 20_000;
        let mut inner = block(vec![]);
        for _ in 0..DEPTH {
            inner = block(vec![Node::new(uast::IF).with_children(vec![cond(), inner])]);
        }
        let got = measure(&func(vec![inner]), "f", None);
        assert_eq!(got.nesting, DEPTH as i64);
        assert_eq!(got.cyclomatic, DEPTH as i64 + 1);
    }
}
