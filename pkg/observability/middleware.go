package observability

import (
	"fmt"
	"net/http"

	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/codes"
	"go.opentelemetry.io/otel/propagation"
	semconv "go.opentelemetry.io/otel/semconv/v1.26.0"
	"go.opentelemetry.io/otel/trace"
)

// httpStatusServerError is the threshold for HTTP server errors.
const httpStatusServerError = 500

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

// HTTPMiddleware returns an [http.Handler] that creates a span per request.
// Span names use route-template format: "METHOD /path".
func HTTPMiddleware(tracer trace.Tracer, next http.Handler) http.Handler {
	return http.HandlerFunc(func(rw http.ResponseWriter, hr *http.Request) {
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
		next.ServeHTTP(sw, hr.WithContext(ctx))

		span.SetAttributes(semconv.HTTPResponseStatusCode(sw.statusCode))

		if sw.statusCode >= httpStatusServerError {
			span.SetStatus(codes.Error, http.StatusText(sw.statusCode))
		}
	})
}
