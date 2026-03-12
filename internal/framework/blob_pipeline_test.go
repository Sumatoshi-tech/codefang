package framework_test

import (
	"testing"

	"github.com/Sumatoshi-tech/codefang/internal/framework"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

func TestBlobPipeline_NewBlobPipeline(t *testing.T) {
	t.Parallel()

	seqCh := make(chan gitlib.WorkerRequest, 1)
	poolCh := make(chan gitlib.WorkerRequest, 1)

	p := framework.NewBlobPipeline(seqCh, poolCh, 5, 1)
	if p == nil {
		t.Fatal("NewBlobPipeline returned nil")
	}

	if p.SeqWorkerChan != seqCh {
		t.Error("SeqWorkerChan not set")
	}

	if p.PoolWorkerChan != poolCh {
		t.Error("PoolWorkerChan not set")
	}

	if p.BufferSize != 5 {
		t.Errorf("BufferSize = %d, want 5", p.BufferSize)
	}
}

func TestBlobPipeline_NewBlobPipelineZeroBufferSize(t *testing.T) {
	t.Parallel()

	seqCh := make(chan gitlib.WorkerRequest, 1)
	poolCh := make(chan gitlib.WorkerRequest, 1)

	p := framework.NewBlobPipeline(seqCh, poolCh, 0, 1)
	if p == nil {
		t.Fatal("NewBlobPipeline returned nil")
	}

	if p.BufferSize != 1 {
		t.Errorf("BufferSize = %d, want 1 (normalized)", p.BufferSize)
	}
}
