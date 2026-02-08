//! Helper utilities for common operations
//!
//! This package demonstrates:
//! - Utility function implementations
//! - Generic programming patterns (Go 1.18+)
//! - Common data processing patterns
//! - Interface definitions and implementations
//! - Error handling utilities

package utils

import (
	"fmt"
	"math"
	"strings"
	"sync"
	"time"
)

// Package-level constants (exported)
const (
	ModuleName     = "utils"
	Version        = "1.0.0"
	MaxBatchSize   = 1000
	DefaultRetries = 3
	BackoffBaseMS  = 100
)

// Package-level variables
var (
	// Exported package variables
	DefaultProcessor *DataProcessor
	// unexported package variables
	processorCount int
	globalMutex    sync.RWMutex
)

// FormatOutput formats a message for output display
func FormatOutput(message string) string {
	timestamp := time.Now().Unix()
	return fmt.Sprintf("[%d] OUTPUT: %s", timestamp, message)
}

// ValidateInput validates input data (basic email-like validation)
func ValidateInput(data string) bool {
	return len(data) >= 5 && strings.Contains(data, "@")
}

// ValidateInputDetailed provides detailed input validation with error reporting
func ValidateInputDetailed(data string) error {
	if data == "" {
		return NewValidationError(ErrEmpty, "input cannot be empty")
	}

	if len(data) < 5 {
		return NewValidationError(ErrTooShort, fmt.Sprintf("input too short (minimum 5 characters, got %d)", len(data)))
	}

	if len(data) > 254 {
		return NewValidationError(ErrTooLong, fmt.Sprintf("input too long (maximum 254 characters, got %d)", len(data)))
	}

	if !strings.Contains(data, "@") {
		return NewValidationError(ErrInvalidFormat, "input must contain @ symbol")
	}

	return nil
}

// DataProcessor handles data transformation with configurable behavior
type DataProcessor struct {
	config         map[string]string
	processedCount int
	mutex          sync.RWMutex
}

// NewDataProcessor creates a new data processor with configuration
func NewDataProcessor(config map[string]string) *DataProcessor {
	globalMutex.Lock()
	processorCount++
	globalMutex.Unlock()

	if config == nil {
		config = make(map[string]string)
	}

	return &DataProcessor{
		config:         config,
		processedCount: 0,
	}
}

// Process processes input data according to configuration
func (dp *DataProcessor) Process(data string) string {
	dp.mutex.Lock()
	dp.processedCount++
	count := dp.processedCount
	dp.mutex.Unlock()

	mode := dp.GetConfig("transform", "standard")

	switch mode {
	case "uppercase":
		return strings.ToUpper(data)
	case "lowercase":
		return strings.ToLower(data)
	case "reverse":
		return reverseString(data)
	case "trim":
		return strings.TrimSpace(data)
	case "capitalize":
		return capitalizeWords(data)
	default:
		return fmt.Sprintf("processed(%d): %s", count, data)
	}
}

// ProcessBatch processes multiple items efficiently
func (dp *DataProcessor) ProcessBatch(items []string) []string {
	results := make([]string, len(items))

	// Process items in chunks for better performance
	chunkSize := MaxBatchSize
	if len(items) < chunkSize {
		chunkSize = len(items)
	}

	for i := 0; i < len(items); i += chunkSize {
		end := i + chunkSize
		if end > len(items) {
			end = len(items)
		}

		for j := i; j < end; j++ {
			results[j] = dp.Process(items[j])
		}
	}

	return results
}

// Stats returns processing statistics
func (dp *DataProcessor) Stats() ProcessingStats {
	dp.mutex.RLock()
	defer dp.mutex.RUnlock()

	return ProcessingStats{
		ProcessedCount: dp.processedCount,
		ConfigCount:    len(dp.config),
		Efficiency:     dp.calculateEfficiency(),
	}
}

// SetConfig updates processor configuration
func (dp *DataProcessor) SetConfig(key, value string) {
	dp.mutex.Lock()
	defer dp.mutex.Unlock()
	dp.config[key] = value
}

// GetConfig retrieves configuration value with optional default
func (dp *DataProcessor) GetConfig(key, defaultValue string) string {
	dp.mutex.RLock()
	defer dp.mutex.RUnlock()

	if value, exists := dp.config[key]; exists {
		return value
	}
	return defaultValue
}

// Reset resets processor state
func (dp *DataProcessor) Reset() {
	dp.mutex.Lock()
	defer dp.mutex.Unlock()
	dp.processedCount = 0
}

// Clone creates a copy of the processor
func (dp *DataProcessor) Clone() *DataProcessor {
	dp.mutex.RLock()
	defer dp.mutex.RUnlock()

	configCopy := make(map[string]string)
	for k, v := range dp.config {
		configCopy[k] = v
	}

	return &DataProcessor{
		config:         configCopy,
		processedCount: 0, // Reset count for new instance
	}
}

// IsMode checks if processor is configured for a specific mode
func (dp *DataProcessor) IsMode(mode string) bool {
	return dp.GetConfig("transform", "standard") == mode
}

// Private methods

func (dp *DataProcessor) calculateEfficiency() float64 {
	if len(dp.config) == 0 {
		return 0.0
	}
	return float64(dp.processedCount) / float64(len(dp.config))
}

// ProcessingStats holds statistics about data processing
type ProcessingStats struct {
	ProcessedCount int     `json:"processed_count"`
	ConfigCount    int     `json:"config_count"`
	Efficiency     float64 `json:"efficiency"`
}

// String implements fmt.Stringer interface
func (ps ProcessingStats) String() string {
	return fmt.Sprintf("ProcessingStats(processed: %d, config_entries: %d, efficiency: %.2f)",
		ps.ProcessedCount, ps.ConfigCount, ps.Efficiency)
}

// Generic utility functions

// SafeDivision performs safe division returning nil for division by zero
func SafeDivision[T ~float32 | ~float64](a, b T) *T {
	if b == 0 {
		return nil
	}
	result := a / b
	return &result
}

// RetryWithBackoff executes an operation with exponential backoff
func RetryWithBackoff[T any](
	operation func() (T, error),
	maxAttempts int,
	baseDelayMS int,
) (T, error) {
	var zero T

	for attempt := 1; attempt <= maxAttempts; attempt++ {
		result, err := operation()
		if err == nil {
			return result, nil
		}

		if attempt == maxAttempts {
			return zero, fmt.Errorf("operation failed after %d attempts: %w", maxAttempts, err)
		}

		// Calculate delay with exponential backoff
		delay := time.Duration(baseDelayMS*int(math.Pow(2, float64(attempt-1)))) * time.Millisecond
		fmt.Printf("Retry attempt %d after %v delay\n", attempt, delay)
		time.Sleep(delay)
	}

	return zero, fmt.Errorf("unexpected end of retry loop")
}

// FindDuplicates finds duplicate items in a slice
func FindDuplicates[T comparable](items []T) []T {
	seen := make(map[T]bool)
	duplicates := make([]T, 0)

	for _, item := range items {
		if seen[item] {
			// Only add to duplicates once
			found := false
			for _, dup := range duplicates {
				if dup == item {
					found = true
					break
				}
			}
			if !found {
				duplicates = append(duplicates, item)
			}
		} else {
			seen[item] = true
		}
	}

	return duplicates
}

// MergeMaps merges two maps, with the second taking precedence
func MergeMaps[K comparable, V any](map1, map2 map[K]V) map[K]V {
	result := make(map[K]V)

	// Copy first map
	for k, v := range map1 {
		result[k] = v
	}

	// Override with second map
	for k, v := range map2 {
		result[k] = v
	}

	return result
}

// String utility functions

// TruncateString truncates a string to specified length with ellipsis
func TruncateString(s string, maxLength int) string {
	if len(s) <= maxLength {
		return s
	}

	if maxLength <= 3 {
		return s[:maxLength]
	}

	return s[:maxLength-3] + "..."
}

// NormalizeWhitespace normalizes whitespace in a string
func NormalizeWhitespace(s string) string {
	fields := strings.Fields(s)
	return strings.Join(fields, " ")
}

// ExtractDomainFromEmail extracts domain part from email address
func ExtractDomainFromEmail(email string) (string, error) {
	parts := strings.Split(email, "@")
	if len(parts) != 2 {
		return "", fmt.Errorf("invalid email format: %s", email)
	}
	return strings.ToLower(parts[1]), nil
}

// capitalizeWords capitalizes the first letter of each word
func capitalizeWords(s string) string {
	words := strings.Fields(s)
	for i, word := range words {
		if len(word) > 0 {
			words[i] = strings.ToUpper(string(word[0])) + strings.ToLower(word[1:])
		}
	}
	return strings.Join(words, " ")
}

// reverseString reverses a string
func reverseString(s string) string {
	runes := []rune(s)
	for i, j := 0, len(runes)-1; i < j; i, j = i+1, j-1 {
		runes[i], runes[j] = runes[j], runes[i]
	}
	return string(runes)
}

// Package-level utility functions

// GetModuleInfo returns information about this module
func GetModuleInfo() string {
	globalMutex.RLock()
	defer globalMutex.RUnlock()
	return fmt.Sprintf("%s v%s (processors created: %d)", ModuleName, Version, processorCount)
}

// CreateDefaultProcessor creates a data processor with default configuration
func CreateDefaultProcessor() *DataProcessor {
	config := map[string]string{
		"transform": "standard",
		"encoding":  "utf8",
		"mode":      "production",
	}
	return NewDataProcessor(config)
}

// Validation error types
type ValidationErrorCode int

const (
	ErrEmpty ValidationErrorCode = iota
	ErrTooShort
	ErrTooLong
	ErrInvalidFormat
	ErrInvalidCharacters
)

// ValidationError represents input validation errors
type ValidationError struct {
	Code    ValidationErrorCode
	Message string
	Field   string
}

func NewValidationError(code ValidationErrorCode, message string) *ValidationError {
	return &ValidationError{
		Code:    code,
		Message: message,
	}
}

func NewValidationFieldError(code ValidationErrorCode, message, field string) *ValidationError {
	return &ValidationError{
		Code:    code,
		Message: message,
		Field:   field,
	}
}

func (e *ValidationError) Error() string {
	if e.Field != "" {
		return fmt.Sprintf("validation error [%d] in field '%s': %s", int(e.Code), e.Field, e.Message)
	}
	return fmt.Sprintf("validation error [%d]: %s", int(e.Code), e.Message)
}

// Is implements error comparison
func (e *ValidationError) Is(target error) bool {
	if other, ok := target.(*ValidationError); ok {
		return e.Code == other.Code
	}
	return false
}

// Package initialization
func init() {
	fmt.Println("[INIT] Utils package initialized")

	// Initialize default processor
	DefaultProcessor = CreateDefaultProcessor()
}
