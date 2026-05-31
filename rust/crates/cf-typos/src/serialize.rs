//! Typos report serialization.
//!
//! Port of the Go `typos` analyzer `Serialize` / `SerializeTICKs` paths. The
//! analyzer's serialized output is its metric set (typo_list, patterns,
//! file_typos, aggregate), emitted through the shared Go-byte-compatible
//! encoders via [`cf_analyze::serialize::serialize_report`] so MACHINE formats
//! (json/yaml/ndjson/timeseries/compact/bin) are byte-identical to Go.

use cf_analyze::formats::Format;
use cf_analyze::report::Report;
use anyhow::Result;

use crate::metrics::{metrics_report, ReportData};
use crate::typos::Typo;

/// Builds the serializable report (the four metrics) from the typos input.
///
/// Port of the metrics-output assembly that the Go `Serialize` path runs before
/// encoding. The result is a [`Report`] keyed by metric name; serialize it with
/// [`serialize`].
pub fn build_report(typos: &[Typo]) -> Report {
    let input = ReportData {
        typos: typos.to_vec(),
    };
    metrics_report(&input)
}

/// Serializes the typos metrics report to `out` in the given format.
///
/// Routes through the shared serializer so output bytes match Go.
pub fn serialize(typos: &[Typo], format: Format, out: &mut Vec<u8>) -> Result<()> {
    let report = build_report(typos);
    cf_analyze::serialize::serialize_report(&report, format, out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_gitlib::Hash;
    use cf_gojson::GoValue;

    fn typo(wrong: &str, correct: &str, file: &str, line: i64) -> Typo {
        Typo {
            wrong: wrong.to_string(),
            correct: correct.to_string(),
            file: file.to_string(),
            commit: Hash::default(),
            line,
        }
    }

    #[test]
    fn build_report_has_four_metrics() {
        let report = build_report(&[typo("tets", "test", "main.go", 10)]);
        assert!(report.contains_key("typo_list"));
        assert!(report.contains_key("patterns"));
        assert!(report.contains_key("file_typos"));
        assert!(report.contains_key("aggregate"));
    }

    #[test]
    fn build_report_aggregate_total() {
        let report = build_report(&[
            typo("tets", "test", "main.go", 10),
            typo("functon", "function", "util.go", 20),
        ]);
        let GoValue::Struct(fields) = report.get("aggregate").unwrap() else {
            panic!("aggregate should be struct");
        };
        let total = fields
            .iter()
            .find(|(k, _)| k == "total_typos")
            .map(|(_, v)| v)
            .unwrap();
        assert_eq!(*total, GoValue::Int(2));
    }

    #[test]
    fn serialize_json_round_trips() {
        let mut out = Vec::new();
        serialize(&[typo("tets", "test", "main.go", 10)], Format::Json, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        // Indented JSON, contains the metric keys; trailing newline.
        assert!(s.contains("typo_list"));
        assert!(s.contains("aggregate"));
        assert!(s.ends_with('\n'));
    }

    #[test]
    fn serialize_empty() {
        let mut out = Vec::new();
        serialize(&[], Format::Json, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("typo_list"));
    }
}
