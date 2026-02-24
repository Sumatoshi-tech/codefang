package analyze_test

import (
	"testing"
	"time"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestTC_ZeroValue(t *testing.T) {
	t.Parallel()

	var tc analyze.TC

	if tc.CommitHash != (gitlib.Hash{}) {
		t.Errorf("expected zero Hash, got %v", tc.CommitHash)
	}

	if tc.Tick != 0 {
		t.Errorf("expected Tick=0, got %d", tc.Tick)
	}

	if tc.AuthorID != 0 {
		t.Errorf("expected AuthorID=0, got %d", tc.AuthorID)
	}

	if !tc.Timestamp.IsZero() {
		t.Errorf("expected zero Timestamp, got %v", tc.Timestamp)
	}

	if tc.Data != nil {
		t.Errorf("expected nil Data, got %v", tc.Data)
	}

	_ = time.Now // ensure time import is used.
}

func TestTC_WithData(t *testing.T) {
	t.Parallel()

	hash := gitlib.Hash{1, 2, 3}
	ts := time.Date(2024, 1, 15, 10, 30, 0, 0, time.UTC)

	tc := analyze.TC{
		CommitHash: hash,
		Tick:       7,
		AuthorID:   42,
		Timestamp:  ts,
		Data:       map[string]int{"lines": 100},
	}

	if tc.CommitHash != hash {
		t.Errorf("expected CommitHash=%v, got %v", hash, tc.CommitHash)
	}

	if tc.Tick != 7 {
		t.Errorf("expected Tick=7, got %d", tc.Tick)
	}

	if tc.AuthorID != 42 {
		t.Errorf("expected AuthorID=42, got %d", tc.AuthorID)
	}

	if !tc.Timestamp.Equal(ts) {
		t.Errorf("expected Timestamp=%v, got %v", ts, tc.Timestamp)
	}

	data, ok := tc.Data.(map[string]int)
	if !ok {
		t.Fatalf("expected Data to be map[string]int, got %T", tc.Data)
	}

	if data["lines"] != 100 {
		t.Errorf("expected lines=100, got %d", data["lines"])
	}
}

func TestTICK_ZeroValue(t *testing.T) {
	t.Parallel()

	var tick analyze.TICK

	if tick.Tick != 0 {
		t.Errorf("expected Tick=0, got %d", tick.Tick)
	}

	if !tick.StartTime.IsZero() {
		t.Errorf("expected zero StartTime, got %v", tick.StartTime)
	}

	if !tick.EndTime.IsZero() {
		t.Errorf("expected zero EndTime, got %v", tick.EndTime)
	}

	if tick.Data != nil {
		t.Errorf("expected nil Data, got %v", tick.Data)
	}
}

func TestTICK_WithData(t *testing.T) {
	t.Parallel()

	start := time.Date(2024, 1, 1, 0, 0, 0, 0, time.UTC)
	end := time.Date(2024, 1, 2, 0, 0, 0, 0, time.UTC)

	tick := analyze.TICK{
		Tick:      3,
		StartTime: start,
		EndTime:   end,
		Data:      []float64{1.0, 2.0, 3.0},
	}

	if tick.Tick != 3 {
		t.Errorf("expected Tick=3, got %d", tick.Tick)
	}

	if !tick.StartTime.Equal(start) {
		t.Errorf("expected StartTime=%v, got %v", start, tick.StartTime)
	}

	if !tick.EndTime.Equal(end) {
		t.Errorf("expected EndTime=%v, got %v", end, tick.EndTime)
	}

	data, ok := tick.Data.([]float64)
	if !ok {
		t.Fatalf("expected Data to be []float64, got %T", tick.Data)
	}

	if len(data) != 3 {
		t.Errorf("expected 3 data points, got %d", len(data))
	}
}

func TestAggregatorOptions_Defaults(t *testing.T) {
	t.Parallel()

	var opts analyze.AggregatorOptions

	if opts.SpillBudget != 0 {
		t.Errorf("expected SpillBudget=0, got %d", opts.SpillBudget)
	}

	if opts.SpillDir != "" {
		t.Errorf("expected empty SpillDir, got %q", opts.SpillDir)
	}

	if opts.Sampling != 0 {
		t.Errorf("expected Sampling=0, got %d", opts.Sampling)
	}

	if opts.Granularity != 0 {
		t.Errorf("expected Granularity=0, got %d", opts.Granularity)
	}
}

// mockAggregator is a minimal implementation of Aggregator for
// compile-time interface compliance verification.
type mockAggregator struct{}

func (m *mockAggregator) Add(_ analyze.TC) error                 { return nil }
func (m *mockAggregator) FlushTick(_ int) (analyze.TICK, error)  { return analyze.TICK{}, nil }
func (m *mockAggregator) FlushAllTicks() ([]analyze.TICK, error) { return nil, nil }
func (m *mockAggregator) Spill() (int64, error)                  { return 0, nil }
func (m *mockAggregator) Collect() error                         { return nil }
func (m *mockAggregator) EstimatedStateSize() int64              { return 0 }
func (m *mockAggregator) SpillState() analyze.AggregatorSpillInfo {
	return analyze.AggregatorSpillInfo{}
}
func (m *mockAggregator) RestoreSpillState(_ analyze.AggregatorSpillInfo) {}
func (m *mockAggregator) Close() error                                    { return nil }

// Compile-time interface compliance check.
var _ analyze.Aggregator = (*mockAggregator)(nil)

func TestAggregator_InterfaceCompliance(t *testing.T) {
	t.Parallel()

	var agg analyze.Aggregator = &mockAggregator{}

	addErr := agg.Add(analyze.TC{})
	if addErr != nil {
		t.Errorf("Add returned unexpected error: %v", addErr)
	}

	tick, flushErr := agg.FlushTick(0)
	if flushErr != nil {
		t.Errorf("FlushTick returned unexpected error: %v", flushErr)
	}

	if tick.Tick != 0 {
		t.Errorf("expected Tick=0, got %d", tick.Tick)
	}

	size := agg.EstimatedStateSize()
	if size != 0 {
		t.Errorf("expected EstimatedStateSize=0, got %d", size)
	}

	closeErr := agg.Close()
	if closeErr != nil {
		t.Errorf("Close returned unexpected error: %v", closeErr)
	}
}
