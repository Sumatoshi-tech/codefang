//! Unified time-series machine format.
//!
//! Port of `internal/analyzers/analyze/timeseries.go`. Builds a merged,
//! per-commit time-series from per-analyzer extracted data and serializes it
//! as either indented JSON (`timeseries`) or NDJSON (`timeseries+ndjson`).
//!
//! # Byte-identity
//!
//! * [`MergedCommitData`] flattens the four fixed metadata fields with the
//!   per-analyzer flag keys into a single `map[string]any` and marshals it,
//!   which Go's `encoding/json` then **byte-sorts**. We reproduce this by
//!   emitting a map-origin [`GoMap`] (sorted on encode by `cf-gojson`).
//! * [`MergedTimeSeries`] is a wrapper struct whose fields keep declaration
//!   order (`version`, `tick_size_hours`, `analyzers`, `commits`), so it is a
//!   struct-origin [`GoMap`].
//! * `tick_size_hours` is a float, rendered via Go's float formatter
//!   (`format_go_float`).

use std::collections::BTreeMap;
use std::io::{self, Write};

use cf_alg_mapx::build_lookup_set;
use cf_gojson::{Encoder, GoMap, GoValue};

/// Schema version for unified time-series output. Go `TimeSeriesModelVersion`.
pub const TIMESERIES_MODEL_VERSION: &str = "codefang.timeseries.v1";

/// Fallback tick duration in hours. Go `defaultTickSizeHours`.
const DEFAULT_TICK_SIZE_HOURS: f64 = 24.0;

/// Number of fixed metadata fields flattened with analyzer data. Go
/// `numCommitMetaFields`.
const NUM_COMMIT_META_FIELDS: usize = 4;

/// Per-commit metadata for time-series construction. Port of Go `CommitMeta`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CommitMeta {
    /// `hash` — commit hash (hex).
    pub hash: String,
    /// `timestamp` — RFC3339 commit timestamp.
    pub timestamp: String,
    /// `author` — commit author.
    pub author: String,
    /// `tick` — tick index.
    pub tick: i64,
}

/// Pairs an analyzer flag with its per-commit extracted data (hash → value).
/// Port of Go `AnalyzerData`.
#[derive(Debug, Clone, Default)]
pub struct AnalyzerData {
    /// The analyzer flag (e.g. `quality`).
    pub flag: String,
    /// Per-commit data keyed by commit hash.
    pub data: BTreeMap<String, GoValue>,
}

/// Merged analyzer data for a single commit. Port of Go `MergedCommitData`.
#[derive(Debug, Clone, Default)]
pub struct MergedCommitData {
    /// `hash` — commit hash.
    pub hash: String,
    /// `timestamp` — RFC3339 timestamp.
    pub timestamp: String,
    /// `author` — commit author.
    pub author: String,
    /// `tick` — tick index.
    pub tick: i64,
    /// Per-analyzer data keyed by flag (the Go `Analyzers map[string]any` with
    /// `json:"-"`, flattened into the parent object by `MarshalJSON`).
    pub analyzers: BTreeMap<String, GoValue>,
}

impl MergedCommitData {
    /// Flattens metadata and per-analyzer data into a single map-origin object
    /// (byte-sorted on encode). Port of Go `(MergedCommitData).MarshalJSON`.
    pub fn to_go_value(&self) -> GoValue {
        let mut flat = GoMap::new_map();
        // Capacity hint parity only; ordering comes from the map-origin sort.
        let _ = self.analyzers.len() + NUM_COMMIT_META_FIELDS;
        flat.push("hash", GoValue::Str(self.hash.clone()));
        flat.push("timestamp", GoValue::Str(self.timestamp.clone()));
        flat.push("author", GoValue::Str(self.author.clone()));
        flat.push("tick", GoValue::Int(self.tick));
        for (k, v) in &self.analyzers {
            flat.push(k.clone(), v.clone());
        }
        GoValue::Object(flat)
    }
}

/// Top-level unified time-series output. Port of Go `MergedTimeSeries`.
#[derive(Debug, Clone, Default)]
pub struct MergedTimeSeries {
    /// `version` — schema version.
    pub version: String,
    /// `tick_size_hours` — tick duration in hours.
    pub tick_size_hours: f64,
    /// `analyzers` — active analyzer flags.
    pub analyzers: Vec<String>,
    /// `commits` — per-commit merged data, ordered.
    pub commits: Vec<MergedCommitData>,
}

impl MergedTimeSeries {
    /// Encodes the wrapper as a struct-origin object (declaration order
    /// preserved): `version`, `tick_size_hours`, `analyzers`, `commits`.
    fn to_go_value(&self) -> GoValue {
        let mut obj = GoMap::new_struct();
        obj.push("version", GoValue::Str(self.version.clone()));
        obj.push("tick_size_hours", GoValue::Float(self.tick_size_hours));
        obj.push(
            "analyzers",
            GoValue::Array(self.analyzers.iter().cloned().map(GoValue::Str).collect()),
        );
        obj.push(
            "commits",
            GoValue::Array(self.commits.iter().map(MergedCommitData::to_go_value).collect()),
        );
        GoValue::Object(obj)
    }
}

/// Builds a unified time-series from pre-extracted per-analyzer commit data.
/// Port of Go `BuildMergedTimeSeriesDirect`.
pub fn build_merged_time_series_direct(
    active: &[AnalyzerData],
    commit_meta: &[CommitMeta],
    tick_size_hours: f64,
) -> MergedTimeSeries {
    let tick_size_hours = if tick_size_hours <= 0.0 {
        DEFAULT_TICK_SIZE_HOURS
    } else {
        tick_size_hours
    };

    let analyzer_names: Vec<String> = active.iter().map(|a| a.flag.clone()).collect();
    let commits = assemble_commits(active, commit_meta);

    MergedTimeSeries {
        version: TIMESERIES_MODEL_VERSION.to_string(),
        tick_size_hours,
        analyzers: analyzer_names,
        commits,
    }
}

/// Merges per-analyzer data into ordered [`MergedCommitData`]. Port of Go
/// `assembleCommits`.
fn assemble_commits(active: &[AnalyzerData], commit_meta: &[CommitMeta]) -> Vec<MergedCommitData> {
    let mut meta_by_hash: BTreeMap<String, &CommitMeta> = BTreeMap::new();
    for m in commit_meta {
        meta_by_hash.insert(m.hash.clone(), m);
    }

    let mut commit_hashes: Vec<String> = Vec::new();
    for a in active {
        for hash in a.data.keys() {
            commit_hashes.push(hash.clone());
        }
    }
    let commit_set = build_lookup_set(&commit_hashes);

    let ordered = order_commits_by_meta(commit_meta, &commit_set);
    let mut commits = Vec::with_capacity(ordered.len());
    for hash in ordered {
        let meta = meta_by_hash.get(&hash).copied().cloned().unwrap_or_default();
        let mut analyzer_map: BTreeMap<String, GoValue> = BTreeMap::new();
        for a in active {
            if let Some(v) = a.data.get(&hash) {
                analyzer_map.insert(a.flag.clone(), v.clone());
            }
        }
        commits.push(MergedCommitData {
            hash: meta.hash,
            timestamp: meta.timestamp,
            author: meta.author,
            tick: meta.tick,
            analyzers: analyzer_map,
        });
    }
    commits
}

/// Returns commit hashes in `meta` order, filtered to those in `commit_set`.
/// Port of Go `orderCommitsByMeta`.
fn order_commits_by_meta(
    meta: &[CommitMeta],
    commit_set: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut ordered = Vec::with_capacity(commit_set.len());
    for m in meta {
        if commit_set.contains(&m.hash) {
            ordered.push(m.hash.clone());
        }
    }
    ordered
}

/// Encodes a [`MergedTimeSeries`] as indented JSON. Port of Go
/// `WriteMergedTimeSeries` (`json.NewEncoder` + `SetIndent("", "  ")`).
pub fn write_merged_time_series(ts: &MergedTimeSeries, w: &mut dyn Write) -> io::Result<()> {
    let bytes = Encoder::indented("  ").encode_to_vec(&ts.to_go_value());
    w.write_all(&bytes)
}

/// Writes a [`MergedTimeSeries`] as NDJSON — one JSON line per commit. Port of
/// Go `WriteTimeSeriesNDJSON` (`json.NewEncoder`, compact, trailing newline per
/// line).
pub fn write_time_series_ndjson(ts: &MergedTimeSeries, w: &mut dyn Write) -> io::Result<()> {
    let enc = Encoder::encoder();
    for commit in &ts.commits {
        let bytes = enc.encode_to_vec(&commit.to_go_value());
        w.write_all(&bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(hash: &str, tick: i64) -> CommitMeta {
        CommitMeta {
            hash: hash.to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            author: "a@b".to_string(),
            tick,
        }
    }

    #[test]
    fn default_tick_size_applied() {
        let ts = build_merged_time_series_direct(&[], &[], 0.0);
        assert_eq!(ts.tick_size_hours, 24.0);
        assert_eq!(ts.version, TIMESERIES_MODEL_VERSION);
    }

    #[test]
    fn merged_commit_data_flattens_and_sorts() {
        let mcd = MergedCommitData {
            hash: "abc".into(),
            timestamp: "t".into(),
            author: "auth".into(),
            tick: 3,
            analyzers: {
                let mut m = BTreeMap::new();
                m.insert("quality".to_string(), GoValue::Int(5));
                m
            },
        };
        let out = cf_gojson::marshal(&mcd.to_go_value());
        // Keys byte-sorted: author, hash, quality, tick, timestamp.
        assert_eq!(
            out,
            br#"{"author":"auth","hash":"abc","quality":5,"tick":3,"timestamp":"t"}"#
        );
    }

    #[test]
    fn build_orders_commits_by_meta() {
        let active = vec![AnalyzerData {
            flag: "quality".into(),
            data: {
                let mut m = BTreeMap::new();
                m.insert("h2".to_string(), GoValue::Int(2));
                m.insert("h1".to_string(), GoValue::Int(1));
                m
            },
        }];
        let metas = vec![meta("h1", 0), meta("h2", 1)];
        let ts = build_merged_time_series_direct(&active, &metas, 24.0);
        assert_eq!(ts.commits.len(), 2);
        assert_eq!(ts.commits[0].hash, "h1");
        assert_eq!(ts.commits[1].hash, "h2");
    }

    #[test]
    fn write_indented_has_two_space_indent_and_trailing_newline() {
        let ts = build_merged_time_series_direct(&[], &[], 24.0);
        let mut buf = Vec::new();
        write_merged_time_series(&ts, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert!(s.starts_with("{\n  \"version\": \"codefang.timeseries.v1\""));
        assert!(s.ends_with("}\n"));
        // tick_size_hours float renders as integral "24" (Go 'g' formatting).
        assert!(s.contains("\"tick_size_hours\": 24"));
        assert!(!s.contains("24.0"));
    }

    #[test]
    fn write_ndjson_one_line_per_commit() {
        let active = vec![AnalyzerData {
            flag: "quality".into(),
            data: {
                let mut m = BTreeMap::new();
                m.insert("h1".to_string(), GoValue::Int(1));
                m
            },
        }];
        let ts = build_merged_time_series_direct(&active, &[meta("h1", 0)], 24.0);
        let mut buf = Vec::new();
        write_time_series_ndjson(&ts, &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        assert_eq!(s.lines().count(), 1);
        assert!(s.ends_with('\n'));
    }
}
