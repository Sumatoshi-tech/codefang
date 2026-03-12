//go:build !linux

package meminfo

// ReadRSSBytes returns the current process RSS in bytes.
// Returns 0 on non-Linux platforms.
func ReadRSSBytes() int64 {
	return 0
}
