// Package main demonstrates basic Go language constructs
package main

import (
	"fmt"
	"os"
	"strconv"
	"strings"
)

import (
	"context"
	"time"
)

import _ "database/sql"
import . "math"
import mylog "log"

// Package-level constants
const (
	DefaultPort     = 8080
	MaxConnections  = 100
	ServiceName     = "go-parser-test"
	enableDebug     = true
)

const Version = "1.0.0"

// Package-level variables
var (
	globalCounter int
	serverConfig  map[string]string
	isInitialized bool
)

var appName = "test-app"

// Type aliases
type UserID int64
type UserName string
type ConfigMap map[string]interface{}

// Basic function with no parameters or return values
func initialize() {
	isInitialized = true
	globalCounter = 0
}

// Function with parameters and return value
func add(a, b int) int {
	return a + b
}

// Function with multiple return values
func divide(a, b float64) (float64, error) {
	if b == 0 {
		return 0, fmt.Errorf("division by zero")
	}
	return a / b, nil
}

// Function with named return values
func parseConfig(data string) (config ConfigMap, err error) {
	config = make(ConfigMap)
	// Implementation omitted
	return
}

// Function with variadic parameters
func concatenate(separator string, items ...string) string {
	return strings.Join(items, separator)
}

// Function with different parameter types
func processRequest(id UserID, name UserName, timeout time.Duration, ctx context.Context) error {
	// Implementation omitted
	return nil
}

// Exported function (starts with uppercase)
func PublicFunction() string {
	return "public"
}

// Unexported function (starts with lowercase)  
func privateFunction() string {
	return "private"
}

// Function that takes a function as parameter
func processItems(items []string, processor func(string) string) []string {
	result := make([]string, len(items))
	for i, item := range items {
		result[i] = processor(item)
	}
	return result
}

// Function that returns a function
func createMultiplier(factor int) func(int) int {
	return func(x int) int {
		return x * factor
	}
}

// Main function
func main() {
	initialize()
	
	// Local variables
	result := add(5, 3)
	quotient, err := divide(10.0, 3.0)
	
	if err != nil {
		fmt.Printf("Error: %v\n", err)
		os.Exit(1)
	}
	
	// Function calls
	config, _ := parseConfig("test=value")
	text := concatenate(",", "a", "b", "c")
	
	// Anonymous function
	square := func(x int) int {
		return x * x
	}
	
	multiplier := createMultiplier(2)
	
	fmt.Printf("Result: %d, Quotient: %f\n", result, quotient)
	fmt.Printf("Config: %v, Text: %s\n", config, text)
	fmt.Printf("Square of 5: %d\n", square(5))
	fmt.Printf("Double of 7: %d\n", multiplier(7))
	
	// Type conversion
	userID := UserID(123)
	userName := UserName("john_doe")
	
	err = processRequest(userID, userName, time.Second*30, context.Background())
	if err != nil {
		mylog.Printf("Failed to process request: %v", err)
	}
	
	// String conversion
	port := strconv.Itoa(DefaultPort)
	fmt.Printf("Server running on port: %s\n", port)
}