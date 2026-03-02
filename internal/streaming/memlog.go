package streaming

import (
	"context"
	"log/slog"

	"github.com/Sumatoshi-tech/codefang/pkg/units"
)

// ChunkMemoryLog holds memory measurements for a single chunk.
type ChunkMemoryLog struct {
	ChunkIndex      int
	HeapBefore      int64
	HeapAfter       int64
	SysAfter        int64
	RSSAfter        int64
	BudgetUsedPct   float64
	GrowthPerCommit int64
	EMAGrowthRate   float64
	Replanned       bool
}

// LogChunkMemory emits a structured log entry with per-chunk memory telemetry.
func LogChunkMemory(ctx context.Context, logger *slog.Logger, entry ChunkMemoryLog) {
	logger.InfoContext(ctx, "streaming: chunk memory",
		"chunk", entry.ChunkIndex+1,
		"heap_before_mib", entry.HeapBefore/units.MiB,
		"heap_after_mib", entry.HeapAfter/units.MiB,
		"sys_mib", entry.SysAfter/units.MiB,
		"rss_mib", entry.RSSAfter/units.MiB,
		"budget_used_pct", entry.BudgetUsedPct,
		"growth_per_commit_kib", entry.GrowthPerCommit/units.KiB,
		"ema_growth_kib", int64(entry.EMAGrowthRate)/units.KiB,
		"replanned", entry.Replanned,
	)
}
