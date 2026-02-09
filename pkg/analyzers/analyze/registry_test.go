package analyze_test

import (
	"errors"
	"io"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
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
func (s *stubHistoryAnalyzer) Configure(_ map[string]any) error                        { return nil }
func (s *stubHistoryAnalyzer) Initialize(_ *gitlib.Repository) error                   { return nil }
func (s *stubHistoryAnalyzer) Consume(_ *analyze.Context) error                        { return nil }
func (s *stubHistoryAnalyzer) Finalize() (analyze.Report, error)                       { return analyze.Report{}, nil }
func (s *stubHistoryAnalyzer) Fork(_ int) []analyze.HistoryAnalyzer                    { return nil }
func (s *stubHistoryAnalyzer) Merge(_ []analyze.HistoryAnalyzer)                       {}
func (s *stubHistoryAnalyzer) Serialize(_ analyze.Report, _ string, _ io.Writer) error { return nil }

func TestRegistry_AllStableOrder(t *testing.T) {
	t.Parallel()

	registry, err := analyze.NewRegistry(defaultStaticForRegistryTest(), defaultHistoryForRegistryTest())
	if err != nil {
		t.Fatalf("unexpected registry creation error: %v", err)
	}

	descriptors := registry.All()
	if len(descriptors) == 0 {
		t.Fatal("expected non-empty descriptor list")
	}

	if descriptors[0].ID != "static/complexity" {
		t.Fatalf("unexpected first descriptor: %s", descriptors[0].ID)
	}

	if descriptors[len(descriptors)-1].ID != "history/typos" {
		t.Fatalf("unexpected last descriptor: %s", descriptors[len(descriptors)-1].ID)
	}
}

func TestRegistry_IDsByMode(t *testing.T) {
	t.Parallel()

	registry, err := analyze.NewRegistry(defaultStaticForRegistryTest(), defaultHistoryForRegistryTest())
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

	registry, err := analyze.NewRegistry(defaultStaticForRegistryTest(), defaultHistoryForRegistryTest())
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

	registry, err := analyze.NewRegistry(defaultStaticForRegistryTest(), defaultHistoryForRegistryTest())
	if err != nil {
		t.Fatalf("unexpected registry creation error: %v", err)
	}

	_, _, splitErr := registry.Split([]string{"unknown/id"})
	if !errors.Is(splitErr, analyze.ErrUnknownAnalyzerID) {
		t.Fatalf("expected ErrUnknownAnalyzerID, got %v", splitErr)
	}
}

func defaultStaticForRegistryTest() []analyze.StaticAnalyzer {
	return []analyze.StaticAnalyzer{
		&stubStaticAnalyzer{id: "static/complexity", name: "complexity", desc: "complexity"},
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
