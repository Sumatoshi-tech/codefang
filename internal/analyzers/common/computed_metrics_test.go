package common_test

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common"
)

const (
	testAnalyzerName = "test_analyzer"
	testMetricName   = "test_metric"
	testMetricName2  = "test_metric_2"
	testDisplay      = "Test Metric"
	testDescription  = "A test metric."
	testTypeScalar   = "scalar"
	testTypeList     = "list"
	testScalarValue  = 42
)

func TestComputeAllMetrics_EmptyComputers(t *testing.T) {
	t.Parallel()

	report := analyze.Report{}

	ms := common.ComputeAllMetrics(testAnalyzerName, nil, report)

	assert.Equal(t, testAnalyzerName, ms.AnalyzerName())
	assert.Empty(t, ms.Metrics())
}

func TestComputeAllMetrics_SingleComputer(t *testing.T) {
	t.Parallel()

	computer := func(_ analyze.Report) common.MetricResult {
		return common.MetricResult{
			Name:  testMetricName,
			Value: testScalarValue,
		}
	}

	ms := common.ComputeAllMetrics(testAnalyzerName, []func(analyze.Report) common.MetricResult{computer}, analyze.Report{})

	results := ms.Metrics()
	require.Len(t, results, 1)
	assert.Equal(t, testMetricName, results[0].Name)
	assert.Equal(t, testScalarValue, results[0].Value)
}

func TestComputeAllMetrics_MultipleComputers(t *testing.T) {
	t.Parallel()

	listValue := []string{"a", "b"}

	computers := []func(analyze.Report) common.MetricResult{
		func(_ analyze.Report) common.MetricResult {
			return common.MetricResult{
				Name:        testMetricName,
				Display:     testDisplay,
				Description: testDescription,
				Type:        testTypeScalar,
				Value:       testScalarValue,
			}
		},
		func(_ analyze.Report) common.MetricResult {
			return common.MetricResult{
				Name:  testMetricName2,
				Type:  testTypeList,
				Value: listValue,
			}
		},
	}

	ms := common.ComputeAllMetrics(testAnalyzerName, computers, analyze.Report{})

	results := ms.Metrics()
	require.Len(t, results, 2)
	assert.Equal(t, testMetricName, results[0].Name)
	assert.Equal(t, testScalarValue, results[0].Value)
	assert.Equal(t, testMetricName2, results[1].Name)
	assert.Equal(t, listValue, results[1].Value)
}

func TestComputeAllMetrics_ComputerReceivesReport(t *testing.T) {
	t.Parallel()

	const reportKey = "key"

	const reportValue = "hello"

	computer := func(r analyze.Report) common.MetricResult {
		val, ok := r[reportKey].(string)
		if !ok {
			return common.MetricResult{Name: testMetricName}
		}

		return common.MetricResult{
			Name:  testMetricName,
			Value: val,
		}
	}

	report := analyze.Report{reportKey: reportValue}
	ms := common.ComputeAllMetrics(testAnalyzerName, []func(analyze.Report) common.MetricResult{computer}, report)

	results := ms.Metrics()
	require.Len(t, results, 1)
	assert.Equal(t, reportValue, results[0].Value)
}

func TestMetricSet_AnalyzerName(t *testing.T) {
	t.Parallel()

	ms := common.ComputeAllMetrics(testAnalyzerName, nil, analyze.Report{})

	assert.Equal(t, testAnalyzerName, ms.AnalyzerName())
}

func TestMetricSet_ToJSON_EmptyMetrics(t *testing.T) {
	t.Parallel()

	ms := common.ComputeAllMetrics(testAnalyzerName, nil, analyze.Report{})

	result := ms.ToJSON()

	m, ok := result.(map[string]any)
	require.True(t, ok)
	assert.Empty(t, m)
}

func TestMetricSet_ToJSON_KeyedByName(t *testing.T) {
	t.Parallel()

	computers := []func(analyze.Report) common.MetricResult{
		func(_ analyze.Report) common.MetricResult {
			return common.MetricResult{Name: testMetricName, Value: testScalarValue}
		},
		func(_ analyze.Report) common.MetricResult {
			return common.MetricResult{Name: testMetricName2, Value: "text"}
		},
	}

	ms := common.ComputeAllMetrics(testAnalyzerName, computers, analyze.Report{})

	result := ms.ToJSON()

	m, ok := result.(map[string]any)
	require.True(t, ok)
	assert.Equal(t, testScalarValue, m[testMetricName])
	assert.Equal(t, "text", m[testMetricName2])
}

func TestMetricSet_ToYAML_MatchesToJSON(t *testing.T) {
	t.Parallel()

	computers := []func(analyze.Report) common.MetricResult{
		func(_ analyze.Report) common.MetricResult {
			return common.MetricResult{Name: testMetricName, Value: testScalarValue}
		},
	}

	ms := common.ComputeAllMetrics(testAnalyzerName, computers, analyze.Report{})

	assert.Equal(t, ms.ToJSON(), ms.ToYAML())
}

func TestMetricSet_Metrics_PreservesOrder(t *testing.T) {
	t.Parallel()

	const metricA = "aaa"

	const metricZ = "zzz"

	computers := []func(analyze.Report) common.MetricResult{
		func(_ analyze.Report) common.MetricResult {
			return common.MetricResult{Name: metricZ}
		},
		func(_ analyze.Report) common.MetricResult {
			return common.MetricResult{Name: metricA}
		},
	}

	ms := common.ComputeAllMetrics(testAnalyzerName, computers, analyze.Report{})

	results := ms.Metrics()
	require.Len(t, results, 2)
	assert.Equal(t, metricZ, results[0].Name)
	assert.Equal(t, metricA, results[1].Name)
}
