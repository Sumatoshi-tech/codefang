//! Closed mapping-DSL vocabularies: [`UastType`], [`Role`], [`TokenSource`].
//!
//! Extracted from the complete `.uastmap` corpus (68 languages, 6,354 rules):
//! every `type:` / `roles:` / `token:` value that appears is representable by a
//! non-`Other` variant, and `as_str()` returns the exact string the DSL uses —
//! the byte-parity contract for [`crate::Rule`] conversion. The corpus-coverage
//! test in this module proves the vocabularies stay closed.

/// A UAST node type as written in a mapping rule's `type:` field.
///
/// The 62 named variants cover the entire corpus; [`UastType::Other`] exists so
/// the model stays total for hand-written future values (its use is expected to
/// be justified in review — the coverage test keeps generated code `Other`-free).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UastType {
    /// `"Annotation"`.
    Annotation,
    /// `"Assignment"`.
    Assignment,
    /// `"Attribute"`.
    Attribute,
    /// `"Await"`.
    Await,
    /// `"BinaryOp"`.
    BinaryOp,
    /// `"Block"`.
    Block,
    /// `"Break"`.
    Break,
    /// `"Call"`.
    Call,
    /// `"Case"`.
    Case,
    /// `"Cast"`.
    Cast,
    /// `"Catch"`.
    Catch,
    /// `"Class"`.
    Class,
    /// `"Comment"`.
    Comment,
    /// `"Comprehension"`.
    Comprehension,
    /// `"Continue"`.
    Continue,
    /// `"Decorator"`.
    Decorator,
    /// `"Defer"`.
    Defer,
    /// `"Dict"`.
    Dict,
    /// `"Enum"`.
    Enum,
    /// `"EnumMember"`.
    EnumMember,
    /// `"Export"`.
    Export,
    /// `"Field"`.
    Field,
    /// `"File"`.
    File,
    /// `"Finally"`.
    Finally,
    /// `"Function"`.
    Function,
    /// `"Generator"`.
    Generator,
    /// `"Getter"`.
    Getter,
    /// `"Identifier"`.
    Identifier,
    /// `"If"`.
    If,
    /// `"Import"`.
    Import,
    /// `"Index"`.
    Index,
    /// `"Interface"`.
    Interface,
    /// `"KeyValue"`.
    KeyValue,
    /// `"Lambda"`.
    Lambda,
    /// `"List"`.
    List,
    /// `"Literal"`.
    Literal,
    /// `"Loop"`.
    Loop,
    /// `"Match"`.
    Match,
    /// `"MemberAccess"`.
    MemberAccess,
    /// `"Method"`.
    Method,
    /// `"Module"`.
    Module,
    /// `"Namespace"`.
    Namespace,
    /// `"Package"`.
    Package,
    /// `"Parameter"`.
    Parameter,
    /// `"Pattern"`.
    Pattern,
    /// `"Property"`.
    Property,
    /// `"Return"`.
    Return,
    /// `"Set"`.
    Set,
    /// `"Setter"`.
    Setter,
    /// `"Slice"`.
    Slice,
    /// `"Spread"`.
    Spread,
    /// `"Struct"`.
    Struct,
    /// `"Switch"`.
    Switch,
    /// `"Synthetic"`.
    Synthetic,
    /// `"Throw"`.
    Throw,
    /// `"Try"`.
    Try,
    /// `"Tuple"`.
    Tuple,
    /// `"TypeAnnotation"`.
    TypeAnnotation,
    /// `"TypeDeclaration"`.
    TypeDeclaration,
    /// `"UnaryOp"`.
    UnaryOp,
    /// `"Variable"`.
    Variable,
    /// `"Yield"`.
    Yield,
    /// An out-of-vocabulary type (escape hatch; not produced by the corpus).
    Other(&'static str),
}

impl UastType {
    /// The exact `type:` string the DSL uses for this variant.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            UastType::Annotation => "Annotation",
            UastType::Assignment => "Assignment",
            UastType::Attribute => "Attribute",
            UastType::Await => "Await",
            UastType::BinaryOp => "BinaryOp",
            UastType::Block => "Block",
            UastType::Break => "Break",
            UastType::Call => "Call",
            UastType::Case => "Case",
            UastType::Cast => "Cast",
            UastType::Catch => "Catch",
            UastType::Class => "Class",
            UastType::Comment => "Comment",
            UastType::Comprehension => "Comprehension",
            UastType::Continue => "Continue",
            UastType::Decorator => "Decorator",
            UastType::Defer => "Defer",
            UastType::Dict => "Dict",
            UastType::Enum => "Enum",
            UastType::EnumMember => "EnumMember",
            UastType::Export => "Export",
            UastType::Field => "Field",
            UastType::File => "File",
            UastType::Finally => "Finally",
            UastType::Function => "Function",
            UastType::Generator => "Generator",
            UastType::Getter => "Getter",
            UastType::Identifier => "Identifier",
            UastType::If => "If",
            UastType::Import => "Import",
            UastType::Index => "Index",
            UastType::Interface => "Interface",
            UastType::KeyValue => "KeyValue",
            UastType::Lambda => "Lambda",
            UastType::List => "List",
            UastType::Literal => "Literal",
            UastType::Loop => "Loop",
            UastType::Match => "Match",
            UastType::MemberAccess => "MemberAccess",
            UastType::Method => "Method",
            UastType::Module => "Module",
            UastType::Namespace => "Namespace",
            UastType::Package => "Package",
            UastType::Parameter => "Parameter",
            UastType::Pattern => "Pattern",
            UastType::Property => "Property",
            UastType::Return => "Return",
            UastType::Set => "Set",
            UastType::Setter => "Setter",
            UastType::Slice => "Slice",
            UastType::Spread => "Spread",
            UastType::Struct => "Struct",
            UastType::Switch => "Switch",
            UastType::Synthetic => "Synthetic",
            UastType::Throw => "Throw",
            UastType::Try => "Try",
            UastType::Tuple => "Tuple",
            UastType::TypeAnnotation => "TypeAnnotation",
            UastType::TypeDeclaration => "TypeDeclaration",
            UastType::UnaryOp => "UnaryOp",
            UastType::Variable => "Variable",
            UastType::Yield => "Yield",
            UastType::Other(s) => s,
        }
    }

    /// Parses a DSL `type:` string into its named variant ([`None`] when the
    /// value is out of vocabulary — the caller decides whether that is an error
    /// or an [`UastType::Other`]).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Annotation" => Some(UastType::Annotation),
            "Assignment" => Some(UastType::Assignment),
            "Attribute" => Some(UastType::Attribute),
            "Await" => Some(UastType::Await),
            "BinaryOp" => Some(UastType::BinaryOp),
            "Block" => Some(UastType::Block),
            "Break" => Some(UastType::Break),
            "Call" => Some(UastType::Call),
            "Case" => Some(UastType::Case),
            "Cast" => Some(UastType::Cast),
            "Catch" => Some(UastType::Catch),
            "Class" => Some(UastType::Class),
            "Comment" => Some(UastType::Comment),
            "Comprehension" => Some(UastType::Comprehension),
            "Continue" => Some(UastType::Continue),
            "Decorator" => Some(UastType::Decorator),
            "Defer" => Some(UastType::Defer),
            "Dict" => Some(UastType::Dict),
            "Enum" => Some(UastType::Enum),
            "EnumMember" => Some(UastType::EnumMember),
            "Export" => Some(UastType::Export),
            "Field" => Some(UastType::Field),
            "File" => Some(UastType::File),
            "Finally" => Some(UastType::Finally),
            "Function" => Some(UastType::Function),
            "Generator" => Some(UastType::Generator),
            "Getter" => Some(UastType::Getter),
            "Identifier" => Some(UastType::Identifier),
            "If" => Some(UastType::If),
            "Import" => Some(UastType::Import),
            "Index" => Some(UastType::Index),
            "Interface" => Some(UastType::Interface),
            "KeyValue" => Some(UastType::KeyValue),
            "Lambda" => Some(UastType::Lambda),
            "List" => Some(UastType::List),
            "Literal" => Some(UastType::Literal),
            "Loop" => Some(UastType::Loop),
            "Match" => Some(UastType::Match),
            "MemberAccess" => Some(UastType::MemberAccess),
            "Method" => Some(UastType::Method),
            "Module" => Some(UastType::Module),
            "Namespace" => Some(UastType::Namespace),
            "Package" => Some(UastType::Package),
            "Parameter" => Some(UastType::Parameter),
            "Pattern" => Some(UastType::Pattern),
            "Property" => Some(UastType::Property),
            "Return" => Some(UastType::Return),
            "Set" => Some(UastType::Set),
            "Setter" => Some(UastType::Setter),
            "Slice" => Some(UastType::Slice),
            "Spread" => Some(UastType::Spread),
            "Struct" => Some(UastType::Struct),
            "Switch" => Some(UastType::Switch),
            "Synthetic" => Some(UastType::Synthetic),
            "Throw" => Some(UastType::Throw),
            "Try" => Some(UastType::Try),
            "Tuple" => Some(UastType::Tuple),
            "TypeAnnotation" => Some(UastType::TypeAnnotation),
            "TypeDeclaration" => Some(UastType::TypeDeclaration),
            "UnaryOp" => Some(UastType::UnaryOp),
            "Variable" => Some(UastType::Variable),
            "Yield" => Some(UastType::Yield),
            _ => None,
        }
    }
}

/// A UAST role as written in a mapping rule's `roles:` list.
///
/// The 61 named variants cover the entire corpus; see [`UastType::Other`] for
/// the escape-hatch contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// `"Annotation"`.
    Annotation,
    /// `"Argument"`.
    Argument,
    /// `"Assignment"`.
    Assignment,
    /// `"Attribute"`.
    Attribute,
    /// `"Await"`.
    Await,
    /// `"Body"`.
    Body,
    /// `"Branch"`.
    Branch,
    /// `"Break"`.
    Break,
    /// `"Call"`.
    Call,
    /// `"Case"`.
    Case,
    /// `"Cast"`.
    Cast,
    /// `"Catch"`.
    Catch,
    /// `"Class"`.
    Class,
    /// `"Comment"`.
    Comment,
    /// `"Condition"`.
    Condition,
    /// `"Constant"`.
    Constant,
    /// `"Continue"`.
    Continue,
    /// `"Declaration"`.
    Declaration,
    /// `"Defer"`.
    Defer,
    /// `"Doc"`.
    Doc,
    /// `"Else"`.
    Else,
    /// `"Enum"`.
    Enum,
    /// `"EnumMember"`.
    EnumMember,
    /// `"Export"`.
    Export,
    /// `"Exported"`.
    Exported,
    /// `"Finally"`.
    Finally,
    /// `"For"`.
    For,
    /// `"Function"`.
    Function,
    /// `"Generator"`.
    Generator,
    /// `"Getter"`.
    Getter,
    /// `"Import"`.
    Import,
    /// `"Index"`.
    Index,
    /// `"Interface"`.
    Interface,
    /// `"Key"`.
    Key,
    /// `"Lambda"`.
    Lambda,
    /// `"Literal"`.
    Literal,
    /// `"Loop"`.
    Loop,
    /// `"Match"`.
    Match,
    /// `"Member"`.
    Member,
    /// `"Method"`.
    Method,
    /// `"Module"`.
    Module,
    /// `"Mutable"`.
    Mutable,
    /// `"Name"`.
    Name,
    /// `"Operator"`.
    Operator,
    /// `"Parameter"`.
    Parameter,
    /// `"Pattern"`.
    Pattern,
    /// `"Private"`.
    Private,
    /// `"Public"`.
    Public,
    /// `"Reference"`.
    Reference,
    /// `"Return"`.
    Return,
    /// `"Setter"`.
    Setter,
    /// `"Spread"`.
    Spread,
    /// `"Static"`.
    Static,
    /// `"Struct"`.
    Struct,
    /// `"Switch"`.
    Switch,
    /// `"Throw"`.
    Throw,
    /// `"Try"`.
    Try,
    /// `"Type"`.
    Type,
    /// `"Value"`.
    Value,
    /// `"Variable"`.
    Variable,
    /// `"Yield"`.
    Yield,
    /// An out-of-vocabulary role (escape hatch; not produced by the corpus).
    Other(&'static str),
}

impl Role {
    /// The exact `roles:` string the DSL uses for this variant.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Role::Annotation => "Annotation",
            Role::Argument => "Argument",
            Role::Assignment => "Assignment",
            Role::Attribute => "Attribute",
            Role::Await => "Await",
            Role::Body => "Body",
            Role::Branch => "Branch",
            Role::Break => "Break",
            Role::Call => "Call",
            Role::Case => "Case",
            Role::Cast => "Cast",
            Role::Catch => "Catch",
            Role::Class => "Class",
            Role::Comment => "Comment",
            Role::Condition => "Condition",
            Role::Constant => "Constant",
            Role::Continue => "Continue",
            Role::Declaration => "Declaration",
            Role::Defer => "Defer",
            Role::Doc => "Doc",
            Role::Else => "Else",
            Role::Enum => "Enum",
            Role::EnumMember => "EnumMember",
            Role::Export => "Export",
            Role::Exported => "Exported",
            Role::Finally => "Finally",
            Role::For => "For",
            Role::Function => "Function",
            Role::Generator => "Generator",
            Role::Getter => "Getter",
            Role::Import => "Import",
            Role::Index => "Index",
            Role::Interface => "Interface",
            Role::Key => "Key",
            Role::Lambda => "Lambda",
            Role::Literal => "Literal",
            Role::Loop => "Loop",
            Role::Match => "Match",
            Role::Member => "Member",
            Role::Method => "Method",
            Role::Module => "Module",
            Role::Mutable => "Mutable",
            Role::Name => "Name",
            Role::Operator => "Operator",
            Role::Parameter => "Parameter",
            Role::Pattern => "Pattern",
            Role::Private => "Private",
            Role::Public => "Public",
            Role::Reference => "Reference",
            Role::Return => "Return",
            Role::Setter => "Setter",
            Role::Spread => "Spread",
            Role::Static => "Static",
            Role::Struct => "Struct",
            Role::Switch => "Switch",
            Role::Throw => "Throw",
            Role::Try => "Try",
            Role::Type => "Type",
            Role::Value => "Value",
            Role::Variable => "Variable",
            Role::Yield => "Yield",
            Role::Other(s) => s,
        }
    }

    /// Parses a DSL role string into its named variant ([`None`] when out of
    /// vocabulary).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Annotation" => Some(Role::Annotation),
            "Argument" => Some(Role::Argument),
            "Assignment" => Some(Role::Assignment),
            "Attribute" => Some(Role::Attribute),
            "Await" => Some(Role::Await),
            "Body" => Some(Role::Body),
            "Branch" => Some(Role::Branch),
            "Break" => Some(Role::Break),
            "Call" => Some(Role::Call),
            "Case" => Some(Role::Case),
            "Cast" => Some(Role::Cast),
            "Catch" => Some(Role::Catch),
            "Class" => Some(Role::Class),
            "Comment" => Some(Role::Comment),
            "Condition" => Some(Role::Condition),
            "Constant" => Some(Role::Constant),
            "Continue" => Some(Role::Continue),
            "Declaration" => Some(Role::Declaration),
            "Defer" => Some(Role::Defer),
            "Doc" => Some(Role::Doc),
            "Else" => Some(Role::Else),
            "Enum" => Some(Role::Enum),
            "EnumMember" => Some(Role::EnumMember),
            "Export" => Some(Role::Export),
            "Exported" => Some(Role::Exported),
            "Finally" => Some(Role::Finally),
            "For" => Some(Role::For),
            "Function" => Some(Role::Function),
            "Generator" => Some(Role::Generator),
            "Getter" => Some(Role::Getter),
            "Import" => Some(Role::Import),
            "Index" => Some(Role::Index),
            "Interface" => Some(Role::Interface),
            "Key" => Some(Role::Key),
            "Lambda" => Some(Role::Lambda),
            "Literal" => Some(Role::Literal),
            "Loop" => Some(Role::Loop),
            "Match" => Some(Role::Match),
            "Member" => Some(Role::Member),
            "Method" => Some(Role::Method),
            "Module" => Some(Role::Module),
            "Mutable" => Some(Role::Mutable),
            "Name" => Some(Role::Name),
            "Operator" => Some(Role::Operator),
            "Parameter" => Some(Role::Parameter),
            "Pattern" => Some(Role::Pattern),
            "Private" => Some(Role::Private),
            "Public" => Some(Role::Public),
            "Reference" => Some(Role::Reference),
            "Return" => Some(Role::Return),
            "Setter" => Some(Role::Setter),
            "Spread" => Some(Role::Spread),
            "Static" => Some(Role::Static),
            "Struct" => Some(Role::Struct),
            "Switch" => Some(Role::Switch),
            "Throw" => Some(Role::Throw),
            "Try" => Some(Role::Try),
            "Type" => Some(Role::Type),
            "Value" => Some(Role::Value),
            "Variable" => Some(Role::Variable),
            "Yield" => Some(Role::Yield),
            _ => None,
        }
    }
}

/// A mapping rule's `token:` source, mirroring the string forms the DSL parser
/// stores in `Rule.uast_spec.token`:
///
/// * absent          → `TokenSource::None`        → `""`
/// * `token: "self"` → `TokenSource::SelfText`    → `"self"`
/// * `token: "child:identifier"` → `TokenSource::Child("identifier")`
/// * `token: "@name"`            → `TokenSource::Capture("name")`
///
/// The corpus uses only the first three forms; `Capture` mirrors the parser's
/// capability so the model stays total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource<'a> {
    /// No token (`""`).
    None,
    /// The node's own source text (`"self"`).
    SelfText,
    /// The text of the first child of the given type (`"child:<type>"`).
    Child(&'a str),
    /// The text of a query capture (`"@<name>"`).
    Capture(&'a str),
}

impl<'a> TokenSource<'a> {
    /// The exact `token:` string the DSL parser stores for this source.
    #[must_use]
    pub fn token_string(&self) -> String {
        match self {
            TokenSource::None => String::new(),
            TokenSource::SelfText => "self".to_string(),
            TokenSource::Child(t) => format!("child:{t}"),
            TokenSource::Capture(c) => format!("@{c}"),
        }
    }

    /// Parses a stored `token:` string back into a source. Returns [`None`]
    /// for a raw literal outside the recognized forms (the DSL parser would
    /// store such a string verbatim, but the corpus contains none — the
    /// coverage test asserts that).
    #[must_use]
    pub fn parse(s: &'a str) -> Option<Self> {
        if s.is_empty() {
            Some(TokenSource::None)
        } else if s == "self" {
            Some(TokenSource::SelfText)
        } else if let Some(rest) = s.strip_prefix("child:") {
            Some(TokenSource::Child(rest))
        } else if let Some(rest) = s.strip_prefix('@') {
            Some(TokenSource::Capture(rest))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_str_parse_round_trip() {
        // Spot-check both vocabularies; the corpus-coverage test below is the
        // exhaustive proof.
        for (t, s) in [(UastType::Assignment, "Assignment"), (UastType::BinaryOp, "BinaryOp"), (UastType::Synthetic, "Synthetic")] {
            assert_eq!(t.as_str(), s);
            assert_eq!(UastType::parse(s), Some(t));
        }
        for (r, s) in [(Role::Operator, "Operator"), (Role::EnumMember, "EnumMember")] {
            assert_eq!(r.as_str(), s);
            assert_eq!(Role::parse(s), Some(r));
        }
        assert_eq!(UastType::parse("Bogus"), None);
        assert_eq!(UastType::Other("Bogus").as_str(), "Bogus");
    }

    #[test]
    fn token_source_forms_round_trip() {
        for s in ["", "self", "child:identifier", "@name"] {
            let t = TokenSource::parse(s).expect("recognized form");
            assert_eq!(t.token_string(), s);
        }
        assert_eq!(TokenSource::parse("self"), Some(TokenSource::SelfText));
        assert_eq!(TokenSource::parse("child:identifier"), Some(TokenSource::Child("identifier")));
        assert_eq!(TokenSource::parse("@x"), Some(TokenSource::Capture("x")));
        assert_eq!(TokenSource::parse("raw-literal"), None);
    }

    /// The exhaustive closed-vocabulary proof: every `type:` / role / `token:`
    /// value in all 68 embedded `.uastmap` files resolves to a non-`Other`
    /// variant (or recognized token form) and round-trips byte-exactly.
    #[test]
    fn corpus_vocabulary_is_closed() {
        let parser = crate::Parser::new();
        let mut checked_rules = 0usize;
        for (&lang, &content) in cf_uast_uastmaps::embedded_mappings() {
            let (rules, _info) = parser
                .parse_mapping(content)
                .unwrap_or_else(|e| panic!("{lang}: parse failed: {e}"));
            for rule in &rules {
                checked_rules += 1;
                let spec = &rule.uast_spec;
                if !spec.r#type.is_empty() {
                    let t = UastType::parse(&spec.r#type).unwrap_or_else(|| {
                        panic!("{lang}/{}: out-of-vocabulary type {:?}", rule.name, spec.r#type)
                    });
                    assert_eq!(t.as_str(), spec.r#type, "{lang}/{}", rule.name);
                }
                for role in &spec.roles {
                    let r = Role::parse(role).unwrap_or_else(|| {
                        panic!("{lang}/{}: out-of-vocabulary role {role:?}", rule.name)
                    });
                    assert_eq!(r.as_str(), *role, "{lang}/{}", rule.name);
                }
                let tok = TokenSource::parse(&spec.token).unwrap_or_else(|| {
                    panic!("{lang}/{}: unrecognized token form {:?}", rule.name, spec.token)
                });
                assert_eq!(tok.token_string(), spec.token, "{lang}/{}", rule.name);
            }
        }
        // 6,354 rules at extraction time; assert the corpus did not silently shrink.
        assert!(checked_rules >= 6_000, "only {checked_rules} rules checked");
    }
}
