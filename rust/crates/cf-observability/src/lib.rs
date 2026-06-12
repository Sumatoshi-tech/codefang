//! `cf-observability` — telemetry for the codefang binary.
//!
//! OpenTelemetry tracing/metrics/logging initialization, RED + analysis +
//! scheduler metrics, the diagnostics HTTP server (`/healthz`, `/readyz`,
//! `/metrics`), span filtering, and `/proc` memory introspection. Nothing in
//! this crate produces report bytes (DESIGN §2); instrument names, attribute
//! keys, log fields, and operator-facing wire strings nevertheless follow the
//! established telemetry contract so dashboards and alerts keep working.
//!
//! The run command initializes this crate on every invocation; the consumers
//! are the `cf-commands` run handler (behind its `runtime` feature) and the
//! trait hooks in `cf-mcp` / `cf-framework` / `cf-streaming`.

pub mod analysis_metrics;
pub mod attribute_filter;
pub mod config;
pub mod diagnostics;
pub mod health;
pub mod init_otel;
pub mod logger;
pub mod metric_builder;
pub mod metrics;
pub mod middleware;
pub mod prometheus_bridge;
pub mod scheduler_metrics;
pub mod sysmetrics;
pub mod tracer_filter;

/// Crate name, used by smoke tests to confirm the module links.
pub const CRATE_NAME: &str = "cf-observability";
