package observability_test

import (
	"context"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/observability"
)

func TestInit_NoopWhenNoEndpoint(t *testing.T) {
	t.Parallel()

	cfg := observability.DefaultConfig()

	providers, err := observability.Init(cfg)
	require.NoError(t, err)

	assert.NotNil(t, providers.Tracer)
	assert.NotNil(t, providers.Meter)
	assert.NotNil(t, providers.Logger)
	assert.NotNil(t, providers.Shutdown)

	// Shutdown should succeed without error.
	err = providers.Shutdown(context.Background())
	assert.NoError(t, err)
}

func TestInit_NoopSpanIsValid(t *testing.T) {
	t.Parallel()

	cfg := observability.DefaultConfig()

	providers, err := observability.Init(cfg)
	require.NoError(t, err)

	t.Cleanup(func() { require.NoError(t, providers.Shutdown(context.Background())) })

	// Creating a span should work even in no-op mode.
	ctx, span := providers.Tracer.Start(context.Background(), "test-op")
	defer span.End()

	assert.NotNil(t, ctx)
	assert.NotNil(t, span)
}

func TestInit_WithResourceAttributes(t *testing.T) {
	t.Parallel()

	cfg := observability.DefaultConfig()
	cfg.ServiceVersion = "1.2.3"
	cfg.Environment = "test"
	cfg.Mode = observability.ModeMCP

	providers, err := observability.Init(cfg)
	require.NoError(t, err)

	t.Cleanup(func() { require.NoError(t, providers.Shutdown(context.Background())) })

	// Providers should still be valid with custom resource attributes.
	assert.NotNil(t, providers.Tracer)
	assert.NotNil(t, providers.Meter)
}

func TestInit_LoggerHasTracingHandler(t *testing.T) {
	t.Parallel()

	cfg := observability.DefaultConfig()
	cfg.LogJSON = true

	providers, err := observability.Init(cfg)
	require.NoError(t, err)

	t.Cleanup(func() { require.NoError(t, providers.Shutdown(context.Background())) })

	// The logger returned by Init should be non-nil and usable.
	assert.NotNil(t, providers.Logger)

	// Should not panic when logging with context.
	providers.Logger.InfoContext(context.Background(), "init test")
}

func TestInit_ShutdownIdempotent(t *testing.T) {
	t.Parallel()

	cfg := observability.DefaultConfig()

	providers, err := observability.Init(cfg)
	require.NoError(t, err)

	// Multiple shutdowns should not panic or error.
	require.NoError(t, providers.Shutdown(context.Background()))
	require.NoError(t, providers.Shutdown(context.Background()))
}

func TestParseOTLPHeaders(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name  string
		input string
		want  map[string]string
	}{
		{"empty", "", nil},
		{"single", "key=value", map[string]string{"key": "value"}},
		{"multiple", "k1=v1,k2=v2", map[string]string{"k1": "v1", "k2": "v2"}},
		{"spaces", " k1 = v1 , k2 = v2 ", map[string]string{"k1": "v1", "k2": "v2"}},
		{"no_equals", "invalid", nil},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			got := observability.ParseOTLPHeaders(tt.input)
			assert.Equal(t, tt.want, got)
		})
	}
}

func TestBuildResource_IncludesAppMode(t *testing.T) {
	t.Parallel()

	cfg := observability.DefaultConfig()
	cfg.Mode = observability.ModeMCP

	res, err := observability.ProbeBuildResource(cfg)
	require.NoError(t, err)

	found := false

	for _, attr := range res.Attributes() {
		if string(attr.Key) == "app.mode" {
			assert.Equal(t, "mcp", attr.Value.AsString())

			found = true
		}
	}

	assert.True(t, found, "app.mode attribute not found in resource")
}

func TestSampler_AlwaysOn(t *testing.T) {
	t.Setenv("OTEL_TRACES_SAMPLER", "always_on")

	assert.True(t, observability.ProbeSamplerSpan(observability.DefaultConfig()))
}

func TestSampler_AlwaysOff(t *testing.T) {
	t.Setenv("OTEL_TRACES_SAMPLER", "always_off")

	assert.False(t, observability.ProbeSamplerSpan(observability.DefaultConfig()))
}

func TestSampler_TraceIDRatio(t *testing.T) {
	// Ratio 1.0 should always sample.
	t.Setenv("OTEL_TRACES_SAMPLER", "traceidratio")
	t.Setenv("OTEL_TRACES_SAMPLER_ARG", "1.0")

	assert.True(t, observability.ProbeSamplerSpan(observability.DefaultConfig()))
}

func TestSampler_ParentBasedAlwaysOn(t *testing.T) {
	t.Setenv("OTEL_TRACES_SAMPLER", "parentbased_always_on")

	assert.True(t, observability.ProbeSamplerSpan(observability.DefaultConfig()))
}

func TestSampler_ParentBasedAlwaysOff(t *testing.T) {
	t.Setenv("OTEL_TRACES_SAMPLER", "parentbased_always_off")

	// Root span with no parent: parentbased_always_off drops root spans.
	assert.False(t, observability.ProbeSamplerSpan(observability.DefaultConfig()))
}

func TestSampler_DebugTraceOverridesEnv(t *testing.T) {
	t.Setenv("OTEL_TRACES_SAMPLER", "always_off")

	cfg := observability.DefaultConfig()
	cfg.DebugTrace = true

	// DebugTrace forces always-on even when env says always_off.
	assert.True(t, observability.ProbeSamplerSpan(cfg))
}

func TestSampler_ConfigSampleRatioFallback(t *testing.T) {
	t.Parallel()

	cfg := observability.DefaultConfig()
	cfg.SampleRatio = 1.0

	// Full ratio samples everything.
	assert.True(t, observability.ProbeSamplerSpan(cfg))
}

func TestSampler_DefaultSamples(t *testing.T) {
	t.Parallel()
	// Default (parent-based always-on) should sample root spans.
	assert.True(t, observability.ProbeSamplerSpan(observability.DefaultConfig()))
}
