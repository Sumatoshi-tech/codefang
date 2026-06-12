//! HTTP diagnostics server (`/healthz`, `/readyz`, `/metrics`).
//!
//! Starts a hyper + tokio HTTP server that exposes liveness, readiness, and
//! Prometheus metrics endpoints for operational monitoring, registering
//! scheduler metrics on the supplied meter. The route table and response
//! bodies are a stable operational surface.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use opentelemetry::metrics::Meter;
use tokio::net::TcpListener;
use tokio::sync::oneshot;

use crate::health::{health_response, HEALTH_STATUS_OK};
use crate::prometheus_bridge::PrometheusScrape;
use crate::scheduler_metrics::SchedulerMetrics;

/// Error returned when the diagnostics server fails to start or stop.
///
/// The error wording is a stable operator-facing surface.
#[derive(Debug, thiserror::Error)]
pub enum DiagnosticsError {
    /// Binding the listener failed.
    #[error("listen: {0}")]
    Listen(#[source] std::io::Error),
    /// Building the Prometheus handler failed.
    #[error("create prometheus handler: {0}")]
    Prometheus(String),
    /// Registering scheduler metrics failed.
    #[error("register scheduler metrics: {0}")]
    SchedulerMetrics(String),
}

/// Exposes health, readiness, and Prometheus metrics endpoints over HTTP.
pub struct DiagnosticsServer {
    addr: SocketAddr,
    shutdown_tx: Option<oneshot::Sender<()>>,
    join: Option<tokio::task::JoinHandle<()>>,
    /// Kept alive so the scrape registry/provider outlive the server.
    _scrape: Arc<PrometheusScrape>,
    /// Kept alive so the registered scheduler-metrics callback stays active.
    _scheduler_metrics: Option<SchedulerMetrics>,
}

impl DiagnosticsServer {
    /// Starts the server at `addr` with `/healthz`, `/readyz`, and `/metrics`.
    ///
    /// When `meter` is `Some`, scheduler metrics are registered on it; pass
    /// `None` to skip.
    ///
    /// # Errors
    ///
    /// Returns [`DiagnosticsError`] if the Prometheus handler cannot be built,
    /// scheduler metrics fail to register, or the listener cannot bind.
    pub async fn new(addr: &str, meter: Option<&Meter>) -> Result<Self, DiagnosticsError> {
        let scrape = Arc::new(
            PrometheusScrape::new().map_err(|e| DiagnosticsError::Prometheus(e.to_string()))?,
        );

        let scheduler_metrics = meter
            .map(|m| {
                SchedulerMetrics::new(m)
                    .map_err(|e| DiagnosticsError::SchedulerMetrics(e.to_string()))
            })
            .transpose()?;

        let listener = TcpListener::bind(addr)
            .await
            .map_err(DiagnosticsError::Listen)?;
        let local_addr = listener.local_addr().map_err(DiagnosticsError::Listen)?;

        let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();
        let scrape_for_task = Arc::clone(&scrape);

        let join = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { continue };
                        let io = TokioIo::new(stream);
                        let scrape = Arc::clone(&scrape_for_task);
                        tokio::spawn(async move {
                            let service = service_fn(move |req| {
                                let scrape = Arc::clone(&scrape);
                                async move { route(req, scrape).await }
                            });
                            let _ = hyper::server::conn::http1::Builder::new()
                                .serve_connection(io, service)
                                .await;
                        });
                    }
                }
            }
        });

        Ok(Self {
            addr: local_addr,
            shutdown_tx: Some(shutdown_tx),
            join: Some(join),
            _scrape: scrape,
            _scheduler_metrics: scheduler_metrics,
        })
    }

    /// Returns the address the server is listening on.
    #[must_use]
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Gracefully shuts down the server.
    ///
    /// # Errors
    ///
    /// Returns an error if the server task cannot be joined.
    pub async fn close(mut self) -> Result<(), DiagnosticsError> {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(join) = self.join.take() {
            let _ = join.await;
        }
        Ok(())
    }
}

/// Routes a request to the matching diagnostics endpoint.
async fn route(
    req: Request<Incoming>,
    scrape: Arc<PrometheusScrape>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = req.uri().path();
    match path {
        "/healthz" => Ok(json_response(StatusCode::OK, health_response().body)),
        "/readyz" => {
            // No checks are registered through this server, so readiness is
            // always OK.
            let body = format!("{{\"status\":\"{HEALTH_STATUS_OK}\"}}").into_bytes();
            Ok(json_response(StatusCode::OK, body))
        }
        "/metrics" => match scrape.render() {
            Ok((content_type, body)) => Ok(Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", content_type)
                .body(Full::new(Bytes::from(body)))
                .expect("valid response")),
            Err(_) => Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Full::new(Bytes::new()))
                .expect("valid response")),
        },
        _ => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::new()))
            .expect("valid response")),
    }
}

/// Builds a JSON response with the given status and body.
fn json_response(status: StatusCode, body: Vec<u8>) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Full::new(Bytes::from(body)))
        .expect("valid response")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Integration: server starts, serves /healthz, and shuts down cleanly.
    /// Covers the route table and lifecycle.
    #[tokio::test]
    async fn serves_healthz_and_shuts_down() {
        let server = DiagnosticsServer::new("127.0.0.1:0", None)
            .await
            .expect("server starts");
        let addr = server.addr();
        assert_ne!(addr.port(), 0);

        // Hit /healthz with a raw client.
        let url = format!("http://{addr}/healthz");
        let body = simple_get(&url).await;
        assert!(body.contains("\"status\":\"ok\""));

        server.close().await.expect("clean shutdown");
    }

    /// Minimal HTTP GET returning the response body as a string.
    async fn simple_get(url: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let url = url.strip_prefix("http://").unwrap();
        let (authority, path) = url.split_once('/').unwrap();
        let mut stream = tokio::net::TcpStream::connect(authority).await.unwrap();
        let req = format!(
            "GET /{path} HTTP/1.1\r\nHost: {authority}\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(req.as_bytes()).await.unwrap();
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).await.unwrap();
        String::from_utf8_lossy(&buf).to_string()
    }
}
