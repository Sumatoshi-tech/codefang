// Package meminfo provides memory information utilities.
package meminfo

import (
	"fmt"
	"os"
)

// procStatmPath is the path to the process statm file.
const procStatmPath = "/proc/self/statm"

// ReadRSSBytes returns the current process RSS in bytes.
// Returns 0 if the information is unavailable.
func ReadRSSBytes() int64 {
	f, err := os.Open(procStatmPath)
	if err != nil {
		return 0
	}
	defer f.Close()

	var vsize, rss int64

	_, scanErr := fmt.Fscan(f, &vsize)
	if scanErr != nil {
		return 0
	}

	_, scanErr = fmt.Fscan(f, &rss)
	if scanErr != nil {
		return 0
	}

	return rss * int64(os.Getpagesize())
}
