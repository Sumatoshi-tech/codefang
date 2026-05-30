//! `UASTChanges` provider.
//!
//! Port of `internal/analyzers/plumbing/uast.go`.
//!
//! Produces, per change, the parsed UAST root of the before and/or after blob:
//! * the **before** side is parsed for `Modify` and `Delete`;
//! * the **after** side is parsed for `Modify` and `Insert`.
//!
//! A change is only emitted when at least one side parsed to a node (Go:
//! `if before != nil || after != nil`). `parseBlob` gates on the path filter,
//! cache membership, parser language support, and a blob-size limit, mirroring
//! `uast.go` step for step.

use std::collections::HashMap;

use crate::analyzer::{dep, Analyzer, AnalyzerError, ValueMap};
use crate::blob_cache::CachedBlob;
use crate::git_model::{Action, Change, Changes, Hash};
use crate::uast_iface::{AllowAllPathFilter, Node, PathFilter, SharedParser, SharedPathFilter};
use std::sync::Arc;

/// Maximum blob size (bytes) for UAST parsing, mirroring Go's
/// `maxUASTBlobSize = 256 * 1024`.
pub const MAX_UAST_BLOB_SIZE: usize = 256 * 1024;

/// A change paired with its before/after UAST roots, mirroring Go's
/// `uast.Change`.
#[derive(Debug, Clone)]
pub struct UASTChange {
    /// The underlying file change.
    pub change: Change,
    /// UAST root of the pre-change blob (`None` for inserts and unparsable
    /// files).
    pub before: Option<Node>,
    /// UAST root of the post-change blob (`None` for deletes and unparsable
    /// files).
    pub after: Option<Node>,
}

/// `UASTChanges` provider, mirroring Go's `UASTChangesAnalyzer`.
pub struct UASTChanges {
    parser: Option<SharedParser>,
    path_filter: SharedPathFilter,
    /// Maximum blob size for parsing; `0` uses [`MAX_UAST_BLOB_SIZE`]
    /// (Go's `MaxBlobSize`).
    pub max_blob_size: usize,
}

impl Default for UASTChanges {
    fn default() -> Self {
        Self::new()
    }
}

impl UASTChanges {
    /// Construct without a parser (every parse yields `None` until one is set),
    /// using a permissive path filter.
    pub fn new() -> Self {
        UASTChanges {
            parser: None,
            path_filter: Arc::new(AllowAllPathFilter),
            max_blob_size: 0,
        }
    }

    /// Set the path filter used to skip vendor/generated files
    /// (Go's `pathfilter.New()`).
    pub fn set_path_filter(&mut self, filter: SharedPathFilter) {
        self.path_filter = filter;
    }

    /// Build the list of UAST changes for one commit, mirroring Go's
    /// `changesSequential`: parse before/after per change and keep only changes
    /// with at least one parsed side.
    pub fn build(
        &self,
        changes: &Changes,
        cache: &HashMap<Hash, CachedBlob>,
    ) -> Vec<UASTChange> {
        let mut result: Vec<UASTChange> = Vec::new();
        for change in changes {
            let before = self.parse_before_version(change, cache);
            let after = self.parse_after_version(change, cache);
            if before.is_some() || after.is_some() {
                result.push(UASTChange {
                    change: change.clone(),
                    before,
                    after,
                });
            }
        }
        result
    }

    /// Parse the before side for Modify/Delete, mirroring `parseBeforeVersion`.
    fn parse_before_version(
        &self,
        change: &Change,
        cache: &HashMap<Hash, CachedBlob>,
    ) -> Option<Node> {
        match change.action() {
            Some(Action::Modify) | Some(Action::Delete) => {
                self.parse_blob(change.from.hash, &change.from.name, cache)
            }
            _ => None,
        }
    }

    /// Parse the after side for Modify/Insert, mirroring `parseAfterVersion`.
    fn parse_after_version(
        &self,
        change: &Change,
        cache: &HashMap<Hash, CachedBlob>,
    ) -> Option<Node> {
        match change.action() {
            Some(Action::Modify) | Some(Action::Insert) => {
                self.parse_blob(change.to.hash, &change.to.name, cache)
            }
            _ => None,
        }
    }

    /// Parse a blob into a UAST root, mirroring Go's `parseBlob` gate sequence:
    /// path-filter name exclusion -> cache membership -> parser support ->
    /// blob-size limit -> path-filter content exclusion -> parse.
    fn parse_blob(
        &self,
        hash: Hash,
        filename: &str,
        cache: &HashMap<Hash, CachedBlob>,
    ) -> Option<Node> {
        if self.path_filter.is_excluded(filename) {
            return None;
        }
        let blob = cache.get(&hash)?;
        let parser = self.parser.as_ref()?;
        if !parser.is_supported(filename) {
            return None;
        }
        let limit = if self.max_blob_size == 0 {
            MAX_UAST_BLOB_SIZE
        } else {
            self.max_blob_size
        };
        if blob.data.len() > limit {
            return None;
        }
        if self.path_filter.is_excluded_with_content(filename, &blob.data) {
            return None;
        }
        parser.parse(filename, &blob.data)
    }
}

impl Analyzer for UASTChanges {
    fn name(&self) -> &'static str {
        "UASTChanges"
    }

    fn provides(&self) -> Vec<&'static str> {
        vec!["uast_changes"]
    }

    fn requires(&self) -> Vec<&'static str> {
        vec!["changes", "blob_cache"]
    }

    fn configure_uast(&mut self, parser: SharedParser) {
        self.parser = Some(parser);
    }

    fn consume(&mut self, deps: &mut ValueMap) -> Result<ValueMap, AnalyzerError> {
        let changes = dep::<Changes>(deps, "changes")?.clone();
        let cache = dep::<HashMap<Hash, CachedBlob>>(deps, "blob_cache")?.clone();
        let result = self.build(&changes, &cache);
        let mut out = ValueMap::new();
        out.insert("uast_changes".to_string(), Box::new(result));
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_model::ChangeEntry;
    use crate::uast_iface::{NodeLike, Parser};

    #[derive(Debug)]
    struct FakeNode;
    impl NodeLike for FakeNode {}

    /// Parser that supports `.go` files and parses any non-empty content.
    struct GoParser;
    impl Parser for GoParser {
        fn is_supported(&self, filename: &str) -> bool {
            filename.ends_with(".go")
        }
        fn parse(&self, _filename: &str, content: &[u8]) -> Option<Node> {
            if content.is_empty() {
                None
            } else {
                Some(Arc::new(FakeNode) as Node)
            }
        }
    }

    fn h(n: u8) -> Hash {
        let mut b = [0u8; 20];
        b[0] = n;
        Hash(b)
    }

    #[test]
    fn modify_parses_both_sides() {
        let mut uc = UASTChanges::new();
        uc.configure_uast(Arc::new(GoParser));
        let mut cache = HashMap::new();
        cache.insert(h(1), CachedBlob::new(b"old".to_vec()));
        cache.insert(h(2), CachedBlob::new(b"new".to_vec()));
        let changes = vec![Change {
            from: ChangeEntry { name: "f.go".into(), hash: h(1) },
            to: ChangeEntry { name: "f.go".into(), hash: h(2) },
        }];
        let out = uc.build(&changes, &cache);
        assert_eq!(out.len(), 1);
        assert!(out[0].before.is_some());
        assert!(out[0].after.is_some());
    }

    #[test]
    fn unsupported_language_is_skipped() {
        let mut uc = UASTChanges::new();
        uc.configure_uast(Arc::new(GoParser));
        let mut cache = HashMap::new();
        cache.insert(h(1), CachedBlob::new(b"x".to_vec()));
        // A .txt file is unsupported -> no node -> change dropped.
        let changes = vec![Change {
            from: ChangeEntry::default(),
            to: ChangeEntry { name: "f.txt".into(), hash: h(1) },
        }];
        assert!(uc.build(&changes, &cache).is_empty());
    }

    #[test]
    fn oversized_blob_is_skipped() {
        let mut uc = UASTChanges::new();
        uc.configure_uast(Arc::new(GoParser));
        uc.max_blob_size = 4;
        let mut cache = HashMap::new();
        cache.insert(h(1), CachedBlob::new(b"too long".to_vec()));
        let changes = vec![Change {
            from: ChangeEntry::default(),
            to: ChangeEntry { name: "f.go".into(), hash: h(1) },
        }];
        assert!(uc.build(&changes, &cache).is_empty());
    }

    #[test]
    fn no_parser_yields_no_changes() {
        let uc = UASTChanges::new();
        let mut cache = HashMap::new();
        cache.insert(h(1), CachedBlob::new(b"x".to_vec()));
        let changes = vec![Change {
            from: ChangeEntry::default(),
            to: ChangeEntry { name: "f.go".into(), hash: h(1) },
        }];
        assert!(uc.build(&changes, &cache).is_empty());
    }

    #[test]
    fn provider_metadata() {
        let uc = UASTChanges::new();
        assert_eq!(uc.name(), "UASTChanges");
        assert_eq!(uc.provides(), vec!["uast_changes"]);
        assert_eq!(uc.requires(), vec!["changes", "blob_cache"]);
    }
}
