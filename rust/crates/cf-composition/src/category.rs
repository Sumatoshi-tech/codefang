//! File classification categories.
//!
//! Ported from Go `internal/analyzers/file_history/classify.go` (the `Category`
//! constants and `AllCategories`). The Rust `cf-file-history` crate is still a
//! scaffold, so the minimal surface that `cf-composition` depends on is
//! reproduced here verbatim (DESIGN rule 5 — define the minimal interface for a
//! not-yet-ported transitive dependency). When `cf-file-history` is fully ported
//! these should move there and be re-exported.

use std::collections::HashMap;

/// A file classification type.
///
/// The string value of each variant is the exact wire string used in reports
/// (e.g. `"source"`, `"vendor"`), matching the Go `Category string` constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    /// Source code files.
    Source,
    /// Vendored/third-party dependency files.
    Vendor,
    /// Auto-generated files.
    Generated,
    /// Documentation files.
    Documentation,
    /// Configuration files.
    Configuration,
    /// Image files.
    Image,
    /// Dotfiles (hidden config files).
    DotFile,
    /// Binary files.
    Binary,
}

impl Category {
    /// Returns the wire string for this category, identical to Go's
    /// `string(Category)`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
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

    /// Parses a wire string back into a [`Category`].
    ///
    /// Returns `None` for any string that is not a known category.
    #[must_use]
    // Mirrors Go's `string -> Category` lookup, which is fallible-by-`Option`
    // (not `Result`), so the inherent method intentionally does not implement
    // `std::str::FromStr`.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(value: &str) -> Option<Category> {
        match value {
            "source" => Some(Category::Source),
            "vendor" => Some(Category::Vendor),
            "generated" => Some(Category::Generated),
            "documentation" => Some(Category::Documentation),
            "configuration" => Some(Category::Configuration),
            "image" => Some(Category::Image),
            "dotfile" => Some(Category::DotFile),
            "binary" => Some(Category::Binary),
            _ => None,
        }
    }
}

/// The canonical order for display and charting.
///
/// Mirrors Go `AllCategories` exactly (`classify.go`):
/// source, documentation, configuration, vendor, generated, dotfile, image,
/// binary. This ordering is load-bearing — it governs the order of distribution
/// items and issues in the report section, and the iteration order in
/// [`crate::aggregator::Aggregator::get_result`].
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

/// Tracks counts per file category.
///
/// Unknown category strings (those not in [`ALL_CATEGORIES`]) are still counted
/// under their raw string key so the "skips invalid category" behaviour
/// matches: the count is recorded but never surfaces in
/// [`crate::aggregator::Aggregator::get_result`] because the result only
/// iterates the known categories.
#[derive(Debug, Default, Clone)]
pub struct CategoryCounts {
    counts: HashMap<String, i64>,
}

impl CategoryCounts {
    /// Creates an empty counter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Increases the count for a category string by one.
    pub fn increment(&mut self, category: &str) {
        *self.counts.entry(category.to_string()).or_insert(0) += 1;
    }

    /// Returns the count for a category (zero if never incremented).
    #[must_use]
    pub fn get(&self, category: Category) -> i64 {
        self.counts.get(category.as_str()).copied().unwrap_or(0)
    }
}
