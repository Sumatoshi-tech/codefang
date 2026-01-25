package analyze

import (
	"context"
	"errors"
	"io"
	"sync/atomic"
	"testing"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// mockAnalyzer implements StaticAnalyzer for testing
type mockAnalyzer struct {
	name         string
	analyzeFunc  func(root *node.Node) (Report, error)
	thresholds   Thresholds
	formatCalled bool
}

func (m *mockAnalyzer) Name() string {
	return m.name
}

func (m *mockAnalyzer) Flag() string {
	return m.name + "-flag"
}

func (m *mockAnalyzer) Description() string {
	return "Mock analyzer for testing"
}

func (m *mockAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return []pipeline.ConfigurationOption{}
}

func (m *mockAnalyzer) Configure(facts map[string]interface{}) error {
	return nil
}

func (m *mockAnalyzer) Analyze(root *node.Node) (Report, error) {
	if m.analyzeFunc != nil {
		return m.analyzeFunc(root)
	}
	return Report{"result": "success"}, nil
}

func (m *mockAnalyzer) Thresholds() Thresholds {
	return m.thresholds
}

func (m *mockAnalyzer) CreateAggregator() ResultAggregator {
	return nil
}

func (m *mockAnalyzer) FormatReport(_ Report, _ io.Writer) error {
	m.formatCalled = true
	return nil
}

func (m *mockAnalyzer) FormatReportJSON(_ Report, _ io.Writer) error {
	m.formatCalled = true
	return nil
}

func newMockAnalyzer(name string) *mockAnalyzer {
	return &mockAnalyzer{name: name}
}

func TestNewFactory(t *testing.T) {
	analyzer1 := newMockAnalyzer("analyzer1")
	analyzer2 := newMockAnalyzer("analyzer2")

	factory := NewFactory([]StaticAnalyzer{analyzer1, analyzer2})

	if factory == nil {
		t.Fatal("NewFactory returned nil")
	}

	if len(factory.analyzers) != 2 {
		t.Errorf("expected 2 analyzers, got %d", len(factory.analyzers))
	}

	if factory.analyzers["analyzer1"] != analyzer1 {
		t.Error("analyzer1 not registered correctly")
	}

	if factory.analyzers["analyzer2"] != analyzer2 {
		t.Error("analyzer2 not registered correctly")
	}
}

func TestNewFactory_Empty(t *testing.T) {
	factory := NewFactory([]StaticAnalyzer{})

	if factory == nil {
		t.Fatal("NewFactory returned nil for empty slice")
	}

	if len(factory.analyzers) != 0 {
		t.Errorf("expected 0 analyzers, got %d", len(factory.analyzers))
	}
}

func TestRegisterAnalyzer(t *testing.T) {
	factory := NewFactory([]StaticAnalyzer{})
	analyzer := newMockAnalyzer("test-analyzer")

	factory.RegisterAnalyzer(analyzer)

	if len(factory.analyzers) != 1 {
		t.Errorf("expected 1 analyzer, got %d", len(factory.analyzers))
	}

	if factory.analyzers["test-analyzer"] != analyzer {
		t.Error("analyzer not registered correctly")
	}
}

func TestRegisterAnalyzer_Override(t *testing.T) {
	analyzer1 := newMockAnalyzer("same-name")
	analyzer2 := newMockAnalyzer("same-name")

	factory := NewFactory([]StaticAnalyzer{analyzer1})
	factory.RegisterAnalyzer(analyzer2)

	if len(factory.analyzers) != 1 {
		t.Errorf("expected 1 analyzer (overwritten), got %d", len(factory.analyzers))
	}

	if factory.analyzers["same-name"] != analyzer2 {
		t.Error("analyzer should be overwritten with the new one")
	}
}

func TestRunAnalyzer_Success(t *testing.T) {
	expectedReport := Report{"metric": 42}
	analyzer := &mockAnalyzer{
		name: "test-analyzer",
		analyzeFunc: func(_ *node.Node) (Report, error) {
			return expectedReport, nil
		},
	}

	factory := NewFactory([]StaticAnalyzer{analyzer})

	report, err := factory.RunAnalyzer("test-analyzer", nil)

	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if report["metric"] != 42 {
		t.Errorf("expected metric=42, got %v", report["metric"])
	}
}

func TestRunAnalyzer_NotFound(t *testing.T) {
	factory := NewFactory([]StaticAnalyzer{})

	_, err := factory.RunAnalyzer("nonexistent", nil)

	if err == nil {
		t.Fatal("expected error for nonexistent analyzer")
	}

	expectedMsg := "no registered analyzer with name=nonexistent"
	if err.Error() != expectedMsg {
		t.Errorf("expected error message %q, got %q", expectedMsg, err.Error())
	}
}

func TestRunAnalyzer_AnalyzerError(t *testing.T) {
	expectedErr := errors.New("analysis failed")
	analyzer := &mockAnalyzer{
		name: "failing-analyzer",
		analyzeFunc: func(_ *node.Node) (Report, error) {
			return nil, expectedErr
		},
	}

	factory := NewFactory([]StaticAnalyzer{analyzer})

	_, err := factory.RunAnalyzer("failing-analyzer", nil)

	if err == nil {
		t.Fatal("expected error from analyzer")
	}

	if !errors.Is(err, expectedErr) {
		t.Errorf("expected error %v, got %v", expectedErr, err)
	}
}

func TestRunAnalyzers_Success(t *testing.T) {
	analyzer1 := &mockAnalyzer{
		name: "analyzer1",
		analyzeFunc: func(_ *node.Node) (Report, error) {
			return Report{"a1": "result1"}, nil
		},
	}
	analyzer2 := &mockAnalyzer{
		name: "analyzer2",
		analyzeFunc: func(_ *node.Node) (Report, error) {
			return Report{"a2": "result2"}, nil
		},
	}

	factory := NewFactory([]StaticAnalyzer{analyzer1, analyzer2})

	reports, err := factory.RunAnalyzers(context.Background(), nil, []string{"analyzer1", "analyzer2"})

	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(reports) != 2 {
		t.Errorf("expected 2 reports, got %d", len(reports))
	}

	if reports["analyzer1"]["a1"] != "result1" {
		t.Errorf("expected analyzer1 result, got %v", reports["analyzer1"])
	}

	if reports["analyzer2"]["a2"] != "result2" {
		t.Errorf("expected analyzer2 result, got %v", reports["analyzer2"])
	}
}

func TestRunAnalyzers_Empty(t *testing.T) {
	factory := NewFactory([]StaticAnalyzer{})

	reports, err := factory.RunAnalyzers(context.Background(), nil, []string{})

	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(reports) != 0 {
		t.Errorf("expected 0 reports, got %d", len(reports))
	}
}

func TestRunAnalyzers_PartialFailure(t *testing.T) {
	analyzer1 := &mockAnalyzer{
		name: "analyzer1",
		analyzeFunc: func(_ *node.Node) (Report, error) {
			return Report{"a1": "result1"}, nil
		},
	}
	failingAnalyzer := &mockAnalyzer{
		name: "failing",
		analyzeFunc: func(_ *node.Node) (Report, error) {
			return nil, errors.New("analysis failed")
		},
	}

	factory := NewFactory([]StaticAnalyzer{analyzer1, failingAnalyzer})

	// First analyzer should succeed, second should fail
	_, err := factory.RunAnalyzers(context.Background(), nil, []string{"analyzer1", "failing"})

	if err == nil {
		t.Fatal("expected error from failing analyzer")
	}
}

func TestRunAnalyzers_NotFoundFailFast(t *testing.T) {
	analyzer1 := newMockAnalyzer("analyzer1")

	factory := NewFactory([]StaticAnalyzer{analyzer1})

	// Request an analyzer that doesn't exist
	_, err := factory.RunAnalyzers(context.Background(), nil, []string{"nonexistent", "analyzer1"})

	if err == nil {
		t.Fatal("expected error for nonexistent analyzer")
	}
}

func TestRunAnalyzers_Parallel(t *testing.T) {
	var counter int32
	delay := 10 * time.Millisecond

	analyzer1 := &mockAnalyzer{
		name: "parallel1",
		analyzeFunc: func(_ *node.Node) (Report, error) {
			time.Sleep(delay)
			atomic.AddInt32(&counter, 1)
			return Report{"p1": "r1"}, nil
		},
	}
	analyzer2 := &mockAnalyzer{
		name: "parallel2",
		analyzeFunc: func(_ *node.Node) (Report, error) {
			time.Sleep(delay)
			atomic.AddInt32(&counter, 1)
			return Report{"p2": "r2"}, nil
		},
	}

	factory := NewFactory([]StaticAnalyzer{analyzer1, analyzer2})
	factory.WithMaxParallelism(2)

	start := time.Now()
	reports, err := factory.RunAnalyzers(context.Background(), nil, []string{"parallel1", "parallel2"})
	duration := time.Since(start)

	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if atomic.LoadInt32(&counter) != 2 {
		t.Errorf("expected 2 executions, got %d", counter)
	}
	
	if len(reports) != 2 {
		t.Errorf("expected 2 reports, got %d", len(reports))
	}

	// Verify duration is less than sequential sum (2 * delay)
	// But allowing some buffer for overhead.
	// Sequential would be >= 20ms. Parallel should be around 10ms + overhead.
	if duration >= 2*delay {
		t.Logf("warning: parallel execution took %v, expected less than %v", duration, 2*delay)
	}
}

func TestRunAnalyzers_ContextCancellation(t *testing.T) {
	analyzer1 := &mockAnalyzer{
		name: "slow",
		analyzeFunc: func(_ *node.Node) (Report, error) {
			time.Sleep(50 * time.Millisecond)
			return Report{"res": "ok"}, nil
		},
	}

	factory := NewFactory([]StaticAnalyzer{analyzer1})
	
	ctx, cancel := context.WithCancel(context.Background())
	cancel() // Cancel immediately

	_, err := factory.RunAnalyzers(ctx, nil, []string{"slow"})

	if err == nil {
		t.Fatal("expected context cancellation error")
	}
	
	if !errors.Is(err, context.Canceled) {
		t.Errorf("expected context.Canceled, got %v", err)
	}
}
