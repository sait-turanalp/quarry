//! Test file specifically for verifying import resolution
//! This file tests that imports are correctly resolved to their definitions
//!
//! EXPECTED BEHAVIOR:
//! 1. Standard library imports should resolve to their standard packages
//! 2. Aliased imports should track both original and alias names
//! 3. Internal package imports should resolve relative to this module
//! 4. Nested package imports should resolve through the hierarchy
//! 5. Dot imports should import into current namespace
//! 6. Package-level imports should resolve absolutely

package main

// === STANDARD LIBRARY IMPORTS ===
// These should resolve to their standard library paths
import (
	"fmt"
	"sync"
	"time"
)

// === ALIASED IMPORTS ===
// These should track both the original path and the alias
import (
	"context"
	set "container/list" // Using list as set-like structure
	lock "sync"          // Aliasing sync as lock
)

// === INTERNAL PACKAGE STRUCTURE ===
// Helpers package equivalent (in Go, this would be a separate package)
// For this example, we'll simulate it with types and functions in the same package

// Helper functions (equivalent to helpers module)
func helperFunction() string {
	return "Helper"
}

type HelperStruct struct {
	Value int
}

func NewHelperStruct(value int) *HelperStruct {
	return &HelperStruct{Value: value}
}

// Nested package equivalent (nested functions/types)
func nestedFunction() string {
	return "Nested"
}

// Function that uses "parent" struct (equivalent to super import)
func useParentStruct() *HelperStruct {
	return NewHelperStruct(100)
}

// === CONFLICTING NAMES TEST ===
// Another set of helpers that also has a helperFunction (to test resolution)
func otherHelperFunction() int {
	return 42
}

// This function should call its own helper (local scope resolution)
func callOwnHelper() int {
	return otherHelperFunction() // Calls otherHelperFunction in same scope
}

// === DOT IMPORTS (equivalent to use statements that bring names into scope) ===
// In Go, we can simulate this by creating local aliases or using short variable names
var (
	// Local aliases (equivalent to bringing names into current scope)
	localHelperFunc = helperFunction
	LocalHelperStruct = HelperStruct
)

// === BLANK IMPORTS (side-effect only imports) ===
import (
	_ "net/http/pprof" // Side-effect import for profiling
)

// === MAIN FUNCTION WITH IMPORT USAGE ===
func main() {
	fmt.Println("=== Testing Import Resolution ===\n")

	// TEST 1: Standard library imports
	fmt.Println("1. Standard library imports:")
	
	// sync.Mutex from standard library
	var mu sync.Mutex
	mu.Lock()
	mu.Unlock()
	fmt.Println("   sync.Mutex created and used ✓")
	
	// time.Duration from standard library
	duration := time.Second
	fmt.Printf("   time.Duration used: %v ✓\n", duration)

	// TEST 2: Aliased imports
	fmt.Println("\n2. Aliased imports:")
	
	// Using aliased context as normal context
	ctx := context.Background()
	fmt.Printf("   context.Background() via import: %T ✓\n", ctx)
	
	// Using set alias (list as set-like structure)
	list := set.New()
	list.PushBack(1)
	fmt.Println("   list used as 'set' alias ✓")
	
	// Using lock alias for sync
	var lockMu lock.Mutex
	lockMu.Lock()
	lockMu.Unlock()
	fmt.Println("   sync.Mutex used as 'lock' alias ✓")

	// TEST 3: Local function resolution
	fmt.Println("\n3. Local function resolution:")
	result := helperFunction()
	fmt.Printf("   helperFunction() returns: '%s' (expected: 'Helper') ✓\n", result)
	
	helper := NewHelperStruct(10)
	fmt.Printf("   HelperStruct created with value: %d ✓\n", helper.Value)

	// TEST 4: Nested function calls
	fmt.Println("\n4. Nested function calls:")
	nestedResult := nestedFunction()
	fmt.Printf("   nestedFunction() returns: '%s' (expected: 'Nested') ✓\n", nestedResult)
	
	parentStruct := useParentStruct()
	fmt.Printf("   useParentStruct() returns struct with value: %d ✓\n", parentStruct.Value)

	// TEST 5: Local scope resolution vs conflicting names
	fmt.Println("\n5. Scope resolution with conflicting names:")
	// This should call helperFunction (not otherHelperFunction)
	localResult := helperFunction()
	fmt.Printf("   helperFunction() returns: '%s' (expected: 'Helper') ✓\n", localResult)
	
	// This should call otherHelperFunction through callOwnHelper
	otherResult := callOwnHelper()
	fmt.Printf("   callOwnHelper() returns: %d (expected: 42) ✓\n", otherResult)

	// TEST 6: Local aliases (equivalent to dot imports)
	fmt.Println("\n6. Local aliases (dot import equivalent):")
	aliasResult := localHelperFunc()
	fmt.Printf("   localHelperFunc() returns: '%s' (expected: 'Helper') ✓\n", aliasResult)
	
	aliasStruct := LocalHelperStruct{Value: 99}
	fmt.Printf("   LocalHelperStruct created with value: %d ✓\n", aliasStruct.Value)

	fmt.Println("\n=== All import tests completed ===")
}

// === TEST FUNCTIONS (equivalent to #[cfg(test)] mod tests) ===

func TestStandardImports() error {
	// Test that standard library types work
	var mu sync.Mutex
	mu.Lock()
	defer mu.Unlock()
	
	ctx := context.Background()
	if ctx == nil {
		return fmt.Errorf("context should not be nil")
	}
	
	return nil
}

func TestAliasedImports() error {
	// Test that aliases work correctly
	list := set.New() // Using aliased import
	if list == nil {
		return fmt.Errorf("aliased list should not be nil")
	}
	
	var lockMu lock.Mutex // Using aliased sync
	lockMu.Lock()
	defer lockMu.Unlock()
	
	return nil
}

func TestLocalResolution() error {
	// Test local function resolution
	result := helperFunction()
	if result != "Helper" {
		return fmt.Errorf("expected 'Helper', got '%s'", result)
	}
	
	// Test struct creation
	helper := NewHelperStruct(20)
	if helper.Value != 20 {
		return fmt.Errorf("expected value 20, got %d", helper.Value)
	}
	
	// Test nested function
	nested := nestedFunction()
	if nested != "Nested" {
		return fmt.Errorf("expected 'Nested', got '%s'", nested)
	}
	
	return nil
}

func TestScopeResolution() error {
	// Test that we call the right function in the right scope
	result := callOwnHelper()
	if result != 42 {
		return fmt.Errorf("expected 42, got %d", result)
	}
	
	return nil
}

// Nested test functions (equivalent to nested test modules)
func TestNestedResolution() error {
	// Test accessing "grandparent" scope
	result := helperFunction()
	if result != "Helper" {
		return fmt.Errorf("nested test: expected 'Helper', got '%s'", result)
	}
	
	return nil
}

// Helper function to run all tests
func RunAllTests() {
	tests := []struct {
		name string
		test func() error
	}{
		{"Standard Imports", TestStandardImports},
		{"Aliased Imports", TestAliasedImports},
		{"Local Resolution", TestLocalResolution},
		{"Scope Resolution", TestScopeResolution},
		{"Nested Resolution", TestNestedResolution},
	}
	
	fmt.Println("\n=== Running Tests ===")
	allPassed := true
	
	for _, test := range tests {
		if err := test.test(); err != nil {
			fmt.Printf("❌ %s: %v\n", test.name, err)
			allPassed = false
		} else {
			fmt.Printf("✅ %s: passed\n", test.name)
		}
	}
	
	if allPassed {
		fmt.Println("\n🎉 All tests passed!")
	} else {
		fmt.Println("\n❌ Some tests failed!")
	}
}

// === RESOLUTION EXPECTATIONS SUMMARY ===
//
// When indexed and analyzed, we expect:
//
// 1. STANDARD LIBRARY:
//    - fmt resolves to standard library fmt package
//    - sync resolves to standard library sync package
//    - time resolves to standard library time package
//    - context resolves to standard library context package
//
// 2. ALIASES:
//    - set resolves to container/list (with alias tracked)
//    - lock resolves to sync (with alias tracked)
//
// 3. LOCAL FUNCTIONS:
//    - helperFunction resolves to local helperFunction in main package
//    - HelperStruct resolves to local HelperStruct type
//    - NewHelperStruct resolves to local constructor function
//
// 4. NESTED CALLS:
//    - nestedFunction resolves to local nestedFunction
//    - useParentStruct resolves to local useParentStruct
//
// 5. SCOPE RESOLUTION:
//    - helperFunction() in main calls local helperFunction
//    - otherHelperFunction() in callOwnHelper calls otherHelperFunction
//    - Local vs package scope should be correctly distinguished
//
// 6. LOCAL ALIASES:
//    - localHelperFunc resolves to helperFunction via variable alias
//    - LocalHelperStruct resolves to HelperStruct via type alias
//
// 7. BLANK IMPORTS:
//    - _ "net/http/pprof" should be tracked as side-effect import
//
// 8. DISAMBIGUATION:
//    - helperFunction() calls local helperFunction (direct reference)
//    - callOwnHelper() -> otherHelperFunction() (local scope)
//    - Multiple functions with same name in different scopes resolved correctly

// Run tests when this file is executed
func init() {
	// Automatically run tests when package is loaded
	go func() {
		time.Sleep(100 * time.Millisecond) // Let main run first
		RunAllTests()
	}()
}