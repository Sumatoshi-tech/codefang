package framework

import (
	"fmt"
	"log"
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
// No-op if path is empty.
func MaybeWriteHeapProfile(path string) {
	if path == "" {
		return
	}

	profileFile, err := os.Create(path)
	if err != nil {
		log.Printf("could not create heap profile: %v", err)

		return
	}
	defer profileFile.Close()

	runtime.GC()

	writeErr := pprof.WriteHeapProfile(profileFile)
	if writeErr != nil {
		log.Printf("could not write heap profile: %v", writeErr)
	}
}
