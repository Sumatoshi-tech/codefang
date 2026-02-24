package framework

import (
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
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

	var active int32

	var maxActive int32

	var startWg sync.WaitGroup

	startWg.Add(2)

	r.TCSink = func(_ analyze.TC, _ string) error {
		startWg.Done()
		startWg.Wait()

		current := atomic.AddInt32(&active, 1)

		for {
			maxA := atomic.LoadInt32(&maxActive)
			if current <= maxA {
				break
			}

			if atomic.CompareAndSwapInt32(&maxActive, maxA, current) {
				break
			}
		}

		time.Sleep(10 * time.Millisecond)
		atomic.AddInt32(&active, -1)

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
	assert.Equal(t, int32(2), atomic.LoadInt32(&maxActive), "should have 2 concurrent routes")
}
