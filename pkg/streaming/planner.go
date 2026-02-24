package streaming

import (
	"runtime"
	"time"
)

// Size constants.
const (
	kib = 1024
	mib = 1024 * kib

	// KiB is the exported kibibyte constant for use in log formatting.
	KiB = kib

	// MiB is the exported mebibyte constant for use in log formatting.
	MiB = mib
)

// Planner constraints.
const (
	// MinChunkSize is the minimum commits per chunk to amortize hibernation cost.
	MinChunkSize = 50

	// MaxChunkSize is the safety cap on commits per chunk. The primary constraint
	// is the memory budget divided by per-analyzer growth rate, not this cap.
	MaxChunkSize = 3000

	// BaseOverhead is the fixed memory overhead for Go runtime + libgit2 + caches.
	BaseOverhead = 400 * mib

	// safetyMarginPercent is added to the aggregate growth rate to account for
	// transient pipeline allocations (tree diffs, blobs in flight, GC headroom)
	// that scale with chunk size but aren't captured by analyzer-declared growth.
	safetyMarginPercent = 50

	// percentDivisor converts safetyMarginPercent to a fraction.
	percentDivisor = 100

	// DefaultStateGrowthPerCommit is the conservative fallback when
	// AggregateGrowthPerCommit is zero (e.g. in tests or when no analyzers
	// are selected). Equal to DefaultWorkingStateSize + DefaultAvgTCSize.
	DefaultStateGrowthPerCommit = 500 * kib

	// DefaultWorkingStateSize is the fallback per-commit working state estimate.
	DefaultWorkingStateSize = 400 * kib

	// DefaultAvgTCSize is the fallback per-commit TC payload estimate.
	DefaultAvgTCSize = 100 * kib
)

// Planner calculates chunk boundaries for streaming execution.
type Planner struct {
	TotalCommits int
	MemoryBudget int64

	// AggregateGrowthPerCommit is the summed per-commit state growth across all
	// selected leaf analyzers. When zero, DefaultStateGrowthPerCommit is used.
	AggregateGrowthPerCommit int64

	// PipelineOverhead is the estimated memory consumed by caches, workers, and
	// buffers (everything except analyzer state). When positive, it replaces
	// BaseOverhead for more accurate chunk sizing. The budget solver provides
	// this value via EstimateMemoryUsage.
	PipelineOverhead int64
}

// ChunkBounds represents a chunk of commits to process.
type ChunkBounds struct {
	Start int // Inclusive index.
	End   int // Exclusive index.
}

// Plan returns chunk boundaries as [start, end) index pairs.
func (p *Planner) Plan() []ChunkBounds {
	if p.TotalCommits <= 0 {
		return nil
	}

	chunkSize := p.calculateChunkSize()

	// Single chunk if all commits fit.
	if p.TotalCommits <= chunkSize {
		return []ChunkBounds{{Start: 0, End: p.TotalCommits}}
	}

	// Split into multiple chunks.
	var chunks []ChunkBounds

	for start := 0; start < p.TotalCommits; start += chunkSize {
		end := min(start+chunkSize, p.TotalCommits)
		chunks = append(chunks, ChunkBounds{Start: start, End: end})
	}

	return chunks
}

// calculateChunkSize determines the optimal chunk size based on budget and
// the aggregate per-commit growth rate of selected analyzers.
func (p *Planner) calculateChunkSize() int {
	if p.MemoryBudget <= 0 {
		return MaxChunkSize
	}

	// Available memory for analyzer state (after overhead).
	overhead := int64(BaseOverhead)
	if p.PipelineOverhead > 0 {
		overhead = p.PipelineOverhead
	}

	available := p.MemoryBudget - overhead
	if available <= 0 {
		return MinChunkSize
	}

	growth := p.AggregateGrowthPerCommit
	if growth <= 0 {
		growth = DefaultStateGrowthPerCommit
	}

	// Add safety margin for transient pipeline allocations.
	growth += growth * safetyMarginPercent / percentDivisor

	// Max commits that fit in available memory.
	maxCommits := int(available / growth)

	return max(min(maxCommits, MaxChunkSize), MinChunkSize)
}

// PlanFrom returns chunk boundaries for commits [startCommit..TotalCommits).
// Used by the adaptive planner to re-plan remaining chunks after observing
// actual growth rates.
func (p *Planner) PlanFrom(startCommit int) []ChunkBounds {
	remaining := p.TotalCommits - startCommit
	if remaining <= 0 {
		return nil
	}

	sub := &Planner{
		TotalCommits:             remaining,
		MemoryBudget:             p.MemoryBudget,
		AggregateGrowthPerCommit: p.AggregateGrowthPerCommit,
		PipelineOverhead:         p.PipelineOverhead,
	}

	subChunks := sub.Plan()

	for i := range subChunks {
		subChunks[i].Start += startCommit
		subChunks[i].End += startCommit
	}

	return subChunks
}

// HeapSnapshot captures Go runtime memory stats at a point in time.
type HeapSnapshot struct {
	HeapInuse int64
	HeapAlloc int64
	Sys       int64 // Total bytes obtained from the OS (Go runtime).
	NumGC     uint32
	TakenAtNS int64
}

// TakeHeapSnapshot reads [runtime.MemStats] and returns a HeapSnapshot.
func TakeHeapSnapshot() HeapSnapshot {
	var m runtime.MemStats
	runtime.ReadMemStats(&m)

	return HeapSnapshot{
		HeapInuse: int64(m.HeapInuse),
		HeapAlloc: int64(m.HeapAlloc),
		Sys:       int64(m.Sys),
		NumGC:     m.NumGC,
		TakenAtNS: time.Now().UnixNano(),
	}
}

// emaGrowthRate holds an exponentially-weighted moving average of the
// observed per-commit state growth.
type emaGrowthRate struct {
	value       float64
	initialized bool
}

// Update incorporates a new observation and returns the updated EMA.
// Alpha controls responsiveness: 1.0 = trust only latest, 0.3 = ~3-chunk half-life.
func (e *emaGrowthRate) Update(observed, alpha float64) float64 {
	if !e.initialized {
		e.value = observed
		e.initialized = true

		return e.value
	}

	e.value = alpha*observed + (1-alpha)*e.value

	return e.value
}

// Adaptive planner constants.
const (
	// DefaultReplanThreshold triggers re-planning when observed growth diverges
	// from predicted by more than 25%.
	DefaultReplanThreshold = 0.25

	// DefaultEMAAlpha controls EMA smoothing. 0.3 gives ~3-chunk half-life.
	DefaultEMAAlpha = 0.3

	// minObservedGrowth is the floor for observed per-commit growth (1 KiB).
	// Prevents zero/negative chunk sizes when hibernation frees more than allocated.
	minObservedGrowth = 1 * kib
)

// AdaptivePlanner wraps the static Planner with feedback-driven re-planning.
// After each chunk, it examines three separate metrics (working state growth,
// TC payload size, aggregator state growth), updates smoothed EMA estimates,
// and re-plans remaining chunks if any metric diverges beyond a threshold.
type AdaptivePlanner struct {
	totalCommits     int
	memoryBudget     int64
	pipelineOverhead int64
	declaredGrowth   int64
	currentGrowth    int64 // Growth rate used for the most recent plan.
	workEMA          emaGrowthRate
	tcEMA            emaGrowthRate
	aggEMA           emaGrowthRate
	alpha            float64
	replanThreshold  float64
	replanCount      int
}

// AdaptiveStats holds telemetry from the adaptive planner.
type AdaptiveStats struct {
	ReplanCount       int
	FinalGrowthRate   float64
	InitialGrowthRate float64
	FinalWorkGrowth   float64
	FinalTCSize       float64
	FinalAggGrowth    float64
}

// ReplanObservation carries per-chunk metric observations for adaptive replanning.
type ReplanObservation struct {
	// ChunkIndex is the zero-based index of the chunk just processed.
	ChunkIndex int

	// Chunk is the bounds of the chunk just processed.
	Chunk ChunkBounds

	// WorkGrowthPerCommit is the observed per-commit working state growth in bytes.
	// Computed as (HeapInuse delta - aggregator state delta) / commits.
	WorkGrowthPerCommit int64

	// TCPayloadPerCommit is the observed per-commit TC payload size in bytes.
	TCPayloadPerCommit int64

	// AggGrowthPerCommit is the observed per-commit aggregator state growth in bytes.
	AggGrowthPerCommit int64

	// CurrentChunks is the current chunk plan (including already-processed chunks).
	CurrentChunks []ChunkBounds
}

// NewAdaptivePlanner creates an adaptive planner seeded with the declared growth rate.
func NewAdaptivePlanner(totalCommits int, memBudget, declaredGrowth, pipelineOverhead int64) *AdaptivePlanner {
	return &AdaptivePlanner{
		totalCommits:     totalCommits,
		memoryBudget:     memBudget,
		pipelineOverhead: pipelineOverhead,
		declaredGrowth:   declaredGrowth,
		currentGrowth:    declaredGrowth,
		alpha:            DefaultEMAAlpha,
		replanThreshold:  DefaultReplanThreshold,
	}
}

// InitialPlan returns the first set of chunk boundaries using the declared growth rate.
func (ap *AdaptivePlanner) InitialPlan() []ChunkBounds {
	return ap.buildPlanner(ap.declaredGrowth).Plan()
}

// Replan examines three per-chunk metric observations (working state growth,
// TC payload size, aggregator state growth) and, if any metric diverges from
// prediction by more than replanThreshold, re-computes chunk boundaries for
// all chunks after the observed chunk.
//
// Processed chunks [0..obs.ChunkIndex] are never modified (checkpoint safety).
// The returned slice always covers exactly [0..totalCommits).
func (ap *AdaptivePlanner) Replan(obs ReplanObservation) []ChunkBounds {
	commitsInChunk := int64(obs.Chunk.End - obs.Chunk.Start)
	if commitsInChunk <= 0 {
		return obs.CurrentChunks
	}

	// Update all three EMAs with clamped observations.
	workVal := ap.workEMA.Update(float64(max(obs.WorkGrowthPerCommit, minObservedGrowth)), ap.alpha)
	tcVal := ap.tcEMA.Update(float64(max(obs.TCPayloadPerCommit, minObservedGrowth)), ap.alpha)
	aggVal := ap.aggEMA.Update(float64(max(obs.AggGrowthPerCommit, minObservedGrowth)), ap.alpha)

	// Predicted effective growth rate (with safety margin).
	rawGrowth := float64(ap.currentGrowth)
	if rawGrowth <= 0 {
		rawGrowth = float64(DefaultStateGrowthPerCommit)
	}

	predicted := rawGrowth + rawGrowth*safetyMarginPercent/percentDivisor

	// Check divergence for each metric independently.
	// Working state is compared against the predicted chunk-sizing growth rate.
	// TC and aggregator metrics are compared against their own EMA baseline
	// (the previous EMA value, or the first observation). Since we don't have
	// separate declared rates for TC and agg, we use the working state predicted
	// rate as a reference for all three — a divergence in any signals instability.
	triggered := exceedsThreshold(workVal, predicted, ap.replanThreshold) ||
		exceedsThreshold(tcVal, predicted, ap.replanThreshold) ||
		exceedsThreshold(aggVal, predicted, ap.replanThreshold)

	if !triggered {
		return obs.CurrentChunks
	}

	// Use work growth EMA for chunk resizing (TC and agg are informational).
	newRawGrowth := max(int64(workVal*percentDivisor/(percentDivisor+safetyMarginPercent)), minObservedGrowth)

	ap.currentGrowth = newRawGrowth
	ap.replanCount++

	planner := ap.buildPlanner(newRawGrowth)
	tailChunks := planner.PlanFrom(obs.Chunk.End)

	// Splice: keep processed chunks [0..obs.ChunkIndex], append new tail.
	result := make([]ChunkBounds, obs.ChunkIndex+1, obs.ChunkIndex+1+len(tailChunks))
	copy(result, obs.CurrentChunks[:obs.ChunkIndex+1])
	result = append(result, tailChunks...)

	return result
}

// exceedsThreshold returns true if the observed EMA value diverges from predicted
// by more than the given threshold fraction.
func exceedsThreshold(observed, predicted, threshold float64) bool {
	if predicted <= 0 {
		return false
	}

	divergence := (observed - predicted) / predicted
	if divergence < 0 {
		divergence = -divergence
	}

	return divergence > threshold
}

// Stats returns adaptive planner telemetry.
func (ap *AdaptivePlanner) Stats() AdaptiveStats {
	finalRate := float64(ap.declaredGrowth)
	if ap.workEMA.initialized {
		finalRate = ap.workEMA.value
	}

	return AdaptiveStats{
		ReplanCount:       ap.replanCount,
		FinalGrowthRate:   finalRate,
		InitialGrowthRate: float64(ap.declaredGrowth),
		FinalWorkGrowth:   ap.workEMA.value,
		FinalTCSize:       ap.tcEMA.value,
		FinalAggGrowth:    ap.aggEMA.value,
	}
}

// TotalCommits returns the total number of commits being planned.
func (ap *AdaptivePlanner) TotalCommits() int {
	return ap.totalCommits
}

// DeclaredGrowth returns the initial declared growth rate in bytes/commit.
func (ap *AdaptivePlanner) DeclaredGrowth() int64 {
	return ap.declaredGrowth
}

func (ap *AdaptivePlanner) buildPlanner(growth int64) *Planner {
	return &Planner{
		TotalCommits:             ap.totalCommits,
		MemoryBudget:             ap.memoryBudget,
		AggregateGrowthPerCommit: growth,
		PipelineOverhead:         ap.pipelineOverhead,
	}
}

// Memory pressure detection constants.
const (
	// PressureWarningRatio is the fraction of budget at which a warning is logged.
	PressureWarningRatio = 0.80

	// PressureCriticalRatio is the fraction of budget at which early hibernation
	// is triggered to prevent OOM before the next chunk starts.
	PressureCriticalRatio = 0.90
)

// MemoryPressureLevel indicates how close heap usage is to the budget.
type MemoryPressureLevel int

const (
	// PressureNone indicates heap usage is well within budget.
	PressureNone MemoryPressureLevel = iota

	// PressureWarning indicates heap usage exceeds 80% of budget.
	PressureWarning

	// PressureCritical indicates heap usage exceeds 90% of budget.
	PressureCritical
)

// CheckMemoryPressure compares the current heap usage against the memory budget
// and returns the pressure level. Returns PressureNone when budget is zero
// (unlimited).
func CheckMemoryPressure(heapInuse, memBudget int64) MemoryPressureLevel {
	if memBudget <= 0 {
		return PressureNone
	}

	ratio := float64(heapInuse) / float64(memBudget)

	switch {
	case ratio >= PressureCriticalRatio:
		return PressureCritical
	case ratio >= PressureWarningRatio:
		return PressureWarning
	default:
		return PressureNone
	}
}

// Budget decomposition constants (SPEC section 3.2).
const (
	// UsablePercent is the fraction of total budget available after slack reserve.
	UsablePercent = 95

	// WorkStatePercent is the fraction of remaining budget for analyzer working state.
	WorkStatePercent = 60

	// AggStatePercent is the fraction of remaining budget for aggregator state.
	AggStatePercent = 30

	// ChunkMemPercent is the fraction of remaining budget for in-flight data.
	ChunkMemPercent = 10
)

// SchedulerConfig holds inputs for the unified budget-aware scheduler.
type SchedulerConfig struct {
	// TotalCommits is the number of commits to process.
	TotalCommits int

	// MemoryBudget is the user-specified memory budget in bytes. Zero means unlimited.
	MemoryBudget int64

	// PipelineOverhead is the estimated fixed overhead for the pipeline
	// (workers, caches, buffers). When zero, BaseOverhead is used.
	PipelineOverhead int64

	// WorkStatePerCommit is the per-commit working state growth in bytes.
	// When zero, DefaultWorkingStateSize is used.
	WorkStatePerCommit int64

	// AvgTCSize is the average TC payload size per commit in bytes.
	// Currently informational; used for future chunkMem sizing.
	AvgTCSize int64

	// MaxBuffering is the maximum buffering factor (1=single, 2=double, 3=triple).
	// The scheduler iterates from MaxBuffering down to 1, selecting the highest
	// factor where ChunkSize >= MinChunkSize. When zero or negative, treated as 1.
	MaxBuffering int
}

// Schedule holds the computed scheduling parameters.
type Schedule struct {
	// Chunks are the planned chunk boundaries.
	Chunks []ChunkBounds

	// ChunkSize is the number of commits per chunk (last chunk may be smaller).
	ChunkSize int

	// BufferingFactor is the pipelining factor (1=single, 2=double, 3=triple).
	BufferingFactor int

	// AggSpillBudget is the maximum bytes of aggregator state before spilling.
	// Zero means no limit (unlimited budget or budget too small).
	AggSpillBudget int64
}

// clampMaxBuffering returns maxBuf clamped to at least 1.
func clampMaxBuffering(maxBuf int) int {
	if maxBuf <= 0 {
		return 1
	}

	return maxBuf
}

// ComputeSchedule decomposes the memory budget into P + W + A + S regions
// and computes chunk boundaries, buffering factor, and aggregator spill budget.
// The buffering factor is the highest value in [1, MaxBuffering] for which
// ChunkSize >= MinChunkSize. Only the workState region is divided among
// buffering slots; AggSpillBudget is unaffected.
func ComputeSchedule(cfg SchedulerConfig) Schedule {
	maxBuf := clampMaxBuffering(cfg.MaxBuffering)

	if cfg.TotalCommits <= 0 {
		return Schedule{BufferingFactor: 1}
	}

	if cfg.MemoryBudget <= 0 {
		chunks := (&Planner{TotalCommits: cfg.TotalCommits}).Plan()

		chunkSize := MaxChunkSize
		if len(chunks) > 0 {
			chunkSize = chunks[0].End - chunks[0].Start
		}

		return Schedule{
			Chunks:          chunks,
			ChunkSize:       chunkSize,
			BufferingFactor: maxBuf,
			AggSpillBudget:  0,
		}
	}

	usable := cfg.MemoryBudget * UsablePercent / percentDivisor

	overhead := cfg.PipelineOverhead
	if overhead <= 0 {
		overhead = int64(BaseOverhead)
	}

	remaining := usable - overhead
	if remaining <= 0 {
		chunks := buildChunks(cfg.TotalCommits, MinChunkSize)

		return Schedule{
			Chunks:          chunks,
			ChunkSize:       MinChunkSize,
			BufferingFactor: 1,
			AggSpillBudget:  0,
		}
	}

	workState := remaining * WorkStatePercent / percentDivisor
	aggState := remaining * AggStatePercent / percentDivisor

	growth := cfg.WorkStatePerCommit
	if growth <= 0 {
		growth = DefaultWorkingStateSize
	}

	effectiveGrowth := growth + growth*safetyMarginPercent/percentDivisor

	// Iterate from maxBuf down to 1, selecting the highest factor where
	// chunkSize >= MinChunkSize. Only workState is divided among slots.
	chosenFactor := 1
	chosenChunkSize := MinChunkSize

	for bf := maxBuf; bf >= 1; bf-- {
		cs := int(workState / (int64(bf) * effectiveGrowth))
		cs = min(cs, MaxChunkSize)

		if cs >= MinChunkSize {
			chosenFactor = bf
			chosenChunkSize = cs

			break
		}
	}

	chunks := buildChunks(cfg.TotalCommits, chosenChunkSize)

	return Schedule{
		Chunks:          chunks,
		ChunkSize:       chosenChunkSize,
		BufferingFactor: chosenFactor,
		AggSpillBudget:  aggState,
	}
}

// buildChunks splits totalCommits into chunks of the given size.
func buildChunks(totalCommits, chunkSize int) []ChunkBounds {
	if totalCommits <= 0 || chunkSize <= 0 {
		return nil
	}

	var chunks []ChunkBounds

	for start := 0; start < totalCommits; start += chunkSize {
		end := min(start+chunkSize, totalCommits)
		chunks = append(chunks, ChunkBounds{Start: start, End: end})
	}

	return chunks
}
