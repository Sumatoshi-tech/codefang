//! File classification by category.
//!
//! Files are classified into one of eight [`Category`] values using
//! enry-style language/vendor/documentation heuristics plus generated-path and
//! generated-content detection. Categories are checked in a fixed priority
//! order; the first match wins.
//!
//! # Backend parity
//!
//! The reference classifier delegates its predicates to the `enry` library.
//! The full predicate set (image/vendor/documentation/configuration/dotfile)
//! is not yet available in this tree, so this module routes every predicate
//! through the [`Enry`] and [`GeneratedDetector`] traits. The category *set*
//! and *priority order* are final; a faithful enry port is dropped in by
//! implementing those traits, with no change to the classifier itself. The
//! [`PlaceholderEnry`] backend reproduces only the NUL-byte binary heuristic,
//! classifying everything else as [`Category::Source`]. Tracked as a roadmap
//! item.

/// A file classification category.
///
/// The string values are part of the report contract (they appear verbatim in
/// machine report keys such as the composition breakdown).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    /// First-party project source code.
    Source,
    /// Third-party vendored dependencies.
    Vendor,
    /// Generated code (protobuf, mocks, deepcopy, etc.).
    Generated,
    /// Documentation (README, CHANGELOG, docs/).
    Documentation,
    /// Configuration files (YAML/JSON/TOML/...).
    Configuration,
    /// Image assets (PNG/JPG/GIF).
    Image,
    /// Dotfiles (.editorconfig, .bashrc, ...).
    DotFile,
    /// Files with binary content.
    Binary,
}

impl Category {
    /// Returns the lowercase string identifier used in report keys.
    ///
    /// ```
    /// use cf_file_history::Category;
    ///
    /// assert_eq!(Category::Source.as_str(), "source");
    /// assert_eq!(Category::DotFile.as_str(), "dotfile");
    /// ```
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Source => "source",
            Category::Vendor => "vendor",
            Category::Generated => "generated",
            Category::Documentation => "documentation",
            Category::Configuration => "configuration",
            Category::Image => "image",
            Category::DotFile => "dotfile",
            Category::Binary => "binary",
        }
    }
}

/// The canonical order of categories for display and charting:
/// source, documentation, configuration, vendor, generated, dotfile, image,
/// binary.
pub const ALL_CATEGORIES: [Category; 8] = [
    Category::Source,
    Category::Documentation,
    Category::Configuration,
    Category::Vendor,
    Category::Generated,
    Category::DotFile,
    Category::Image,
    Category::Binary,
];

/// Returns the canonical [`Category`] order (function form of [`ALL_CATEGORIES`]).
#[must_use]
pub fn all_categories() -> &'static [Category] {
    &ALL_CATEGORIES
}

/// Heuristic predicates supplied by an enry-equivalent backend.
///
/// These map one-to-one onto the predicates used by the reference classifier.
/// A faithful enry port implements this trait so [`Classifier`] reproduces the
/// contract classification byte-for-byte.
pub trait Enry {
    /// Whether `content` is binary.
    fn is_binary(&self, content: &[u8]) -> bool;
    /// Whether `path` is an image asset.
    fn is_image(&self, path: &str) -> bool;
    /// Whether `path` is vendored third-party code.
    fn is_vendor(&self, path: &str) -> bool;
    /// Whether `path` is documentation.
    fn is_documentation(&self, path: &str) -> bool;
    /// Whether `path` is configuration.
    fn is_configuration(&self, path: &str) -> bool;
    /// Whether `path` is a dotfile.
    fn is_dot_file(&self, path: &str) -> bool;
}

/// Generated-path / generated-content predicates.
pub trait GeneratedDetector {
    /// Whether the path denotes generated code.
    fn is_generated_path(&self, path: &str) -> bool;
    /// Whether the content carries a generated-code marker.
    fn is_generated_content(&self, content: &[u8]) -> bool;
}

/// Categorizes files using an [`Enry`] backend and a [`GeneratedDetector`].
///
/// Construct with [`Classifier::new`] for the default backends, or
/// [`Classifier::with_backends`] to inject a faithful enry/pathfilter port.
pub struct Classifier<E: Enry, G: GeneratedDetector> {
    enry: E,
    generated: G,
}

impl<E: Enry, G: GeneratedDetector> Classifier<E, G> {
    /// Creates a classifier from explicit backends.
    pub fn with_backends(enry: E, generated: G) -> Self {
        Self { enry, generated }
    }

    /// Returns the category for a file.
    ///
    /// `content` may be empty for path-only classification. Categories are
    /// checked in priority order (first match wins):
    ///
    /// Binary > Image > Vendor > Generated(path) > Generated(content) >
    /// Documentation > Configuration > `DotFile` > Source.
    pub fn classify(&self, file_path: &str, content: &[u8]) -> Category {
        if !content.is_empty() && self.enry.is_binary(content) {
            return Category::Binary;
        }
        if self.enry.is_image(file_path) {
            return Category::Image;
        }
        if self.enry.is_vendor(file_path) {
            return Category::Vendor;
        }
        if self.generated.is_generated_path(file_path) {
            return Category::Generated;
        }
        if !content.is_empty() && self.generated.is_generated_content(content) {
            return Category::Generated;
        }
        if self.enry.is_documentation(file_path) {
            return Category::Documentation;
        }
        if self.enry.is_configuration(file_path) {
            return Category::Configuration;
        }
        if self.enry.is_dot_file(file_path) {
            return Category::DotFile;
        }
        Category::Source
    }
}

/// Default enry backend pending the vendored data tables.
///
/// Reproduces only the heuristic available without enry's data tables: content
/// containing a NUL byte is binary. All path predicates return `false`, so
/// [`Classifier::new`] classifies every non-binary file as [`Category::Source`].
/// Replace by implementing [`Enry`] over the vendored enry tables.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlaceholderEnry;

impl Enry for PlaceholderEnry {
    fn is_binary(&self, content: &[u8]) -> bool {
        content.contains(&0)
    }
    fn is_image(&self, _path: &str) -> bool {
        false
    }
    fn is_vendor(&self, _path: &str) -> bool {
        false
    }
    fn is_documentation(&self, _path: &str) -> bool {
        false
    }
    fn is_configuration(&self, _path: &str) -> bool {
        false
    }
    fn is_dot_file(&self, _path: &str) -> bool {
        false
    }
}

/// Default generated-content detector (always `false`).
///
/// Replace by implementing [`GeneratedDetector`] over a pathfilter port.
#[derive(Debug, Default, Clone, Copy)]
pub struct PlaceholderGenerated;

impl GeneratedDetector for PlaceholderGenerated {
    fn is_generated_path(&self, _path: &str) -> bool {
        false
    }
    fn is_generated_content(&self, _content: &[u8]) -> bool {
        false
    }
}

impl Classifier<PlaceholderEnry, PlaceholderGenerated> {
    /// Creates a `Classifier` with the default backends.
    ///
    /// Pending the vendored enry data tables, this only distinguishes
    /// [`Category::Binary`] (NUL-containing content) from [`Category::Source`].
    ///
    /// ```
    /// use cf_file_history::{Category, Classifier};
    ///
    /// let c = Classifier::new();
    /// // NUL-containing content classifies as Binary, even for a source path.
    /// assert_eq!(c.classify("main.go", b"hello\x00world"), Category::Binary);
    /// // Plain text defaults to Source with the placeholder backend.
    /// assert_eq!(c.classify("main.go", b"package main"), Category::Source);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::with_backends(PlaceholderEnry, PlaceholderGenerated)
    }
}

impl Default for Classifier<PlaceholderEnry, PlaceholderGenerated> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Cases that require real enry/pathfilter heuristics are gated behind
    // #[ignore] until the data tables are vendored; the binary detection and
    // priority-scaffold cases run against the default backend.

    #[test]
    fn classify_binary_content() {
        // NUL-containing content -> Binary.
        let c = Classifier::new();
        let binary = b"hello\x00world";
        assert_eq!(c.classify("data.bin", binary), Category::Binary);
        // Binary takes priority over other categories.
        assert_eq!(c.classify("main.go", binary), Category::Binary);
    }

    #[test]
    fn classify_source_default() {
        // Plain source files (default => Source).
        let c = Classifier::new();
        assert_eq!(c.classify("main.go", b""), Category::Source);
        assert_eq!(c.classify("internal/server/handler.go", b""), Category::Source);
        assert_eq!(c.classify("src/index.ts", b""), Category::Source);
    }

    #[test]
    fn all_categories_contains_all() {
        let expected = [
            Category::Source,
            Category::Vendor,
            Category::Generated,
            Category::Documentation,
            Category::Configuration,
            Category::Image,
            Category::DotFile,
            Category::Binary,
        ];
        assert_eq!(ALL_CATEGORIES.len(), expected.len());
        for cat in ALL_CATEGORIES {
            assert!(expected.contains(&cat), "unexpected category {cat:?}");
        }
    }

    #[test]
    fn category_as_str_is_stable() {
        assert_eq!(Category::Source.as_str(), "source");
        assert_eq!(Category::Vendor.as_str(), "vendor");
        assert_eq!(Category::Generated.as_str(), "generated");
        assert_eq!(Category::Documentation.as_str(), "documentation");
        assert_eq!(Category::Configuration.as_str(), "configuration");
        assert_eq!(Category::Image.as_str(), "image");
        assert_eq!(Category::DotFile.as_str(), "dotfile");
        assert_eq!(Category::Binary.as_str(), "binary");
    }

    #[test]
    #[ignore = "requires vendored enry data tables (roadmap item)"]
    fn classify_vendor_documentation_image_dotfile() {
        // Covers the cases that need the real enry backend; build a classifier
        // with the real enry implementation here once available.
    }
}
