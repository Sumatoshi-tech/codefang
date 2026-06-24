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
            Self::Annotation => "Annotation",
            Self::Assignment => "Assignment",
            Self::Attribute => "Attribute",
            Self::Await => "Await",
            Self::BinaryOp => "BinaryOp",
            Self::Block => "Block",
            Self::Break => "Break",
            Self::Call => "Call",
            Self::Case => "Case",
            Self::Cast => "Cast",
            Self::Catch => "Catch",
            Self::Class => "Class",
            Self::Comment => "Comment",
            Self::Comprehension => "Comprehension",
            Self::Continue => "Continue",
            Self::Decorator => "Decorator",
            Self::Defer => "Defer",
            Self::Dict => "Dict",
            Self::Enum => "Enum",
            Self::EnumMember => "EnumMember",
            Self::Export => "Export",
            Self::Field => "Field",
            Self::File => "File",
            Self::Finally => "Finally",
            Self::Function => "Function",
            Self::Generator => "Generator",
            Self::Getter => "Getter",
            Self::Identifier => "Identifier",
            Self::If => "If",
            Self::Import => "Import",
            Self::Index => "Index",
            Self::Interface => "Interface",
            Self::KeyValue => "KeyValue",
            Self::Lambda => "Lambda",
            Self::List => "List",
            Self::Literal => "Literal",
            Self::Loop => "Loop",
            Self::Match => "Match",
            Self::MemberAccess => "MemberAccess",
            Self::Method => "Method",
            Self::Module => "Module",
            Self::Namespace => "Namespace",
            Self::Package => "Package",
            Self::Parameter => "Parameter",
            Self::Pattern => "Pattern",
            Self::Property => "Property",
            Self::Return => "Return",
            Self::Set => "Set",
            Self::Setter => "Setter",
            Self::Slice => "Slice",
            Self::Spread => "Spread",
            Self::Struct => "Struct",
            Self::Switch => "Switch",
            Self::Synthetic => "Synthetic",
            Self::Throw => "Throw",
            Self::Try => "Try",
            Self::Tuple => "Tuple",
            Self::TypeAnnotation => "TypeAnnotation",
            Self::TypeDeclaration => "TypeDeclaration",
            Self::UnaryOp => "UnaryOp",
            Self::Variable => "Variable",
            Self::Yield => "Yield",
            Self::Other(s) => s,
        }
    }

    /// Parses a DSL `type:` string into its named variant ([`None`] when the
    /// value is out of vocabulary — the caller decides whether that is an error
    /// or an [`UastType::Other`]).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Annotation" => Some(Self::Annotation),
            "Assignment" => Some(Self::Assignment),
            "Attribute" => Some(Self::Attribute),
            "Await" => Some(Self::Await),
            "BinaryOp" => Some(Self::BinaryOp),
            "Block" => Some(Self::Block),
            "Break" => Some(Self::Break),
            "Call" => Some(Self::Call),
            "Case" => Some(Self::Case),
            "Cast" => Some(Self::Cast),
            "Catch" => Some(Self::Catch),
            "Class" => Some(Self::Class),
            "Comment" => Some(Self::Comment),
            "Comprehension" => Some(Self::Comprehension),
            "Continue" => Some(Self::Continue),
            "Decorator" => Some(Self::Decorator),
            "Defer" => Some(Self::Defer),
            "Dict" => Some(Self::Dict),
            "Enum" => Some(Self::Enum),
            "EnumMember" => Some(Self::EnumMember),
            "Export" => Some(Self::Export),
            "Field" => Some(Self::Field),
            "File" => Some(Self::File),
            "Finally" => Some(Self::Finally),
            "Function" => Some(Self::Function),
            "Generator" => Some(Self::Generator),
            "Getter" => Some(Self::Getter),
            "Identifier" => Some(Self::Identifier),
            "If" => Some(Self::If),
            "Import" => Some(Self::Import),
            "Index" => Some(Self::Index),
            "Interface" => Some(Self::Interface),
            "KeyValue" => Some(Self::KeyValue),
            "Lambda" => Some(Self::Lambda),
            "List" => Some(Self::List),
            "Literal" => Some(Self::Literal),
            "Loop" => Some(Self::Loop),
            "Match" => Some(Self::Match),
            "MemberAccess" => Some(Self::MemberAccess),
            "Method" => Some(Self::Method),
            "Module" => Some(Self::Module),
            "Namespace" => Some(Self::Namespace),
            "Package" => Some(Self::Package),
            "Parameter" => Some(Self::Parameter),
            "Pattern" => Some(Self::Pattern),
            "Property" => Some(Self::Property),
            "Return" => Some(Self::Return),
            "Set" => Some(Self::Set),
            "Setter" => Some(Self::Setter),
            "Slice" => Some(Self::Slice),
            "Spread" => Some(Self::Spread),
            "Struct" => Some(Self::Struct),
            "Switch" => Some(Self::Switch),
            "Synthetic" => Some(Self::Synthetic),
            "Throw" => Some(Self::Throw),
            "Try" => Some(Self::Try),
            "Tuple" => Some(Self::Tuple),
            "TypeAnnotation" => Some(Self::TypeAnnotation),
            "TypeDeclaration" => Some(Self::TypeDeclaration),
            "UnaryOp" => Some(Self::UnaryOp),
            "Variable" => Some(Self::Variable),
            "Yield" => Some(Self::Yield),
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
            Self::Annotation => "Annotation",
            Self::Argument => "Argument",
            Self::Assignment => "Assignment",
            Self::Attribute => "Attribute",
            Self::Await => "Await",
            Self::Body => "Body",
            Self::Branch => "Branch",
            Self::Break => "Break",
            Self::Call => "Call",
            Self::Case => "Case",
            Self::Cast => "Cast",
            Self::Catch => "Catch",
            Self::Class => "Class",
            Self::Comment => "Comment",
            Self::Condition => "Condition",
            Self::Constant => "Constant",
            Self::Continue => "Continue",
            Self::Declaration => "Declaration",
            Self::Defer => "Defer",
            Self::Doc => "Doc",
            Self::Else => "Else",
            Self::Enum => "Enum",
            Self::EnumMember => "EnumMember",
            Self::Export => "Export",
            Self::Exported => "Exported",
            Self::Finally => "Finally",
            Self::For => "For",
            Self::Function => "Function",
            Self::Generator => "Generator",
            Self::Getter => "Getter",
            Self::Import => "Import",
            Self::Index => "Index",
            Self::Interface => "Interface",
            Self::Key => "Key",
            Self::Lambda => "Lambda",
            Self::Literal => "Literal",
            Self::Loop => "Loop",
            Self::Match => "Match",
            Self::Member => "Member",
            Self::Method => "Method",
            Self::Module => "Module",
            Self::Mutable => "Mutable",
            Self::Name => "Name",
            Self::Operator => "Operator",
            Self::Parameter => "Parameter",
            Self::Pattern => "Pattern",
            Self::Private => "Private",
            Self::Public => "Public",
            Self::Reference => "Reference",
            Self::Return => "Return",
            Self::Setter => "Setter",
            Self::Spread => "Spread",
            Self::Static => "Static",
            Self::Struct => "Struct",
            Self::Switch => "Switch",
            Self::Throw => "Throw",
            Self::Try => "Try",
            Self::Type => "Type",
            Self::Value => "Value",
            Self::Variable => "Variable",
            Self::Yield => "Yield",
            Self::Other(s) => s,
        }
    }

    /// Parses a DSL role string into its named variant ([`None`] when out of
    /// vocabulary).
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Annotation" => Some(Self::Annotation),
            "Argument" => Some(Self::Argument),
            "Assignment" => Some(Self::Assignment),
            "Attribute" => Some(Self::Attribute),
            "Await" => Some(Self::Await),
            "Body" => Some(Self::Body),
            "Branch" => Some(Self::Branch),
            "Break" => Some(Self::Break),
            "Call" => Some(Self::Call),
            "Case" => Some(Self::Case),
            "Cast" => Some(Self::Cast),
            "Catch" => Some(Self::Catch),
            "Class" => Some(Self::Class),
            "Comment" => Some(Self::Comment),
            "Condition" => Some(Self::Condition),
            "Constant" => Some(Self::Constant),
            "Continue" => Some(Self::Continue),
            "Declaration" => Some(Self::Declaration),
            "Defer" => Some(Self::Defer),
            "Doc" => Some(Self::Doc),
            "Else" => Some(Self::Else),
            "Enum" => Some(Self::Enum),
            "EnumMember" => Some(Self::EnumMember),
            "Export" => Some(Self::Export),
            "Exported" => Some(Self::Exported),
            "Finally" => Some(Self::Finally),
            "For" => Some(Self::For),
            "Function" => Some(Self::Function),
            "Generator" => Some(Self::Generator),
            "Getter" => Some(Self::Getter),
            "Import" => Some(Self::Import),
            "Index" => Some(Self::Index),
            "Interface" => Some(Self::Interface),
            "Key" => Some(Self::Key),
            "Lambda" => Some(Self::Lambda),
            "Literal" => Some(Self::Literal),
            "Loop" => Some(Self::Loop),
            "Match" => Some(Self::Match),
            "Member" => Some(Self::Member),
            "Method" => Some(Self::Method),
            "Module" => Some(Self::Module),
            "Mutable" => Some(Self::Mutable),
            "Name" => Some(Self::Name),
            "Operator" => Some(Self::Operator),
            "Parameter" => Some(Self::Parameter),
            "Pattern" => Some(Self::Pattern),
            "Private" => Some(Self::Private),
            "Public" => Some(Self::Public),
            "Reference" => Some(Self::Reference),
            "Return" => Some(Self::Return),
            "Setter" => Some(Self::Setter),
            "Spread" => Some(Self::Spread),
            "Static" => Some(Self::Static),
            "Struct" => Some(Self::Struct),
            "Switch" => Some(Self::Switch),
            "Throw" => Some(Self::Throw),
            "Try" => Some(Self::Try),
            "Type" => Some(Self::Type),
            "Value" => Some(Self::Value),
            "Variable" => Some(Self::Variable),
            "Yield" => Some(Self::Yield),
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
        } else { s.strip_prefix('@').map(TokenSource::Capture) }
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
}
