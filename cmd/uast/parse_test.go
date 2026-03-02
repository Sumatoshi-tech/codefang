package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"testing"

	"github.com/Sumatoshi-tech/codefang/pkg/uast"
)

// ParseFile parses a single source file into UAST format.
func ParseFile(file, lang, output, format string, writer io.Writer) error {
	parser, err := uast.NewParser()
	if err != nil {
		return fmt.Errorf("failed to initialize parser: %w", err)
	}

	return parseFileWithParser(parser, file, lang, output, format, writer)
}

func TestParseOutputIncludesPositions(t *testing.T) {
	t.Parallel()

	// Create a simple Go source file.
	source := `package main

func main() {
    println("hi")
}`

	tmpFile, err := os.CreateTemp(t.TempDir(), "test.go")
	if err != nil {
		t.Fatalf("failed to create temp file: %v", err)
	}
	defer os.Remove(tmpFile.Name())

	_, writeErr := tmpFile.WriteString(source)
	if writeErr != nil {
		t.Fatalf("failed to write to temp file: %v", writeErr)
	}

	tmpFile.Close()

	var buf bytes.Buffer

	parseErr := ParseFile(tmpFile.Name(), "go", "", "json", &buf)
	if parseErr != nil {
		t.Fatalf("parseFile failed: %v", parseErr)
	}

	// Parse the output JSON.
	var out map[string]any

	dec := json.NewDecoder(&buf)

	decodeErr := dec.Decode(&out)
	if decodeErr != nil {
		t.Fatalf("failed to decode output JSON: %v", decodeErr)
	}

	// Recursively check for required fields in the output.
	required := []string{"start_line", "start_col", "start_offset", "end_line", "end_col", "end_offset"}
	found := false

	check := func(posMap map[string]any) {
		for _, fieldKey := range required {
			if _, hasField := posMap[fieldKey]; !hasField {
				return // If any field is missing, return early.
			}
		}

		// All required fields found in this pos object.
		found = true
	}

	var walk func(any)

	walk = func(nodeData any) {
		if found {
			return
		}

		nodeMap, isMap := nodeData.(map[string]any)
		if !isMap {
			return
		}

		if posData, hasPos := nodeMap["pos"].(map[string]any); hasPos {
			check(posData)
		}

		if children, hasChildren := nodeMap["children"].([]any); hasChildren {
			for _, child := range children {
				walk(child)
			}
		}
	}

	walk(out)

	if !found {
		t.Errorf("UAST output does not include all required position fields in 'pos': %v", required)
	}
}
