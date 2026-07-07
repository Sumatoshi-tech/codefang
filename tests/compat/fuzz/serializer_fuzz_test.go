package compatfuzz

import (
	"os"
	"testing"
	"time"
)

// staticRunArgv builds a deterministic single-directory static analyzer run.
// We pin every nondeterminism knob (--head --workers 1 --static-workers 1,
// no cache/checkpoint/resume) exactly as parity_gate.sh does, so the ONLY
// variable is the input source under dir.
func staticRunArgv(dir, analyzer, format string) []string {
	return []string{"run",
		"--checkpoint=false", "--resume=false", "--no-cache",
		"--head", "--workers", "1", "--static-workers", "1",
		"-p", dir, "--analyzers", analyzer, "--format", format}
}

// removeAll is os.RemoveAll, named locally to keep imports tidy in test files.
func removeAll(p string) { _ = os.RemoveAll(p) }

// runSerializerStage feeds mutated source through a real analyzer and serializes
// it with the chosen format, then diffs Go vs Rust. The analyzer is held fixed
// (static/complexity, fully deterministic in Go) so any divergence localizes to
// the SERIALIZER under test (cf-gojson / cf-goyaml / CFB1) rather than the
// metric computation.
func runSerializerStage(t *testing.T, stage, ext string, data []byte, format string) {
	ext = sanitizeExt(ext)
	dir, _ := writeTemp(t, ext, data) // writeTemp puts file inside dir
	defer removeAll(dir)

	argv := staticRunArgv(dir, "static/complexity", format)
	res, err := differential(argv, 3, 30*time.Second)
	if err != nil {
		t.Skipf("oracle exec error: %v", err)
	}
	if !res.OK {
		path, _ := distill(stage, ext, data, res)
		t.Fatalf("%s SERIALIZER DIVERGENCE: %s\n  %s\n  distilled -> %s",
			stage, res.Reason, firstDiff(res.GoOut, res.RustOut), path)
	}
}

// FuzzSerializerJSON — PURE STAGE 3a: cf-gojson serializer.
func FuzzSerializerJSON(f *testing.F) {
	for _, s := range loadSeeds("go", "py", "ts", "js", "c", "cpp", "rs") {
		f.Add(s.Ext, s.Data)
	}
	f.Fuzz(func(t *testing.T, ext string, data []byte) {
		runSerializerStage(t, "gojson", ext, data, "json")
	})
}

// FuzzSerializerYAML — PURE STAGE 3b: cf-goyaml serializer.
func FuzzSerializerYAML(f *testing.F) {
	for _, s := range loadSeeds("go", "py", "ts", "js", "c", "cpp", "rs") {
		f.Add(s.Ext, s.Data)
	}
	f.Fuzz(func(t *testing.T, ext string, data []byte) {
		runSerializerStage(t, "goyaml", ext, data, "yaml")
	})
}

// FuzzSerializerCFB1 — PURE STAGE 3c: CFB1 binary envelope serializer.
func FuzzSerializerCFB1(f *testing.F) {
	for _, s := range loadSeeds("go", "py", "ts", "js", "c", "cpp", "rs") {
		f.Add(s.Ext, s.Data)
	}
	f.Fuzz(func(t *testing.T, ext string, data []byte) {
		// CFB1 output is binary; differential() falls back to whole-blob
		// measured comparison (Go-stable bytes must match exactly).
		_ = time.Second
		runSerializerStage(t, "cfb1", ext, data, "bin")
	})
}
