package burndown

import (
	"encoding/gob"
	"fmt"
	"os"
	"path/filepath"

	"github.com/Sumatoshi-tech/codefang/internal/burndown"
)

// shardSpillState tracks spill files for one shard.
type shardSpillState struct {
	dir        string
	fileSpillN int // file treap spill counter.
}

// spillDir returns the shard-specific spill directory, creating it on first call.
func (ss *shardSpillState) spillDir(parentDir string, shardIdx int) (string, error) {
	if ss.dir != "" {
		return ss.dir, nil
	}

	dir := filepath.Join(parentDir, fmt.Sprintf("shard_%03d", shardIdx))

	err := os.MkdirAll(dir, 0o700)
	if err != nil {
		return "", fmt.Errorf("create shard spill dir: %w", err)
	}

	ss.dir = dir

	return dir, nil
}

// mergeSparseHistory merges src into dst (additive).
func mergeSparseHistory(dst, src sparseHistory) {
	for tick, counts := range src {
		if dst[tick] == nil {
			dst[tick] = map[int]int64{}
		}

		for prevTick, count := range counts {
			dst[tick][prevTick] += count
		}
	}
}

// mergeMatrixInto merges src matrix rows into dst (additive), growing dst as needed.
func mergeMatrixInto(dst *[]map[int]int64, src []map[int]int64) {
	for author, row := range src {
		if len(row) == 0 {
			continue
		}

		for len(*dst) <= author {
			*dst = append(*dst, nil)
		}

		if (*dst)[author] == nil {
			(*dst)[author] = map[int]int64{}
		}

		for other, count := range row {
			(*dst)[author][other] += count
		}
	}
}

// mergePeopleHistories merges src per-person histories into dst (additive).
func mergePeopleHistories(dst, src map[int]sparseHistory) {
	for person, history := range src {
		if len(history) == 0 {
			continue
		}

		if dst[person] == nil {
			dst[person] = sparseHistory{}
		}

		mergeSparseHistory(dst[person], history)
	}
}

// cleanupShardSpills removes all spill files for a shard.
func cleanupShardSpills(ss *shardSpillState) {
	if ss.dir == "" {
		return
	}

	os.RemoveAll(ss.dir)
	ss.dir = ""
	ss.fileSpillN = 0
}

// fileEntry holds one file's serialized treap segments for spilling.
type fileEntry struct {
	PathID   PathID
	Segments []burndown.Segment
}

// fileHistoryEntry holds one file's sparse history for spilling.
type fileHistoryEntry struct {
	PathID  PathID
	History sparseHistory
}

// shardFilesSnapshot holds all active files' segments and histories for one shard.
// Each spill is a complete snapshot — only the latest is authoritative.
type shardFilesSnapshot struct {
	Files         []fileEntry
	FileHistories []fileHistoryEntry
}

// collectFileEntries extracts segments from active files and frees their treap nodes.
func collectFileEntries(shard *Shard) []fileEntry {
	var entries []fileEntry

	for _, id := range shard.activeIDs {
		if int(id) >= len(shard.filesByID) {
			continue
		}

		file := shard.filesByID[id]
		if file == nil {
			continue
		}

		entries = append(entries, fileEntry{
			PathID:   id,
			Segments: file.Segments(),
		})

		file.Delete()

		shard.filesByID[id] = nil
	}

	return entries
}

// collectFileHistoryEntries extracts file histories from active files and clears them.
func collectFileHistoryEntries(shard *Shard) []fileHistoryEntry {
	var entries []fileHistoryEntry

	for _, id := range shard.activeIDs {
		if int(id) >= len(shard.fileHistoriesByID) {
			continue
		}

		history := shard.fileHistoriesByID[id]
		if len(history) == 0 {
			continue
		}

		entries = append(entries, fileHistoryEntry{
			PathID:  id,
			History: history,
		})

		shard.fileHistoriesByID[id] = nil
	}

	return entries
}

// spillShardFiles serializes all active files' treap segments and file histories
// to a gob file, then frees the treap nodes to reclaim memory.
func spillShardFiles(shard *Shard, ss *shardSpillState, parentDir string, shardIdx int) error {
	snapshot := shardFilesSnapshot{
		Files:         collectFileEntries(shard),
		FileHistories: collectFileHistoryEntries(shard),
	}

	if len(snapshot.Files) == 0 && len(snapshot.FileHistories) == 0 {
		return nil
	}

	dir, err := ss.spillDir(parentDir, shardIdx)
	if err != nil {
		return err
	}

	path := filepath.Join(dir, fmt.Sprintf("files_%03d.gob", ss.fileSpillN))

	f, err := os.Create(path)
	if err != nil {
		return fmt.Errorf("create file spill: %w", err)
	}

	enc := gob.NewEncoder(f)

	err = enc.Encode(&snapshot)

	closeErr := f.Close()

	if err != nil {
		return fmt.Errorf("encode file snapshot: %w", err)
	}

	if closeErr != nil {
		return fmt.Errorf("close file spill: %w", closeErr)
	}

	ss.fileSpillN++

	return nil
}

// restoreFileEntries reconstructs file treaps from spilled entries.
func restoreFileEntries(shard *Shard, entries []fileEntry) {
	for _, entry := range entries {
		id := entry.PathID
		growShardSlices(shard, id)
		shard.filesByID[id] = burndown.NewFileFromSegments(entry.Segments)
	}
}

// restoreFileHistoryEntries restores file histories from spilled entries.
func restoreFileHistoryEntries(shard *Shard, entries []fileHistoryEntry) {
	for _, entry := range entries {
		id := entry.PathID
		growShardSlices(shard, id)
		shard.fileHistoriesByID[id] = entry.History
	}
}

// growShardSlices ensures shard slices are large enough to hold the given PathID.
func growShardSlices(shard *Shard, id PathID) {
	n := int(id) + 1

	if n > len(shard.filesByID) {
		newFiles := make([]*burndown.File, n)
		copy(newFiles, shard.filesByID)
		shard.filesByID = newFiles
	}

	if n > len(shard.fileHistoriesByID) {
		newHistories := make([]sparseHistory, n)
		copy(newHistories, shard.fileHistoriesByID)
		shard.fileHistoriesByID = newHistories
	}
}

// loadSpilledFiles restores file treaps and file histories from the most recent spill.
// Only the last spill is authoritative because each spill is a complete snapshot.
func loadSpilledFiles(shard *Shard, ss *shardSpillState) error {
	if ss.fileSpillN == 0 {
		return nil
	}

	// Only the last spill file matters — it's a complete snapshot.
	path := filepath.Join(ss.dir, fmt.Sprintf("files_%03d.gob", ss.fileSpillN-1))

	f, err := os.Open(path)
	if os.IsNotExist(err) {
		return nil // No files were spilled (all empty).
	}

	if err != nil {
		return fmt.Errorf("open file spill: %w", err)
	}

	defer f.Close()

	var snapshot shardFilesSnapshot

	err = gob.NewDecoder(f).Decode(&snapshot)
	if err != nil {
		return fmt.Errorf("decode file snapshot: %w", err)
	}

	restoreFileEntries(shard, snapshot.Files)
	restoreFileHistoryEntries(shard, snapshot.FileHistories)

	return nil
}
