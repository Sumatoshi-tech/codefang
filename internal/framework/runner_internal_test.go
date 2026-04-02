package framework

import (
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

type mockAnalyzer struct {
	analyze.HistoryAnalyzer

	flag string
}

func (m mockAnalyzer) Flag() string { return m.flag }

func TestRunner_drainWorkerTCs_ConcurrentRouting(t *testing.T) {
	t.Parallel()

	r := &Runner{
		Analyzers: []analyze.HistoryAnalyzer{
			mockAnalyzer{flag: "a0"},
			mockAnalyzer{flag: "a1"},
		},
		commitMeta: make(map[string]analyze.CommitMeta),
	}

	var active atomic.Int32

	var maxActive atomic.Int32

	var startWg sync.WaitGroup

	startWg.Add(2)

	r.TCSink = func(_ analyze.TC, _ string) error {
		startWg.Done()
		startWg.Wait()

		current := active.Add(1)

		for {
			maxA := maxActive.Load()
			if current <= maxA {
				break
			}

			if maxActive.CompareAndSwap(maxA, current) {
				break
			}
		}

		time.Sleep(10 * time.Millisecond)
		active.Add(-1)

		return nil
	}

	workers := []*leafWorker{
		{
			tcs: []bufferedTC{
				{idx: 0, tc: analyze.TC{CommitHash: gitlib.Hash{}}},
				{idx: 1, tc: analyze.TC{CommitHash: gitlib.Hash{}}},
			},
		},
	}

	start := time.Now()

	r.drainWorkerTCs(workers)

	elapsed := time.Since(start)

	assert.Less(t, elapsed, 50*time.Millisecond, "should run concurrently")
	assert.Equal(t, int32(2), maxActive.Load(), "should have 2 concurrent routes")
}
