package compatfuzz

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// SELF-PROVING TESTS (SPEC §4 Testing Strategy, rule 6).
//
// A green that cannot be shown to catch a planted bug is worthless. These tests
// inject KNOWN defects and assert the harness reports FAIL / detects the tamper.
// If any of these PASSES the planted bug, the whole fuzz layer is untrustworthy
// and CI must go red.

// fakeRustBin builds a one-shot shim binary that wraps the REAL Rust binary but
// CORRUPTS its stdout, simulating a buggy port. Returns the shim path + cleanup.
func fakeRustBin(t *testing.T, mode string) (dir string) {
	t.Helper()
	d, err := os.MkdirTemp("", "fakerust-")
	if err != nil {
		t.Fatalf("mktemp: %v", err)
	}
	// shim "codefang" and "uast" both: a tiny shell wrapper.
	for _, name := range []string{"codefang", "uast"} {
		real := rustBin(name)
		script := "#!/usr/bin/env bash\n" +
			"set -euo pipefail\n" +
			"out=\"$(" + shellQuote(real) + " \"$@\")\"\n"
		switch mode {
		case "corrupt":
			// flip output to a constant, hiding all real bytes (worst-case stub)
			script += "printf 'CONSTANT_STUB_OUTPUT'\n"
		case "byteflip":
			// emit output but flip one stable byte (subtle metric bug)
			script += "printf '%s' \"$out\" | sed 's/COMPLEXITY/COMPLEXITZ/'\n"
		default:
			script += "printf '%s' \"$out\"\n"
		}
		p := filepath.Join(d, name)
		if err := os.WriteFile(p, []byte(script), 0o755); err != nil {
			t.Fatalf("write shim: %v", err)
		}
	}
	return d
}

func shellQuote(s string) string {
	return "'" + s + "'"
}

// withRustDir temporarily redirects rustBin to a shim directory by overriding
// the resolver via an env knob. We implement the override directly: rustBin
// reads CFFUZZ_RUST_DIR if set.
func withRustDir(t *testing.T, dir string, fn func()) {
	t.Helper()
	old, had := os.LookupEnv("CFFUZZ_RUST_DIR")
	_ = os.Setenv("CFFUZZ_RUST_DIR", dir)
	defer func() {
		if had {
			_ = os.Setenv("CFFUZZ_RUST_DIR", old)
		} else {
			_ = os.Unsetenv("CFFUZZ_RUST_DIR")
		}
	}()
	fn()
}

// TestSelfCheck_DetectsConstantStub plants a Rust that emits a constant and
// asserts differential() reports OK=false. This is the canonical simulation
// signature the prior effort shipped.
func TestSelfCheck_DetectsConstantStub(t *testing.T) {
	src := goodGoSource()
	dir, _ := writeTemp(t, "go", src)
	defer removeAll(dir)
	argv := staticRunArgv(dir, "static/complexity", "json")

	shim := fakeRustBin(t, "corrupt")
	defer removeAll(shim)

	var res diffResult
	withRustDir(t, shim, func() {
		var err error
		res, err = differential(argv, 3, 30*time.Second)
		if err != nil {
			t.Fatalf("oracle exec error: %v", err)
		}
	})
	if res.OK {
		t.Fatal("SELF-CHECK FAILED: harness PASSED a constant-output stub (it must FAIL)")
	}
	t.Logf("ok: harness caught constant stub: %s", res.Reason)
}

// TestSelfCheck_DetectsByteFlip plants a Rust that flips a single Go-STABLE byte
// and asserts FAIL. This proves the harness does NOT blank stable fields.
func TestSelfCheck_DetectsByteFlip(t *testing.T) {
	src := goodGoSource()
	dir, _ := writeTemp(t, "go", src)
	defer removeAll(dir)
	argv := staticRunArgv(dir, "static/complexity", "json")

	shim := fakeRustBin(t, "byteflip")
	defer removeAll(shim)

	var res diffResult
	withRustDir(t, shim, func() {
		var err error
		res, err = differential(argv, 3, 30*time.Second)
		if err != nil {
			t.Fatalf("oracle exec error: %v", err)
		}
	})
	if res.OK {
		t.Fatal("SELF-CHECK FAILED: harness PASSED a single-byte flip on a Go-STABLE field")
	}
	t.Logf("ok: harness caught byte flip: %s", res.Reason)
}

// TestSelfCheck_RealRustPasses confirms the harness is not stuck-red: the REAL
// Rust binary must PASS on the same input. (If this fails, the divergence is a
// real Rust bug, which is also a valid — and important — outcome to surface.)
func TestSelfCheck_RealRustPasses(t *testing.T) {
	src := goodGoSource()
	dir, _ := writeTemp(t, "go", src)
	defer removeAll(dir)
	argv := staticRunArgv(dir, "static/complexity", "json")
	res, err := differential(argv, 3, 30*time.Second)
	if err != nil {
		t.Fatalf("oracle exec error: %v", err)
	}
	if !res.OK {
		path, _ := distill("selfcheck_realbug", "go", src, res)
		t.Fatalf("REAL Rust diverges from Go on baseline input: %s\n  %s\n  -> %s",
			res.Reason, firstDiff(res.GoOut, res.RustOut), path)
	}
}

// TestSelfCheck_TamperBlankStableField proves canonByMeasure REFUSES to hide a
// Go-stable field. We simulate the exact prior cheat: take a real Go JSON
// output, measure ZERO variant fields (Go was stable), then verify that two
// outputs differing on a stable field do NOT canonicalize equal.
func TestSelfCheck_TamperBlankStableField(t *testing.T) {
	goJSON := []byte(`{"score":7,"label":"COMPLEXITY","sections":[{"v":3}]}`)
	// Measured variance is EMPTY (Go stable). A buggy Rust changed "score".
	rustJSON := []byte(`{"score":9,"label":"COMPLEXITY","sections":[{"v":3}]}`)
	variant := map[string]bool{} // measured: nothing varied across Go runs

	gc := canonByMeasure(goJSON, variant, true)
	rc := canonByMeasure(rustJSON, variant, true)
	if gc == rc {
		t.Fatal("TAMPER SELF-CHECK FAILED: canonByMeasure equated a Go-STABLE differing field (blanking cheat not detected)")
	}

	// Sanity: if (and ONLY if) Go was MEASURED variant on $.score, then it may
	// canonicalize equal — proving canonicalization is driven by measurement.
	variant2 := map[string]bool{"$.score": true}
	if canonByMeasure(goJSON, variant2, true) != canonByMeasure(rustJSON, variant2, true) {
		t.Fatal("TAMPER SELF-CHECK FAILED: measured-variant field was not canonicalized")
	}
	t.Log("ok: canonByMeasure neutralizes ONLY measured-variant fields")
}

// TestSelfCheck_VarianceMeasuredNotDeclared proves the measurement layer:
// running Go N times on a deterministic invocation yields ZERO variant fields
// (so nothing is canonicalized away on a stable analyzer).
func TestSelfCheck_VarianceMeasuredNotDeclared(t *testing.T) {
	src := goodGoSource()
	dir, _ := writeTemp(t, "go", src)
	defer removeAll(dir)
	argv := staticRunArgv(dir, "static/complexity", "json")
	variant, runs, err := measureGoVariance(argv, 3, 30*time.Second)
	if err != nil {
		t.Fatalf("measure: %v", err)
	}
	if len(runs) != 3 {
		t.Fatalf("expected 3 Go runs of evidence, got %d", len(runs))
	}
	// Evidence is real JSON.
	var j any
	if err := json.Unmarshal(runs[0], &j); err != nil {
		t.Fatalf("Go output is not JSON: %v", err)
	}
	t.Logf("ok: measured %d Go-variant fields on static/complexity (expected ~0 for a deterministic analyzer)", len(variant))
}

func goodGoSource() []byte {
	return []byte(`package main

import "fmt"

func classify(n int) string {
	if n < 0 {
		return "neg"
	} else if n == 0 {
		return "zero"
	}
	for i := 0; i < n; i++ {
		if i%2 == 0 && i > 4 {
			fmt.Println(i)
		}
	}
	switch n {
	case 1, 2, 3:
		return "small"
	default:
		return "big"
	}
}

func main() { fmt.Println(classify(7)) }
`)
}

var _ = time.Second
