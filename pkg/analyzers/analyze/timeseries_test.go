package analyze_test

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
)

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

func TestBuildMergedTimeSeriesDirect_EmptyData(t *testing.T) {
	t.Parallel()

	ts := analyze.BuildMergedTimeSeriesDirect(nil, nil, 0)

	if ts.Version != analyze.TimeSeriesModelVersion {
		t.Errorf("expected version=%s, got %s", analyze.TimeSeriesModelVersion, ts.Version)
	}

	if len(ts.Commits) != 0 {
		t.Errorf("expected 0 commits, got %d", len(ts.Commits))
	}

	if len(ts.Analyzers) != 0 {
		t.Errorf("expected 0 analyzers, got %d", len(ts.Analyzers))
	}

	if ts.TickSizeHours != 24 {
		t.Errorf("expected default tick_size_hours=24, got %f", ts.TickSizeHours)
	}
}

func TestBuildMergedTimeSeriesDirect_SingleAnalyzer(t *testing.T) {
	t.Parallel()

	active := []analyze.AnalyzerData{
		{
			Flag: "quality",
			Data: map[string]any{
				"aaa111": map[string]any{"score": 10},
				"bbb222": map[string]any{"score": 20},
			},
		},
	}

	meta := []analyze.CommitMeta{
		{Hash: "aaa111", Timestamp: "2024-01-01T00:00:00Z", Author: "alice", Tick: 0},
		{Hash: "bbb222", Timestamp: "2024-01-02T00:00:00Z", Author: "bob", Tick: 1},
	}

	ts := analyze.BuildMergedTimeSeriesDirect(active, meta, 0)

	if len(ts.Analyzers) != 1 {
		t.Fatalf("expected 1 analyzer, got %d", len(ts.Analyzers))
	}

	if ts.Analyzers[0] != "quality" {
		t.Errorf("expected analyzer=quality, got %s", ts.Analyzers[0])
	}

	if len(ts.Commits) != 2 {
		t.Fatalf("expected 2 commits, got %d", len(ts.Commits))
	}

	if ts.Commits[0].Hash != "aaa111" {
		t.Errorf("expected first commit=aaa111, got %s", ts.Commits[0].Hash)
	}

	if ts.Commits[1].Hash != "bbb222" {
		t.Errorf("expected second commit=bbb222, got %s", ts.Commits[1].Hash)
	}

	if ts.Commits[0].Author != "alice" {
		t.Errorf("expected author=alice, got %s", ts.Commits[0].Author)
	}

	if ts.Commits[0].Tick != 0 {
		t.Errorf("expected tick=0, got %d", ts.Commits[0].Tick)
	}

	if ts.Commits[1].Tick != 1 {
		t.Errorf("expected tick=1, got %d", ts.Commits[1].Tick)
	}
}

func TestBuildMergedTimeSeriesDirect_MultiAnalyzer(t *testing.T) {
	t.Parallel()

	active := []analyze.AnalyzerData{
		{
			Flag: "analyzer-a",
			Data: map[string]any{
				"commit1": map[string]any{"metric_a": 1},
			},
		},
		{
			Flag: "analyzer-b",
			Data: map[string]any{
				"commit1": map[string]any{"metric_b": 2},
			},
		},
	}

	meta := []analyze.CommitMeta{
		{Hash: "commit1", Timestamp: "2024-01-01T00:00:00Z", Author: "alice", Tick: 0},
	}

	ts := analyze.BuildMergedTimeSeriesDirect(active, meta, 0)

	if len(ts.Commits) != 1 {
		t.Fatalf("expected 1 commit, got %d", len(ts.Commits))
	}

	entry := ts.Commits[0]

	if len(entry.Analyzers) != 2 {
		t.Fatalf("expected 2 analyzer entries, got %d", len(entry.Analyzers))
	}

	if _, ok := entry.Analyzers["analyzer-a"]; !ok {
		t.Error("expected analyzer-a in analyzers")
	}

	if _, ok := entry.Analyzers["analyzer-b"]; !ok {
		t.Error("expected analyzer-b in analyzers")
	}
}

func TestBuildMergedTimeSeriesDirect_CustomTickSize(t *testing.T) {
	t.Parallel()

	ts := analyze.BuildMergedTimeSeriesDirect(nil, nil, 12)

	if ts.TickSizeHours != 12 {
		t.Errorf("expected tick_size_hours=12, got %f", ts.TickSizeHours)
	}
}

func TestBuildMergedTimeSeriesDirect_CommitOrderFollowsMeta(t *testing.T) {
	t.Parallel()

	active := []analyze.AnalyzerData{
		{
			Flag: "test",
			Data: map[string]any{
				"commit3": "data3",
				"commit1": "data1",
				"commit2": "data2",
			},
		},
	}

	// Meta defines the canonical order.
	meta := []analyze.CommitMeta{
		{Hash: "commit1", Tick: 0},
		{Hash: "commit2", Tick: 1},
		{Hash: "commit3", Tick: 2},
	}

	ts := analyze.BuildMergedTimeSeriesDirect(active, meta, 0)

	if len(ts.Commits) != 3 {
		t.Fatalf("expected 3 commits, got %d", len(ts.Commits))
	}

	if ts.Commits[0].Hash != "commit1" {
		t.Errorf("expected first=commit1, got %s", ts.Commits[0].Hash)
	}

	if ts.Commits[1].Hash != "commit2" {
		t.Errorf("expected second=commit2, got %s", ts.Commits[1].Hash)
	}

	if ts.Commits[2].Hash != "commit3" {
		t.Errorf("expected third=commit3, got %s", ts.Commits[2].Hash)
	}
}

func TestBuildMergedTimeSeriesDirect_SkipsCommitsNotInMeta(t *testing.T) {
	t.Parallel()

	active := []analyze.AnalyzerData{
		{
			Flag: "test",
			Data: map[string]any{
				"known":   "data",
				"unknown": "data",
			},
		},
	}

	meta := []analyze.CommitMeta{
		{Hash: "known", Tick: 0},
	}

	ts := analyze.BuildMergedTimeSeriesDirect(active, meta, 0)

	// Only "known" should appear since "unknown" is not in meta.
	if len(ts.Commits) != 1 {
		t.Fatalf("expected 1 commit, got %d", len(ts.Commits))
	}

	if ts.Commits[0].Hash != "known" {
		t.Errorf("expected hash=known, got %s", ts.Commits[0].Hash)
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
