package meminfo

// FRD: specs/frds/FRD-20260312-static-rss-logging.md.

import (
	"runtime"
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestReadRSSBytes_ReturnsNonNegative(t *testing.T) {
	t.Parallel()

	rss := ReadRSSBytes()

	assert.GreaterOrEqual(t, rss, int64(0))
}

func TestReadRSSBytes_NonZeroOnLinux(t *testing.T) {
	t.Parallel()

	if runtime.GOOS != "linux" {
		t.Skip("RSS reading only available on Linux")
	}

	rss := ReadRSSBytes()

	assert.Positive(t, rss, "RSS should be positive on Linux")
}
