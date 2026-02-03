package renderer

import (
	"encoding/json"
	"errors"
	"fmt"

	"gopkg.in/yaml.v3"
)

// ErrNilMetricsOutput is returned when nil is passed to render functions.
var ErrNilMetricsOutput = errors.New("metrics output is nil")

// MetricsOutput is implemented by analyzer ComputedMetrics structs
// to provide serializable output for JSON and YAML renderers.
// This interface establishes the metrics-first pipeline where all
// output (JSON, YAML, Plot) flows through computed metrics.
type MetricsOutput interface {
	// AnalyzerName returns the analyzer identifier (e.g., "devs", "burndown").
	AnalyzerName() string

	// ToJSON returns a struct suitable for JSON marshaling.
	// The returned value will be passed to json.Marshal.
	ToJSON() any

	// ToYAML returns a struct suitable for YAML marshaling.
	// The returned value will be passed to yaml.Marshal.
	// For most analyzers, this can return the same value as ToJSON.
	ToYAML() any
}

// RenderMetricsJSON serializes metrics output to JSON bytes.
func RenderMetricsJSON(m MetricsOutput) ([]byte, error) {
	if m == nil {
		return nil, ErrNilMetricsOutput
	}

	data, err := json.Marshal(m.ToJSON())
	if err != nil {
		return nil, fmt.Errorf("marshal metrics to JSON: %w", err)
	}

	return data, nil
}

// RenderMetricsYAML serializes metrics output to YAML bytes.
func RenderMetricsYAML(m MetricsOutput) ([]byte, error) {
	if m == nil {
		return nil, ErrNilMetricsOutput
	}

	data, err := yaml.Marshal(m.ToYAML())
	if err != nil {
		return nil, fmt.Errorf("marshal metrics to YAML: %w", err)
	}

	return data, nil
}
