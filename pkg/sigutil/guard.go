// Package sigutil provides signal-handling utilities for graceful cleanup.
package sigutil

import (
	"log/slog"
	"os"
	"os/signal"
	"sync"
	"syscall"
)

// SignalCleanupGuard ensures a cleanup function runs exactly once when the
// process exits normally, on error, or via SIGINT/SIGTERM. Create one via
// NewSignalCleanupGuard and defer its Close method.
type SignalCleanupGuard struct {
	cleanup func()
	logger  *slog.Logger
	sigCh   chan os.Signal
	once    sync.Once
}

// NewSignalCleanupGuard registers SIGINT and SIGTERM handlers that invoke the
// cleanup function exactly once. The caller must defer Close() to ensure
// cleanup runs on normal/error exit and the signal handler is deregistered.
// A nil cleanup function is treated as a no-op.
func NewSignalCleanupGuard(cleanup func(), logger *slog.Logger) *SignalCleanupGuard {
	if cleanup == nil {
		cleanup = func() {}
	}

	g := &SignalCleanupGuard{
		cleanup: cleanup,
		logger:  logger,
		sigCh:   make(chan os.Signal, 1),
	}

	signal.Notify(g.sigCh, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		sig, ok := <-g.sigCh
		if !ok {
			return
		}

		g.logger.Warn("signal received, running cleanup guard", "signal", sig.String())
		g.run()
	}()

	return g
}

// Close performs cleanup (if not already done) and deregisters the signal
// handler.
func (g *SignalCleanupGuard) Close() {
	g.run()
	signal.Stop(g.sigCh)
	close(g.sigCh)
}

func (g *SignalCleanupGuard) run() {
	g.once.Do(g.cleanup)
}
