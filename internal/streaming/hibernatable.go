// Package streaming provides chunked execution with analyzer hibernation for memory-bounded analysis.
package streaming

import (
	"log/slog"
	"os"
	"os/signal"
	"sync"
	"syscall"
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
type SpillCleanupGuard struct {
	cleaners []SpillCleaner
	logger   *slog.Logger
	sigCh    chan os.Signal
	once     sync.Once
}

// NewSpillCleanupGuard registers SIGTERM and SIGINT handlers that invoke
// CleanupSpills on all registered analyzers. The caller must defer Close()
// to ensure cleanup runs on normal/error exit and the signal handler is
// deregistered.
func NewSpillCleanupGuard(cleaners []SpillCleaner, logger *slog.Logger) *SpillCleanupGuard {
	g := &SpillCleanupGuard{
		cleaners: cleaners,
		logger:   logger,
		sigCh:    make(chan os.Signal, 1),
	}

	signal.Notify(g.sigCh, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		sig, ok := <-g.sigCh
		if !ok {
			return
		}

		g.logger.Warn("streaming: received signal, cleaning up spill files", "signal", sig.String())
		g.cleanup()
	}()

	return g
}

// Close performs spill cleanup (if not already done) and deregisters
// the signal handler.
func (g *SpillCleanupGuard) Close() {
	g.cleanup()
	signal.Stop(g.sigCh)
	close(g.sigCh)
}

func (g *SpillCleanupGuard) cleanup() {
	g.once.Do(func() {
		for _, c := range g.cleaners {
			c.CleanupSpills()
		}
	})
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

// hibernateAll calls Hibernate on all hibernatable analyzers.
func hibernateAll(analyzers []Hibernatable) error {
	for _, h := range analyzers {
		err := h.Hibernate()
		if err != nil {
			return err
		}
	}

	return nil
}

// bootAll calls Boot on all hibernatable analyzers.
func bootAll(analyzers []Hibernatable) error {
	for _, h := range analyzers {
		err := h.Boot()
		if err != nil {
			return err
		}
	}

	return nil
}
