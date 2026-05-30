//! HTTP tracing middleware + span error classification.
//!
//! Port of `internal/observability/middleware.go`. Provides:
//! - the error-type / error-source classification constants (matched verbatim);
//! - [`record_span_error`], which sets a span's status + `error.type`
//!   (and optional `error.source`) attributes;
//! - [`span_name`], the `"METHOD /path"` route-template span name;
//! - [`is_server_error`], the `>= 500` threshold check.
//!
//! The full per-request middleware (extract W3C traceparent, start a server
//! span, recover panics, emit a one-line access log) is wired in
//! [`crate::diagnostics`] over hyper; the load-bearing, fully-tested pieces live
//! here as pure functions so the ported assertions hold without a live server.

use opentelemetry::trace::{Span, Status};
use opentelemetry::KeyValue;

/// Threshold for HTTP server errors (Go `httpStatusServerError`).
pub const HTTP_STATUS_SERVER_ERROR: u16 = 500;

// Error type classification constants per OTel semantic conventions (Go consts).
/// `error.type` value: operation timed out.
pub const ERR_TYPE_TIMEOUT: &str = "timeout";
/// `error.type` value: operation cancelled.
pub const ERR_TYPE_CANCEL: &str = "cancel";
/// `error.type` value: input validation failure.
pub const ERR_TYPE_VALIDATION: &str = "validation";
/// `error.type` value: a dependency was unavailable.
pub const ERR_TYPE_DEPENDENCY_UNAVAILABLE: &str = "dependency_unavailable";
/// `error.type` value: internal error.
pub const ERR_TYPE_INTERNAL: &str = "internal";

// Error source classification constants (Go consts).
/// `error.source` value: caused by the client.
pub const ERR_SOURCE_CLIENT: &str = "client";
/// `error.source` value: caused by the server.
pub const ERR_SOURCE_SERVER: &str = "server";
/// `error.source` value: caused by a dependency.
pub const ERR_SOURCE_DEPENDENCY: &str = "dependency";

/// Records an error on a span with structured classification attributes.
///
/// Port of Go `RecordSpanError`: records the error, sets `Status::error` with the
/// error's message as description, sets `error.type`, and (when non-empty) sets
/// `error.source`.
pub fn record_span_error<S: Span>(span: &mut S, err: &dyn std::error::Error, err_type: &str, err_source: &str) {
    let msg = err.to_string();
    // OTel-Rust records errors via an event; mirror Go's span.RecordError.
    span.add_event(
        "exception",
        vec![KeyValue::new("exception.message", msg.clone())],
    );
    span.set_status(Status::error(msg));

    span.set_attribute(KeyValue::new("error.type", err_type.to_string()));
    if !err_source.is_empty() {
        span.set_attribute(KeyValue::new("error.source", err_source.to_string()));
    }
}

/// Returns the route-template span name `"METHOD /path"` (Go `spanName`).
#[must_use]
pub fn span_name(method: &str, path: &str) -> String {
    format!("{method} {path}")
}

/// Returns true when `status` is a server error (`>= 500`), matching Go's check.
#[must_use]
pub fn is_server_error(status: u16) -> bool {
    status >= HTTP_STATUS_SERVER_ERROR
}

/// Captures the first written HTTP status code (Go `statusWriter`).
///
/// Defaults to 200 once a body is written without an explicit status, exactly
/// like Go's `statusWriter.Write`.
#[derive(Debug, Clone, Copy)]
pub struct StatusRecorder {
    status_code: u16,
    written: bool,
}

impl Default for StatusRecorder {
    fn default() -> Self {
        StatusRecorder {
            status_code: 0,
            written: false,
        }
    }
}

impl StatusRecorder {
    /// Records an explicit status code (Go `WriteHeader`); first write wins.
    pub fn write_header(&mut self, code: u16) {
        if !self.written {
            self.status_code = code;
            self.written = true;
        }
    }

    /// Marks a body write; if no explicit header was set, defaults to 200
    /// (Go `Write`).
    pub fn on_write(&mut self) {
        if !self.written {
            self.status_code = 200;
            self.written = true;
        }
    }

    /// Returns the captured status code (0 if nothing was written).
    #[must_use]
    pub fn status_code(&self) -> u16 {
        self.status_code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_name_route_template() {
        // Port of the span-name assertions in Go middleware tests.
        assert_eq!(span_name("GET", "/v1/analyze"), "GET /v1/analyze");
        assert_eq!(span_name("POST", "/v1/history"), "POST /v1/history");
    }

    #[test]
    fn server_error_threshold() {
        assert!(is_server_error(500));
        assert!(is_server_error(503));
        assert!(!is_server_error(499));
        assert!(!is_server_error(200));
    }

    /// Port of Go `statusWriter` behavior: explicit header wins, first write wins.
    #[test]
    fn status_recorder_explicit_wins() {
        let mut sr = StatusRecorder::default();
        sr.write_header(500);
        sr.write_header(200); // ignored
        sr.on_write(); // ignored
        assert_eq!(sr.status_code(), 500);
    }

    /// Port of Go `statusWriter.Write` default-to-200.
    #[test]
    fn status_recorder_defaults_to_200_on_write() {
        let mut sr = StatusRecorder::default();
        sr.on_write();
        assert_eq!(sr.status_code(), 200);
    }

    #[test]
    fn classification_constants_match_go() {
        assert_eq!(ERR_TYPE_TIMEOUT, "timeout");
        assert_eq!(ERR_TYPE_CANCEL, "cancel");
        assert_eq!(ERR_TYPE_VALIDATION, "validation");
        assert_eq!(ERR_TYPE_DEPENDENCY_UNAVAILABLE, "dependency_unavailable");
        assert_eq!(ERR_TYPE_INTERNAL, "internal");
        assert_eq!(ERR_SOURCE_CLIENT, "client");
        assert_eq!(ERR_SOURCE_SERVER, "server");
        assert_eq!(ERR_SOURCE_DEPENDENCY, "dependency");
    }
}
