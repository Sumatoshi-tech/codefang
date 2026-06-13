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
//!
//! # Example
//!
//! Two pure pieces of the telemetry contract: the liveness probe body and the
//! span-attribute allow-list decision (which strips PII / unknown keys before
//! anything reaches an exporter).
//!
//! ```
//! use cf_observability::health::health_response;
//! use cf_observability::attribute_filter::is_attribute_allowed;
//!
//! // Liveness is always 200 with a compact JSON body probes can match on.
//! let resp = health_response();
//! assert_eq!(resp.status, 200);
//! assert_eq!(resp.body, br#"{"status":"ok"}"#);
//!
//! // Allowed prefixes pass; PII and unknown keys are denied.
//! assert!(is_attribute_allowed("analysis.duration"));
//! assert!(is_attribute_allowed("error"));
//! assert!(!is_attribute_allowed("user.id"));   // blocked prefix
//! assert!(!is_attribute_allowed("email"));     // blocked exact key
//! assert!(!is_attribute_allowed("mystery"));   // not on the allow-list
//! ```

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
///
/// ```
/// assert_eq!(cf_observability::CRATE_NAME, "cf-observability");
/// ```
pub const CRATE_NAME: &str = "cf-observability";
