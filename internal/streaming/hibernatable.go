// Package streaming provides chunked execution with analyzer hibernation for memory-bounded analysis.
package streaming

import (
	"log/slog"

	"github.com/Sumatoshi-tech/codefang/pkg/sigutil"
)

// SpillCleaner is an optional interface for analyzers that create spill
// files on disk during hibernation. CleanupSpills removes all temp
// directories and files. It is called by SpillCleanupGuard on normal
// exit, error exit, and SIGTERM/SIGINT to prevent orphaned temp files.
type SpillCleaner interface {
	CleanupSpills()
}

// SpillCleanupGuard ensures that spill temp directories are removed when
// the streaming pipeline exits, whether normally, on error, or via signal.
// Create one via NewSpillCleanupGuard and defer its Close method.
// It embeds sigutil.SignalCleanupGuard for reusable signal-driven cleanup.
type SpillCleanupGuard struct {
	*sigutil.SignalCleanupGuard
}

// NewSpillCleanupGuard registers SIGTERM and SIGINT handlers that invoke
// CleanupSpills on all registered analyzers. The caller must defer Close()
// to ensure cleanup runs on normal/error exit and the signal handler is
// deregistered.
func NewSpillCleanupGuard(cleaners []SpillCleaner, logger *slog.Logger) *SpillCleanupGuard {
	cleanup := func() {
		for _, c := range cleaners {
			c.CleanupSpills()
		}
	}

	return &SpillCleanupGuard{
		SignalCleanupGuard: sigutil.NewSignalCleanupGuard(cleanup, logger),
	}
}

// Hibernatable is an optional interface for analyzers that support hibernation.
// Analyzers implementing this interface can have their state compressed between
// chunks to reduce memory usage during streaming execution.
type Hibernatable interface {
	// Hibernate compresses the analyzer's state to reduce memory usage.
	// Called between chunks during streaming execution.
	Hibernate() error

	// Boot restores the analyzer from hibernated state.
	// Called before processing a new chunk after hibernation.
	Boot() error
}
