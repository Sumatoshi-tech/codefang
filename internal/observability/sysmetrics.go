package observability

import (
	"bufio"
	"os"
	"runtime"
	"strconv"
	"strings"
	"time"
)

// HeapSnapshot captures Go runtime memory stats at a point in time.
type HeapSnapshot struct {
	HeapInuse   int64
	HeapAlloc   int64
	HeapObjects int64 // Live heap objects (detect accumulation).
	StackInuse  int64 // Stack memory (goroutine stacks).
	NextGC      int64 // Target heap size for next GC cycle.
	Sys         int64 // Total bytes obtained from the OS (Go runtime).
	RSS         int64 // Resident set size (Go + native C memory).
	NumGC       uint32
	Goroutines  int // Number of goroutines.
	TakenAtNS   int64
}

// TakeHeapSnapshot reads [runtime.MemStats] and returns a HeapSnapshot.
func TakeHeapSnapshot() HeapSnapshot {
	var m runtime.MemStats
	runtime.ReadMemStats(&m)

	return HeapSnapshot{
		HeapInuse:   int64(m.HeapInuse),
		HeapAlloc:   int64(m.HeapAlloc),
		HeapObjects: int64(m.HeapObjects),
		StackInuse:  int64(m.StackInuse),
		NextGC:      int64(m.NextGC),
		Sys:         int64(m.Sys),
		RSS:         ReadRSSBytes(),
		NumGC:       m.NumGC,
		Goroutines:  runtime.NumGoroutine(),
		TakenAtNS:   time.Now().UnixNano(),
	}
}

// SmapsRollup holds parsed /proc/self/smaps_rollup data for classifying
// memory into anonymous (heap/stacks/native) vs file-backed (mmap/packfiles).
type SmapsRollup struct {
	Rss          int64
	Pss          int64
	Anonymous    int64
	FileBacked   int64 // Computed: Rss - Anonymous.
	SharedClean  int64
	SharedDirty  int64
	PrivateClean int64
	PrivateDirty int64
}

// ReadSmapsRollup reads and parses /proc/self/smaps_rollup.
// Returns a zero SmapsRollup on non-Linux platforms or on error.
func ReadSmapsRollup() SmapsRollup {
	f, err := os.Open("/proc/self/smaps_rollup")
	if err != nil {
		return SmapsRollup{}
	}
	defer f.Close()

	var s SmapsRollup

	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		line := scanner.Text()
		if v, ok := parseSmapsKB(line, "Rss:"); ok {
			s.Rss = v
		} else if v, ok := parseSmapsKB(line, "Pss:"); ok {
			s.Pss = v
		} else if v, ok := parseSmapsKB(line, "Anonymous:"); ok {
			s.Anonymous = v
		} else if v, ok := parseSmapsKB(line, "Shared_Clean:"); ok {
			s.SharedClean = v
		} else if v, ok := parseSmapsKB(line, "Shared_Dirty:"); ok {
			s.SharedDirty = v
		} else if v, ok := parseSmapsKB(line, "Private_Clean:"); ok {
			s.PrivateClean = v
		} else if v, ok := parseSmapsKB(line, "Private_Dirty:"); ok {
			s.PrivateDirty = v
		}
	}

	s.FileBacked = s.Rss - s.Anonymous

	return s
}

// parseSmapsKB extracts a kB value from a smaps line like "Rss: 1234 kB".
// Returns the value in bytes and true if the line matches the prefix.
func parseSmapsKB(line, prefix string) (int64, bool) {
	after, ok := strings.CutPrefix(line, prefix)
	if !ok {
		return 0, false
	}

	trimmed := strings.TrimSpace(after)
	trimmed = strings.TrimSuffix(trimmed, " kB")

	v, err := strconv.ParseInt(strings.TrimSpace(trimmed), 10, 64)
	if err != nil {
		return 0, false
	}

	return v * 1024, true
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
