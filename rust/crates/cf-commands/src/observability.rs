//! Run-command observability bootstrap.
//!
//! The init/shutdown bracket around the run pipeline: build the default
//! observability config, overlay the version, the standard
//! `OTEL_EXPORTER_OTLP_*` environment, and `--debug-trace`, then hand the
//! config to `cf_observability::init_otel::init`.
//!
//! With no `OTEL_EXPORTER_OTLP_ENDPOINT` set (the default CLI case) the
//! providers are no-op with zero export overhead and zero output, exactly like
//! the reference binary — nothing here can touch report bytes.

use cf_observability::config::Config;
use cf_observability::init_otel::{self, parse_otlp_headers, Providers};

/// RAII bracket for the run command's observability lifetime.
///
/// Holding it mirrors the reference implementation's `defer providers.Shutdown(ctx)`:
/// dropping the guard flushes/shuts down the providers.
pub(crate) struct ObservabilityGuard {
    providers: Providers,
}

impl Drop for ObservabilityGuard {
    fn drop(&mut self) {
        // The reference implementation logs "observability shutdown failed" as a warning and proceeds;
        // shutdown failure must never affect the exit code or report bytes.
        if let Err(e) = self.providers.shutdown() {
            eprintln!("observability shutdown failed: {e}");
        }
    }
}

/// Initializes observability for one `codefang run` invocation.
///
/// On failure the caller surfaces the reference implementation's wrapped `init observability: <err>`
/// error path (CLI `Error: <msg>`, rc 1). The default no-op
/// path cannot fail.
pub(crate) fn init_run_observability(
    debug_trace: bool,
) -> Result<ObservabilityGuard, init_otel::ShutdownError> {
    let cfg = Config {
        service_version: cf_version::VERSION.to_string(),
        otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_default(),
        otlp_headers: parse_otlp_headers(
            &std::env::var("OTEL_EXPORTER_OTLP_HEADERS").unwrap_or_default(),
        ),
        otlp_insecure: std::env::var("OTEL_EXPORTER_OTLP_INSECURE").as_deref() == Ok("true"),
        // Mode is already Cli in the default config; set it explicitly anyway,
        // matching the reference implementation.
        mode: cf_observability::config::AppMode::Cli,
        debug_trace,
        ..Config::default()
    };

    init_otel::init(&cfg).map(|providers| ObservabilityGuard { providers })
}
