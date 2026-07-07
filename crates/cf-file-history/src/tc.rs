//! Per-commit transport-cell (TC) payload types.
//!
//! These types capture, for a single commit, the path actions
//! (insert/modify/delete/rename), the per-author line-stat deltas, and the file
//! category composition. The full TC plumbing (change routing, blob cache,
//! identity detection) belongs to the framework integration sketched in
//! [`crate::framework`]. The data shapes and the [`CategoryCounts`] arithmetic
//! are report-contract-relevant (the category names feed the composition
//! report) and live here.

use crate::classify::Category;

/// Line statistics for a single file/author change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LineStats {
    /// Lines added.
    pub added: i64,
    /// Lines removed.
    pub removed: i64,
    /// Lines changed.
    pub changed: i64,
}

impl std::ops::Add for LineStats {
    type Output = LineStats;

    /// Returns the element-wise sum of two line-stat values.
    fn add(self, other: LineStats) -> LineStats {
        LineStats {
            added: self.added + other.added,
            removed: self.removed + other.removed,
            changed: self.changed + other.changed,
        }
    }
}

/// File category counts for a single commit.
///
/// JSON field order (when emitted directly) is the declaration order:
/// source, vendor, generated, documentation, configuration, image, dotfile,
/// binary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CategoryCounts {
    /// Source file count.
    pub source: i64,
    /// Vendor file count.
    pub vendor: i64,
    /// Generated file count.
    pub generated: i64,
    /// Documentation file count.
    pub documentation: i64,
    /// Configuration file count.
    pub configuration: i64,
    /// Image file count.
    pub image: i64,
    /// Dotfile count.
    pub dotfile: i64,
    /// Binary file count.
    pub binary: i64,
}

impl CategoryCounts {
    /// Adds the counts from `other` into `self`.
    pub fn add(&mut self, other: &CategoryCounts) {
        self.source += other.source;
        self.vendor += other.vendor;
        self.generated += other.generated;
        self.documentation += other.documentation;
        self.configuration += other.configuration;
        self.image += other.image;
        self.dotfile += other.dotfile;
        self.binary += other.binary;
    }

    /// Returns the sum of all category counts.
    #[must_use]
    pub fn total(&self) -> i64 {
        self.source
            + self.vendor
            + self.generated
            + self.documentation
            + self.configuration
            + self.image
            + self.dotfile
            + self.binary
    }

    /// Returns the count for the given category.
    #[must_use]
    pub fn get(&self, cat: Category) -> i64 {
        match cat {
            Category::Source => self.source,
            Category::Vendor => self.vendor,
            Category::Generated => self.generated,
            Category::Documentation => self.documentation,
            Category::Configuration => self.configuration,
            Category::Image => self.image,
            Category::DotFile => self.dotfile,
            Category::Binary => self.binary,
        }
    }

    /// Adds one to the count for the given category.
    ///
    /// ```
    /// use cf_file_history::{Category, CategoryCounts};
    ///
    /// let mut counts = CategoryCounts::default();
    /// counts.increment(Category::Source);
    /// counts.increment(Category::Source);
    /// counts.increment(Category::Binary);
    /// assert_eq!(counts.get(Category::Source), 2);
    /// assert_eq!(counts.get(Category::Binary), 1);
    /// assert_eq!(counts.total(), 3);
    /// ```
    pub fn increment(&mut self, cat: Category) {
        match cat {
            Category::Source => self.source += 1,
            Category::Vendor => self.vendor += 1,
            Category::Generated => self.generated += 1,
            Category::Documentation => self.documentation += 1,
            Category::Configuration => self.configuration += 1,
            Category::Image => self.image += 1,
            Category::DotFile => self.dotfile += 1,
            Category::Binary => self.binary += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_counts_add_total_get_increment() {
        let mut c = CategoryCounts::default();
        c.increment(Category::Source);
        c.increment(Category::Source);
        c.increment(Category::Binary);
        assert_eq!(c.get(Category::Source), 2);
        assert_eq!(c.get(Category::Binary), 1);
        assert_eq!(c.total(), 3);

        let other = CategoryCounts {
            vendor: 5,
            ..Default::default()
        };
        c.add(&other);
        assert_eq!(c.get(Category::Vendor), 5);
        assert_eq!(c.total(), 8);
    }

    #[test]
    fn line_stats_add() {
        let a = LineStats {
            added: 1,
            removed: 2,
            changed: 3,
        };
        let b = LineStats {
            added: 10,
            removed: 20,
            changed: 30,
        };
        assert_eq!(
            a + b,
            LineStats {
                added: 11,
                removed: 22,
                changed: 33
            }
        );
    }
}
