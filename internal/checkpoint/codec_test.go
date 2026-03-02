package checkpoint

import (
	"bytes"
	"strings"
	"testing"
)

// NewCompactJSONCodec creates a JSON codec without indentation.
func NewCompactJSONCodec() *JSONCodec {
	return &JSONCodec{Indent: ""}
}

// testState is a simple struct for testing codecs.
type testState struct {
	Name   string         `json:"name"`
	Count  int            `json:"count"`
	Values map[string]int `json:"values"`
}

func TestJSONCodec_RoundTrip(t *testing.T) {
	t.Parallel()

	codec := NewJSONCodec()

	original := testState{
		Name:   "test",
		Count:  42,
		Values: map[string]int{"a": 1, "b": 2},
	}

	var buf bytes.Buffer

	err := codec.Encode(&buf, original)
	if err != nil {
		t.Fatalf("Encode failed: %v", err)
	}

	var decoded testState

	err = codec.Decode(&buf, &decoded)
	if err != nil {
		t.Fatalf("Decode failed: %v", err)
	}

	if decoded.Name != original.Name {
		t.Errorf("Name = %q, want %q", decoded.Name, original.Name)
	}

	if decoded.Count != original.Count {
		t.Errorf("Count = %d, want %d", decoded.Count, original.Count)
	}

	if len(decoded.Values) != len(original.Values) {
		t.Errorf("Values length = %d, want %d", len(decoded.Values), len(original.Values))
	}
}

func TestJSONCodec_Extension(t *testing.T) {
	t.Parallel()

	codec := NewJSONCodec()
	if ext := codec.Extension(); ext != ".json" {
		t.Errorf("Extension() = %q, want %q", ext, ".json")
	}
}

func TestCompactJSONCodec_NoIndent(t *testing.T) {
	t.Parallel()

	codec := NewCompactJSONCodec()

	state := testState{Name: "compact", Count: 1}

	var buf bytes.Buffer

	err := codec.Encode(&buf, state)
	if err != nil {
		t.Fatalf("Encode failed: %v", err)
	}

	// Compact JSON should not contain newlines within the object.
	output := buf.String()
	if strings.Count(output, "\n") > 1 {
		t.Errorf("Compact JSON has too many newlines: %q", output)
	}
}

func TestGobCodec_RoundTrip(t *testing.T) {
	t.Parallel()

	codec := NewGobCodec()

	original := testState{
		Name:   "gob-test",
		Count:  123,
		Values: map[string]int{"x": 10, "y": 20},
	}

	var buf bytes.Buffer

	err := codec.Encode(&buf, original)
	if err != nil {
		t.Fatalf("Encode failed: %v", err)
	}

	var decoded testState

	err = codec.Decode(&buf, &decoded)
	if err != nil {
		t.Fatalf("Decode failed: %v", err)
	}

	if decoded.Name != original.Name {
		t.Errorf("Name = %q, want %q", decoded.Name, original.Name)
	}

	if decoded.Count != original.Count {
		t.Errorf("Count = %d, want %d", decoded.Count, original.Count)
	}
}

func TestGobCodec_Extension(t *testing.T) {
	t.Parallel()

	codec := NewGobCodec()
	if ext := codec.Extension(); ext != ".gob" {
		t.Errorf("Extension() = %q, want %q", ext, ".gob")
	}
}
