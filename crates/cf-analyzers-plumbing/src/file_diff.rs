//! `FileDiff` provider.
//!
//! Produces, per **modified** file, the old/new line-of-code counts and a
//! line-granular diff. Only `Modify` changes are diffed (inserts and deletes
//! are not emitted here); two fast paths and a binary guard run first, and the
//! diff itself uses the diff-match-patch line algorithm.
//!
//! # Diff engine boundary
//!
//! The line diff itself is the byte-identity-critical step. The reference
//! pipeline is: encode both (whitespace-stripped) sides to lines-as-runes,
//! run the main diff, and — unless cleanup is disabled — apply the
//! semantic-lossless and merge cleanup passes. The engine is expressed as the
//! [`LineDiffer`] trait and injected; `cf-godiff` is the byte-faithful
//! implementation (its output is pinned by the differential gate).

use std::collections::HashMap;

use crate::analyzer::{dep, Analyzer, AnalyzerError, ValueMap};
use crate::blob_cache::{is_binary, CachedBlob};
use crate::git_model::{Action, Change, Changes, Hash};

/// Default diff timeout in milliseconds.
pub const DEFAULT_DIFF_TIMEOUT_MS: i64 = 1000;

/// Diff operation kind.
///
/// The discriminants (`Delete = -1`, `Equal = 0`, `Insert = 1`) are frozen so
/// a serializer can emit the same integer codes as the diff engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffOp {
    /// Text removed.
    Delete = -1,
    /// Text unchanged.
    Equal = 0,
    /// Text added.
    Insert = 1,
}

/// One element of a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    /// Diff operation.
    pub op: DiffOp,
    /// The text covered by this diff segment.
    pub text: String,
}

/// Parameters passed to a [`LineDiffer`].
#[derive(Debug, Clone, Copy)]
pub struct DiffParams {
    /// Diff timeout in milliseconds (`0` = no limit).
    pub timeout_ms: i64,
    /// Whether to skip the semantic-lossless + merge cleanup pass.
    pub cleanup_disabled: bool,
}

/// Outcome of a line diff: the diff segments plus the encoded-line counts
/// reported as the old/new lines of code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineDiffResult {
    /// Number of encoded lines on the old side.
    pub old_lines_of_code: usize,
    /// Number of encoded lines on the new side.
    pub new_lines_of_code: usize,
    /// Diff segments (after the optional cleanup pass).
    pub diffs: Vec<Diff>,
}

/// The line-diff engine boundary.
///
/// Implementations must reproduce the reference diff-match-patch line
/// pipeline exactly for the machine output to stay byte-identical
/// (`cf-godiff` does); see the module note.
pub trait LineDiffer {
    /// Diff two already-whitespace-normalized strings at line granularity.
    fn line_diff(&self, old: &str, new: &str, params: DiffParams) -> LineDiffResult;
}

/// Per-file diff result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiffData {
    /// Lines of code before the change.
    pub old_lines_of_code: usize,
    /// Lines of code after the change.
    pub new_lines_of_code: usize,
    /// The diff segments.
    pub diffs: Vec<Diff>,
}

/// `FileDiff` provider.
pub struct FileDiff<D: LineDiffer> {
    /// When true, skip the cleanup pass.
    pub cleanup_disabled: bool,
    /// Ignore whitespace when diffing: spaces are stripped from both sides
    /// before the line diff.
    pub whitespace_ignore: bool,
    /// Diff timeout in milliseconds; `0` means no limit.
    pub timeout_ms: i64,
    differ: D,
}

impl<D: LineDiffer> FileDiff<D> {
    /// Construct a `FileDiff` over the given diff engine with the standard
    /// defaults.
    pub const fn new(differ: D) -> Self {
        Self {
            cleanup_disabled: false,
            whitespace_ignore: false,
            timeout_ms: DEFAULT_DIFF_TIMEOUT_MS,
            differ,
        }
    }

    /// Build the per-file diff map for one commit's changes (only
    /// modifications are processed).
    pub fn build(
        &self,
        changes: &Changes,
        cache: &HashMap<Hash, CachedBlob>,
    ) -> HashMap<String, FileDiffData> {
        let mut result: HashMap<String, FileDiffData> = HashMap::new();
        for change in changes {
            self.process_change(change, cache, &mut result);
        }
        result
    }

    /// Process one change.
    fn process_change(
        &self,
        change: &Change,
        cache: &HashMap<Hash, CachedBlob>,
        result: &mut HashMap<String, FileDiffData>,
    ) {
        // Only Modify changes are diffed.
        if change.action() != Some(Action::Modify) {
            return;
        }

        let (Some(blob_from), Some(blob_to)) =
            (cache.get(&change.from.hash), cache.get(&change.to.hash))
        else {
            return;
        };

        // Fast path: identical content by hash.
        if change.from.hash == change.to.hash {
            return;
        }

        // Skip binary blobs (either side binary skips the pair).
        if is_binary(&blob_from.data) || is_binary(&blob_to.data) {
            return;
        }

        // Decode lossily; the diff engine operates on decoded text.
        let str_from = String::from_utf8_lossy(&blob_from.data).into_owned();
        let str_to = String::from_utf8_lossy(&blob_to.data).into_owned();

        // Fast path: identical strings -> single DiffEqual with "L"*lineCount.
        if str_from == str_to {
            let line_count = count_trailing_aware_lines(&str_from);
            result.insert(
                change.to.name.clone(),
                FileDiffData {
                    old_lines_of_code: line_count,
                    new_lines_of_code: line_count,
                    diffs: vec![Diff {
                        op: DiffOp::Equal,
                        text: "L".repeat(line_count),
                    }],
                },
            );
            return;
        }

        let data = self.compute_modify(&str_from, &str_to);
        result.insert(change.to.name.clone(), data);
    }

    /// Compute the diff for a modification: whitespace strip, line diff,
    /// optional cleanup, and the encoded-line LOC counts.
    pub fn compute_modify(&self, str_from: &str, str_to: &str) -> FileDiffData {
        let from = strip_whitespace(str_from, self.whitespace_ignore);
        let to = strip_whitespace(str_to, self.whitespace_ignore);
        let res = self.differ.line_diff(
            &from,
            &to,
            DiffParams {
                timeout_ms: self.timeout_ms,
                cleanup_disabled: self.cleanup_disabled,
            },
        );
        FileDiffData {
            old_lines_of_code: res.old_lines_of_code,
            new_lines_of_code: res.new_lines_of_code,
            diffs: res.diffs,
        }
    }
}

impl<D: LineDiffer> Analyzer for FileDiff<D> {
    fn name(&self) -> &'static str {
        "FileDiff"
    }

    fn provides(&self) -> Vec<&'static str> {
        vec!["file_diff"]
    }

    fn requires(&self) -> Vec<&'static str> {
        vec!["changes", "blob_cache"]
    }

    fn consume(&mut self, deps: &mut ValueMap) -> Result<ValueMap, AnalyzerError> {
        let changes = dep::<Changes>(deps, "changes")?;
        let cache = dep::<HashMap<Hash, CachedBlob>>(deps, "blob_cache")?;
        let result = self.build(changes, cache);
        let mut out = ValueMap::new();
        out.insert("file_diff".to_string(), Box::new(result));
        Ok(out)
    }
}

/// Strip spaces (only U+0020) if whitespace is ignored.
fn strip_whitespace(s: &str, ignore: bool) -> String {
    if ignore {
        s.replace(' ', "")
    } else {
        s.to_string()
    }
}

/// Line counting for the identical-string fast path: the newline count, plus
/// one when the string is non-empty and does not end in a newline.
fn count_trailing_aware_lines(s: &str) -> usize {
    let mut count = s.bytes().filter(|&b| b == b'\n').count();
    if !s.is_empty() && s.as_bytes().last() != Some(&b'\n') {
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git_model::ChangeEntry;

    /// A trivial line differ for exercising the provider control flow. It is
    /// NOT byte-faithful; the real engine is `cf-godiff` (see module note).
    struct LineCountDiffer;
    impl LineDiffer for LineCountDiffer {
        fn line_diff(&self, old: &str, new: &str, _p: DiffParams) -> LineDiffResult {
            let old_loc = line_count(old);
            let new_loc = line_count(new);
            LineDiffResult {
                old_lines_of_code: old_loc,
                new_lines_of_code: new_loc,
                diffs: vec![
                    Diff {
                        op: DiffOp::Delete,
                        text: old.to_string(),
                    },
                    Diff {
                        op: DiffOp::Insert,
                        text: new.to_string(),
                    },
                ],
            }
        }
    }

    fn line_count(s: &str) -> usize {
        if s.is_empty() {
            0
        } else {
            s.split('\n').count()
        }
    }

    fn h(n: u8) -> Hash {
        let mut b = [0u8; 20];
        b[0] = n;
        Hash(b)
    }

    fn modify(from_h: Hash, to_h: Hash, name: &str) -> Change {
        Change {
            from: ChangeEntry {
                name: name.into(),
                hash: from_h,
            },
            to: ChangeEntry {
                name: name.into(),
                hash: to_h,
            },
        }
    }

    #[test]
    fn count_trailing_aware_lines_table() {
        assert_eq!(count_trailing_aware_lines(""), 0);
        assert_eq!(count_trailing_aware_lines("a"), 1);
        assert_eq!(count_trailing_aware_lines("a\n"), 1);
        assert_eq!(count_trailing_aware_lines("a\nb"), 2);
        assert_eq!(count_trailing_aware_lines("a\nb\n"), 2);
    }

    #[test]
    fn inserts_and_deletes_are_not_diffed() {
        let f = FileDiff::new(LineCountDiffer);
        let mut cache = HashMap::new();
        cache.insert(h(1), CachedBlob::new(b"x\n".to_vec()));
        let changes = vec![
            Change {
                from: ChangeEntry::default(),
                to: ChangeEntry {
                    name: "a".into(),
                    hash: h(1),
                },
            },
            Change {
                from: ChangeEntry {
                    name: "b".into(),
                    hash: h(1),
                },
                to: ChangeEntry::default(),
            },
        ];
        assert!(f.build(&changes, &cache).is_empty());
    }

    #[test]
    fn same_hash_modify_is_skipped() {
        let f = FileDiff::new(LineCountDiffer);
        let mut cache = HashMap::new();
        cache.insert(h(1), CachedBlob::new(b"x\n".to_vec()));
        let changes = vec![modify(h(1), h(1), "a")];
        assert!(f.build(&changes, &cache).is_empty());
    }

    #[test]
    fn identical_strings_fast_path() {
        let f = FileDiff::new(LineCountDiffer);
        let mut cache = HashMap::new();
        cache.insert(h(1), CachedBlob::new(b"a\nb\n".to_vec()));
        cache.insert(h(2), CachedBlob::new(b"a\nb\n".to_vec()));
        let changes = vec![modify(h(1), h(2), "a")];
        let out = f.build(&changes, &cache);
        let d = out.get("a").unwrap();
        assert_eq!(d.old_lines_of_code, 2);
        assert_eq!(d.new_lines_of_code, 2);
        assert_eq!(d.diffs.len(), 1);
        assert_eq!(d.diffs[0].op, DiffOp::Equal);
        assert_eq!(d.diffs[0].text, "LL");
    }

    #[test]
    fn binary_blobs_are_skipped() {
        let f = FileDiff::new(LineCountDiffer);
        let mut cache = HashMap::new();
        cache.insert(h(1), CachedBlob::new(vec![0u8, 1, 2]));
        cache.insert(h(2), CachedBlob::new(b"text\n".to_vec()));
        let changes = vec![modify(h(1), h(2), "a")];
        assert!(f.build(&changes, &cache).is_empty());
    }

    #[test]
    fn real_modify_invokes_differ() {
        let f = FileDiff::new(LineCountDiffer);
        let mut cache = HashMap::new();
        cache.insert(h(1), CachedBlob::new(b"a\nb\nc".to_vec()));
        cache.insert(h(2), CachedBlob::new(b"a\nB\nc".to_vec()));
        let changes = vec![modify(h(1), h(2), "a")];
        let out = f.build(&changes, &cache);
        let d = out.get("a").unwrap();
        assert_eq!(d.old_lines_of_code, 3);
        assert_eq!(d.new_lines_of_code, 3);
        assert!(!d.diffs.is_empty());
    }

    #[test]
    fn whitespace_ignore_strips_spaces() {
        let mut f = FileDiff::new(LineCountDiffer);
        f.whitespace_ignore = true;
        // After stripping spaces both sides are "a\nb", so the differ sees
        // equal line counts.
        let r = f.compute_modify("a \n b", "a\nb");
        assert_eq!(r.old_lines_of_code, 2);
        assert_eq!(r.new_lines_of_code, 2);
    }

    #[test]
    fn provider_metadata() {
        let f = FileDiff::new(LineCountDiffer);
        assert_eq!(f.name(), "FileDiff");
        assert_eq!(f.requires(), vec!["changes", "blob_cache"]);
        assert_eq!(f.provides(), vec!["file_diff"]);
    }
}
