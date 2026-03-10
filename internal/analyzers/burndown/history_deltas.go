package burndown

import (
	"github.com/Sumatoshi-tech/codefang/internal/burndown"
	"github.com/Sumatoshi-tech/codefang/internal/identity"
	"github.com/Sumatoshi-tech/codefang/pkg/alg/mapx"
)

// incrementSparseHistory adds delta to history[curTick][prevTick], lazily initializing nested maps.
func incrementSparseHistory(history sparseHistory, curTick, prevTick, delta int) {
	row := history[curTick]
	if row == nil {
		row = map[int]int64{}
		history[curTick] = row
	}

	row[prevTick] += int64(delta)
}

// mergeKeyedDeltas merges per-key sparse history from a shard into a lazily-initialized result map.
func mergeKeyedDeltas[K comparable](source, result map[K]sparseHistory) map[K]sparseHistory {
	for key, history := range source {
		if len(history) == 0 {
			continue
		}

		if result == nil {
			result = map[K]sparseHistory{}
		}

		if result[key] == nil {
			result[key] = sparseHistory{}
		}

		mapx.MergeNestedAdditive(result[key], history)
	}

	return result
}

// resetDeltaBuffers clears per-shard delta buffers before processing a new commit.
func (b *HistoryAnalyzer) resetDeltaBuffers() {
	for _, shard := range b.shards {
		shard.deltas.globalDeltas = sparseHistory{}

		if b.PeopleNumber > 0 {
			shard.deltas.peopleDeltas = map[int]sparseHistory{}
			shard.deltas.matrixDeltas = nil
		}

		if b.TrackFiles {
			shard.deltas.fileDeltas = map[PathID]sparseHistory{}
		}
	}
}

// collectDeltas merges delta buffers from all shards into a single CommitResult.
func (b *HistoryAnalyzer) collectDeltas() *CommitResult {
	result := &CommitResult{
		GlobalDeltas: sparseHistory{},
	}

	for _, shard := range b.shards {
		mapx.MergeNestedAdditive(result.GlobalDeltas, shard.deltas.globalDeltas)
		b.collectPeopleDeltas(result, shard)
		b.collectMatrixDeltas(result, shard)
		b.collectFileDeltas(result, shard)
	}

	if b.TrackFiles && b.PeopleNumber > 0 {
		result.FileOwnership = b.collectFileOwnership()
	}

	return result
}

// collectPeopleDeltas merges per-author sparse history from a shard into the result.
func (b *HistoryAnalyzer) collectPeopleDeltas(result *CommitResult, shard *Shard) {
	if b.PeopleNumber == 0 {
		return
	}

	result.PeopleDeltas = mergeKeyedDeltas(shard.deltas.peopleDeltas, result.PeopleDeltas)
}

// collectMatrixDeltas merges per-author matrix counters from a shard into the result.
func (b *HistoryAnalyzer) collectMatrixDeltas(result *CommitResult, shard *Shard) {
	if b.PeopleNumber == 0 {
		return
	}

	for author, row := range shard.deltas.matrixDeltas {
		if len(row) == 0 {
			continue
		}

		for len(result.MatrixDeltas) <= author {
			result.MatrixDeltas = append(result.MatrixDeltas, nil)
		}

		if result.MatrixDeltas[author] == nil {
			result.MatrixDeltas[author] = map[int]int64{}
		}

		for other, count := range row {
			result.MatrixDeltas[author][other] += count
		}
	}
}

// collectFileDeltas merges per-file sparse history from a shard into the result.
func (b *HistoryAnalyzer) collectFileDeltas(result *CommitResult, shard *Shard) {
	if !b.TrackFiles {
		return
	}

	result.FileDeltas = mergeKeyedDeltas(shard.deltas.fileDeltas, result.FileDeltas)
}

// collectFileOwnership extracts per-file author ownership from live file
// segments across all shards.
func (b *HistoryAnalyzer) collectFileOwnership() map[PathID]map[int]int {
	ownership := map[PathID]map[int]int{}

	for _, shard := range b.shards {
		b.collectShardFileOwnership(ownership, shard)
	}

	return ownership
}

// collectShardFileOwnership merges file ownership data from a single shard into the result.
func (b *HistoryAnalyzer) collectShardFileOwnership(ownership map[PathID]map[int]int, shard *Shard) {
	for pathID, file := range shard.filesByID {
		if file == nil {
			continue
		}

		pid := PathID(pathID)
		fileOwnership := extractFileOwnership(file, b.unpackPersonWithTick)

		mergeFileOwnership(ownership, pid, fileOwnership)
	}
}

// mergeFileOwnership adds per-author line counts into the ownership map for a given path.
func mergeFileOwnership(ownership map[PathID]map[int]int, pid PathID, fileOwnership map[int]int) {
	if len(fileOwnership) == 0 {
		return
	}

	if ownership[pid] == nil {
		ownership[pid] = fileOwnership

		return
	}

	for author, count := range fileOwnership {
		ownership[pid][author] += count
	}
}

// extractFileOwnership iterates a file's segments and sums line counts per author.
func extractFileOwnership(file *burndown.File, unpack func(int) (int, int)) map[int]int {
	result := map[int]int{}

	for _, seg := range file.Segments() {
		if seg.Value == burndown.TreeEnd {
			continue
		}

		author, _ := unpack(int(seg.Value))
		if author != identity.AuthorMissing {
			result[author] += seg.Length
		}
	}

	return result
}

// updateGlobal records a delta in the global sparse history.
func (b *HistoryAnalyzer) updateGlobal(shard *Shard, currentTime, previousTime, delta int) {
	_, curTick := b.unpackPersonWithTick(currentTime)
	_, prevTick := b.unpackPersonWithTick(previousTime)

	incrementSparseHistory(shard.deltas.globalDeltas, curTick, prevTick, delta)
}

// updateFile records a delta in a file's sparse history.
func (b *HistoryAnalyzer) updateFile(shard *Shard, pathID PathID, currentTime, previousTime, delta int) {
	_, curTick := b.unpackPersonWithTick(currentTime)
	_, prevTick := b.unpackPersonWithTick(previousTime)

	if shard.deltas.fileDeltas[pathID] == nil {
		shard.deltas.fileDeltas[pathID] = sparseHistory{}
	}

	incrementSparseHistory(shard.deltas.fileDeltas[pathID], curTick, prevTick, delta)
}

// updateAuthor records a delta in the per-author sparse history.
func (b *HistoryAnalyzer) updateAuthor(shard *Shard, currentTime, previousTime, delta int) {
	previousAuthor, prevTick := b.unpackPersonWithTick(previousTime)
	if previousAuthor == identity.AuthorMissing {
		return
	}

	_, curTick := b.unpackPersonWithTick(currentTime)

	if shard.deltas.peopleDeltas[previousAuthor] == nil {
		shard.deltas.peopleDeltas[previousAuthor] = sparseHistory{}
	}

	incrementSparseHistory(shard.deltas.peopleDeltas[previousAuthor], curTick, prevTick, delta)
}

// updateMatrix records a delta in the people interaction matrix.
func (b *HistoryAnalyzer) updateMatrix(shard *Shard, currentTime, previousTime, delta int) {
	newAuthor, _ := b.unpackPersonWithTick(currentTime)
	oldAuthor, _ := b.unpackPersonWithTick(previousTime)

	if oldAuthor == identity.AuthorMissing {
		return
	}

	if newAuthor == oldAuthor && delta > 0 {
		newAuthor = authorSelf
	}

	for len(shard.deltas.matrixDeltas) <= oldAuthor {
		shard.deltas.matrixDeltas = append(shard.deltas.matrixDeltas, nil)
	}

	row := shard.deltas.matrixDeltas[oldAuthor]
	if row == nil {
		row = map[int]int64{}
		shard.deltas.matrixDeltas[oldAuthor] = row
	}

	row[newAuthor] += int64(delta)
}

// createUpdaters builds the set of treap updater callbacks for a file.
func (b *HistoryAnalyzer) createUpdaters(shard *Shard, pathID PathID) []burndown.Updater {
	const maxUpdaters = 4

	updaters := make([]burndown.Updater, 0, maxUpdaters)

	updaters = append(updaters, func(currentTime, previousTime, delta int) {
		b.updateGlobal(shard, currentTime, previousTime, delta)
	})

	if b.TrackFiles {
		updaters = append(updaters, func(currentTime, previousTime, delta int) {
			b.updateFile(shard, pathID, currentTime, previousTime, delta)
		})
	}

	if b.PeopleNumber > 0 {
		updaters = append(updaters, func(currentTime, previousTime, delta int) {
			b.updateAuthor(shard, currentTime, previousTime, delta)
		}, func(currentTime, previousTime, delta int) {
			b.updateMatrix(shard, currentTime, previousTime, delta)
		})
	}

	return updaters
}

// newFile creates a new burndown file with the appropriate updaters and initial value.
func (b *HistoryAnalyzer) newFile(
	shard *Shard, pathID PathID, author int, tick int, size int,
) *burndown.File {
	updaters := b.createUpdaters(shard, pathID)

	if b.PeopleNumber > 0 {
		tick = b.packPersonWithTick(author, tick)
	}

	return burndown.NewFile(tick, size, updaters...)
}
