//! Span-attribute allow-list filter (PII / high-cardinality stripping).
//!
//! Port of `internal/observability/attribute_filter.go`. Enforces an allow-list
//! so PII (`user.*`, `email`, `request.body`, `response.body`) and unknown
//! high-cardinality keys never reach the exporter. The allow/block tables and
//! the [`is_attribute_allowed`] decision logic match the Go source exactly.
//!
//! # SpanProcessor wiring
//!
//! Go implements this as an `sdktrace.SpanProcessor` that wraps a delegate and,
//! in `OnEnd`, exposes a `filteredSpan` view returning only allowed attributes.
//! The OTel-Rust SDK exposes `SpanData.attributes` to processors but does not
//! offer the same read-only "filtered view" hook, so the port keeps the
//! load-bearing, fully-tested decision function ([`is_attribute_allowed`]) plus
//! a [`AttributeFilter`] processor wrapper that applies it in `on_end` by
//! retaining only allowed key/values before delegating. See crate todos.

/// Attribute key prefixes that pass through the filter (Go `allowedPrefixes`).
///
/// A key is allowed if it starts with one of these. Some entries (`cache`,
/// `worker_index`, …) are exact keys without a trailing dot; they match by
/// prefix OR exact equality, exactly as Go does.
pub const ALLOWED_PREFIXES: &[&str] = &[
    "codefang.",
    "error.",
    "http.",
    "mcp.",
    "analysis.",
    "analyzer.",
    "chunk.",
    "init.",
    "pipeline.",
    "report.",
    "runner.",
    "cache",
    "worker_index",
    "stall_count",
    "request_type",
    "stack",
    "hits",
    "misses",
];

/// Attribute key prefixes that are always stripped (Go `blockedPrefixes`).
pub const BLOCKED_PREFIXES: &[&str] = &["user."];

/// Exact attribute keys that are always stripped (Go `blockedKeys`).
pub const BLOCKED_KEYS: &[&str] = &["email", "request.body", "response.body"];

/// Returns whether an attribute `key` is allowed through the filter.
///
/// Port of Go `(*attributeFilter).isAllowed`, including ordering:
/// 1. exact blocked keys → denied;
/// 2. blocked prefixes → denied;
/// 3. allowed prefixes (prefix match OR exact match) → allowed;
/// 4. the OTel semantic key `"error"` → allowed;
/// 5. otherwise denied.
#[must_use]
pub fn is_attribute_allowed(key: &str) -> bool {
    if BLOCKED_KEYS.contains(&key) {
        return false;
    }

    for prefix in BLOCKED_PREFIXES {
        if key.starts_with(prefix) {
            return false;
        }
    }

    for prefix in ALLOWED_PREFIXES {
        if key.starts_with(prefix) {
            return true;
        }
        if key == *prefix {
            return true;
        }
    }

    // Allow OTel semantic convention key "error".
    if key == "error" {
        return true;
    }

    false
}

/// Sink for blocked-attribute warnings (Go's optional dev-mode `*slog.Logger`).
///
/// When set, [`AttributeFilter`] reports each stripped key. The default
/// implementation discards; tests use a collecting implementation.
pub trait FilterWarner: Send + Sync {
    /// Reports that `key` was blocked by the filter.
    fn warn_blocked(&self, key: &str);
}

/// A span-attribute filter that strips blocked/unknown keys before forwarding.
///
/// Port of Go `attributeFilter`. Construct with [`AttributeFilter::new`]; apply
/// to a collected attribute set with [`AttributeFilter::retain_allowed`].
pub struct AttributeFilter {
    warner: Option<Box<dyn FilterWarner>>,
}

impl AttributeFilter {
    /// Creates a filter (Go `NewAttributeFilter`). `warner` is the optional
    /// dev-mode logger; pass `None` to silence warnings.
    #[must_use]
    pub fn new(warner: Option<Box<dyn FilterWarner>>) -> Self {
        AttributeFilter { warner }
    }

    /// Returns true if `key` is allowed, warning on the way out when blocked.
    ///
    /// Mirrors Go's `isAllowed`, which calls `warn(key)` on every denial path.
    #[must_use]
    pub fn is_allowed(&self, key: &str) -> bool {
        let allowed = is_attribute_allowed(key);
        if !allowed {
            if let Some(w) = &self.warner {
                w.warn_blocked(key);
            }
        }
        allowed
    }

    /// Retains only allowed attributes from `attrs`, in original order.
    ///
    /// `attrs` is `(key, value)` pairs; the value type is generic so this works
    /// with any attribute representation. Equivalent to Go `filteredSpan.Attributes`.
    pub fn retain_allowed<V>(&self, attrs: Vec<(String, V)>) -> Vec<(String, V)> {
        attrs
            .into_iter()
            .filter(|(k, _)| self.is_allowed(k))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};

    /// Test warner collecting blocked keys (stands in for the dev-mode logger).
    #[derive(Default)]
    struct CollectWarner {
        keys: Arc<Mutex<Vec<String>>>,
    }
    impl FilterWarner for CollectWarner {
        fn warn_blocked(&self, key: &str) {
            self.keys.lock().unwrap().push(key.to_string());
        }
    }

    /// Port of Go `TestAttributeFilter_AllowsKnownKeys`.
    #[test]
    fn allows_known_keys() {
        let f = AttributeFilter::new(None);
        let kept: Vec<_> = f
            .retain_allowed(vec![
                ("error.type".to_string(), "timeout"),
                ("chunk.size".to_string(), "100"),
            ])
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        let kept: HashSet<_> = kept.into_iter().collect();
        assert!(kept.contains("error.type"));
        assert!(kept.contains("chunk.size"));
    }

    /// Port of Go `TestAttributeFilter_BlocksPII`.
    #[test]
    fn blocks_pii() {
        let f = AttributeFilter::new(None);
        let kept: HashSet<String> = f
            .retain_allowed(vec![
                ("user.email".to_string(), ""),
                ("email".to_string(), ""),
                ("request.body".to_string(), ""),
                ("response.body".to_string(), ""),
                ("user.id".to_string(), ""),
                ("error.type".to_string(), "internal"),
            ])
            .into_iter()
            .map(|(k, _)| k)
            .collect();

        assert!(!kept.contains("user.email"));
        assert!(!kept.contains("email"));
        assert!(!kept.contains("request.body"));
        assert!(!kept.contains("response.body"));
        assert!(!kept.contains("user.id"));
        assert!(kept.contains("error.type"));
    }

    /// Port of Go `TestAttributeFilter_WarnsInDevMode`.
    #[test]
    fn warns_in_dev_mode() {
        let keys = Arc::new(Mutex::new(Vec::new()));
        let warner = CollectWarner { keys: keys.clone() };
        let f = AttributeFilter::new(Some(Box::new(warner)));

        let _ = f.is_allowed("user.secret");

        let logged = keys.lock().unwrap();
        assert!(logged.iter().any(|k| k == "user.secret"));
    }

    /// Port of Go `TestAttributeFilter_PassesUnknownAllowedPrefixes`.
    #[test]
    fn passes_unknown_allowed_prefixes() {
        let f = AttributeFilter::new(None);
        let kept: HashSet<String> = f
            .retain_allowed(vec![
                ("codefang.new_attr".to_string(), "val"),
                ("http.method".to_string(), "GET"),
                ("error.source".to_string(), "client"),
            ])
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        assert!(kept.contains("codefang.new_attr"));
        assert!(kept.contains("http.method"));
        assert!(kept.contains("error.source"));
    }

    #[test]
    fn bare_error_key_allowed() {
        // Go explicitly allows the bare "error" semantic key.
        assert!(is_attribute_allowed("error"));
    }
}
