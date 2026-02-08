//! Authentication service implementation
//!
//! This package demonstrates:
//! - Service implementation with dependencies
//! - Cross-package type usage and imports
//! - Interface definitions and implementations
//! - Error handling patterns
//! - Context usage for cancellation/timeouts

package services

import (
	"context"
	"crypto/sha256"
	"errors"
	"fmt"
	"strings"
	"sync"
	"time"

	// Import from other packages in this module
	"app/models"
)

// Package-level constants
const (
	TokenExpiry       = 24 * time.Hour
	MaxSessions       = 1000
	MinPasswordLength = 6
)

// AuthToken represents an authentication token
type AuthToken string

// Session represents a user session
type Session struct {
	Token     AuthToken
	UserID    uint64
	CreatedAt time.Time
	ExpiresAt time.Time
}

// IsExpired checks if the session has expired
func (s *Session) IsExpired() bool {
	return time.Now().After(s.ExpiresAt)
}

// AuthService handles user authentication and session management
type AuthService struct {
	db           *DatabaseConnection
	sessions     map[AuthToken]*Session
	users        map[string]*models.User // email -> user
	mutex        sync.RWMutex
	tokenCounter uint64
}

// NewAuthService creates a new authentication service
func NewAuthService(db *DatabaseConnection) *AuthService {
	return &AuthService{
		db:       db,
		sessions: make(map[AuthToken]*Session),
		users:    make(map[string]*models.User),
		mutex:    sync.RWMutex{},
	}
}

// RegisterUser registers a new user in the system
func (a *AuthService) RegisterUser(user *models.User) error {
	a.mutex.Lock()
	defer a.mutex.Unlock()

	// Validate user data
	if err := user.Validate(); err != nil {
		return fmt.Errorf("user validation failed: %w", err)
	}

	// Check for duplicate email
	if _, exists := a.users[user.Email()]; exists {
		return NewAuthError(ErrDuplicateEmail, "email already registered")
	}

	// Store user (in real implementation, would use database)
	a.users[user.Email()] = user

	// Mock database operation
	if err := a.db.Execute("INSERT INTO users (name, email, role) VALUES (?, ?, ?)",
		[]interface{}{user.Name(), user.Email(), user.Role().String()}); err != nil {
		return fmt.Errorf("database error: %w", err)
	}

	return nil
}

// Authenticate authenticates a user and creates a session
func (a *AuthService) Authenticate(ctx context.Context, email, password string) (AuthToken, error) {
	// Check context cancellation
	select {
	case <-ctx.Done():
		return "", ctx.Err()
	default:
	}

	a.mutex.RLock()
	user, exists := a.users[email]
	a.mutex.RUnlock()

	if !exists {
		return "", NewAuthError(ErrUserNotFound, "user not found")
	}

	// Mock password validation
	if len(password) < MinPasswordLength {
		return "", NewAuthError(ErrInvalidCredentials, "invalid credentials")
	}

	// Generate session token
	token := a.generateToken(user)

	// Create session
	session := &Session{
		Token:     token,
		UserID:    user.ID,
		CreatedAt: time.Now(),
		ExpiresAt: time.Now().Add(TokenExpiry),
	}

	a.mutex.Lock()
	a.sessions[token] = session
	a.mutex.Unlock()

	// Update user's last login
	user.UpdateLastLogin()

	return token, nil
}

// ValidateSession validates a session token and returns the user
func (a *AuthService) ValidateSession(token AuthToken) (*models.User, error) {
	a.mutex.RLock()
	session, exists := a.sessions[token]
	a.mutex.RUnlock()

	if !exists {
		return nil, NewAuthError(ErrInvalidToken, "invalid token")
	}

	if session.IsExpired() {
		// Remove expired session
		a.mutex.Lock()
		delete(a.sessions, token)
		a.mutex.Unlock()
		return nil, NewAuthError(ErrExpiredToken, "token expired")
	}

	// Find user by ID
	a.mutex.RLock()
	defer a.mutex.RUnlock()

	for _, user := range a.users {
		if user.ID == session.UserID {
			return user, nil
		}
	}

	return nil, NewAuthError(ErrUserNotFound, "user not found for session")
}

// Logout invalidates a session token
func (a *AuthService) Logout(token AuthToken) error {
	a.mutex.Lock()
	defer a.mutex.Unlock()

	if _, exists := a.sessions[token]; !exists {
		return NewAuthError(ErrInvalidToken, "invalid token")
	}

	delete(a.sessions, token)
	return nil
}

// GetSessionCount returns the number of active sessions
func (a *AuthService) GetSessionCount() int {
	a.mutex.RLock()
	defer a.mutex.RUnlock()

	count := 0
	for _, session := range a.sessions {
		if !session.IsExpired() {
			count++
		}
	}
	return count
}

// CleanupExpiredSessions removes expired sessions
func (a *AuthService) CleanupExpiredSessions() int {
	a.mutex.Lock()
	defer a.mutex.Unlock()

	removed := 0
	for token, session := range a.sessions {
		if session.IsExpired() {
			delete(a.sessions, token)
			removed++
		}
	}
	return removed
}

// GetUserByEmail retrieves a user by email (package-private for testing)
func (a *AuthService) getUserByEmail(email string) *models.User {
	a.mutex.RLock()
	defer a.mutex.RUnlock()
	return a.users[email]
}

// Private helper methods

// generateToken generates a unique session token
func (a *AuthService) generateToken(user *models.User) AuthToken {
	a.tokenCounter++
	data := fmt.Sprintf("%d-%d-%d", user.ID, a.tokenCounter, time.Now().UnixNano())
	hash := sha256.Sum256([]byte(data))
	return AuthToken(fmt.Sprintf("%x", hash[:16])) // Use first 16 bytes
}

// Package-level utility functions

// HashPassword creates a hash of the password (mock implementation)
func HashPassword(password string) string {
	hash := sha256.Sum256([]byte(password + "salt"))
	return fmt.Sprintf("%x", hash)
}

// VerifyPassword verifies a password against its hash
func VerifyPassword(password, hash string) bool {
	return HashPassword(password) == hash
}

// ParseAuthError attempts to parse an error string into an AuthError
func ParseAuthError(errorMsg string) (*AuthError, error) {
	errorMsg = strings.ToLower(errorMsg)

	switch {
	case strings.Contains(errorMsg, "not found"):
		return NewAuthError(ErrUserNotFound, errorMsg), nil
	case strings.Contains(errorMsg, "duplicate"):
		return NewAuthError(ErrDuplicateEmail, errorMsg), nil
	case strings.Contains(errorMsg, "invalid"):
		return NewAuthError(ErrInvalidCredentials, errorMsg), nil
	default:
		return nil, errors.New("cannot parse error message")
	}
}

// Error types and constants
type AuthErrorCode int

const (
	ErrUserNotFound AuthErrorCode = iota
	ErrDuplicateEmail
	ErrInvalidCredentials
	ErrInvalidToken
	ErrExpiredToken
	ErrTooManySessions
)

// AuthError represents authentication-related errors
type AuthError struct {
	Code    AuthErrorCode
	Message string
}

func NewAuthError(code AuthErrorCode, message string) *AuthError {
	return &AuthError{
		Code:    code,
		Message: message,
	}
}

func (e *AuthError) Error() string {
	return fmt.Sprintf("auth error [%d]: %s", int(e.Code), e.Message)
}

// Is implements error comparison
func (e *AuthError) Is(target error) bool {
	if other, ok := target.(*AuthError); ok {
		return e.Code == other.Code
	}
	return false
}

// Convert from models.UserError
func (e *AuthError) FromUserError(userErr *models.UserError) *AuthError {
	switch userErr.Code {
	case 3: // models.ErrUserNotFound
		return NewAuthError(ErrUserNotFound, userErr.Message)
	case 4: // models.ErrDuplicateEmail
		return NewAuthError(ErrDuplicateEmail, userErr.Message)
	default:
		return NewAuthError(ErrInvalidCredentials, userErr.Message)
	}
}

// Package initialization
func init() {
	fmt.Println("[INIT] Auth service package initialized")
}
