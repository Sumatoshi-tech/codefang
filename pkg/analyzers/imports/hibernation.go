package imports

import (
	"fmt"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

// Hibernate compresses the analyzer's state to reduce memory usage.
// Releases the UAST parser which is memory-heavy but can be recreated.
func (h *HistoryAnalyzer) Hibernate() error {
	// Release parser to free memory - it will be recreated in Boot
	h.parser = nil

	return nil
}

// Boot restores the analyzer from hibernated state.
// Recreates the UAST parser for the next chunk.
func (h *HistoryAnalyzer) Boot() error {
	// Recreate parser if needed
	if h.parser == nil {
		var err error

		h.parser, err = uast.NewParser()
		if err != nil {
			return fmt.Errorf("failed to recreate UAST parser: %w", err)
		}
	}

	return nil
}
