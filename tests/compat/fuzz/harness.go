// Package compatfuzz hosts Go-native differential fuzz targets (testing/F), one
// per PURE stage of the codefang/uast pipeline. Each target feeds the SAME input
// to the LIVE Go binary and the Rust binary (as subprocesses, under the pinned
// env) and FAILS on any divergence.
//
// NON-NEGOTIABLE design rules (encode why the prior effort failed):
//
//  1. ORACLE = the LIVE Go binary at $ROOT/build/bin/{codefang,uast}. We never
//     re-derive expected output in Go or Rust here; we run the actual Go binary.
//  2. Canonicalization is MEASURED, never declared. For each input we run Go
//     N>=3 times; ONLY JSON leaf fields that VARY across Go's own runs may be
//     normalized, and the differing Go outputs are stored as evidence. Blanking
//     a Go-STABLE field is the cheat that hid a real bug before; canonByMeasure
//     refuses to neutralize any field Go was stable on.
//  3. Run env is pinned exactly: TZ=UTC NO_COLOR=1 LANG=C LC_ALL=C
//     SOURCE_DATE_EPOCH=315532800; argv passed as a list (no shell glob). We
//     compare STDOUT only (stderr is progress).
//
// A divergence-finding input is distilled back into ../corpus/fuzzfinds/.
package compatfuzz

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sort"
	"time"
)

// ---------------------------------------------------------------------------
// Paths & pinned env
// ---------------------------------------------------------------------------

// repoRoot resolves $ROOT from this file's location (…/tests/compat/fuzz).
func repoRoot() string {
	_, self, _, ok := runtime.Caller(0)
	if !ok {
		panic("cannot resolve caller for repoRoot")
	}
	// self = …/tests/compat/fuzz/harness.go; Dir(self) = …/tests/compat/fuzz,
	// so three levels up is the repo root.
	return filepath.Clean(filepath.Join(filepath.Dir(self), "..", "..", ".."))
}

func goBin(name string) string { return filepath.Join(repoRoot(), "build", "bin", name) }

// rustBin resolves the Rust binary. CFFUZZ_RUST_DIR overrides the directory and
// is used EXCLUSIVELY by the self-proving tests to inject a buggy/shim "Rust"
// and assert the harness reports FAIL. It must never be set in a real run.
func rustBin(name string) string {
	if d := os.Getenv("CFFUZZ_RUST_DIR"); d != "" {
		return filepath.Join(d, name)
	}
	// The Rust workspace moved from rust/ to the repo root when the rewrite
	// superseded the Go implementation.
	return filepath.Join(repoRoot(), "target", "release", name)
}

func corpusFuzzFinds() string {
	return filepath.Join(repoRoot(), "tests", "compat", "corpus", "fuzzfinds")
}

var pinnedEnv = []string{
	"TZ=UTC", "NO_COLOR=1", "LANG=C", "LC_ALL=C",
	"SOURCE_DATE_EPOCH=315532800",
}

// pinnedEnviron returns the parent environment with the pinned vars forced on.
func pinnedEnviron() []string {
	base := os.Environ()
	keys := map[string]bool{}
	for _, kv := range pinnedEnv {
		eq := bytes.IndexByte([]byte(kv), '=')
		keys[kv[:eq]] = true
	}
	out := base[:0:0]
	for _, kv := range base {
		eq := bytes.IndexByte([]byte(kv), '=')
		if eq < 0 || !keys[kv[:eq]] {
			out = append(out, kv)
		}
	}
	return append(out, pinnedEnv...)
}

// ---------------------------------------------------------------------------
// Subprocess execution: the binaries ARE the oracle.
// ---------------------------------------------------------------------------

// side selects which implementation to run.
type side int

const (
	sideGo side = iota
	sideRust
)

// resolve maps a logical argv[0] to a real binary + prefix args, mirroring the
// parity_gate.sh / oracle.py convention: "uast …" -> uast binary; everything
// else (e.g. "run …", "version") -> codefang binary with argv[0] kept.
func resolve(s side, argv0 string) (string, []string) {
	bin := func(n string) string {
		if s == sideGo {
			return goBin(n)
		}
		return rustBin(n)
	}
	if argv0 == "uast" {
		return bin("uast"), nil
	}
	return bin("codefang"), []string{argv0}
}

// runOnce runs the LIVE binary once under the pinned env; returns stdout bytes.
// stderr is discarded (it is progress, per the contract). A timeout guards
// against pathological fuzz inputs.
func runOnce(s side, argv []string, timeout time.Duration) (rc int, stdout []byte, err error) {
	exe, prefix := resolve(s, argv[0])
	args := append(append([]string{}, prefix...), argv[1:]...)
	cmd := exec.Command(exe, args...)
	cmd.Env = pinnedEnviron()
	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = nil

	done := make(chan error, 1)
	if e := cmd.Start(); e != nil {
		return -1, nil, e
	}
	go func() { done <- cmd.Wait() }()
	select {
	case e := <-done:
		rc = cmd.ProcessState.ExitCode()
		return rc, out.Bytes(), e
	case <-time.After(timeout):
		_ = cmd.Process.Kill()
		<-done
		return -1, out.Bytes(), fmt.Errorf("timeout after %s", timeout)
	}
}

func shaHex(b []byte) string {
	s := sha256.Sum256(b)
	return hex.EncodeToString(s[:])
}

// ---------------------------------------------------------------------------
// MEASURED canonicalization.
//
// We run Go N times on the same input. Fields STABLE across all N Go runs MUST
// match byte-exact; fields that VARY across Go's own runs are the only ones we
// may neutralize. We never declare a field nondeterministic — we observe it.
// ---------------------------------------------------------------------------

// leafPaths yields {jsonpath: canonical-value-string} for every JSON leaf.
func leafPaths(obj any, prefix string, out map[string]string) {
	switch v := obj.(type) {
	case map[string]any:
		keys := make([]string, 0, len(v))
		for k := range v {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		for _, k := range keys {
			leafPaths(v[k], prefix+"."+k, out)
		}
	case []any:
		for i, e := range v {
			leafPaths(e, fmt.Sprintf("%s[%d]", prefix, i), out)
		}
	default:
		b, _ := json.Marshal(v)
		out[prefix] = string(b)
	}
}

func fieldMap(b []byte) (map[string]string, bool) {
	var j any
	if err := json.Unmarshal(b, &j); err != nil {
		return map[string]string{"$<bytes>": shaHex(b)}, false
	}
	m := map[string]string{}
	leafPaths(j, "$", m)
	return m, true
}

// measureGoVariance runs Go n times and returns:
//   - the set of field paths that VARIED across Go's own runs (legitimately
//     canonicalizable), and
//   - the list of raw Go outputs (evidence).
//
// If outputs are not JSON, variance is measured on the whole-byte blob.
func measureGoVariance(argv []string, n int, timeout time.Duration) (variant map[string]bool, runs [][]byte, err error) {
	variant = map[string]bool{}
	maps := make([]map[string]string, 0, n)
	for i := 0; i < n; i++ {
		_, out, e := runOnce(sideGo, argv, timeout)
		if e != nil {
			return nil, runs, fmt.Errorf("go run %d: %w", i, e)
		}
		runs = append(runs, out)
		fm, _ := fieldMap(out)
		maps = append(maps, fm)
	}
	// union of keys
	keys := map[string]bool{}
	for _, m := range maps {
		for k := range m {
			keys[k] = true
		}
	}
	for k := range keys {
		first, seen := "", false
		for _, m := range maps {
			v, ok := m[k]
			if !ok {
				variant[k] = true // present in some runs, absent in others
				break
			}
			if !seen {
				first, seen = v, true
			} else if v != first {
				variant[k] = true
				break
			}
		}
	}
	return variant, runs, nil
}

// canonByMeasure produces a canonical form of an output, neutralizing ONLY the
// fields measured as Go-variant. Go-stable fields are left untouched (byte
// content participates in the comparison). For non-JSON outputs we either pass
// the raw bytes through (when Go was byte-stable) or accept any bytes (when Go
// itself varied at the blob level).
func canonByMeasure(b []byte, variant map[string]bool, jsonMode bool) string {
	if !jsonMode {
		if variant["$<bytes>"] {
			return "<go-variant-bytes>"
		}
		return shaHex(b)
	}
	var j any
	if err := json.Unmarshal(b, &j); err != nil {
		// Rust produced non-JSON where Go produced JSON: a real divergence.
		return "<<NON-JSON:" + shaHex(b) + ">>"
	}
	var neutralize func(obj any, prefix string) any
	neutralize = func(obj any, prefix string) any {
		switch v := obj.(type) {
		case map[string]any:
			m := make(map[string]any, len(v))
			for k, e := range v {
				m[k] = neutralize(e, prefix+"."+k)
			}
			return m
		case []any:
			a := make([]any, len(v))
			for i, e := range v {
				a[i] = neutralize(e, fmt.Sprintf("%s[%d]", prefix, i))
			}
			return a
		default:
			if variant[prefix] {
				return "<go-variant>"
			}
			return v
		}
	}
	out := neutralize(j, "$")
	bb, _ := json.Marshal(canonSort(out))
	return string(bb)
}

// canonSort recursively sorts maps (json.Marshal already sorts map keys) and
// leaves lists in order — order differences in Go-stable lists are REAL
// divergences and must not be hidden.
func canonSort(o any) any { return o }

// ---------------------------------------------------------------------------
// The differential check used by every fuzz target.
// ---------------------------------------------------------------------------

// diffResult is the structured verdict of one differential comparison.
type diffResult struct {
	OK         bool
	GoOut      []byte
	RustOut    []byte
	Variant    map[string]bool
	Evidence   [][]byte // the N Go runs
	JSONMode   bool
	Reason     string
}

// differential runs the MEASURED differential check for one (stage, argv):
//   - measure Go variance over goRuns runs (>=3),
//   - run Rust once,
//   - compare canonical-by-measure; Go-stable fields must match byte-exact.
//
// It returns OK=false on any divergence. It does NOT itself call t.Fatal so the
// self-check can assert on a planted divergence.
func differential(argv []string, goRuns int, timeout time.Duration) (diffResult, error) {
	if goRuns < 3 {
		goRuns = 3
	}
	variant, runs, err := measureGoVariance(argv, goRuns, timeout)
	if err != nil {
		return diffResult{}, err
	}
	_, rust, err := runOnce(sideRust, argv, timeout)
	if err != nil {
		return diffResult{Reason: "rust exec error: " + err.Error()}, nil
	}
	goRef := runs[0]
	_, jsonMode := fieldMap(goRef)

	gc := canonByMeasure(goRef, variant, jsonMode)
	rc := canonByMeasure(rust, variant, jsonMode)

	res := diffResult{
		GoOut: goRef, RustOut: rust, Variant: variant,
		Evidence: runs, JSONMode: jsonMode,
	}
	if gc == rc {
		res.OK = true
		return res, nil
	}
	res.OK = false
	res.Reason = fmt.Sprintf("canonical divergence (go=%dB rust=%dB, variant-fields=%d)",
		len(goRef), len(rust), len(variant))
	return res, nil
}

// firstDiff returns a short hexdump-style description of the first byte that
// differs between a and b, to localize a divergence.
func firstDiff(a, b []byte) string {
	n := len(a)
	if len(b) < n {
		n = len(b)
	}
	for i := 0; i < n; i++ {
		if a[i] != b[i] {
			lo := i - 16
			if lo < 0 {
				lo = 0
			}
			return fmt.Sprintf("first diff @%d: go=%q rust=%q",
				i, snippet(a, lo, i+16), snippet(b, lo, i+16))
		}
	}
	if len(a) != len(b) {
		return fmt.Sprintf("length differs: go=%dB rust=%dB", len(a), len(b))
	}
	return "no byte diff (canonical-only divergence)"
}

func snippet(b []byte, lo, hi int) string {
	if lo < 0 {
		lo = 0
	}
	if hi > len(b) {
		hi = len(b)
	}
	return string(b[lo:hi])
}

// divergenceClass keys a divergence by its SHAPE, not its exact input bytes, so
// the distilled corpus keeps the SMALLEST set that still preserves every
// DISTINCT finding (SPEC §3 "distilled: minimized while preserving combined
// coverage"). Fuzzer minimization can emit hundreds of near-duplicate inputs
// for one underlying bug; we store one representative per class.
func divergenceClass(stage string, res diffResult) string {
	if res.JSONMode {
		// Class = the SET OF FIELD KEYS (last path segment) whose value differs
		// between Go and Rust, ignoring array indices / offsets. This collapses
		// "Rust computes a different node id" (one bug) into a single class
		// regardless of WHERE in the tree it surfaces.
		keys := diffKeySet(res.GoOut, res.RustOut)
		return shaHex([]byte(stage+"|json|"+keys))[:16]
	}
	// Non-JSON: classify by whether it's a length/first-byte/empty divergence and
	// by the Rust-output emptiness (constant-stub vs partial).
	shape := "bytes"
	if len(res.RustOut) == 0 {
		shape = "rust-empty"
	} else if len(res.GoOut) == 0 {
		shape = "go-empty"
	}
	return shaHex([]byte(stage+"|raw|"+shape))[:16]
}

// diffKeySet returns a sorted, comma-joined set of the LAST path segments whose
// leaf value differs between two JSON outputs (or a structural marker if one is
// non-JSON / array-shaped differently).
func diffKeySet(a, b []byte) string {
	ma, oka := fieldMap(a)
	mb, okb := fieldMap(b)
	if !oka || !okb {
		return "<non-json>"
	}
	keys := map[string]bool{}
	all := map[string]bool{}
	for k := range ma {
		all[k] = true
	}
	for k := range mb {
		all[k] = true
	}
	for p := range all {
		if ma[p] != mb[p] {
			keys[lastSegment(p)] = true
		}
	}
	out := make([]string, 0, len(keys))
	for k := range keys {
		out = append(out, k)
	}
	sort.Strings(out)
	return fmt.Sprintf("%v", out)
}

func lastSegment(path string) string {
	// strip trailing [i] index and take the segment after the last '.'
	end := len(path)
	for i := len(path) - 1; i >= 0; i-- {
		if path[i] == ']' {
			// find matching '['
			for j := i; j >= 0; j-- {
				if path[j] == '[' {
					end = j
					i = j
					break
				}
			}
			continue
		}
		if path[i] == '.' {
			return path[i+1 : end]
		}
	}
	return path[:end]
}

func indexByte(s string, c byte) int {
	for i := 0; i < len(s); i++ {
		if s[i] == c {
			return i
		}
	}
	return -1
}

func firstNonEmpty(bs [][]byte) []byte {
	for _, b := range bs {
		if len(b) > 0 {
			return b
		}
	}
	return nil
}

// distill writes a divergence-finding input back into the regression corpus so a
// later run reproduces it. It is deduplicated by divergence CLASS: if a
// representative of the same class is already stored, this is a no-op (keeps the
// corpus minimal). Returns the stored path.
func distill(stage, ext string, input []byte, res diffResult) (string, error) {
	dir := corpusFuzzFinds()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return "", err
	}
	cls := divergenceClass(stage, res)
	// Class-marker file: if it exists, we already captured this divergence shape.
	marker := filepath.Join(dir, ".class_"+cls)
	if _, err := os.Stat(marker); err == nil {
		return marker, nil
	}
	h := shaHex(input)
	base := filepath.Join(dir, fmt.Sprintf("%s_%s.%s", stage, h[:16], ext))
	if err := os.WriteFile(base, input, 0o644); err != nil {
		return "", err
	}
	_ = os.WriteFile(marker, []byte(base+"\n"), 0o644)
	// store evidence (the N Go runs) + the Rust output + reason
	meta := map[string]any{
		"stage":           stage,
		"input_sha256":    h,
		"reason":          res.Reason,
		"json_mode":       res.JSONMode,
		"variant_fields":  keysOf(res.Variant),
		"go_runs_sha256":  shaList(res.Evidence),
		"rust_out_sha256": shaHex(res.RustOut),
		"first_diff":      firstDiff(res.GoOut, res.RustOut),
	}
	mb, _ := json.MarshalIndent(meta, "", "  ")
	_ = os.WriteFile(base+".evidence.json", mb, 0o644)
	for i, r := range res.Evidence {
		_ = os.WriteFile(fmt.Sprintf("%s.go_run_%d", base, i), r, 0o644)
	}
	_ = os.WriteFile(base+".rust_out", res.RustOut, 0o644)
	return base, nil
}

func keysOf(m map[string]bool) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

func shaList(bs [][]byte) []string {
	out := make([]string, len(bs))
	for i, b := range bs {
		out[i] = shaHex(b)
	}
	return out
}
