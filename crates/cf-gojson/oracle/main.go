// Command cfgojson_oracle emits Go-canonical golden fixtures that the Rust
// cf-gojson crate asserts byte-equality against.
//
// It has two modes:
//
//	go run . floats     -> tab-separated "<bits-hex>\t<g-format>" lines on stdout
//	go run . json       -> emits json.Marshal / Encoder fixtures as JSON records
//
// The float mode prints, for each f64 in a fixed adversarial corpus, the exact
// bytes of strconv.FormatFloat(f, 'g', -1, 64). The Rust test reconstructs the
// f64 from the bit pattern and compares its ftoa output.
//
// The json mode prints, for each named case, the exact bytes of json.Marshal
// (compact) and an indented json.Encoder (SetIndent("","  ")) so the Rust
// marshal / marshal_indent can be compared byte-for-byte.
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"math"
	"os"
	"strconv"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: oracle <floats|json>")
		os.Exit(2)
	}
	switch os.Args[1] {
	case "floats":
		emitFloats()
	case "jsonfloats":
		emitJSONFloats()
	case "json":
		emitJSON()
	default:
		fmt.Fprintln(os.Stderr, "unknown mode:", os.Args[1])
		os.Exit(2)
	}
}

// floatCorpus returns the adversarial f64 set: every value where Go's 'g'
// rendering is known to diverge from Rust's Display, plus structural edges.
func floatCorpus() []float64 {
	vals := []float64{
		0.0,
		math.Copysign(0, -1), // -0.0
		1.0, -1.0,
		0.5, -0.5,
		3.14, -3.14,
		2.0, 10.0, 100.0,
		// exponent threshold edges (>=21 -> 'e', <-4 -> 'e')
		1e20, 1e21, 1e22, -1e21,
		1.5e20, 1.5e21,
		1e-4, 1e-5, 1e-6, -1e-5,
		0.0001, 0.00001,
		1e5, 1e6, 1e7, 1e15, 1e16, 1e17,
		123456789012345680000.0,
		1.2345678901234568e+20,
		// govader compound score range: x/sqrt(x*x+15)
		0.4404, -0.4404, 0.6249, -0.5719, 0.34, 0.8316,
		// many small decimals
		0.1, 0.2, 0.3, 0.7, 0.123456789,
		1.0 / 3.0, 2.0 / 3.0,
		// subnormals & extremes
		math.SmallestNonzeroFloat64,
		math.MaxFloat64,
		-math.MaxFloat64,
		2.2250738585072014e-308, // min normal
		// integers that are large
		9007199254740992.0,  // 2^53
		9007199254740993.0,  // 2^53+1 (not representable -> rounds)
		18014398509481984.0, // 2^54
		// percentages * 100 forms
		33.33333333333333, 66.66666666666666,
		12.5, 87.5, 0.125,
		// negatives near edges
		-0.0001, -1e16,
		// values producing trailing-zero stripping
		1.10, 1.20, 100.10, 0.250,
		// stat outputs
		1.4142135623730951, 2.718281828459045, 3.141592653589793,
		1.959963984540054, // z 97.5%
		// zscore sentinel
		100.0, -100.0,
	}
	// Add a deterministic sweep so the corpus is large without RNG seeds.
	for i := -330; i <= 330; i++ {
		vals = append(vals, math.Pow(10, float64(i)/7.0))
	}
	for n := 1; n <= 2000; n++ {
		// rational-ish values that stress shortest round-trip.
		vals = append(vals, float64(n)/7.0)
		vals = append(vals, float64(n)*1.0e-3)
		vals = append(vals, float64(n)*1.0e6)
	}
	return vals
}

func emitFloats() {
	w := os.Stdout
	for _, f := range floatCorpus() {
		bits := math.Float64bits(f)
		s := strconv.FormatFloat(f, 'g', -1, 64)
		fmt.Fprintf(w, "%016x\t%s\n", bits, s)
	}
}

// emitJSONFloats prints, for every corpus value, the exact bytes that Go's
// encoding/json produces for that float as a top-level JSON number. This is the
// authoritative target for cf-gojson's GoValue::Float encoding — it differs
// from strconv 'g' in exponent rendering (json strips the leading exponent
// zero: e-05 -> e-5) and in the 'e'-vs-'f' threshold (json uses
// abs < 1e-6 || abs >= 1e21, computed on the decimal exponent).
func emitJSONFloats() {
	w := os.Stdout
	for _, f := range floatCorpus() {
		// json.Marshal errors on NaN/Inf; the corpus contains neither.
		b, err := json.Marshal(f)
		if err != nil {
			panic(err)
		}
		bits := math.Float64bits(f)
		fmt.Fprintf(w, "%016x\t%s\n", bits, string(b))
	}
}

// jsonCase is one logical value plus its Go-canonical encodings.
type jsonCase struct {
	Name    string          `json:"name"`
	Compact string          `json:"compact"` // json.Marshal(value)
	Indent  string          `json:"indent"`  // Encoder with SetIndent("","  "), trailing \n trimmed
	Value   json.RawMessage `json:"-"`
}

func encodeIndent(v any) string {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetIndent("", "  ")
	if err := enc.Encode(v); err != nil {
		panic(err)
	}
	// Encoder.Encode appends exactly one trailing newline; callers add \n
	// themselves, so the Rust marshal_indent emits none. Trim it for parity.
	out := buf.Bytes()
	if len(out) > 0 && out[len(out)-1] == '\n' {
		out = out[:len(out)-1]
	}
	return string(out)
}

func marshal(v any) string {
	b, err := json.Marshal(v)
	if err != nil {
		panic(err)
	}
	return string(b)
}

func emitJSON() {
	// ordered map type that preserves Go map encoding (keys sorted) — plain
	// map[string]any already sorts keys in encoding/json.
	cases := []struct {
		name string
		v    any
	}{
		{"null", nil},
		{"bool_true", true},
		{"bool_false", false},
		{"int", 42},
		{"int_neg", -7},
		{"int_zero", 0},
		{"float_int_valued", 2.0},
		{"float_frac", 3.14},
		{"float_exp_big", 1e21},
		{"float_exp_small", 1e-5},
		{"string_plain", "hello"},
		{"string_html", "a<b>c&d"},
		{"string_unicode_sep", "x y z"},
		{"string_quotes", "she said \"hi\"\tand\nleft"},
		{"string_backslash", "a\\b/c"},
		{"string_control", "\x00\x01\x1f\x7f"},
		{"string_emoji", "go🚀rust"},
		{"empty_array", []any{}},
		{"empty_object", map[string]any{}},
		{"array_mixed", []any{1, "two", 3.5, true, nil}},
		{"nested_array", []any{[]any{1, 2}, []any{}, []any{"a"}}},
		{"map_unsorted", map[string]any{"zebra": 1, "apple": 2, "Mango": 3, "_x": 4}},
		{"map_byte_order", map[string]any{"Z": 1, "a": 2, "A": 3, "z": 4, "[": 5}},
		{"map_html_keys", map[string]any{"<k>": "<v>", "a&b": "c&d"}},
		{"map_nested", map[string]any{
			"b": map[string]any{"y": 2, "x": 1},
			"a": []any{map[string]any{"k": "v"}},
		}},
		{"map_floats", map[string]any{"pi": 3.141592653589793, "big": 1e21, "tiny": 1e-5, "neg": -0.5}},
		{"deep", map[string]any{
			"score":   0.875,
			"name":    "test<x>",
			"items":   []any{},
			"counts":  map[string]any{"go": 3, "rust": 1},
			"nested":  map[string]any{"deep": map[string]any{"deeper": []any{1, 2, 3}}},
			"unicode": "line1 line2",
		}},
		{"all_escapes", "\b\f\n\r\t\"\\<>&  "},
		{"string_with_slash_only", "https://example.com/path?x=1&y=2"},
	}

	out := make([]jsonCase, 0, len(cases))
	for _, c := range cases {
		out = append(out, jsonCase{
			Name:    c.name,
			Compact: marshal(c.v),
			Indent:  encodeIndent(c.v),
		})
	}
	b, err := json.MarshalIndent(out, "", "  ")
	if err != nil {
		panic(err)
	}
	os.Stdout.Write(b)
	os.Stdout.Write([]byte("\n"))
}
