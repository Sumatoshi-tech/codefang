package filehistory

import (
	"context"
	"maps"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/internal/analyzers/common/spillstore"
	"github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

const (
	fileHistoryEntryBytes = 64
	hashEntryBytes        = 24
)

// CommitSummary holds per-commit summary data for timeseries output.
type CommitSummary struct {
	FilesTouched int `json:"files_touched"`
	LinesAdded   int `json:"lines_added"`
	LinesRemoved int `json:"lines_removed"`
	LinesChanged int `json:"lines_changed"`
	Inserts      int `json:"inserts"`
	Deletes      int `json:"deletes"`
	Modifies     int `json:"modifies"`
}

// Aggregator implements analyze.Aggregator for the file history analyzer.
// It accumulates file histories and line stats from the TC stream.
type Aggregator struct {
	files          *spillstore.SpillStore[FileHistory]
	lastCommitHash gitlib.Hash
	commitStats    map[string]*CommitSummary
	commitsByTick  map[int][]gitlib.Hash
	opts           analyze.AggregatorOptions
	closed         bool
}

// NewAggregator creates a new aggregator for the file history analyzer.
func NewAggregator(opts analyze.AggregatorOptions) *Aggregator {
	return &Aggregator{
		files:         spillstore.New[FileHistory](opts.SpillDir),
		commitStats:   make(map[string]*CommitSummary),
		commitsByTick: make(map[int][]gitlib.Hash),
		opts:          opts,
	}
}

// Add ingests a single per-commit TC into the aggregator.
func (a *Aggregator) Add(tc analyze.TC) error {
	cd, ok := tc.Data.(*CommitData)
	if !ok || cd == nil {
		return nil
	}

	if !tc.CommitHash.IsZero() {
		a.lastCommitHash = tc.CommitHash
	}

	a.applyPathActions(cd.PathActions)
	a.applyLineStatUpdates(cd.LineStatUpdates)

	if !tc.CommitHash.IsZero() {
		a.trackCommitStats(tc, cd)
	}

	if a.opts.SpillBudget > 0 && a.EstimatedStateSize() > a.opts.SpillBudget {
		_, err := a.Spill()
		if err != nil {
			return err
		}
	}

	return nil
}

func (a *Aggregator) trackCommitStats(tc analyze.TC, cd *CommitData) {
	summary := &CommitSummary{
		FilesTouched: len(cd.PathActions),
	}

	for _, pa := range cd.PathActions {
		switch pa.Action {
		case gitlib.Insert:
			summary.Inserts++
		case gitlib.Delete:
			summary.Deletes++
		case gitlib.Modify:
			summary.Modifies++
		default:
			summary.Modifies++
		}
	}

	for _, u := range cd.LineStatUpdates {
		summary.LinesAdded += u.Stats.Added
		summary.LinesRemoved += u.Stats.Removed
		summary.LinesChanged += u.Stats.Changed
	}

	hashStr := tc.CommitHash.String()
	a.commitStats[hashStr] = summary
	a.commitsByTick[tc.Tick] = append(a.commitsByTick[tc.Tick], tc.CommitHash)
}

func (a *Aggregator) applyPathActions(actions []PathAction) {
	for _, pa := range actions {
		switch pa.Action {
		case gitlib.Insert:
			a.applyInsert(pa.Path, pa.CommitHash)
		case gitlib.Modify:
			a.applyModify(pa)
		case gitlib.Delete:
			a.applyDelete(pa.Path, pa.CommitHash)
		}
	}
}

func (a *Aggregator) applyInsert(path string, hash gitlib.Hash) {
	fh := a.getOrCreate(path)

	fh.Hashes = []gitlib.Hash{hash}

	if fh.People == nil {
		fh.People = make(map[int]plumbing.LineStats)
	}

	a.files.Put(path, *fh)
}

func (a *Aggregator) applyModify(pa PathAction) {
	if pa.FromPath != "" && pa.ToPath != "" {
		a.applyRename(pa.FromPath, pa.ToPath, pa.CommitHash)

		return
	}

	if pa.Path == "" {
		return
	}

	fh := a.getOrCreate(pa.Path)

	fh.Hashes = append(fh.Hashes, pa.CommitHash)

	if fh.People == nil {
		fh.People = make(map[int]plumbing.LineStats)
	}

	a.files.Put(pa.Path, *fh)
}

func (a *Aggregator) applyDelete(path string, hash gitlib.Hash) {
	fh := a.getOrCreate(path)

	fh.Hashes = append(fh.Hashes, hash)

	if fh.People == nil {
		fh.People = make(map[int]plumbing.LineStats)
	}

	a.files.Put(path, *fh)
}

func (a *Aggregator) applyRename(fromPath, toPath string, commitHash gitlib.Hash) {
	fh, ok := a.files.Get(fromPath)
	if !ok {
		fh = FileHistory{
			People: make(map[int]plumbing.LineStats),
		}
	}

	fh.Hashes = append(fh.Hashes, commitHash)
	if fh.People == nil {
		fh.People = make(map[int]plumbing.LineStats)
	}

	a.files.Put(toPath, fh)
}

func (a *Aggregator) getOrCreate(path string) *FileHistory {
	fh, ok := a.files.Get(path)
	if !ok {
		fh = FileHistory{
			People: make(map[int]plumbing.LineStats),
		}
	}

	return &fh
}

func (a *Aggregator) applyLineStatUpdates(updates []LineStatUpdate) {
	for _, u := range updates {
		fh := a.getOrCreate(u.Path)
		oldStats := fh.People[u.AuthorID]
		fh.People[u.AuthorID] = plumbing.LineStats{
			Added:   oldStats.Added + u.Stats.Added,
			Removed: oldStats.Removed + u.Stats.Removed,
			Changed: oldStats.Changed + u.Stats.Changed,
		}
		a.files.Put(u.Path, *fh)
	}
}

// FlushTick returns the aggregated TICK for the given tick index.
// File history uses a single cumulative tick (0).
func (a *Aggregator) FlushTick(tick int) (analyze.TICK, error) {
	if a.files.Len() == 0 {
		return analyze.TICK{Tick: tick, Data: &TickData{Files: map[string]FileHistory{}, LastCommitHash: a.lastCommitHash}}, nil
	}

	files := make(map[string]FileHistory)

	maps.Copy(files, a.files.Current())

	return analyze.TICK{
		Tick: tick,
		Data: &TickData{
			Files:          files,
			LastCommitHash: a.lastCommitHash,
			CommitStats:    a.commitStats,
			CommitsByTick:  a.commitsByTick,
		},
	}, nil
}

// FlushAllTicks returns a single TICK containing all accumulated file history.
func (a *Aggregator) FlushAllTicks() ([]analyze.TICK, error) {
	t, err := a.FlushTick(0)
	if err != nil {
		return nil, err
	}

	if td, ok := t.Data.(*TickData); ok && len(td.Files) == 0 && td.LastCommitHash.IsZero() {
		return nil, nil
	}

	return []analyze.TICK{t}, nil
}

// DiscardState clears all in-memory cumulative state without serialization.
func (a *Aggregator) DiscardState() {
	a.files = spillstore.New[FileHistory](a.opts.SpillDir)
}

// Spill writes accumulated state to disk to free memory.
func (a *Aggregator) Spill() (int64, error) {
	if a.files.Len() == 0 {
		return 0, nil
	}

	sizeBefore := a.EstimatedStateSize()

	err := a.files.Spill()
	if err != nil {
		return 0, err
	}

	return sizeBefore, nil
}

// Collect reloads spilled state back into memory.
func (a *Aggregator) Collect() error {
	collected, err := a.files.CollectWith(mergeFileHistory)
	if err != nil {
		return err
	}

	for k, v := range collected {
		a.files.Put(k, v)
	}

	return nil
}

func mergeFileHistory(existing, incoming FileHistory) FileHistory {
	if existing.People == nil {
		existing.People = make(map[int]plumbing.LineStats)
	}

	for author, stats := range incoming.People {
		old := existing.People[author]
		existing.People[author] = plumbing.LineStats{
			Added:   old.Added + stats.Added,
			Removed: old.Removed + stats.Removed,
			Changed: old.Changed + stats.Changed,
		}
	}

	existing.Hashes = append(existing.Hashes, incoming.Hashes...)

	return existing
}

// EstimatedStateSize returns the current in-memory footprint in bytes.
func (a *Aggregator) EstimatedStateSize() int64 {
	var size int64

	for _, fh := range a.files.Current() {
		size += fileHistoryEntryBytes
		size += int64(len(fh.Hashes)) * hashEntryBytes

		for _, stats := range fh.People {
			_ = stats
			size += 32
		}
	}

	return size
}

// SpillState returns the current on-disk spill state for checkpoint persistence.
func (a *Aggregator) SpillState() analyze.AggregatorSpillInfo {
	return analyze.AggregatorSpillInfo{
		Dir:   a.files.SpillDir(),
		Count: a.files.SpillCount(),
	}
}

// RestoreSpillState points the aggregator at a previously-saved spill directory.
func (a *Aggregator) RestoreSpillState(info analyze.AggregatorSpillInfo) {
	a.files.RestoreFromDir(info.Dir, info.Count)
}

// DrainCommitStats implements analyze.CommitStatsDrainer.
// It extracts and clears per-commit data, returning the same shape as ExtractCommitTimeSeries.
func (a *Aggregator) DrainCommitStats() (stats map[string]any, tickHashes map[int][]gitlib.Hash) {
	if len(a.commitStats) == 0 {
		return nil, nil
	}

	result := make(map[string]any, len(a.commitStats))
	for hash, cs := range a.commitStats {
		result[hash] = map[string]any{
			"files_touched": cs.FilesTouched,
			"lines_added":   cs.LinesAdded,
			"lines_removed": cs.LinesRemoved,
			"lines_changed": cs.LinesChanged,
			"inserts":       cs.Inserts,
			"deletes":       cs.Deletes,
			"modifies":      cs.Modifies,
		}
	}

	cbt := a.commitsByTick
	a.commitStats = make(map[string]*CommitSummary)
	a.commitsByTick = make(map[int][]gitlib.Hash)

	return result, cbt
}

// Close releases all resources. Idempotent.
func (a *Aggregator) Close() error {
	if a.closed {
		return nil
	}

	a.closed = true
	a.files.Cleanup()

	return nil
}

// TickData is the aggregated payload stored in analyze.TICK.Data for file history.
type TickData struct {
	Files          map[string]FileHistory
	LastCommitHash gitlib.Hash
	CommitStats    map[string]*CommitSummary
	CommitsByTick  map[int][]gitlib.Hash
}

// TicksToReport builds the analyze.Report from TICKs.
// Requires repo for filtering by last commit's file tree.
func TicksToReport(ctx context.Context, ticks []analyze.TICK, repo *gitlib.Repository) analyze.Report {
	files := mergeTicksIntoFiles(ticks)
	commitStats, commitsByTick := mergeTickCommitData(ticks)

	lastCommitHash := extractLastCommitHash(ticks)

	var report analyze.Report
	if lastCommitHash.IsZero() || repo == nil {
		report = analyze.Report{"Files": files}
	} else {
		filtered := filterFilesByLastCommit(ctx, repo, lastCommitHash, files)
		report = analyze.Report{"Files": filtered}
	}

	if len(commitStats) > 0 {
		report["commit_stats"] = commitStats
		report["commits_by_tick"] = commitsByTick
	}

	return report
}

func mergeTickCommitData(ticks []analyze.TICK) (commitStats map[string]*CommitSummary, commitsByTick map[int][]gitlib.Hash) {
	commitStats = make(map[string]*CommitSummary)
	commitsByTick = make(map[int][]gitlib.Hash)

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil {
			continue
		}

		maps.Copy(commitStats, td.CommitStats)

		for t, hashes := range td.CommitsByTick {
			commitsByTick[t] = append(commitsByTick[t], hashes...)
		}
	}

	return commitStats, commitsByTick
}

func mergeTicksIntoFiles(ticks []analyze.TICK) map[string]FileHistory {
	files := make(map[string]FileHistory)

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil {
			continue
		}

		for path, fh := range td.Files {
			existing := files[path]
			files[path] = mergeFileHistory(existing, fh)
		}
	}

	return files
}

func extractLastCommitHash(ticks []analyze.TICK) gitlib.Hash {
	var last gitlib.Hash

	for _, tick := range ticks {
		td, ok := tick.Data.(*TickData)
		if !ok || td == nil {
			continue
		}

		if !td.LastCommitHash.IsZero() {
			last = td.LastCommitHash
		}
	}

	return last
}

func filterFilesByLastCommit(
	ctx context.Context,
	repo *gitlib.Repository,
	hash gitlib.Hash,
	files map[string]FileHistory,
) map[string]FileHistory {
	lastCommit, err := repo.LookupCommit(ctx, hash)
	if err != nil {
		return files
	}

	fileIter, err := lastCommit.FilesContext(ctx)
	if err != nil {
		return files
	}

	filtered := make(map[string]FileHistory)
	processed := 0

	err = fileIter.ForEach(func(file *gitlib.File) error {
		if fh, ok := files[file.Name]; ok {
			filtered[file.Name] = fh
		}

		processed++
		if processed%1000 == 0 {
			gitlib.ReleaseNativeMemory()
		}

		return nil
	})
	if err != nil {
		return files
	}

	return filtered
}
