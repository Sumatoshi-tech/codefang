package framework

import (
	"errors"
	"fmt"
	"log/slog"
	"sync"
	"time"

	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/trace"

	"github.com/Sumatoshi-tech/codefang/pkg/gitlib"
)

// ErrWorkerStalled is returned when a worker does not respond within the timeout
// and all retry attempts with exponential backoff are exhausted.
var ErrWorkerStalled = errors.New("worker stalled: CGO call did not return within timeout")

// Watchdog constants.
const (
	// MaxStallRetries is the maximum number of retry attempts before giving up.
	MaxStallRetries = 3

	// backoffBase is the base duration for exponential backoff between retries.
	backoffBase = time.Second

	// backoffMultiplier is the exponential growth factor for backoff durations.
	// Sequence: 0s (immediate), 1s, 4s.
	backoffMultiplier = 4
)

// Watchdog monitors worker pool health and recreates stalled workers.
// It wraps the pool worker channel with timeout-aware dispatch and
// provides exponential backoff retry on stall detection.
type Watchdog struct {
	mu sync.Mutex

	span     trace.Span
	repoPath string
	config   CoordinatorConfig
	timeout  time.Duration
	logger   *slog.Logger

	// poolRepos holds per-worker repository handles (one per pool worker).
	poolRepos []*gitlib.Repository

	// poolWorkers holds per-worker Worker instances.
	poolWorkers []*gitlib.Worker

	// poolRequests is the shared channel for dispatching requests to pool workers.
	poolRequests chan gitlib.WorkerRequest

	// stalledCount tracks total stall events for observability.
	stalledCount int
}

// WatchdogConfig holds parameters for creating a Watchdog.
type WatchdogConfig struct {
	Span         trace.Span
	RepoPath     string
	Config       CoordinatorConfig
	PoolRepos    []*gitlib.Repository
	PoolWorkers  []*gitlib.Worker
	PoolRequests chan gitlib.WorkerRequest
	Logger       *slog.Logger
}

// NewWatchdog creates a Watchdog that monitors the worker pool.
// Returns nil if timeout is zero (disabled).
func NewWatchdog(cfg WatchdogConfig) *Watchdog {
	if cfg.Config.WorkerTimeout <= 0 {
		return nil
	}

	lg := cfg.Logger
	if lg == nil {
		lg = slog.Default()
	}

	span := cfg.Span

	return &Watchdog{
		span:         span,
		repoPath:     cfg.RepoPath,
		config:       cfg.Config,
		timeout:      cfg.Config.WorkerTimeout,
		logger:       lg,
		poolRepos:    cfg.PoolRepos,
		poolWorkers:  cfg.PoolWorkers,
		poolRequests: cfg.PoolRequests,
	}
}

// StalledCount returns the total number of stall events observed.
func (wd *Watchdog) StalledCount() int {
	wd.mu.Lock()
	defer wd.mu.Unlock()

	return wd.stalledCount
}

// WaitForResponse waits for a response on the given channel with timeout.
// Returns true if the response was received, false if the worker stalled.
// On stall, it recreates one pool worker and returns false so the caller can retry.
func (wd *Watchdog) WaitForResponse(ch <-chan gitlib.BlobBatchResponse) (gitlib.BlobBatchResponse, bool) {
	select {
	case resp := <-ch:
		return resp, true
	case <-time.After(wd.timeout):
		wd.handleStall("BlobBatchRequest")

		return gitlib.BlobBatchResponse{}, false
	}
}

// WaitForDiffResponse waits for a diff response with timeout.
func (wd *Watchdog) WaitForDiffResponse(ch <-chan gitlib.DiffBatchResponse) (gitlib.DiffBatchResponse, bool) {
	select {
	case resp := <-ch:
		return resp, true
	case <-time.After(wd.timeout):
		wd.handleStall("DiffBatchRequest")

		return gitlib.DiffBatchResponse{}, false
	}
}

// WaitForTreeDiffResponse waits for a tree diff response with timeout.
func (wd *Watchdog) WaitForTreeDiffResponse(ch <-chan gitlib.TreeDiffResponse) (gitlib.TreeDiffResponse, bool) {
	select {
	case resp := <-ch:
		return resp, true
	case <-time.After(wd.timeout):
		wd.handleStall("TreeDiffRequest")

		return gitlib.TreeDiffResponse{}, false
	}
}

// handleStall records a stall event and recreates one pool worker.
func (wd *Watchdog) handleStall(reqType string) {
	wd.mu.Lock()
	defer wd.mu.Unlock()

	wd.stalledCount++

	wd.logger.Warn("worker stall detected",
		slog.String("request_type", reqType),
		slog.Int("stall_count", wd.stalledCount),
		slog.Duration("timeout", wd.timeout),
	)

	if wd.span != nil {
		wd.span.AddEvent("worker.stall_detected", trace.WithAttributes(
			attribute.String("request_type", reqType),
			attribute.Int("stall_count", wd.stalledCount),
		))
	}

	wd.recreateOneWorker()
}

// recreateOneWorker replaces one pool worker with a fresh one.
// The old worker goroutine is abandoned (CGO cannot be preempted).
// A new repository handle and worker are created to replace it.
func (wd *Watchdog) recreateOneWorker() {
	if len(wd.poolWorkers) == 0 {
		return
	}

	// Replace the last worker in the pool.
	idx := len(wd.poolWorkers) - 1

	newRepo, err := gitlib.OpenRepository(wd.repoPath)
	if err != nil {
		wd.logger.Error("failed to open repository for worker recreation",
			slog.String("error", err.Error()),
		)

		return
	}

	// Old worker goroutine is abandoned; it will exit when CGO returns
	// and tries to send on the closed/full response channel.
	// Old repo handle is intentionally leaked (freeing during active CGO would crash).
	newWorker := gitlib.NewWorker(newRepo, wd.poolRequests)
	newWorker.Start()

	wd.poolRepos[idx] = newRepo
	wd.poolWorkers[idx] = newWorker

	wd.logger.Info("worker recreated",
		slog.Int("worker_index", idx),
	)

	if wd.span != nil {
		wd.span.AddEvent("worker.recreated", trace.WithAttributes(
			attribute.Int("worker_index", idx),
		))
	}
}

// BackoffDuration returns the backoff duration for the given retry attempt (0-indexed).
// Sequence: 0s, 1s, 4s.
func BackoffDuration(attempt int) time.Duration {
	if attempt <= 0 {
		return 0
	}

	dur := backoffBase
	for range attempt - 1 {
		dur *= backoffMultiplier
	}

	return dur
}

// StallError creates a descriptive error for a stalled worker.
func StallError(reqType string, retries int) error {
	return fmt.Errorf(
		"%w: request_type=%s retries=%d; check repository integrity with 'git fsck'",
		ErrWorkerStalled, reqType, retries,
	)
}
