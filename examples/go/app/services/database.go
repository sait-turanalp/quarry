//! Database connection and management
//!
//! This package demonstrates:
//! - Resource management patterns
//! - Interface definitions for testability
//! - Error handling and custom error types
//! - Connection pooling concepts
//! - Transaction management

package services

import (
	"fmt"
	"strings"
	"sync"
	"time"
)

// Package-level constants
const (
	MaxConnections    = 100
	ConnectionTimeout = 30 * time.Second
	QueryTimeout      = 60 * time.Second
	DefaultPort       = 5432
)

// Database interface for testability
type Database interface {
	Connect() error
	Close() error
	Execute(query string, args []interface{}) error
	Query(query string, args []interface{}) (*QueryResult, error)
	BeginTransaction() (Transaction, error)
	IsConnected() bool
}

// DatabaseConnection implements the Database interface
type DatabaseConnection struct {
	connectionURL  string
	connected      bool
	mockData       map[string]interface{}
	mutex          sync.RWMutex
	connectionPool *ConnectionPool
}

// ConnectionPool manages database connections
type ConnectionPool struct {
	maxConnections    int
	activeConnections int
	mutex             sync.Mutex
}

func NewConnectionPool(maxConnections int) *ConnectionPool {
	return &ConnectionPool{
		maxConnections:    maxConnections,
		activeConnections: 0,
	}
}

func (cp *ConnectionPool) Acquire() error {
	cp.mutex.Lock()
	defer cp.mutex.Unlock()

	if cp.activeConnections >= cp.maxConnections {
		return NewDatabaseError(ErrConnectionPoolFull, "connection pool is full")
	}

	cp.activeConnections++
	return nil
}

func (cp *ConnectionPool) Release() {
	cp.mutex.Lock()
	defer cp.mutex.Unlock()

	if cp.activeConnections > 0 {
		cp.activeConnections--
	}
}

func (cp *ConnectionPool) Stats() ConnectionPoolStats {
	cp.mutex.Lock()
	defer cp.mutex.Unlock()

	return ConnectionPoolStats{
		MaxConnections:       cp.maxConnections,
		ActiveConnections:    cp.activeConnections,
		AvailableConnections: cp.maxConnections - cp.activeConnections,
	}
}

// ConnectionPoolStats provides connection pool statistics
type ConnectionPoolStats struct {
	MaxConnections       int
	ActiveConnections    int
	AvailableConnections int
}

// NewDatabaseConnection creates a new database connection
func NewDatabaseConnection(connectionURL string) *DatabaseConnection {
	return &DatabaseConnection{
		connectionURL:  connectionURL,
		connected:      false,
		mockData:       make(map[string]interface{}),
		connectionPool: NewConnectionPool(MaxConnections),
	}
}

// Connect establishes a connection to the database
func (db *DatabaseConnection) Connect() error {
	if db.connected {
		return nil
	}

	if db.connectionURL == "" {
		return NewDatabaseError(ErrInvalidConnectionString, "connection URL cannot be empty")
	}

	if !strings.Contains(db.connectionURL, "://") {
		return NewDatabaseError(ErrInvalidConnectionString, "invalid connection URL format")
	}

	// Acquire connection from pool
	if err := db.connectionPool.Acquire(); err != nil {
		return err
	}

	// Mock connection establishment
	fmt.Printf("Connecting to database: %s\n", db.connectionURL)

	db.mutex.Lock()
	db.connected = true
	db.mutex.Unlock()

	return nil
}

// Close closes the database connection
func (db *DatabaseConnection) Close() error {
	db.mutex.Lock()
	defer db.mutex.Unlock()

	if !db.connected {
		return nil
	}

	db.connected = false
	db.mockData = make(map[string]interface{})
	db.connectionPool.Release()

	fmt.Println("Database connection closed")
	return nil
}

// Execute executes a database command (INSERT, UPDATE, DELETE)
func (db *DatabaseConnection) Execute(query string, args []interface{}) error {
	if err := db.checkConnection(); err != nil {
		return err
	}

	if strings.TrimSpace(query) == "" {
		return NewDatabaseError(ErrInvalidQuery, "query cannot be empty")
	}

	// Mock query execution
	fmt.Printf("Executing query: %s with %d args\n", query, len(args))

	// Simulate different query types
	queryUpper := strings.ToUpper(strings.TrimSpace(query))
	switch {
	case strings.HasPrefix(queryUpper, "INSERT"):
		return db.mockInsert(query, args)
	case strings.HasPrefix(queryUpper, "UPDATE"):
		return db.mockUpdate(query, args)
	case strings.HasPrefix(queryUpper, "DELETE"):
		return db.mockDelete(query, args)
	default:
		return NewDatabaseError(ErrUnsupportedOperation, "execute only supports INSERT, UPDATE, DELETE")
	}
}

// Query executes a database query (SELECT) and returns results
func (db *DatabaseConnection) Query(query string, args []interface{}) (*QueryResult, error) {
	if err := db.checkConnection(); err != nil {
		return nil, err
	}

	if strings.TrimSpace(query) == "" {
		return nil, NewDatabaseError(ErrInvalidQuery, "query cannot be empty")
	}

	// Mock query execution
	fmt.Printf("Querying: %s with %d args\n", query, len(args))

	// Simulate SELECT query
	queryUpper := strings.ToUpper(strings.TrimSpace(query))
	if strings.HasPrefix(queryUpper, "SELECT") {
		return db.mockSelect(query, args)
	}

	return nil, NewDatabaseError(ErrUnsupportedOperation, "query only supports SELECT statements")
}

// BeginTransaction starts a new database transaction
func (db *DatabaseConnection) BeginTransaction() (Transaction, error) {
	if err := db.checkConnection(); err != nil {
		return nil, err
	}

	return NewDatabaseTransaction(db), nil
}

// IsConnected returns whether the database is connected
func (db *DatabaseConnection) IsConnected() bool {
	db.mutex.RLock()
	defer db.mutex.RUnlock()
	return db.connected
}

// GetConnectionURL returns the connection URL (for testing)
func (db *DatabaseConnection) GetConnectionURL() string {
	return db.connectionURL
}

// GetPoolStats returns connection pool statistics
func (db *DatabaseConnection) GetPoolStats() ConnectionPoolStats {
	return db.connectionPool.Stats()
}

// Private helper methods

func (db *DatabaseConnection) checkConnection() error {
	db.mutex.RLock()
	defer db.mutex.RUnlock()

	if !db.connected {
		return NewDatabaseError(ErrNotConnected, "database not connected")
	}
	return nil
}

func (db *DatabaseConnection) mockInsert(query string, args []interface{}) error {
	// Mock INSERT operation
	db.mutex.Lock()
	defer db.mutex.Unlock()

	key := fmt.Sprintf("insert_%d", len(db.mockData))
	db.mockData[key] = args
	return nil
}

func (db *DatabaseConnection) mockUpdate(query string, args []interface{}) error {
	// Mock UPDATE operation
	return nil
}

func (db *DatabaseConnection) mockDelete(query string, args []interface{}) error {
	// Mock DELETE operation
	return nil
}

func (db *DatabaseConnection) mockSelect(query string, args []interface{}) (*QueryResult, error) {
	// Mock SELECT operation
	rows := []map[string]interface{}{
		{"id": 1, "name": "Test User", "email": "test@example.com"},
		{"id": 2, "name": "Another User", "email": "another@example.com"},
	}

	return &QueryResult{
		Rows:         rows,
		RowsAffected: 0,
		LastInsertID: nil,
	}, nil
}

// Transaction interface
type Transaction interface {
	Commit() error
	Rollback() error
	Execute(query string, args []interface{}) error
	Query(query string, args []interface{}) (*QueryResult, error)
}

// DatabaseTransaction implements the Transaction interface
type DatabaseTransaction struct {
	db         *DatabaseConnection
	committed  bool
	rolledBack bool
}

func NewDatabaseTransaction(db *DatabaseConnection) *DatabaseTransaction {
	return &DatabaseTransaction{
		db: db,
	}
}

func (tx *DatabaseTransaction) Commit() error {
	if tx.committed || tx.rolledBack {
		return NewDatabaseError(ErrTransactionClosed, "transaction already closed")
	}

	tx.committed = true
	fmt.Println("Transaction committed")
	return nil
}

func (tx *DatabaseTransaction) Rollback() error {
	if tx.committed || tx.rolledBack {
		return NewDatabaseError(ErrTransactionClosed, "transaction already closed")
	}

	tx.rolledBack = true
	fmt.Println("Transaction rolled back")
	return nil
}

func (tx *DatabaseTransaction) Execute(query string, args []interface{}) error {
	if tx.committed || tx.rolledBack {
		return NewDatabaseError(ErrTransactionClosed, "transaction is closed")
	}

	return tx.db.Execute(query, args)
}

func (tx *DatabaseTransaction) Query(query string, args []interface{}) (*QueryResult, error) {
	if tx.committed || tx.rolledBack {
		return nil, NewDatabaseError(ErrTransactionClosed, "transaction is closed")
	}

	return tx.db.Query(query, args)
}

// QueryResult represents the result of a database query
type QueryResult struct {
	Rows         []map[string]interface{}
	RowsAffected int64
	LastInsertID *int64
}

func (qr *QueryResult) HasRows() bool {
	return len(qr.Rows) > 0
}

func (qr *QueryResult) RowCount() int {
	return len(qr.Rows)
}

// Package-level utility functions

// ValidateConnectionString checks if a connection string is valid
func ValidateConnectionString(connectionString string) error {
	if connectionString == "" {
		return NewDatabaseError(ErrInvalidConnectionString, "connection string cannot be empty")
	}

	if !strings.Contains(connectionString, "://") {
		return NewDatabaseError(ErrInvalidConnectionString, "connection string must contain protocol")
	}

	return nil
}

// EscapeSQLIdentifier escapes SQL identifiers
func EscapeSQLIdentifier(identifier string) string {
	return fmt.Sprintf("`%s`", strings.ReplaceAll(identifier, "`", "``"))
}

// EscapeSQLString escapes SQL string values
func EscapeSQLString(value string) string {
	return fmt.Sprintf("'%s'", strings.ReplaceAll(value, "'", "''"))
}

// ParseConnectionString parses a connection string into components
func ParseConnectionString(connectionString string) (ConnectionInfo, error) {
	if err := ValidateConnectionString(connectionString); err != nil {
		return ConnectionInfo{}, err
	}

	// Simple parsing for demonstration
	parts := strings.Split(connectionString, "://")
	if len(parts) != 2 {
		return ConnectionInfo{}, NewDatabaseError(ErrInvalidConnectionString, "invalid format")
	}

	return ConnectionInfo{
		Protocol: parts[0],
		Address:  parts[1],
	}, nil
}

// ConnectionInfo holds parsed connection information
type ConnectionInfo struct {
	Protocol string
	Address  string
	Host     string
	Port     int
	Database string
}

// Error types and constants
type DatabaseErrorCode int

const (
	ErrNotConnected DatabaseErrorCode = iota
	ErrInvalidConnectionString
	ErrInvalidQuery
	ErrConnectionPoolFull
	ErrTransactionClosed
	ErrUnsupportedOperation
	ErrQueryTimeout
)

// DatabaseError represents database-related errors
type DatabaseError struct {
	Code    DatabaseErrorCode
	Message string
}

func NewDatabaseError(code DatabaseErrorCode, message string) *DatabaseError {
	return &DatabaseError{
		Code:    code,
		Message: message,
	}
}

func (e *DatabaseError) Error() string {
	return fmt.Sprintf("database error [%d]: %s", int(e.Code), e.Message)
}

// Is implements error comparison
func (e *DatabaseError) Is(target error) bool {
	if other, ok := target.(*DatabaseError); ok {
		return e.Code == other.Code
	}
	return false
}

// Temporary returns true for temporary errors
func (e *DatabaseError) Temporary() bool {
	return e.Code == ErrConnectionPoolFull || e.Code == ErrQueryTimeout
}

// Package initialization
func init() {
	fmt.Println("[INIT] Database service package initialized")
}
