// Package scoping demonstrates Go scoping rules and variable shadowing
package scoping

import (
	"fmt"
	"os"
	"strconv"
)

// Package-level declarations (package scope)
var globalVar = "global"
var packageCount int = 0

const MAX_RETRIES = 3
const debug = true

type Config struct {
	Name string
	Port int
}

type Logger interface {
	Log(message string)
}

// Package-level function
func Initialize() {
	globalVar = "initialized"
	packageCount = 1
}

// Function with parameters and local variables (function scope)
func ProcessData(input string, config Config) (string, error) {
	// Function parameters are in function scope: input, config
	
	// Local variables in function scope
	var result string
	var count int
	
	// Variable shadowing - shadows package-level debug
	debug := false
	if debug {
		fmt.Println("Debug mode is on")
	}
	
	// Short variable declaration creates new variable in function scope
	data := "processed: " + input
	
	// Block scope within if statement
	if len(input) > 0 {
		// New variable in block scope
		temp := strings.ToUpper(input)
		result = temp + "_PROCESSED"
		
		// Shadow function-scope count variable
		count := len(temp)
		fmt.Printf("Block count: %d\n", count)
		
		// Short variable declaration in block
		status := "valid"
		if status == "valid" {
			// Another nested block scope
			message := fmt.Sprintf("Processing %s", temp)
			fmt.Println(message)
		}
		// message is not accessible here (out of scope)
	}
	// temp is not accessible here (out of scope)
	
	// For loop creates its own block scope
	for i := 0; i < MAX_RETRIES; i++ {
		// i is only accessible within the for loop
		if i == 2 {
			break
		}
		
		// Inner for loop with its own scope
		for j := 0; j < 2; j++ {
			// j is only accessible within inner loop
			fmt.Printf("Iteration %d-%d\n", i, j)
		}
		// j is not accessible here
	}
	// i is not accessible here
	
	// Switch statement creates block scope
	switch config.Name {
	case "production":
		// Variables declared in case are in block scope
		logLevel := "error"
		fmt.Printf("Log level: %s\n", logLevel)
		
	case "development":
		// Another case with its own block scope
		logLevel := "debug"  // Different variable than above
		verbose := true
		if verbose {
			fmt.Printf("Verbose log level: %s\n", logLevel)
		}
		
	default:
		logLevel := "info"
		fmt.Printf("Default log level: %s\n", logLevel)
	}
	// logLevel and verbose are not accessible here
	
	// Range loop creates block scope
	items := []string{"a", "b", "c"}
	for index, value := range items {
		// index and value are in the range loop's block scope
		fmt.Printf("Item %d: %s\n", index, value)
		
		// Shadow the function-scope result variable
		result := fmt.Sprintf("item_%s", value)
		_ = result // Use to avoid unused variable error
	}
	// index and value are not accessible here
	
	// Type switch creates block scope
	var data interface{} = "test"
	switch v := data.(type) {
	case string:
		// v is a string in this case's block scope
		length := len(v)
		fmt.Printf("String length: %d\n", length)
		
	case int:
		// v is an int in this case's block scope  
		doubled := v * 2
		fmt.Printf("Doubled: %d\n", doubled)
	}
	// v, length, and doubled are not accessible here
	
	// Anonymous function creates its own function scope
	processor := func(text string) string {
		// text parameter is in anonymous function scope
		
		// Can access outer function variables (closure)
		prefix := config.Name + ": "
		
		// Local variable in anonymous function
		processed := prefix + text
		return processed
	}
	
	// Call the anonymous function
	final := processor(data)
	
	return final, nil
}

// Method with receiver (method receiver scope)
func (c Config) GetDisplayName() string {
	// c (receiver) is in method receiver scope
	
	// Can shadow package-level variables
	globalVar := "method_local"
	
	// Method-local variables
	prefix := "[CONFIG]"
	suffix := fmt.Sprintf(":%d", c.Port)
	
	return prefix + " " + c.Name + suffix + " " + globalVar
}

// Method with pointer receiver
func (c *Config) UpdatePort(newPort int) {
	// c (pointer receiver) is in method receiver scope
	
	// Validate in block scope
	if newPort > 0 && newPort < 65536 {
		// Block-scoped variable
		oldPort := c.Port
		c.Port = newPort
		
		// Nested block for logging
		if debug {  // Access package-level debug
			fmt.Printf("Port changed from %d to %d\n", oldPort, newPort)
		}
	}
}

// Function demonstrating variable shadowing at multiple levels
func DemonstrateScoping() {
	fmt.Println("=== Go Scoping Demonstration ===")
	
	// Shadow package-level globalVar
	globalVar := "function_level"
	fmt.Printf("Function level globalVar: %s\n", globalVar)
	
	// Block scope level 1
	{
		// Shadow function-level globalVar
		globalVar := "block_level_1" 
		fmt.Printf("Block level 1 globalVar: %s\n", globalVar)
		
		// Block scope level 2
		{
			// Shadow block-level globalVar
			globalVar := "block_level_2"
			fmt.Printf("Block level 2 globalVar: %s\n", globalVar)
			
			// Access package-level variable by avoiding shadowing
			fmt.Printf("Package level packageCount: %d\n", packageCount)
		}
		
		// Back to block level 1
		fmt.Printf("Back to block level 1 globalVar: %s\n", globalVar)
	}
	
	// Back to function level
	fmt.Printf("Back to function level globalVar: %s\n", globalVar)
	
	// Access package-level by avoiding local shadowing
	packageCount++
	fmt.Printf("Updated package level packageCount: %d\n", packageCount)
}

// Function with complex control flow and scoping
func ComplexScoping(numbers []int) {
	// Function parameters and variables
	sum := 0
	average := 0.0
	
	// For loop with range
	for i, num := range numbers {
		// i and num are in for loop block scope
		sum += num
		
		// Nested if with block scope
		if num > 10 {
			// Block-scoped variables
			factor := 2
			adjusted := num * factor
			sum += adjusted
			
			fmt.Printf("Index %d: %d adjusted to %d\n", i, num, adjusted)
		}
		
		// Select statement for demonstration (would need channels in real code)
		switch {
		case num < 0:
			// Case block scope
			errorMsg := fmt.Sprintf("Negative number at index %d: %d", i, num)
			fmt.Println(errorMsg)
			
		case num == 0:
			// Another case block scope
			warningMsg := "Zero value encountered"
			fmt.Println(warningMsg)
		}
		// errorMsg and warningMsg are not accessible here
	}
	
	// Calculate average (i and num from range loop not accessible here)
	if len(numbers) > 0 {
		// Block scope for average calculation
		count := len(numbers)
		average = float64(sum) / float64(count)
		
		fmt.Printf("Sum: %d, Average: %.2f\n", sum, average)
	}
}

// Function that returns a closure demonstrating capture of local variables
func CreateCounter(start int) func() int {
	// start parameter is in function scope
	count := start  // Local variable in function scope
	
	// Return anonymous function that captures count
	return func() int {
		// This anonymous function has access to count from outer scope
		count++      // Modifies the captured variable
		return count
	}
}

// Main function to demonstrate all scoping rules
func main() {
	fmt.Println("Go Scoping Examples")
	
	// Package-level access
	Initialize()
	
	// Function scope
	config := Config{Name: "test", Port: 8080}
	result, err := ProcessData("hello", config)
	
	if err != nil {
		fmt.Printf("Error: %v\n", err)
		os.Exit(1)
	}
	
	fmt.Printf("Result: %s\n", result)
	
	// Method calls (receiver scope)
	displayName := config.GetDisplayName()
	fmt.Printf("Display name: %s\n", displayName)
	
	config.UpdatePort(9090)
	
	// Demonstrate scoping
	DemonstrateScoping()
	
	// Complex scoping
	numbers := []int{1, 15, -3, 0, 22, 8}
	ComplexScoping(numbers)
	
	// Closure example
	counter := CreateCounter(10)
	for i := 0; i < 3; i++ {
		count := counter()
		fmt.Printf("Counter: %d\n", count)
	}
	
	// Final package-level variable access
	fmt.Printf("Final package count: %d\n", packageCount)
	fmt.Printf("Final global var: %s\n", globalVar)
}