// Package config provides configuration loading and validation for the Codefang server.
package config

import (
	"errors"
	"fmt"
	"strings"
	"time"

	"github.com/spf13/viper"
)

// Sentinel validation errors.
var (
	ErrInvalidPort        = errors.New("invalid server port")
	ErrInvalidConcurrent  = errors.New("max concurrent analyses must be positive")
	ErrInvalidTickSize    = errors.New("default tick size must be positive")
	ErrInvalidGranularity = errors.New("default granularity must be positive")
	ErrInvalidSampling    = errors.New("default sampling must be positive")
)

// Default configuration values.
const (
	defaultPort          = 8080
	defaultHost          = "0.0.0.0"
	defaultTickSize      = 24
	defaultGranularity   = 30
	defaultSampling      = 30
	defaultMaxConcurrent = 10
	maxPort              = 65535
)

// Config holds all configuration for the Codefang server.
type Config struct {
	Server     ServerConfig     `mapstructure:"server"`
	Cache      CacheConfig      `mapstructure:"cache"`
	Analysis   AnalysisConfig   `mapstructure:"analysis"`
	Logging    LoggingConfig    `mapstructure:"logging"`
	Repository RepositoryConfig `mapstructure:"repository"`
}

// ServerConfig holds server-specific configuration.
type ServerConfig struct {
	Host         string        `mapstructure:"host"`
	ReadTimeout  time.Duration `mapstructure:"read_timeout"`
	WriteTimeout time.Duration `mapstructure:"write_timeout"`
	IdleTimeout  time.Duration `mapstructure:"idle_timeout"`
	Port         int           `mapstructure:"port"`
	Enabled      bool          `mapstructure:"enabled"`
}

// CacheConfig holds cache-specific configuration.
type CacheConfig struct {
	// Core settings.
	Backend   string `mapstructure:"backend"`
	Directory string `mapstructure:"directory"`

	// S3 settings.
	S3Bucket   string `mapstructure:"s3_bucket"`
	S3Region   string `mapstructure:"s3_region"`
	S3Endpoint string `mapstructure:"s3_endpoint"`
	S3Prefix   string `mapstructure:"s3_prefix"`

	// AWS credentials (optional).
	AWSAccessKeyID     string `mapstructure:"aws_access_key_id"`
	AWSSecretAccessKey string `mapstructure:"aws_secret_access_key"`

	// Legacy settings.
	MaxSize         string        `mapstructure:"max_size"`
	TTL             time.Duration `mapstructure:"ttl"`
	CleanupInterval time.Duration `mapstructure:"cleanup_interval"`
	Enabled         bool          `mapstructure:"enabled"`
}

// AnalysisConfig holds analysis-specific configuration.
type AnalysisConfig struct {
	Timeout               time.Duration `mapstructure:"timeout"`
	DefaultTickSize       int           `mapstructure:"default_tick_size"`
	DefaultGranularity    int           `mapstructure:"default_granularity"`
	DefaultSampling       int           `mapstructure:"default_sampling"`
	MaxConcurrentAnalyses int           `mapstructure:"max_concurrent_analyses"`
}

// LoggingConfig holds logging-specific configuration.
type LoggingConfig struct {
	Level  string `mapstructure:"level"`
	Format string `mapstructure:"format"`
	Output string `mapstructure:"output"`
}

// RepositoryConfig holds repository-specific configuration.
type RepositoryConfig struct {
	MaxFileSize      string        `mapstructure:"max_file_size"`
	AllowedProtocols []string      `mapstructure:"allowed_protocols"`
	CloneTimeout     time.Duration `mapstructure:"clone_timeout"`
}

// LoadConfig loads configuration from file and environment variables.
func LoadConfig(configPath string) (*Config, error) {
	viperCfg := viper.New()

	// Set defaults.
	setDefaults(viperCfg)

	// Read config file.
	if configPath != "" {
		viperCfg.SetConfigFile(configPath)
	} else {
		viperCfg.SetConfigName("config")
		viperCfg.SetConfigType("yaml")
		viperCfg.AddConfigPath(".")
		viperCfg.AddConfigPath("./config")
		viperCfg.AddConfigPath("/etc/codefang")
	}

	// Read environment variables.
	viperCfg.SetEnvPrefix("CODEFANG")
	viperCfg.AutomaticEnv()
	viperCfg.SetEnvKeyReplacer(strings.NewReplacer(".", "_"))

	// Read config file.
	readErr := viperCfg.ReadInConfig()
	if readErr != nil {
		var notFoundErr viper.ConfigFileNotFoundError
		if !errors.As(readErr, &notFoundErr) {
			return nil, fmt.Errorf("failed to read config file: %w", readErr)
		}
	}

	var config Config

	unmarshalErr := viperCfg.Unmarshal(&config)
	if unmarshalErr != nil {
		return nil, fmt.Errorf("failed to unmarshal config: %w", unmarshalErr)
	}

	validateErr := validateConfig(&config)
	if validateErr != nil {
		return nil, fmt.Errorf("invalid configuration: %w", validateErr)
	}

	return &config, nil
}

// setDefaults sets default configuration values.
func setDefaults(viperCfg *viper.Viper) {
	// Server defaults.
	viperCfg.SetDefault("server.enabled", false)
	viperCfg.SetDefault("server.port", defaultPort)
	viperCfg.SetDefault("server.host", defaultHost)
	viperCfg.SetDefault("server.read_timeout", "30s")
	viperCfg.SetDefault("server.write_timeout", "30s")
	viperCfg.SetDefault("server.idle_timeout", "60s")

	// Cache defaults.
	viperCfg.SetDefault("cache.enabled", true)
	viperCfg.SetDefault("cache.backend", "local")
	viperCfg.SetDefault("cache.directory", "/tmp/codefang-cache")
	viperCfg.SetDefault("cache.ttl", "24h")
	viperCfg.SetDefault("cache.cleanup_interval", "1h")
	viperCfg.SetDefault("cache.max_size", "10GB")

	// Analysis defaults.
	viperCfg.SetDefault("analysis.default_tick_size", defaultTickSize)
	viperCfg.SetDefault("analysis.default_granularity", defaultGranularity)
	viperCfg.SetDefault("analysis.default_sampling", defaultSampling)
	viperCfg.SetDefault("analysis.max_concurrent_analyses", defaultMaxConcurrent)
	viperCfg.SetDefault("analysis.timeout", "30m")

	// Logging defaults.
	viperCfg.SetDefault("logging.level", "info")
	viperCfg.SetDefault("logging.format", "json")
	viperCfg.SetDefault("logging.output", "stdout")

	// Repository defaults.
	viperCfg.SetDefault("repository.clone_timeout", "10m")
	viperCfg.SetDefault("repository.max_file_size", "1MB")
	viperCfg.SetDefault("repository.allowed_protocols", []string{"https", "http", "ssh", "git"})
}

// validateConfig validates the configuration.
func validateConfig(config *Config) error {
	if config.Server.Port <= 0 || config.Server.Port > maxPort {
		return fmt.Errorf("%w: %d", ErrInvalidPort, config.Server.Port)
	}

	if config.Analysis.MaxConcurrentAnalyses <= 0 {
		return fmt.Errorf("%w: %d", ErrInvalidConcurrent, config.Analysis.MaxConcurrentAnalyses)
	}

	if config.Analysis.DefaultTickSize <= 0 {
		return fmt.Errorf("%w: %d", ErrInvalidTickSize, config.Analysis.DefaultTickSize)
	}

	if config.Analysis.DefaultGranularity <= 0 {
		return fmt.Errorf("%w: %d", ErrInvalidGranularity, config.Analysis.DefaultGranularity)
	}

	if config.Analysis.DefaultSampling <= 0 {
		return fmt.Errorf("%w: %d", ErrInvalidSampling, config.Analysis.DefaultSampling)
	}

	return nil
}
