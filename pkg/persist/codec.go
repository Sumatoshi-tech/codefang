// Package persist provides codec-based file persistence for arbitrary state types.
package persist

import (
	"encoding/gob"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
)

// File extensions for supported codecs.
const (
	jsonExtension = ".json"
	gobExtension  = ".gob"
)

// Default indentation for pretty-printed JSON.
const defaultIndent = "  "

// Codec defines how state is serialized and deserialized.
type Codec interface {
	// Encode writes the state to the writer.
	Encode(w io.Writer, state any) error
	// Decode reads the state from the reader.
	Decode(r io.Reader, state any) error
	// Extension returns the file extension for this codec (e.g., ".json", ".gob").
	Extension() string
}

// JSONCodec implements Codec using JSON encoding with optional indentation.
type JSONCodec struct {
	// Indent specifies the indentation string. Empty string means compact JSON.
	Indent string
}

// NewJSONCodec creates a JSON codec with pretty-printing (2-space indent).
func NewJSONCodec() *JSONCodec {
	return &JSONCodec{Indent: defaultIndent}
}

// Encode implements Codec.Encode using JSON encoding.
func (c *JSONCodec) Encode(w io.Writer, state any) error {
	encoder := json.NewEncoder(w)
	if c.Indent != "" {
		encoder.SetIndent("", c.Indent)
	}

	err := encoder.Encode(state)
	if err != nil {
		return fmt.Errorf("json encode: %w", err)
	}

	return nil
}

// Decode implements Codec.Decode using JSON decoding.
func (c *JSONCodec) Decode(r io.Reader, state any) error {
	decoder := json.NewDecoder(r)

	err := decoder.Decode(state)
	if err != nil {
		return fmt.Errorf("json decode: %w", err)
	}

	return nil
}

// Extension implements Codec.Extension for JSON files.
func (c *JSONCodec) Extension() string {
	return jsonExtension
}

// GobCodec implements Codec using gob encoding.
type GobCodec struct{}

// NewGobCodec creates a gob codec.
func NewGobCodec() *GobCodec {
	return &GobCodec{}
}

// Encode implements Codec.Encode using gob encoding.
func (c *GobCodec) Encode(w io.Writer, state any) error {
	encoder := gob.NewEncoder(w)

	err := encoder.Encode(state)
	if err != nil {
		return fmt.Errorf("gob encode: %w", err)
	}

	return nil
}

// Decode implements Codec.Decode using gob decoding.
func (c *GobCodec) Decode(r io.Reader, state any) error {
	decoder := gob.NewDecoder(r)

	err := decoder.Decode(state)
	if err != nil {
		return fmt.Errorf("gob decode: %w", err)
	}

	return nil
}

// Extension implements Codec.Extension for gob files.
func (c *GobCodec) Extension() string {
	return gobExtension
}

// SaveState saves the given state to a file in the specified directory.
// The filename is constructed from the basename and the codec's extension.
func SaveState(dir, basename string, codec Codec, state any) error {
	filename := basename + codec.Extension()
	path := filepath.Join(dir, filename)

	file, err := os.Create(path)
	if err != nil {
		return fmt.Errorf("create state file: %w", err)
	}
	defer file.Close()

	err = codec.Encode(file, state)
	if err != nil {
		return fmt.Errorf("encode state: %w", err)
	}

	return nil
}

// LoadState loads state from a file in the specified directory.
// The filename is constructed from the basename and the codec's extension.
// The state parameter must be a pointer to the target struct.
func LoadState(dir, basename string, codec Codec, state any) error {
	filename := basename + codec.Extension()
	path := filepath.Join(dir, filename)

	file, err := os.Open(path)
	if err != nil {
		return fmt.Errorf("open state file: %w", err)
	}
	defer file.Close()

	err = codec.Decode(file, state)
	if err != nil {
		return fmt.Errorf("decode state: %w", err)
	}

	return nil
}
