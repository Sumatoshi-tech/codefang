//! Hand-written recursive-descent parser for the UAST query DSL.
//!
//! Implements the PEG grammar documented in the [`dsl`](crate::dsl) module
//! docs.

use crate::types::{DslLiteral, DslNode};

/// A DSL parse error. Callers wrap it with the `"DSL parse error: ..."` prefix
/// (part of the CLI compatibility contract).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ParseError(pub String);

/// Parses a DSL query string into its AST. Entry point for `Query <- Pipeline EOT`.
///
/// # Errors
///
/// Returns a [`ParseError`] describing the first grammar violation (unexpected
/// trailing input, unterminated literal, missing delimiter, ...).
pub fn parse(input: &str) -> Result<DslNode, ParseError> {
    let mut p = Parser {
        src: input.as_bytes(),
        pos: 0,
    };
    p.spacing();
    let node = p.pipeline()?;
    p.spacing();
    if p.pos != p.src.len() {
        return Err(ParseError(format!(
            "unexpected trailing input at byte {}",
            p.pos
        )));
    }
    Ok(node)
}

struct Parser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    /// `Spacing <- [ \t\n\r]*`
    fn spacing(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Consumes `lit` if it matches at the cursor; returns whether it did.
    fn consume(&mut self, lit: &str) -> bool {
        let bytes = lit.as_bytes();
        if self.src[self.pos..].starts_with(bytes) {
            self.pos += bytes.len();
            true
        } else {
            false
        }
    }

    /// Consumes `lit` followed by trailing `Spacing` (token rule).
    fn token(&mut self, lit: &str) -> bool {
        if self.consume(lit) {
            self.spacing();
            true
        } else {
            false
        }
    }

    /// `Pipeline <- Stage (PIPE Stage)*`
    fn pipeline(&mut self) -> Result<DslNode, ParseError> {
        let first = self.stage()?;
        let mut stages = vec![first];
        loop {
            let save = self.pos;
            if self.token("|") {
                stages.push(self.stage()?);
            } else {
                self.pos = save;
                break;
            }
        }
        if stages.len() == 1 {
            Ok(stages.pop().unwrap())
        } else {
            Ok(DslNode::Pipeline(stages))
        }
    }

    /// `Stage <- MapOp / FilterOp / ReduceOp / RMapOp / RFilterOp / FunctionCall / FieldAccess`
    fn stage(&mut self) -> Result<DslNode, ParseError> {
        if let Some(n) = self.op_call("map")? {
            return Ok(DslNode::Map(Box::new(n)));
        }
        if let Some(n) = self.op_call("filter")? {
            return Ok(DslNode::Filter(Box::new(n)));
        }
        if let Some(n) = self.reduce_op()? {
            return Ok(DslNode::Reduce(Box::new(n)));
        }
        if let Some(n) = self.op_call("rmap")? {
            return Ok(DslNode::RMap(Box::new(n)));
        }
        if let Some(n) = self.op_call("rfilter")? {
            return Ok(DslNode::RFilter(Box::new(n)));
        }
        // FunctionCall before FieldAccess (a bare identifier followed by '(').
        let save = self.pos;
        if let Some(call) = self.try_function_call()? {
            return Ok(call);
        }
        self.pos = save;
        self.field_access()
    }

    /// Parses `<keyword> LPAR Expr RPAR`, returning the inner expr if the
    /// keyword (as a whole word followed by `(`) matched, else restoring pos.
    fn op_call(&mut self, keyword: &str) -> Result<Option<DslNode>, ParseError> {
        let save = self.pos;
        if !self.consume(keyword) {
            return Ok(None);
        }
        // The keyword must be immediately followed by optional spacing then '('.
        self.spacing();
        if !self.consume("(") {
            self.pos = save;
            return Ok(None);
        }
        self.spacing();
        let expr = self.expr()?;
        self.spacing();
        if !self.consume(")") {
            return Err(ParseError(format!("expected ')' to close {keyword}(")));
        }
        self.spacing();
        Ok(Some(expr))
    }

    /// `Reduce <- 'reduce' ((Spacing '(' Spacing ReducerName Spacing ')') / (Spacing ReducerName))`
    /// where `ReducerName <- [a-zA-Z_][a-zA-Z0-9_]*`. The reducer name becomes a
    /// bare `Call{name, args:[]}`, NOT a general `Expr` — so `reduce(count)`
    /// parses `count` as an identifier, not as a literal.
    fn reduce_op(&mut self) -> Result<Option<DslNode>, ParseError> {
        let save = self.pos;
        if !self.consume("reduce") {
            return Ok(None);
        }
        self.spacing();
        if self.consume("(") {
            self.spacing();
            let Some(name) = self.identifier() else {
                return Err(ParseError("expected reducer name in reduce(".into()));
            };
            self.spacing();
            if !self.consume(")") {
                return Err(ParseError("expected ')' to close reduce(".into()));
            }
            self.spacing();
            return Ok(Some(DslNode::Call {
                name,
                args: Vec::new(),
            }));
        }
        // Paren-less form: `reduce <ReducerName>`.
        let Some(name) = self.identifier() else {
            self.pos = save;
            return Ok(None);
        };
        self.spacing();
        Ok(Some(DslNode::Call {
            name,
            args: Vec::new(),
        }))
    }

    /// `FunctionCall <- Identifier LPAR ArgList? RPAR`
    fn try_function_call(&mut self) -> Result<Option<DslNode>, ParseError> {
        let save = self.pos;
        let Some(name) = self.identifier() else {
            self.pos = save;
            return Ok(None);
        };
        self.spacing();
        if !self.consume("(") {
            self.pos = save;
            return Ok(None);
        }
        self.spacing();
        let mut args = Vec::new();
        if self.peek() != Some(b')') {
            // ArgList <- Expr (COMMA Expr)*
            args.push(self.expr()?);
            loop {
                let s = self.pos;
                if self.token(",") {
                    args.push(self.expr()?);
                } else {
                    self.pos = s;
                    break;
                }
            }
        }
        self.spacing();
        if !self.consume(")") {
            return Err(ParseError("expected ')' to close function call".into()));
        }
        self.spacing();
        Ok(Some(DslNode::Call { name, args }))
    }

    /// `FieldAccess <- DOT Identifier (DOT Identifier)*`
    fn field_access(&mut self) -> Result<DslNode, ParseError> {
        if !self.consume(".") {
            return Err(ParseError(format!(
                "expected field access ('.') at byte {}",
                self.pos
            )));
        }
        self.spacing();
        let Some(first) = self.identifier() else {
            return Err(ParseError("expected identifier after '.'".into()));
        };
        let mut fields = vec![first];
        self.spacing();
        loop {
            let save = self.pos;
            if self.consume(".") {
                self.spacing();
                if let Some(id) = self.identifier() {
                    fields.push(id);
                    self.spacing();
                } else {
                    self.pos = save;
                    break;
                }
            } else {
                self.pos = save;
                break;
            }
        }
        Ok(DslNode::Field(fields))
    }

    /// `Expr <- Comparison / FieldAccess / FunctionCall / Literal`
    fn expr(&mut self) -> Result<DslNode, ParseError> {
        // Try Comparison first (it begins with FieldAccess/FunctionCall).
        let save = self.pos;
        if let Some(cmp) = self.try_comparison()? {
            return Ok(cmp);
        }
        self.pos = save;

        if self.peek() == Some(b'.') {
            return self.field_access();
        }
        let save2 = self.pos;
        if let Some(call) = self.try_function_call()? {
            return Ok(call);
        }
        self.pos = save2;
        self.literal()
    }

    /// `Comparison <- (FieldAccess / FunctionCall) CompareOp (Literal / FieldAccess)`
    fn try_comparison(&mut self) -> Result<Option<DslNode>, ParseError> {
        let save = self.pos;
        // lhs: FieldAccess or FunctionCall.
        let lhs = if self.peek() == Some(b'.') {
            self.field_access()?
        } else if let Some(call) = self.try_function_call()? {
            call
        } else {
            self.pos = save;
            return Ok(None);
        };
        self.spacing();
        let Some(op) = self.compare_op() else {
            self.pos = save;
            return Ok(None);
        };
        // `has` is a keyword operator (`Membership <- FieldAccess 'has' Value`):
        // it must be a whole word, not a prefix of an identifier.
        if op == "has" {
            if let Some(c) = self.peek() {
                if c.is_ascii_alphanumeric() || c == b'_' {
                    self.pos = save;
                    return Ok(None);
                }
            }
        }
        self.spacing();
        // rhs: Literal or FieldAccess.
        let rhs = if self.peek() == Some(b'.') {
            self.field_access()?
        } else {
            self.literal()?
        };
        Ok(Some(DslNode::Comparison {
            lhs: Box::new(lhs),
            op,
            rhs: Box::new(rhs),
        }))
    }

    /// `CompareOp <- "==" / "!=" / "<=" / ">=" / "<" / ">"`, plus the membership
    /// keyword `has` (`Membership <- FieldAccess 'has' Value`), which the caller
    /// guards with a word-boundary check.
    fn compare_op(&mut self) -> Option<String> {
        for op in ["==", "!=", "<=", ">=", "<", ">", "has"] {
            if self.consume(op) {
                return Some(op.to_string());
            }
        }
        None
    }

    /// `Literal <- StringLiteral / NumberLiteral / BoolLiteral`
    fn literal(&mut self) -> Result<DslNode, ParseError> {
        match self.peek() {
            Some(b'\'' | b'"') => {
                let quote = self.peek().unwrap();
                self.pos += 1;
                let start = self.pos;
                while let Some(c) = self.peek() {
                    if c == quote {
                        break;
                    }
                    self.pos += 1;
                }
                if self.peek() != Some(quote) {
                    return Err(ParseError("unterminated string literal".into()));
                }
                let s = std::str::from_utf8(&self.src[start..self.pos])
                    .map_err(|_| ParseError("invalid UTF-8 in string literal".into()))?
                    .to_string();
                self.pos += 1; // closing quote
                self.spacing();
                Ok(DslNode::Literal(DslLiteral::Str(s)))
            }
            Some(c) if c.is_ascii_digit() => {
                let start = self.pos;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.pos += 1;
                }
                if self.peek() == Some(b'.') {
                    let save = self.pos;
                    self.pos += 1;
                    if self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                            self.pos += 1;
                        }
                    } else {
                        self.pos = save; // a lone '.' is not part of the number
                    }
                }
                let num = std::str::from_utf8(&self.src[start..self.pos])
                    .unwrap()
                    .to_string();
                self.spacing();
                Ok(DslNode::Literal(DslLiteral::Number(num)))
            }
            _ => {
                if self.consume("true") {
                    self.spacing();
                    Ok(DslNode::Literal(DslLiteral::Bool(true)))
                } else if self.consume("false") {
                    self.spacing();
                    Ok(DslNode::Literal(DslLiteral::Bool(false)))
                } else {
                    Err(ParseError(format!("expected literal at byte {}", self.pos)))
                }
            }
        }
    }

    /// `Identifier <- [a-zA-Z_] [a-zA-Z0-9_]*`
    fn identifier(&mut self) -> Option<String> {
        let start = self.pos;
        match self.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == b'_' => self.pos += 1,
            _ => return None,
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        Some(
            std::str::from_utf8(&self.src[start..self.pos])
                .unwrap()
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_field_access() {
        assert_eq!(
            parse(".children").unwrap(),
            DslNode::Field(vec!["children".into()])
        );
        assert_eq!(
            parse(".a.b.c").unwrap(),
            DslNode::Field(vec!["a".into(), "b".into(), "c".into()])
        );
    }

    #[test]
    fn parses_filter_with_comparison() {
        let ast = parse("filter(.type == 'Function')").unwrap();
        match ast {
            DslNode::Filter(inner) => match *inner {
                DslNode::Comparison { op, .. } => assert_eq!(op, "=="),
                other => panic!("expected comparison, got {other:?}"),
            },
            other => panic!("expected filter, got {other:?}"),
        }
    }

    #[test]
    fn parses_pipeline() {
        let ast = parse("map(.children) | filter(.type == 'Function')").unwrap();
        match ast {
            DslNode::Pipeline(stages) => assert_eq!(stages.len(), 2),
            other => panic!("expected pipeline, got {other:?}"),
        }
    }

    #[test]
    fn parses_function_call() {
        let ast = parse("count()").unwrap();
        assert!(matches!(ast, DslNode::Call { .. }));
        let ast = parse("contains('foo')").unwrap();
        match ast {
            DslNode::Call { name, args } => {
                assert_eq!(name, "contains");
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected call, got {other:?}"),
        }
    }

    #[test]
    fn parses_number_literal() {
        let ast = parse("filter(.token == 42)").unwrap();
        match ast {
            DslNode::Filter(inner) => match *inner {
                DslNode::Comparison { rhs, .. } => {
                    assert_eq!(*rhs, DslNode::Literal(DslLiteral::Number("42".into())));
                }
                other => panic!("got {other:?}"),
            },
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert!(parse(".a $$$").is_err());
    }
}
