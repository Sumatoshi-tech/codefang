package observability

import (
	"os"
	"runtime"
	"strconv"
	"strings"
	"time"
)

// HeapSnapshot captures Go runtime memory stats at a point in time.
type HeapSnapshot struct {
	HeapInuse int64
	HeapAlloc int64
	Sys       int64 // Total bytes obtained from the OS (Go runtime).
	RSS       int64 // Resident set size (Go + native C memory).
	NumGC     uint32
	TakenAtNS int64
}

// TakeHeapSnapshot reads [runtime.MemStats] and returns a HeapSnapshot.
func TakeHeapSnapshot() HeapSnapshot {
	var m runtime.MemStats
	runtime.ReadMemStats(&m)

	return HeapSnapshot{
		HeapInuse: int64(m.HeapInuse),
		HeapAlloc: int64(m.HeapAlloc),
		Sys:       int64(m.Sys),
		RSS:       ReadRSSBytes(),
		NumGC:     m.NumGC,
		TakenAtNS: time.Now().UnixNano(),
	}
}

// statmMinFields is the minimum number of fields required from /proc/self/statm
// to extract the RSS (resident set size) value (fields: vsize, rss).
const statmMinFields = 2

// ReadRSSBytes reads the process RSS from /proc/self/statm.
// Returns 0 on non-Linux platforms or on error.
func ReadRSSBytes() int64 {
	data, err := os.ReadFile("/proc/self/statm")
	if err != nil {
		return 0
	}

	fields := strings.Fields(string(data))
	if len(fields) < statmMinFields {
		return 0
	}

	// Field 1 is resident pages.
	pages, err := strconv.ParseInt(fields[1], 10, 64)
	if err != nil {
		return 0
	}

	return pages * int64(os.Getpagesize())
}
