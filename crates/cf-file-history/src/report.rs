//! Report-format rendering of the file-history metrics.
//!
//! All machine-format report bytes are produced through [`cf_gojson`] (never
//! raw serde); the bytes are pinned against the reference binary by
//! `tests/compat`.
//!
//! The wrapper structs ([`ComputedMetrics`] and its fields) are emitted in
//! **declaration order** (a struct-origin [`cf_gojson::GoMap`]). The
//! composition `breakdown` / `percentages` maps and the per-tick composition
//! `breakdown` are emitted as map-origin `GoMap`s (byte-sorted keys), per the
//! report-format contract for string-keyed maps. Empty `start_time` /
//! `end_time` are omitted (`omitempty`).
//!
//! The compact form here ([`to_compact_json`]) is the `bin`/ndjson payload
//! shape (no indent, HTML-escape on, no trailing newline). The indented `json`
//! run/render form (two-space indent) is produced by the cross-format
//! conversion hub (`cf-analyze`) over the same [`GoValue`] tree returned by
//! [`computed_metrics_to_go`].

use cf_gojson::{marshal, Encoder, GoMap, GoValue};

use crate::metrics::{
    AggregateData, CompositionData, CompositionTimeSeriesEntry, ComputedMetrics, ContributorEntry,
    FileChurnData, FileContributorData, HotspotData,
};

/// Converts [`ComputedMetrics`] into its report-shape [`GoValue`].
///
/// Field order: `file_churn`, `file_contributors`, `hotspots`, `aggregate`,
/// `composition`, `composition_ts`.
#[must_use]
pub fn computed_metrics_to_go(m: &ComputedMetrics) -> GoValue {
    let mut obj = GoMap::new_struct();
    obj.push(
        "file_churn",
        GoValue::Array(m.file_churn.iter().map(file_churn_to_go).collect()),
    );
    obj.push(
        "file_contributors",
        GoValue::Array(
            m.file_contributors
                .iter()
                .map(file_contributor_to_go)
                .collect(),
        ),
    );
    obj.push(
        "hotspots",
        GoValue::Array(m.hotspots.iter().map(hotspot_to_go).collect()),
    );
    obj.push("aggregate", aggregate_to_go(&m.aggregate));
    obj.push("composition", composition_to_go(&m.composition));
    obj.push(
        "composition_ts",
        GoValue::Array(m.composition_ts.iter().map(composition_ts_to_go).collect()),
    );
    GoValue::Object(obj)
}

fn file_churn_to_go(f: &FileChurnData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("path", GoValue::Str(f.path.clone()));
    o.push("commit_count", GoValue::Int(f.commit_count));
    o.push("contributor_count", GoValue::Int(f.contributor_count));
    o.push("total_lines_added", GoValue::Int(f.total_added));
    o.push("total_lines_removed", GoValue::Int(f.total_removed));
    o.push("total_lines_changed", GoValue::Int(f.total_changed));
    o.push("churn_score", GoValue::Float(f.churn_score));
    GoValue::Object(o)
}

fn file_contributor_to_go(f: &FileContributorData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("path", GoValue::Str(f.path.clone()));
    o.push(
        "contributors",
        GoValue::Array(f.contributors.iter().map(contributor_entry_to_go).collect()),
    );
    o.push("top_contributor_id", GoValue::Int(f.top_contributor_id));
    o.push(
        "top_contributor_lines",
        GoValue::Int(f.top_contributor_lines),
    );
    GoValue::Object(o)
}

fn contributor_entry_to_go(c: &ContributorEntry) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("dev_id", GoValue::Int(c.dev_id));
    o.push("added", GoValue::Int(c.added));
    o.push("removed", GoValue::Int(c.removed));
    o.push("changed", GoValue::Int(c.changed));
    GoValue::Object(o)
}

fn hotspot_to_go(h: &HotspotData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("path", GoValue::Str(h.path.clone()));
    o.push("commit_count", GoValue::Int(h.commit_count));
    o.push("churn_score", GoValue::Float(h.churn_score));
    o.push("risk_level", GoValue::Str(h.risk_level.clone()));
    GoValue::Object(o)
}

fn aggregate_to_go(a: &AggregateData) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("total_files", GoValue::Int(a.total_files));
    o.push("total_commits", GoValue::Int(a.total_commits));
    o.push("total_contributors", GoValue::Int(a.total_contributors));
    o.push(
        "avg_commits_per_file",
        GoValue::Float(a.avg_commits_per_file),
    );
    o.push(
        "avg_contributors_per_file",
        GoValue::Float(a.avg_contributors_per_file),
    );
    o.push("high_churn_files", GoValue::Int(a.high_churn_files));
    GoValue::Object(o)
}

fn composition_to_go(c: &CompositionData) -> GoValue {
    let mut o = GoMap::new_struct();
    // breakdown / percentages are map-origin (byte-sorted keys).
    let mut breakdown = GoMap::new_map();
    for (k, v) in &c.breakdown {
        breakdown.push(k.clone(), GoValue::Int(*v));
    }
    let mut percentages = GoMap::new_map();
    for (k, v) in &c.percentages {
        percentages.push(k.clone(), GoValue::Float(*v));
    }
    o.push("breakdown", GoValue::Object(breakdown));
    o.push("percentages", GoValue::Object(percentages));
    GoValue::Object(o)
}

fn composition_ts_to_go(e: &CompositionTimeSeriesEntry) -> GoValue {
    let mut o = GoMap::new_struct();
    o.push("tick", GoValue::Int(e.tick));
    // start_time / end_time are omitempty: only emit when non-empty.
    if !e.start_time.is_empty() {
        o.push("start_time", GoValue::Str(e.start_time.clone()));
    }
    if !e.end_time.is_empty() {
        o.push("end_time", GoValue::Str(e.end_time.clone()));
    }
    let mut breakdown = GoMap::new_map();
    for (k, v) in &e.breakdown {
        breakdown.push(k.clone(), GoValue::Int(*v));
    }
    o.push("breakdown", GoValue::Object(breakdown));
    GoValue::Object(o)
}

/// Renders [`ComputedMetrics`] to compact report-format JSON bytes (the
/// `bin` / ndjson payload form: no indent, HTML-escape on, no trailing
/// newline).
#[must_use]
pub fn to_compact_json(m: &ComputedMetrics) -> Vec<u8> {
    marshal(&computed_metrics_to_go(m))
}

/// Renders [`ComputedMetrics`] to a compact report-format JSON string
/// (convenience over [`to_compact_json`], using the [`Encoder`] builder).
#[must_use]
pub fn to_compact_json_string(m: &ComputedMetrics) -> String {
    Encoder::marshal().encode_to_string(&computed_metrics_to_go(m))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_metrics_compact_json_field_order() {
        // Field declaration order must be preserved in compact output, and
        // empty slices render as [] (report contract), empty maps as {}.
        let m = ComputedMetrics::default();
        let s = to_compact_json_string(&m);
        assert_eq!(
            s,
            "{\"file_churn\":[],\"file_contributors\":[],\"hotspots\":[],\
\"aggregate\":{\"total_files\":0,\"total_commits\":0,\"total_contributors\":0,\
\"avg_commits_per_file\":0,\"avg_contributors_per_file\":0,\"high_churn_files\":0},\
\"composition\":{\"breakdown\":{},\"percentages\":{}},\"composition_ts\":[]}"
        );
    }

    #[test]
    fn aggregate_integer_floats_render_without_decimal() {
        // The report contract renders 15.0 as "15" (no decimal point).
        let m = ComputedMetrics {
            aggregate: AggregateData {
                total_files: 2,
                total_commits: 30,
                total_contributors: 3,
                avg_commits_per_file: 15.0,
                avg_contributors_per_file: 2.0,
                high_churn_files: 1,
            },
            ..Default::default()
        };
        let s = to_compact_json_string(&m);
        assert!(s.contains("\"avg_commits_per_file\":15"), "{s}");
        assert!(s.contains("\"avg_contributors_per_file\":2"), "{s}");
        assert!(!s.contains("15.0"), "{s}");
    }

    #[test]
    fn compact_bytes_match_string() {
        let m = ComputedMetrics::default();
        assert_eq!(to_compact_json(&m), to_compact_json_string(&m).into_bytes());
    }
}
