package common

import (
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

// ResultBuilder provides generic result building capabilities for analyzers
type ResultBuilder struct{}

// NewResultBuilder creates a new ResultBuilder
func NewResultBuilder() *ResultBuilder {
	return &ResultBuilder{}
}

// BuildEmptyResult creates a standard empty result for when no data is found
func (rb *ResultBuilder) BuildEmptyResult(analyzerName string) analyze.Report {
	return analyze.Report{
		"analyzer_name": analyzerName,
		"total_items":   0,
		"message":       "No data found",
	}
}

// BuildCustomEmptyResult creates an empty result with custom fields
func (rb *ResultBuilder) BuildCustomEmptyResult(fields map[string]interface{}) analyze.Report {
	result := analyze.Report{}

	// Merge custom fields
	for key, value := range fields {
		result[key] = value
	}

	return result
}

// BuildBasicResult creates a basic result with common fields
func (rb *ResultBuilder) BuildBasicResult(analyzerName string, totalItems int, message string) analyze.Report {
	return analyze.Report{
		"analyzer_name": analyzerName,
		"total_items":   totalItems,
		"message":       message,
	}
}

// BuildDetailedResult creates a detailed result with custom fields
func (rb *ResultBuilder) BuildDetailedResult(analyzerName string, fields map[string]interface{}) analyze.Report {
	result := analyze.Report{
		"analyzer_name": analyzerName,
	}

	// Merge custom fields
	for key, value := range fields {
		result[key] = value
	}

	return result
}

// BuildCollectionResult creates a result with a collection of items
func (rb *ResultBuilder) BuildCollectionResult(analyzerName string, collectionKey string, items []map[string]interface{}, metrics map[string]interface{}, message string) analyze.Report {
	result := analyze.Report{
		"analyzer_name":          analyzerName,
		"total_" + collectionKey: len(items),
		collectionKey:            items,
		"message":                message,
	}

	// Add metrics
	for key, value := range metrics {
		result[key] = value
	}

	return result
}

// BuildMetricResult creates a result focused on metrics
func (rb *ResultBuilder) BuildMetricResult(analyzerName string, metrics map[string]interface{}, message string) analyze.Report {
	result := analyze.Report{
		"analyzer_name": analyzerName,
		"message":       message,
	}

	// Add metrics
	for key, value := range metrics {
		result[key] = value
	}

	return result
}
