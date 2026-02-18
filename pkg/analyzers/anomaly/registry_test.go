package anomaly

import (
	"maps"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestRegisterTimeSeriesExtractor(t *testing.T) {
	t.Parallel()

	// Save and restore global state.
	original := make(map[string]TimeSeriesExtractor)
	maps.Copy(original, timeSeriesExtractors)

	t.Cleanup(func() {
		timeSeriesExtractors = original
	})

	called := false
	fn := func(_ analyze.Report) ([]int, map[string][]float64) {
		called = true

		return []int{0, 1}, map[string][]float64{"dim": {1.0, 2.0}}
	}

	RegisterTimeSeriesExtractor("test-analyzer", fn)

	got := TimeSeriesExtractorFor("test-analyzer")
	require.NotNil(t, got)

	ticks, dims := got(analyze.Report{})

	assert.True(t, called)
	assert.Equal(t, []int{0, 1}, ticks)
	assert.Equal(t, map[string][]float64{"dim": {1.0, 2.0}}, dims)
}

func TestTimeSeriesExtractorFor_NotRegistered(t *testing.T) {
	t.Parallel()

	got := TimeSeriesExtractorFor("nonexistent-analyzer")
	assert.Nil(t, got)
}
