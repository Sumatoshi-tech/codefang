package analyze_test

import (
	"io"
	"testing"
	"context"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
	"github.com/stretchr/testify/assert"
)

// MockVisitorAnalyzer implements StaticAnalyzer and VisitorProvider
type MockVisitorAnalyzer struct {
	visited bool
}

func (m *MockVisitorAnalyzer) Name() string { return "mock_visitor" }
func (m *MockVisitorAnalyzer) Flag() string { return "mock-visitor" }
func (m *MockVisitorAnalyzer) Description() string { return "Mock visitor" }
func (m *MockVisitorAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption { return nil }
func (m *MockVisitorAnalyzer) Configure(facts map[string]interface{}) error { return nil }
func (m *MockVisitorAnalyzer) Analyze(root *node.Node) (analyze.Report, error) {
	// Legacy analyze shouldn't be called if visitor is used
	return nil, nil
}
func (m *MockVisitorAnalyzer) Thresholds() analyze.Thresholds { return nil }
func (m *MockVisitorAnalyzer) CreateAggregator() analyze.ResultAggregator { return nil }
func (m *MockVisitorAnalyzer) FormatReport(report analyze.Report, w io.Writer) error { return nil }
func (m *MockVisitorAnalyzer) FormatReportJSON(report analyze.Report, w io.Writer) error { return nil }

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

func (v *MockNodeVisitor) OnEnter(n *node.Node, depth int) {
	v.analyzer.visited = true
}

func (v *MockNodeVisitor) OnExit(n *node.Node, depth int) {}

func TestFactory_RunAnalyzers_WithVisitor(t *testing.T) {
	mockAnalyzer := &MockVisitorAnalyzer{}
	factory := analyze.NewFactory([]analyze.StaticAnalyzer{})
	factory.RegisterAnalyzer(mockAnalyzer) // Register manually as types might mismatch with legacy interface slice?
    // NewFactory takes []StaticAnalyzer. MockVisitorAnalyzer satisfies StaticAnalyzer.
    // So explicit registration shouldn't be needed if I passed it to NewFactory.
    
    // Create new factory with our mock
    factory = analyze.NewFactory([]analyze.StaticAnalyzer{mockAnalyzer})

	root := &node.Node{Type: node.UASTFile}
	
	reports, err := factory.RunAnalyzers(context.Background(), root, []string{"mock_visitor"})
	
	assert.NoError(t, err)
	assert.True(t, reports["mock_visitor"]["visited"].(bool))
}
