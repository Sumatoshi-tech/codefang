package renderer_test

import (
	"encoding/json"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/burndown"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/cohesion"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/comments"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/renderer"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/complexity"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/couples"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/devs"
	filehistory "github.com/Sumatoshi-tech/codefang/pkg/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/halstead"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/imports"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/sentiment"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/shotness"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/typos"
)

// --- Interface Compliance Tests ---
// These tests verify that all analyzer ComputedMetrics types implement MetricsOutput.

func TestMetricsOutput_InterfaceCompliance_HistoryAnalyzers(t *testing.T) {
	t.Parallel()

	// Compile-time interface compliance checks.
	var (
		_ renderer.MetricsOutput = (*devs.ComputedMetrics)(nil)
		_ renderer.MetricsOutput = (*burndown.ComputedMetrics)(nil)
		_ renderer.MetricsOutput = (*filehistory.ComputedMetrics)(nil)
		_ renderer.MetricsOutput = (*couples.ComputedMetrics)(nil)
		_ renderer.MetricsOutput = (*shotness.ComputedMetrics)(nil)
		_ renderer.MetricsOutput = (*sentiment.ComputedMetrics)(nil)
	)

	// Runtime verification.
	tests := []struct {
		name     string
		metrics  renderer.MetricsOutput
		expected string
	}{
		{"devs", &devs.ComputedMetrics{}, "devs"},
		{"burndown", &burndown.ComputedMetrics{}, "burndown"},
		{"file_history", &filehistory.ComputedMetrics{}, "file_history"},
		{"couples", &couples.ComputedMetrics{}, "couples"},
		{"shotness", &shotness.ComputedMetrics{}, "shotness"},
		{"sentiment", &sentiment.ComputedMetrics{}, "sentiment"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			assert.Equal(t, tt.expected, tt.metrics.AnalyzerName())
			assert.NotNil(t, tt.metrics.ToJSON())
			assert.NotNil(t, tt.metrics.ToYAML())
		})
	}
}

func TestMetricsOutput_InterfaceCompliance_StaticAnalyzers(t *testing.T) {
	t.Parallel()

	// Compile-time interface compliance checks.
	var (
		_ renderer.MetricsOutput = (*complexity.ComputedMetrics)(nil)
		_ renderer.MetricsOutput = (*cohesion.ComputedMetrics)(nil)
		_ renderer.MetricsOutput = (*halstead.ComputedMetrics)(nil)
		_ renderer.MetricsOutput = (*comments.ComputedMetrics)(nil)
		_ renderer.MetricsOutput = (*imports.ComputedMetrics)(nil)
		_ renderer.MetricsOutput = (*typos.ComputedMetrics)(nil)
	)

	// Runtime verification.
	tests := []struct {
		name     string
		metrics  renderer.MetricsOutput
		expected string
	}{
		{"complexity", &complexity.ComputedMetrics{}, "complexity"},
		{"cohesion", &cohesion.ComputedMetrics{}, "cohesion"},
		{"halstead", &halstead.ComputedMetrics{}, "halstead"},
		{"comments", &comments.ComputedMetrics{}, "comments"},
		{"imports", &imports.ComputedMetrics{}, "imports"},
		{"typos", &typos.ComputedMetrics{}, "typos"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			assert.Equal(t, tt.expected, tt.metrics.AnalyzerName())
			assert.NotNil(t, tt.metrics.ToJSON())
			assert.NotNil(t, tt.metrics.ToYAML())
		})
	}
}

// --- JSON Output Structure Tests ---
// These tests verify that JSON output contains expected keys from ComputedMetrics.

func TestJSONOutputStructure_HistoryAnalyzers(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name         string
		metrics      renderer.MetricsOutput
		expectedKeys []string
	}{
		{
			name:         "devs",
			metrics:      &devs.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "developers", "languages", "busfactor", "activity", "churn"},
		},
		{
			name:         "burndown",
			metrics:      &burndown.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "global_survival", "file_survival", "developer_survival", "interactions"},
		},
		{
			name:         "file_history",
			metrics:      &filehistory.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "file_churn", "file_contributors", "hotspots"},
		},
		{
			name:         "couples",
			metrics:      &couples.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "file_coupling", "developer_coupling", "file_ownership"},
		},
		{
			name:         "shotness",
			metrics:      &shotness.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "node_hotness", "node_coupling", "hotspot_nodes"},
		},
		{
			name:         "sentiment",
			metrics:      &sentiment.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "time_series", "trend", "low_sentiment_periods"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			data, err := json.Marshal(tt.metrics.ToJSON())
			require.NoError(t, err)

			var result map[string]any

			err = json.Unmarshal(data, &result)
			require.NoError(t, err)

			for _, key := range tt.expectedKeys {
				assert.Contains(t, result, key, "JSON output should contain key: %s", key)
			}
		})
	}
}

func TestJSONOutputStructure_StaticAnalyzers(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name         string
		metrics      renderer.MetricsOutput
		expectedKeys []string
	}{
		{
			name:         "complexity",
			metrics:      &complexity.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "function_complexity", "distribution", "high_risk_functions"},
		},
		{
			name:         "cohesion",
			metrics:      &cohesion.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "function_cohesion", "distribution", "low_cohesion_functions"},
		},
		{
			name:         "halstead",
			metrics:      &halstead.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "function_halstead", "distribution", "high_effort_functions"},
		},
		{
			name:         "comments",
			metrics:      &comments.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "comment_quality", "function_documentation", "undocumented_functions"},
		},
		{
			name:         "imports",
			metrics:      &imports.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "import_list", "categories", "dependencies"},
		},
		{
			name:         "typos",
			metrics:      &typos.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "typo_list", "patterns", "file_typos"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			data, err := json.Marshal(tt.metrics.ToJSON())
			require.NoError(t, err)

			var result map[string]any

			err = json.Unmarshal(data, &result)
			require.NoError(t, err)

			for _, key := range tt.expectedKeys {
				assert.Contains(t, result, key, "JSON output should contain key: %s", key)
			}
		})
	}
}

// --- YAML Output Structure Tests ---
// These tests verify that YAML output contains expected keys from ComputedMetrics.

func TestYAMLOutputStructure_HistoryAnalyzers(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name         string
		metrics      renderer.MetricsOutput
		expectedKeys []string
	}{
		{
			name:         "devs",
			metrics:      &devs.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "developers", "languages", "busfactor", "activity", "churn"},
		},
		{
			name:         "burndown",
			metrics:      &burndown.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "global_survival", "file_survival", "developer_survival", "interactions"},
		},
		{
			name:         "file_history",
			metrics:      &filehistory.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "file_churn", "file_contributors", "hotspots"},
		},
		{
			name:         "couples",
			metrics:      &couples.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "file_coupling", "developer_coupling", "file_ownership"},
		},
		{
			name:         "shotness",
			metrics:      &shotness.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "node_hotness", "node_coupling", "hotspot_nodes"},
		},
		{
			name:         "sentiment",
			metrics:      &sentiment.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "time_series", "trend", "low_sentiment_periods"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			data, err := yaml.Marshal(tt.metrics.ToYAML())
			require.NoError(t, err)

			var result map[string]any

			err = yaml.Unmarshal(data, &result)
			require.NoError(t, err)

			for _, key := range tt.expectedKeys {
				assert.Contains(t, result, key, "YAML output should contain key: %s", key)
			}
		})
	}
}

func TestYAMLOutputStructure_StaticAnalyzers(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name         string
		metrics      renderer.MetricsOutput
		expectedKeys []string
	}{
		{
			name:         "complexity",
			metrics:      &complexity.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "function_complexity", "distribution", "high_risk_functions"},
		},
		{
			name:         "cohesion",
			metrics:      &cohesion.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "function_cohesion", "distribution", "low_cohesion_functions"},
		},
		{
			name:         "halstead",
			metrics:      &halstead.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "function_halstead", "distribution", "high_effort_functions"},
		},
		{
			name:         "comments",
			metrics:      &comments.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "comment_quality", "function_documentation", "undocumented_functions"},
		},
		{
			name:         "imports",
			metrics:      &imports.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "import_list", "categories", "dependencies"},
		},
		{
			name:         "typos",
			metrics:      &typos.ComputedMetrics{},
			expectedKeys: []string{"aggregate", "typo_list", "patterns", "file_typos"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			t.Parallel()

			data, err := yaml.Marshal(tt.metrics.ToYAML())
			require.NoError(t, err)

			var result map[string]any

			err = yaml.Unmarshal(data, &result)
			require.NoError(t, err)

			for _, key := range tt.expectedKeys {
				assert.Contains(t, result, key, "YAML output should contain key: %s", key)
			}
		})
	}
}

// --- Render Helper Tests ---
// These tests verify the RenderMetricsJSON and RenderMetricsYAML helpers work correctly.

func TestRenderMetricsJSON_AllAnalyzers(t *testing.T) {
	t.Parallel()

	analyzers := []renderer.MetricsOutput{
		&devs.ComputedMetrics{},
		&burndown.ComputedMetrics{},
		&filehistory.ComputedMetrics{},
		&couples.ComputedMetrics{},
		&shotness.ComputedMetrics{},
		&sentiment.ComputedMetrics{},
		&complexity.ComputedMetrics{},
		&cohesion.ComputedMetrics{},
		&halstead.ComputedMetrics{},
		&comments.ComputedMetrics{},
		&imports.ComputedMetrics{},
		&typos.ComputedMetrics{},
	}

	for _, m := range analyzers {
		t.Run(m.AnalyzerName(), func(t *testing.T) {
			t.Parallel()

			data, err := renderer.RenderMetricsJSON(m)
			require.NoError(t, err)
			assert.NotEmpty(t, data)

			// Verify it's valid JSON.
			var result map[string]any

			err = json.Unmarshal(data, &result)
			require.NoError(t, err)
		})
	}
}

func TestRenderMetricsYAML_AllAnalyzers(t *testing.T) {
	t.Parallel()

	analyzers := []renderer.MetricsOutput{
		&devs.ComputedMetrics{},
		&burndown.ComputedMetrics{},
		&filehistory.ComputedMetrics{},
		&couples.ComputedMetrics{},
		&shotness.ComputedMetrics{},
		&sentiment.ComputedMetrics{},
		&complexity.ComputedMetrics{},
		&cohesion.ComputedMetrics{},
		&halstead.ComputedMetrics{},
		&comments.ComputedMetrics{},
		&imports.ComputedMetrics{},
		&typos.ComputedMetrics{},
	}

	for _, m := range analyzers {
		t.Run(m.AnalyzerName(), func(t *testing.T) {
			t.Parallel()

			data, err := renderer.RenderMetricsYAML(m)
			require.NoError(t, err)
			assert.NotEmpty(t, data)

			// Verify it's valid YAML.
			var result map[string]any

			err = yaml.Unmarshal(data, &result)
			require.NoError(t, err)
		})
	}
}

// --- JSON/YAML Consistency Tests ---
// These tests verify JSON and YAML outputs have the same structure.

func TestJSONYAMLConsistency_AllAnalyzers(t *testing.T) {
	t.Parallel()

	analyzers := []renderer.MetricsOutput{
		&devs.ComputedMetrics{},
		&burndown.ComputedMetrics{},
		&filehistory.ComputedMetrics{},
		&couples.ComputedMetrics{},
		&shotness.ComputedMetrics{},
		&sentiment.ComputedMetrics{},
		&complexity.ComputedMetrics{},
		&cohesion.ComputedMetrics{},
		&halstead.ComputedMetrics{},
		&comments.ComputedMetrics{},
		&imports.ComputedMetrics{},
		&typos.ComputedMetrics{},
	}

	for _, m := range analyzers {
		t.Run(m.AnalyzerName(), func(t *testing.T) {
			t.Parallel()

			// Marshal to JSON.
			jsonData, err := json.Marshal(m.ToJSON())
			require.NoError(t, err)

			var jsonResult map[string]any

			err = json.Unmarshal(jsonData, &jsonResult)
			require.NoError(t, err)

			// Marshal to YAML.
			yamlData, err := yaml.Marshal(m.ToYAML())
			require.NoError(t, err)

			var yamlResult map[string]any

			err = yaml.Unmarshal(yamlData, &yamlResult)
			require.NoError(t, err)

			// Verify same top-level keys.
			jsonKeys := make([]string, 0, len(jsonResult))
			for k := range jsonResult {
				jsonKeys = append(jsonKeys, k)
			}

			yamlKeys := make([]string, 0, len(yamlResult))
			for k := range yamlResult {
				yamlKeys = append(yamlKeys, k)
			}

			assert.ElementsMatch(t, jsonKeys, yamlKeys,
				"JSON and YAML should have same top-level keys for %s", m.AnalyzerName())
		})
	}
}
