// Package interfaces demonstrates Go interface declarations and implementations
package interfaces

import (
	"fmt"
	"io"
	"time"
)

// Empty interface
type Any interface{}

// Simple interface with one method
type Stringer interface {
	String() string
}

// Interface with multiple methods
type Reader interface {
	Read(data []byte) (int, error)
	Close() error
}

// Interface with variadic method
type Logger interface {
	Log(level string, format string, args ...interface{})
	SetLevel(level string)
}

// Interface that embeds other interfaces
type ReadWriteCloser interface {
	Reader
	Writer
	io.Closer
}

// Interface with embedded standard library interface
type CustomWriter interface {
	io.Writer
	Flush() error
}

// Interface with different parameter and return types
type DataProcessor interface {
	Process(input []byte) ([]byte, error)
	Validate(data interface{}) bool
	GetMetadata() map[string]interface{}
}

// Interface with function type methods
type EventHandler interface {
	Handle(event Event) error
	Subscribe(eventType string, handler func(Event) error)
	Unsubscribe(eventType string)
}

// Interface with channel methods
type MessageBroker interface {
	Send(message string) error
	Receive() <-chan string
	Subscribe(topic string) <-chan Message
}

// Supporting types for interfaces
type Event struct {
	Type string
	Data interface{}
	Time time.Time
}

type Message struct {
	Topic   string
	Content string
	Headers map[string]string
}

type Writer interface {
	Write(data []byte) (int, error)
}

// Concrete type implementing Stringer
type User struct {
	Name  string
	Email string
}

func (u User) String() string {
	return fmt.Sprintf("User{Name: %s, Email: %s}", u.Name, u.Email)
}

// Concrete type implementing multiple interfaces
type FileProcessor struct {
	filename string
	buffer   []byte
	closed   bool
}

func (f *FileProcessor) Read(data []byte) (int, error) {
	if f.closed {
		return 0, fmt.Errorf("file is closed")
	}
	// Implementation omitted
	return len(data), nil
}

func (f *FileProcessor) Write(data []byte) (int, error) {
	if f.closed {
		return 0, fmt.Errorf("file is closed")
	}
	f.buffer = append(f.buffer, data...)
	return len(data), nil
}

func (f *FileProcessor) Close() error {
	f.closed = true
	return nil
}

func (f *FileProcessor) Process(input []byte) ([]byte, error) {
	// Transform input data
	result := make([]byte, len(input))
	copy(result, input)
	return result, nil
}

func (f *FileProcessor) Validate(data interface{}) bool {
	_, ok := data.([]byte)
	return ok
}

func (f *FileProcessor) GetMetadata() map[string]interface{} {
	return map[string]interface{}{
		"filename": f.filename,
		"size":     len(f.buffer),
		"closed":   f.closed,
	}
}

// Another implementation of DataProcessor
type JSONProcessor struct {
	config map[string]interface{}
}

func (j *JSONProcessor) Process(input []byte) ([]byte, error) {
	// JSON processing logic
	return input, nil
}

func (j *JSONProcessor) Validate(data interface{}) bool {
	// JSON validation logic
	return true
}

func (j *JSONProcessor) GetMetadata() map[string]interface{} {
	return j.config
}

// Implementation of Logger interface
type SimpleLogger struct {
	level string
}

func (l *SimpleLogger) Log(level string, format string, args ...interface{}) {
	if level == l.level {
		fmt.Printf("[%s] "+format+"\n", append([]interface{}{level}, args...)...)
	}
}

func (l *SimpleLogger) SetLevel(level string) {
	l.level = level
}

// Implementation of EventHandler interface
type EventDispatcher struct {
	handlers map[string][]func(Event) error
}

func NewEventDispatcher() *EventDispatcher {
	return &EventDispatcher{
		handlers: make(map[string][]func(Event) error),
	}
}

func (e *EventDispatcher) Handle(event Event) error {
	handlers, exists := e.handlers[event.Type]
	if !exists {
		return nil
	}
	
	for _, handler := range handlers {
		if err := handler(event); err != nil {
			return err
		}
	}
	return nil
}

func (e *EventDispatcher) Subscribe(eventType string, handler func(Event) error) {
	e.handlers[eventType] = append(e.handlers[eventType], handler)
}

func (e *EventDispatcher) Unsubscribe(eventType string) {
	delete(e.handlers, eventType)
}

// Functions that work with interfaces
func ProcessData(processor DataProcessor, data []byte) ([]byte, error) {
	if !processor.Validate(data) {
		return nil, fmt.Errorf("invalid data")
	}
	return processor.Process(data)
}

func LogMessage(logger Logger, message string) {
	logger.Log("INFO", message)
}

func CopyData(src Reader, dst Writer) error {
	buffer := make([]byte, 1024)
	for {
		n, err := src.Read(buffer)
		if err != nil && err != io.EOF {
			return err
		}
		if n == 0 {
			break
		}
		
		_, writeErr := dst.Write(buffer[:n])
		if writeErr != nil {
			return writeErr
		}
	}
	return nil
}

// Function that returns interface
func CreateProcessor(processorType string) DataProcessor {
	switch processorType {
	case "file":
		return &FileProcessor{filename: "default.txt"}
	case "json":
		return &JSONProcessor{config: make(map[string]interface{})}
	default:
		return nil
	}
}

// Function with interface parameter and return
func WrapProcessor(processor DataProcessor) DataProcessor {
	return &ProcessorWrapper{processor: processor}
}

// Wrapper type that implements DataProcessor
type ProcessorWrapper struct {
	processor DataProcessor
}

func (w *ProcessorWrapper) Process(input []byte) ([]byte, error) {
	// Add preprocessing
	return w.processor.Process(input)
}

func (w *ProcessorWrapper) Validate(data interface{}) bool {
	return w.processor.Validate(data)
}

func (w *ProcessorWrapper) GetMetadata() map[string]interface{} {
	metadata := w.processor.GetMetadata()
	metadata["wrapped"] = true
	return metadata
}

// Type assertion functions
func GetStringLength(s interface{}) int {
	if str, ok := s.(string); ok {
		return len(str)
	}
	return 0
}

func IsProcessor(v interface{}) bool {
	_, ok := v.(DataProcessor)
	return ok
}

// Interface with generic-like behavior using interface{}
type Container interface {
	Store(key string, value interface{})
	Retrieve(key string) (interface{}, bool)
	Delete(key string) bool
	Keys() []string
}

// Implementation of Container
type MapContainer struct {
	data map[string]interface{}
}

func NewMapContainer() *MapContainer {
	return &MapContainer{data: make(map[string]interface{})}
}

func (m *MapContainer) Store(key string, value interface{}) {
	m.data[key] = value
}

func (m *MapContainer) Retrieve(key string) (interface{}, bool) {
	value, exists := m.data[key]
	return value, exists
}

func (m *MapContainer) Delete(key string) bool {
	_, exists := m.data[key]
	if exists {
		delete(m.data, key)
	}
	return exists
}

func (m *MapContainer) Keys() []string {
	keys := make([]string, 0, len(m.data))
	for key := range m.data {
		keys = append(keys, key)
	}
	return keys
}