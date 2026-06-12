//! Minimal UAST + path-filter interface used by the plumbing providers.
//!
//! The canonical UAST node model and parser live in `cf-uast`/`cf-uast-node`,
//! and path filtering lives in `cf-pathfilter`. This module defines only the
//! minimal surface `UASTChanges` needs (`Parser` with `is_supported`/`parse`,
//! and a `PathFilter` content/name exclusion check) so this crate stays
//! decoupled; it should collapse into re-exports of `cf_uast::Parser` and
//! `cf_pathfilter::Filter` as those surfaces stabilize.

use std::sync::Arc;

/// A parsed UAST root, threaded through the pipeline by `UASTChanges`.
///
/// Modelled as an opaque, refcounted trait object so the plumbing layer is
/// agnostic to `cf-uast-node`'s concrete `Node` type.
pub trait NodeLike: std::fmt::Debug + Send + Sync {}

/// Shared, cloneable parsed UAST root.
pub type Node = Arc<dyn NodeLike>;

/// A UAST parser (the subset of the parser surface used here).
pub trait Parser: Send + Sync {
    /// Whether the file's language is supported.
    fn is_supported(&self, filename: &str) -> bool;

    /// Parse a blob into a UAST root, or `None` if it cannot be parsed.
    fn parse(&self, filename: &str, content: &[u8]) -> Option<Node>;
}

/// Shared, cloneable handle to a [`Parser`].
pub type SharedParser = Arc<dyn Parser>;

/// Vendor/generated path filtering (the subset of the filter surface used by
/// `UASTChanges`).
pub trait PathFilter: Send + Sync {
    /// Whether the path is excluded by name alone (vendor/generated rules).
    fn is_excluded(&self, filename: &str) -> bool;

    /// Whether the path is excluded once content is considered (e.g. a
    /// "DO NOT EDIT" generated-file header).
    fn is_excluded_with_content(&self, filename: &str, content: &[u8]) -> bool;
}

/// Shared, cloneable handle to a [`PathFilter`].
pub type SharedPathFilter = Arc<dyn PathFilter>;

/// A [`PathFilter`] that never excludes anything — the conservative default
/// when no real filter is wired (every file is considered).
#[derive(Debug, Clone, Copy, Default)]
pub struct AllowAllPathFilter;

impl PathFilter for AllowAllPathFilter {
    fn is_excluded(&self, _filename: &str) -> bool {
        false
    }
    fn is_excluded_with_content(&self, _filename: &str, _content: &[u8]) -> bool {
        false
    }
}
