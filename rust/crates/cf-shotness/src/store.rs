//! Report-store record types and writing logic.
//!
//! Ports `internal/analyzers/shotness/store_writer.go` (and the record shapes
//! read back by `store_reader.go`). The store streams one `node_data` record per
//! node (in sorted node-key order) followed by a single `aggregate` record.
//!
//! The store codec itself (the [`ReportWriter`] trait) is defined here as the
//! minimal interface the shotness writer needs; the concrete persistence backend
//! lives in the not-yet-ported `cf-analyze`/`cf-persist` crates (see crate-level
//! todos). Record payloads are exposed as [`GoValue`] trees so whichever backend
//! is wired in serializes them through cf-gojson for byte parity.

use std::collections::HashMap;

use cf_gojson::{GoMap, GoValue};

use crate::aggregate::{build_report_from_merged, TickNodes};
use crate::metrics::compute_aggregate;
use crate::report::aggregate_to_value;
use crate::types::NodeSummary;

/// Store record kind for per-node data. Mirrors Go `KindNodeData`.
pub const KIND_NODE_DATA: &str = "node_data";
/// Store record kind for the aggregate. Mirrors Go `KindAggregate`.
pub const KIND_AGGREGATE: &str = "aggregate";

/// A single node's summary plus its co-change counter row.
///
/// Mirrors Go `NodeStoreRecord`. Counter keys are node indices into the ordered
/// node list; `counter[self]` is the self-change count.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeStoreRecord {
    /// Node identity.
    pub summary: NodeSummary,
    /// Co-change counter row keyed by node index.
    pub counter: HashMap<usize, i64>,
}

impl NodeStoreRecord {
    /// Serialize this record to a [`GoValue`] tree.
    ///
    /// `Summary` is a struct (Go field order: `Type`, `Name`, `File`).
    /// `Counter` is a `map[int]int`; Go's `encoding/json` stringifies integer
    /// keys and sorts them by their **UTF-8 byte order** (so `"10"` precedes
    /// `"2"`, NOT numeric order). We push stringified indices into a map-origin
    /// object and let cf-gojson's byte-sort reproduce Go's ordering exactly —
    /// confirmed against `json.Marshal(map[int]int{...})`.
    #[must_use]
    pub fn to_go_value(&self) -> GoValue {
        let mut summary = GoMap::new_struct();
        summary.push("Type", GoValue::Str(self.summary.type_.clone()));
        summary.push("Name", GoValue::Str(self.summary.name.clone()));
        summary.push("File", GoValue::Str(self.summary.file.clone()));

        // map-origin: cf-gojson byte-sorts the stringified int keys at encode
        // time, matching Go's map[int]int rendering.
        let mut counter = GoMap::new_map();
        for (idx, val) in &self.counter {
            counter.push(idx.to_string(), GoValue::Int(*val));
        }

        let mut root = GoMap::new_struct();
        root.push("Summary", GoValue::Map(summary));
        root.push("Counter", GoValue::Map(counter));
        GoValue::Map(root)
    }
}

/// Minimal report-store writer interface.
///
/// Mirrors the `analyze.ReportWriter` surface used by the shotness store writer.
/// The concrete implementation lives in `cf-analyze` (todo); this trait lets the
/// shotness logic be ported and tested in isolation.
pub trait ReportWriter {
    /// Write a single record of the given kind. The payload is a [`GoValue`]
    /// tree (serialized by the backend through cf-gojson for byte parity).
    fn write(&mut self, kind: &str, data: GoValue) -> Result<(), String>;
}

/// Streams shotness store records from merged per-tick node data.
///
/// Port of Go `(*Analyzer).WriteToStore` minus the framework plumbing: given the
/// already-merged tick node map (what `ticksToReport` produces internally), it
/// writes one `node_data` record per node (sorted by node key) then a single
/// `aggregate` record.
///
/// # Errors
///
/// Returns the underlying [`ReportWriter`] error, prefixed with the failing
/// record kind, matching Go's `fmt.Errorf("write %s: %w", ...)`.
pub fn write_to_store<W: ReportWriter>(merged: &TickNodes, w: &mut W) -> Result<(), String> {
    let report = build_report_from_merged(merged);

    for i in 0..report.nodes.len() {
        let rec = NodeStoreRecord {
            summary: report.nodes[i].clone(),
            counter: report.counters[i].clone(),
        };
        w.write(KIND_NODE_DATA, rec.to_go_value())
            .map_err(|e| format!("write {KIND_NODE_DATA}: {e}"))?;
    }

    let agg = compute_aggregate(&report);
    w.write(KIND_AGGREGATE, aggregate_to_value(&agg))
        .map_err(|e| format!("write {KIND_AGGREGATE}: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::{accumulate_nodes, compute_coupling_pairs};
    use cf_gojson::marshal::marshal;
    use std::collections::BTreeMap;

    struct FakeWriter {
        records: Vec<(String, GoValue)>,
    }

    impl ReportWriter for FakeWriter {
        fn write(&mut self, kind: &str, data: GoValue) -> Result<(), String> {
            self.records.push((kind.to_string(), data));
            Ok(())
        }
    }

    // Ported from TestWriteToStore_EmptyTicks: empty input → only the aggregate.
    #[test]
    fn write_to_store_empty_emits_only_aggregate() {
        let merged: TickNodes = BTreeMap::new();
        let mut w = FakeWriter { records: vec![] };
        write_to_store(&merged, &mut w).unwrap();
        assert_eq!(w.records.len(), 1);
        assert_eq!(w.records[0].0, KIND_AGGREGATE);
    }

    #[test]
    fn node_store_record_fields() {
        let rec = NodeStoreRecord {
            summary: NodeSummary::new("Function", "foo", "a.go"),
            counter: [(0usize, 5i64), (1usize, 2i64)].into_iter().collect(),
        };
        assert_eq!(rec.summary.type_, "Function");
        assert_eq!(rec.summary.name, "foo");
        assert_eq!(rec.counter[&0], 5);
        assert_eq!(rec.counter[&1], 2);
    }

    // Ported from TestWriteToStore_RoundTrip (record count + sorted order).
    #[test]
    fn write_to_store_two_nodes_emits_node_then_aggregate() {
        let foo = NodeSummary::new("Function", "foo", "a.go");
        let bar = NodeSummary::new("Function", "bar", "a.go");
        let mut acc: TickNodes = BTreeMap::new();
        let t: BTreeMap<String, NodeSummary> =
            [(foo.key(), foo.clone()), (bar.key(), bar.clone())].into_iter().collect();
        accumulate_nodes(&mut acc, &t);
        compute_coupling_pairs(&mut acc, &t);

        let mut w = FakeWriter { records: vec![] };
        write_to_store(&acc, &mut w).unwrap();

        // Two node_data records (sorted: bar then foo) + one aggregate.
        assert_eq!(w.records.len(), 3);
        assert_eq!(w.records[0].0, KIND_NODE_DATA);
        assert_eq!(w.records[1].0, KIND_NODE_DATA);
        assert_eq!(w.records[2].0, KIND_AGGREGATE);

        let first = String::from_utf8(marshal(&w.records[0].1)).unwrap();
        assert!(first.contains(r#""Name":"bar""#), "got: {first}");
    }

    // Go renders map[int]int keys as strings byte-sorted: "10" precedes "2".
    // Confirmed against json.Marshal(map[int]int{0:5,1:9,10:1,2:3}).
    #[test]
    fn counter_int_keys_byte_sorted_like_go() {
        let rec = NodeStoreRecord {
            summary: NodeSummary::new("Function", "foo", "a.go"),
            counter: [(0usize, 5i64), (1, 9), (10, 1), (2, 3)].into_iter().collect(),
        };
        let s = String::from_utf8(marshal(&rec.to_go_value())).unwrap();
        assert!(
            s.contains(r#""Counter":{"0":5,"1":9,"10":1,"2":3}"#),
            "got: {s}"
        );
    }
}
