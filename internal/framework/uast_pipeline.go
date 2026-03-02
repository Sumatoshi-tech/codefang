package framework

import (
	"bufio"
	"context"
	"encoding/gob"
	"fmt"
	"log"
	"os"
	"sync"

	"github.com/Sumatoshi-tech/codefang/internal/analyzers/analyze"
	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/uast"
	"github.com/Sumatoshi-tech/codefang/pkg/uast/pkg/node"
)

// UASTPipeline pre-computes UAST changes for each commit in the pipeline,
// enabling cross-commit parallelism. It sits between DiffPipeline and the
// serial analyzer consumption loop.
type UASTPipeline struct {
	Parser     *uast.Parser
	Workers    int
	BufferSize int
}

// NewUASTPipeline creates a new UAST pipeline stage.
func NewUASTPipeline(parser *uast.Parser, workers, bufferSize int) *UASTPipeline {
	if workers <= 0 {
		workers = 1
	}

	if bufferSize <= 0 {
		bufferSize = 1
	}

	return &UASTPipeline{
		Parser:     parser,
		Workers:    workers,
		BufferSize: bufferSize,
	}
}

// uastSlot holds a commit being processed. The done channel is closed when
// processing is complete, allowing the emitter to wait without spinning.
type uastSlot struct {
	data CommitData
	done chan struct{}
}

// Process receives commit data with blobs and diffs, and adds pre-computed
// UAST changes. Multiple commits are processed concurrently by worker goroutines.
// Output order matches input order via a slot-based approach.
func (p *UASTPipeline) Process(ctx context.Context, diffs <-chan CommitData) <-chan CommitData {
	out := make(chan CommitData, p.BufferSize)
	slots := make(chan *uastSlot, p.BufferSize)
	jobs := make(chan *uastSlot, p.BufferSize)

	go p.dispatch(ctx, diffs, slots, jobs)

	wg := p.startWorkers(ctx, jobs)

	go p.emit(ctx, slots, out, wg)

	return out
}

// dispatch reads from diffs, creates slots, and sends them to the ordered queue and worker pool.
func (p *UASTPipeline) dispatch(ctx context.Context, diffs <-chan CommitData, slots, jobs chan<- *uastSlot) {
	defer close(slots)
	defer close(jobs)

	for data := range diffs {
		select {
		case <-ctx.Done():
			return
		default:
		}

		slot := &uastSlot{data: data, done: make(chan struct{})}

		if data.Error != nil {
			close(slot.done)
		}

		select {
		case slots <- slot:
		case <-ctx.Done():
			return
		}

		if data.Error == nil {
			select {
			case jobs <- slot:
			case <-ctx.Done():
				return
			}
		}
	}
}

// startWorkers launches worker goroutines that parse UAST for each commit.
func (p *UASTPipeline) startWorkers(ctx context.Context, jobs <-chan *uastSlot) *sync.WaitGroup {
	var wg sync.WaitGroup

	wg.Add(p.Workers)

	for range p.Workers {
		go func() {
			defer wg.Done()

			for slot := range jobs {
				if len(slot.data.Changes) > uastSpillThreshold {
					path, err := p.parseCommitAndSpill(ctx, slot.data.Changes, slot.data.BlobCache)
					if err != nil {
						log.Printf("UAST spill error: %v", err)
						// Fallback to in-memory path on spill failure.
						slot.data.UASTChanges = p.parseCommitChanges(ctx, slot.data.Changes, slot.data.BlobCache)
					} else {
						slot.data.UASTSpillPath = path
					}
				} else {
					slot.data.UASTChanges = p.parseCommitChanges(ctx, slot.data.Changes, slot.data.BlobCache)
				}

				uast.MallocTrim()
				close(slot.done)
			}
		}()
	}

	return &wg
}

// emit sends results in order by waiting on each slot's done channel.
func (p *UASTPipeline) emit(ctx context.Context, slots <-chan *uastSlot, out chan<- CommitData, wg *sync.WaitGroup) {
	defer close(out)

	for slot := range slots {
		select {
		case <-slot.done:
		case <-ctx.Done():
			return
		}

		select {
		case out <- slot.data:
		case <-ctx.Done():
			return
		}
	}

	wg.Wait()
}

// uastSpillThreshold is the number of file changes above which the UAST pipeline
// spills parsed node trees to disk instead of accumulating them in memory.
// This bounds peak native memory for monster commits (vendor updates, large PRs)
// to one tree-sitter C allocation per worker, rather than hundreds.
const uastSpillThreshold = 32

// uastSpillTrimInterval is how often to call MallocTrim during spill-mode parsing.
// Every N files, reclaim C arena pages to prevent fragmentation buildup.
const uastSpillTrimInterval = 16

// intraCommitParallelThreshold is the minimum number of file changes in a commit
// before intra-commit parallelism is used. Below this, sequential parsing is faster.
const intraCommitParallelThreshold = 4

// maxUASTBlobSize is the maximum blob size (in bytes) for UAST parsing.
// Files larger than this are skipped — they are typically generated code
// (protobuf, deepcopy, mock) where structural coupling analysis produces
// noise, and their tree-sitter parse trees consume hundreds of MB of CGO
// memory that the Go GC cannot track or reclaim.
const maxUASTBlobSize = 256 * 1024 // 256 KiB.

// parseCommitChanges parses UAST for all file changes in a commit.
// For commits with many files, parsing is done in parallel across files.
func (p *UASTPipeline) parseCommitChanges(
	ctx context.Context,
	changes gitlib.Changes,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) []uast.Change {
	if len(changes) == 0 {
		return nil
	}

	if len(changes) <= intraCommitParallelThreshold {
		return p.parseCommitSequential(ctx, changes, cache)
	}

	return p.parseCommitParallel(ctx, changes, cache)
}

// parseCommitSequential parses files one at a time within a commit.
func (p *UASTPipeline) parseCommitSequential(
	ctx context.Context,
	changes gitlib.Changes,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) []uast.Change {
	var result []uast.Change

	for _, change := range changes {
		before := p.parseBlob(ctx, change.From.Hash, change.From.Name, cache, change.Action, true)
		after := p.parseBlob(ctx, change.To.Hash, change.To.Name, cache, change.Action, false)

		if before != nil || after != nil {
			result = append(result, uast.Change{
				Before: before,
				After:  after,
				Change: change,
			})
		}
	}

	return result
}

// uastFileResult holds the result of parsing a single file change.
type uastFileResult struct {
	before *node.Node
	after  *node.Node
	change *gitlib.Change
}

// parseCommitParallel parses files in parallel within a single commit.
// Uses a bounded goroutine pool to avoid excessive concurrency.
func (p *UASTPipeline) parseCommitParallel(
	ctx context.Context,
	changes gitlib.Changes,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) []uast.Change {
	jobs := make(chan *gitlib.Change, len(changes))
	results := make(chan uastFileResult, len(changes))

	// maxIntraCommitWorkers caps the goroutine count for parsing files within
	// a single commit. Keeping this small avoids excessive concurrency.
	const maxIntraCommitWorkers = 4

	numWorkers := min(maxIntraCommitWorkers, len(changes))

	var wg sync.WaitGroup

	wg.Add(numWorkers)

	for range numWorkers {
		go func() {
			defer wg.Done()

			for change := range jobs {
				before := p.parseBlob(ctx, change.From.Hash, change.From.Name, cache, change.Action, true)
				after := p.parseBlob(ctx, change.To.Hash, change.To.Name, cache, change.Action, false)

				if before != nil || after != nil {
					results <- uastFileResult{before, after, change}
				}
			}
		}()
	}

	for _, change := range changes {
		jobs <- change
	}

	close(jobs)
	wg.Wait()
	close(results)

	var result []uast.Change
	for r := range results {
		result = append(result, uast.Change{
			Before: r.before,
			After:  r.after,
			Change: r.change,
		})
	}

	return result
}

// parseCommitAndSpill parses UAST for a large commit, serializing each file's
// node tree to a temporary gob file and releasing it immediately. This bounds
// peak native memory to one tree-sitter C allocation at a time per worker,
// regardless of how many files the commit touches.
func (p *UASTPipeline) parseCommitAndSpill(
	ctx context.Context,
	changes gitlib.Changes,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
) (string, error) {
	f, err := os.CreateTemp("", "codefang-uast-*.gob")
	if err != nil {
		return "", fmt.Errorf("create uast spill file: %w", err)
	}

	spillPath := f.Name()
	bw := bufio.NewWriter(f)
	enc := gob.NewEncoder(bw)
	wrote := 0

	for i, change := range changes {
		select {
		case <-ctx.Done():
			f.Close()
			os.Remove(spillPath)

			return "", fmt.Errorf("uast spill canceled: %w", ctx.Err())
		default:
		}

		before := p.parseBlob(ctx, change.From.Hash, change.From.Name, cache, change.Action, true)
		after := p.parseBlob(ctx, change.To.Hash, change.To.Name, cache, change.Action, false)

		if before == nil && after == nil {
			continue
		}

		encodeErr := analyze.EncodeUASTRecord(enc, i, before, after)
		if encodeErr != nil {
			node.ReleaseTree(before)
			node.ReleaseTree(after)
			f.Close()
			os.Remove(spillPath)

			return "", encodeErr
		}

		// Release Go node trees immediately — the data is on disk now.
		node.ReleaseTree(before)
		node.ReleaseTree(after)

		wrote++

		// Periodically reclaim C arena pages during long sequential parsing.
		if wrote%uastSpillTrimInterval == 0 {
			uast.MallocTrim()
		}
	}

	flushErr := bw.Flush()
	if flushErr != nil {
		f.Close()
		os.Remove(spillPath)

		return "", fmt.Errorf("flush uast spill: %w", flushErr)
	}

	closeErr := f.Close()
	if closeErr != nil {
		os.Remove(spillPath)

		return "", fmt.Errorf("close uast spill: %w", closeErr)
	}

	// No parseable files produced any output — clean up empty file.
	if wrote == 0 {
		os.Remove(spillPath)

		return "", nil
	}

	return spillPath, nil
}

// parseBlob parses a single blob into a UAST node if the file is supported.
// isBefore indicates whether this is the "before" (old) or "after" (new) version.
func (p *UASTPipeline) parseBlob(
	ctx context.Context,
	hash gitlib.Hash,
	filename string,
	cache map[gitlib.Hash]*gitlib.CachedBlob,
	action gitlib.ChangeAction,
	isBefore bool,
) *node.Node {
	// Check action relevance: before only for Modify/Delete, after only for Modify/Insert.
	if isBefore && action != gitlib.Modify && action != gitlib.Delete {
		return nil
	}

	if !isBefore && action != gitlib.Modify && action != gitlib.Insert {
		return nil
	}

	blob, ok := cache[hash]
	if !ok {
		return nil
	}

	if !p.Parser.IsSupported(filename) {
		return nil
	}

	if len(blob.Data) > maxUASTBlobSize {
		return nil
	}

	parsed, err := p.Parser.Parse(ctx, filename, blob.Data)
	if err != nil {
		return nil
	}

	return parsed
}
