package config

import (
	"os"
	"testing"
	"time"
)

func TestLoadConfigDefaults(t *testing.T) {
	// Test loading with no config file (should use defaults)
	cfg, err := LoadConfig("")
	if err != nil {
		t.Fatalf("Failed to load config with defaults: %v", err)
	}

	// Check default values
	if cfg.Server.Port != 8080 {
		t.Errorf("Expected default port 8080, got %d", cfg.Server.Port)
	}

	if cfg.Server.Host != "0.0.0.0" {
		t.Errorf("Expected default host 0.0.0.0, got %s", cfg.Server.Host)
	}

	if cfg.Analysis.DefaultTickSize != 24 {
		t.Errorf("Expected default tick size 24, got %d", cfg.Analysis.DefaultTickSize)
	}

	if cfg.Analysis.MaxConcurrentAnalyses != 10 {
		t.Errorf("Expected default max concurrent analyses 10, got %d", cfg.Analysis.MaxConcurrentAnalyses)
	}
}

func TestLoadConfigFromFile(t *testing.T) {
	// Create a temporary config file
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

	tmpFile, err := os.CreateTemp("", "test-config-*.yaml")
	if err != nil {
		t.Fatalf("Failed to create temp file: %v", err)
	}
	defer os.Remove(tmpFile.Name())

	if _, err := tmpFile.WriteString(configContent); err != nil {
		t.Fatalf("Failed to write config content: %v", err)
	}
	tmpFile.Close()

	// Load config from file
	cfg, err := LoadConfig(tmpFile.Name())
	if err != nil {
		t.Fatalf("Failed to load config from file: %v", err)
	}

	// Check custom values
	if cfg.Server.Port != 9000 {
		t.Errorf("Expected port 9000, got %d", cfg.Server.Port)
	}

	if cfg.Server.Host != "127.0.0.1" {
		t.Errorf("Expected host 127.0.0.1, got %s", cfg.Server.Host)
	}

	if cfg.Analysis.DefaultTickSize != 12 {
		t.Errorf("Expected tick size 12, got %d", cfg.Analysis.DefaultTickSize)
	}

	if cfg.Analysis.MaxConcurrentAnalyses != 5 {
		t.Errorf("Expected max concurrent analyses 5, got %d", cfg.Analysis.MaxConcurrentAnalyses)
	}

	if cfg.Cache.Directory != "/tmp/test-cache" {
		t.Errorf("Expected cache directory /tmp/test-cache, got %s", cfg.Cache.Directory)
	}
}

func TestLoadConfigFromEnvironment(t *testing.T) {
	// Set environment variables
	os.Setenv("CODEFANG_SERVER_PORT", "9090")
	os.Setenv("CODEFANG_ANALYSIS_DEFAULT_TICK_SIZE", "6")
	os.Setenv("CODEFANG_CACHE_DIRECTORY", "/tmp/env-cache")
	defer func() {
		os.Unsetenv("CODEFANG_SERVER_PORT")
		os.Unsetenv("CODEFANG_ANALYSIS_DEFAULT_TICK_SIZE")
		os.Unsetenv("CODEFANG_CACHE_DIRECTORY")
	}()

	// Load config (should pick up environment variables)
	cfg, err := LoadConfig("")
	if err != nil {
		t.Fatalf("Failed to load config from environment: %v", err)
	}

	// Check environment variable values
	if cfg.Server.Port != 9090 {
		t.Errorf("Expected port 9090 from environment, got %d", cfg.Server.Port)
	}

	if cfg.Analysis.DefaultTickSize != 6 {
		t.Errorf("Expected tick size 6 from environment, got %d", cfg.Analysis.DefaultTickSize)
	}

	if cfg.Cache.Directory != "/tmp/env-cache" {
		t.Errorf("Expected cache directory /tmp/env-cache from environment, got %s", cfg.Cache.Directory)
	}
}

func TestValidateConfig(t *testing.T) {
	// Test valid configuration
	validConfig := &Config{
		Server: ServerConfig{
			Port: 8080,
		},
		Analysis: AnalysisConfig{
			MaxConcurrentAnalyses: 10,
			DefaultTickSize:       24,
			DefaultGranularity:    30,
			DefaultSampling:       30,
		},
	}

	if err := validateConfig(validConfig); err != nil {
		t.Errorf("Valid config should not return error: %v", err)
	}

	// Test invalid port
	invalidPortConfig := &Config{
		Server: ServerConfig{
			Port: 0, // Invalid port
		},
		Analysis: AnalysisConfig{
			MaxConcurrentAnalyses: 10,
			DefaultTickSize:       24,
			DefaultGranularity:    30,
			DefaultSampling:       30,
		},
	}

	if err := validateConfig(invalidPortConfig); err == nil {
		t.Error("Invalid port should return error")
	}

	// Test invalid max concurrent analyses
	invalidConcurrentConfig := &Config{
		Server: ServerConfig{
			Port: 8080,
		},
		Analysis: AnalysisConfig{
			MaxConcurrentAnalyses: 0, // Invalid
			DefaultTickSize:       24,
			DefaultGranularity:    30,
			DefaultSampling:       30,
		},
	}

	if err := validateConfig(invalidConcurrentConfig); err == nil {
		t.Error("Invalid max concurrent analyses should return error")
	}
}

func TestTimeDurationParsing(t *testing.T) {
	// Test that time durations are parsed correctly
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

	tmpFile, err := os.CreateTemp("", "test-duration-*.yaml")
	if err != nil {
		t.Fatalf("Failed to create temp file: %v", err)
	}
	defer os.Remove(tmpFile.Name())

	if _, err := tmpFile.WriteString(configContent); err != nil {
		t.Fatalf("Failed to write config content: %v", err)
	}
	tmpFile.Close()

	cfg, err := LoadConfig(tmpFile.Name())
	if err != nil {
		t.Fatalf("Failed to load config: %v", err)
	}

	// Check time durations
	expectedReadTimeout := 15 * time.Second
	if cfg.Server.ReadTimeout != expectedReadTimeout {
		t.Errorf("Expected read timeout %v, got %v", expectedReadTimeout, cfg.Server.ReadTimeout)
	}

	expectedWriteTimeout := 30 * time.Second
	if cfg.Server.WriteTimeout != expectedWriteTimeout {
		t.Errorf("Expected write timeout %v, got %v", expectedWriteTimeout, cfg.Server.WriteTimeout)
	}

	expectedIdleTimeout := 2 * time.Minute
	if cfg.Server.IdleTimeout != expectedIdleTimeout {
		t.Errorf("Expected idle timeout %v, got %v", expectedIdleTimeout, cfg.Server.IdleTimeout)
	}

	expectedCleanupInterval := 30 * time.Minute
	if cfg.Cache.CleanupInterval != expectedCleanupInterval {
		t.Errorf("Expected cleanup interval %v, got %v", expectedCleanupInterval, cfg.Cache.CleanupInterval)
	}

	expectedAnalysisTimeout := 1 * time.Hour
	if cfg.Analysis.Timeout != expectedAnalysisTimeout {
		t.Errorf("Expected analysis timeout %v, got %v", expectedAnalysisTimeout, cfg.Analysis.Timeout)
	}
}
