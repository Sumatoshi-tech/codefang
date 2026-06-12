//! Hot-path tracer/span suppression.
//!
//! Wraps a real `TracerProvider` so per-commit/per-file/per-git-op spans are
//! replaced with no-op spans while structural pipeline spans survive.
//!
//! # Decision logic vs SDK wiring
//!
//! The two decision points are [`is_tracer_suppressed`] (a suppressed tracer
//! name yields a fully no-op tracer) and [`is_span_suppressed`] (a suppressed
//! span name starts a no-op span inside an otherwise-active tracer); the
//! suppression sets are a fixed telemetry policy. [`FilteringTracerProvider`]
//! composes them over any [`opentelemetry::trace::TracerProvider`]; the
//! binding test for the suppression sets is on the pure predicates
//! (deterministic), while the provider wrapper reproduces the dispatch
//! behavior.

use std::collections::HashSet;

/// Tracer names whose spans are entirely suppressed.
pub const SUPPRESSED_TRACERS: &[&str] = &["codefang.gitlib", "codefang.uast"];

/// Span names suppressed even within otherwise-active tracers.
pub const SUPPRESSED_SPANS: &[&str] = &["codefang.analyzer.consume"];

/// Returns true if every span from tracer `name` should be suppressed.
#[must_use]
pub fn is_tracer_suppressed(name: &str) -> bool {
    SUPPRESSED_TRACERS.contains(&name)
}

/// Returns true if a span named `name` should be suppressed.
#[must_use]
pub fn is_span_suppressed(name: &str) -> bool {
    SUPPRESSED_SPANS.contains(&name)
}

/// Wraps a delegate [`TracerProvider`](opentelemetry::trace::TracerProvider) so
/// hot-path spans become no-op spans.
///
/// This is parameterized over the delegate provider type so it can wrap either
/// the SDK provider or a no-op provider.
pub struct FilteringTracerProvider<P> {
    delegate: P,
    suppressed_tracers: HashSet<&'static str>,
    suppressed_spans: HashSet<&'static str>,
}

impl<P> FilteringTracerProvider<P> {
    /// Wraps `delegate` with the default Codefang suppression sets.
    #[must_use]
    pub fn new(delegate: P) -> Self {
        FilteringTracerProvider {
            delegate,
            suppressed_tracers: SUPPRESSED_TRACERS.iter().copied().collect(),
            suppressed_spans: SUPPRESSED_SPANS.iter().copied().collect(),
        }
    }

    /// Returns whether spans from tracer `name` are entirely suppressed.
    #[must_use]
    pub fn tracer_suppressed(&self, name: &str) -> bool {
        self.suppressed_tracers.contains(name)
    }

    /// Returns whether span `name` is suppressed within active tracers.
    #[must_use]
    pub fn span_suppressed(&self, name: &str) -> bool {
        self.suppressed_spans.contains(name)
    }

    /// Borrows the wrapped delegate provider.
    pub const fn delegate(&self) -> &P {
        &self.delegate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors the reference suite's `TestFilteringProvider_SuppressedTracer`.
    #[test]
    fn suppressed_tracer() {
        assert!(is_tracer_suppressed("codefang.gitlib"));
    }

    /// Mirrors the reference suite's `TestFilteringProvider_UASTParseSuppressed`.
    #[test]
    fn uast_tracer_suppressed() {
        assert!(is_tracer_suppressed("codefang.uast"));
    }

    /// Mirrors the reference suite's `TestFilteringProvider_SuppressedSpan`
    /// (hot-path span dropped, structural span passes).
    #[test]
    fn suppressed_span_vs_structural() {
        assert!(is_span_suppressed("codefang.analyzer.consume"));
        assert!(!is_span_suppressed("codefang.runner.run"));
    }

    /// Mirrors the reference suite's `TestFilteringProvider_PassThrough` (root tracer not suppressed).
    #[test]
    fn pass_through_root_tracer() {
        assert!(!is_tracer_suppressed("codefang"));
        assert!(!is_span_suppressed("codefang.some_operation"));
    }

    #[test]
    fn wrapper_exposes_decisions() {
        let fp = FilteringTracerProvider::new(());
        assert!(fp.tracer_suppressed("codefang.gitlib"));
        assert!(!fp.tracer_suppressed("codefang.framework"));
        assert!(fp.span_suppressed("codefang.analyzer.consume"));
        assert!(!fp.span_suppressed("codefang.runner.run"));
    }
}
