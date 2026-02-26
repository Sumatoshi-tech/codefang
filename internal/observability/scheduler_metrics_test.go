package observability_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/observability"

	"github.com/stretchr/testify/require"
	noopmetric "go.opentelemetry.io/otel/metric/noop"
)

func TestNewSchedulerMetrics_NoopMeter(t *testing.T) {
	t.Parallel()

	mt := noopmetric.NewMeterProvider().Meter("test")
	sm, err := observability.NewSchedulerMetrics(mt)

	require.NoError(t, err)
	require.NotNil(t, sm)
}
