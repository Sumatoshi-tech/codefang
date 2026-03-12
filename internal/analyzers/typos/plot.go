package typos

import (
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
)

const (
	topFilesLimit = 20
)

// RegisterPlotSections registers the typos plot section renderer with the analyze package.
func RegisterPlotSections() {
	analyze.RegisterStorePlotSections("typos", GenerateStoreSections)
}
