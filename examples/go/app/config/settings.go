//! Application settings and configuration management
//!
//! This package demonstrates:
//! - Configuration struct design
//! - Environment variable integration
//! - Validation patterns
//! - Default value strategies
//! - Nested configuration structures

package config

import (
	"fmt"
	"os"
	"strconv"
	"strings"
	"time"
)

// Package-level constants
const (
	DefaultConfigFile = "app.yaml"
	EnvPrefix         = "APP_"
	DefaultHost       = "localhost"
	DefaultPort       = 8080
	DefaultDBURL      = "sqlite://app.db"
)

// Settings represents the main application configuration
type Settings struct {
	Server   ServerConfig   `json:"server" yaml:"server"`
	Database DatabaseConfig `json:"database" yaml:"database"`
	Logging  LoggingConfig  `json:"logging" yaml:"logging"`
	Features FeatureFlags   `json:"features" yaml:"features"`

	// Private fields for internal state
	loaded  bool
	envVars map[string]string
}

// NewSettings creates a new Settings instance with defaults
func NewSettings() *Settings {
	return &Settings{
		Server:   DefaultServerConfig(),
		Database: DefaultDatabaseConfig(),
		Logging:  DefaultLoggingConfig(),
		Features: DefaultFeatureFlags(),
		loaded:   false,
		envVars:  make(map[string]string),
	}
}

// LoadFromEnv loads configuration from environment variables
func (s *Settings) LoadFromEnv() error {
	// Server configuration
	if host := os.Getenv(EnvPrefix + "SERVER_HOST"); host != "" {
		s.Server.Host = host
	}

	if portStr := os.Getenv(EnvPrefix + "SERVER_PORT"); portStr != "" {
		port, err := strconv.Atoi(portStr)
		if err != nil {
			return NewConfigError(ErrInvalidValue, fmt.Sprintf("invalid port: %s", portStr))
		}
		s.Server.Port = port
	}

	if timeoutStr := os.Getenv(EnvPrefix + "SERVER_TIMEOUT"); timeoutStr != "" {
		timeout, err := time.ParseDuration(timeoutStr)
		if err != nil {
			return NewConfigError(ErrInvalidValue, fmt.Sprintf("invalid timeout: %s", timeoutStr))
		}
		s.Server.Timeout = timeout
	}

	// Database configuration
	if dbURL := os.Getenv(EnvPrefix + "DATABASE_URL"); dbURL != "" {
		s.Database.URL = dbURL
	}

	if maxConnStr := os.Getenv(EnvPrefix + "DATABASE_MAX_CONNECTIONS"); maxConnStr != "" {
		maxConn, err := strconv.Atoi(maxConnStr)
		if err != nil {
			return NewConfigError(ErrInvalidValue, fmt.Sprintf("invalid max connections: %s", maxConnStr))
		}
		s.Database.MaxConnections = maxConn
	}

	// Logging configuration
	if logLevel := os.Getenv(EnvPrefix + "LOG_LEVEL"); logLevel != "" {
		level, err := ParseLogLevel(logLevel)
		if err != nil {
			return err
		}
		s.Logging.Level = level
	}

	if logFile := os.Getenv(EnvPrefix + "LOG_FILE"); logFile != "" {
		s.Logging.FilePath = &logFile
	}

	// Feature flags
	if metricsEnabled := os.Getenv(EnvPrefix + "FEATURE_METRICS"); metricsEnabled != "" {
		s.Features.EnableMetrics = strings.ToLower(metricsEnabled) == "true"
	}

	if tracingEnabled := os.Getenv(EnvPrefix + "FEATURE_TRACING"); tracingEnabled != "" {
		s.Features.EnableTracing = strings.ToLower(tracingEnabled) == "true"
	}

	s.loaded = true
	return nil
}

// Validate validates all configuration settings
func (s *Settings) Validate() error {
	if err := s.Server.Validate(); err != nil {
		return fmt.Errorf("server config error: %w", err)
	}

	if err := s.Database.Validate(); err != nil {
		return fmt.Errorf("database config error: %w", err)
	}

	if err := s.Logging.Validate(); err != nil {
		return fmt.Errorf("logging config error: %w", err)
	}

	return nil
}

// DatabaseURL returns the database connection URL
func (s *Settings) DatabaseURL() string {
	return s.Database.URL
}

// ServerAddress returns the complete server address
func (s *Settings) ServerAddress() string {
	return fmt.Sprintf("%s:%d", s.Server.Host, s.Server.Port)
}

// IsLoaded returns whether configuration has been loaded from environment
func (s *Settings) IsLoaded() bool {
	return s.loaded
}

// ServerConfig holds server-related configuration
type ServerConfig struct {
	Host           string        `json:"host" yaml:"host"`
	Port           int           `json:"port" yaml:"port"`
	Timeout        time.Duration `json:"timeout" yaml:"timeout"`
	MaxConnections int           `json:"max_connections" yaml:"max_connections"`
	TLSEnabled     bool          `json:"tls_enabled" yaml:"tls_enabled"`
}

// DefaultServerConfig returns default server configuration
func DefaultServerConfig() ServerConfig {
	return ServerConfig{
		Host:           DefaultHost,
		Port:           DefaultPort,
		Timeout:        30 * time.Second,
		MaxConnections: 100,
		TLSEnabled:     false,
	}
}

// Validate validates server configuration
func (sc *ServerConfig) Validate() error {
	if sc.Host == "" {
		return NewConfigError(ErrMissingRequired, "server host is required")
	}

	if sc.Port <= 0 || sc.Port > 65535 {
		return NewConfigError(ErrInvalidValue, "server port must be between 1 and 65535")
	}

	if sc.Timeout <= 0 {
		return NewConfigError(ErrInvalidValue, "server timeout must be positive")
	}

	if sc.MaxConnections <= 0 {
		return NewConfigError(ErrInvalidValue, "max connections must be positive")
	}

	return nil
}

// DatabaseConfig holds database-related configuration
type DatabaseConfig struct {
	URL               string        `json:"url" yaml:"url"`
	MaxConnections    int           `json:"max_connections" yaml:"max_connections"`
	ConnectionTimeout time.Duration `json:"connection_timeout" yaml:"connection_timeout"`
	QueryTimeout      time.Duration `json:"query_timeout" yaml:"query_timeout"`
	SSL               SSLConfig     `json:"ssl" yaml:"ssl"`
}

// DefaultDatabaseConfig returns default database configuration
func DefaultDatabaseConfig() DatabaseConfig {
	return DatabaseConfig{
		URL:               DefaultDBURL,
		MaxConnections:    10,
		ConnectionTimeout: 5 * time.Second,
		QueryTimeout:      30 * time.Second,
		SSL:               DefaultSSLConfig(),
	}
}

// Validate validates database configuration
func (dc *DatabaseConfig) Validate() error {
	if dc.URL == "" {
		return NewConfigError(ErrMissingRequired, "database URL is required")
	}

	if !strings.Contains(dc.URL, "://") {
		return NewConfigError(ErrInvalidValue, "database URL must include protocol")
	}

	if dc.MaxConnections <= 0 {
		return NewConfigError(ErrInvalidValue, "max connections must be positive")
	}

	if dc.ConnectionTimeout <= 0 {
		return NewConfigError(ErrInvalidValue, "connection timeout must be positive")
	}

	return nil
}

// SSLConfig holds SSL/TLS configuration for database
type SSLConfig struct {
	Enabled  bool   `json:"enabled" yaml:"enabled"`
	CertFile string `json:"cert_file" yaml:"cert_file"`
	KeyFile  string `json:"key_file" yaml:"key_file"`
	CAFile   string `json:"ca_file" yaml:"ca_file"`
}

// DefaultSSLConfig returns default SSL configuration
func DefaultSSLConfig() SSLConfig {
	return SSLConfig{
		Enabled: false,
	}
}

// LoggingConfig holds logging-related configuration
type LoggingConfig struct {
	Level    LogLevel `json:"level" yaml:"level"`
	FilePath *string  `json:"file_path" yaml:"file_path"`
	Console  bool     `json:"console" yaml:"console"`
	Format   string   `json:"format" yaml:"format"`
}

// DefaultLoggingConfig returns default logging configuration
func DefaultLoggingConfig() LoggingConfig {
	return LoggingConfig{
		Level:   LogLevelInfo,
		Console: true,
		Format:  "json",
	}
}

// Validate validates logging configuration
func (lc *LoggingConfig) Validate() error {
	if !lc.Level.IsValid() {
		return NewConfigError(ErrInvalidValue, "invalid log level")
	}

	if lc.Format != "json" && lc.Format != "text" {
		return NewConfigError(ErrInvalidValue, "log format must be 'json' or 'text'")
	}

	return nil
}

// LogLevel represents logging levels
type LogLevel int

const (
	LogLevelDebug LogLevel = iota
	LogLevelInfo
	LogLevelWarn
	LogLevelError
)

// String returns string representation of log level
func (ll LogLevel) String() string {
	switch ll {
	case LogLevelDebug:
		return "debug"
	case LogLevelInfo:
		return "info"
	case LogLevelWarn:
		return "warn"
	case LogLevelError:
		return "error"
	default:
		return "unknown"
	}
}

// IsValid checks if log level is valid
func (ll LogLevel) IsValid() bool {
	return ll >= LogLevelDebug && ll <= LogLevelError
}

// ParseLogLevel parses a string into LogLevel
func ParseLogLevel(level string) (LogLevel, error) {
	switch strings.ToLower(level) {
	case "debug":
		return LogLevelDebug, nil
	case "info":
		return LogLevelInfo, nil
	case "warn", "warning":
		return LogLevelWarn, nil
	case "error":
		return LogLevelError, nil
	default:
		return LogLevelInfo, NewConfigError(ErrInvalidValue, fmt.Sprintf("invalid log level: %s", level))
	}
}

// FeatureFlags holds feature toggle configuration
type FeatureFlags struct {
	EnableMetrics        bool `json:"enable_metrics" yaml:"enable_metrics"`
	EnableTracing        bool `json:"enable_tracing" yaml:"enable_tracing"`
	ExperimentalFeatures bool `json:"experimental_features" yaml:"experimental_features"`
	MaintenanceMode      bool `json:"maintenance_mode" yaml:"maintenance_mode"`
}

// DefaultFeatureFlags returns default feature flags
func DefaultFeatureFlags() FeatureFlags {
	return FeatureFlags{
		EnableMetrics:        false,
		EnableTracing:        false,
		ExperimentalFeatures: false,
		MaintenanceMode:      false,
	}
}

// Package-level utility functions

// GetEnvWithDefault returns environment variable value or default
func GetEnvWithDefault(key, defaultValue string) string {
	if value := os.Getenv(key); value != "" {
		return value
	}
	return defaultValue
}

// ParseDurationFromString parses duration from string with validation
func ParseDurationFromString(durationStr string) (time.Duration, error) {
	if durationStr == "" {
		return 0, NewConfigError(ErrInvalidValue, "duration cannot be empty")
	}

	duration, err := time.ParseDuration(durationStr)
	if err != nil {
		return 0, NewConfigError(ErrInvalidValue, fmt.Sprintf("invalid duration: %s", durationStr))
	}

	return duration, nil
}

// LoadFromFile loads configuration from a file (placeholder implementation)
func LoadFromFile(filePath string) (*Settings, error) {
	if filePath == "" {
		return nil, NewConfigError(ErrInvalidValue, "file path cannot be empty")
	}

	// In a real implementation, this would parse YAML/JSON/TOML
	settings := NewSettings()
	fmt.Printf("Loading configuration from file: %s\n", filePath)

	return settings, nil
}

// MergeSettings merges two settings, with the second taking precedence
func MergeSettings(base, override *Settings) *Settings {
	result := *base // Copy base

	// Merge server config
	if override.Server.Host != DefaultHost {
		result.Server.Host = override.Server.Host
	}
	if override.Server.Port != DefaultPort {
		result.Server.Port = override.Server.Port
	}

	// Merge database config
	if override.Database.URL != DefaultDBURL {
		result.Database.URL = override.Database.URL
	}

	// Merge other configs...

	return &result
}

// Error types and constants
type ConfigErrorCode int

const (
	ErrMissingRequired ConfigErrorCode = iota
	ErrInvalidValue
	ErrFileNotFound
	ErrParseError
	ErrValidationFailed
)

// ConfigError represents configuration-related errors
type ConfigError struct {
	Code    ConfigErrorCode
	Message string
	Field   string
}

func NewConfigError(code ConfigErrorCode, message string) *ConfigError {
	return &ConfigError{
		Code:    code,
		Message: message,
	}
}

func NewConfigFieldError(code ConfigErrorCode, message, field string) *ConfigError {
	return &ConfigError{
		Code:    code,
		Message: message,
		Field:   field,
	}
}

func (e *ConfigError) Error() string {
	if e.Field != "" {
		return fmt.Sprintf("config error [%d] in field '%s': %s", int(e.Code), e.Field, e.Message)
	}
	return fmt.Sprintf("config error [%d]: %s", int(e.Code), e.Message)
}

// Is implements error comparison
func (e *ConfigError) Is(target error) bool {
	if other, ok := target.(*ConfigError); ok {
		return e.Code == other.Code
	}
	return false
}

// Package initialization
func init() {
	fmt.Println("[INIT] Config package initialized")
}
