package observability

import (
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"runtime/debug"
	"time"

	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/codes"
	"go.opentelemetry.io/otel/propagation"
	semconv "go.opentelemetry.io/otel/semconv/v1.26.0"
	"go.opentelemetry.io/otel/trace"
)

// httpStatusServerError is the threshold for HTTP server errors.
const httpStatusServerError = 500

// Error type classification constants per OTel semantic conventions.
const (
	ErrTypeTimeout               = "timeout"
	ErrTypeCancel                = "cancel"
	ErrTypeValidation            = "validation"
	ErrTypeDependencyUnavailable = "dependency_unavailable"
	ErrTypeInternal              = "internal"
)

// Error source classification constants.
const (
	ErrSourceClient     = "client"
	ErrSourceServer     = "server"
	ErrSourceDependency = "dependency"
)

// RecordSpanError records an error on a span with structured classification
// attributes (error.type and optionally error.source).
func RecordSpanError(span trace.Span, err error, errType, errSource string) {
	span.RecordError(err)
	span.SetStatus(codes.Error, err.Error())

	attrs := []attribute.KeyValue{
		attribute.String("error.type", errType),
	}

	if errSource != "" {
		attrs = append(attrs, attribute.String("error.source", errSource))
	}

	span.SetAttributes(attrs...)
}

// errPanic is a sentinel error for recovered panics.
var errPanic = errors.New("panic recovered")

// statusWriter wraps [http.ResponseWriter] to capture the status code.
type statusWriter struct {
	http.ResponseWriter

	statusCode int
	written    bool
}

// WriteHeader captures the status code before delegating to the wrapped writer.
func (sw *statusWriter) WriteHeader(code int) {
	if !sw.written {
		sw.statusCode = code
		sw.written = true
	}

	sw.ResponseWriter.WriteHeader(code)
}

func (sw *statusWriter) Write(buf []byte) (int, error) {
	if !sw.written {
		sw.statusCode = http.StatusOK
		sw.written = true
	}

	n, err := sw.ResponseWriter.Write(buf)
	if err != nil {
		return n, fmt.Errorf("write response: %w", err)
	}

	return n, nil
}

// HTTPMiddleware returns an [http.Handler] that creates a span per request,
// emits a one-line access log, and recovers panics.
// Span names use route-template format: "METHOD /path".
func HTTPMiddleware(tracer trace.Tracer, logger *slog.Logger, next http.Handler) http.Handler {
	return http.HandlerFunc(func(rw http.ResponseWriter, hr *http.Request) {
		start := time.Now()
		spanName := hr.Method + " " + hr.URL.Path

		// Extract W3C traceparent/tracestate/baggage from incoming headers.
		parentCtx := otel.GetTextMapPropagator().Extract(hr.Context(), propagation.HeaderCarrier(hr.Header))

		ctx, span := tracer.Start(parentCtx, spanName,
			trace.WithSpanKind(trace.SpanKindServer),
			trace.WithAttributes(
				semconv.HTTPRequestMethodKey.String(hr.Method),
				attribute.String("http.target", hr.URL.Path),
			),
		)
		defer span.End()

		sw := &statusWriter{ResponseWriter: rw}

		defer func() {
			if r := recover(); r != nil {
				span.RecordError(fmt.Errorf("%w: %v", errPanic, r))
				span.SetStatus(codes.Error, "panic")
				span.SetAttributes(attribute.String("error.type", "panic"))
				span.AddEvent("panic.stack", trace.WithAttributes(
					attribute.String("stack", string(debug.Stack())),
				))
				sw.WriteHeader(http.StatusInternalServerError)
			}
		}()

		next.ServeHTTP(sw, hr.WithContext(ctx))

		span.SetAttributes(semconv.HTTPResponseStatusCode(sw.statusCode))

		if sw.statusCode >= httpStatusServerError {
			span.SetStatus(codes.Error, http.StatusText(sw.statusCode))
		}

		logger.InfoContext(ctx, "http.request",
			"method", hr.Method,
			"path", hr.URL.Path,
			"status", sw.statusCode,
			"duration_ms", time.Since(start).Milliseconds(),
		)
	})
}
