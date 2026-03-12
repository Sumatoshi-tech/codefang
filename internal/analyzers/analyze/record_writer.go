package analyze

import "fmt"

// WriteSliceKind writes each element of a typed slice as a separate record
// under the given kind. Returns nil for empty or nil slices.
func WriteSliceKind[T any](w ReportWriter, kind string, records []T) error {
	for i := range records {
		writeErr := w.Write(kind, records[i])
		if writeErr != nil {
			return fmt.Errorf("write %s: %w", kind, writeErr)
		}
	}

	return nil
}
