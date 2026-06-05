//! cf-shotness — Go→Rust port of `internal/analyzers/shotness`.
//!
//! Structural hotspots: co-change frequency of DSL-selected UAST entities
//! (functions, classes, …) across commit history. Analyzer id `history/shotness`.
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
//! - **Metrics.** [`metrics`] is a faithful port of `metrics.go`: hotness,
//!   coupling strength (`co / max(co, a, b)`), risk classification, and the
//!   aggregate summary.
//! - **Machine-format report.** [`report`] assembles the
//!   [`metrics::ComputedMetrics`] tree as ordered [`cf_gojson::GoValue`] so the
//!   json / yaml / ndjson / timeseries / compact / bin outputs are byte-identical
//!   to Go (DESIGN §2). All serialization routes through cf-gojson, never serde.
//! - **Report store.** [`store`] ports the `node_data` / `aggregate` record
//!   stream.
//!
//! # Parity scope
//!
//! Machine formats (json, yaml, ndjson, timeseries, timeseries+ndjson, compact,
//! bin) are byte-identity targets. Terminal text and plot output are cosmetic
//! and are not ported here (see crate todos).
//!
//! Go sources: `internal/analyzers/shotness/{analyzer,aggregator,report,metrics,
//! store_writer,store_reader,hibernation,text,plot}.go`.

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

/// Analyzer identifier as used in the report registry and CLI (`history/shotness`).
pub const ANALYZER_ID: &str = "history/shotness";

/// Default DSL expression for selecting code structures.
/// Mirrors `DefaultShotnessDSLStruct`.
pub const DEFAULT_SHOTNESS_DSL_STRUCT: &str = r#"filter(.roles has "Function")"#;
/// Default DSL expression for extracting names. Mirrors `DefaultShotnessDSLName`.
pub const DEFAULT_SHOTNESS_DSL_NAME: &str = ".props.name";

/// Configuration key for the DSL structure expression.
/// Mirrors `ConfigShotnessDSLStruct`.
pub const CONFIG_SHOTNESS_DSL_STRUCT: &str = "Shotness.DSLStruct";
/// Configuration key for the DSL name expression. Mirrors `ConfigShotnessDSLName`.
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
