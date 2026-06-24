//! Incremental-analysis cache metadata (`cache.json`).
//!
//! The [`IncrementalMeta`] record plus the [`key`], [`is_stale`],
//! [`write_meta`], and [`read_meta`] helpers and the [`ReadMetaError`]
//! sentinels.
//!
//! # Byte identity
//!
//! `cache.json` is written as indented JSON with HTML escaping on, fields in
//! struct declaration order, and a trailing newline. The file is INTERNAL state
//! rather than a user-visible machine report, but its bytes are still part of
//! the compatibility contract, so the write routes through
//! [`cf_textutil::write_json`] (the report-format encoder), NEVER raw
//! `serde_json`, building a [`cf_textutil::GoValue`] whose fields are pushed in
//! declaration order so the emitted key order is stable.
//!
//! Reads decode with `serde_json` (parsing the artifact back into the struct
//! does not affect output bytes).

use std::fs;
use std::path::Path;

use cf_textutil::GoValue;
use sha2::{Digest, Sha256};

use crate::gitlib::HEX_DIGITS;

/// Name of the cache metadata file.
pub const META_FILENAME: &str = "cache.json";

/// File permission for cache metadata, `0o640`.
pub const META_FILE_PERM: u32 = 0o640;

/// Separator between root SHA and branch in the cache-key input.
const CACHE_KEY_SEPARATOR: &str = ":";

/// Metadata for an incremental analysis cache.
///
/// Field order is significant: it governs the emitted JSON key order (see
/// [`to_govalue`](IncrementalMeta::to_govalue)), which is pinned by the
/// on-disk byte-layout test below.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IncrementalMeta {
    /// Cache format version.
    #[serde(rename = "version")]
    pub version: i64,
    /// HEAD commit SHA at cache time.
    #[serde(rename = "head_sha")]
    pub head_sha: String,
    /// Branch name.
    #[serde(rename = "branch")]
    pub branch: String,
    /// Root commit SHA; a change indicates a force-push / history rewrite.
    #[serde(rename = "root_sha")]
    pub root_sha: String,
    /// Number of commits analyzed.
    #[serde(rename = "commit_count")]
    pub commit_count: i64,
    /// IDs of the analyzers whose results are cached.
    #[serde(rename = "analyzer_ids")]
    pub analyzer_ids: Vec<String>,
    /// Cache creation timestamp, RFC3339(Nano) wire text.
    #[serde(rename = "timestamp")]
    pub timestamp: String,
}

impl IncrementalMeta {
    /// Builds the report-format [`GoValue`] for this metadata, with fields in
    /// struct declaration order (struct-origin object, NOT byte-sorted) so the
    /// emitted JSON key order is stable.
    fn to_govalue(&self) -> GoValue {
        // STRUCT-origin object: emit fields in DECLARATION order, NOT
        // byte-sorted. The report-format encoder sorts keys only for map-origin
        // objects; a struct emits fields in source order. We therefore use
        // `GoMap::new_struct()`, which preserves insertion (declaration) order
        // on encode, unlike `GoMap::from_map`, which byte-sorts keys. This is
        // the dual-mode `GoMap` distinction from DESIGN.md §2.2.
        let mut m = cf_textutil::GoMap::new_struct();
        m.push("version", GoValue::Int(self.version));
        m.push("head_sha", GoValue::Str(self.head_sha.clone()));
        m.push("branch", GoValue::Str(self.branch.clone()));
        m.push("root_sha", GoValue::Str(self.root_sha.clone()));
        m.push("commit_count", GoValue::Int(self.commit_count));
        m.push(
            "analyzer_ids",
            GoValue::Array(
                self.analyzer_ids
                    .iter()
                    .map(|s| GoValue::Str(s.clone()))
                    .collect(),
            ),
        );
        m.push("timestamp", GoValue::Str(self.timestamp.clone()));
        GoValue::Object(m)
    }
}

/// Errors returned by [`read_meta`]: the not-found / corrupt sentinels, plus
/// wrapped I/O errors.
#[derive(Debug, thiserror::Error)]
pub enum ReadMetaError {
    /// The cache metadata file does not exist.
    #[error("cache metadata not found")]
    NotFound,
    /// The cache metadata file could not be parsed.
    #[error("cache metadata corrupt: {0}")]
    Corrupt(serde_json::Error),
    /// An I/O error other than "not found" occurred reading the file.
    #[error("read cache meta: {0}")]
    Io(std::io::Error),
}

/// Errors returned by [`write_meta`].
///
/// Both variants render with the same `write cache meta: ` prefix; the variant
/// distinguishes the failing stage for programmatic matching.
#[derive(Debug, thiserror::Error)]
pub enum WriteMetaError {
    /// The value could not be encoded to report-format JSON.
    #[error("write cache meta: {0}")]
    Encode(String),
    /// An I/O error occurred while writing or renaming the metadata file.
    #[error("write cache meta: {0}")]
    Io(String),
}

/// Produces a deterministic directory name from root SHA and branch.
///
/// The key is the SHA-256 of `"rootSHA:branch"`, lowercase hex-encoded.
#[must_use]
pub fn key(root_sha: &str, branch: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(root_sha.as_bytes());
    hasher.update(CACHE_KEY_SEPARATOR.as_bytes());
    hasher.update(branch.as_bytes());
    let digest = hasher.finalize();

    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push(HEX_DIGITS[(b >> 4) as usize] as char);
        s.push(HEX_DIGITS[(b & 0x0f) as usize] as char);
    }
    s
}

/// Reports whether the cached root SHA differs from the current one, indicating
/// a force-push or history rewrite.
#[must_use]
pub fn is_stale(meta: &IncrementalMeta, current_root_sha: &str) -> bool {
    meta.root_sha != current_root_sha
}

/// Atomically writes cache metadata as indented JSON to `dir/cache.json`.
///
/// The bytes are produced by [`cf_textutil::write_json`] (pretty, HTML escaping
/// on, trailing newline) and committed via [`cf_storage::write_atomic`] with
/// mode `0o640`.
///
/// # Errors
///
/// Returns [`WriteMetaError::Encode`] if the metadata cannot be encoded, or
/// [`WriteMetaError::Io`] if the atomic write fails.
pub fn write_meta(dir: &Path, meta: &IncrementalMeta) -> Result<(), WriteMetaError> {
    let meta_path = dir.join(META_FILENAME);
    let value = meta.to_govalue();

    cf_storage::write_atomic(&meta_path, META_FILE_PERM, |w| {
        cf_textutil::write_json(w, &value, true).map_err(|e| std::io::Error::other(e.to_string()))
    })
    .map_err(|e| WriteMetaError::Io(e.to_string()))
}

/// Reads and parses cache metadata from `dir/cache.json`.
///
/// # Errors
///
/// Returns [`ReadMetaError::NotFound`] if the file does not exist,
/// [`ReadMetaError::Corrupt`] if it cannot be parsed, and
/// [`ReadMetaError::Io`] for any other read failure.
pub fn read_meta(dir: &Path) -> Result<IncrementalMeta, ReadMetaError> {
    let meta_path = dir.join(META_FILENAME);

    let data = match fs::read(&meta_path) {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(ReadMetaError::NotFound),
        Err(e) => return Err(ReadMetaError::Io(e)),
    };

    serde_json::from_slice(&data).map_err(ReadMetaError::Corrupt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_meta() -> IncrementalMeta {
        IncrementalMeta {
            version: 1,
            head_sha: "abc123def456".to_string(),
            branch: "main".to_string(),
            root_sha: "root789".to_string(),
            commit_count: 1000,
            analyzer_ids: vec!["burndown".to_string(), "couples".to_string()],
            // RFC3339 wire form of a fixed instant (2026-03-28T12:00:00 UTC).
            timestamp: "2026-03-28T12:00:00Z".to_string(),
        }
    }

    // Mirrors the reference test TestCacheKey_Deterministic.
    #[test]
    fn key_deterministic() {
        let k1 = key("abc123", "main");
        let k2 = key("abc123", "main");
        assert_eq!(k1, k2);
        assert!(!k1.is_empty());
    }

    // Mirrors the reference test TestCacheKey_DifferentBranch.
    #[test]
    fn key_different_branch() {
        assert_ne!(key("abc123", "main"), key("abc123", "feature/x"));
    }

    // Mirrors the reference test TestCacheKey_DifferentRoot.
    #[test]
    fn key_different_root() {
        assert_ne!(key("abc123", "main"), key("def456", "main"));
    }

    // Known-answer vector: SHA-256("root789:main") hex. Locks the exact
    // derivation against the reference implementation.
    #[test]
    fn key_known_answer() {
        // Independently computed from "root789:main".
        let mut h = Sha256::new();
        h.update(b"root789:main");
        let expected: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(key("root789", "main"), expected);
    }

    // Mirrors the reference test TestWriteReadMeta_RoundTrip.
    #[test]
    fn write_read_round_trip() {
        let dir = tempdir().unwrap();
        let original = test_meta();
        write_meta(dir.path(), &original).unwrap();

        let got = read_meta(dir.path()).unwrap();
        assert_eq!(original.version, got.version);
        assert_eq!(original.head_sha, got.head_sha);
        assert_eq!(original.branch, got.branch);
        assert_eq!(original.root_sha, got.root_sha);
        assert_eq!(original.commit_count, got.commit_count);
        assert_eq!(original.analyzer_ids, got.analyzer_ids);
        assert_eq!(original.timestamp, got.timestamp);
    }

    // Mirrors the reference test TestReadMeta_MissingFile.
    #[test]
    fn read_meta_missing_file() {
        let dir = tempdir().unwrap();
        let err = read_meta(dir.path()).unwrap_err();
        assert!(matches!(err, ReadMetaError::NotFound));
    }

    // Mirrors the reference test TestReadMeta_CorruptFile.
    #[test]
    fn read_meta_corrupt_file() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("cache.json"), b"{not valid json").unwrap();
        let err = read_meta(dir.path()).unwrap_err();
        assert!(matches!(err, ReadMetaError::Corrupt(_)));
    }

    // Mirrors the reference test TestIsStale_MatchingRootSHA.
    #[test]
    fn is_stale_matching() {
        let meta = test_meta();
        assert!(!is_stale(&meta, &meta.root_sha));
    }

    // Mirrors the reference test TestIsStale_MismatchingRootSHA.
    #[test]
    fn is_stale_mismatching() {
        let meta = test_meta();
        assert!(is_stale(&meta, "different_root"));
    }

    // Byte-identity of the on-disk cache.json (pinned reference-format layout):
    // 2-space indent, space after colon, empty array collapsed, struct field
    // DECLARATION order, trailing newline. `to_govalue` builds the struct-origin
    // object in declaration order (not the byte-sorted map-origin helper), so
    // this golden matches the reference output exactly.
    #[test]
    fn cache_json_byte_layout() {
        let dir = tempdir().unwrap();
        write_meta(dir.path(), &test_meta()).unwrap();
        let bytes = fs::read(dir.path().join("cache.json")).unwrap();
        let expected = concat!(
            "{\n",
            "  \"version\": 1,\n",
            "  \"head_sha\": \"abc123def456\",\n",
            "  \"branch\": \"main\",\n",
            "  \"root_sha\": \"root789\",\n",
            "  \"commit_count\": 1000,\n",
            "  \"analyzer_ids\": [\n",
            "    \"burndown\",\n",
            "    \"couples\"\n",
            "  ],\n",
            "  \"timestamp\": \"2026-03-28T12:00:00Z\"\n",
            "}\n",
        );
        assert_eq!(String::from_utf8(bytes).unwrap(), expected);
    }

    #[test]
    fn cache_json_empty_array_collapses() {
        let dir = tempdir().unwrap();
        let meta = IncrementalMeta {
            analyzer_ids: vec![],
            ..test_meta()
        };
        write_meta(dir.path(), &meta).unwrap();
        let text = fs::read_to_string(dir.path().join("cache.json")).unwrap();
        assert!(text.contains("\"analyzer_ids\": [],\n"), "got: {text}");
    }
}
