// Package textutil provides byte-level text utilities: binary detection,
// line counting, JSON encoding, and byte-slice reader adapters.
package textutil

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
)

// jsonIndent is the indentation string used for pretty-printed JSON output.
const jsonIndent = "  "

// WriteJSON encodes v as JSON to w.
// If pretty is true, output is indented with two spaces.
func WriteJSON(w io.Writer, v any, pretty bool) error {
	enc := json.NewEncoder(w)
	if pretty {
		enc.SetIndent("", jsonIndent)
	}

	err := enc.Encode(v)
	if err != nil {
		return fmt.Errorf("encode JSON: %w", err)
	}

	return nil
}

// BinarySniffLength is the maximum number of bytes scanned for null-byte
// detection. Matches the heuristic used by Git and most editors.
const BinarySniffLength = 8000

// IsBinary returns true if data contains a null byte within the first
// BinarySniffLength bytes. Empty data is not binary.
func IsBinary(data []byte) bool {
	if len(data) == 0 {
		return false
	}

	sniff := data
	if len(sniff) > BinarySniffLength {
		sniff = sniff[:BinarySniffLength]
	}

	return bytes.IndexByte(sniff, 0) >= 0
}

// CountLines returns the number of newline-delimited lines in data.
// A non-empty buffer without a trailing newline counts the last partial line.
// Returns 0 for empty data.
func CountLines(data []byte) int {
	if len(data) == 0 {
		return 0
	}

	lines := bytes.Count(data, []byte{'\n'})

	if data[len(data)-1] != '\n' {
		lines++
	}

	return lines
}
