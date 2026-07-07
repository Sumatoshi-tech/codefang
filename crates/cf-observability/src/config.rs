//! Observability configuration.
//!
//! Defines [`AppMode`], the [`Config`] struct, and the zero-config defaults in
//! [`Config::default`].

use std::collections::BTreeMap;

use tracing::Level;

/// Default OTel service name.
pub const DEFAULT_SERVICE_NAME: &str = "codefang";

/// Default shutdown timeout in seconds.
pub const DEFAULT_SHUTDOWN_TIMEOUT_SEC: i32 = 5;

/// Identifies the application execution mode.
///
/// The string form (used as the `app.mode` resource attribute and the `mode`
/// log field) is part of the telemetry contract: `"cli"`, `"mcp"`, `"serve"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppMode {
    /// CLI command execution mode (`"cli"`).
    Cli,
    /// MCP stdio server mode (`"mcp"`).
    Mcp,
    /// HTTP/gRPC server mode (`"serve"`).
    Serve,
}

impl AppMode {
    /// Returns the wire string for this mode.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Mcp => "mcp",
            Self::Serve => "serve",
        }
    }
}

impl std::fmt::Display for AppMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Holds all observability configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// OTel resource service name.
    pub service_name: String,

    /// Semantic version of the running binary. Empty disables the
    /// `service.version` resource attribute.
    pub service_version: String,

    /// Deployment environment (e.g. `"production"`, `"staging"`, `"dev"`).
    /// Empty disables the `deployment.environment` resource attribute.
    pub environment: String,

    /// Identifies how the binary was launched.
    pub mode: AppMode,

    /// OTLP gRPC collector address (e.g. `"localhost:4317"`).
    /// Empty disables export; providers become no-op.
    pub otlp_endpoint: String,

    /// Additional gRPC metadata headers for the OTLP exporter.
    ///
    /// A [`BTreeMap`] keeps iteration order deterministic; header order is not
    /// observable behavior.
    pub otlp_headers: BTreeMap<String, String>,

    /// Disables TLS for the OTLP gRPC connection.
    pub otlp_insecure: bool,

    /// Forces 100% trace sampling when true.
    pub debug_trace: bool,

    /// Trace sampling ratio (0.0 to 1.0) when [`debug_trace`](Self::debug_trace)
    /// is false. Zero uses the SDK default (parent-based with always-on root).
    pub sample_ratio: f64,

    /// Minimum log severity.
    pub log_level: Level,

    /// Enables hot-path spans (per-commit, per-file, per-git-op). When false
    /// (default), only structural pipeline spans are recorded.
    pub trace_verbose: bool,

    /// Enables JSON-formatted log output.
    pub log_json: bool,

    /// Maximum seconds to wait for flush on shutdown.
    pub shutdown_timeout_sec: i32,
}

impl Default for Config {
    /// Returns a `Config` with sensible defaults for zero-config startup:
    /// `service_name = "codefang"`, `mode = Cli`, `log_level = INFO`,
    /// `shutdown_timeout_sec = 5`; all other fields zero/empty.
    fn default() -> Self {
        Self {
            service_name: DEFAULT_SERVICE_NAME.to_string(),
            service_version: String::new(),
            environment: String::new(),
            mode: AppMode::Cli,
            otlp_endpoint: String::new(),
            otlp_headers: BTreeMap::new(),
            otlp_insecure: false,
            debug_trace: false,
            sample_ratio: 0.0,
            log_level: Level::INFO,
            trace_verbose: false,
            log_json: false,
            shutdown_timeout_sec: DEFAULT_SHUTDOWN_TIMEOUT_SEC,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors the reference suite's `TestDefaultConfig_HasSensibleDefaults`.
    #[test]
    fn default_config_has_sensible_defaults() {
        let cfg = Config::default();

        assert_eq!(cfg.service_name, "codefang");
        assert_eq!(cfg.mode, AppMode::Cli);
        assert_eq!(cfg.log_level, Level::INFO);
        assert_eq!(cfg.shutdown_timeout_sec, 5);
        assert!(cfg.otlp_endpoint.is_empty());
        assert!(!cfg.debug_trace);
        assert!(cfg.service_version.is_empty());
        assert!(cfg.environment.is_empty());
    }

    #[test]
    fn app_mode_wire_strings() {
        assert_eq!(AppMode::Cli.as_str(), "cli");
        assert_eq!(AppMode::Mcp.as_str(), "mcp");
        assert_eq!(AppMode::Serve.as_str(), "serve");
    }
}
