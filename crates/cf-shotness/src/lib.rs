//! cf-shotness — structural hotspots.
//!
//! Co-change frequency of DSL-selected UAST entities (functions, methods,
//! classes, …) across commit history. Analyzer id `history/shotness`.
//!
//! # What this crate covers
//!
//! - **Entity identity.** [`types::NodeSummary`] derives the canonical key
//!   `Type + "_" + Name + "_" + File`; the name is the entity's QUALIFIED name
//!   (`Class.method`, `Receiver.method`), derived by the walk from the
//!   enclosing declarations, so two `String()` methods on different receivers
//!   are two entities.
//! - **Live state.** [`state::ShotnessState`] is the one source of truth,
//!   built by replaying each commit's [`state::EntityEffect`]s in walk order:
//!   a commit touches an entity at most once, the touched entities are
//!   coupled pairwise, a file rename moves the entities (counts and couplings
//!   preserved), a deleted file or a removed declaration ends them. The report
//!   ([`state::ShotnessState::report`]) lists exactly the entities alive at the
//!   end of the window as the index-keyed `Nodes`/`Counters` matrix.
//! - **Metrics.** [`metrics`] computes hotness, coupling strength
//!   (`co / max(co, a, b)`), risk classification, and the aggregate summary.
//! - **Machine-format report.** [`report`] assembles the
//!   [`metrics::ComputedMetrics`] tree as ordered [`cf_gojson::GoValue`] so
//!   the json / yaml / ndjson / timeseries / compact / bin outputs follow the
//!   report-format contract. All serialization routes through cf-gojson,
//!   never serde.

pub mod metrics;
pub mod report;
pub mod state;
pub mod types;

pub use metrics::{
    classify_change_risk, compute_aggregate, compute_all_metrics, compute_coupling_strength,
    compute_hotspot_nodes, compute_node_coupling, compute_node_hotness, AggregateData,
    ComputedMetrics, HotspotNodeData, NodeCouplingData, NodeHotnessData,
};
pub use state::{
    coupling_pair_count, touched_entities, CommitEffects, EntityEffect, NodeShotness, ShotnessState,
};
pub use types::{NodeSummary, ReportData};

/// Analyzer identifier as used in the report registry and CLI.
pub const ANALYZER_ID: &str = "history/shotness";

/// Default DSL expression selecting the tracked code structures: every node
/// carrying the `Function` role anywhere in the tree (top-level functions,
/// class methods, nested functions).
pub const DEFAULT_SHOTNESS_DSL_STRUCT: &str = r#"rfilter(.roles has "Function")"#;
/// Default DSL expression extracting an entity's own name from a selected
/// node (the mapping contract's `props.name`).
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
        assert_eq!(
            DEFAULT_SHOTNESS_DSL_STRUCT,
            r#"rfilter(.roles has "Function")"#
        );
        assert_eq!(DEFAULT_SHOTNESS_DSL_NAME, ".props.name");
        assert_eq!(CONFIG_SHOTNESS_DSL_STRUCT, "Shotness.DSLStruct");
        assert_eq!(CONFIG_SHOTNESS_DSL_NAME, "Shotness.DSLName");
    }
}
