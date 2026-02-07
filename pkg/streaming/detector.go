package streaming

// Size constants.
const (
	kib = 1024
	mib = 1024 * kib
)

// Detection thresholds.
const (
	// DefaultCommitThreshold is the commit count above which streaming is recommended.
	DefaultCommitThreshold = 50000

	// BaseOverhead is the fixed memory overhead for Go runtime + libgit2.
	BaseOverhead = 50 * mib

	// AvgStateGrowthPerCommit is the estimated memory growth per commit for analyzer state.
	AvgStateGrowthPerCommit = 2 * kib

	// BudgetSafetyFactor is the fraction of budget to use (leave headroom).
	BudgetSafetyFactor = 80

	// percentDivisor is used for percentage calculations.
	percentDivisor = 100
)

// Detector determines whether streaming mode should be used.
type Detector struct {
	CommitCount  int
	MemoryBudget int64
}

// ShouldStream returns true if streaming mode is recommended.
func (d *Detector) ShouldStream() bool {
	// Check commit threshold first.
	if d.CommitCount >= DefaultCommitThreshold {
		return true
	}

	// If budget is set, check if estimated memory exceeds it.
	if d.MemoryBudget > 0 {
		estimated := d.estimatePeakMemory()
		usableBudget := d.MemoryBudget * BudgetSafetyFactor / percentDivisor

		return estimated > usableBudget
	}

	return false
}

// estimatePeakMemory calculates the estimated peak memory for full history analysis.
func (d *Detector) estimatePeakMemory() int64 {
	stateGrowth := int64(d.CommitCount) * AvgStateGrowthPerCommit

	return BaseOverhead + stateGrowth
}
