// Package generics demonstrates Go 1.18+ generic types and functions
package generics

import (
	"fmt"
	"constraints"
)

// Basic generic function with type parameter
func Identity[T any](value T) T {
	return value
}

// Generic function with constraint
func Add[T constraints.Ordered](a, b T) T {
	return a + b
}

// Generic function with multiple type parameters
func Pair[T, U any](first T, second U) (T, U) {
	return first, second
}

// Generic function with custom constraint
func Max[T Comparable](a, b T) T {
	if a.Compare(b) > 0 {
		return a
	}
	return b
}

// Custom constraint interface
type Comparable interface {
	Compare(other Comparable) int
}

// Number constraint using type union
type Number interface {
	~int | ~int8 | ~int16 | ~int32 | ~int64 |
	~uint | ~uint8 | ~uint16 | ~uint32 | ~uint64 |
	~float32 | ~float64
}

// Generic function with number constraint
func Sum[T Number](values []T) T {
	var total T
	for _, v := range values {
		total += v
	}
	return total
}

// Generic struct with single type parameter
type Stack[T any] struct {
	items []T
}

// Methods on generic struct
func (s *Stack[T]) Push(item T) {
	s.items = append(s.items, item)
}

func (s *Stack[T]) Pop() (T, bool) {
	if len(s.items) == 0 {
		var zero T
		return zero, false
	}
	
	index := len(s.items) - 1
	item := s.items[index]
	s.items = s.items[:index]
	return item, true
}

func (s *Stack[T]) Peek() (T, bool) {
	if len(s.items) == 0 {
		var zero T
		return zero, false
	}
	return s.items[len(s.items)-1], true
}

func (s *Stack[T]) Size() int {
	return len(s.items)
}

func (s *Stack[T]) IsEmpty() bool {
	return len(s.items) == 0
}

// Generic struct with multiple type parameters
type Map[K comparable, V any] struct {
	data map[K]V
}

func NewMap[K comparable, V any]() *Map[K, V] {
	return &Map[K, V]{
		data: make(map[K]V),
	}
}

func (m *Map[K, V]) Set(key K, value V) {
	m.data[key] = value
}

func (m *Map[K, V]) Get(key K) (V, bool) {
	value, exists := m.data[key]
	return value, exists
}

func (m *Map[K, V]) Delete(key K) bool {
	_, exists := m.data[key]
	if exists {
		delete(m.data, key)
	}
	return exists
}

func (m *Map[K, V]) Keys() []K {
	keys := make([]K, 0, len(m.data))
	for key := range m.data {
		keys = append(keys, key)
	}
	return keys
}

func (m *Map[K, V]) Values() []V {
	values := make([]V, 0, len(m.data))
	for _, value := range m.data {
		values = append(values, value)
	}
	return values
}

// Generic interface
type Container[T any] interface {
	Add(item T)
	Remove(item T) bool
	Contains(item T) bool
	Size() int
	Items() []T
}

// Generic struct implementing generic interface
type List[T comparable] struct {
	items []T
}

func NewList[T comparable]() *List[T] {
	return &List[T]{items: make([]T, 0)}
}

func (l *List[T]) Add(item T) {
	l.items = append(l.items, item)
}

func (l *List[T]) Remove(item T) bool {
	for i, existing := range l.items {
		if existing == item {
			l.items = append(l.items[:i], l.items[i+1:]...)
			return true
		}
	}
	return false
}

func (l *List[T]) Contains(item T) bool {
	for _, existing := range l.items {
		if existing == item {
			return true
		}
	}
	return false
}

func (l *List[T]) Size() int {
	return len(l.items)
}

func (l *List[T]) Items() []T {
	result := make([]T, len(l.items))
	copy(result, l.items)
	return result
}

// Generic function that works with generic interface
func PrintContainer[T any](container Container[T]) {
	items := container.Items()
	fmt.Printf("Container has %d items: %v\n", container.Size(), items)
}

// Complex generic constraint
type Serializable interface {
	Serialize() ([]byte, error)
	Deserialize([]byte) error
}

type SerializableNumber interface {
	Number
	Serializable
}

// Generic function with complex constraint
func ProcessSerializableNumbers[T SerializableNumber](numbers []T) ([]byte, error) {
	total := Sum(numbers)
	return total.Serialize()
}

// Generic type with method constraint
type Processor[T Processable] struct {
	items []T
}

type Processable interface {
	Process() error
	IsProcessed() bool
}

func (p *Processor[T]) ProcessAll() error {
	for _, item := range p.items {
		if !item.IsProcessed() {
			if err := item.Process(); err != nil {
				return err
			}
		}
	}
	return nil
}

// Generic function with type inference
func Filter[T any](slice []T, predicate func(T) bool) []T {
	result := make([]T, 0)
	for _, item := range slice {
		if predicate(item) {
			result = append(result, item)
		}
	}
	return result
}

func Map[T, U any](slice []T, mapper func(T) U) []U {
	result := make([]U, len(slice))
	for i, item := range slice {
		result[i] = mapper(item)
	}
	return result
}

func Reduce[T, U any](slice []T, initial U, reducer func(U, T) U) U {
	result := initial
	for _, item := range slice {
		result = reducer(result, item)
	}
	return result
}

// Generic struct with embedded generic type
type Repository[T any] struct {
	*Map[string, T]
	cache *List[T]
}

func NewRepository[T comparable]() *Repository[T] {
	return &Repository[T]{
		Map:   NewMap[string, T](),
		cache: NewList[T](),
	}
}

func (r *Repository[T]) Store(id string, item T) {
	r.Set(id, item)
	if !r.cache.Contains(item) {
		r.cache.Add(item)
	}
}

// Generic method with additional type parameter
func (r *Repository[T]) Transform[U any](transformer func(T) U) []U {
	items := r.Values()
	return Map(items, transformer)
}

// Generic function with variadic parameters
func Combine[T any](items ...[]T) []T {
	var result []T
	for _, slice := range items {
		result = append(result, slice...)
	}
	return result
}

// Generic type alias
type StringMap[V any] = Map[string, V]
type IntSet = Map[int, struct{}]

// Generic function using type alias
func CreateStringMap[V any]() StringMap[V] {
	return *NewMap[string, V]()
}

// Complex generic example with multiple constraints
type Cache[K comparable, V Serializable] struct {
	data    *Map[K, V]
	maxSize int
}

func NewCache[K comparable, V Serializable](maxSize int) *Cache[K, V] {
	return &Cache[K, V]{
		data:    NewMap[K, V](),
		maxSize: maxSize,
	}
}

func (c *Cache[K, V]) Put(key K, value V) error {
	if c.data.Size() >= c.maxSize {
		// Simple eviction: remove first key
		keys := c.data.Keys()
		if len(keys) > 0 {
			c.data.Delete(keys[0])
		}
	}
	c.data.Set(key, value)
	return nil
}

func (c *Cache[K, V]) Get(key K) (V, bool) {
	return c.data.Get(key)
}

// Utility functions for working with generics
func Zero[T any]() T {
	var zero T
	return zero
}

func Ptr[T any](value T) *T {
	return &value
}

func Deref[T any](ptr *T) T {
	if ptr == nil {
		return Zero[T]()
	}
	return *ptr
}

// Example usage functions (would typically be in tests)
func ExampleUsage() {
	// Basic generic usage
	stringStack := &Stack[string]{}
	stringStack.Push("hello")
	stringStack.Push("world")
	
	intMap := NewMap[int, string]()
	intMap.Set(1, "one")
	intMap.Set(2, "two")
	
	// Generic function usage
	numbers := []int{1, 2, 3, 4, 5}
	total := Sum(numbers)
	even := Filter(numbers, func(n int) bool { return n%2 == 0 })
	doubled := Map(numbers, func(n int) int { return n * 2 })
	
	fmt.Printf("Total: %d, Even: %v, Doubled: %v\n", total, even, doubled)
}