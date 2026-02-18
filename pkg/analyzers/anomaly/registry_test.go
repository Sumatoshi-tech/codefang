package anomaly

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// withIsolatedRegistry saves the global extractors, replaces them with an empty
// map, and restores the originals when the test finishes. All access goes
// through the mutex so there is no race with other parallel tests.
func withIsolatedRegistry(t *testing.T) {
	t.Helper()

	timeSeriesExtractorsMu.Lock()
	original := timeSeriesExtractors
	timeSeriesExtractors = make(map[string]TimeSeriesExtractor)
	timeSeriesExtractorsMu.Unlock()

	t.Cleanup(func() {
		timeSeriesExtractorsMu.Lock()
		timeSeriesExtractors = original
		timeSeriesExtractorsMu.Unlock()
	})
}

func TestRegisterTimeSeriesExtractor(t *testing.T) {
	t.Parallel()
	withIsolatedRegistry(t)

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
