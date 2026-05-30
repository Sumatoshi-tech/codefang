//! Observability configuration.
//!
//! Port of `internal/observability/config.go`. Defines [`AppMode`], the
//! [`Config`] struct, and [`Config::default`] (Go's `DefaultConfig`).

use std::collections::BTreeMap;

use tracing::Level;

/// Default OTel service name (`defaultServiceName` in Go).
pub const DEFAULT_SERVICE_NAME: &str = "codefang";

/// Default shutdown timeout in seconds (`defaultShutdownTimeoutSec` in Go).
pub const DEFAULT_SHUTDOWN_TIMEOUT_SEC: i32 = 5;

/// Identifies the application execution mode (Go `AppMode`).
///
/// The string form (used as the `app.mode` resource attribute and the `mode`
/// log field) matches Go exactly: `"cli"`, `"mcp"`, `"serve"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppMode {
    /// CLI command execution mode (`ModeCLI` = `"cli"`).
    Cli,
    /// MCP stdio server mode (`ModeMCP` = `"mcp"`).
    Mcp,
    /// HTTP/gRPC server mode (`ModeServe` = `"serve"`).
    Serve,
}

impl AppMode {
    /// Returns the wire string for this mode, matching the Go `AppMode` value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            AppMode::Cli => "cli",
            AppMode::Mcp => "mcp",
            AppMode::Serve => "serve",
        }
    }
}

impl std::fmt::Display for AppMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Holds all observability configuration (Go `Config`).
///
/// Field semantics mirror the Go struct one-for-one. [`Config::default`]
/// reproduces `DefaultConfig()`.
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
    /// A [`BTreeMap`] is used so iteration order is deterministic (mirroring
    /// the Go map being sorted before use); Go's `ParseOTLPHeaders` returns an
    /// unordered map, but header order is not observable behavior.
    pub otlp_headers: BTreeMap<String, String>,

    /// Disables TLS for the OTLP gRPC connection.
    pub otlp_insecure: bool,

    /// Forces 100% trace sampling when true.
    pub debug_trace: bool,

    /// Trace sampling ratio (0.0 to 1.0) when [`debug_trace`](Self::debug_trace)
    /// is false. Zero uses the SDK default (parent-based with always-on root).
    pub sample_ratio: f64,

    /// Minimum log severity (Go `slog.Level`).
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
    /// Returns a `Config` with sensible defaults for zero-config startup.
    ///
    /// Port of Go `DefaultConfig()`:
    /// `ServiceName="codefang"`, `Mode=ModeCLI`, `LogLevel=slog.LevelInfo`,
    /// `ShutdownTimeoutSec=5`; all other fields zero/empty.
    fn default() -> Self {
        Config {
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

    /// Port of Go `TestDefaultConfig_HasSensibleDefaults`.
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
    fn app_mode_strings_match_go() {
        assert_eq!(AppMode::Cli.as_str(), "cli");
        assert_eq!(AppMode::Mcp.as_str(), "mcp");
        assert_eq!(AppMode::Serve.as_str(), "serve");
    }
}
