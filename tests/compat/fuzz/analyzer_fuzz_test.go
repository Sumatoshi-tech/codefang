package compatfuzz

import (
	"testing"
	"time"
)

// FuzzComputeAllMetrics — PURE STAGE 4: an analyzer's pure ComputeAllMetrics.
//
// static/complexity's ComputeAllMetrics is a pure function of the parsed report
// (no git, no clock). Driving it through `codefang run --analyzers
// static/complexity --format json` over a single mutated source file exercises
// exactly that pure computation; the cf-gojson serializer is held fixed so a
// divergence localizes to the metric math (cyclomatic/cognitive counts,
// scoring, status labels). Differential vs the LIVE Go binary, measured-canon.
//
// We additionally probe a SECOND analyzer (static/composition, also pure +
// deterministic) so the stage is not single-analyzer.
func FuzzComputeAllMetrics(f *testing.F) {
	for _, s := range loadSeeds("go", "py", "ts", "js", "c", "cpp", "rs") {
		f.Add(s.Ext, s.Data)
	}
	f.Fuzz(func(t *testing.T, ext string, data []byte) {
		ext = sanitizeExt(ext)
		for _, analyzer := range []string{"static/complexity", "static/composition"} {
			dir, _ := writeTemp(t, ext, data)
			argv := staticRunArgv(dir, analyzer, "json")
			res, err := differential(argv, 3, 30*time.Second)
			removeAll(dir)
			if err != nil {
				t.Skipf("oracle exec error (%s): %v", analyzer, err)
			}
			if !res.OK {
				path, _ := distill("metrics_"+sanitizeAnalyzer(analyzer), ext, data, res)
				t.Fatalf("COMPUTEALLMETRICS DIVERGENCE [%s]: %s\n  %s\n  distilled -> %s",
					analyzer, res.Reason, firstDiff(res.GoOut, res.RustOut), path)
			}
		}
	})
}

// FuzzComputeAllMetricsGo digs into the analyzer's pure metric math for the ONE
// language whose grammar is wired in Rust (Go), so the mutator reaches scoring /
// status-label / counting branches instead of the grammar gap. Two pure,
// deterministic analyzers are probed.
func FuzzComputeAllMetricsGo(f *testing.F) {
	for _, s := range loadSeeds("go") {
		f.Add(s.Data)
	}
	f.Fuzz(func(t *testing.T, data []byte) {
		for _, analyzer := range []string{"static/complexity", "static/composition"} {
			dir, _ := writeTemp(t, "go", data)
			argv := staticRunArgv(dir, analyzer, "json")
			res, err := differential(argv, 3, 30*time.Second)
			removeAll(dir)
			if err != nil {
				t.Skipf("oracle exec error (%s): %v", analyzer, err)
			}
			if !res.OK {
				path, _ := distill("metrics_go_"+sanitizeAnalyzer(analyzer), "go", data, res)
				t.Fatalf("GO-COMPUTEALLMETRICS DIVERGENCE [%s]: %s\n  %s\n  -> %s",
					analyzer, res.Reason, firstDiff(res.GoOut, res.RustOut), path)
			}
		}
	})
}

func sanitizeAnalyzer(a string) string {
	out := make([]byte, 0, len(a))
	for i := 0; i < len(a); i++ {
		c := a[i]
		if c == '/' {
			c = '-'
		}
		out = append(out, c)
	}
	return string(out)
}

var _ = time.Second
