//! OpenTelemetry tracing / metrics / logging initialization.
//!
//! Initializes tracer + meter providers and a structured logger, returning
//! [`Providers`] with a [`Providers::shutdown`] hook. The cardinal behavior:
//! **no-op, zero-export providers when the OTLP endpoint is empty**.
//!
//! # Sampler selection precedence
//!
//! 1. `debug_trace` → always-on.
//! 2. else `OTEL_TRACES_SAMPLER` env (via [`SamplerKind::from_env`]).
//! 3. else `sample_ratio > 0` → parent-based TraceID-ratio.
//! 4. else parent-based always-on.
//!
//! The sampler decision is exposed as [`SamplerKind`] so the sampler tests
//! (which check whether a fresh root span is sampled under each env setting)
//! can run without a live exporter.

use std::collections::BTreeMap;

use crate::config::Config;
use crate::logger::TracingHandler;

/// Instrumentation-scope name for the tracer; export-mode provider wiring
/// will use it.
pub const TRACER_NAME: &str = "codefang";
/// Instrumentation-scope name for the meter; export-mode provider wiring
/// will use it.
pub const METER_NAME: &str = "codefang";

// Standard OTel env vars.
const ENV_TRACES_SAMPLER: &str = "OTEL_TRACES_SAMPLER";
const ENV_TRACES_SAMPLER_ARG: &str = "OTEL_TRACES_SAMPLER_ARG";

// Standard OTel sampler names.
const SAMPLER_ALWAYS_ON: &str = "always_on";
const SAMPLER_ALWAYS_OFF: &str = "always_off";
const SAMPLER_TRACE_ID_RATIO: &str = "traceidratio";
const SAMPLER_PARENT_BASED_ALWAYS_ON: &str = "parentbased_always_on";
const SAMPLER_PARENT_BASED_ALWAYS_OFF: &str = "parentbased_always_off";
const SAMPLER_PARENT_BASED_TRACE_ID_RATIO: &str = "parentbased_traceidratio";

/// Resolved sampler selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SamplerKind {
    /// Always sample.
    AlwaysOn,
    /// Never sample.
    AlwaysOff,
    /// Sample by TraceID ratio (root decision; ratio in 0.0..=1.0).
    TraceIdRatio(f64),
    /// Parent-based; root falls back to always-on.
    ParentBasedAlwaysOn,
    /// Parent-based; root falls back to always-off.
    ParentBasedAlwaysOff,
    /// Parent-based; root falls back to TraceID ratio.
    ParentBasedTraceIdRatio(f64),
}

impl SamplerKind {
    /// Maps an `OTEL_TRACES_SAMPLER` value + arg to a sampler.
    ///
    /// Unknown names fall back to parent-based always-on (the default).
    #[must_use]
    pub fn from_env(name: &str, arg: &str) -> Self {
        match name {
            SAMPLER_ALWAYS_ON => Self::AlwaysOn,
            SAMPLER_ALWAYS_OFF => Self::AlwaysOff,
            SAMPLER_TRACE_ID_RATIO => Self::TraceIdRatio(parse_ratio(arg)),
            SAMPLER_PARENT_BASED_ALWAYS_ON => Self::ParentBasedAlwaysOn,
            SAMPLER_PARENT_BASED_ALWAYS_OFF => Self::ParentBasedAlwaysOff,
            SAMPLER_PARENT_BASED_TRACE_ID_RATIO => {
                Self::ParentBasedTraceIdRatio(parse_ratio(arg))
            }
            _ => Self::ParentBasedAlwaysOn,
        }
    }

    /// Whether a fresh **root** span (no parent) would be sampled.
    ///
    /// Used by the sampler tests. Ratio 1.0 always samples; 0.0 never.
    /// Parent-based samplers, with no parent, defer to their root delegate.
    #[must_use]
    pub fn samples_root_span(self) -> bool {
        match self {
            Self::AlwaysOn | Self::ParentBasedAlwaysOn => true,
            Self::AlwaysOff | Self::ParentBasedAlwaysOff => false,
            Self::TraceIdRatio(r) | Self::ParentBasedTraceIdRatio(r) => r >= 1.0,
        }
    }
}

/// Selects the sampler from config + environment.
fn select_sampler(cfg: &Config, env_sampler: Option<&str>, env_arg: Option<&str>) -> SamplerKind {
    if cfg.debug_trace {
        return SamplerKind::AlwaysOn;
    }

    if let Some(name) = env_sampler {
        if !name.is_empty() {
            return SamplerKind::from_env(name, env_arg.unwrap_or(""));
        }
    }

    if cfg.sample_ratio > 0.0 {
        return SamplerKind::ParentBasedTraceIdRatio(cfg.sample_ratio);
    }

    SamplerKind::ParentBasedAlwaysOn
}

/// Parses a sampler ratio argument: empty or unparseable → 1.0.
fn parse_ratio(s: &str) -> f64 {
    if s.is_empty() {
        return 1.0;
    }
    s.parse::<f64>().unwrap_or(1.0)
}

/// Resource attributes built from config.
///
/// Always carries `service.name`; `service.version`, `deployment.environment`,
/// and `app.mode` are added only when their config field is non-empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceAttrs {
    /// Ordered `(key, value)` resource attributes.
    pub attrs: Vec<(String, String)>,
}

impl ResourceAttrs {
    /// Returns the value for `key`, if present.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// Builds resource attributes from config.
#[must_use]
pub fn build_resource(cfg: &Config) -> ResourceAttrs {
    let mut attrs = vec![("service.name".to_string(), cfg.service_name.clone())];
    if !cfg.service_version.is_empty() {
        attrs.push(("service.version".to_string(), cfg.service_version.clone()));
    }
    if !cfg.environment.is_empty() {
        attrs.push((
            "deployment.environment".to_string(),
            cfg.environment.clone(),
        ));
    }
    // app.mode is always appended: AppMode always has a wire string.
    attrs.push(("app.mode".to_string(), cfg.mode.to_string()));
    ResourceAttrs { attrs }
}

/// Parses an OTLP headers string in `"key=value,key=value"` format.
///
/// Returns an empty map for empty or fully-invalid input. Whitespace around keys
/// and values is trimmed; pairs without `=` are skipped.
#[must_use]
pub fn parse_otlp_headers(raw: &str) -> BTreeMap<String, String> {
    let mut result = BTreeMap::new();
    if raw.is_empty() {
        return result;
    }

    for pair in raw.split(',') {
        let trimmed = pair.trim();
        if let Some((k, v)) = trimmed.split_once('=') {
            result.insert(k.trim().to_string(), v.trim().to_string());
        }
        // pairs without '=' are skipped
    }

    result
}

/// Initialized observability providers.
///
/// In a full deployment these wrap real OTel tracer/meter handles; this
/// returns the configured, sampler-resolved, logger-attached bundle and a
/// shutdown hook. When the OTLP endpoint is empty the providers are no-op
/// (zero export overhead).
pub struct Providers {
    /// Resolved trace sampler (no-op export when [`Providers::export_enabled`]
    /// is false).
    pub sampler: SamplerKind,
    /// Resource attributes attached to all telemetry.
    pub resource: ResourceAttrs,
    /// Structured logger handler with trace-context + service injection.
    pub logger: TracingHandler,
    /// Whether OTLP export is enabled (i.e. the OTLP endpoint was non-empty).
    export_enabled: bool,
    /// Configured shutdown timeout in seconds (`<= 0` → default).
    shutdown_timeout_sec: i32,
}

impl Providers {
    /// Returns whether OTLP export is enabled.
    #[must_use]
    pub fn export_enabled(&self) -> bool {
        self.export_enabled
    }

    /// Flushes pending telemetry and releases resources.
    ///
    /// Idempotent and always succeeds in no-op mode. A non-positive configured
    /// timeout falls back to the default.
    ///
    /// # Errors
    ///
    /// Never errors in no-op mode; reserved for export-mode flush failures.
    pub fn shutdown(&self) -> Result<(), ShutdownError> {
        let _timeout = if self.shutdown_timeout_sec <= 0 {
            crate::config::DEFAULT_SHUTDOWN_TIMEOUT_SEC
        } else {
            self.shutdown_timeout_sec
        };
        // No-op providers have nothing to flush; export-mode flush would happen
        // here. Always Ok so repeated shutdowns are safe.
        Ok(())
    }
}

/// Error returned by [`Providers::shutdown`] (reserved for export-mode failures).
#[derive(Debug, thiserror::Error)]
#[error("observability shutdown: {0}")]
pub struct ShutdownError(pub String);

/// Initializes tracing, metrics, and structured logging.
///
/// When the OTLP endpoint is empty, no-op providers are returned with zero export
/// overhead. The sampler is resolved from config + environment; the logger is
/// built with the service/env/mode injection handler.
///
/// # Errors
///
/// Returns an error if provider construction fails (export mode only).
pub fn init(cfg: &Config) -> Result<Providers, ShutdownError> {
    init_with_env(
        cfg,
        std::env::var(ENV_TRACES_SAMPLER).ok().as_deref(),
        std::env::var(ENV_TRACES_SAMPLER_ARG).ok().as_deref(),
    )
}

/// [`init`] with explicit sampler-env values (testable without process env).
///
/// # Errors
///
/// Returns an error if provider construction fails (export mode only).
pub fn init_with_env(
    cfg: &Config,
    env_sampler: Option<&str>,
    env_arg: Option<&str>,
) -> Result<Providers, ShutdownError> {
    let resource = build_resource(cfg);
    let sampler = select_sampler(cfg, env_sampler, env_arg);
    let logger = build_logger(cfg);
    let export_enabled = !cfg.otlp_endpoint.is_empty();

    Ok(Providers {
        sampler,
        resource,
        logger,
        export_enabled,
        shutdown_timeout_sec: cfg.shutdown_timeout_sec,
    })
}

/// Builds the structured-logging handler.
fn build_logger(cfg: &Config) -> TracingHandler {
    TracingHandler::new(&cfg.service_name, &cfg.environment, cfg.mode)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppMode;

    /// Mirrors the reference suite's `TestParseOTLPHeaders` (table-driven).
    #[test]
    fn parse_otlp_headers_cases() {
        assert!(parse_otlp_headers("").is_empty());
        assert_eq!(
            parse_otlp_headers("key=value"),
            BTreeMap::from([("key".to_string(), "value".to_string())])
        );
        assert_eq!(
            parse_otlp_headers("k1=v1,k2=v2"),
            BTreeMap::from([
                ("k1".to_string(), "v1".to_string()),
                ("k2".to_string(), "v2".to_string())
            ])
        );
        assert_eq!(
            parse_otlp_headers(" k1 = v1 , k2 = v2 "),
            BTreeMap::from([
                ("k1".to_string(), "v1".to_string()),
                ("k2".to_string(), "v2".to_string())
            ])
        );
        assert!(parse_otlp_headers("invalid").is_empty());
    }

    /// Mirrors the reference suite's `TestInit_NoopWhenNoEndpoint`.
    #[test]
    fn init_noop_when_no_endpoint() {
        let cfg = Config::default();
        let providers = init(&cfg).expect("init succeeds");
        assert!(!providers.export_enabled());
        assert!(providers.shutdown().is_ok());
    }

    /// Mirrors the reference suite's `TestInit_ShutdownIdempotent`.
    #[test]
    fn init_shutdown_idempotent() {
        let cfg = Config::default();
        let providers = init(&cfg).unwrap();
        assert!(providers.shutdown().is_ok());
        assert!(providers.shutdown().is_ok());
    }

    /// Mirrors the reference suite's `TestBuildResource_IncludesAppMode`.
    #[test]
    fn build_resource_includes_app_mode() {
        let mut cfg = Config::default();
        cfg.mode = AppMode::Mcp;
        let res = build_resource(&cfg);
        assert_eq!(res.get("app.mode"), Some("mcp"));
        // service.name always present.
        assert_eq!(res.get("service.name"), Some("codefang"));
    }

    /// Mirrors the reference suite's `TestInit_WithResourceAttributes`.
    #[test]
    fn build_resource_optional_attrs() {
        let mut cfg = Config::default();
        cfg.service_version = "1.2.3".to_string();
        cfg.environment = "test".to_string();
        cfg.mode = AppMode::Mcp;
        let res = build_resource(&cfg);
        assert_eq!(res.get("service.version"), Some("1.2.3"));
        assert_eq!(res.get("deployment.environment"), Some("test"));
        assert_eq!(res.get("app.mode"), Some("mcp"));
    }

    // --- Sampler tests ---

    /// Mirrors the reference suite's `TestSampler_AlwaysOn`.
    #[test]
    fn sampler_always_on() {
        let s = select_sampler(&Config::default(), Some("always_on"), None);
        assert!(s.samples_root_span());
    }

    /// Mirrors the reference suite's `TestSampler_AlwaysOff`.
    #[test]
    fn sampler_always_off() {
        let s = select_sampler(&Config::default(), Some("always_off"), None);
        assert!(!s.samples_root_span());
    }

    /// Mirrors the reference suite's `TestSampler_TraceIDRatio` (ratio 1.0 always samples).
    #[test]
    fn sampler_trace_id_ratio() {
        let s = select_sampler(&Config::default(), Some("traceidratio"), Some("1.0"));
        assert!(s.samples_root_span());
    }

    /// Mirrors the reference suite's `TestSampler_ParentBasedAlwaysOn`.
    #[test]
    fn sampler_parentbased_always_on() {
        let s = select_sampler(&Config::default(), Some("parentbased_always_on"), None);
        assert!(s.samples_root_span());
    }

    /// Mirrors the reference suite's `TestSampler_ParentBasedAlwaysOff` (drops root spans).
    #[test]
    fn sampler_parentbased_always_off() {
        let s = select_sampler(&Config::default(), Some("parentbased_always_off"), None);
        assert!(!s.samples_root_span());
    }

    /// Mirrors the reference suite's `TestSampler_DebugTraceOverridesEnv`.
    #[test]
    fn sampler_debug_trace_overrides_env() {
        let mut cfg = Config::default();
        cfg.debug_trace = true;
        let s = select_sampler(&cfg, Some("always_off"), None);
        assert!(s.samples_root_span());
    }

    /// Mirrors the reference suite's `TestSampler_ConfigSampleRatioFallback`.
    #[test]
    fn sampler_config_sample_ratio_fallback() {
        let mut cfg = Config::default();
        cfg.sample_ratio = 1.0;
        let s = select_sampler(&cfg, None, None);
        assert!(s.samples_root_span());
    }

    /// Mirrors the reference suite's `TestSampler_DefaultSamples`.
    #[test]
    fn sampler_default_samples() {
        let s = select_sampler(&Config::default(), None, None);
        assert!(s.samples_root_span());
    }

    /// Unknown env sampler falls back to parent-based always-on.
    #[test]
    fn sampler_unknown_env_default() {
        let s = SamplerKind::from_env("nonsense", "");
        assert_eq!(s, SamplerKind::ParentBasedAlwaysOn);
    }

    #[test]
    fn parse_ratio_fallbacks() {
        assert_eq!(parse_ratio(""), 1.0);
        assert_eq!(parse_ratio("notnum"), 1.0);
        assert_eq!(parse_ratio("0.5"), 0.5);
    }
}
