package streaming

import (
	"context"
	"log/slog"
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
		"heap_before_mib", entry.HeapBefore/int64(mib),
		"heap_after_mib", entry.HeapAfter/int64(mib),
		"sys_mib", entry.SysAfter/int64(mib),
		"rss_mib", entry.RSSAfter/int64(mib),
		"budget_used_pct", entry.BudgetUsedPct,
		"growth_per_commit_kib", entry.GrowthPerCommit/int64(kib),
		"ema_growth_kib", int64(entry.EMAGrowthRate)/int64(kib),
		"replanned", entry.Replanned,
	)
}
