//! Comprehensive Go test file for parser maturity assessment
//! Tests all major Go language features and constructs

package main

import (
	"context"
	"errors"
	"fmt"
	"io"
	"sync"
	"time"
	"unsafe"
)

// Internal module equivalent (using internal package structure)
type InnerStruct struct{}

// Re-exports equivalent (promoted fields/methods)
type PublicInnerStruct = InnerStruct

// Constants
const MaxSize = 1024
const DefaultName = "default"

// Package-level variables (equivalent to statics)
var (
	counter  uint32
	instance sync.Once
	config   *Config
)

// Type aliases
type Result[T any] struct {
	Value T
	Err   error
}

type NodeID uint32
type SharedData *[]byte

// Generic type alias with constraints
type Handler[T any] func(T) (T, error)

// Struct with various field types
type Config struct {
	Name    string `json:"name"`
	port    uint16 `json:"port"`
	Enabled bool   `json:"enabled" deprecated:"true"`
	phantom struct{} // Go equivalent of PhantomData
}

// "Tuple struct" equivalent (unnamed struct fields)
type Point struct {
	X, Y, Z float64
}

// Unit struct equivalent
type Marker struct{}

// Struct with "lifetime" equivalent (no direct translation, using context)
type BorrowedData struct {
	data    string
	mutable []byte
	ctx     context.Context // Context for lifetime-like behavior
}

// Enum equivalent using type and constants
type Status int

const (
	StatusActive Status = iota
	StatusInactive
	StatusPending
	StatusComplex
)

// Complex enum with data (using interface and concrete types)
type StatusData interface {
	isStatus()
}

type StatusActive struct{}
type StatusInactive struct{ Reason string }
type StatusPending struct{ Duration time.Duration }
type StatusComplex struct {
	ID   uint32
	Data []byte
}

func (StatusActive) isStatus()   {}
func (StatusInactive) isStatus() {}
func (StatusPending) isStatus()  {}
func (StatusComplex) isStatus()  {}

// Generic enum equivalent
type Option[T any] interface {
	isOption()
}

type Some[T any] struct{ Value T }
type None[T any] struct{}

func (Some[T]) isOption() {}
func (None[T]) isOption() {}

// Interface with associated types (Go interfaces)
type Parser[Input, Output any] interface {
	Parse(input Input) (Output, error)
	Validate(input Input) bool
	New() Parser[Input, Output] // Associated function equivalent
}

// Interface with generic methods
type Container[T any] interface {
	Add(item T)
	Get(index int) *T
	Iter() <-chan T // Go channel as iterator
}

// Interface with context (lifetime equivalent)
type Lifecycle[T any] interface {
	Process(ctx context.Context) T
}

// Constants for interface (Go doesn't have interface constants, use separate const)
const MaxDepth = 100

// Implementation methods for Config
// Associated constant equivalent
const DefaultPort uint16 = 8080

// Constructor (associated function)
func NewConfig(name string) *Config {
	return &Config{
		Name:    name,
		port:    DefaultPort,
		Enabled: true,
		phantom: struct{}{},
	}
}

// Method with receiver
func (c *Config) Port() uint16 {
	return c.port
}

// Method with mutable receiver
func (c *Config) SetPort(port uint16) {
	c.port = port
}

// Method consuming self (takes ownership via value receiver)
func (c Config) IntoName() string {
	return c.Name
}

// Generic method
func WithData[T any](c *Config, data T) (*Config, T) {
	// Clone equivalent
	newConfig := *c
	return &newConfig, data
}

// Async equivalent (using goroutines)
func (c *Config) Connect(ctx context.Context) error {
	// Simulate async operation
	go func() {
		time.Sleep(100 * time.Millisecond)
	}()
	return nil
}

// Unsafe method
func (c *Config) GetRawPtr() unsafe.Pointer {
	return unsafe.Pointer(&c.port)
}

// Interface implementation
type ConfigParser struct{}

func (cp ConfigParser) Parse(input string) (*Config, error) {
	return NewConfig(input), nil
}

func (cp ConfigParser) Validate(input string) bool {
	return true
}

func (cp ConfigParser) New() Parser[string, *Config] {
	return ConfigParser{}
}

// Generic struct with constraints
type GenericContainer[T any, U comparable] struct {
	items    []T
	metadata U
}

// Constructor for generic struct
func NewGenericContainer[T any, U comparable]() *GenericContainer[T, U] {
	var zero U
	return &GenericContainer[T, U]{
		items:    make([]T, 0),
		metadata: zero,
	}
}

func (gc *GenericContainer[T, U]) Add(item T) {
	gc.items = append(gc.items, item)
}

// Container interface implementation
func (gc *GenericContainer[T, U]) Get(index int) *T {
	if index < len(gc.items) {
		return &gc.items[index]
	}
	return nil
}

func (gc *GenericContainer[T, U]) Iter() <-chan T {
	ch := make(chan T)
	go func() {
		defer close(ch)
		for _, item := range gc.items {
			ch <- item
		}
	}()
	return ch
}

// Function with various parameter types
func ComplexFunction[T any, U fmt.Stringer](
	reference string,
	mutable *[]T,
	owned string,
	generic U,
	closure func() T,
) (string, error) {
	*mutable = append(*mutable, closure())
	return reference, nil
}

// Async function equivalent
func AsyncOperation(ctx context.Context, url string) <-chan Result[string] {
	result := make(chan Result[string], 1)
	go func() {
		defer close(result)
		// Simulate async work
		time.Sleep(10 * time.Millisecond)
		result <- Result[string]{Value: url, Err: nil}
	}()
	return result
}

// Const function equivalent (pure function)
func ConstFunction(x uint32) uint32 {
	return x * 2
}

// Unsafe function
func UnsafeOperation(ptr unsafe.Pointer) {
	*(*byte)(ptr) = 0
}

// Function returning interface (equivalent to impl Trait)
func ReturnsInterface() fmt.Stringer {
	return &stringWrapper{"hello"}
}

type stringWrapper struct{ s string }

func (sw *stringWrapper) String() string { return sw.s }

// Function taking interface parameter
func TakesDynInterface(parser Parser[string, *Config]) {
	// Implementation
}

// Higher-ranked trait bounds equivalent (using generics)
func HigherRanked[F ~func(string) string](f F) {
	f("test")
}

// Code generation equivalent (Go generate, but showing pattern)
//go:generate go run generate_functions.go

// Generated function (would be generated by go generate)
func GeneratedFunc() {
	fmt.Printf("Function: %s\n", "GeneratedFunc")
}

// Union equivalent (using unsafe and interface)
type MyUnion interface {
	isUnion()
}

type UnionF1 struct{ Value uint32 }
type UnionF2 struct{ Value float32 }

func (UnionF1) isUnion() {}
func (UnionF2) isUnion() {}

// Extern equivalent (using CGO)
/*
#include <stdio.h>
int external_function(int x) {
    return x * 2;
}
*/
import "C"

func CallExternalFunction(x int32) int32 {
	return int32(C.external_function(C.int(x)))
}

// Error type
type CustomError struct {
	Message string
}

func (e *CustomError) Error() string {
	return e.Message
}

// Test module equivalent
func TestConfig() error {
	config := NewConfig("test")
	if config.Port() != DefaultPort {
		return fmt.Errorf("expected port %d, got %d", DefaultPort, config.Port())
	}
	return nil
}

// Benchmark equivalent
func BenchmarkCreate(b interface{ 
	ResetTimer()
	StartTimer()
	StopTimer()
}) {
	b.ResetTimer()
	for i := 0; i < 1000; i++ {
		NewConfig("bench")
	}
}

// Main function
func main() {
	config := NewConfig("app")
	fmt.Printf("Config: %+v\n", config)

	// Test various features
	fmt.Println("Testing Go features:")

	// Test generics
	container := NewGenericContainer[int, string]()
	container.Add(42)
	
	// Test channels/iteration
	for item := range container.Iter() {
		fmt.Printf("Item: %d\n", item)
		break // Just test one item
	}

	// Test async operation
	ctx := context.Background()
	resultChan := AsyncOperation(ctx, "https://example.com")
	result := <-resultChan
	if result.Err == nil {
		fmt.Printf("Async result: %s\n", result.Value)
	}

	// Test interface
	stringer := ReturnsInterface()
	fmt.Printf("Interface result: %s\n", stringer.String())

	// Test error handling
	if err := TestConfig(); err != nil {
		fmt.Printf("Test failed: %v\n", err)
	} else {
		fmt.Println("All tests passed!")
	}
}