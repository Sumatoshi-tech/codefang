//! cf-shotness — structural hotspots.
//!
//! Co-change frequency of DSL-selected UAST entities (functions, classes, …)
//! across commit history. Analyzer id `history/shotness`.
//!
//! # What this crate covers
//!
//! - **Node identity & last-wins collision.** [`types::NodeSummary`] derives the
//!   canonical key `Type + "_" + Name + "_" + File`. When DSL name extraction
//!   yields the same name for multiple nodes the last one wins (resolved by the
//!   upstream UAST extraction step; see crate todos).
//! - **Additive merge.** [`aggregate`] accumulates per-commit node touches into
//!   per-tick state, derives coupling pairs from sorted touched-node keys, and
//!   merges tick/fork state purely additively (counts and coupling counters
//!   add). The merged data flattens into the index-keyed `Nodes`/`Counters`
//!   report.
//! - **Metrics.** [`metrics`] computes hotness, coupling strength
//!   (`co / max(co, a, b)`), risk classification, and the aggregate summary.
//! - **Machine-format report.** [`report`] assembles the
//!   [`metrics::ComputedMetrics`] tree as ordered [`cf_gojson::GoValue`] so
//!   the json / yaml / ndjson / timeseries / compact / bin outputs follow the
//!   report-format contract. All serialization routes through cf-gojson,
//!   never serde.
//! - **Report store.** [`store`] emits the `node_data` / `aggregate` record
//!   stream.
//!
//! # Parity scope
//!
//! Machine formats (json, yaml, ndjson, timeseries, timeseries+ndjson,
//! compact, bin) are byte-identity targets, pinned against the reference
//! implementation by `tests/compat`. Terminal text and plot output are
//! cosmetic and live elsewhere (see crate todos).

pub mod aggregate;
pub mod metrics;
pub mod report;
pub mod store;
pub mod types;

pub use metrics::{
    classify_change_risk, compute_aggregate, compute_all_metrics, compute_coupling_strength,
    compute_hotspot_nodes, compute_node_coupling, compute_node_hotness, AggregateData,
    ComputedMetrics, HotspotNodeData, NodeCouplingData, NodeHotnessData,
};
pub use report::{aggregate_to_value, CommitSummary};
pub use store::{write_to_store, NodeStoreRecord, ReportWriter, KIND_AGGREGATE, KIND_NODE_DATA};
pub use types::{NodeSummary, ReportData};

/// Analyzer identifier as used in the report registry and CLI.
pub const ANALYZER_ID: &str = "history/shotness";

/// Default DSL expression for selecting code structures.
pub const DEFAULT_SHOTNESS_DSL_STRUCT: &str = r#"filter(.roles has "Function")"#;
/// Default DSL expression for extracting names.
pub const DEFAULT_SHOTNESS_DSL_NAME: &str = ".props.name";

/// Configuration key for the DSL structure expression.
pub const CONFIG_SHOTNESS_DSL_STRUCT: &str = "Shotness.DSLStruct";
/// Configuration key for the DSL name expression.
pub const CONFIG_SHOTNESS_DSL_NAME: &str = "Shotness.DSLName";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_constants() {
        assert_eq!(ANALYZER_ID, "history/shotness");
        assert_eq!(DEFAULT_SHOTNESS_DSL_STRUCT, r#"filter(.roles has "Function")"#);
        assert_eq!(DEFAULT_SHOTNESS_DSL_NAME, ".props.name");
        assert_eq!(CONFIG_SHOTNESS_DSL_STRUCT, "Shotness.DSLStruct");
        assert_eq!(CONFIG_SHOTNESS_DSL_NAME, "Shotness.DSLName");
    }
}
