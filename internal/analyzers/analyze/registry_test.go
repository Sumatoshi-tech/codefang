package analyze_test

import (
	"context"
	"errors"
	"io"
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

type stubStaticAnalyzer struct {
	id   string
	name string
	desc string
}

func (s *stubStaticAnalyzer) Name() string        { return s.name }
func (s *stubStaticAnalyzer) Flag() string        { return s.name }
func (s *stubStaticAnalyzer) Description() string { return s.desc }
func (s *stubStaticAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.Descriptor{
		ID:          s.id,
		Description: s.desc,
		Mode:        analyze.ModeStatic,
	}
}
func (s *stubStaticAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return nil
}
func (s *stubStaticAnalyzer) Configure(_ map[string]any) error { return nil }
func (s *stubStaticAnalyzer) Analyze(_ *node.Node) (analyze.Report, error) {
	return analyze.Report{}, nil
}
func (s *stubStaticAnalyzer) Thresholds() analyze.Thresholds                       { return nil }
func (s *stubStaticAnalyzer) CreateAggregator() analyze.ResultAggregator           { return nil }
func (s *stubStaticAnalyzer) FormatReport(_ analyze.Report, _ io.Writer) error     { return nil }
func (s *stubStaticAnalyzer) FormatReportJSON(_ analyze.Report, _ io.Writer) error { return nil }
func (s *stubStaticAnalyzer) FormatReportYAML(_ analyze.Report, _ io.Writer) error { return nil }
func (s *stubStaticAnalyzer) FormatReportPlot(_ analyze.Report, _ io.Writer) error { return nil }
func (s *stubStaticAnalyzer) FormatReportBinary(_ analyze.Report, _ io.Writer) error {
	return nil
}

type stubHistoryAnalyzer struct {
	id   string
	name string
	desc string
}

func (s *stubHistoryAnalyzer) Name() string        { return s.name }
func (s *stubHistoryAnalyzer) Flag() string        { return s.name }
func (s *stubHistoryAnalyzer) Description() string { return s.desc }
func (s *stubHistoryAnalyzer) Descriptor() analyze.Descriptor {
	return analyze.Descriptor{
		ID:          s.id,
		Description: s.desc,
		Mode:        analyze.ModeHistory,
	}
}
func (s *stubHistoryAnalyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return nil
}
func (s *stubHistoryAnalyzer) Configure(_ map[string]any) error      { return nil }
func (s *stubHistoryAnalyzer) Initialize(_ *gitlib.Repository) error { return nil }
func (s *stubHistoryAnalyzer) Consume(_ context.Context, _ *analyze.Context) (analyze.TC, error) {
	return analyze.TC{}, nil
}
func (s *stubHistoryAnalyzer) Fork(_ int) []analyze.HistoryAnalyzer                    { return nil }
func (s *stubHistoryAnalyzer) Merge(_ []analyze.HistoryAnalyzer)                       {}
func (s *stubHistoryAnalyzer) Serialize(_ analyze.Report, _ string, _ io.Writer) error { return nil }
func (s *stubHistoryAnalyzer) WorkingStateSize() int64                                 { return 0 }
func (s *stubHistoryAnalyzer) AvgTCSize() int64                                        { return 0 }
func (s *stubHistoryAnalyzer) NewAggregator(_ analyze.AggregatorOptions) analyze.Aggregator {
	return nil
}
func (s *stubHistoryAnalyzer) SerializeTICKs(_ []analyze.TICK, _ string, _ io.Writer) error {
	return analyze.ErrNotImplemented
}

func (s *stubHistoryAnalyzer) ReportFromTICKs(_ context.Context, _ []analyze.TICK) (analyze.Report, error) {
	return nil, analyze.ErrNotImplemented
}

func TestRegistry_AllStableOrder(t *testing.T) {
	t.Parallel()

	registry, err := analyze.NewRegistry(defaultStaticForRegistryTest(), nil, defaultHistoryForRegistryTest())
	if err != nil {
		t.Fatalf("unexpected registry creation error: %v", err)
	}

	descriptors := registry.All()
	if len(descriptors) == 0 {
		t.Fatal("expected non-empty descriptor list")
	}

	if descriptors[0].ID != complexityID {
		t.Fatalf("unexpected first descriptor: %s", descriptors[0].ID)
	}

	if descriptors[len(descriptors)-1].ID != "history/typos" {
		t.Fatalf("unexpected last descriptor: %s", descriptors[len(descriptors)-1].ID)
	}
}

func TestRegistry_IDsByMode(t *testing.T) {
	t.Parallel()

	registry, err := analyze.NewRegistry(defaultStaticForRegistryTest(), nil, defaultHistoryForRegistryTest())
	if err != nil {
		t.Fatalf("unexpected registry creation error: %v", err)
	}

	staticIDs := registry.IDsByMode(analyze.ModeStatic)
	historyIDs := registry.IDsByMode(analyze.ModeHistory)

	if len(staticIDs) != 5 {
		t.Fatalf("expected 5 static analyzers, got %d", len(staticIDs))
	}

	if len(historyIDs) != 8 {
		t.Fatalf("expected 8 history analyzers, got %d", len(historyIDs))
	}
}

func TestRegistry_Split(t *testing.T) {
	t.Parallel()

	registry, err := analyze.NewRegistry(defaultStaticForRegistryTest(), nil, defaultHistoryForRegistryTest())
	if err != nil {
		t.Fatalf("unexpected registry creation error: %v", err)
	}

	staticIDs, historyIDs, err := registry.Split([]string{"static/comments", "history/devs", "static/imports"})
	if err != nil {
		t.Fatalf("unexpected split error: %v", err)
	}

	if len(staticIDs) != 2 {
		t.Fatalf("expected 2 static analyzers, got %d", len(staticIDs))
	}

	if len(historyIDs) != 1 {
		t.Fatalf("expected 1 history analyzer, got %d", len(historyIDs))
	}
}

func TestRegistry_SplitUnknown(t *testing.T) {
	t.Parallel()

	registry, err := analyze.NewRegistry(defaultStaticForRegistryTest(), nil, defaultHistoryForRegistryTest())
	if err != nil {
		t.Fatalf("unexpected registry creation error: %v", err)
	}

	_, _, splitErr := registry.Split([]string{"unknown/id"})
	if !errors.Is(splitErr, analyze.ErrUnknownAnalyzerID) {
		t.Fatalf("expected ErrUnknownAnalyzerID, got %v", splitErr)
	}
}

// complexityID is a stable fixture for the first registered static analyzer.
// Used by ExpandPatterns tests — FRD: specs/frds/FRD-20260306-append-unique-ids-removal.md.
const complexityID = "static/complexity"

func TestRegistry_ExpandPatterns_ExactMatch(t *testing.T) {
	t.Parallel()

	registry := newTestRegistry(t)

	ids, err := registry.ExpandPatterns([]string{complexityID})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(ids) != 1 || ids[0] != complexityID {
		t.Fatalf("expected [static/complexity], got %v", ids)
	}
}

func TestRegistry_ExpandPatterns_GlobMatch(t *testing.T) {
	t.Parallel()

	registry := newTestRegistry(t)

	ids, err := registry.ExpandPatterns([]string{"static/*"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(ids) != 5 {
		t.Fatalf("expected 5 static ids, got %d: %v", len(ids), ids)
	}

	if ids[0] != complexityID {
		t.Fatalf("expected stable order: first should be static/complexity, got %s", ids[0])
	}
}

func TestRegistry_ExpandPatterns_Wildcard(t *testing.T) {
	t.Parallel()

	registry := newTestRegistry(t)

	ids, err := registry.ExpandPatterns([]string{"*"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(ids) != 13 {
		t.Fatalf("expected 13 total ids, got %d: %v", len(ids), ids)
	}
}

func TestRegistry_ExpandPatterns_DedupAcrossPatterns(t *testing.T) {
	t.Parallel()

	registry := newTestRegistry(t)

	// complexityID appears explicitly and again via "static/*".
	// The first occurrence wins; no duplicates.
	ids, err := registry.ExpandPatterns([]string{complexityID, "static/*"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(ids) != 5 {
		t.Fatalf("expected 5 ids (no duplicates), got %d: %v", len(ids), ids)
	}

	if ids[0] != complexityID {
		t.Fatalf("expected static/complexity first, got %s", ids[0])
	}
}

func TestRegistry_ExpandPatterns_UnknownExact(t *testing.T) {
	t.Parallel()

	registry := newTestRegistry(t)

	_, err := registry.ExpandPatterns([]string{"unknown/id"})
	if !errors.Is(err, analyze.ErrUnknownAnalyzerID) {
		t.Fatalf("expected ErrUnknownAnalyzerID, got %v", err)
	}
}

func TestRegistry_ExpandPatterns_EmptyPattern(t *testing.T) {
	t.Parallel()

	registry := newTestRegistry(t)

	_, err := registry.ExpandPatterns([]string{""})
	if !errors.Is(err, analyze.ErrUnknownAnalyzerID) {
		t.Fatalf("expected ErrUnknownAnalyzerID for empty pattern, got %v", err)
	}
}

func TestRegistry_ExpandPatterns_GlobNoMatch(t *testing.T) {
	t.Parallel()

	registry := newTestRegistry(t)

	_, err := registry.ExpandPatterns([]string{"nosuchprefix/*"})
	if !errors.Is(err, analyze.ErrUnknownAnalyzerID) {
		t.Fatalf("expected ErrUnknownAnalyzerID for no-match glob, got %v", err)
	}
}

func TestRegistry_SelectedIDs_EmptyPatternsReturnsAll(t *testing.T) {
	t.Parallel()

	registry := newTestRegistry(t)

	ids, err := registry.SelectedIDs(nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(ids) != 13 {
		t.Fatalf("expected 13 ids (all), got %d", len(ids))
	}
}

func TestRegistry_SelectedIDs_WithPatterns(t *testing.T) {
	t.Parallel()

	registry := newTestRegistry(t)

	ids, err := registry.SelectedIDs([]string{"history/burndown", "history/couples"})
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	if len(ids) != 2 {
		t.Fatalf("expected 2 ids, got %d: %v", len(ids), ids)
	}
}

func newTestRegistry(t *testing.T) *analyze.Registry {
	t.Helper()

	registry, err := analyze.NewRegistry(defaultStaticForRegistryTest(), nil, defaultHistoryForRegistryTest())
	if err != nil {
		t.Fatalf("failed to create registry: %v", err)
	}

	return registry
}

func defaultStaticForRegistryTest() []analyze.StaticAnalyzer {
	return []analyze.StaticAnalyzer{
		&stubStaticAnalyzer{id: complexityID, name: "complexity", desc: "complexity"},
		&stubStaticAnalyzer{id: "static/comments", name: "comments", desc: "comments"},
		&stubStaticAnalyzer{id: "static/halstead", name: "halstead", desc: "halstead"},
		&stubStaticAnalyzer{id: "static/cohesion", name: "cohesion", desc: "cohesion"},
		&stubStaticAnalyzer{id: "static/imports", name: "imports", desc: "imports"},
	}
}

func defaultHistoryForRegistryTest() []analyze.HistoryAnalyzer {
	return []analyze.HistoryAnalyzer{
		&stubHistoryAnalyzer{id: "history/burndown", name: "Burndown", desc: "burndown"},
		&stubHistoryAnalyzer{id: "history/couples", name: "Couples", desc: "couples"},
		&stubHistoryAnalyzer{id: "history/devs", name: "Devs", desc: "devs"},
		&stubHistoryAnalyzer{id: "history/file-history", name: "FileHistoryAnalysis", desc: "file history"},
		&stubHistoryAnalyzer{id: "history/imports", name: "ImportsPerDeveloper", desc: "imports history"},
		&stubHistoryAnalyzer{id: "history/sentiment", name: "Sentiment", desc: "sentiment"},
		&stubHistoryAnalyzer{id: "history/shotness", name: "Shotness", desc: "shotness"},
		&stubHistoryAnalyzer{id: "history/typos", name: "TyposDataset", desc: "typos"},
	}
}
