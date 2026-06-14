package compatfuzz

import (
	"testing"
	"time"
)

// FuzzMap — PURE STAGE 2: UAST mapping / structural analysis.
//
// `uast analyze --format json <file>` runs the tree-sitter parse THROUGH the
// UAST mapping layer and reports the mapped tree's composition. This exercises
// the map stage (node-type → UAST-type translation, role assignment) beyond raw
// parse. Differential vs the LIVE Go binary, measured-canonical comparison.
func FuzzMap(f *testing.F) {
	seeds := loadSeeds()
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

		argv := []string{"uast", "analyze", "--format", "json", file}
		res, err := differential(argv, 3, 20*time.Second)
		if err != nil {
			t.Skipf("oracle exec error: %v", err)
		}
		if !res.OK {
			path, _ := distill("map", ext, data, res)
			t.Fatalf("UAST-MAP DIVERGENCE: %s\n  %s\n  distilled -> %s",
				res.Reason, firstDiff(res.GoOut, res.RustOut), path)
		}
	})
}

// FuzzQuery — also exercises the map stage via the query DSL evaluator over the
// mapped tree. A divergence here localizes to map+query rather than parse.
func FuzzQuery(f *testing.F) {
	seeds := loadSeeds("go", "py", "ts", "tsx", "js", "c", "cpp", "rs")
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

		// A fixed, structurally meaningful query (mirrors parity_gate.sh).
		argv := []string{"uast", "query",
			`filter(.roles has "Function")`, "--format", "json", file}
		res, err := differential(argv, 3, 20*time.Second)
		if err != nil {
			t.Skipf("oracle exec error: %v", err)
		}
		if !res.OK {
			path, _ := distill("query", ext, data, res)
			t.Fatalf("QUERY DIVERGENCE: %s\n  %s\n  distilled -> %s",
				res.Reason, firstDiff(res.GoOut, res.RustOut), path)
		}
	})
}
