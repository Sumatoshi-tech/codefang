package persist

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// testState is a struct for round-trip codec testing.
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

	require.NoError(t, codec.Encode(&buf, original))

	var decoded testState

	require.NoError(t, codec.Decode(&buf, &decoded))

	assert.Equal(t, original.Name, decoded.Name)
	assert.Equal(t, original.Count, decoded.Count)
	assert.Equal(t, original.Values, decoded.Values)
}

func TestJSONCodec_Extension(t *testing.T) {
	t.Parallel()

	codec := NewJSONCodec()

	assert.Equal(t, ".json", codec.Extension())
}

func TestJSONCodec_CompactNoIndent(t *testing.T) {
	t.Parallel()

	codec := &JSONCodec{Indent: ""}

	state := testState{Name: "compact", Count: 1}

	var buf bytes.Buffer

	require.NoError(t, codec.Encode(&buf, state))

	// Compact JSON has at most one trailing newline (from json.Encoder).
	output := buf.String()

	assert.LessOrEqual(t, strings.Count(output, "\n"), 1)
}

func TestJSONCodec_PrettyPrint(t *testing.T) {
	t.Parallel()

	codec := NewJSONCodec()

	state := testState{Name: "pretty", Count: 1}

	var buf bytes.Buffer

	require.NoError(t, codec.Encode(&buf, state))

	// Pretty-printed JSON has indentation.
	output := buf.String()

	assert.Contains(t, output, defaultIndent)
}

func TestJSONCodec_DecodeError(t *testing.T) {
	t.Parallel()

	codec := NewJSONCodec()

	var decoded testState

	err := codec.Decode(strings.NewReader("not valid json{{{"), &decoded)

	require.Error(t, err)
	assert.Contains(t, err.Error(), "json decode")
}

func TestJSONCodec_EncodeError(t *testing.T) {
	t.Parallel()

	codec := NewJSONCodec()

	// Channels cannot be JSON-encoded.
	var buf bytes.Buffer

	err := codec.Encode(&buf, make(chan int))

	require.Error(t, err)
	assert.Contains(t, err.Error(), "json encode")
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

	require.NoError(t, codec.Encode(&buf, original))

	var decoded testState

	require.NoError(t, codec.Decode(&buf, &decoded))

	assert.Equal(t, original.Name, decoded.Name)
	assert.Equal(t, original.Count, decoded.Count)
	assert.Equal(t, original.Values, decoded.Values)
}

func TestGobCodec_Extension(t *testing.T) {
	t.Parallel()

	codec := NewGobCodec()

	assert.Equal(t, ".gob", codec.Extension())
}

func TestGobCodec_DecodeError(t *testing.T) {
	t.Parallel()

	codec := NewGobCodec()

	var decoded testState

	err := codec.Decode(strings.NewReader("not gob data"), &decoded)

	require.Error(t, err)
	assert.Contains(t, err.Error(), "gob decode")
}

func TestGobCodec_EncodeError(t *testing.T) {
	t.Parallel()

	codec := NewGobCodec()

	// Functions cannot be gob-encoded.
	var buf bytes.Buffer

	err := codec.Encode(&buf, func() {})

	require.Error(t, err)
	assert.Contains(t, err.Error(), "gob encode")
}

func TestSaveState_JSON(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	codec := NewJSONCodec()

	state := testState{Name: "save-test", Count: 99}

	require.NoError(t, SaveState(dir, "test_state", codec, state))

	path := filepath.Join(dir, "test_state.json")

	_, err := os.Stat(path)

	assert.NoError(t, err)
}

func TestLoadState_JSON(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	codec := NewJSONCodec()

	original := testState{Name: "load-test", Count: 77, Values: map[string]int{"k": 5}}

	require.NoError(t, SaveState(dir, "test_state", codec, original))

	var loaded testState

	require.NoError(t, LoadState(dir, "test_state", codec, &loaded))

	assert.Equal(t, original.Name, loaded.Name)
	assert.Equal(t, original.Count, loaded.Count)
	assert.Equal(t, original.Values, loaded.Values)
}

func TestSaveState_Gob(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	codec := NewGobCodec()

	state := testState{Name: "gob-save", Count: 88}

	require.NoError(t, SaveState(dir, "gob_state", codec, state))

	path := filepath.Join(dir, "gob_state.gob")

	_, err := os.Stat(path)

	assert.NoError(t, err)
}

func TestLoadState_Gob(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	codec := NewGobCodec()

	original := testState{Name: "gob-load", Count: 66}

	require.NoError(t, SaveState(dir, "gob_state", codec, original))

	var loaded testState

	require.NoError(t, LoadState(dir, "gob_state", codec, &loaded))

	assert.Equal(t, original.Name, loaded.Name)
	assert.Equal(t, original.Count, loaded.Count)
}

func TestLoadState_FileNotFound(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	codec := NewJSONCodec()

	var state testState

	err := LoadState(dir, "nonexistent", codec, &state)

	require.Error(t, err)
	assert.Contains(t, err.Error(), "open")
}

func TestSaveState_InvalidDirectory(t *testing.T) {
	t.Parallel()

	codec := NewJSONCodec()
	state := testState{Name: "test"}

	err := SaveState("/nonexistent/path/that/does/not/exist", "test", codec, state)

	require.Error(t, err)
	assert.Contains(t, err.Error(), "create")
}

func TestSaveState_EncodeError(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()
	codec := NewJSONCodec()

	// Channels cannot be JSON-encoded.
	err := SaveState(dir, "bad", codec, make(chan int))

	require.Error(t, err)
	assert.Contains(t, err.Error(), "encode")
}

func TestLoadState_DecodeError(t *testing.T) {
	t.Parallel()

	dir := t.TempDir()

	// Write invalid JSON to a file that LoadState will try to decode.
	path := filepath.Join(dir, "corrupt.json")

	require.NoError(t, os.WriteFile(path, []byte("not json{{{"), 0o600))

	codec := NewJSONCodec()

	var state testState

	err := LoadState(dir, "corrupt", codec, &state)

	require.Error(t, err)
	assert.Contains(t, err.Error(), "decode")
}
