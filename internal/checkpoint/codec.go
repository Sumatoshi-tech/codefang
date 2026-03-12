package checkpoint

import "github.com/Sumatoshi-tech/codefang/pkg/persist"

// Codec is an alias for [persist.Codec].
type Codec = persist.Codec

// JSONCodec is an alias for [persist.JSONCodec].
type JSONCodec = persist.JSONCodec

// GobCodec is an alias for [persist.GobCodec].
type GobCodec = persist.GobCodec

// NewJSONCodec creates a JSON codec with pretty-printing.
func NewJSONCodec() *JSONCodec {
	return persist.NewJSONCodec()
}

// NewGobCodec creates a gob codec.
func NewGobCodec() *GobCodec {
	return persist.NewGobCodec()
}
