package compatfuzz

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// Seed corpora are STRUCTURE-AWARE: we seed from REAL source files (the
// content-addressed corpus under ../corpus/files plus any prior fuzzfinds), not
// random bytes — random bytes bounce off the tree-sitter parser and never reach
// the analyzer/serializer logic (SPEC §3, Directed Grammar-Based Test Gen).
//
// The Go fuzzer then mutates these valid inputs; because mutations start from
// valid source they stay close enough to the grammar to exercise deep stages.

func corpusFilesDir() string {
	return filepath.Join(repoRoot(), "rust", "tests", "compat", "corpus", "files")
}

// seedFile holds a seed input with the file extension that drives language
// detection in both binaries.
type seedFile struct {
	Ext  string
	Data []byte
}

// loadSeeds returns seed files from the corpus whose extension is in want.
// If want is empty, every corpus file is returned.
func loadSeeds(want ...string) []seedFile {
	wantSet := map[string]bool{}
	for _, e := range want {
		wantSet[strings.TrimPrefix(e, ".")] = true
	}
	var out []seedFile
	addFrom := func(dir string) {
		entries, err := os.ReadDir(dir)
		if err != nil {
			return
		}
		names := make([]string, 0, len(entries))
		for _, e := range entries {
			if !e.IsDir() {
				names = append(names, e.Name())
			}
		}
		sort.Strings(names)
		for _, n := range names {
			// Skip distill sidecars so evidence artifacts are never re-seeded
			// as inputs (corpus hygiene): *.evidence.json, *.go_run_N, *.rust_out.
			if strings.HasSuffix(n, ".evidence.json") ||
				strings.HasSuffix(n, ".rust_out") ||
				strings.Contains(n, ".go_run_") ||
				strings.HasPrefix(n, ".class_") {
				continue
			}
			ext := strings.TrimPrefix(filepath.Ext(n), ".")
			if ext == "" {
				continue
			}
			if len(wantSet) > 0 && !wantSet[ext] {
				continue
			}
			b, err := os.ReadFile(filepath.Join(dir, n))
			if err != nil {
				continue
			}
			out = append(out, seedFile{Ext: ext, Data: b})
		}
	}
	addFrom(corpusFilesDir())
	addFrom(corpusFuzzFinds()) // re-seed from prior divergence finds
	return out
}

// pickExt chooses an extension for a fuzz-mutated payload. The fuzzer mutates
// bytes of a seed; we keep the seed's original language by carrying its ext in
// a separate, non-mutated fuzz argument.
func writeTemp(t fataler, ext string, data []byte) (dir, file string) {
	d, err := os.MkdirTemp("", "cffuzz-")
	if err != nil {
		t.Fatalf("mktemp: %v", err)
	}
	f := filepath.Join(d, "input."+sanitizeExt(ext))
	if err := os.WriteFile(f, data, 0o644); err != nil {
		t.Fatalf("write temp: %v", err)
	}
	return d, f
}

// sanitizeExt guards against a fuzz-mutated extension containing path/control
// characters; we only ever use extensions from a fixed allowlist anyway.
func sanitizeExt(ext string) string {
	ext = strings.TrimPrefix(ext, ".")
	for _, c := range ext {
		if !(c >= 'a' && c <= 'z') && !(c >= 'A' && c <= 'Z') && !(c >= '0' && c <= '9') {
			return "txt"
		}
	}
	if ext == "" {
		return "txt"
	}
	return ext
}

// fataler is the subset of *testing.T / *testing.F we need.
type fataler interface {
	Fatalf(format string, args ...any)
}
