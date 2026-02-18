package analyze_test

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

func TestRegisterTickExtractor_StoresAndRetrieves(t *testing.T) { //nolint:paralleltest // writes to global map
	called := false

	extractor := func(_ analyze.Report) map[string]any {
		called = true

		return nil
	}

	analyze.RegisterTickExtractor("test-analyzer", extractor)

	got := analyze.TickExtractorFor("test-analyzer")
	if got == nil {
		t.Fatal("expected registered extractor, got nil")
	}

	got(nil)

	if !called {
		t.Error("expected extractor to be called")
	}
}

func TestTickExtractorFor_Unregistered(t *testing.T) { //nolint:paralleltest // reads global map
	got := analyze.TickExtractorFor("nonexistent-analyzer")
	if got != nil {
		t.Errorf("expected nil for unregistered analyzer, got %v", got)
	}
}

func TestMergedCommitData_MarshalJSON_Flattened(t *testing.T) {
	t.Parallel()

	entry := analyze.MergedCommitData{
		Hash:      "abc123",
		Timestamp: "2024-01-15T10:30:00Z",
		Author:    "alice",
		Tick:      0,
		Analyzers: map[string]any{
			"quality": map[string]any{"complexity_median": 5.2},
		},
	}

	data, err := json.Marshal(entry)
	if err != nil {
		t.Fatalf("MarshalJSON failed: %v", err)
	}

	var result map[string]any

	err = json.Unmarshal(data, &result)
	if err != nil {
		t.Fatalf("Unmarshal failed: %v", err)
	}

	if result["hash"] != "abc123" {
		t.Errorf("expected hash=abc123, got %v", result["hash"])
	}

	if result["author"] != "alice" {
		t.Errorf("expected author=alice, got %v", result["author"])
	}

	if result["timestamp"] != "2024-01-15T10:30:00Z" {
		t.Errorf("expected timestamp, got %v", result["timestamp"])
	}

	// tick comes back as float64 from JSON.
	if result["tick"] != float64(0) {
		t.Errorf("expected tick=0, got %v", result["tick"])
	}

	qualityRaw, ok := result["quality"]
	if !ok {
		t.Fatal("expected 'quality' key in flattened output")
	}

	qualityMap, ok := qualityRaw.(map[string]any)
	if !ok {
		t.Fatalf("expected quality to be map, got %T", qualityRaw)
	}

	if qualityMap["complexity_median"] != 5.2 {
		t.Errorf("expected complexity_median=5.2, got %v", qualityMap["complexity_median"])
	}

	// Verify "analyzers" key is NOT present (flattened, not nested).
	if _, exists := result["analyzers"]; exists {
		t.Error("expected 'analyzers' key to be absent in flattened JSON")
	}
}

func TestBuildMergedTimeSeries_EmptyReports(t *testing.T) {
	t.Parallel()

	reports := make(map[string]analyze.Report)

	ts := analyze.BuildMergedTimeSeries(reports, nil, 0)

	if ts.Version != analyze.TimeSeriesModelVersion {
		t.Errorf("expected version=%s, got %s", analyze.TimeSeriesModelVersion, ts.Version)
	}

	if len(ts.Commits) != 0 {
		t.Errorf("expected 0 commits, got %d", len(ts.Commits))
	}

	if len(ts.Analyzers) != 0 {
		t.Errorf("expected 0 analyzers, got %d", len(ts.Analyzers))
	}
}

func TestBuildMergedTimeSeries_SingleAnalyzer(t *testing.T) { //nolint:paralleltest // writes to global map
	analyze.RegisterTickExtractor("build-test-single", func(_ analyze.Report) map[string]any {
		return map[string]any{
			"aaa111": map[string]any{"score": 10},
			"bbb222": map[string]any{"score": 20},
		}
	})

	reports := map[string]analyze.Report{
		"build-test-single": {},
	}

	meta := []analyze.CommitMeta{
		{Hash: "aaa111", Timestamp: "2024-01-01T00:00:00Z", Author: "alice", Tick: 0},
		{Hash: "bbb222", Timestamp: "2024-01-02T00:00:00Z", Author: "bob", Tick: 1},
	}

	ts := analyze.BuildMergedTimeSeries(reports, meta, 0)

	if len(ts.Analyzers) != 1 {
		t.Fatalf("expected 1 analyzer, got %d", len(ts.Analyzers))
	}

	if ts.Analyzers[0] != "build-test-single" {
		t.Errorf("expected analyzer=build-test-single, got %s", ts.Analyzers[0])
	}

	if len(ts.Commits) != 2 {
		t.Fatalf("expected 2 commits, got %d", len(ts.Commits))
	}

	// Sorted by commit order (meta order).
	if ts.Commits[0].Hash != "aaa111" {
		t.Errorf("expected first commit=aaa111, got %s", ts.Commits[0].Hash)
	}

	if ts.Commits[1].Hash != "bbb222" {
		t.Errorf("expected second commit=bbb222, got %s", ts.Commits[1].Hash)
	}

	if ts.Commits[0].Author != "alice" {
		t.Errorf("expected author=alice, got %s", ts.Commits[0].Author)
	}
}

func TestBuildMergedTimeSeries_MultiAnalyzer(t *testing.T) { //nolint:paralleltest // writes to global map
	analyze.RegisterTickExtractor("build-test-multi-a", func(_ analyze.Report) map[string]any {
		return map[string]any{
			"commit1": map[string]any{"metric_a": 1},
		}
	})

	analyze.RegisterTickExtractor("build-test-multi-b", func(_ analyze.Report) map[string]any {
		return map[string]any{
			"commit1": map[string]any{"metric_b": 2},
		}
	})

	reports := map[string]analyze.Report{
		"build-test-multi-a": {},
		"build-test-multi-b": {},
	}

	meta := []analyze.CommitMeta{
		{Hash: "commit1", Timestamp: "2024-01-01T00:00:00Z", Author: "alice", Tick: 0},
	}

	ts := analyze.BuildMergedTimeSeries(reports, meta, 0)

	if len(ts.Commits) != 1 {
		t.Fatalf("expected 1 commit, got %d", len(ts.Commits))
	}

	entry := ts.Commits[0]

	if len(entry.Analyzers) != 2 {
		t.Fatalf("expected 2 analyzer entries, got %d", len(entry.Analyzers))
	}

	if _, ok := entry.Analyzers["build-test-multi-a"]; !ok {
		t.Error("expected build-test-multi-a in analyzers")
	}

	if _, ok := entry.Analyzers["build-test-multi-b"]; !ok {
		t.Error("expected build-test-multi-b in analyzers")
	}
}

func TestWriteMergedTimeSeries_ValidJSON(t *testing.T) {
	t.Parallel()

	ts := &analyze.MergedTimeSeries{
		Version:       analyze.TimeSeriesModelVersion,
		TickSizeHours: 24,
		Analyzers:     []string{"quality"},
		Commits: []analyze.MergedCommitData{
			{
				Hash:      "abc",
				Timestamp: "2024-01-01T00:00:00Z",
				Author:    "alice",
				Tick:      0,
				Analyzers: map[string]any{"quality": map[string]any{"score": 5}},
			},
		},
	}

	var buf bytes.Buffer

	err := analyze.WriteMergedTimeSeries(ts, &buf)
	if err != nil {
		t.Fatalf("WriteMergedTimeSeries failed: %v", err)
	}

	var parsed map[string]any

	err = json.Unmarshal(buf.Bytes(), &parsed)
	if err != nil {
		t.Fatalf("output is not valid JSON: %v", err)
	}

	if parsed["version"] != analyze.TimeSeriesModelVersion {
		t.Errorf("expected version=%s, got %v", analyze.TimeSeriesModelVersion, parsed["version"])
	}
}
