package streaming_test

import (
	"bytes"
	"context"
	"log/slog"
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/internal/streaming"
	"github.com/Sumatoshi-tech/codefang/pkg/units"
)

func TestLogChunkMemory_EmitsStructuredFields(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	logger := slog.New(slog.NewTextHandler(&buf, &slog.HandlerOptions{Level: slog.LevelInfo}))

	streaming.LogChunkMemory(context.Background(), logger, streaming.ChunkMemoryLog{
		ChunkIndex:      2,
		HeapBefore:      500 * units.KiB * 1024,
		HeapAfter:       900 * units.KiB * 1024,
		BudgetUsedPct:   43.5,
		GrowthPerCommit: 478 * units.KiB,
		EMAGrowthRate:   502 * float64(units.KiB),
		Replanned:       false,
	})

	output := buf.String()
	assert.Contains(t, output, "streaming: chunk memory")
	assert.Contains(t, output, "chunk=3")
	assert.Contains(t, output, "replanned=false")
}

func TestLogChunkMemory_ReplanTrue(t *testing.T) {
	t.Parallel()

	var buf bytes.Buffer

	logger := slog.New(slog.NewTextHandler(&buf, &slog.HandlerOptions{Level: slog.LevelInfo}))

	streaming.LogChunkMemory(context.Background(), logger, streaming.ChunkMemoryLog{
		ChunkIndex:      0,
		HeapBefore:      0,
		HeapAfter:       100 * units.KiB * 1024,
		BudgetUsedPct:   10.0,
		GrowthPerCommit: 100 * units.KiB,
		EMAGrowthRate:   100 * float64(units.KiB),
		Replanned:       true,
	})

	output := buf.String()
	assert.Contains(t, output, "replanned=true")
}
