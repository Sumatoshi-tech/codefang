package observability

import (
	"context"

	"go.opentelemetry.io/otel/sdk/resource"
	sdktrace "go.opentelemetry.io/otel/sdk/trace"
	"go.opentelemetry.io/otel/sdk/trace/tracetest"
)

// ProbeBuildResource exposes buildResource for testing.
func ProbeBuildResource(cfg Config) (*resource.Resource, error) {
	return buildResource(cfg)
}

// ProbeSamplerSpan creates a span using the sampler resolved from cfg and
// returns whether the span was sampled. This tests sampler selection without
// exposing the Sampler interface.
func ProbeSamplerSpan(cfg Config) (sampled bool) {
	exporter := tracetest.NewInMemoryExporter()
	tp := sdktrace.NewTracerProvider(
		sdktrace.WithSyncer(exporter),
		sdktrace.WithSampler(selectSampler(cfg)),
	)

	_, span := tp.Tracer("test").Start(context.Background(), "probe")
	span.End()

	// Check spans before Shutdown, which clears the exporter.
	spans := exporter.GetSpans()

	shutdownErr := tp.Shutdown(context.Background())
	if shutdownErr != nil {
		return false
	}

	return len(spans) > 0
}
