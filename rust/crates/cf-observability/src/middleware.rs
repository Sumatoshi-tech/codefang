//! HTTP tracing middleware + span error classification.
//!
//! Provides:
//! - the error-type / error-source classification constants (telemetry
//!   contract);
//! - [`record_span_error`], which sets a span's status + `error.type`
//!   (and optional `error.source`) attributes;
//! - [`span_name`], the `"METHOD /path"` route-template span name;
//! - [`is_server_error`], the `>= 500` threshold check.
//!
//! The full per-request middleware (extract W3C traceparent, start a server
//! span, recover panics, emit a one-line access log) is wired in
//! [`crate::diagnostics`] over hyper; the load-bearing, fully-tested pieces
//! live here as pure functions so the assertions hold without a live server.

use opentelemetry::trace::{Span, Status};
use opentelemetry::KeyValue;

/// Threshold for HTTP server errors.
pub const HTTP_STATUS_SERVER_ERROR: u16 = 500;

// Error type classification constants per OTel semantic conventions.
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

// Error source classification constants.
/// `error.source` value: caused by the client.
pub const ERR_SOURCE_CLIENT: &str = "client";
/// `error.source` value: caused by the server.
pub const ERR_SOURCE_SERVER: &str = "server";
/// `error.source` value: caused by a dependency.
pub const ERR_SOURCE_DEPENDENCY: &str = "dependency";

/// Records an error on a span with structured classification attributes.
///
/// Records the error as an exception event, sets `Status::error` with the
/// error's message as description, sets `error.type`, and (when non-empty)
/// sets `error.source`.
pub fn record_span_error<S: Span>(span: &mut S, err: &dyn std::error::Error, err_type: &str, err_source: &str) {
    let msg = err.to_string();
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

/// Returns the route-template span name `"METHOD /path"`.
#[must_use]
pub fn span_name(method: &str, path: &str) -> String {
    format!("{method} {path}")
}

/// Returns true when `status` is a server error (`>= 500`).
#[must_use]
pub const fn is_server_error(status: u16) -> bool {
    status >= HTTP_STATUS_SERVER_ERROR
}

/// Captures the first written HTTP status code.
///
/// Defaults to 200 once a body is written without an explicit status, like an
/// HTTP response writer.
#[derive(Debug, Clone, Copy, Default)]
pub struct StatusRecorder {
    status_code: u16,
    written: bool,
}

impl StatusRecorder {
    /// Records an explicit status code; first write wins.
    pub fn write_header(&mut self, code: u16) {
        if !self.written {
            self.status_code = code;
            self.written = true;
        }
    }

    /// Marks a body write; if no explicit header was set, defaults to 200.
    pub fn on_write(&mut self) {
        if !self.written {
            self.status_code = 200;
            self.written = true;
        }
    }

    /// Returns the captured status code (0 if nothing was written).
    #[must_use]
    pub const fn status_code(&self) -> u16 {
        self.status_code
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_name_route_template() {
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

    /// Explicit header wins, first write wins.
    #[test]
    fn status_recorder_explicit_wins() {
        let mut sr = StatusRecorder::default();
        sr.write_header(500);
        sr.write_header(200); // ignored
        sr.on_write(); // ignored
        assert_eq!(sr.status_code(), 500);
    }

    /// A body write without an explicit header defaults to 200.
    #[test]
    fn status_recorder_defaults_to_200_on_write() {
        let mut sr = StatusRecorder::default();
        sr.on_write();
        assert_eq!(sr.status_code(), 200);
    }

    #[test]
    fn classification_constants_match_contract() {
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
