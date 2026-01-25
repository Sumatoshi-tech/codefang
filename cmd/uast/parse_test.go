package main

import (
	"bytes"
	"encoding/json"
	"os"
	"testing"
)

func TestParseOutputIncludesPositions(t *testing.T) {
	// Create a simple Go source file
	source := `package main

func main() {
    println("hi")
}`

	tmpFile, err := os.CreateTemp("", "test.go")
	if err != nil {
		t.Fatalf("failed to create temp file: %v", err)
	}
	defer os.Remove(tmpFile.Name())
	_, err = tmpFile.WriteString(source)
	if err != nil {
		t.Fatalf("failed to write to temp file: %v", err)
	}
	tmpFile.Close()

	var buf bytes.Buffer
	err = ParseFile(tmpFile.Name(), "go", "", "json", &buf)
	if err != nil {
		t.Fatalf("parseFile failed: %v", err)
	}

	// Parse the output JSON
	var out map[string]interface{}
	dec := json.NewDecoder(&buf)
	if err := dec.Decode(&out); err != nil {
		t.Fatalf("failed to decode output JSON: %v", err)
	}

	// Recursively check for required fields in the output
	required := []string{"start_line", "start_col", "start_offset", "end_line", "end_col", "end_offset"}
	found := false
	var check func(map[string]interface{})
	check = func(m map[string]interface{}) {
		for _, k := range required {
			if _, ok := m[k]; !ok {
				return // If any field is missing, return early
			}
		}
		// All required fields found in this pos object
		found = true
	}
	var walk func(interface{})
	walk = func(n interface{}) {
		if found {
			return
		}
		m, ok := n.(map[string]interface{})
		if !ok {
			return
		}
		if pos, ok := m["pos"].(map[string]interface{}); ok {
			check(pos)
		}
		if children, ok := m["children"].([]interface{}); ok {
			for _, c := range children {
				walk(c)
			}
		}
	}
	walk(out)

	if !found {
		t.Errorf("UAST output does not include all required position fields in 'pos': %v", required)
	}
}
