package renderer

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Test constants.
const (
	testAnalyzerName = "test-analyzer"
)

// mockJSONData is test data for JSON serialization.
type mockJSONData struct {
	Value int `json:"value"`
}

// mockOutputData is test data for YAML serialization.
type mockOutputData struct {
	Text string `yaml:"text"`
}

// mockMetricsOutput is a test implementation of MetricsOutput.
type mockMetricsOutput struct {
	name       string
	jsonData   mockJSONData
	outputData mockOutputData
}

func (m *mockMetricsOutput) AnalyzerName() string {
	return m.name
}

func (m *mockMetricsOutput) ToJSON() any {
	return m.jsonData
}

func (m *mockMetricsOutput) ToYAML() any {
	return m.outputData
}

const testJSONValue = 42

func TestMetricsOutput_AnalyzerName(t *testing.T) {
	mock := &mockMetricsOutput{name: testAnalyzerName}

	// Verify interface compliance
	var _ MetricsOutput = mock

	assert.Equal(t, testAnalyzerName, mock.AnalyzerName())
}

func TestMetricsOutput_ToJSON(t *testing.T) {
	mock := &mockMetricsOutput{
		name:     testAnalyzerName,
		jsonData: mockJSONData{Value: testJSONValue},
	}

	// Verify interface compliance
	var _ MetricsOutput = mock

	result := mock.ToJSON()
	data, ok := result.(mockJSONData)
	assert.True(t, ok, "ToJSON should return mockJSONData")
	assert.Equal(t, testJSONValue, data.Value)
}

const testOutputText = "test-output-value"

func TestMetricsOutput_ToYAML(t *testing.T) {
	mock := &mockMetricsOutput{
		name:       testAnalyzerName,
		outputData: mockOutputData{Text: testOutputText},
	}

	// Verify interface compliance
	var _ MetricsOutput = mock

	result := mock.ToYAML()
	data, ok := result.(mockOutputData)
	require.True(t, ok, "ToYAML should return mockOutputData")

	got := data.Text
	assert.Equal(t, testOutputText, got)
}

func TestRenderMetricsJSON(t *testing.T) {
	mock := &mockMetricsOutput{
		name:     testAnalyzerName,
		jsonData: mockJSONData{Value: testJSONValue},
	}

	result, err := RenderMetricsJSON(mock)

	require.NoError(t, err)
	assert.Contains(t, string(result), `"value":42`)
}

func TestRenderMetricsJSON_NilInput(t *testing.T) {
	result, err := RenderMetricsJSON(nil)

	require.Error(t, err)
	assert.Nil(t, result)
}

func TestRenderMetricsYAML(t *testing.T) {
	mock := &mockMetricsOutput{
		name:       testAnalyzerName,
		outputData: mockOutputData{Text: testOutputText},
	}

	result, err := RenderMetricsYAML(mock)

	require.NoError(t, err)
	assert.Contains(t, string(result), "text: "+testOutputText)
}

func TestRenderMetricsYAML_NilInput(t *testing.T) {
	result, err := RenderMetricsYAML(nil)

	require.Error(t, err)
	assert.Nil(t, result)
}
