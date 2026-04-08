// Package composition provides a static file composition analyzer that classifies
// files by type (source, vendor, generated, docs, config, binary, image) using enry.
package composition

import (
	"encoding/json"
	"fmt"
	"io"

	"gopkg.in/yaml.v3"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/reportutil"
	filehistory "github.com/Sumatoshi-tech/codefang/internal/analyzers/file_history"
	"github.com/Sumatoshi-tech/codefang/pkg/pipeline"
)

// Analyzer constants.
const (
	analyzerName        = "composition"
	analyzerFlag        = "composition"
	analyzerID          = "static/composition"
	analyzerDescription = "Classifies files by type (source, vendor, generated, docs, config, binary, image) using enry."

	// keyCategory is the report key for the file category.
	keyCategory = "category"
)

// Analyzer implements analyze.RawFileAnalyzer for file composition analysis.
// It classifies files by type using enry-based detection on raw file content.
type Analyzer struct {
	classifier *filehistory.Classifier
}

// NewAnalyzer creates a new composition Analyzer.
func NewAnalyzer() *Analyzer {
	return &Analyzer{
		classifier: filehistory.NewClassifier(),
	}
}

// Name returns the analyzer name.
func (a *Analyzer) Name() string { return analyzerName }

// Flag returns the CLI flag name.
func (a *Analyzer) Flag() string { return analyzerFlag }

// Descriptor returns the analyzer descriptor.
func (a *Analyzer) Descriptor() analyze.Descriptor {
	return analyze.NewDescriptor(analyze.ModeStatic, analyzerName, analyzerDescription)
}

// ListConfigurationOptions returns available configuration options.
func (a *Analyzer) ListConfigurationOptions() []pipeline.ConfigurationOption {
	return nil
}

// Configure applies configuration facts.
func (a *Analyzer) Configure(_ map[string]any) error {
	return nil
}

// Thresholds returns metric thresholds. Composition is informational, no thresholds.
func (a *Analyzer) Thresholds() analyze.Thresholds {
	return nil
}

// CreateAggregator returns a new composition aggregator.
func (a *Analyzer) CreateAggregator() analyze.ResultAggregator {
	return NewAggregator()
}

// AnalyzeFileContent classifies a file by its path and content using enry.
func (a *Analyzer) AnalyzeFileContent(path string, content []byte) (analyze.Report, error) {
	category := a.classifier.Classify(path, content)

	return analyze.Report{
		keyCategory: string(category),
	}, nil
}

// CreateReportSection creates a ReportSection from aggregated composition data.
func (a *Analyzer) CreateReportSection(report analyze.Report) analyze.ReportSection {
	return NewReportSection(report)
}

// FormatReport writes human-readable text output.
func (a *Analyzer) FormatReport(report analyze.Report, writer io.Writer) error {
	return encodeJSON(report, writer)
}

// FormatReportJSON writes JSON output.
func (a *Analyzer) FormatReportJSON(report analyze.Report, writer io.Writer) error {
	return encodeJSON(report, writer)
}

// FormatReportYAML writes YAML output.
func (a *Analyzer) FormatReportYAML(report analyze.Report, writer io.Writer) error {
	yamlErr := yaml.NewEncoder(writer).Encode(report)
	if yamlErr != nil {
		return fmt.Errorf("encode yaml: %w", yamlErr)
	}

	return nil
}

func encodeJSON(report analyze.Report, writer io.Writer) error {
	encoder := json.NewEncoder(writer)
	encoder.SetIndent("", "  ")

	encodeErr := encoder.Encode(report)
	if encodeErr != nil {
		return fmt.Errorf("encode json: %w", encodeErr)
	}

	return nil
}

// FormatReportPlot writes plot output (same as JSON for composition).
func (a *Analyzer) FormatReportPlot(report analyze.Report, writer io.Writer) error {
	return a.FormatReportJSON(report, writer)
}

// FormatReportBinary writes binary envelope output.
func (a *Analyzer) FormatReportBinary(report analyze.Report, writer io.Writer) error {
	return reportutil.EncodeBinaryEnvelope(report, writer)
}
