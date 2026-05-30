//! Liveness / readiness health responses.
//!
//! Port of `internal/observability/health.go`. Produces the exact response
//! bodies, status strings, and HTTP status codes the Go handlers emit:
//! `/healthz` → 200 `{"status":"ok"}`; `/readyz` → 200 `{"status":"ok"}` when
//! all checks pass (or none given), else 503 `{"status":"unavailable"}`.
//!
//! The handler wiring lives in [`crate::diagnostics`]; this module provides the
//! pure, fully-testable response builders so the body bytes are verified
//! directly (Go asserts `body["status"] == "ok"`).

use std::future::Future;
use std::pin::Pin;

/// Health status string for success (Go `healthStatusOK`).
pub const HEALTH_STATUS_OK: &str = "ok";

/// Health status string for failure (Go `healthStatusUnavailable`).
pub const HEALTH_STATUS_UNAVAILABLE: &str = "unavailable";

/// HTTP 200.
pub const STATUS_OK: u16 = 200;

/// HTTP 503.
pub const STATUS_SERVICE_UNAVAILABLE: u16 = 503;

/// Content type of all health responses.
pub const CONTENT_TYPE_JSON: &str = "application/json";

/// A readiness check (Go `ReadyCheck`): returns `Ok(())` if the subsystem is
/// ready, or `Err(message)` describing the failure.
///
/// Boxed async closure so multiple heterogeneous checks can be stored together.
pub type ReadyCheck = Box<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> + Send + Sync,
>;

/// An HTTP response: status code, content type, and JSON body bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// `Content-Type` header value.
    pub content_type: &'static str,
    /// Response body bytes.
    pub body: Vec<u8>,
}

/// Serializes a `{"status":<status>}` body exactly as Go's
/// `json.Marshal(map[string]string{"status": status})` does.
///
/// The single-key object has a fixed shape, so `serde_json` reproduces Go's
/// compact bytes (`{"status":"ok"}`) byte-for-byte; this is an HTTP body, not a
/// machine report, so cf-gojson is not required (DESIGN §3).
fn status_body(status: &str) -> Vec<u8> {
    // Build manually to guarantee compact, no-space form identical to Go.
    let mut buf = Vec::with_capacity(status.len() + 13);
    buf.extend_from_slice(b"{\"status\":\"");
    buf.extend_from_slice(status.as_bytes());
    buf.extend_from_slice(b"\"}");
    buf
}

/// Builds the liveness response (Go `HealthHandler`): always 200 `{"status":"ok"}`.
#[must_use]
pub fn health_response() -> HttpResponse {
    HttpResponse {
        status: STATUS_OK,
        content_type: CONTENT_TYPE_JSON,
        body: status_body(HEALTH_STATUS_OK),
    }
}

/// Builds the readiness response (Go `ReadyHandler`).
///
/// Runs each check in order; the first failure yields a 503
/// `{"status":"unavailable"}` response (subsequent checks are not run). If all
/// pass (or none are supplied) returns 200 `{"status":"ok"}`.
pub async fn ready_response(checks: &[ReadyCheck]) -> HttpResponse {
    for check in checks {
        if check().await.is_err() {
            return HttpResponse {
                status: STATUS_SERVICE_UNAVAILABLE,
                content_type: CONTENT_TYPE_JSON,
                body: status_body(HEALTH_STATUS_UNAVAILABLE),
            };
        }
    }

    HttpResponse {
        status: STATUS_OK,
        content_type: CONTENT_TYPE_JSON,
        body: status_body(HEALTH_STATUS_OK),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass_check() -> ReadyCheck {
        Box::new(|| Box::pin(async { Ok(()) }))
    }

    fn fail_check() -> ReadyCheck {
        Box::new(|| Box::pin(async { Err("db unreachable".to_string()) }))
    }

    /// Port of Go `TestHealthHandler_ReturnsOK` + `TestHealthHandler_ContentTypeJSON`.
    #[test]
    fn health_handler_returns_ok() {
        let resp = health_response();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.content_type, "application/json");
        assert_eq!(resp.body, br#"{"status":"ok"}"#);
    }

    /// Port of Go `TestReadyHandler_AllChecksPass`.
    #[tokio::test]
    async fn ready_handler_all_checks_pass() {
        let checks = vec![pass_check(), pass_check()];
        let resp = ready_response(&checks).await;
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, br#"{"status":"ok"}"#);
    }

    /// Port of Go `TestReadyHandler_NoChecks`.
    #[tokio::test]
    async fn ready_handler_no_checks() {
        let resp = ready_response(&[]).await;
        assert_eq!(resp.status, 200);
    }

    /// Port of Go `TestReadyHandler_CheckFails` (pass then fail → 503).
    #[tokio::test]
    async fn ready_handler_check_fails() {
        let checks = vec![pass_check(), fail_check()];
        let resp = ready_response(&checks).await;
        assert_eq!(resp.status, 503);
        assert_eq!(resp.body, br#"{"status":"unavailable"}"#);
    }
}
