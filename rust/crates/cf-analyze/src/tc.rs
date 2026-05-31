//! Per-commit (`TC`) and aggregated tick-level (`TICK`) result carriers.
//!
//! Port of `internal/analyzers/analyze/tc.go`. The commit hash and timestamps
//! cross the serialization boundary (NDJSON / timeseries), so the types mirror
//! the Go fields exactly. `gitlib.Hash` is abstracted as [`CommitHash`] — a
//! 40-hex-char wrapper — to avoid linking the (unported) `cf-gitlib` crate;
//! downstream code can convert to/from the real hash type.

use std::time::SystemTime;

/// A commit hash, the analyze-layer stand-in for `gitlib.Hash`.
///
/// Holds the lowercase 40-character hex string. `gitlib.NewHash(s)` /
/// `Hash.String()` round-trip to this wrapper; [`CommitHash::as_str`] is what
/// the NDJSON `hash` field serializes (`tc.CommitHash.String()`,
/// streaming_sink.go:51).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct CommitHash(pub String);

impl CommitHash {
    /// Wraps a hex hash string. Mirrors `gitlib.NewHash`.
    #[must_use]
    pub fn new(hex: impl Into<String>) -> Self {
        Self(hex.into())
    }

    /// The hex string. Mirrors `gitlib.Hash.String()`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CommitHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A per-commit result emitted by a `HistoryAnalyzer`.
///
/// Mirrors Go's `TC` (tc.go:13). Each `consume` produces one `TC`. `data` holds
/// an analyzer-specific payload as a [`cf_gojson::GoValue`] so it serializes
/// byte-identically through the shared encoder; `None` (Go's nil `Data`)
/// signals "no per-commit output" (plumbing analyzers).
#[derive(Debug, Clone, Default)]
pub struct Tc {
    /// Identifies the analyzed commit (`CommitHash`).
    pub commit_hash: CommitHash,
    /// Time-bucket index this commit belongs to (`Tick`).
    pub tick: i32,
    /// Numeric identity of the commit author (`AuthorID`).
    pub author_id: i32,
    /// The commit's author time (`Timestamp`). `None` mirrors Go's zero `time.Time`.
    pub timestamp: Option<SystemTime>,
    /// Analyzer-specific per-commit payload (`Data`). `None` is Go's nil.
    pub data: Option<cf_gojson::GoValue>,
}

/// An aggregated tick-level result produced by an `Aggregator`.
///
/// Mirrors Go's `TICK` (tc.go:34): the merged output of all `TC`s within one
/// time bucket. The Rust name is [`Tick`] to satisfy naming conventions; it is
/// the `TICK` of the Go API.
#[derive(Debug, Clone, Default)]
pub struct Tick {
    /// Time-bucket index (`Tick`).
    pub tick: i32,
    /// Earliest commit timestamp in this tick (`StartTime`).
    pub start_time: Option<SystemTime>,
    /// Latest commit timestamp in this tick (`EndTime`).
    pub end_time: Option<SystemTime>,
    /// Analyzer-specific aggregated payload (`Data`).
    pub data: Option<cf_gojson::GoValue>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_hash_round_trips() {
        let h = CommitHash::new("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(h.as_str(), "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(h.to_string(), "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    }

    #[test]
    fn tc_default_has_no_data() {
        let tc = Tc::default();
        assert!(tc.data.is_none());
        assert!(tc.timestamp.is_none());
    }
}
