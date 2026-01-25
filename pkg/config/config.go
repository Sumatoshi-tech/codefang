package config

import (
	"fmt"
	"strings"
	"time"

	"github.com/spf13/viper"
)

// Config holds all configuration for the Codefang server
type Config struct {
	Server     ServerConfig     `mapstructure:"server"`
	Cache      CacheConfig      `mapstructure:"cache"`
	Analysis   AnalysisConfig   `mapstructure:"analysis"`
	Logging    LoggingConfig    `mapstructure:"logging"`
	Repository RepositoryConfig `mapstructure:"repository"`
}

// ServerConfig holds server-specific configuration
type ServerConfig struct {
	Enabled      bool          `mapstructure:"enabled"`
	Port         int           `mapstructure:"port"`
	Host         string        `mapstructure:"host"`
	ReadTimeout  time.Duration `mapstructure:"read_timeout"`
	WriteTimeout time.Duration `mapstructure:"write_timeout"`
	IdleTimeout  time.Duration `mapstructure:"idle_timeout"`
}

// CacheConfig holds cache-specific configuration
type CacheConfig struct {
	Enabled   bool          `mapstructure:"enabled"`
	Backend   string        `mapstructure:"backend"`
	Directory string        `mapstructure:"directory"`
	TTL       time.Duration `mapstructure:"ttl"`

	// S3 settings
	S3Bucket   string `mapstructure:"s3_bucket"`
	S3Region   string `mapstructure:"s3_region"`
	S3Endpoint string `mapstructure:"s3_endpoint"`
	S3Prefix   string `mapstructure:"s3_prefix"`

	// AWS credentials (optional)
	AWSAccessKeyID     string `mapstructure:"aws_access_key_id"`
	AWSSecretAccessKey string `mapstructure:"aws_secret_access_key"`

	// Legacy settings
	CleanupInterval time.Duration `mapstructure:"cleanup_interval"`
	MaxSize         string        `mapstructure:"max_size"`
}

// AnalysisConfig holds analysis-specific configuration
type AnalysisConfig struct {
	DefaultTickSize       int           `mapstructure:"default_tick_size"`
	DefaultGranularity    int           `mapstructure:"default_granularity"`
	DefaultSampling       int           `mapstructure:"default_sampling"`
	MaxConcurrentAnalyses int           `mapstructure:"max_concurrent_analyses"`
	Timeout               time.Duration `mapstructure:"timeout"`
}

// LoggingConfig holds logging-specific configuration
type LoggingConfig struct {
	Level  string `mapstructure:"level"`
	Format string `mapstructure:"format"`
	Output string `mapstructure:"output"`
}

// RepositoryConfig holds repository-specific configuration
type RepositoryConfig struct {
	CloneTimeout     time.Duration `mapstructure:"clone_timeout"`
	MaxFileSize      string        `mapstructure:"max_file_size"`
	AllowedProtocols []string      `mapstructure:"allowed_protocols"`
}

// LoadConfig loads configuration from file and environment variables
func LoadConfig(configPath string) (*Config, error) {
	v := viper.New()

	// Set defaults
	setDefaults(v)

	// Read config file
	if configPath != "" {
		v.SetConfigFile(configPath)
	} else {
		v.SetConfigName("config")
		v.SetConfigType("yaml")
		v.AddConfigPath(".")
		v.AddConfigPath("./config")
		v.AddConfigPath("/etc/codefang")
	}

	// Read environment variables
	v.SetEnvPrefix("CODEFANG")
	v.AutomaticEnv()
	v.SetEnvKeyReplacer(strings.NewReplacer(".", "_"))

	// Read config file
	if err := v.ReadInConfig(); err != nil {
		if _, ok := err.(viper.ConfigFileNotFoundError); !ok {
			return nil, fmt.Errorf("failed to read config file: %w", err)
		}
	}

	var config Config
	if err := v.Unmarshal(&config); err != nil {
		return nil, fmt.Errorf("failed to unmarshal config: %w", err)
	}

	// Validate configuration
	if err := validateConfig(&config); err != nil {
		return nil, fmt.Errorf("invalid configuration: %w", err)
	}

	return &config, nil
}

// setDefaults sets default configuration values
func setDefaults(v *viper.Viper) {
	// Server defaults
	v.SetDefault("server.enabled", false)
	v.SetDefault("server.port", 8080)
	v.SetDefault("server.host", "0.0.0.0")
	v.SetDefault("server.read_timeout", "30s")
	v.SetDefault("server.write_timeout", "30s")
	v.SetDefault("server.idle_timeout", "60s")

	// Cache defaults
	v.SetDefault("cache.enabled", true)
	v.SetDefault("cache.backend", "local")
	v.SetDefault("cache.directory", "/tmp/codefang-cache")
	v.SetDefault("cache.ttl", "24h")
	v.SetDefault("cache.cleanup_interval", "1h")
	v.SetDefault("cache.max_size", "10GB")

	// Analysis defaults
	v.SetDefault("analysis.default_tick_size", 24)
	v.SetDefault("analysis.default_granularity", 30)
	v.SetDefault("analysis.default_sampling", 30)
	v.SetDefault("analysis.max_concurrent_analyses", 10)
	v.SetDefault("analysis.timeout", "30m")

	// Logging defaults
	v.SetDefault("logging.level", "info")
	v.SetDefault("logging.format", "json")
	v.SetDefault("logging.output", "stdout")

	// Repository defaults
	v.SetDefault("repository.clone_timeout", "10m")
	v.SetDefault("repository.max_file_size", "1MB")
	v.SetDefault("repository.allowed_protocols", []string{"https", "http", "ssh", "git"})
}

// validateConfig validates the configuration
func validateConfig(config *Config) error {
	if config.Server.Port <= 0 || config.Server.Port > 65535 {
		return fmt.Errorf("invalid server port: %d", config.Server.Port)
	}

	if config.Analysis.MaxConcurrentAnalyses <= 0 {
		return fmt.Errorf("max concurrent analyses must be positive: %d", config.Analysis.MaxConcurrentAnalyses)
	}

	if config.Analysis.DefaultTickSize <= 0 {
		return fmt.Errorf("default tick size must be positive: %d", config.Analysis.DefaultTickSize)
	}

	if config.Analysis.DefaultGranularity <= 0 {
		return fmt.Errorf("default granularity must be positive: %d", config.Analysis.DefaultGranularity)
	}

	if config.Analysis.DefaultSampling <= 0 {
		return fmt.Errorf("default sampling must be positive: %d", config.Analysis.DefaultSampling)
	}

	return nil
}
