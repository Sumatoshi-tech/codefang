//! Structured logging handler that injects trace context + service metadata.
//!
//! Port of `internal/observability/logger.go`'s `TracingHandler`.
//!
//! # Design note: slog handler vs tracing Layer
//!
//! Go layers a custom [`slog.Handler`] that, for every record, (a) pre-attaches
//! the service attributes `service` / `mode` (+ `env` when non-empty) so they
//! stay at the top level even under `WithGroup`, and (b) injects `trace_id` /
//! `span_id` pulled from the active span context.
//!
//! Rust's ecosystem analogue is a [`tracing_subscriber`] layer plus the
//! `tracing-opentelemetry` bridge. However, the ported Go tests assert directly
//! on the emitted **JSON record fields** (`trace_id`, `span_id`, `service`,
//! `env`, `mode`, and that grouped attrs nest under their group key while service
//! attrs stay top-level). To preserve that exact, testable record shape
//! independent of global subscriber state, `TracingHandler` here is a
//! self-contained record formatter with the same semantics. The `init` module
//! wires this handler shape into the global `tracing` subscriber.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::config::AppMode;

// Attribute names (Go consts), matched byte-for-byte.
const ATTR_TRACE_ID: &str = "trace_id";
const ATTR_SPAN_ID: &str = "span_id";
const ATTR_SERVICE: &str = "service";
const ATTR_ENV: &str = "env";
const ATTR_MODE: &str = "mode";

/// Trace/span identifiers extracted from the active span context.
///
/// Mirrors the data `slog`'s handler pulls from
/// `trace.SpanContextFromContext(ctx)` when the context is valid.
#[derive(Debug, Clone, Default)]
pub struct SpanContextIds {
    /// Lowercase hex trace id (32 chars), or `None` when no valid span.
    pub trace_id: Option<String>,
    /// Lowercase hex span id (16 chars), or `None` when no valid span.
    pub span_id: Option<String>,
}

impl SpanContextIds {
    /// Returns true when both ids are present (a valid sampled span context).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.trace_id.is_some() && self.span_id.is_some()
    }
}

/// An [`slog.Handler`]-equivalent that injects OpenTelemetry trace context
/// (`trace_id`, `span_id`) and service metadata (`service`, `env`, `mode`) into
/// every log record.
///
/// Port of Go `TracingHandler`. Service attributes are pre-attached at
/// construction so they remain at the top level even when groups are used.
#[derive(Debug, Clone)]
pub struct TracingHandler {
    /// Pre-attached service-level attributes (always emitted at top level).
    service_attrs: Vec<(String, Value)>,
    /// `With(...)` attributes accumulated outside any group (top level).
    base_attrs: Vec<(String, Value)>,
    /// Active group prefix chain (Go `WithGroup`). Nested groups nest records.
    groups: Vec<String>,
    /// Attributes added via `With(...)` while inside a group, keyed by the
    /// group path they belong to.
    grouped_attrs: Vec<(Vec<String>, String, Value)>,
}

impl TracingHandler {
    /// Wraps service metadata, injecting trace context per record.
    ///
    /// Port of Go `NewTracingHandler(inner, service, env, appMode)`. Service
    /// attributes are pre-attached so they appear at the top level regardless of
    /// subsequent `with_group` calls. `env` is omitted when empty (Go behavior).
    #[must_use]
    pub fn new(service: &str, env: &str, app_mode: AppMode) -> Self {
        let mut service_attrs = vec![
            (ATTR_SERVICE.to_string(), Value::String(service.to_string())),
            (ATTR_MODE.to_string(), Value::String(app_mode.to_string())),
        ];
        if !env.is_empty() {
            service_attrs.push((ATTR_ENV.to_string(), Value::String(env.to_string())));
        }

        TracingHandler {
            service_attrs,
            base_attrs: Vec::new(),
            groups: Vec::new(),
            grouped_attrs: Vec::new(),
        }
    }

    /// Returns a new handler with additional attributes (Go `WithAttrs`).
    ///
    /// Attributes added while a group is active nest under that group; otherwise
    /// they are top-level.
    #[must_use]
    pub fn with_attrs(&self, attrs: &[(String, Value)]) -> Self {
        let mut next = self.clone();
        if self.groups.is_empty() {
            for (k, v) in attrs {
                next.base_attrs.push((k.clone(), v.clone()));
            }
        } else {
            for (k, v) in attrs {
                next.grouped_attrs
                    .push((self.groups.clone(), k.clone(), v.clone()));
            }
        }
        next
    }

    /// Returns a new handler with a group prefix (Go `WithGroup`).
    #[must_use]
    pub fn with_group(&self, name: &str) -> Self {
        let mut next = self.clone();
        next.groups.push(name.to_string());
        next
    }

    /// Builds the JSON log record for a message, mirroring slog's JSON handler
    /// plus this handler's injection.
    ///
    /// Service attributes are always top-level; `with_attrs` attributes respect
    /// their group; record attributes (`attrs`) nest under the active group;
    /// trace context (when `span` is valid) is injected at the top level.
    #[must_use]
    pub fn build_record(
        &self,
        msg: &str,
        span: &SpanContextIds,
        attrs: &[(String, Value)],
    ) -> Value {
        let mut root = Map::new();
        root.insert("msg".to_string(), Value::String(msg.to_string()));

        // Service attrs: always top-level (survive groups).
        for (k, v) in &self.service_attrs {
            root.insert(k.clone(), v.clone());
        }
        // Base (ungrouped) with-attrs: top-level.
        for (k, v) in &self.base_attrs {
            root.insert(k.clone(), v.clone());
        }

        // Trace context injection (Go AddAttrs when span context is valid).
        if span.is_valid() {
            if let Some(tid) = &span.trace_id {
                root.insert(ATTR_TRACE_ID.to_string(), Value::String(tid.clone()));
            }
            if let Some(sid) = &span.span_id {
                root.insert(ATTR_SPAN_ID.to_string(), Value::String(sid.clone()));
            }
        }

        // Grouped with-attrs + the per-record attrs, nested under their group.
        // Collect by group path into a tree.
        let mut grouped: BTreeMap<Vec<String>, Vec<(String, Value)>> = BTreeMap::new();
        for (path, k, v) in &self.grouped_attrs {
            grouped.entry(path.clone()).or_default().push((k.clone(), v.clone()));
        }
        if !self.groups.is_empty() {
            for (k, v) in attrs {
                grouped
                    .entry(self.groups.clone())
                    .or_default()
                    .push((k.clone(), v.clone()));
            }
        } else {
            for (k, v) in attrs {
                root.insert(k.clone(), v.clone());
            }
        }

        for (path, kvs) in grouped {
            insert_nested(&mut root, &path, kvs);
        }

        Value::Object(root)
    }
}

/// Inserts `kvs` into `root` at the nested object path `path`, creating
/// intermediate objects as needed (slog group nesting).
fn insert_nested(root: &mut Map<String, Value>, path: &[String], kvs: Vec<(String, Value)>) {
    if path.is_empty() {
        for (k, v) in kvs {
            root.insert(k, v);
        }
        return;
    }

    let mut cur = root;
    for (i, key) in path.iter().enumerate() {
        let is_last = i + 1 == path.len();
        let entry = cur
            .entry(key.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        let obj = entry.as_object_mut().expect("just ensured object");
        if is_last {
            for (k, v) in kvs {
                obj.insert(k, v);
            }
            return;
        }
        cur = obj;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Port of Go `TestTracingHandler_InjectsTraceContext`.
    #[test]
    fn injects_trace_context() {
        let h = TracingHandler::new("test-svc", "test", AppMode::Cli);
        let span = SpanContextIds {
            trace_id: Some("0102030405060708090a0b0c0d0e0f10".to_string()),
            span_id: Some("0102030405060708".to_string()),
        };
        let rec = h.build_record("test message", &span, &[]);

        assert_eq!(rec["trace_id"], json!("0102030405060708090a0b0c0d0e0f10"));
        assert_eq!(rec["span_id"], json!("0102030405060708"));
        assert_eq!(rec["service"], json!("test-svc"));
        assert_eq!(rec["env"], json!("test"));
        assert_eq!(rec["mode"], json!("cli"));
    }

    /// Port of Go `TestTracingHandler_NoTraceContext`.
    #[test]
    fn no_trace_context() {
        let h = TracingHandler::new("codefang", "", AppMode::Mcp);
        let rec = h.build_record("no span", &SpanContextIds::default(), &[]);

        assert!(rec.get("trace_id").is_none());
        assert_eq!(rec["service"], json!("codefang"));
        assert_eq!(rec["mode"], json!("mcp"));
        // env omitted when empty.
        assert!(rec.get("env").is_none());
    }

    /// Port of Go `TestTracingHandler_WithGroup`.
    #[test]
    fn with_group() {
        let h = TracingHandler::new("codefang", "", AppMode::Cli);
        let grouped = h.with_group("pipeline");
        let rec = grouped.build_record(
            "stage done",
            &SpanContextIds::default(),
            &[("stage".to_string(), json!("blob"))],
        );

        // Service attrs at top level.
        assert_eq!(rec["service"], json!("codefang"));
        // Grouped attrs nested.
        assert_eq!(rec["pipeline"]["stage"], json!("blob"));
    }

    /// Port of Go `TestTracingHandler_WithAttrs`.
    #[test]
    fn with_attrs() {
        let h = TracingHandler::new("codefang", "", AppMode::Cli);
        let wa = h.with_attrs(&[("op".to_string(), json!("analyze"))]);
        let rec = wa.build_record("started", &SpanContextIds::default(), &[]);

        assert_eq!(rec["op"], json!("analyze"));
        assert_eq!(rec["service"], json!("codefang"));
    }
}
