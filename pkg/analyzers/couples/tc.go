package couples

// RenamePair represents a file rename detected in a single commit.
type RenamePair struct {
	FromName string
	ToName   string
}

// CommitData is the per-commit TC payload emitted by Consume().
// It captures the coupling context, author-file touches, renames,
// and whether the author's commit count was incremented.
type CommitData struct {
	// CouplingFiles is the list of files forming the coupling context
	// (already filtered by CouplesMaximumMeaningfulContextSize).
	CouplingFiles []string
	// AuthorFiles maps file name to touch count for this commit's author.
	AuthorFiles map[string]int
	// Renames holds rename pairs detected in this commit.
	Renames []RenamePair
	// CommitCounted is true when this commit incremented the author's commit count.
	CommitCounted bool
}

// TickData is the per-tick aggregated payload stored in analyze.TICK.Data.
type TickData struct {
	// Files maps file -> otherFile -> co-occurrence count.
	Files map[string]map[string]int
	// People is per-author file touch counts, indexed by author ID.
	People []map[string]int
	// PeopleCommits is per-author commit counts, indexed by author ID.
	PeopleCommits []int
	// Renames accumulated during this tick.
	Renames []RenamePair
}
