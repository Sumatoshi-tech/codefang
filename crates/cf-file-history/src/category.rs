//! The file categories of the composition breakdown.
//!
//! Files are classified by the shared `cf-composition` classifier (path and
//! content cascade: binary, image, vendor, generated, documentation,
//! configuration, dotfile, else source); this module holds the category set
//! the file-history report names.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_categories_lists_each_variant_once() {
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
        for cat in expected {
            assert_eq!(
                ALL_CATEGORIES.iter().filter(|c| **c == cat).count(),
                1,
                "{cat:?}"
            );
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
}
