package compatfuzz

import (
	"testing"
	"time"
)

// FuzzParse — PURE STAGE 1: tree-sitter parse.
//
// Feeds the same source bytes (with a language-carrying extension) to
//   uast parse --format json <file>
// on the LIVE Go binary and on the Rust binary, and FAILS on divergence.
// Parse output is fully deterministic in Go (verified: ids are content hashes,
// no timestamps), so the measured Go variance is expected to be EMPTY and the
// comparison is effectively byte-exact — but it is still MEASURED, not assumed.
func FuzzParse(f *testing.F) {
	seeds := loadSeeds() // all languages
	for _, s := range seeds {
		f.Add(s.Ext, s.Data)
	}
	if len(seeds) == 0 {
		f.Skip("no seed corpus found")
	}
	f.Fuzz(func(t *testing.T, ext string, data []byte) {
		ext = sanitizeExt(ext)
		dir, file := writeTemp(t, ext, data)
		defer removeAll(dir)

		argv := []string{"uast", "parse", "--format", "json", file}
		res, err := differential(argv, 3, 20*time.Second)
		if err != nil {
			t.Skipf("oracle exec error (input not gradeable): %v", err)
		}
		if !res.OK {
			path, _ := distill("parse", ext, data, res)
			t.Fatalf("PARSE DIVERGENCE: %s\n  %s\n  distilled -> %s",
				res.Reason, firstDiff(res.GoOut, res.RustOut), path)
		}
	})
}

// FuzzParseGo restricts mutation to Go source — the ONE language whose
// tree-sitter grammar is currently wired in the Rust port. This lets the
// coverage-guided mutator dig DEEP into parse parity for a supported language
// instead of bouncing off the "grammar not wired" gap that FuzzParse exposes for
// every other language. (Once more grammars are wired in Rust, drop this and
// rely on FuzzParse across all languages.)
func FuzzParseGo(f *testing.F) {
	seeds := loadSeeds("go")
	for _, s := range seeds {
		f.Add(s.Data)
	}
	if len(seeds) == 0 {
		f.Skip("no Go seed corpus found")
	}
	f.Fuzz(func(t *testing.T, data []byte) {
		dir, file := writeTemp(t, "go", data)
		defer removeAll(dir)
		argv := []string{"uast", "parse", "--format", "json", file}
		res, err := differential(argv, 3, 20*time.Second)
		if err != nil {
			t.Skipf("oracle exec error: %v", err)
		}
		if !res.OK {
			path, _ := distill("parse_go", "go", data, res)
			t.Fatalf("GO-PARSE DIVERGENCE: %s\n  %s\n  distilled -> %s",
				res.Reason, firstDiff(res.GoOut, res.RustOut), path)
		}
	})
}
