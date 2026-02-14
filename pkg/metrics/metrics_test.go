package metrics

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

// Test constants to avoid magic strings/numbers.
const (
	testMetricName        = "test_metric"
	testMetricName2       = "test_metric_2"
	testMetricDisplayName = "Test Metric"
	testMetricDescription = "A test metric for unit testing"
	testMetricType        = "aggregate"
	testInputValue        = 42
	testOutputMultiplier  = 2
)

// testMetric is a concrete implementation for testing the Metric interface.
type testMetric struct {
	MetricMeta
}

// Compute doubles the input value.
func (m *testMetric) Compute(input int) int {
	return input * testOutputMultiplier
}

// newTestMetric creates a test metric with standard metadata.
func newTestMetric() *testMetric {
	return &testMetric{
		MetricMeta: MetricMeta{
			MetricName:        testMetricName,
			MetricDisplayName: testMetricDisplayName,
			MetricDescription: testMetricDescription,
			MetricType:        testMetricType,
		},
	}
}

func TestMetricMeta_Name(t *testing.T) {
	t.Parallel()

	meta := MetricMeta{
		MetricName: testMetricName,
	}

	assert.Equal(t, testMetricName, meta.Name())
}

func TestMetricMeta_DisplayName(t *testing.T) {
	t.Parallel()

	meta := MetricMeta{
		MetricDisplayName: testMetricDisplayName,
	}

	assert.Equal(t, testMetricDisplayName, meta.DisplayName())
}

func TestMetricMeta_Description(t *testing.T) {
	t.Parallel()

	meta := MetricMeta{
		MetricDescription: testMetricDescription,
	}

	assert.Equal(t, testMetricDescription, meta.Description())
}

func TestMetricMeta_Type(t *testing.T) {
	t.Parallel()

	meta := MetricMeta{
		MetricType: testMetricType,
	}

	assert.Equal(t, testMetricType, meta.Type())
}

func TestNewRegistry(t *testing.T) {
	t.Parallel()

	registry := NewRegistry()

	assert.NotNil(t, registry)
	assert.NotNil(t, registry.metrics)
}

func TestRegistry_Names_Empty(t *testing.T) {
	t.Parallel()

	registry := NewRegistry()

	names := registry.Names()

	assert.Empty(t, names)
}

func TestRegistry_Register(t *testing.T) {
	t.Parallel()

	registry := NewRegistry()
	metric := newTestMetric()

	Register(registry, metric)

	assert.Len(t, registry.metrics, 1)
}

func TestRegistry_Get_Found(t *testing.T) {
	t.Parallel()

	registry := NewRegistry()
	metric := newTestMetric()
	Register(registry, metric)

	retrieved, found := registry.Get(testMetricName)

	assert.True(t, found)
	assert.Equal(t, metric, retrieved)
}

func TestRegistry_Get_NotFound(t *testing.T) {
	t.Parallel()

	registry := NewRegistry()

	retrieved, found := registry.Get("nonexistent_metric")

	assert.False(t, found)
	assert.Nil(t, retrieved)
}

func TestRegistry_Names(t *testing.T) {
	t.Parallel()

	registry := NewRegistry()
	metric1 := newTestMetric()
	metric2 := &testMetric{
		MetricMeta: MetricMeta{
			MetricName: testMetricName2,
		},
	}

	Register(registry, metric1)
	Register(registry, metric2)

	names := registry.Names()

	assert.Len(t, names, 2)
	assert.Contains(t, names, testMetricName)
	assert.Contains(t, names, testMetricName2)
}

func TestMetric_Compute(t *testing.T) {
	t.Parallel()

	metric := newTestMetric()

	result := metric.Compute(testInputValue)

	expectedOutput := testInputValue * testOutputMultiplier
	assert.Equal(t, expectedOutput, result)
}

func TestRiskLevel_Constants(t *testing.T) {
	t.Parallel()

	assert.Equal(t, RiskCritical, RiskLevel("CRITICAL"))
	assert.Equal(t, RiskHigh, RiskLevel("HIGH"))
	assert.Equal(t, RiskMedium, RiskLevel("MEDIUM"))
	assert.Equal(t, RiskLow, RiskLevel("LOW"))
}

func TestTimeSeriesPoint_Fields(t *testing.T) {
	t.Parallel()

	point := TimeSeriesPoint{
		Tick:  testInputValue,
		Value: float64(testInputValue),
	}

	assert.Equal(t, testInputValue, point.Tick)
	assert.InDelta(t, float64(testInputValue), point.Value, 0.001)
}

func TestRiskResult_Fields(t *testing.T) {
	t.Parallel()

	result := RiskResult{
		Value:     testInputValue,
		Level:     RiskHigh,
		Threshold: float64(testInputValue),
		Message:   testMetricDescription,
	}

	assert.Equal(t, testInputValue, result.Value)
	assert.Equal(t, RiskHigh, result.Level)
	assert.InDelta(t, float64(testInputValue), result.Threshold, 0.001)
	assert.Equal(t, testMetricDescription, result.Message)
}
