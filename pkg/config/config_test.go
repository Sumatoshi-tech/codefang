package config_test

import (
	"os"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/Sumatoshi-tech/codefang/pkg/config"
)

func TestLoadConfigDefaults(t *testing.T) {
	t.Parallel()

	// Test loading with no config file (should use defaults).
	cfg, err := config.LoadConfig("")
	require.NoError(t, err)

	// Check default values.
	assert.Equal(t, 8080, cfg.Server.Port)
	assert.Equal(t, "0.0.0.0", cfg.Server.Host)
	assert.Equal(t, 24, cfg.Analysis.DefaultTickSize)
	assert.Equal(t, 10, cfg.Analysis.MaxConcurrentAnalyses)
}

func TestLoadConfigFromFile(t *testing.T) {
	t.Parallel()

	// Create a temporary config file.
	configContent := `
server:
  port: 9000
  host: "127.0.0.1"

analysis:
  default_tick_size: 12
  max_concurrent_analyses: 5

cache:
  directory: "/tmp/test-cache"
`

	tmpDir := t.TempDir()

	tmpFile, err := os.CreateTemp(tmpDir, "test-config-*.yaml")
	require.NoError(t, err)

	_, writeErr := tmpFile.WriteString(configContent)
	require.NoError(t, writeErr)

	tmpFile.Close()

	// Load config from file.
	cfg, loadErr := config.LoadConfig(tmpFile.Name())
	require.NoError(t, loadErr)

	// Check custom values.
	assert.Equal(t, 9000, cfg.Server.Port)
	assert.Equal(t, "127.0.0.1", cfg.Server.Host)
	assert.Equal(t, 12, cfg.Analysis.DefaultTickSize)
	assert.Equal(t, 5, cfg.Analysis.MaxConcurrentAnalyses)
	assert.Equal(t, "/tmp/test-cache", cfg.Cache.Directory)
}

func TestLoadConfigFromEnvironment(t *testing.T) {
	// Set environment variables.
	t.Setenv("CODEFANG_SERVER_PORT", "9090")
	t.Setenv("CODEFANG_ANALYSIS_DEFAULT_TICK_SIZE", "6")
	t.Setenv("CODEFANG_CACHE_DIRECTORY", "/tmp/env-cache")

	// Load config (should pick up environment variables).
	cfg, err := config.LoadConfig("")
	require.NoError(t, err)

	// Check environment variable values.
	assert.Equal(t, 9090, cfg.Server.Port)
	assert.Equal(t, 6, cfg.Analysis.DefaultTickSize)
	assert.Equal(t, "/tmp/env-cache", cfg.Cache.Directory)
}

func TestValidateConfig(t *testing.T) {
	t.Parallel()

	// Test valid configuration.
	cfg, err := config.LoadConfig("")
	require.NoError(t, err)
	assert.NotNil(t, cfg)

	// Test that loading with all defaults passes validation.
	assert.Equal(t, 8080, cfg.Server.Port)
	assert.Equal(t, 10, cfg.Analysis.MaxConcurrentAnalyses)
	assert.Equal(t, 24, cfg.Analysis.DefaultTickSize)
	assert.Equal(t, 30, cfg.Analysis.DefaultGranularity)
	assert.Equal(t, 30, cfg.Analysis.DefaultSampling)
}

func TestTimeDurationParsing(t *testing.T) {
	t.Parallel()

	// Test that time durations are parsed correctly.
	configContent := `
server:
  read_timeout: "15s"
  write_timeout: "30s"
  idle_timeout: "2m"

cache:
  cleanup_interval: "30m"

analysis:
  timeout: "1h"
`

	tmpDir := t.TempDir()

	tmpFile, err := os.CreateTemp(tmpDir, "test-duration-*.yaml")
	require.NoError(t, err)

	_, writeErr := tmpFile.WriteString(configContent)
	require.NoError(t, writeErr)

	tmpFile.Close()

	cfg, loadErr := config.LoadConfig(tmpFile.Name())
	require.NoError(t, loadErr)

	// Check time durations.
	assert.Equal(t, 15*time.Second, cfg.Server.ReadTimeout)
	assert.Equal(t, 30*time.Second, cfg.Server.WriteTimeout)
	assert.Equal(t, 2*time.Minute, cfg.Server.IdleTimeout)
	assert.Equal(t, 30*time.Minute, cfg.Cache.CleanupInterval)
	assert.Equal(t, 1*time.Hour, cfg.Analysis.Timeout)
}
