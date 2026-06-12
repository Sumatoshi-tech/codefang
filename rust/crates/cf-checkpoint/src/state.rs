//! Checkpoint state and metadata structures.
//!
//! These are the wrapper structs whose JSON field order is governed by
//! **struct declaration order** (DESIGN §2): the declaration order and the
//! serde rename strings below are frozen so that `checkpoint.json` keeps the
//! pinned byte layout. `checksums` is the only map-origin field; serialized
//! via a `BTreeMap` it is byte-sorted by key (map-key ordering contract).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Records on-disk spill state for a single aggregator.
///
/// Both fields are omitted when zero-valued, so an empty entry
/// (`dir == ""`, `count == 0`) serializes to `{}` (metadata-layout contract).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AggregatorSpillEntry {
    /// Directory containing gob/bincode-encoded spill files.
    #[serde(rename = "dir", default, skip_serializing_if = "String::is_empty")]
    pub dir: String,

    /// Number of spill files in [`dir`](AggregatorSpillEntry::dir).
    #[serde(rename = "count", default, skip_serializing_if = "is_zero_i64")]
    pub count: i64,
}

/// Tracks chunk-orchestrator progress for streaming analysis.
///
/// The six scalar fields are always emitted (in declaration order);
/// `aggregator_spills` is omitted when empty (metadata-layout contract).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamingState {
    /// Total number of commits in the analysis.
    #[serde(rename = "total_commits", default)]
    pub total_commits: i64,

    /// Number of commits processed so far.
    #[serde(rename = "processed_commits", default)]
    pub processed_commits: i64,

    /// Index of the chunk currently being processed.
    #[serde(rename = "current_chunk", default)]
    pub current_chunk: i64,

    /// Total number of chunks.
    #[serde(rename = "total_chunks", default)]
    pub total_chunks: i64,

    /// Hash of the last fully processed commit.
    #[serde(rename = "last_commit_hash", default)]
    pub last_commit_hash: String,

    /// Last processed tick (burndown time index).
    #[serde(rename = "last_tick", default)]
    pub last_tick: i64,

    /// Spill state of each aggregator at checkpoint time, indexed by analyzer
    /// position in the runner's analyzer list. Empty entries mean the analyzer
    /// has no aggregator (e.g. plumbing, `file_history`).
    ///
    /// Serialized only when non-empty.
    #[serde(
        rename = "aggregator_spills",
        default,
        skip_serializing_if = "Vec::is_empty"
    )]
    pub aggregator_spills: Vec<AggregatorSpillEntry>,
}

/// Checkpoint metadata used for validation and resume.
///
/// Field declaration order is frozen (it governs the emitted key order).
/// `checksums` is a map; using a [`BTreeMap`] makes its keys byte-sorted on
/// serialize (map-key ordering contract). Every field is always present (a
/// `null` `checksums`/`analyzers` round-trips to an empty map/vec on load).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    /// Checkpoint metadata format version (see [`MetadataVersion`](crate::MetadataVersion)).
    #[serde(rename = "version", default)]
    pub version: i64,

    /// Absolute path of the analyzed repository.
    #[serde(rename = "repo_path", default)]
    pub repo_path: String,

    /// Short hash of [`repo_path`](Metadata::repo_path) (see [`repo_hash`](crate::repo_hash)).
    #[serde(rename = "repo_hash", default)]
    pub repo_hash: String,

    /// RFC3339 UTC timestamp of checkpoint creation.
    #[serde(rename = "created_at", default)]
    pub created_at: String,

    /// Ordered list of analyzer names this checkpoint was created for.
    #[serde(rename = "analyzers", default, deserialize_with = "null_to_default")]
    pub analyzers: Vec<String>,

    /// Streaming progress at checkpoint time.
    #[serde(rename = "streaming_state", default)]
    pub streaming_state: StreamingState,

    /// Per-file checksums (byte-sorted keys on serialize).
    #[serde(rename = "checksums", default, deserialize_with = "null_to_default")]
    pub checksums: BTreeMap<String, String>,
}

/// `skip_serializing_if` predicate for `i64` zero values (omit-when-zero
/// metadata-layout rule).
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

/// Deserializes a JSON `null` into the type's `Default`, so metadata written
/// with null lists/maps loads as empty collections.
fn null_to_default<'de, D, T>(deserializer: D) -> std::result::Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    let opt = Option::<T>::deserialize(deserializer)?;
    Ok(opt.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::JsonCodec;

    // Mirrors TestStreamingState_JSONRoundTrip.
    #[test]
    fn streaming_state_json_round_trip() {
        let state = StreamingState {
            total_commits: 100_000,
            processed_commits: 50_000,
            current_chunk: 1,
            total_chunks: 2,
            last_commit_hash: "abc123def456".into(),
            last_tick: 42,
            aggregator_spills: Vec::new(),
        };
        let data = serde_json::to_vec(&state).unwrap();
        let restored: StreamingState = serde_json::from_slice(&data).unwrap();
        assert_eq!(state, restored);
    }

    // Mirrors TestMetadata_JSONRoundTrip.
    #[test]
    fn metadata_json_round_trip() {
        let mut checksums = BTreeMap::new();
        checksums.insert("file1.bin".to_string(), "sha256:abc".to_string());
        let meta = Metadata {
            version: 1,
            repo_path: "/home/user/repo".into(),
            repo_hash: "abc123".into(),
            created_at: String::new(),
            analyzers: vec!["burndown".into(), "devs".into()],
            streaming_state: StreamingState {
                total_commits: 100,
                processed_commits: 50,
                ..Default::default()
            },
            checksums: checksums.clone(),
        };
        let data = serde_json::to_vec(&meta).unwrap();
        let restored: Metadata = serde_json::from_slice(&data).unwrap();
        assert_eq!(meta.version, restored.version);
        assert_eq!(meta.repo_path, restored.repo_path);
        assert_eq!(meta.analyzers, restored.analyzers);
        assert_eq!(meta.checksums, restored.checksums);
    }

    // Mirrors TestMetadata_CreatedAt.
    #[test]
    fn metadata_created_at_round_trip() {
        let meta = Metadata {
            version: 1,
            created_at: "2026-02-05T12:00:00Z".into(),
            ..Default::default()
        };
        let data = serde_json::to_vec(&meta).unwrap();
        let restored: Metadata = serde_json::from_slice(&data).unwrap();
        assert_eq!(restored.created_at, "2026-02-05T12:00:00Z");
    }

    #[test]
    fn metadata_field_order_matches_declaration() {
        // Declaration order: version, repo_path, repo_hash, created_at,
        // analyzers, streaming_state, checksums. serde_json preserves struct
        // field order.
        let meta = Metadata {
            version: 2,
            repo_path: "/r".into(),
            repo_hash: "h".into(),
            created_at: "t".into(),
            analyzers: vec!["a".into()],
            streaming_state: StreamingState::default(),
            checksums: BTreeMap::new(),
        };
        let codec = JsonCodec::compact();
        let s = String::from_utf8(codec.to_vec(&meta).unwrap()).unwrap();
        let pos = |k: &str| s.find(k).unwrap();
        assert!(pos("\"version\"") < pos("\"repo_path\""));
        assert!(pos("\"repo_path\"") < pos("\"repo_hash\""));
        assert!(pos("\"repo_hash\"") < pos("\"created_at\""));
        assert!(pos("\"created_at\"") < pos("\"analyzers\""));
        assert!(pos("\"analyzers\"") < pos("\"streaming_state\""));
        assert!(pos("\"streaming_state\"") < pos("\"checksums\""));
    }

    #[test]
    fn empty_aggregator_spills_omitted() {
        let state = StreamingState::default();
        let s = serde_json::to_string(&state).unwrap();
        assert!(!s.contains("aggregator_spills"), "{s}");
    }

    #[test]
    fn empty_spill_entry_serializes_to_empty_object() {
        let entry = AggregatorSpillEntry::default();
        assert_eq!(serde_json::to_string(&entry).unwrap(), "{}");
    }

    #[test]
    fn checksums_keys_byte_sorted_on_serialize() {
        let mut checksums = BTreeMap::new();
        checksums.insert("b".to_string(), "2".to_string());
        checksums.insert("a".to_string(), "1".to_string());
        let meta = Metadata {
            checksums,
            ..Default::default()
        };
        let s = serde_json::to_string(&meta).unwrap();
        assert!(s.find("\"a\"").unwrap() < s.find("\"b\"").unwrap(), "{s}");
    }
}
