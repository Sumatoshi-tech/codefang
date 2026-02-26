package analyze_test

import (
	"context"
	"io"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// MockVisitorAnalyzer implements StaticAnalyzer and VisitorProvider.
type MockVisitorAnalyzer struct {
	visited bool
}

func (m *MockVisitorAnalyzer) Name() string        { return "mock_visitor" }
func (m *MockVisitorAnalyzer) Flag() string        { return "mock-visitor" }
func (m *MockVisitorAnalyzer) Description() string { return "Mock visitor" }
func (m *MockVisitorAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.NewDescriptor(analyze.ModeStatic, m.Name(), m.Description())
}
func (m *MockVisitorAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption { return nil }
func (m *MockVisitorAnalyzer) Configure(_ map[string]any) error                         { return nil }
func (m *MockVisitorAnalyzer) Analyze(_ *node.Node) (analyze.Report, error) {
	// Legacy analyze shouldn't be called if visitor is used.
	return analyze.Report{}, nil
}
func (m *MockVisitorAnalyzer) Thresholds() analyze.Thresholds { return nil }

func (m *MockVisitorAnalyzer) CreateAggregator() analyze.ResultAggregator           { return nil }
func (m *MockVisitorAnalyzer) FormatReport(_ analyze.Report, _ io.Writer) error     { return nil }
func (m *MockVisitorAnalyzer) FormatReportJSON(_ analyze.Report, _ io.Writer) error { return nil }
func (m *MockVisitorAnalyzer) FormatReportYAML(_ analyze.Report, _ io.Writer) error { return nil }
func (m *MockVisitorAnalyzer) FormatReportPlot(_ analyze.Report, _ io.Writer) error { return nil }
func (m *MockVisitorAnalyzer) FormatReportBinary(_ analyze.Report, _ io.Writer) error {
	return nil
}

func (m *MockVisitorAnalyzer) CreateVisitor() analyze.AnalysisVisitor {
	return &MockNodeVisitor{analyzer: m}
}

func (m *MockVisitorAnalyzer) GetReport() analyze.Report {
	return analyze.Report{"visited": m.visited}
}

type MockNodeVisitor struct {
	analyzer *MockVisitorAnalyzer
}

func (v *MockNodeVisitor) GetReport() analyze.Report {
	return v.analyzer.GetReport()
}

func (v *MockNodeVisitor) OnEnter(_ *node.Node, _ int) {
	v.analyzer.visited = true
}

func (v *MockNodeVisitor) OnExit(_ *node.Node, _ int) {}

func TestFactory_RunAnalyzers_WithVisitor(t *testing.T) {
	t.Parallel()

	mockAnalyzer := &MockVisitorAnalyzer{}
	factory := analyze.NewFactory([]analyze.StaticAnalyzer{})
	factory.RegisterAnalyzer(mockAnalyzer) // Register manually as types might mismatch with legacy interface slice?
	// NewFactory takes []StaticAnalyzer. MockVisitorAnalyzer satisfies StaticAnalyzer.
	// So explicit registration shouldn't be needed if I passed it to NewFactory.

	// Create new factory with our mock.
	factory = analyze.NewFactory([]analyze.StaticAnalyzer{mockAnalyzer})

	root := &node.Node{Type: node.UASTFile}

	reports, err := factory.RunAnalyzers(context.Background(), root, []string{"mock_visitor"})

	require.NoError(t, err)

	visited, ok := reports["mock_visitor"]["visited"].(bool)
	require.True(t, ok, "type assertion failed for visited")
	assert.True(t, visited)
}
