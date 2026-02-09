package renderer

import (
	"fmt"
	"io"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/common/terminal"
)

// DefaultStaticRenderer implements analyze.StaticRenderer using the renderer
// and terminal packages.
type DefaultStaticRenderer struct{}

// NewDefaultStaticRenderer creates a DefaultStaticRenderer.
func NewDefaultStaticRenderer() *DefaultStaticRenderer {
	return &DefaultStaticRenderer{}
}

// SectionsToJSON converts report sections to a JSON-serializable value.
func (r *DefaultStaticRenderer) SectionsToJSON(sections []analyze.ReportSection) any {
	return SectionsToJSON(sections)
}

// RenderText writes human-readable text output for the given sections.
func (r *DefaultStaticRenderer) RenderText(
	sections []analyze.ReportSection,
	verbose, noColor bool,
	writer io.Writer,
) error {
	config := terminal.NewConfig()
	if noColor {
		config.NoColor = true
	}

	sectionRenderer := NewSectionRenderer(config.Width, verbose, config.NoColor)

	if len(sections) >= MinSectionsForSummary {
		summary := NewExecutiveSummary(sections)

		fmt.Fprintln(writer, sectionRenderer.RenderSummary(summary))
	}

	for _, section := range sections {
		fmt.Fprintln(writer)
		fmt.Fprintln(writer, sectionRenderer.Render(section))
	}

	return nil
}

// RenderCompact writes single-line-per-section compact output.
func (r *DefaultStaticRenderer) RenderCompact(
	sections []analyze.ReportSection,
	noColor bool,
	writer io.Writer,
) error {
	config := terminal.NewConfig()
	if noColor {
		config.NoColor = true
	}

	sectionRenderer := NewSectionRenderer(config.Width, false, config.NoColor)

	for _, section := range sections {
		fmt.Fprintln(writer, sectionRenderer.RenderCompact(section))
	}

	return nil
}
