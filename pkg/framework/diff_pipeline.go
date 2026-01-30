package framework

import (
	"context"
	"strings"

	"github.com/sergi/go-diff/diffmatchpatch"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
	"github.com/Sumatoshi-tech/codefang/pkg/plumbing"
)

// CommitData holds all processed data for a commit.
type CommitData struct {
	Commit    *gitlib.Commit
	Index     int
	Changes   gitlib.Changes
	BlobCache map[gitlib.Hash]*gitlib.CachedBlob
	FileDiffs map[string]plumbing.FileDiffData
	Error     error
}

// DiffPipeline processes blob data to compute file diffs.
type DiffPipeline struct {
	PoolWorkerChan chan<- gitlib.WorkerRequest
	BufferSize     int
}

// NewDiffPipeline creates a new diff pipeline.
func NewDiffPipeline(workerChan chan<- gitlib.WorkerRequest, bufferSize int) *DiffPipeline {
	if bufferSize <= 0 {
		bufferSize = 1
	}
	return &DiffPipeline{
		PoolWorkerChan: workerChan,
		BufferSize:     bufferSize,
	}
}

type diffJob struct {
	data     CommitData
	respChan chan gitlib.DiffBatchResponse
	paths    []string
	changes  []*gitlib.Change
}

// Process receives blob data and outputs commit data with computed diffs.
func (p *DiffPipeline) Process(ctx context.Context, blobs <-chan BlobData) <-chan CommitData {
	out := make(chan CommitData)

	jobs := make(chan diffJob, p.BufferSize)

	// Producer: Prepare Diff Requests -> Jobs
	go func() {
		defer close(jobs)

		for blobData := range blobs {
			select {
			case <-ctx.Done():
				return
			default:
			}

			// Initial CommitData
			commitData := CommitData{
				Commit:    blobData.Commit,
				Index:     blobData.Index,
				Changes:   blobData.Changes,
				BlobCache: blobData.BlobCache,
				Error:     blobData.Error,
			}

			job := diffJob{data: commitData}

			if commitData.Error == nil {
				req, paths, changes := p.prepareDiffRequest(blobData)
				job.paths = paths
				job.changes = changes
				
				if len(req.Requests) > 0 {
					job.respChan = make(chan gitlib.DiffBatchResponse, 1)
					req.Response = job.respChan
					
					select {
					case p.PoolWorkerChan <- req:
					case <-ctx.Done():
						return
					}
				}
			}

			select {
			case jobs <- job:
			case <-ctx.Done():
				return
			}
		}
	}()

	// Consumer: Wait for Diffs -> Output
	go func() {
		defer close(out)

		for job := range jobs {
			select {
			case <-ctx.Done():
				return
			default:
			}

			if job.data.Error != nil {
				out <- job.data
				continue
			}

			job.data.FileDiffs = make(map[string]plumbing.FileDiffData)

			if job.respChan != nil {
				select {
				case resp := <-job.respChan:
					p.processDiffResponse(job.data, resp, job.paths, job.changes)
				case <-ctx.Done():
					return
				}
			}
			
			select {
			case out <- job.data:
			case <-ctx.Done():
				return
			}
		}
	}()

	return out
}

func (p *DiffPipeline) prepareDiffRequest(blobData BlobData) (gitlib.DiffBatchRequest, []string, []*gitlib.Change) {
	var requests []gitlib.DiffRequest
	var paths []string
	var changesForIndex []*gitlib.Change

	for _, change := range blobData.Changes {
		if change.Action != gitlib.Modify {
			continue
		}
		oldBlob := blobData.BlobCache[change.From.Hash]
		newBlob := blobData.BlobCache[change.To.Hash]
		if oldBlob == nil || newBlob == nil {
			continue
		}
		if oldBlob.IsBinary() || newBlob.IsBinary() {
			continue
		}
		requests = append(requests, gitlib.DiffRequest{
			OldHash: change.From.Hash,
			NewHash: change.To.Hash,
			HasOld:  true,
			HasNew:  true,
		})
		paths = append(paths, change.To.Name)
		changesForIndex = append(changesForIndex, change)
	}

	return gitlib.DiffBatchRequest{Requests: requests}, paths, changesForIndex
}

func (p *DiffPipeline) processDiffResponse(
	data CommitData,
	resp gitlib.DiffBatchResponse,
	paths []string,
	changes []*gitlib.Change,
) {
	diffResults := resp.Results
	
	for i, path := range paths {
		oldBlob := data.BlobCache[changes[i].From.Hash]
		newBlob := data.BlobCache[changes[i].To.Hash]

		// Use Go's counting
		oldLines, errOld := oldBlob.CountLines()
		newLines, errNew := newBlob.CountLines()
		if errOld != nil || errNew != nil {
			continue
		}

		diffRes := diffResults[i]
		
		if diffRes.Error != nil {
			data.FileDiffs[path] = p.fileDiffFromGoDiff(oldBlob, newBlob, oldLines, newLines)
			continue
		}

		diffs := convertDiffOpsToDMP(diffRes.Ops)
		data.FileDiffs[path] = plumbing.FileDiffData{
			OldLinesOfCode: oldLines,
			NewLinesOfCode: newLines,
			Diffs:          diffs,
		}
	}
}

func convertDiffOpsToDMP(ops []gitlib.DiffOp) []diffmatchpatch.Diff {
	diffs := make([]diffmatchpatch.Diff, 0, len(ops))
	for _, op := range ops {
		var dmpType diffmatchpatch.Operation
		switch op.Type {
		case gitlib.DiffOpEqual:
			dmpType = diffmatchpatch.DiffEqual
		case gitlib.DiffOpInsert:
			dmpType = diffmatchpatch.DiffInsert
		case gitlib.DiffOpDelete:
			dmpType = diffmatchpatch.DiffDelete
		default:
			continue
		}
		diffs = append(diffs, diffmatchpatch.Diff{
			Type: dmpType,
			Text: strings.Repeat("L", op.LineCount),
		})
	}
	return diffs
}

func (p *DiffPipeline) fileDiffFromGoDiff(oldBlob, newBlob *gitlib.CachedBlob, oldLines, newLines int) plumbing.FileDiffData {
	strFrom, strTo := string(oldBlob.Data), string(newBlob.Data)
	if strFrom == strTo {
		return plumbing.FileDiffData{
			OldLinesOfCode: oldLines,
			NewLinesOfCode: newLines,
			Diffs:          []diffmatchpatch.Diff{{Type: diffmatchpatch.DiffEqual, Text: strings.Repeat("L", oldLines)}},
		}
	}
	dmp := diffmatchpatch.New()
	src, dst, _ := dmp.DiffLinesToRunes(strFrom, strTo)
	diffs := dmp.DiffMainRunes(src, dst, false)
	diffs = dmp.DiffCleanupMerge(dmp.DiffCleanupSemanticLossless(diffs))
	return plumbing.FileDiffData{
		OldLinesOfCode: oldLines,
		NewLinesOfCode: newLines,
		Diffs:          diffs,
	}
}
