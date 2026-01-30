package devs

import (
	"errors"
	"fmt"
	"io"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"

	git2go "github.com/libgit2/git2go/v34"
	"github.com/src-d/enry/v2"

	"github.com/Sumatoshi-tech/codefang/pkg/analyzers/analyze"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// Sentinel errors for serialization.
var (
	ErrInvalidTicks      = errors.New("missing or invalid Ticks in result")
	ErrInvalidPeopleDict = errors.New("missing or invalid ReversedPeopleDict in result")
	ErrInvalidTickSize   = errors.New("missing or invalid TickSize in result")
)

// Constants for tick calculations.
const (
	// DefaultTickDuration is the default time bucket size for developer activity.
	DefaultTickDuration = 24 * time.Hour
	// TickSizeSeconds is the tick size in seconds for fast calculations.
	tickSizeSeconds = 24 * 60 * 60
)

// Libgit2Analyzer uses libgit2 (via git2go) for high-performance analysis.
// Libgit2 is embedded C code, not an external binary dependency.
type Libgit2Analyzer struct {
	RepoPath string
	Since    time.Time
	Limit    int
}

// NewFastAnalyzer creates a new fast devs analyzer using libgit2.
func NewFastAnalyzer() *Libgit2Analyzer {
	return &Libgit2Analyzer{}
}

// Analyze runs the fast devs analysis on the given repository.
func (fa *Libgit2Analyzer) Analyze(repoPath, sinceStr string, limit int) (analyze.Report, error) {
	fa.RepoPath = repoPath
	fa.Limit = limit

	if sinceStr != "" {
		fa.Since = parseSinceTime(sinceStr)
	}

	return fa.Run()
}

// parseSinceTime parses a time string as duration, RFC3339, or DateOnly format.
func parseSinceTime(sinceStr string) time.Time {
	// Try parsing as duration (e.g., "24h") relative to now.
	d, durErr := time.ParseDuration(sinceStr)
	if durErr == nil {
		return time.Now().Add(-d)
	}

	// Try RFC3339.
	ts, rfc3339Err := time.Parse(time.RFC3339, sinceStr)
	if rfc3339Err == nil {
		return ts
	}

	// Try YYYY-MM-DD.
	dateTS, dateErr := time.Parse(time.DateOnly, sinceStr)
	if dateErr == nil {
		return dateTS
	}

	return time.Time{}
}

// libgit2WorkerResult holds the aggregated stats from one worker.
type libgit2WorkerResult struct {
	ticks              map[int]map[int]*DevTick
	peopleDict         map[string]int
	reversedPeopleDict []string
	err                error
}

// Run executes the fast analysis using parallel libgit2 workers.
func (fa *Libgit2Analyzer) Run() (analyze.Report, error) {
	// Open repository to get commit list.
	repo, err := git2go.OpenRepository(fa.RepoPath)
	if err != nil {
		return nil, fmt.Errorf("failed to open repository: %w", err)
	}
	defer repo.Free()

	// Collect commit OIDs.
	oids, startTime, err := fa.collectCommits(repo)
	if err != nil {
		return nil, err
	}

	if len(oids) == 0 {
		return analyze.Report{
			"Ticks":              map[int]map[int]*DevTick{},
			"ReversedPeopleDict": []string{},
			"TickSize":           DefaultTickDuration,
		}, nil
	}

	// Determine parallelism.
	numWorkers := max(1, min(runtime.NumCPU(), len(oids)))

	chunkSize := (len(oids) + numWorkers - 1) / numWorkers

	// Launch workers.
	results := make(chan libgit2WorkerResult, numWorkers)

	var wg sync.WaitGroup

	for i := range numWorkers {
		start := i * chunkSize
		end := min(start+chunkSize, len(oids))

		if start >= end {
			continue
		}

		wg.Add(1)

		go func(chunk []*git2go.Oid) {
			defer wg.Done()

			results <- fa.processChunk(chunk, startTime)
		}(oids[start:end])
	}

	go func() {
		wg.Wait()
		close(results)
	}()

	return fa.mergeResults(results)
}

// collectCommits walks the history and collects commit OIDs.
func (fa *Libgit2Analyzer) collectCommits(repo *git2go.Repository) ([]*git2go.Oid, int64, error) {
	// Get HEAD.
	head, err := repo.Head()
	if err != nil {
		return nil, 0, fmt.Errorf("failed to get HEAD: %w", err)
	}
	defer head.Free()

	// Create revwalk.
	walk, err := repo.Walk()
	if err != nil {
		return nil, 0, fmt.Errorf("failed to create revwalk: %w", err)
	}
	defer walk.Free()

	// Push HEAD and sort by time (oldest first after reverse).
	err = walk.Push(head.Target())
	if err != nil {
		return nil, 0, fmt.Errorf("failed to push HEAD to revwalk: %w", err)
	}

	walk.Sorting(git2go.SortTime | git2go.SortReverse)

	var oids []*git2go.Oid

	count := 0

	err = walk.Iterate(func(commit *git2go.Commit) bool {
		defer commit.Free()

		// Skip merges.
		if commit.ParentCount() > 1 {
			return true
		}

		// Check since filter.
		if !fa.Since.IsZero() && commit.Committer().When.Before(fa.Since) {
			return true
		}

		// Check limit.
		if fa.Limit > 0 && count >= fa.Limit {
			return false
		}

		oids = append(oids, commit.Id())
		count++

		return true
	})
	if err != nil {
		return nil, 0, fmt.Errorf("failed to iterate commits: %w", err)
	}

	// Get start time from first commit.
	var startTime int64

	if len(oids) > 0 {
		firstCommit, lookupErr := repo.LookupCommit(oids[0])
		if lookupErr == nil {
			startTime = firstCommit.Committer().When.Unix()
			firstCommit.Free()
		}
	}

	return oids, startTime, nil
}

// processChunk processes a slice of commits in an isolated goroutine.
func (fa *Libgit2Analyzer) processChunk(oids []*git2go.Oid, startTime int64) libgit2WorkerResult {
	result := libgit2WorkerResult{
		ticks:              make(map[int]map[int]*DevTick),
		peopleDict:         make(map[string]int),
		reversedPeopleDict: []string{},
	}

	// Each worker opens its own repository for isolation.
	repo, repoErr := git2go.OpenRepository(fa.RepoPath)
	if repoErr != nil {
		result.err = repoErr

		return result
	}
	defer repo.Free()

	tickSize := int64(tickSizeSeconds)

	// Create diff options once.
	diffOpts, optsErr := git2go.DefaultDiffOptions()
	if optsErr != nil {
		result.err = optsErr

		return result
	}

	for _, oid := range oids {
		commit, commitErr := repo.LookupCommit(oid)
		if commitErr != nil {
			continue
		}

		// Author resolution.
		sig := commit.Author()
		authorID := fa.getAuthorID(&result, sig.Name, sig.Email)

		// Tick calculation.
		tick := int((commit.Committer().When.Unix() - startTime) / tickSize)

		if result.ticks[tick] == nil {
			result.ticks[tick] = make(map[int]*DevTick)
		}

		if result.ticks[tick][authorID] == nil {
			result.ticks[tick][authorID] = &DevTick{
				Languages: make(map[string]pkgplumbing.LineStats),
			}
		}

		stats := result.ticks[tick][authorID]
		stats.Commits++

		// Get diff stats using libgit2's efficient implementation.
		fa.processDiff(repo, commit, stats, &diffOpts)

		commit.Free()
	}

	return result
}

// getAuthorID resolves or creates an author ID.
func (fa *Libgit2Analyzer) getAuthorID(result *libgit2WorkerResult, name, email string) int {
	keyEmail := strings.ToLower(email)
	if id, ok := result.peopleDict[keyEmail]; ok {
		return id
	}

	keyName := strings.ToLower(name)
	if id, ok := result.peopleDict[keyName]; ok {
		return id
	}

	id := len(result.reversedPeopleDict)
	result.peopleDict[keyEmail] = id
	result.peopleDict[keyName] = id
	result.reversedPeopleDict = append(result.reversedPeopleDict, name)

	return id
}

// fileLineStats tracks per-file line statistics for language aggregation.
type fileLineStats struct {
	lang    string
	added   int
	removed int
}

// getParentTree returns the parent tree for a commit (nil for initial commit).
func getParentTree(commit *git2go.Commit) (tree *git2go.Tree, cleanup func()) {
	if commit.ParentCount() == 0 {
		return nil, func() {}
	}

	parent := commit.Parent(0)
	if parent == nil {
		return nil, func() {}
	}

	parentTree, err := parent.Tree()
	parent.Free()

	if err != nil || parentTree == nil {
		return nil, func() {}
	}

	return parentTree, parentTree.Free
}

// processDiff gets the diff stats using libgit2's native diff implementation.
func (fa *Libgit2Analyzer) processDiff(
	repo *git2go.Repository, commit *git2go.Commit, stats *DevTick, diffOpts *git2go.DiffOptions,
) {
	currentTree, err := commit.Tree()
	if err != nil {
		return
	}
	defer currentTree.Free()

	parentTree, freeParent := getParentTree(commit)
	defer freeParent()

	diff, err := repo.DiffTreeToTree(parentTree, currentTree, diffOpts)
	if err != nil {
		return
	}
	defer freeDiff(diff)

	var currentFile *fileLineStats

	processErr := diff.ForEach(
		fa.createFileCallback(&currentFile, stats),
		git2go.DiffDetailLines,
	)
	if processErr == nil {
		finalizeFileStats(currentFile, stats)
	}
}

// createFileCallback creates the diff file callback for processing line stats.
func (fa *Libgit2Analyzer) createFileCallback(
	currentFile **fileLineStats, stats *DevTick,
) func(delta git2go.DiffDelta, progress float64) (git2go.DiffForEachHunkCallback, error) {
	return func(delta git2go.DiffDelta, _ float64) (git2go.DiffForEachHunkCallback, error) {
		finalizeFileStats(*currentFile, stats)

		filename := delta.NewFile.Path
		if filename == "" {
			filename = delta.OldFile.Path
		}

		lang := enry.GetLanguage(filepath.Base(filename), nil)
		if lang == "" {
			lang = "other"
		}

		*currentFile = &fileLineStats{lang: lang}

		return func(_ git2go.DiffHunk) (git2go.DiffForEachLineCallback, error) {
			return func(line git2go.DiffLine) error {
				processLineOrigin(line.Origin, *currentFile, stats)

				return nil
			}, nil
		}, nil
	}
}

// processLineOrigin updates stats based on the line origin type.
func processLineOrigin(origin git2go.DiffLineType, currentFile *fileLineStats, stats *DevTick) {
	switch origin {
	case git2go.DiffLineAddition:
		currentFile.added++
		stats.Added++
	case git2go.DiffLineDeletion:
		currentFile.removed++
		stats.Removed++
	case git2go.DiffLineContext, git2go.DiffLineContextEOFNL,
		git2go.DiffLineAddEOFNL, git2go.DiffLineDelEOFNL,
		git2go.DiffLineFileHdr, git2go.DiffLineHunkHdr, git2go.DiffLineBinary:
		// Ignore context lines, EOF markers, headers, etc.
	}
}

// finalizeFileStats adds current file stats to the total if there are any changes.
func finalizeFileStats(currentFile *fileLineStats, stats *DevTick) {
	if currentFile == nil || (currentFile.added == 0 && currentFile.removed == 0) {
		return
	}

	langStats := stats.Languages[currentFile.lang]
	langStats.Added += currentFile.added
	langStats.Removed += currentFile.removed
	stats.Languages[currentFile.lang] = langStats
}

// resultMerger holds state for merging worker results.
type resultMerger struct {
	finalTicks               map[int]map[int]*DevTick
	globalPeopleDict         map[string]int
	globalReversedPeopleDict []string
}

// newResultMerger creates a new result merger.
func newResultMerger() *resultMerger {
	return &resultMerger{
		finalTicks:               make(map[int]map[int]*DevTick),
		globalPeopleDict:         make(map[string]int),
		globalReversedPeopleDict: []string{},
	}
}

// remapAuthorID maps a worker-local author ID to a global ID.
func (rm *resultMerger) remapAuthorID(workerReversed []string, localID int) int {
	if localID >= len(workerReversed) {
		return localID
	}

	name := workerReversed[localID]
	keyName := strings.ToLower(name)

	if globalID, ok := rm.globalPeopleDict[keyName]; ok {
		return globalID
	}

	globalID := len(rm.globalReversedPeopleDict)
	rm.globalPeopleDict[keyName] = globalID
	rm.globalReversedPeopleDict = append(rm.globalReversedPeopleDict, name)

	return globalID
}

// mergeWorkerResult merges a single worker's results into the final ticks.
func (rm *resultMerger) mergeWorkerResult(workerResult libgit2WorkerResult) {
	for tick, devTicks := range workerResult.ticks {
		rm.ensureTickExists(tick)

		for localAuthorID, localStats := range devTicks {
			globalAuthorID := rm.remapAuthorID(workerResult.reversedPeopleDict, localAuthorID)
			rm.mergeDevTick(tick, globalAuthorID, localStats)
		}
	}
}

// ensureTickExists ensures the tick map exists.
func (rm *resultMerger) ensureTickExists(tick int) {
	if rm.finalTicks[tick] == nil {
		rm.finalTicks[tick] = make(map[int]*DevTick)
	}
}

// mergeDevTick merges developer tick stats.
func (rm *resultMerger) mergeDevTick(tick, authorID int, localStats *DevTick) {
	if rm.finalTicks[tick][authorID] == nil {
		rm.finalTicks[tick][authorID] = &DevTick{Languages: make(map[string]pkgplumbing.LineStats)}
	}

	finalStats := rm.finalTicks[tick][authorID]
	finalStats.Commits += localStats.Commits
	finalStats.Added += localStats.Added
	finalStats.Removed += localStats.Removed
	finalStats.Changed += localStats.Changed

	mergeLanguageStats(finalStats.Languages, localStats.Languages)
}

// mergeLanguageStats merges language-specific line stats.
func mergeLanguageStats(target, source map[string]pkgplumbing.LineStats) {
	for lang, langStats := range source {
		existing := target[lang]
		existing.Added += langStats.Added
		existing.Removed += langStats.Removed
		existing.Changed += langStats.Changed
		target[lang] = existing
	}
}

// mergeResults combines results from all workers.
func (fa *Libgit2Analyzer) mergeResults(results chan libgit2WorkerResult) (analyze.Report, error) {
	merger := newResultMerger()

	for workerResult := range results {
		if workerResult.err != nil {
			return nil, workerResult.err
		}

		merger.mergeWorkerResult(workerResult)
	}

	return analyze.Report{
		"Ticks":              merger.finalTicks,
		"ReversedPeopleDict": merger.globalReversedPeopleDict,
		"TickSize":           DefaultTickDuration,
	}, nil
}

// Serialize writes the analysis result to the given writer.
func (fa *Libgit2Analyzer) Serialize(result analyze.Report, _ bool, writer io.Writer) error {
	ticks, ok := result["Ticks"].(map[int]map[int]*DevTick)
	if !ok {
		return ErrInvalidTicks
	}

	reversedPeopleDict, ok := result["ReversedPeopleDict"].([]string)
	if !ok {
		return ErrInvalidPeopleDict
	}

	tickSize, ok := result["TickSize"].(time.Duration)
	if !ok {
		return ErrInvalidTickSize
	}

	fmt.Fprintln(writer, "  ticks:")
	serializeDevTicks(writer, ticks)

	fmt.Fprintln(writer, "  people:")

	for _, person := range reversedPeopleDict {
		fmt.Fprintf(writer, "  - %s\n", person)
	}

	fmt.Fprintln(writer, "  tick_size:", int(tickSize.Seconds()))

	return nil
}

// freeDiff is a helper to release diff resources in deferred calls.
// The error from Free() is checked but ignored as it's non-actionable in cleanup code.
func freeDiff(diff *git2go.Diff) {
	err := diff.Free()
	// Consume error to satisfy linter - Free() errors are non-actionable in cleanup.
	if err != nil {
		return
	}
}
