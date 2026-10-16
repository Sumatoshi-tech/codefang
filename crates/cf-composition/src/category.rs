//! File classification categories.
//!
//! [`Category`] is the 8-way classification shared with file-history
//! analysis; [`FileClass`] is what the composition analyzer reports — every
//! `Category` plus [`FileClass::Other`] for files that match no rule.

/// A file classification type.
///
/// The string value of each variant is the exact wire string used in reports
/// (e.g. `"source"`, `"vendor"`).
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
    /// Returns the wire string for this category.
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

    /// Returns the capitalized display name (`Source`, `Dotfile`, …).
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Category::Source => "Source",
            Category::Vendor => "Vendor",
            Category::Generated => "Generated",
            Category::Documentation => "Documentation",
            Category::Configuration => "Configuration",
            Category::Image => "Image",
            Category::DotFile => "Dotfile",
            Category::Binary => "Binary",
        }
    }
}

/// A composition bucket: one of the shared [`Category`] values, or `Other`
/// for a file that matches no classification rule (unknown extension, no
/// recognizable language).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileClass {
    /// One of the shared categories.
    Category(Category),
    /// Matches no rule.
    Other,
}

/// Wire string of [`FileClass::Other`].
pub const OTHER_STR: &str = "other";
/// Display name of [`FileClass::Other`].
const OTHER_DISPLAY: &str = "Other";

impl FileClass {
    /// Returns the wire string (`"source"`, …, `"other"`).
    ///
    /// ```
    /// use cf_composition::{Category, FileClass};
    /// assert_eq!(FileClass::Category(Category::Vendor).as_str(), "vendor");
    /// assert_eq!(FileClass::Other.as_str(), "other");
    /// ```
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            FileClass::Category(c) => c.as_str(),
            FileClass::Other => OTHER_STR,
        }
    }

    /// Returns the capitalized display name (`Vendor`, `Other`, …).
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            FileClass::Category(c) => c.display_name(),
            FileClass::Other => OTHER_DISPLAY,
        }
    }

    /// Position of this class in [`ALL_FILE_CLASSES`].
    #[must_use]
    pub fn index(self) -> usize {
        ALL_FILE_CLASSES
            .iter()
            .position(|c| *c == self)
            .expect("every FileClass is listed in ALL_FILE_CLASSES")
    }
}

impl From<Category> for FileClass {
    fn from(category: Category) -> Self {
        FileClass::Category(category)
    }
}

/// The canonical class order for reports and charts: source, documentation,
/// configuration, vendor, generated, dotfile, image, binary, other. It
/// governs the order of distribution items and issues in the report section
/// and the iteration order of the aggregated report.
pub const ALL_FILE_CLASSES: [FileClass; 9] = [
    FileClass::Category(Category::Source),
    FileClass::Category(Category::Documentation),
    FileClass::Category(Category::Configuration),
    FileClass::Category(Category::Vendor),
    FileClass::Category(Category::Generated),
    FileClass::Category(Category::DotFile),
    FileClass::Category(Category::Image),
    FileClass::Category(Category::Binary),
    FileClass::Other,
];

/// Per-class file counts.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ClassCounts {
    counts: [i64; ALL_FILE_CLASSES.len()],
}

impl ClassCounts {
    /// Creates an empty counter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Increases the count for `class` by one.
    pub fn increment(&mut self, class: FileClass) {
        self.counts[class.index()] += 1;
    }

    /// Returns the count for `class` (zero if never incremented).
    #[must_use]
    pub fn get(&self, class: FileClass) -> i64 {
        self.counts[class.index()]
    }

    /// Sum over every class.
    #[must_use]
    pub fn total(&self) -> i64 {
        self.counts.iter().sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_strings_and_display_names() {
        assert_eq!(Category::DotFile.as_str(), "dotfile");
        assert_eq!(Category::DotFile.display_name(), "Dotfile");
        assert_eq!(FileClass::Other.display_name(), "Other");
        assert_eq!(FileClass::from(Category::Image).as_str(), "image");
    }

    #[test]
    fn every_class_is_listed_once() {
        for (i, class) in ALL_FILE_CLASSES.iter().enumerate() {
            assert_eq!(class.index(), i);
        }
        assert_eq!(ALL_FILE_CLASSES.len(), 9);
        assert_eq!(ALL_FILE_CLASSES[8], FileClass::Other);
    }

    #[test]
    fn counts_increment_and_total() {
        let mut c = ClassCounts::new();
        c.increment(FileClass::Other);
        c.increment(FileClass::Other);
        c.increment(Category::Source.into());
        assert_eq!(c.get(FileClass::Other), 2);
        assert_eq!(c.get(Category::Source.into()), 1);
        assert_eq!(c.get(Category::Binary.into()), 0);
        assert_eq!(c.total(), 3);
    }
}
