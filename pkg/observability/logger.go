package observability

import (
	"context"
	"fmt"
	"log/slog"

	"go.opentelemetry.io/otel/trace"
)

const (
	attrTraceID = "trace_id"
	attrSpanID  = "span_id"
	attrService = "service"
	attrEnv     = "env"
	attrMode    = "mode"
)

// TracingHandler is an [slog.Handler] that injects OpenTelemetry trace context
// (trace_id, span_id) and service metadata into every log record.
// Service attributes (service, env, mode) are pre-attached at construction
// so they remain at the top level even when groups are used.
type TracingHandler struct {
	inner slog.Handler
}

// NewTracingHandler wraps an [slog.Handler], injecting trace context and service metadata.
// Service attributes are pre-attached to the inner handler so they appear at the
// top level regardless of subsequent WithGroup calls.
func NewTracingHandler(inner slog.Handler, service, env string, appMode AppMode) *TracingHandler {
	attrs := []slog.Attr{
		slog.String(attrService, service),
		slog.String(attrMode, string(appMode)),
	}

	if env != "" {
		attrs = append(attrs, slog.String(attrEnv, env))
	}

	return &TracingHandler{
		inner: inner.WithAttrs(attrs),
	}
}

// Enabled delegates to the inner handler.
func (th *TracingHandler) Enabled(ctx context.Context, level slog.Level) bool {
	return th.inner.Enabled(ctx, level)
}

// Handle adds trace context attributes from the span context, then delegates.
func (th *TracingHandler) Handle(ctx context.Context, record slog.Record) error {
	sc := trace.SpanContextFromContext(ctx)
	if sc.IsValid() {
		record.AddAttrs(
			slog.String(attrTraceID, sc.TraceID().String()),
			slog.String(attrSpanID, sc.SpanID().String()),
		)
	}

	err := th.inner.Handle(ctx, record)
	if err != nil {
		return fmt.Errorf("tracing handler: %w", err)
	}

	return nil
}

// WithAttrs returns a new TracingHandler with additional attributes on the inner handler.
func (th *TracingHandler) WithAttrs(attrs []slog.Attr) slog.Handler {
	return &TracingHandler{
		inner: th.inner.WithAttrs(attrs),
	}
}

// WithGroup returns a new TracingHandler with a group prefix on the inner handler.
func (th *TracingHandler) WithGroup(name string) slog.Handler {
	return &TracingHandler{
		inner: th.inner.WithGroup(name),
	}
}
