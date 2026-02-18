package observability

import (
	"context"

	"go.opentelemetry.io/otel/trace"
	"go.opentelemetry.io/otel/trace/embedded"
	nooptrace "go.opentelemetry.io/otel/trace/noop"
)

// filteringTracerProvider wraps a real TracerProvider and suppresses
// hot-path spans to keep trace volume manageable. Entire tracer names
// can be suppressed (returning noop tracers), and individual span names
// can be suppressed within otherwise-active tracers.
type filteringTracerProvider struct {
	embedded.TracerProvider

	delegate          trace.TracerProvider
	noop              trace.TracerProvider
	suppressedTracers map[string]bool
	suppressedSpans   map[string]bool
}

// NewFilteringTracerProvider wraps delegate so that hot-path spans are
// replaced with no-op spans. This drops per-commit/per-file/per-git-op
// spans while preserving structural pipeline spans.
func NewFilteringTracerProvider(delegate trace.TracerProvider) trace.TracerProvider {
	return &filteringTracerProvider{
		delegate: delegate,
		noop:     nooptrace.NewTracerProvider(),
		suppressedTracers: map[string]bool{
			"codefang.gitlib": true,
			"codefang.uast":   true,
		},
		suppressedSpans: map[string]bool{
			"codefang.analyzer.consume": true,
		},
	}
}

// Tracer returns a tracer for the given name, suppressing hot-path tracers.
func (f *filteringTracerProvider) Tracer(name string, opts ...trace.TracerOption) trace.Tracer {
	if f.suppressedTracers[name] {
		return f.noop.Tracer(name, opts...)
	}

	actual := f.delegate.Tracer(name, opts...)

	if len(f.suppressedSpans) > 0 {
		return &filteringTracer{
			delegate: actual,
			noop:     f.noop.Tracer(name, opts...),
			suppress: f.suppressedSpans,
		}
	}

	return actual
}

// filteringTracer wraps a real Tracer and returns noop spans for
// suppressed span names while delegating everything else.
type filteringTracer struct {
	embedded.Tracer

	delegate trace.Tracer
	noop     trace.Tracer
	suppress map[string]bool
}

// Start creates a span, returning a noop span for suppressed names.
func (f *filteringTracer) Start(ctx context.Context, name string, opts ...trace.SpanStartOption) (context.Context, trace.Span) {
	if f.suppress[name] {
		return f.noop.Start(ctx, name, opts...)
	}

	return f.delegate.Start(ctx, name, opts...)
}
