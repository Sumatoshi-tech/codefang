package plumbing

const (
	// DependencyFileDiff is the name of the dependency provided by FileDiff.
	DependencyFileDiff = "file_diff"

	// DependencyTreeChanges is the name of the dependency provided by TreeDiff.
	DependencyTreeChanges = "changes"

	// DependencyTick is the name of the dependency which DaysSinceStart provides - the number
	// of ticks since the first commit in the analysed sequence.
	DependencyTick = "tick"

	// FactCommitsByTick contains the mapping between day indices and the corresponding commits.
	FactCommitsByTick = "TicksSinceStart.Commits"

	// FactTickSize contains the time.Duration of each tick.
	FactTickSize = "TicksSinceStart.TickSize"

	// DependencyBlobCache identifies the dependency provided by BlobCache.
	DependencyBlobCache = "blob_cache"

	// DependencyLanguages is the name of the dependency provided by LanguagesDetection.
	DependencyLanguages = "languages"

	// DependencyLineStats is the identifier of the data provided by LinesStatsCalculator - line
	// statistics for each file in the commit.
	DependencyLineStats = "line_stats"
)
