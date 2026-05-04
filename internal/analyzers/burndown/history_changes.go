package burndown

import (
	"errors"
	"fmt"
	"log"
	"unicode/utf8"

	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/internal/burndown"
	pkgplumbing "github.com/Sumatoshi-tech/codefang/internal/plumbing"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// blobValidationResult classifies the outcome of validating both blobs for a modification.
type blobValidationResult int

const (
	blobsBothValid  blobValidationResult = iota // Both blobs are text -- proceed with diff.
	blobsBothBinary                             // Both are binary -- skip silently.
	blobsFromBinary                             // From is binary, To is text -- treat as insertion.
	blobsToBinary                               // To is binary, From is text -- treat as deletion.
)

// handleInsertion processes a newly added file.
func (b *HistoryAnalyzer) handleInsertion(
	shard *Shard, change *gitlib.Change, author int, cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
) error {
	blob := cache[change.To.Hash]
	if blob == nil {
		return fmt.Errorf("%w for insertion %s (%s)", errMissingBlob, change.To.Name, change.To.Hash)
	}

	lines, err := blob.CountLines()
	if err != nil {
		return nil
	}

	name := change.To.Name
	id := b.pathInterner.Intern(name)
	b.ensureCapacity(shard, id)

	if shard.filesByID[id] != nil {
		b.clearStaleEntry(shard, id, name)
	}

	file := b.newFile(shard, id, author, b.tick, lines)
	shard.filesByID[id] = file
	shard.activeIDs = append(shard.activeIDs, id)

	delete(shard.deletionsByID, id)

	if b.isMerge {
		shard.mergedByID[id] = true
	}

	return nil
}

// clearStaleEntry removes a stale file entry left from a skipped commit.
func (b *HistoryAnalyzer) clearStaleEntry(shard *Shard, id PathID, name string) {
	log.Printf("burndown: insert collision for %s, resetting stale entry", name)

	shard.filesByID[id] = nil
	b.removeActiveID(shard, id)
}

// handleDeletion processes a deleted file.
func (b *HistoryAnalyzer) handleDeletion(
	shard *Shard, change *gitlib.Change, author int, cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
) error {
	name := deletionName(change)
	id := b.pathInterner.Intern(name)
	b.ensureCapacity(shard, id)

	file := shard.filesByID[id]
	if file == nil {
		return nil
	}

	lines, err := b.countDeletionLines(cache, change, name)
	if err != nil {
		return err
	}

	if file.Len() != lines {
		b.forceRemoveFile(shard, id, name, file)

		return nil
	}

	b.applyDeletionUpdate(shard, file, id, author)
	b.removeFileTracking(shard, id)

	b.GlobalMu.Lock()
	b.clearRenameCascade(name)
	b.GlobalMu.Unlock()

	if b.isMerge {
		shard.mergedByID[id] = false
	}

	return nil
}

// deletionName resolves the file name for a deletion change.
func deletionName(change *gitlib.Change) string {
	if change.To.Hash != gitlib.ZeroHash() {
		return change.To.Name
	}

	return change.From.Name
}

// countDeletionLines retrieves and counts lines from the pre-image blob.
func (b *HistoryAnalyzer) countDeletionLines(
	cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
	change *gitlib.Change, name string,
) (int, error) {
	blob := cache[change.From.Hash]
	if blob == nil {
		return 0, fmt.Errorf("%w for deletion %s (%s)", errMissingBlob, name, change.From.Hash)
	}

	lines, err := blob.CountLines()
	if err != nil {
		return 0, fmt.Errorf("%w: %s", errUnexpectedBinary, name)
	}

	return lines, nil
}

// forceRemoveFile handles treap/blob length mismatch by force-deleting the file tracking.
func (b *HistoryAnalyzer) forceRemoveFile(shard *Shard, id PathID, name string, file *burndown.File) {
	b.mismatch.recordForceRemove(name, file.Len())
	file.Delete()

	shard.filesByID[id] = nil
	shard.fileHistoriesByID[id] = nil
	b.removeActiveID(shard, id)
}

// applyDeletionUpdate performs the treap update that zeroes out a file's lines.
func (b *HistoryAnalyzer) applyDeletionUpdate(shard *Shard, file *burndown.File, id PathID, author int) {
	tick := b.tick

	isDeletion := shard.deletionsByID[id]
	shard.deletionsByID[id] = true

	if b.isMerge && !isDeletion {
		tick = 0
	}

	file.Update(b.packPersonWithTick(author, tick), 0, 0, file.Len())
	file.Delete()
}

// removeFileTracking clears file and history references from a shard.
func (b *HistoryAnalyzer) removeFileTracking(shard *Shard, id PathID) {
	shard.filesByID[id] = nil
	shard.fileHistoriesByID[id] = nil
	b.removeActiveID(shard, id)
}

// clearRenameCascade removes a file and all its transitive rename ancestors
// from the rename tracking maps. Must be called with GlobalMu held.
func (b *HistoryAnalyzer) clearRenameCascade(name string) {
	stack := []string{name}

	for len(stack) > 0 {
		head := stack[len(stack)-1]
		stack = stack[:len(stack)-1]

		b.unlinkRenameEntry(head)

		for child := range b.renamesReverse[head] {
			stack = append(stack, child)
			b.renames[child] = ""
		}

		delete(b.renamesReverse, head)
	}
}

// unlinkRenameEntry removes a single entry from the rename forward/reverse maps.
func (b *HistoryAnalyzer) unlinkRenameEntry(name string) {
	oldTo := b.renames[name]
	if oldTo != "" {
		delete(b.renamesReverse[oldTo], name)

		if len(b.renamesReverse[oldTo]) == 0 {
			delete(b.renamesReverse, oldTo)
		}
	}

	b.renames[name] = ""
}

// handleModification processes an in-place file modification (no rename).
func (b *HistoryAnalyzer) handleModification(
	shard *Shard, change *gitlib.Change, author int,
	cache map[gitlib.Hash]*pkgplumbing.CachedBlob, diffs map[string]pkgplumbing.FileDiffData,
) error {
	id := b.pathInterner.Intern(change.From.Name)
	b.ensureCapacity(shard, id)

	if b.isMerge {
		shard.mergedByID[id] = true
	}

	file := shard.filesByID[id]
	if file == nil {
		return b.handleInsertion(shard, change, author, cache)
	}

	action, err := validateModificationBlobs(cache, change)
	if err != nil {
		return err
	}

	if action != blobsBothValid {
		return b.dispatchBlobAction(shard, change, author, action, cache)
	}

	id = b.pathInterner.Intern(change.To.Name)

	return b.applyModificationDiffs(shard, change, file, id, author, cache, diffs)
}

// handleModificationRename processes a file modification with rename (From != To).
func (b *HistoryAnalyzer) handleModificationRename(
	change *gitlib.Change, author int,
	cache map[gitlib.Hash]*pkgplumbing.CachedBlob, diffs map[string]pkgplumbing.FileDiffData,
) error {
	shardFrom := b.getShard(change.From.Name)
	fromID := b.pathInterner.Intern(change.From.Name)
	b.ensureCapacity(shardFrom, fromID)

	file := shardFrom.filesByID[fromID]
	if file == nil {
		shardTo := b.getShard(change.To.Name)

		return b.handleInsertion(shardTo, change, author, cache)
	}

	file = b.applyRenameIfNeeded(change, file)

	action, err := validateModificationBlobs(cache, change)
	if err != nil {
		return err
	}

	if action != blobsBothValid {
		shardTo := b.getShard(change.To.Name)

		return b.dispatchBlobAction(shardTo, change, author, action, cache)
	}

	shardTo := b.getShard(change.To.Name)
	toID := b.pathInterner.Intern(change.To.Name)

	return b.applyModificationDiffs(shardTo, change, file, toID, author, cache, diffs)
}

// applyRenameIfNeeded performs the rename step if From.Name != To.Name and returns
// the file reference in its new shard location.
func (b *HistoryAnalyzer) applyRenameIfNeeded(change *gitlib.Change, file *burndown.File) *burndown.File {
	if change.To.Name == change.From.Name {
		return file
	}

	err := b.handleRename(change.From.Name, change.To.Name)
	if err != nil {
		return file
	}

	shardTo := b.getShard(change.To.Name)
	toID := b.pathInterner.Intern(change.To.Name)
	b.ensureCapacity(shardTo, toID)

	return shardTo.filesByID[toID]
}

// validateModificationBlobs checks both blobs for a modification and classifies the result.
func validateModificationBlobs(
	cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
	change *gitlib.Change,
) (blobValidationResult, error) {
	blobFrom := cache[change.From.Hash]
	if blobFrom == nil {
		return 0, fmt.Errorf("%w: blobFrom for %s (%s)", errMissingBlob, change.From.Name, change.From.Hash)
	}

	_, errFrom := blobFrom.CountLines()

	blobTo := cache[change.To.Hash]
	if blobTo == nil {
		return 0, fmt.Errorf("%w: blobTo for %s (%s)", errMissingBlob, change.To.Name, change.To.Hash)
	}

	_, errTo := blobTo.CountLines()

	return classifyBlobErrors(errFrom, errTo), nil
}

// classifyBlobErrors maps CountLines error states to the appropriate action.
func classifyBlobErrors(errFrom, errTo error) blobValidationResult {
	if !errors.Is(errFrom, errTo) {
		if errFrom != nil {
			return blobsFromBinary
		}

		return blobsToBinary
	}

	if errFrom != nil {
		return blobsBothBinary
	}

	return blobsBothValid
}

// dispatchBlobAction handles the non-both-valid blob validation cases.
func (b *HistoryAnalyzer) dispatchBlobAction(
	shard *Shard, change *gitlib.Change, author int,
	action blobValidationResult,
	cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
) error {
	switch action {
	case blobsFromBinary:
		return b.handleInsertion(shard, change, author, cache)
	case blobsToBinary:
		return b.handleDeletion(shard, change, author, cache)
	case blobsBothBinary, blobsBothValid:
		return nil
	}

	return nil
}

// applyModificationDiffs applies diffs to a file after successful blob validation.
func (b *HistoryAnalyzer) applyModificationDiffs(
	shard *Shard, change *gitlib.Change, file *burndown.File, id PathID,
	author int, cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
	diffs map[string]pkgplumbing.FileDiffData,
) error {
	thisDiffs := diffs[change.To.Name]

	if file.Len() != thisDiffs.OldLinesOfCode {
		return b.resetAndReinsert(shard, change, id, author, cache)
	}

	b.applyDiffs(file, thisDiffs, author)

	return nil
}

// resetAndReinsert handles src mismatch by clearing the stale file and re-inserting.
func (b *HistoryAnalyzer) resetAndReinsert(
	shard *Shard, change *gitlib.Change, id PathID, author int,
	cache map[gitlib.Hash]*pkgplumbing.CachedBlob,
) error {
	b.mismatch.recordReset(change.To.Name, shard.filesByID[id].Len())

	shard.filesByID[id] = nil
	b.removeActiveID(shard, id)

	return b.handleInsertion(shard, change, author, cache)
}

// handleRename moves a file from one path to another, handling cross-shard migration.
func (b *HistoryAnalyzer) handleRename(from, to string) error {
	if from == to {
		return nil
	}

	shardFrom := b.getShard(from)
	fromID := b.pathInterner.Intern(from)
	toID := b.pathInterner.Intern(to)
	b.ensureCapacity(shardFrom, fromID)

	file := shardFrom.filesByID[fromID]
	if file == nil {
		return fmt.Errorf("%w: %s > %s", errFileNotExist, from, to)
	}

	shardTo := b.getShard(to)
	b.ensureCapacity(shardTo, toID)

	b.moveFile(shardFrom, shardTo, fromID, toID, file)

	if b.TrackFiles {
		b.migrateFileHistory(shardFrom, shardTo, fromID, toID)
	}

	delete(shardTo.deletionsByID, toID)
	b.recordRename(from, to)

	return nil
}

// moveFile transfers a file between shards, deep-cloning if cross-shard.
func (b *HistoryAnalyzer) moveFile(shardFrom, shardTo *Shard, fromID, toID PathID, file *burndown.File) {
	if shardFrom == shardTo {
		shardFrom.filesByID[fromID] = nil
		b.removeActiveID(shardFrom, fromID)
		shardFrom.filesByID[toID] = file
		shardFrom.activeIDs = append(shardFrom.activeIDs, toID)

		return
	}

	newFile := file.CloneDeep()
	newFile.ReplaceUpdaters(b.createUpdaters(shardTo, toID))

	shardTo.filesByID[toID] = newFile
	shardTo.activeIDs = append(shardTo.activeIDs, toID)

	file.Delete()

	shardFrom.filesByID[fromID] = nil
	b.removeActiveID(shardFrom, fromID)
}

// migrateFileHistory moves file history from one shard slot to another.
func (b *HistoryAnalyzer) migrateFileHistory(shardFrom, shardTo *Shard, fromID, toID PathID) {
	b.ensureCapacity(shardFrom, fromID)
	b.ensureCapacity(shardTo, toID)

	history := shardFrom.fileHistoriesByID[fromID]
	if history == nil {
		history = sparseHistory{}
	}

	shardFrom.fileHistoriesByID[fromID] = nil
	shardTo.fileHistoriesByID[toID] = history
}

// recordRename updates the rename tracking maps under GlobalMu.
func (b *HistoryAnalyzer) recordRename(from, to string) {
	b.GlobalMu.Lock()
	defer b.GlobalMu.Unlock()

	b.unlinkRenameEntry(from)

	b.renames[from] = to

	if b.renamesReverse[to] == nil {
		b.renamesReverse[to] = map[string]bool{}
	}

	b.renamesReverse[to][from] = true
}

// diffApplier holds state for applying a sequence of diffs to a burndown file.
type diffApplier struct {
	b        *HistoryAnalyzer
	file     *burndown.File
	author   int
	position int
	pending  diffmatchpatch.Diff
}

func (d *diffApplier) packValue() int {
	return d.b.packPersonWithTick(d.author, d.b.tick)
}

func (d *diffApplier) applySingle(edit diffmatchpatch.Diff) {
	length := utf8.RuneCountInString(edit.Text)
	if edit.Type == diffmatchpatch.DiffInsert {
		d.file.Update(d.packValue(), d.position, length, 0)
		d.position += length
	} else {
		d.file.Update(d.packValue(), d.position, 0, length)
	}

	if d.b.Debug {
		d.file.Validate()
	}
}

func (d *diffApplier) flushPending() {
	if d.pending.Text != "" {
		d.applySingle(d.pending)
		d.pending.Text = ""
	}
}

func (d *diffApplier) handleInsert(edit diffmatchpatch.Diff) {
	length := utf8.RuneCountInString(edit.Text)

	if d.pending.Text != "" {
		d.file.Update(d.packValue(), d.position, length, utf8.RuneCountInString(d.pending.Text))

		if d.b.Debug {
			d.file.Validate()
		}

		d.position += length
		d.pending.Text = ""
	} else {
		d.pending = edit
	}
}

func (b *HistoryAnalyzer) applyDiffs(
	file *burndown.File, thisDiffs pkgplumbing.FileDiffData, author int,
) {
	da := &diffApplier{b: b, file: file, author: author, pending: diffmatchpatch.Diff{Text: ""}}

	for _, edit := range thisDiffs.Diffs {
		switch edit.Type {
		case diffmatchpatch.DiffEqual:
			da.flushPending()
			da.position += utf8.RuneCountInString(edit.Text)
		case diffmatchpatch.DiffInsert:
			da.handleInsert(edit)
		case diffmatchpatch.DiffDelete:
			da.pending = edit
		}
	}

	da.flushPending()
}
