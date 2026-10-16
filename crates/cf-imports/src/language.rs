//! The languages the import extractor and the stdlib classifier know by name.
//!
//! Names are the `cf_uast::Parser::get_language` values (the mapping
//! registry's language names). Anything else maps to [`Language::Other`],
//! which selects the generic extraction rule and the union stdlib table.

/// A language with dedicated import semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// Go (`go`).
    Go,
    /// Python (`python`).
    Python,
    /// JavaScript, TypeScript and TSX (`javascript`, `typescript`, `tsx`):
    /// one module system, one builtin table.
    JavaScript,
    /// Rust (`rust`).
    Rust,
    /// Java (`java`).
    Java,
    /// C (`c`).
    C,
    /// C++ (`cpp`).
    Cpp,
    /// C# (`csharp`).
    CSharp,
    /// PHP (`php`).
    Php,
    /// Ruby (`ruby`).
    Ruby,
    /// Kotlin (`kotlin`).
    Kotlin,
    /// Any other (or unknown) language.
    Other,
}

/// Parser language name for Go.
pub const NAME_GO: &str = "go";
/// Parser language name for Python.
pub const NAME_PYTHON: &str = "python";
/// Parser language name for JavaScript.
pub const NAME_JAVASCRIPT: &str = "javascript";
/// Parser language name for TypeScript.
pub const NAME_TYPESCRIPT: &str = "typescript";
/// Parser language name for TSX.
pub const NAME_TSX: &str = "tsx";
/// Parser language name for Rust.
pub const NAME_RUST: &str = "rust";
/// Parser language name for Java.
pub const NAME_JAVA: &str = "java";
/// Parser language name for C.
pub const NAME_C: &str = "c";
/// Parser language name for C++.
pub const NAME_CPP: &str = "cpp";
/// Mapping-registry language name for C#.
pub const NAME_CSHARP: &str = "csharp";
/// Grammar-stem language name for C# (what `cf_uast::Parser::get_language`
/// reports for `.cs`).
pub const NAME_C_SHARP: &str = "c_sharp";
/// Parser language name for PHP.
pub const NAME_PHP: &str = "php";
/// Parser language name for Ruby.
pub const NAME_RUBY: &str = "ruby";
/// Parser language name for Kotlin.
pub const NAME_KOTLIN: &str = "kotlin";

impl Language {
    /// Maps a parser language name to its [`Language`]; unknown or empty names
    /// are [`Language::Other`].
    ///
    /// ```
    /// use cf_imports::Language;
    ///
    /// assert_eq!(Language::from_name("go"), Language::Go);
    /// assert_eq!(Language::from_name("tsx"), Language::JavaScript);
    /// assert_eq!(Language::from_name("c_sharp"), Language::CSharp);
    /// assert_eq!(Language::from_name(""), Language::Other);
    /// ```
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            NAME_GO => Self::Go,
            NAME_PYTHON => Self::Python,
            NAME_JAVASCRIPT | NAME_TYPESCRIPT | NAME_TSX => Self::JavaScript,
            NAME_RUST => Self::Rust,
            NAME_JAVA => Self::Java,
            NAME_C => Self::C,
            NAME_CPP => Self::Cpp,
            NAME_CSHARP | NAME_C_SHARP => Self::CSharp,
            NAME_PHP => Self::Php,
            NAME_RUBY => Self::Ruby,
            NAME_KOTLIN => Self::Kotlin,
            _ => Self::Other,
        }
    }
}
