package framework

import (
	"fmt"
	"log/slog"
	"os"
	"runtime"
	"runtime/pprof"
)

// MaybeStartCPUProfile starts CPU profiling to the given file.
// Returns a stop function that must be deferred. Returns a no-op if path is empty.
func MaybeStartCPUProfile(path string) (func(), error) {
	if path == "" {
		return func() {}, nil
	}

	profileFile, err := os.Create(path)
	if err != nil {
		return nil, fmt.Errorf("could not create CPU profile: %w", err)
	}

	err = pprof.StartCPUProfile(profileFile)
	if err != nil {
		profileFile.Close()

		return nil, fmt.Errorf("could not start CPU profile: %w", err)
	}

	stopAndClose := func() {
		pprof.StopCPUProfile()

		_ = profileFile.Close()
	}

	return stopAndClose, nil
}

// MaybeWriteHeapProfile writes a heap profile to the given file.
// No-op if path is empty. Uses the provided logger for error reporting.
func MaybeWriteHeapProfile(path string, logger *slog.Logger) {
	if path == "" {
		return
	}

	if logger == nil {
		logger = slog.Default()
	}

	profileFile, err := os.Create(path)
	if err != nil {
		logger.Error("could not create heap profile", "path", path, "error", err)

		return
	}
	defer profileFile.Close()

	runtime.GC()

	writeErr := pprof.WriteHeapProfile(profileFile)
	if writeErr != nil {
		logger.Error("could not write heap profile", "path", path, "error", writeErr)
	}
}
