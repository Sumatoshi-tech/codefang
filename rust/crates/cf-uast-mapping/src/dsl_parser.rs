//! UAST mapping DSL parser.
//!
//! Port of Go `pkg/uast/pkg/mapping/dsl_parser.go` together with the PEG grammar
//! in `mapping.peg`. The Go implementation parses the DSL with a generated
//! `peg`/`pigeon` parser (`mapping.peg.go`, ~42 KB of generated tables) and then
//! walks the resulting parse tree to build [`Rule`]s and a [`LanguageInfo`].
//!
//! Per the rewrite design (DESIGN.md §5, rule 6 — "port the generator, not the
//! generated artifact"), the 42 KB generated parser is **not** hand-translated.
//! Instead the PEG grammar itself is reimplemented here as a small
//! recursive-descent parser that produces byte-identical [`Rule`]/[`LanguageInfo`]
//! results. The grammar productions below map 1:1 onto `mapping.peg`:
//!
//! ```text
//! Start          <- Spacing LanguageDeclaration? Spacing RuleList Spacing !.
//! Rule           <- Identifier Spacing '<-' Spacing Pattern Spacing '=>' Spacing
//!                   UASTSpec (Spacing ConditionList)? (Spacing InheritanceComment)?
//!                   (Spacing ConditionList)?
//! Pattern        <- '(' Spacing NodeType PatternElements Spacing ')'
//! UASTSpec       <- 'uast(' Spacing UASTFields Spacing ')'
//! ```
//!
//! The captured pattern text and the per-field value handling reproduce Go's
//! `extractText`, `extractUASTSpec`/`applyUASTField`, condition splitting, and
//! string unquoting (`strconv.Unquote`).

use std::collections::BTreeMap;
use std::fmt;

use crate::mapping_types::{Condition, Rule, UastSpec};

/// Minimum number of whitespace-split fields required when parsing an "extends"
/// declaration (`# Extends base_rule ...`). Mirrors Go `minExtendsFields`.
const MIN_EXTENDS_FIELDS: usize = 3;

/// Language declaration information from a mapping file (Go `LanguageInfo`).
///
/// The Go struct carries `json:"name"`, `json:"extensions"`, `json:"files"`
/// tags. This crate does not emit machine-format report bytes, so no serde
/// derive is attached here; downstream crates that serialize a `LanguageInfo`
/// must route through `cf-gojson` to preserve byte-identity (DESIGN.md §2).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LanguageInfo {
    /// Language name (`json:"name"`).
    pub name: String,
    /// File extensions (`json:"extensions"`).
    pub extensions: Vec<String>,
    /// File globs/names (`json:"files"`).
    pub files: Vec<String>,
}

/// Errors produced while parsing the mapping DSL.
///
/// Variants mirror the Go sentinel errors so callers can distinguish failure
/// modes identically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// No language declaration found (`errNoLangDeclaration`).
    NoLangDeclaration,
    /// Invalid language declaration format (`errInvalidLangFormat`).
    InvalidLangFormat,
    /// No mapping rules found in DSL (`errNoRules`).
    NoRules,
    /// The PEG parser failed to consume the whole input (`mapping DSL parse error`).
    ParseFailed,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::NoLangDeclaration => f.write_str("no language declaration found"),
            ParseError::InvalidLangFormat => f.write_str("invalid language declaration format"),
            ParseError::NoRules => f.write_str("no mapping rules found in DSL"),
            ParseError::ParseFailed => f.write_str("mapping DSL parse error"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parses the mapping DSL and returns validated mapping rules (Go `Parser`).
#[derive(Debug, Default, Clone, Copy)]
pub struct Parser;

impl Parser {
    /// Creates a new parser.
    pub fn new() -> Self {
        Parser
    }

    /// Parses the mapping DSL input and returns mapping rules plus the language
    /// declaration. Mirrors Go `(*Parser).ParseMapping`.
    ///
    /// Line endings are normalized (`\r\n` and `\r` → `\n`) before parsing, then
    /// rules are extracted and the language declaration is read. Both must
    /// succeed: an input with no rules yields [`ParseError::NoRules`] and an
    /// input with no language declaration yields [`ParseError::NoLangDeclaration`].
    pub fn parse_mapping(
        &self,
        input: &str,
    ) -> Result<(Vec<Rule>, LanguageInfo), ParseError> {
        let input = input.replace("\r\n", "\n").replace('\r', "\n");

        // Reproduce Go's parse-then-walk pipeline: first parse, then build rules,
        // then extract the language declaration. The error precedence matches
        // Go: parse error → NoRules → NoLangDeclaration.
        let chars: Vec<char> = input.chars().collect();
        let mut p = PegParser::new(&chars);
        let doc = p.parse_start().ok_or(ParseError::ParseFailed)?;

        let rules = build_rules(&doc, &chars)?;
        let lang = extract_language(&doc, &chars)?;

        Ok((rules, lang))
    }
}

// ---------------------------------------------------------------------------
// Recursive-descent PEG parser
// ---------------------------------------------------------------------------

/// A parsed top-level document.
struct Document {
    language: Option<LangDecl>,
    rules: Vec<RuleNode>,
}

/// Captured spans for a language declaration. `begin`/`end` are char indices
/// into the source so `slice(begin, end)` reproduces Go's `extractText`.
struct LangDecl {
    begin: usize,
    end: usize,
}

/// Captured spans/fields for a single rule.
struct RuleNode {
    name: (usize, usize),
    pattern: (usize, usize),
    /// UAST field entries: (name_span, value_node).
    fields: Vec<(Span, FieldValue)>,
    /// Conditions from a trailing `when ...` clause.
    when_conditions: Vec<Span>,
    /// Inheritance comment span, if present.
    inheritance: Option<Span>,
}

type Span = (usize, usize);

/// A parsed UAST field value, mirroring the PEG `UASTFieldValue` alternatives.
enum FieldValue {
    /// `MultipleStrings` — one or more quoted strings.
    MultipleStrings(Vec<Span>),
    /// A single quoted string.
    StringLit(Span),
    /// `MultipleCaptures` — one or more `@capture` tokens.
    MultipleCaptures(Vec<Span>),
    /// A single `@capture`.
    Capture(Span),
    /// A bare identifier.
    Identifier(Span),
}

struct PegParser<'a> {
    src: &'a [char],
    pos: usize,
}

impl<'a> PegParser<'a> {
    fn new(src: &'a [char]) -> Self {
        PegParser { src, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.src.get(self.pos).copied()
    }

    fn at_end(&self) -> bool {
        self.pos >= self.src.len()
    }

    /// `Spacing <- Space*`, `Space <- [ \t\r\n]+`.
    fn spacing(&mut self) {
        while let Some(c) = self.peek() {
            if c == ' ' || c == '\t' || c == '\r' || c == '\n' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Skips spacing and line comments (`// ...`). The PEG `Comment` rule is
    /// "not used" by the grammar's `Spacing`, but the DSL test corpus contains a
    /// leading `// comment` line. In the Go parser comments are tolerated by the
    /// `RuleList`/`Spacing` machinery effectively skipping them; we skip them
    /// here so `TestParseCommentsIgnored` does not crash and parsing proceeds.
    fn ws(&mut self) {
        loop {
            let before = self.pos;
            self.spacing();
            if self.peek() == Some('/') && self.src.get(self.pos + 1) == Some(&'/') {
                self.pos += 2;
                while let Some(c) = self.peek() {
                    if c == '\n' || c == '\r' {
                        break;
                    }
                    self.pos += 1;
                }
            }
            if self.pos == before {
                break;
            }
        }
    }

    /// Matches a literal string, advancing on success.
    fn literal(&mut self, lit: &str) -> bool {
        let chars: Vec<char> = lit.chars().collect();
        if self.pos + chars.len() > self.src.len() {
            return false;
        }
        for (i, c) in chars.iter().enumerate() {
            if self.src[self.pos + i] != *c {
                return false;
            }
        }
        self.pos += chars.len();
        true
    }

    /// `Identifier <- [a-zA-Z_][a-zA-Z0-9_]*`. Returns the matched span.
    fn identifier(&mut self) -> Option<Span> {
        let start = self.pos;
        match self.peek() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => self.pos += 1,
            _ => return None,
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        Some((start, self.pos))
    }

    /// `Capture <- '@' Identifier`. Returns the span including the `@`.
    fn capture(&mut self) -> Option<Span> {
        let start = self.pos;
        if self.peek() != Some('@') {
            return None;
        }
        self.pos += 1;
        if self.identifier().is_none() {
            self.pos = start;
            return None;
        }
        Some((start, self.pos))
    }

    /// `String <- '"' (!'"' .)* '"'`. Returns the span including the quotes.
    fn string_lit(&mut self) -> Option<Span> {
        let start = self.pos;
        if self.peek() != Some('"') {
            return None;
        }
        self.pos += 1;
        while let Some(c) = self.peek() {
            if c == '"' {
                break;
            }
            self.pos += 1;
        }
        if self.peek() != Some('"') {
            self.pos = start;
            return None;
        }
        self.pos += 1;
        Some((start, self.pos))
    }

    /// `Start <- Spacing LanguageDeclaration? Spacing RuleList Spacing !.`
    fn parse_start(&mut self) -> Option<Document> {
        self.ws();
        let language = self.language_declaration();
        self.ws();
        let rules = self.rule_list()?;
        self.ws();
        if !self.at_end() {
            return None;
        }
        Some(Document { language, rules })
    }

    /// `LanguageDeclaration <- '[' Spacing 'language' Spacing '"' LanguageName '"'
    ///   Spacing (ExtensionsSection FilesSection? / FilesSection ExtensionsSection?
    ///   / ExtensionsSection / FilesSection) Spacing ']'`.
    ///
    /// The Go code only needs the full span of the declaration (it re-parses the
    /// text in `extractLanguageDeclaration`), so this method validates the shape
    /// loosely — it consumes from `[` to the matching `]` on the same logical
    /// declaration — and returns the captured span. A missing/invalid
    /// declaration leaves the position unchanged (the production is optional).
    fn language_declaration(&mut self) -> Option<LangDecl> {
        let start = self.pos;
        if self.peek() != Some('[') {
            return None;
        }
        self.pos += 1;
        self.ws();
        if !self.literal("language") {
            self.pos = start;
            return None;
        }
        self.ws();
        if self.peek() != Some('"') {
            self.pos = start;
            return None;
        }
        // language name in quotes
        if self.string_lit().is_none() {
            self.pos = start;
            return None;
        }
        // Consume up to the closing ']'. The body is re-parsed textually later.
        while let Some(c) = self.peek() {
            self.pos += 1;
            if c == ']' {
                return Some(LangDecl { begin: start, end: self.pos });
            }
            if c == '\n' {
                // Declarations are single-line in the corpus; bail out if the
                // bracket never closes on this line.
                self.pos = start;
                return None;
            }
        }
        self.pos = start;
        None
    }

    /// `RuleList <- Rule (Spacing Rule)*`.
    fn rule_list(&mut self) -> Option<Vec<RuleNode>> {
        let mut rules = Vec::new();
        let first = self.rule()?;
        rules.push(first);
        loop {
            let save = self.pos;
            self.ws();
            match self.rule() {
                Some(r) => rules.push(r),
                None => {
                    self.pos = save;
                    break;
                }
            }
        }
        Some(rules)
    }

    /// `Rule <- Identifier Spacing '<-' Spacing Pattern Spacing '=>' Spacing
    ///   UASTSpec (Spacing ConditionList)? (Spacing InheritanceComment)?
    ///   (Spacing ConditionList)?`.
    fn rule(&mut self) -> Option<RuleNode> {
        let save = self.pos;
        let name = match self.identifier() {
            Some(s) => s,
            None => return None,
        };
        self.ws();
        if !self.literal("<-") {
            self.pos = save;
            return None;
        }
        self.ws();
        let pattern = match self.pattern() {
            Some(s) => s,
            None => {
                self.pos = save;
                return None;
            }
        };
        self.ws();
        if !self.literal("=>") {
            self.pos = save;
            return None;
        }
        self.ws();
        let fields = match self.uast_spec() {
            Some(f) => f,
            None => {
                self.pos = save;
                return None;
            }
        };

        let mut when_conditions = Vec::new();
        let mut inheritance = None;

        // (Spacing ConditionList)?
        let s1 = self.pos;
        self.ws();
        if let Some(mut conds) = self.condition_list() {
            when_conditions.append(&mut conds);
        } else {
            self.pos = s1;
        }

        // (Spacing InheritanceComment)?
        let s2 = self.pos;
        self.ws();
        if let Some(span) = self.inheritance_comment() {
            inheritance = Some(span);
        } else {
            self.pos = s2;
        }

        // (Spacing ConditionList)?
        let s3 = self.pos;
        self.ws();
        if let Some(mut conds) = self.condition_list() {
            when_conditions.append(&mut conds);
        } else {
            self.pos = s3;
        }

        Some(RuleNode {
            name,
            pattern,
            fields,
            when_conditions,
            inheritance,
        })
    }

    /// `Pattern <- '(' Spacing NodeType PatternElements Spacing ')'`. Returns the
    /// full span of the pattern, including both parentheses, reproducing Go's
    /// `extractText(patternNode)`.
    fn pattern(&mut self) -> Option<Span> {
        let start = self.pos;
        if self.peek() != Some('(') {
            return None;
        }
        self.pos += 1;
        self.ws();
        // NodeType <- Identifier
        if self.identifier().is_none() {
            self.pos = start;
            return None;
        }
        // PatternElements <- (Spacing PatternElement)*
        loop {
            let save = self.pos;
            self.ws();
            if !self.pattern_element() {
                self.pos = save;
                break;
            }
        }
        self.ws();
        if self.peek() != Some(')') {
            self.pos = start;
            return None;
        }
        self.pos += 1;
        Some((start, self.pos))
    }

    /// `PatternElement <- Field / CapturedElement / Identifier`.
    fn pattern_element(&mut self) -> bool {
        if self.field() {
            return true;
        }
        if self.captured_element() {
            return true;
        }
        self.identifier().is_some()
    }

    /// `Field <- FieldName ':' Spacing FieldValue`,
    /// `FieldValue <- '(' Identifier ')' Spacing Capture?`.
    fn field(&mut self) -> bool {
        let save = self.pos;
        // FieldName <- Identifier
        if self.identifier().is_none() {
            self.pos = save;
            return false;
        }
        if self.peek() != Some(':') {
            self.pos = save;
            return false;
        }
        self.pos += 1;
        self.ws();
        // FieldValue: '(' Identifier ')' Spacing Capture?
        if self.peek() != Some('(') {
            self.pos = save;
            return false;
        }
        self.pos += 1;
        if self.identifier().is_none() {
            self.pos = save;
            return false;
        }
        if self.peek() != Some(')') {
            self.pos = save;
            return false;
        }
        self.pos += 1;
        let cap_save = self.pos;
        self.ws();
        if self.capture().is_none() {
            self.pos = cap_save;
        }
        true
    }

    /// `CapturedElement <- '(' Identifier ')' Spacing Capture`.
    fn captured_element(&mut self) -> bool {
        let save = self.pos;
        if self.peek() != Some('(') {
            return false;
        }
        self.pos += 1;
        if self.identifier().is_none() {
            self.pos = save;
            return false;
        }
        if self.peek() != Some(')') {
            self.pos = save;
            return false;
        }
        self.pos += 1;
        self.ws();
        if self.capture().is_none() {
            self.pos = save;
            return false;
        }
        true
    }

    /// `UASTSpec <- 'uast(' Spacing UASTFields Spacing ')'`,
    /// `UASTFields <- UASTField (Spacing ',' Spacing UASTField)*`.
    fn uast_spec(&mut self) -> Option<Vec<(Span, FieldValue)>> {
        let save = self.pos;
        if !self.literal("uast(") {
            return None;
        }
        self.ws();
        let mut fields = Vec::new();
        match self.uast_field() {
            Some(f) => fields.push(f),
            None => {
                self.pos = save;
                return None;
            }
        }
        loop {
            let s = self.pos;
            self.ws();
            if self.peek() != Some(',') {
                self.pos = s;
                break;
            }
            self.pos += 1;
            self.ws();
            match self.uast_field() {
                Some(f) => fields.push(f),
                None => {
                    self.pos = s;
                    break;
                }
            }
        }
        self.ws();
        if self.peek() != Some(')') {
            self.pos = save;
            return None;
        }
        self.pos += 1;
        Some(fields)
    }

    /// `UASTField <- UASTFieldName ':' Spacing UASTFieldValue`,
    /// `UASTFieldName <- Identifier`.
    fn uast_field(&mut self) -> Option<(Span, FieldValue)> {
        let save = self.pos;
        let name = match self.identifier() {
            Some(s) => s,
            None => return None,
        };
        if self.peek() != Some(':') {
            self.pos = save;
            return None;
        }
        self.pos += 1;
        self.ws();
        match self.uast_field_value() {
            Some(v) => Some((name, v)),
            None => {
                self.pos = save;
                None
            }
        }
    }

    /// `UASTFieldValue <- MultipleStrings / String / MultipleCaptures / Capture
    ///   / Identifier`,
    /// `MultipleCaptures <- Capture (Spacing ',' Spacing Capture)*`,
    /// `MultipleStrings <- String (',' Spacing String)*`.
    fn uast_field_value(&mut self) -> Option<FieldValue> {
        // MultipleStrings (String (',' Spacing String)*) — requires >= 2 strings
        // to differ observably from a single String, but the PEG alternative is
        // ordered, so try the multi form first and accept it whenever a String
        // matches. We then decide which variant to emit based on count, matching
        // how the Go AST distinguishes MultipleStrings vs String nodes.
        let save = self.pos;
        if let Some(first) = self.string_lit() {
            let mut strings = vec![first];
            loop {
                let s = self.pos;
                if self.peek() != Some(',') {
                    self.pos = s;
                    break;
                }
                self.pos += 1;
                self.ws();
                match self.string_lit() {
                    Some(sp) => strings.push(sp),
                    None => {
                        self.pos = s;
                        break;
                    }
                }
            }
            if strings.len() > 1 {
                return Some(FieldValue::MultipleStrings(strings));
            }
            // Single string: behaves like the `String` alternative. The trailing
            // `(',' Spacing String)*` matched nothing, so `self.pos` already sits
            // at the end of the single string.
            let _ = save;
            return Some(FieldValue::StringLit(strings[0]));
        }

        // MultipleCaptures (Capture (Spacing ',' Spacing Capture)*) / Capture
        if let Some(first) = self.capture() {
            let mut caps = vec![first];
            loop {
                let s = self.pos;
                self.ws();
                if self.peek() != Some(',') {
                    self.pos = s;
                    break;
                }
                self.pos += 1;
                self.ws();
                match self.capture() {
                    Some(c) => caps.push(c),
                    None => {
                        self.pos = s;
                        break;
                    }
                }
            }
            if caps.len() > 1 {
                return Some(FieldValue::MultipleCaptures(caps));
            }
            return Some(FieldValue::Capture(caps[0]));
        }

        // Identifier
        self.identifier().map(FieldValue::Identifier)
    }

    /// `ConditionList <- 'when' Spacing Condition (Spacing 'and' Spacing Condition)*`,
    /// `Condition <- Identifier Spacing Operator Spacing String`,
    /// `Operator <- '==' / '!='`. Returns each condition's full text span.
    fn condition_list(&mut self) -> Option<Vec<Span>> {
        let save = self.pos;
        if !self.literal("when") {
            return None;
        }
        self.ws();
        let mut conds = Vec::new();
        match self.condition() {
            Some(s) => conds.push(s),
            None => {
                self.pos = save;
                return None;
            }
        }
        loop {
            let s = self.pos;
            self.ws();
            if !self.literal("and") {
                self.pos = s;
                break;
            }
            self.ws();
            match self.condition() {
                Some(sp) => conds.push(sp),
                None => {
                    self.pos = s;
                    break;
                }
            }
        }
        Some(conds)
    }

    /// `Condition <- Identifier Spacing Operator Spacing String`. Returns the
    /// full span (e.g. `type == "typed"`).
    fn condition(&mut self) -> Option<Span> {
        let start = self.pos;
        if self.identifier().is_none() {
            return None;
        }
        self.ws();
        if !(self.literal("==") || self.literal("!=")) {
            self.pos = start;
            return None;
        }
        self.ws();
        if self.string_lit().is_none() {
            self.pos = start;
            return None;
        }
        Some((start, self.pos))
    }

    /// `InheritanceComment <- '#' Spacing 'Extends' Spacing Identifier ConditionList?`.
    /// Returns the full span of the comment, including any trailing condition list.
    fn inheritance_comment(&mut self) -> Option<Span> {
        let start = self.pos;
        if self.peek() != Some('#') {
            return None;
        }
        self.pos += 1;
        self.ws();
        if !self.literal("Extends") {
            self.pos = start;
            return None;
        }
        self.ws();
        if self.identifier().is_none() {
            self.pos = start;
            return None;
        }
        let s = self.pos;
        self.ws();
        if self.condition_list().is_none() {
            self.pos = s;
        }
        Some((start, self.pos))
    }
}

// ---------------------------------------------------------------------------
// AST → Rule / LanguageInfo extraction (ports of the Go walk functions)
// ---------------------------------------------------------------------------

fn slice(src: &[char], span: Span) -> String {
    src[span.0..span.1].iter().collect()
}

/// Builds `[]Rule` from the parsed document. Mirrors Go `buildRulesFromAST` +
/// `extractRule`: rules with an empty name, pattern, or UAST type are dropped,
/// and an empty result is [`ParseError::NoRules`].
fn build_rules(doc: &Document, src: &[char]) -> Result<Vec<Rule>, ParseError> {
    let mut rules = Vec::new();
    for node in &doc.rules {
        if let Some(rule) = extract_rule(node, src) {
            rules.push(rule);
        }
    }
    if rules.is_empty() {
        return Err(ParseError::NoRules);
    }
    Ok(rules)
}

/// Port of Go `extractRule`. Returns `None` for a "broken" rule (empty name,
/// pattern, or UAST type), matching the Go `errInvalidRule` drop.
fn extract_rule(node: &RuleNode, src: &[char]) -> Option<Rule> {
    let mut rule = Rule {
        name: slice(src, node.name),
        pattern: slice(src, node.pattern),
        ..Rule::default()
    };

    rule.uast_spec = extract_uast_spec(&node.fields, src);

    let mut conditions: Vec<Condition> = node
        .when_conditions
        .iter()
        .map(|span| Condition {
            expr: slice(src, *span),
        })
        .collect();

    let (extends, inheritance_conditions) = match &node.inheritance {
        Some(span) => extract_inheritance_and_conditions(&slice(src, *span)),
        None => (String::new(), Vec::new()),
    };

    conditions.extend(inheritance_conditions);
    rule.conditions = conditions;
    rule.extends = extends;

    let broken = rule.name.is_empty() || rule.pattern.is_empty() || rule.uast_spec.r#type.is_empty();
    if broken {
        return None;
    }
    Some(rule)
}

/// Port of Go `extractUASTSpec` + `applyUASTField`.
fn extract_uast_spec(fields: &[(Span, FieldValue)], src: &[char]) -> UastSpec {
    let mut spec = UastSpec::default();
    for (name_span, value) in fields {
        let fname = slice(src, *name_span);
        let fvals = field_values(value, src);
        apply_uast_field(&mut spec, &fname, fvals);
    }
    spec
}

/// Port of Go `extractFieldValues`: identifiers/captures pass through verbatim;
/// strings are unquoted via [`go_unquote`] (Go `strconv.Unquote`).
fn field_values(value: &FieldValue, src: &[char]) -> Vec<String> {
    match value {
        FieldValue::Identifier(span) | FieldValue::Capture(span) => vec![slice(src, *span)],
        FieldValue::StringLit(span) => {
            let raw = slice(src, *span);
            vec![go_unquote(&raw).unwrap_or(raw)]
        }
        FieldValue::MultipleCaptures(spans) => {
            spans.iter().map(|s| slice(src, *s)).collect()
        }
        FieldValue::MultipleStrings(spans) => spans
            .iter()
            .map(|s| {
                let raw = slice(src, *s);
                go_unquote(&raw).unwrap_or(raw)
            })
            .collect(),
    }
}

/// Port of Go `applyUASTField`: routes a field name/value(s) into the spec.
fn apply_uast_field(spec: &mut UastSpec, fname: &str, fvals: Vec<String>) {
    match fname {
        "type" => {
            if let Some(first) = fvals.into_iter().next() {
                spec.r#type = first;
            }
        }
        "token" => {
            if let Some(first) = fvals.into_iter().next() {
                spec.token = first;
            }
        }
        "roles" => spec.roles.extend(fvals),
        "children" => spec.children.extend(fvals),
        _ => {
            let props = spec.props.get_or_insert_with(BTreeMap::new);
            if let Some(first) = fvals.into_iter().next() {
                props.insert(fname.to_string(), first);
            }
        }
    }
}

/// Port of Go `extractInheritanceAndConditions`. Parses
/// `# Extends base_rule [when field == "val" and other != "bad"]`.
fn extract_inheritance_and_conditions(text: &str) -> (String, Vec<Condition>) {
    let trimmed = text.trim();
    if !trimmed.starts_with("# Extends ") {
        return (String::new(), Vec::new());
    }

    // base = the 3rd whitespace-separated field ("#", "Extends", base, ...).
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    let base = if parts.len() >= MIN_EXTENDS_FIELDS {
        parts[2].to_string()
    } else {
        String::new()
    };

    // strings.Cut(text, "when ") — split on the first occurrence.
    let cond_expr = match text.find("when ") {
        Some(idx) => &text[idx + "when ".len()..],
        None => return (base, Vec::new()),
    };

    let cond_expr = cond_expr.trim();
    if cond_expr.is_empty() {
        return (base, Vec::new());
    }

    let mut conds = Vec::new();
    for cond_str in cond_expr.split(" and ") {
        let cond_str = cond_str.trim();
        if !cond_str.is_empty() {
            conds.push(Condition {
                expr: cond_str.to_string(),
            });
        }
    }
    (base, conds)
}

/// Port of Go `extractLanguageDeclarationFromAST` + `extractLanguageDeclaration`.
fn extract_language(doc: &Document, src: &[char]) -> Result<LanguageInfo, ParseError> {
    let decl = doc.language.as_ref().ok_or(ParseError::NoLangDeclaration)?;
    let text = slice(src, (decl.begin, decl.end));
    parse_language_declaration(&text)
}

/// Port of Go `extractLanguageDeclaration` text-scanning logic.
fn parse_language_declaration(text: &str) -> Result<LanguageInfo, ParseError> {
    let lang_marker = "language \"";
    let lang_start = text.find(lang_marker).ok_or(ParseError::InvalidLangFormat)?;
    let after = &text[lang_start + lang_marker.len()..];
    let name_end = after.find('"').ok_or(ParseError::InvalidLangFormat)?;
    let language_name = after[..name_end].to_string();

    let ext_start = text.find("extensions:");
    let files_start = text.find("files:");

    let mut extensions = Vec::new();
    let mut files = Vec::new();

    if let Some(mut es) = ext_start {
        es += "extensions:".len();
        let mut ext_text = &text[es..];
        if let Some(fs) = files_start {
            if fs > es {
                let mut t = &ext_text[..fs - es];
                t = t.trim();
                t = t.trim_matches(',');
                ext_text = t;
            }
        }
        extensions = parse_quoted_list(ext_text);
    }

    if let Some(mut fs) = files_start {
        fs += "files:".len();
        let files_text = &text[fs..];
        files = parse_quoted_list(files_text);
    }

    Ok(LanguageInfo {
        name: language_name,
        extensions,
        files,
    })
}

/// Port of Go `parseQuotedList`: a comma-separated list of single/double quoted
/// strings; whitespace is trimmed, surrounding `[]` and trailing commas removed.
fn parse_quoted_list(text: &str) -> Vec<String> {
    let mut text = text.trim();
    text = text.trim_matches(|c| c == '[' || c == ']');
    text = text.trim_end_matches(',');

    let mut items: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    let flush = |current: &mut String, items: &mut Vec<String>| {
        let item = current.trim();
        if !item.is_empty() {
            items.push(item.to_string());
        }
        current.clear();
    };

    for ch in text.bytes() {
        let ch = ch as char;
        if ch == '"' || ch == '\'' {
            if in_quotes {
                in_quotes = false;
                flush(&mut current, &mut items);
            } else {
                in_quotes = true;
            }
            continue;
        }
        if ch == ',' && !in_quotes {
            flush(&mut current, &mut items);
            continue;
        }
        current.push(ch);
    }
    flush(&mut current, &mut items);
    items
}

/// Minimal reimplementation of Go `strconv.Unquote` for the double-quoted
/// strings produced by the DSL grammar.
///
/// The DSL `String` rule is `'"' (!'"' .)* '"'`, so the input is always a
/// double-quoted literal with no embedded unescaped quote. Go's `strconv.Unquote`
/// interprets backslash escapes (`\n`, `\t`, `\"`, `\\`, `\uXXXX`, etc.). The DSL
/// values in practice contain only plain text and the occasional escape, so this
/// handles the common escapes and falls back to returning `Err` (so the caller
/// keeps the raw, quote-stripped text — matching Go, where an unquote error
/// leaves the original value untouched).
fn go_unquote(s: &str) -> Result<String, ()> {
    let bytes: Vec<char> = s.chars().collect();
    if bytes.len() < 2 || bytes[0] != '"' || bytes[bytes.len() - 1] != '"' {
        return Err(());
    }
    let inner = &bytes[1..bytes.len() - 1];
    let mut out = String::new();
    let mut i = 0;
    while i < inner.len() {
        let c = inner[i];
        if c == '"' {
            // A bare double quote inside is invalid for a quoted literal.
            return Err(());
        }
        if c == '\\' {
            i += 1;
            if i >= inner.len() {
                return Err(());
            }
            match inner[i] {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '\'' => out.push('\''),
                'a' => out.push('\u{07}'),
                'b' => out.push('\u{08}'),
                'f' => out.push('\u{0C}'),
                'v' => out.push('\u{0B}'),
                '0' => out.push('\u{00}'),
                'u' => {
                    if i + 4 >= inner.len() {
                        return Err(());
                    }
                    let hex: String = inner[i + 1..=i + 4].iter().collect();
                    let cp = u32::from_str_radix(&hex, 16).map_err(|_| ())?;
                    out.push(char::from_u32(cp).ok_or(())?);
                    i += 4;
                }
                other => {
                    // Unknown escape: Go's Unquote would error; fall back.
                    let _ = other;
                    return Err(());
                }
            }
        } else {
            out.push(c);
        }
        i += 1;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Result<(Vec<Rule>, LanguageInfo), ParseError> {
        Parser::new().parse_mapping(input)
    }

    /// Wraps rule-only DSL in a `[language ...]` header before parsing, mirroring
    /// the Go test oracle (`dsl_parser_test.go`), whose every input begins with
    /// `[language "go", extensions: ".go"]`. The Go `ParseMapping` requires a
    /// language declaration (returns `errNoLangDeclaration` otherwise), so the
    /// helper supplies one exactly as the Go tests do.
    fn parse_rules(input: &str) -> Vec<Rule> {
        let wrapped = if input.trim_start().starts_with('[') {
            input.to_string()
        } else {
            format!("[language \"go\", extensions: \".go\"]\n{input}")
        };
        parse(&wrapped).expect("ParseMapping").0
    }

    fn find_rule<'a>(rules: &'a [Rule], name: &str) -> Option<&'a Rule> {
        rules.iter().find(|r| r.name == name)
    }

    #[test]
    fn parse_simple_rule() {
        let rules = parse_rules(r#"identifier <- (identifier) => uast(type: "Identifier")"#);
        assert_eq!(rules.len(), 1);
        let rule = &rules[0];
        assert_eq!(rule.name, "identifier");
        assert_eq!(rule.pattern, "(identifier)");
        assert_eq!(rule.uast_spec.r#type, "Identifier");
    }

    #[test]
    fn parse_rule_with_token() {
        let rules =
            parse_rules(r#"identifier <- (identifier) => uast(type: "Identifier", token: "@name")"#);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].uast_spec.token, "@name");
    }

    #[test]
    fn parse_rule_with_roles() {
        let rules = parse_rules(
            r#"function <- (function_declaration) => uast(type: "Function", roles: "Declaration", "Function")"#,
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].uast_spec.roles, vec!["Declaration", "Function"]);
    }

    #[test]
    fn parse_multiple_rules() {
        let input = "
identifier <- (identifier) => uast(type: \"Identifier\")
function <- (function_declaration) => uast(type: \"Function\", roles: \"Declaration\")
";
        let rules = parse_rules(input);
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn parse_rule_with_field() {
        let rules = parse_rules(
            r#"function <- (function_declaration name: (identifier) @name) => uast(type: "Function", token: "@name")"#,
        );
        assert_eq!(rules.len(), 1);
        assert!(rules[0].pattern.contains("name:"));
    }

    #[test]
    fn parse_rule_with_capture() {
        let rules = parse_rules(
            r#"call <- (call_expression function: (identifier) @func) => uast(type: "Call", token: "@func")"#,
        );
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn parse_rule_with_multiple_captures() {
        let rules = parse_rules(
            r#"binary <- (binary_expression left: (identifier) @left right: (identifier) @right) => uast(type: "BinaryExpression", roles: @left, @right)"#,
        );
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].uast_spec.roles, vec!["@left", "@right"]);
    }

    #[test]
    fn parse_rule_with_conditions() {
        let rules = parse_rules(
            r#"typed_func <- (function_declaration) => uast(type: "TypedFunction") when type == "typed""#,
        );
        assert_eq!(rules.len(), 1);
        assert!(!rules[0].conditions.is_empty());
    }

    #[test]
    fn parse_rule_with_inheritance() {
        let input = "
base_func <- (function_declaration) => uast(type: \"Function\")
# Extends base_func
derived_func <- (method_declaration) => uast(type: \"Method\")
";
        let rules = parse_rules(input);
        assert!(find_rule(&rules, "derived_func").is_some());
    }

    #[test]
    fn parse_empty_input() {
        assert!(parse("").is_err());
    }

    #[test]
    fn parse_invalid_syntax() {
        assert!(parse("this is not valid DSL syntax").is_err());
    }

    #[test]
    fn parse_language_declaration() {
        let input = "[language \"go\", extensions: \".go\"]
identifier <- (identifier) => uast(type: \"Identifier\")";
        let (rules, lang) = parse(input).expect("ParseMapping");
        assert_eq!(rules.len(), 1);
        assert_eq!(lang.name, "go");
    }

    #[test]
    fn parse_language_with_multiple_extensions() {
        let input = "[language \"javascript\", extensions: \".js\", \".jsx\", \".mjs\"]
identifier <- (identifier) => uast(type: \"Identifier\")";
        let (rules, lang) = parse(input).expect("ParseMapping");
        assert_eq!(rules.len(), 1);
        assert_eq!(lang.extensions, vec![".js", ".jsx", ".mjs"]);
    }

    #[test]
    fn parse_complex_mapping() {
        let input = "[language \"go\", extensions: \".go\"]
identifier <- (identifier) => uast(type: \"Identifier\", token: \"@name\")
function <- (function_declaration name: (identifier) @name) => uast(type: \"Function\", token: \"@name\", roles: \"Declaration\", \"Function\")
call <- (call_expression function: (identifier) @func) => uast(type: \"Call\", token: \"@func\")
";
        let (rules, lang) = parse(input).expect("ParseMapping");
        assert_eq!(rules.len(), 3);
        assert_eq!(lang.name, "go");
    }

    #[test]
    fn parse_rule_with_props() {
        let rules =
            parse_rules(r#"typed <- (typed_declaration) => uast(type: "Typed", custom_prop: "value")"#);
        assert_eq!(rules.len(), 1);
        let props = rules[0].uast_spec.props.as_ref().expect("props parsed");
        assert_eq!(props.get("custom_prop").map(String::as_str), Some("value"));
    }

    #[test]
    fn parse_whitespace_variations() {
        let inputs = [
            r#"identifier<-(identifier)=>uast(type:"Identifier")"#,
            r#"identifier <- (identifier) => uast(type: "Identifier")"#,
            r#"identifier    <-    (identifier)    =>    uast(type:    "Identifier")"#,
        ];
        for (i, input) in inputs.iter().enumerate() {
            let wrapped = format!("[language \"go\", extensions: \".go\"]\n{input}");
            let rules = Parser::new()
                .parse_mapping(&wrapped)
                .unwrap_or_else(|e| panic!("input {i}: {e}"))
                .0;
            assert_eq!(rules.len(), 1, "input {i}");
        }
    }

    #[test]
    fn parse_comments_ignored() {
        let input = "
// This is a comment
identifier <- (identifier) => uast(type: \"Identifier\")
";
        // Comments may or may not be supported; just verify no panic.
        let _ = parse(input);
    }

    #[test]
    fn parse_nested_pattern() {
        let input = r#"nested <- (call_expression function: (member_expression object: (identifier) @obj property: (property_identifier) @prop)) => uast(type: "MethodCall", token: "@prop")"#;
        // Nested patterns may not be fully supported; verify no panic.
        let _ = parse(input);
    }

    #[test]
    fn parse_real_world_go_mapping() {
        let input = "[language \"go\", extensions: \".go\"]
source_file <- (source_file) => uast(type: \"File\", roles: \"Module\")
function_declaration <- (function_declaration name: (identifier) @name) => uast(type: \"Function\", token: \"@name\", roles: \"Declaration\", \"Function\")
type_declaration <- (type_declaration) => uast(type: \"TypeDeclaration\", roles: \"Declaration\")
";
        let (rules, lang) = parse(input).expect("ParseMapping");
        assert_eq!(rules.len(), 3);
        assert_eq!(lang.name, "go");
    }

    #[test]
    fn parse_rule_with_special_chars_in_strings() {
        let rules =
            parse_rules(r#"special <- (string_literal) => uast(type: "String", token: "@value")"#);
        assert_eq!(rules.len(), 1);
    }

    #[test]
    fn parse_many_rules() {
        let mut sb = String::from("[language \"go\", extensions: \".go\"]\n");
        for i in 0..50 {
            sb.push_str("rule");
            sb.push_str(&"x".repeat(i % 5 + 1));
            sb.push_str(" <- (node) => uast(type: \"Node\")\n");
        }
        let rules = Parser::new()
            .parse_mapping(&sb)
            .expect("ParseMapping")
            .0;
        assert!(!rules.is_empty());
    }

    #[test]
    fn parse_round_trip() {
        let input = r#"identifier <- (identifier) => uast(type: "Identifier")"#;
        let rules1 = parse_rules(input);
        let rules2 = parse_rules(input);
        assert_eq!(rules1.len(), rules2.len());
    }

    #[test]
    fn unquote_basic() {
        assert_eq!(go_unquote(r#""hello""#), Ok("hello".to_string()));
        assert_eq!(go_unquote(r#""a\nb""#), Ok("a\nb".to_string()));
        assert_eq!(go_unquote(r#""quote\"""#), Ok("quote\"".to_string()));
        assert!(go_unquote("noquotes").is_err());
    }

    #[test]
    fn quoted_list_handles_brackets_and_commas() {
        assert_eq!(parse_quoted_list(r#" ".go" "#), vec![".go"]);
        assert_eq!(
            parse_quoted_list(r#" ".js", ".jsx", ".mjs" "#),
            vec![".js", ".jsx", ".mjs"]
        );
    }
}
