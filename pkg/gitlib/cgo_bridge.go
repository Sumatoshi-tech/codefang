package gitlib

/*
#cgo CFLAGS: -I${SRCDIR}/clib -fopenmp
#cgo LDFLAGS: -fopenmp
#include "codefang_git.h"
#include <stdlib.h>

// Link the C source files
#include "clib/utils.c"
#include "clib/blob_ops.c"
#include "clib/diff_ops.c"
*/
import "C"

import (
	"reflect"
	"runtime"
	"unsafe"
)

func init() {
	// Initialize C library settings
	// This sets OMP_NUM_THREADS=1 to prevent thread oversubscription
	C.cf_init()
}

// ConfigureMemoryLimits sets libgit2 global memory limits and glibc malloc
// arena count. mwindowLimit caps mmap'd pack data (default 8 GiB on 64-bit).
// cacheLimit caps the decompressed object cache (default 256 MiB).
// mallocArenaMax caps glibc malloc arenas (default 8*cores, causes RSS bloat).
// Pass 0 for any to leave unchanged. Must be called before opening repos.
func ConfigureMemoryLimits(mwindowLimit, cacheLimit int64, mallocArenaMax int) error {
	rc := C.cf_configure_memory(C.size_t(mwindowLimit), C.size_t(cacheLimit), C.int(mallocArenaMax))
	if rc != 0 {
		return ErrConfigureMemory
	}

	return nil
}

// CGOBridge provides optimized batch operations using the C library.
// It minimizes CGO overhead by processing multiple items per call.
type CGOBridge struct {
	repo       *Repository
	requestBuf []C.cf_blob_request
	resultBuf  []C.cf_blob_arena_result
}

// NewCGOBridge creates a new CGO bridge for the given repository.
func NewCGOBridge(repo *Repository) *CGOBridge {
	return &CGOBridge{repo: repo}
}

// getRepoPtr extracts the underlying C pointer from git2go.Repository.
// Uses reflection to access the unexported 'ptr' field.
func (b *CGOBridge) getRepoPtr() unsafe.Pointer {
	v := reflect.ValueOf(b.repo.repo)
	if v.Kind() == reflect.Ptr {
		v = v.Elem()
	}

	ptrField := v.FieldByName("ptr")
	if !ptrField.IsValid() {
		return nil
	}

	return ptrField.UnsafePointer()
}

// BlobResult represents the result of loading a single blob.
type BlobResult struct {
	Hash      Hash
	Data      []byte
	Size      int64
	IsBinary  bool
	LineCount int
	Error     error
	KeepAlive any
}

// DiffOpType represents the type of diff operation.
type DiffOpType int

// Diff operation types.
const (
	DiffOpEqual  DiffOpType = 0
	DiffOpInsert DiffOpType = 1
	DiffOpDelete DiffOpType = 2
)

// bufferGrowthFactor is the multiplier used when growing internal CGO buffers.
const bufferGrowthFactor = 2

// DiffOp represents a single diff operation.
type DiffOp struct {
	Type      DiffOpType
	LineCount int
}

// DiffResult represents the result of diffing two blobs.
type DiffResult struct {
	OldLines int
	NewLines int
	Ops      []DiffOp
	Error    error
}

// DiffRequest represents a request to diff two blobs.
type DiffRequest struct {
	OldHash Hash
	NewHash Hash
	OldData []byte
	NewData []byte
	HasOld  bool
	HasNew  bool
}

// BatchLoadBlobsArena loads multiple blobs into a provided arena.
// It uses internal recycled buffers for C requests/results to avoid allocation.
func (b *CGOBridge) BatchLoadBlobsArena(hashes []Hash, arena []byte) []BlobResult {
	count := len(hashes)
	if count == 0 {
		return nil
	}

	repoPtr := b.getRepoPtr()
	if repoPtr == nil {
		results := make([]BlobResult, count)
		for i := range results {
			results[i].Error = ErrRepositoryPointer
		}

		return results
	}

	// Ensure internal buffers are sufficient
	if cap(b.requestBuf) < count {
		b.requestBuf = make([]C.cf_blob_request, count, count*bufferGrowthFactor) // Grow with some headroom
	} else {
		b.requestBuf = b.requestBuf[:count]
	}

	if cap(b.resultBuf) < count {
		b.resultBuf = make([]C.cf_blob_arena_result, count, count*bufferGrowthFactor)
	} else {
		b.resultBuf = b.resultBuf[:count]
	}

	// Prepare C requests
	for i, h := range hashes {
		for j := range 20 {
			b.requestBuf[i].oid.id[j] = C.uchar(h[j])
		}
	}

	// Pin memory
	var pinner runtime.Pinner
	pinner.Pin(&b.requestBuf[0])
	pinner.Pin(&b.resultBuf[0])
	if len(arena) > 0 {
		pinner.Pin(&arena[0])
	}
	defer pinner.Unpin()

	var arenaPtr unsafe.Pointer
	if len(arena) > 0 {
		arenaPtr = unsafe.Pointer(&arena[0])
	}

	C.cf_batch_load_blobs_arena(
		(*C.git_repository)(repoPtr),
		&b.requestBuf[0],
		C.int(count),
		arenaPtr,
		C.size_t(len(arena)),
		&b.resultBuf[0],
	)

	// Convert results
	results := make([]BlobResult, count)
	for i, cRes := range b.resultBuf {
		results[i].Hash = hashes[i]

		switch {
		case cRes.error == C.CF_ERR_ARENA_FULL:
			results[i].Error = ErrArenaFull
		case cRes.error != C.CF_OK:
			results[i].Error = cgoBlobError(int(cRes.error))
		default:
			results[i].Size = int64(cRes.size)
			results[i].IsBinary = cRes.is_binary != 0
			results[i].LineCount = int(cRes.line_count)

			// Slice from arena
			offset := int(cRes.offset)
			size := int(cRes.size)
			if offset+size <= len(arena) {
				results[i].Data = arena[offset : offset+size]
			} else {
				results[i].Error = ErrArenaFull
			}
		}
	}

	return results
}

// BatchLoadBlobs loads multiple blobs in a single CGO call.
// This minimizes CGO overhead by processing all requests together.
func (b *CGOBridge) BatchLoadBlobs(hashes []Hash) []BlobResult {
	if len(hashes) == 0 {
		return nil
	}

	repoPtr := b.getRepoPtr()
	if repoPtr == nil {
		// Return errors for all requests
		results := make([]BlobResult, len(hashes))
		for i := range results {
			results[i].Hash = hashes[i]
			results[i].Error = ErrRepositoryPointer
		}

		return results
	}

	// Prepare C requests
	cRequests := make([]C.cf_blob_request, len(hashes))
	for i, h := range hashes {
		for j := range 20 {
			cRequests[i].oid.id[j] = C.uchar(h[j])
		}
	}

	// Prepare C results
	cResults := make([]C.cf_blob_result, len(hashes))

	// Pin memory to prevent GC movement during CGO call
	var pinner runtime.Pinner
	pinner.Pin(&cRequests[0])
	pinner.Pin(&cResults[0])

	// Single CGO call to load all blobs
	C.cf_batch_load_blobs(
		(*C.git_repository)(repoPtr),
		&cRequests[0],
		C.int(len(hashes)),
		&cResults[0],
	)

	pinner.Unpin()

	// Convert C results to Go
	results := make([]BlobResult, len(hashes))
	for i, cRes := range cResults {
		results[i].Hash = hashes[i]

		if cRes.error != C.CF_OK {
			results[i].Error = cgoBlobError(int(cRes.error))

			continue
		}

		results[i].Size = int64(cRes.size)
		results[i].IsBinary = cRes.is_binary != 0
		results[i].LineCount = int(cRes.line_count)

		// Copy data from C to Go memory
		if cRes.size > 0 && cRes.data != nil {
			data := make([]byte, cRes.size)
			copy(data, (*[1 << 30]byte)(cRes.data)[:cRes.size:cRes.size])
			results[i].Data = data
		}
	}

	// Free C-allocated memory
	C.cf_free_blob_results(&cResults[0], C.int(len(cResults)))

	return results
}

// TreeDiff computes the difference between two trees in a single batch CGO call.
// Skips libgit2 diff when both tree OIDs are equal (e.g. metadata-only commits).
func (b *CGOBridge) TreeDiff(oldTreeHash, newTreeHash Hash) (Changes, error) {
	if !oldTreeHash.IsZero() && !newTreeHash.IsZero() && oldTreeHash == newTreeHash {
		return make(Changes, 0), nil
	}

	var cOldOid, cNewOid C.git_oid
	var pOldOid, pNewOid *C.git_oid

	if !oldTreeHash.IsZero() {
		for i := range 20 {
			cOldOid.id[i] = C.uchar(oldTreeHash[i])
		}
		pOldOid = &cOldOid
	}

	if !newTreeHash.IsZero() {
		for i := range 20 {
			cNewOid.id[i] = C.uchar(newTreeHash[i])
		}
		pNewOid = &cNewOid
	}

	repoPtr := b.getRepoPtr()
	if repoPtr == nil {
		return nil, ErrRepositoryPointer
	}

	var cResult C.cf_tree_diff_result

	// Ensure result is clean
	cResult.changes = nil
	cResult.count = 0

	// Call C function
	ret := C.cf_tree_diff(
		(*C.git_repository)(repoPtr),
		pOldOid,
		pNewOid,
		&cResult,
	)

	if ret != C.CF_OK {
		return nil, cgoDiffError(int(ret))
	}
	defer C.cf_free_tree_diff_result(&cResult)

	if cResult.count == 0 {
		return make(Changes, 0), nil
	}

	changes := make(Changes, 0, cResult.count)

	// Iterate C array
	// Unsafe pointer arithmetic
	cChanges := (*[1 << 30]C.cf_change)(unsafe.Pointer(cResult.changes))[:cResult.count:cResult.count]

	const (
		fileModeCommit = 0o160000
		fileModeTree   = 0o040000
	)

	for i := range int(cResult.count) {
		cChange := cChanges[i]

		// Filter out non-blob changes (submodules, trees) early
		// We only want to surface changes to files that we can process content for.
		// If either side is a Commit or Tree, skip it.
		// Exception: If one side is blob and other is not (TypeChange), we might want it?
		// But status is usually TYPECHANGE for that.
		// If status is MODIFIED, modes usually match or compatible (exec vs non-exec).
		if cChange.old_mode == fileModeCommit || cChange.old_mode == fileModeTree ||
			cChange.new_mode == fileModeCommit || cChange.new_mode == fileModeTree {
			continue
		}

		change := &Change{}

		// Map status
		switch cChange.status {
		case C.GIT_DELTA_ADDED:
			change.Action = Insert
			change.To = ChangeEntry{
				Name: C.GoString(cChange.new_path),
				Size: int64(cChange.new_size),
				Mode: uint16(cChange.new_mode),
			}
			copy(change.To.Hash[:], C.GoBytes(unsafe.Pointer(&cChange.new_oid[0]), 20))

		case C.GIT_DELTA_DELETED:
			change.Action = Delete
			change.From = ChangeEntry{
				Name: C.GoString(cChange.old_path),
				Size: int64(cChange.old_size),
				Mode: uint16(cChange.old_mode),
			}
			copy(change.From.Hash[:], C.GoBytes(unsafe.Pointer(&cChange.old_oid[0]), 20))

		case C.GIT_DELTA_MODIFIED, C.GIT_DELTA_RENAMED, C.GIT_DELTA_COPIED:
			change.Action = Modify
			change.From = ChangeEntry{
				Name: C.GoString(cChange.old_path),
				Size: int64(cChange.old_size),
				Mode: uint16(cChange.old_mode),
			}
			copy(change.From.Hash[:], C.GoBytes(unsafe.Pointer(&cChange.old_oid[0]), 20))

			change.To = ChangeEntry{
				Name: C.GoString(cChange.new_path),
				Size: int64(cChange.new_size),
				Mode: uint16(cChange.new_mode),
			}
			copy(change.To.Hash[:], C.GoBytes(unsafe.Pointer(&cChange.new_oid[0]), 20))
		default:
			continue
		}

		changes = append(changes, change)
	}

	return changes, nil
}

// BatchDiffBlobs computes diffs for multiple blob pairs in a single CGO call.
// This minimizes CGO overhead by processing all requests together.
func (b *CGOBridge) BatchDiffBlobs(requests []DiffRequest) []DiffResult {
	if len(requests) == 0 {
		return nil
	}

	repoPtr := b.getRepoPtr()
	if repoPtr == nil {
		// Return errors for all requests
		results := make([]DiffResult, len(requests))
		for i := range results {
			results[i].Error = ErrRepositoryPointer
		}

		return results
	}

	// Pin memory to prevent GC movement during CGO call.
	// CRITICAL: We must pin blob data slices BEFORE setting pointers in cRequests,
	// as the GC could move Go-allocated memory during the CGO call.
	var pinner runtime.Pinner

	// Prepare C requests
	cRequests := make([]C.cf_diff_request, len(requests))
	for i, req := range requests {
		if req.HasOld {
			for j := range 20 {
				cRequests[i].old_oid.id[j] = C.uchar(req.OldHash[j])
			}
			cRequests[i].has_old = 1
			if len(req.OldData) > 0 {
				// Pin the underlying byte slice to prevent GC movement
				pinner.Pin(&req.OldData[0])
				cRequests[i].old_data = unsafe.Pointer(&req.OldData[0])
				cRequests[i].old_size = C.size_t(len(req.OldData))
			}
		}
		if req.HasNew {
			for j := range 20 {
				cRequests[i].new_oid.id[j] = C.uchar(req.NewHash[j])
			}
			cRequests[i].has_new = 1
			if len(req.NewData) > 0 {
				// Pin the underlying byte slice to prevent GC movement
				pinner.Pin(&req.NewData[0])
				cRequests[i].new_data = unsafe.Pointer(&req.NewData[0])
				cRequests[i].new_size = C.size_t(len(req.NewData))
			}
		}
	}

	// Prepare C results
	cResults := make([]C.cf_diff_result, len(requests))

	// Pin the request and result arrays
	pinner.Pin(&cRequests[0])
	pinner.Pin(&cResults[0])

	// Single CGO call to diff all blobs
	C.cf_batch_diff_blobs(
		(*C.git_repository)(repoPtr),
		&cRequests[0],
		C.int(len(requests)),
		&cResults[0],
	)

	pinner.Unpin()

	// Convert C results to Go
	results := make([]DiffResult, len(requests))
	for i, cRes := range cResults {
		if cRes.error != C.CF_OK {
			results[i].Error = cgoDiffError(int(cRes.error))

			continue
		}

		results[i].OldLines = int(cRes.old_lines)
		results[i].NewLines = int(cRes.new_lines)

		// Copy ops from C to Go
		if cRes.op_count > 0 && cRes.ops != nil {
			results[i].Ops = make([]DiffOp, cRes.op_count)
			cOps := (*[1 << 20]C.cf_diff_op)(unsafe.Pointer(cRes.ops))[:cRes.op_count:cRes.op_count]
			for j := range int(cRes.op_count) {
				results[i].Ops[j].Type = DiffOpType(cOps[j].type_)
				results[i].Ops[j].LineCount = int(cOps[j].line_count)
			}
		}
	}

	// Free C-allocated memory
	C.cf_free_diff_results(&cResults[0], C.int(len(cResults)))

	return results
}

// Error types for CGO operations
type cgoError string

func (e cgoError) Error() string { return string(e) }

// CGO operation errors.
var (
	ErrRepositoryPointer = cgoError("failed to get repository pointer")
	ErrBlobLookup        = cgoError("blob lookup failed")
	ErrBlobMemory        = cgoError("memory allocation failed for blob")
	ErrBlobBinary        = cgoError("blob is binary")
	ErrDiffLookup        = cgoError("diff blob lookup failed")
	ErrDiffMemory        = cgoError("memory allocation failed for diff")
	ErrDiffBinary        = cgoError("diff blob is binary")
	ErrDiffCompute       = cgoError("diff computation failed")
	ErrArenaFull         = cgoError("arena full")
	ErrConfigureMemory   = cgoError("cf_configure_memory failed")
)

func cgoBlobError(code int) error {
	switch code {
	case C.CF_ERR_LOOKUP:
		return ErrBlobLookup
	case C.CF_ERR_NOMEM:
		return ErrBlobMemory
	case C.CF_ERR_BINARY:
		return ErrBlobBinary
	case C.CF_ERR_ARENA_FULL:
		return ErrArenaFull
	default:
		return cgoError("unknown blob error")
	}
}

func cgoDiffError(code int) error {
	switch code {
	case C.CF_ERR_LOOKUP:
		return ErrDiffLookup
	case C.CF_ERR_NOMEM:
		return ErrDiffMemory
	case C.CF_ERR_BINARY:
		return ErrDiffBinary
	case C.CF_ERR_DIFF:
		return ErrDiffCompute
	default:
		return cgoError("unknown diff error")
	}
}
