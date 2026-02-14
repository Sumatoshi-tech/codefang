package checkpoint

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

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

func TestSaveState_JSON(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	codec := NewJSONCodec()

	state := testState{Name: "save-test", Count: 99}

	err := SaveState(dir, "test_state", codec, state)
	if err != nil {
		t.Fatalf("SaveState failed: %v", err)
	}

	// Verify file was created.
	path := filepath.Join(dir, "test_state.json")

	_, statErr := os.Stat(path)
	if os.IsNotExist(statErr) {
		t.Errorf("Checkpoint file not created at %s", path)
	}
}

func TestLoadState_JSON(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	codec := NewJSONCodec()

	original := testState{Name: "load-test", Count: 77, Values: map[string]int{"k": 5}}

	err := SaveState(dir, "test_state", codec, original)
	if err != nil {
		t.Fatalf("SaveState failed: %v", err)
	}

	var loaded testState

	err = LoadState(dir, "test_state", codec, &loaded)
	if err != nil {
		t.Fatalf("LoadState failed: %v", err)
	}

	if loaded.Name != original.Name {
		t.Errorf("Name = %q, want %q", loaded.Name, original.Name)
	}

	if loaded.Count != original.Count {
		t.Errorf("Count = %d, want %d", loaded.Count, original.Count)
	}
}

func TestSaveState_Gob(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	codec := NewGobCodec()

	state := testState{Name: "gob-save", Count: 88}

	err := SaveState(dir, "gob_state", codec, state)
	if err != nil {
		t.Fatalf("SaveState failed: %v", err)
	}

	path := filepath.Join(dir, "gob_state.gob")

	_, statErr := os.Stat(path)
	if os.IsNotExist(statErr) {
		t.Errorf("Checkpoint file not created at %s", path)
	}
}

func TestLoadState_Gob(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	codec := NewGobCodec()

	original := testState{Name: "gob-load", Count: 66}

	err := SaveState(dir, "gob_state", codec, original)
	if err != nil {
		t.Fatalf("SaveState failed: %v", err)
	}

	var loaded testState

	err = LoadState(dir, "gob_state", codec, &loaded)
	if err != nil {
		t.Fatalf("LoadState failed: %v", err)
	}

	if loaded.Name != original.Name {
		t.Errorf("Name = %q, want %q", loaded.Name, original.Name)
	}
}

func TestLoadState_FileNotFound(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	codec := NewJSONCodec()

	var state testState

	err := LoadState(dir, "nonexistent", codec, &state)
	if err == nil {
		t.Error("LoadState should fail for nonexistent file")
	}
}

func TestSaveState_InvalidDirectory(t *testing.T) {
	t.Parallel()

	codec := NewJSONCodec()
	state := testState{Name: "test"}

	err := SaveState("/nonexistent/path/that/does/not/exist", "test", codec, state)
	if err == nil {
		t.Error("SaveState should fail for invalid directory")
	}
}
