package common

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// MetricsProcessor handles extraction and calculation of metrics from reports
type MetricsProcessor struct {
	numericKeys []string
	countKeys   []string
	metrics     map[string]float64
	counts      map[string]int
	reportCount int
}

// NewMetricsProcessor creates a new MetricsProcessor with configurable key types
func NewMetricsProcessor(numericKeys, countKeys []string) *MetricsProcessor {
	return &MetricsProcessor{
		numericKeys: numericKeys,
		countKeys:   countKeys,
		metrics:     make(map[string]float64),
		counts:      make(map[string]int),
	}
}

// ProcessReport extracts metrics from a single report
func (mp *MetricsProcessor) ProcessReport(report analyze.Report) {
	if report == nil {
		return
	}

	mp.reportCount++

	// Process numeric metrics
	for key, value := range report {
		if mp.isNumericMetric(key) {
			if floatVal, ok := mp.extractFloat(value); ok {
				mp.metrics[key] += floatVal
			}
		}

		// Process count metrics
		if mp.isCountMetric(key) {
			if intVal, ok := mp.extractInt(value); ok {
				mp.counts[key] += intVal
			}
		}
	}
}

// CalculateAverages returns the calculated average metrics
func (mp *MetricsProcessor) CalculateAverages() map[string]float64 {
	averages := make(map[string]float64)

	for key, total := range mp.metrics {
		if mp.reportCount > 0 {
			averages[key] = total / float64(mp.reportCount)
		}
	}

	return averages
}

// GetCounts returns the total counts
func (mp *MetricsProcessor) GetCounts() map[string]int {
	return mp.counts
}

// GetReportCount returns the total report count
func (mp *MetricsProcessor) GetReportCount() int {
	return mp.reportCount
}

// GetMetric returns a specific metric total
func (mp *MetricsProcessor) GetMetric(key string) float64 {
	return mp.metrics[key]
}

// GetCount returns a specific count total
func (mp *MetricsProcessor) GetCount(key string) int {
	return mp.counts[key]
}

// isNumericMetric checks if a key represents a numeric metric
func (mp *MetricsProcessor) isNumericMetric(key string) bool {
	for _, numericKey := range mp.numericKeys {
		if key == numericKey {
			return true
		}
	}
	return false
}

// isCountMetric checks if a key represents a count metric
func (mp *MetricsProcessor) isCountMetric(key string) bool {
	for _, countKey := range mp.countKeys {
		if key == countKey {
			return true
		}
	}
	return false
}

// extractFloat safely extracts a float value
func (mp *MetricsProcessor) extractFloat(value interface{}) (float64, bool) {
	switch v := value.(type) {
	case float64:
		return v, true
	case int:
		return float64(v), true
	case int32:
		return float64(v), true
	case int64:
		return float64(v), true
	default:
		return 0, false
	}
}

// extractInt safely extracts an int value
func (mp *MetricsProcessor) extractInt(value interface{}) (int, bool) {
	switch v := value.(type) {
	case int:
		return v, true
	case int32:
		return int(v), true
	case int64:
		return int(v), true
	case float64:
		return int(v), true
	default:
		return 0, false
	}
}
