package renderer

import (
	"encoding/json"
	"fmt"
	"html/template"
	"io"
	"sort"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/plotpage"
)

// UnifiedModel is a type alias for analyze.UnifiedModel so that existing code
// referencing renderer.UnifiedModel continues to compile without changes.
type UnifiedModel = analyze.UnifiedModel

// AnalyzerResult is a type alias for analyze.AnalyzerResult so that existing code
// referencing renderer.AnalyzerResult continues to compile without changes.
type AnalyzerResult = analyze.AnalyzerResult

// UnifiedModelVersion is the schema version for converted run outputs.
const UnifiedModelVersion = analyze.UnifiedModelVersion

// ErrInvalidUnifiedModel indicates malformed canonical conversion data.
var ErrInvalidUnifiedModel = analyze.ErrInvalidUnifiedModel

// NewUnifiedModel delegates to analyze.NewUnifiedModel.
func NewUnifiedModel(results []AnalyzerResult) UnifiedModel {
	return analyze.NewUnifiedModel(results)
}

// ParseUnifiedModelJSON delegates to analyze.ParseUnifiedModelJSON.
func ParseUnifiedModelJSON(data []byte) (UnifiedModel, error) {
	return analyze.ParseUnifiedModelJSON(data)
}

func init() { //nolint:gochecknoinits // registration pattern
	analyze.RegisterPlotRenderer(RenderUnifiedModelPlot)
}

// RenderUnifiedModelPlot renders a canonical model into one combined plot page.
func RenderUnifiedModelPlot(model UnifiedModel, writer io.Writer) error {
	err := model.Validate()
	if err != nil {
		return err
	}

	page := plotpage.NewPage(
		"Converted Analysis Report",
		"Report generated from canonical input model",
	)

	for _, analyzer := range model.Analyzers {
		page.Add(plotpage.Section{
			Title:    analyzer.ID,
			Subtitle: fmt.Sprintf("mode: %s", analyzer.Mode),
			Chart:    reportTable(analyzer.Report),
		})
	}

	err = page.Render(writer)
	if err != nil {
		return fmt.Errorf("render unified plot: %w", err)
	}

	return nil
}

func reportTable(report analyze.Report) *plotpage.Table {
	table := plotpage.NewTable([]string{"Key", "Value"})

	keys := make([]string, 0, len(report))
	for key := range report {
		keys = append(keys, key)
	}

	sort.Strings(keys)

	for _, key := range keys {
		value := report[key]
		renderedValue := fmt.Sprintf("%v", value)

		jsonValue, err := json.Marshal(value)
		if err == nil {
			renderedValue = string(jsonValue)
		}

		table.AddRow(template.HTMLEscapeString(key), template.HTMLEscapeString(renderedValue))
	}

	return table
}
