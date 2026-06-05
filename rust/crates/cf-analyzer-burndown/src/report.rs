//! Burndown report model and byte-identical serialization.
//!
//! The Go report is `analyze.Report = map[string]any` (see
//! `internal/analyzers/burndown/aggregator.go::ticksToReport`). The dominant
//! byte-order rule is therefore **map-key byte sorting at encode time**, which
//! [`cf_gojson::MapOrigin::Map`] applies. Every dynamic map in this module is
//! built as a map-origin [`GoMap`] so the encoder sorts keys exactly like Go's
//! `encoding/json`.
//!
//! # Report shape (mirrors `ticksToReport`)
//!
//! The internal report carries these keys (byte-sorted on encode):
//!
//! * `GlobalHistory` — the global dense burndown matrix (`DenseHistory =
//!   [][]int64`): rows = sampling slot, columns = granularity band.
//! * `ReversedPeopleDict` — `[]string`, author id → name.
//! * `TickSize` — Go `time.Duration`, which `encoding/json` marshals as its
//!   **int64 nanosecond count** (`time.Duration` has no `MarshalJSON`).
//! * `Sampling`, `Granularity` — `int`.
//! * `EndTime` — Go `time.Time`, marshalled via `time.Time.MarshalJSON` as an
//!   **RFC3339Nano string** (see [`crate::report::format_rfc3339_from_unix`] —
//!   note the value is commit-derived, so deterministic given the repo).
//! * `PeopleHistories` (`[]DenseHistory`) and `PeopleMatrix` (`DenseHistory`) —
//!   present only when `peopleNumber > 0` and people history is non-empty.
//! * `FileHistories` (`map[string]DenseHistory`) and `FileOwnership`
//!   (`map[string]map[int]int`) — present only when `trackFiles` and non-empty;
//!   files are emitted in **sorted `PathID`** order in Go, but JSON re-sorts the
//!   resulting `map[string]…` by key bytes anyway.
//! * `commit_stats` (`map[string]*CommitSummary`) and `commits_by_tick`
//!   (`map[int][]gitlib.Hash`) — present only when there are per-commit stats.
//!
//! `CommitSummary` is a Go struct with `json:"lines_added"` /
//! `json:"lines_removed"` (declaration order), so it is built as a
//! **struct-origin** [`GoMap`].
//!
//! # Status
//!
//! This module owns the byte-identity-critical *serialization* of the burndown
//! report. The values themselves are produced by the (not-yet-ported) history
//! walk in [`cf_burndown_core`] + the aggregator; this struct is the typed
//! hand-off the walk will populate. See the crate-level `todos`.

use std::collections::BTreeMap;

use cf_gojson::{marshal, marshal_indent, GoMap, GoValue, MapOrigin};

/// A dense burndown matrix. Mirrors Go `DenseHistory = [][]int64`: outer =
/// sampling slots, inner = granularity bands.
pub type DenseHistory = Vec<Vec<i64>>;

/// Per-commit summary, mirroring Go `CommitSummary` (declaration order:
/// `lines_added`, then `lines_removed`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommitSummary {
    /// New lines introduced by the commit (`json:"lines_added"`).
    pub lines_added: i64,
    /// Lines removed by the commit (`json:"lines_removed"`).
    pub lines_removed: i64,
}

impl CommitSummary {
    /// Build the struct-origin value (fields in declaration order).
    #[must_use]
    pub fn to_govalue(&self) -> GoValue {
        let mut m = GoMap::new(MapOrigin::Struct);
        m.push("lines_added", GoValue::Int(self.lines_added));
        m.push("lines_removed", GoValue::Int(self.lines_removed));
        GoValue::Map(m)
    }
}

/// Per-file burndown history, paired with its resolved path name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileHistory {
    /// Resolved file path (the interned `PathID` mapped back to a string).
    pub path: String,
    /// Dense burndown matrix for this file.
    pub matrix: DenseHistory,
    /// `authorID -> surviving line count` snapshot for this file. Empty when
    /// people tracking is off.
    pub ownership: BTreeMap<i64, i64>,
}

/// Full burndown report, the typed analogue of Go's `analyze.Report` map for
/// the burndown analyzer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BurndownReport {
    /// `GlobalHistory`.
    pub global_history: DenseHistory,
    /// `ReversedPeopleDict`: author id → name.
    pub reversed_people_dict: Vec<String>,
    /// `TickSize` as a Go `time.Duration` nanosecond count.
    pub tick_size_nanos: i64,
    /// `Sampling`.
    pub sampling: i64,
    /// `Granularity`.
    pub granularity: i64,
    /// `EndTime` as an RFC3339(Nano) string (already formatted the Go way), or
    /// `None` to emit the zero-time value Go would (`"0001-01-01T00:00:00Z"`).
    pub end_time: Option<String>,
    /// `PeopleHistories`: indexed by author id.
    pub people_histories: Vec<DenseHistory>,
    /// `PeopleMatrix`: author-interaction matrix.
    pub people_matrix: DenseHistory,
    /// `FileHistories` + `FileOwnership`, keyed by resolved path. Empty unless
    /// file tracking is enabled.
    pub files: Vec<FileHistory>,
    /// `commit_stats`: commit hash → summary.
    pub commit_stats: BTreeMap<String, CommitSummary>,
    /// `commits_by_tick`: tick → list of commit hash strings.
    pub commits_by_tick: BTreeMap<i64, Vec<String>>,
}

/// Go's `time.Time` zero value, as `time.Time.MarshalJSON` renders it.
pub const ZERO_TIME_RFC3339: &str = "0001-01-01T00:00:00Z";

/// Encode a dense matrix as a JSON array-of-arrays of integers.
fn matrix_to_govalue(m: &DenseHistory) -> GoValue {
    GoValue::Array(
        m.iter()
            .map(|row| GoValue::Array(row.iter().map(|&v| GoValue::Int(v)).collect()))
            .collect(),
    )
}

impl BurndownReport {
    /// Build the Go-compatible value tree. This is a **map-origin** object, so
    /// keys are byte-sorted by the encoder regardless of insertion order — which
    /// is exactly what Go's `encoding/json` does for `map[string]any`.
    #[must_use]
    pub fn to_govalue(&self) -> GoValue {
        let mut m = GoMap::new(MapOrigin::Map);

        m.push("GlobalHistory", matrix_to_govalue(&self.global_history));
        m.push(
            "ReversedPeopleDict",
            GoValue::Array(
                self.reversed_people_dict
                    .iter()
                    .map(|s| GoValue::Str(s.clone()))
                    .collect(),
            ),
        );
        // time.Duration marshals as its int64 nanosecond count.
        m.push("TickSize", GoValue::Int(self.tick_size_nanos));
        m.push("Sampling", GoValue::Int(self.sampling));
        m.push("Granularity", GoValue::Int(self.granularity));
        m.push(
            "EndTime",
            GoValue::Str(
                self.end_time
                    .clone()
                    .unwrap_or_else(|| ZERO_TIME_RFC3339.to_string()),
            ),
        );

        if !self.people_histories.is_empty() {
            m.push(
                "PeopleHistories",
                GoValue::Array(self.people_histories.iter().map(matrix_to_govalue).collect()),
            );
            m.push("PeopleMatrix", matrix_to_govalue(&self.people_matrix));
        }

        if !self.files.is_empty() {
            let mut histories = GoMap::new(MapOrigin::Map);
            let mut ownership = GoMap::new(MapOrigin::Map);
            for f in &self.files {
                histories.insert(f.path.clone(), matrix_to_govalue(&f.matrix));
                if !f.ownership.is_empty() {
                    let mut own = GoMap::new(MapOrigin::Map);
                    for (&author, &lines) in &f.ownership {
                        // map[int]int keys marshal as their decimal string.
                        own.insert(author.to_string(), GoValue::Int(lines));
                    }
                    ownership.insert(f.path.clone(), GoValue::Map(own));
                }
            }
            m.push("FileHistories", GoValue::Map(histories));
            m.push("FileOwnership", GoValue::Map(ownership));
        }

        if !self.commit_stats.is_empty() {
            let mut stats = GoMap::new(MapOrigin::Map);
            for (hash, cs) in &self.commit_stats {
                stats.insert(hash.clone(), cs.to_govalue());
            }
            m.push("commit_stats", GoValue::Map(stats));

            let mut by_tick = GoMap::new(MapOrigin::Map);
            for (&tick, hashes) in &self.commits_by_tick {
                by_tick.insert(
                    tick.to_string(),
                    GoValue::Array(hashes.iter().map(|h| GoValue::Str(h.clone())).collect()),
                );
            }
            m.push("commits_by_tick", GoValue::Map(by_tick));
        }

        GoValue::Map(m)
    }

    /// `json` machine format: indented (`SetIndent("", "  ")`), HTML-escape ON,
    /// trailing newline. See DESIGN §2.3.
    #[must_use]
    pub fn to_json(&self) -> Vec<u8> {
        let mut out = marshal_indent(&self.to_govalue(), "", "  ");
        out.push(b'\n');
        out
    }

    /// Compact JSON payload (HTML-escape ON, no trailing newline) — the form
    /// used inside the `bin`/`binary` CFB1 envelope and for `ndjson` lines.
    #[must_use]
    pub fn to_compact_json(&self) -> Vec<u8> {
        marshal(&self.to_govalue())
    }

    /// `bin`/`binary` machine format: a single CFB1 envelope wrapping the
    /// compact-JSON payload. See DESIGN §2.5.
    ///
    /// # Errors
    ///
    /// Returns [`cf_reportutil::EncodeError`] if the payload exceeds the CFB1
    /// 4-GiB limit (mirrors Go's `ErrBinaryPayloadTooLarge`).
    pub fn to_binary(&self) -> Result<Vec<u8>, cf_reportutil::EncodeError> {
        cf_reportutil::encode_binary_envelope(&self.to_govalue())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> BurndownReport {
        BurndownReport {
            global_history: vec![vec![10, 0], vec![5, 3]],
            reversed_people_dict: vec!["alice".into(), "bob".into()],
            tick_size_nanos: 24 * 3_600_000_000_000, // 24h in ns
            sampling: 1,
            granularity: 1,
            end_time: Some("2024-01-01T00:00:00Z".into()),
            ..Default::default()
        }
    }

    #[test]
    fn json_keys_are_byte_sorted() {
        let s = String::from_utf8(sample().to_compact_json()).unwrap();
        // Byte order of capitalized keys: EndTime < GlobalHistory <
        // Granularity < ReversedPeopleDict < Sampling < TickSize.
        let positions = [
            s.find("\"EndTime\"").unwrap(),
            s.find("\"GlobalHistory\"").unwrap(),
            s.find("\"Granularity\"").unwrap(),
            s.find("\"ReversedPeopleDict\"").unwrap(),
            s.find("\"Sampling\"").unwrap(),
            s.find("\"TickSize\"").unwrap(),
        ];
        for w in positions.windows(2) {
            assert!(w[0] < w[1], "keys must be byte-sorted: {s}");
        }
    }

    #[test]
    fn tick_size_is_nanoseconds_integer() {
        let s = String::from_utf8(sample().to_compact_json()).unwrap();
        assert!(
            s.contains("\"TickSize\":86400000000000"),
            "duration marshals as ns int64: {s}"
        );
    }

    #[test]
    fn end_time_is_rfc3339_string() {
        let s = String::from_utf8(sample().to_compact_json()).unwrap();
        assert!(s.contains("\"EndTime\":\"2024-01-01T00:00:00Z\""));
    }

    #[test]
    fn zero_end_time_uses_go_zero_value() {
        let mut r = sample();
        r.end_time = None;
        let s = String::from_utf8(r.to_compact_json()).unwrap();
        assert!(s.contains("\"EndTime\":\"0001-01-01T00:00:00Z\""));
    }

    #[test]
    fn json_is_indented_with_trailing_newline() {
        let bytes = sample().to_json();
        assert!(bytes.ends_with(b"\n"), "indented JSON has a trailing newline");
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("  \"GlobalHistory\""), "two-space indent: {s}");
    }

    #[test]
    fn compact_json_has_no_trailing_newline_or_spaces() {
        let bytes = sample().to_compact_json();
        assert!(!bytes.ends_with(b"\n"));
        let s = String::from_utf8(bytes).unwrap();
        assert!(!s.contains(": "), "compact: no space after colon: {s}");
        assert!(!s.contains(", "), "compact: no space after comma: {s}");
    }

    #[test]
    fn binary_wraps_compact_json_in_cfb1() {
        let r = sample();
        let env = r.to_binary().expect("encode");
        let payload = r.to_compact_json();
        assert_eq!(&env[0..4], &cf_reportutil::BINARY_MAGIC[..], "CFB1 magic");
        let len = u32::from_le_bytes([env[4], env[5], env[6], env[7]]) as usize;
        assert_eq!(len, payload.len(), "LE u32 payload length");
        assert_eq!(&env[8..], &payload[..], "payload is the compact JSON");
    }

    #[test]
    fn omits_people_files_commit_keys_when_empty() {
        let s = String::from_utf8(sample().to_compact_json()).unwrap();
        assert!(!s.contains("PeopleHistories"));
        assert!(!s.contains("FileHistories"));
        assert!(!s.contains("commit_stats"));
        assert!(!s.contains("commits_by_tick"));
    }

    #[test]
    fn includes_files_with_ownership_when_present() {
        let mut r = sample();
        r.files = vec![FileHistory {
            path: "src/main.rs".into(),
            matrix: vec![vec![1]],
            ownership: BTreeMap::from([(0, 5), (1, 2)]),
        }];
        let s = String::from_utf8(r.to_compact_json()).unwrap();
        assert!(s.contains("\"FileHistories\""));
        assert!(s.contains("src/main.rs"));
        assert!(s.contains("\"FileOwnership\""));
        // map[int]int keys marshal as decimal strings, byte-sorted ("0" < "1").
        assert!(s.contains(r#"{"0":5,"1":2}"#), "ownership: {s}");
    }

    #[test]
    fn includes_people_when_present() {
        let mut r = sample();
        r.people_histories = vec![vec![vec![2]]];
        r.people_matrix = vec![vec![0, 0, 2]];
        let s = String::from_utf8(r.to_compact_json()).unwrap();
        assert!(s.contains("\"PeopleHistories\""));
        assert!(s.contains("\"PeopleMatrix\""));
    }

    #[test]
    fn includes_commit_stats_struct_order() {
        let mut r = sample();
        r.commit_stats = BTreeMap::from([(
            "abc".to_string(),
            CommitSummary {
                lines_added: 7,
                lines_removed: 3,
            },
        )]);
        r.commits_by_tick = BTreeMap::from([(0, vec!["abc".to_string()])]);
        let s = String::from_utf8(r.to_compact_json()).unwrap();
        // CommitSummary is a struct: declaration order lines_added,lines_removed.
        assert!(s.contains(r#""abc":{"lines_added":7,"lines_removed":3}"#), "{s}");
        assert!(s.contains(r#""commits_by_tick":{"0":["abc"]}"#), "{s}");
    }
}
