package observability_test

import (
	"log/slog"
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/Sumatoshi-tech/codefang/pkg/observability"
)

func TestDefaultConfig_HasSensibleDefaults(t *testing.T) {
	t.Parallel()

	cfg := observability.DefaultConfig()

	assert.Equal(t, "codefang", cfg.ServiceName)
	assert.Equal(t, observability.ModeCLI, cfg.Mode)
	assert.Equal(t, slog.LevelInfo, cfg.LogLevel)
	assert.Equal(t, 5, cfg.ShutdownTimeoutSec)
	assert.Empty(t, cfg.OTLPEndpoint)
	assert.False(t, cfg.DebugTrace)
	assert.Empty(t, cfg.ServiceVersion)
	assert.Empty(t, cfg.Environment)
}
