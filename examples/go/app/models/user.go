//! User model definitions and related functionality
//!
//! This package demonstrates:
//! - Struct definitions with exported/unexported fields
//! - Method definitions with different receiver types
//! - Enum-like constants using iota
//! - Package-level constants and variables
//! - Error types and custom error handling

package models

import (
	"errors"
	"fmt"
	"strings"
	"time"
)

// Package-level constants (exported)
const (
	Version        = "1.0.0"
	MaxNameLength  = 100
	MaxEmailLength = 254
)

// Package-level variables
var (
	// Exported package variable
	DefaultRole = RoleUser
	// unexported package variable
	userCounter int
)

// UserRole represents user permission levels (enum-like)
type UserRole int

const (
	RoleGuest UserRole = iota
	RoleUser
	RoleAdmin
)

// String implements fmt.Stringer interface
func (r UserRole) String() string {
	switch r {
	case RoleGuest:
		return "guest"
	case RoleUser:
		return "user"
	case RoleAdmin:
		return "admin"
	default:
		return "unknown"
	}
}

// IsValid checks if the role is valid
func (r UserRole) IsValid() bool {
	return r >= RoleGuest && r <= RoleAdmin
}

// User represents a system user
type User struct {
	// Exported fields
	ID    uint64   `json:"id"`
	name  string   `json:"name"`  // unexported (private)
	email string   `json:"email"` // unexported (private)
	role  UserRole `json:"role"`  // unexported (private)

	// unexported fields
	createdAt time.Time
	lastLogin *time.Time
}

// NewUser creates a new user with validation
func NewUser(name, email string, role UserRole) *User {
	userCounter++
	return &User{
		ID:        uint64(userCounter),
		name:      strings.TrimSpace(name),
		email:     strings.ToLower(strings.TrimSpace(email)),
		role:      role,
		createdAt: time.Now(),
	}
}

// Getter methods for unexported fields
func (u *User) Name() string {
	return u.name
}

func (u *User) Email() string {
	return u.email
}

func (u *User) Role() UserRole {
	return u.role
}

func (u *User) CreatedAt() time.Time {
	return u.createdAt
}

// Setter methods
func (u *User) SetName(name string) error {
	if strings.TrimSpace(name) == "" {
		return errors.New("name cannot be empty")
	}
	if len(name) > MaxNameLength {
		return fmt.Errorf("name too long (max %d characters)", MaxNameLength)
	}
	u.name = strings.TrimSpace(name)
	return nil
}

func (u *User) SetEmail(email string) error {
	if err := validateEmail(email); err != nil {
		return err
	}
	u.email = strings.ToLower(strings.TrimSpace(email))
	return nil
}

func (u *User) SetRole(role UserRole) error {
	if !role.IsValid() {
		return errors.New("invalid role")
	}
	u.role = role
	return nil
}

// Business logic methods
func (u *User) IsAdmin() bool {
	return u.role == RoleAdmin
}

func (u *User) CanModifyUser(other *User) bool {
	return u.IsAdmin() || u.ID == other.ID
}

func (u *User) UpdateLastLogin() {
	now := time.Now()
	u.lastLogin = &now
}

// String implements fmt.Stringer interface
func (u *User) String() string {
	return fmt.Sprintf("User(id=%d, name=%s, email=%s, role=%s)",
		u.ID, u.name, u.email, u.role)
}

// Validate checks if the user data is valid
func (u *User) Validate() error {
	if strings.TrimSpace(u.name) == "" {
		return NewUserError(ErrInvalidName, "name cannot be empty")
	}

	if err := validateEmail(u.email); err != nil {
		return err
	}

	if !u.role.IsValid() {
		return NewUserError(ErrInvalidRole, "invalid user role")
	}

	return nil
}

// CreateValidatedUser creates a user with validation
func CreateValidatedUser(name, email string, role UserRole) (*User, error) {
	user := NewUser(name, email, role)
	if err := user.Validate(); err != nil {
		return nil, err
	}
	return user, nil
}

// Package-level utility functions

// validateEmail validates email format (unexported helper)
func validateEmail(email string) error {
	email = strings.TrimSpace(email)
	if email == "" {
		return NewUserError(ErrInvalidEmail, "email cannot be empty")
	}

	if !strings.Contains(email, "@") {
		return NewUserError(ErrInvalidEmail, "invalid email format")
	}

	if len(email) > MaxEmailLength {
		return NewUserError(ErrInvalidEmail, fmt.Sprintf("email too long (max %d characters)", MaxEmailLength))
	}

	return nil
}

// CreateGuestUser creates a guest user with auto-generated email
func CreateGuestUser(name string) *User {
	email := fmt.Sprintf("%s@guest.local", strings.ToLower(strings.ReplaceAll(name, " ", ".")))
	return NewUser(name, email, RoleGuest)
}

// GetUserStats returns statistics about created users
func GetUserStats() UserStats {
	return UserStats{
		TotalCreated: userCounter,
		DefaultRole:  DefaultRole.String(),
	}
}

// UserStats holds user creation statistics
type UserStats struct {
	TotalCreated int
	DefaultRole  string
}

// Error handling types and constants
type UserErrorCode int

const (
	ErrInvalidName UserErrorCode = iota
	ErrInvalidEmail
	ErrInvalidRole
	ErrUserNotFound
	ErrDuplicateEmail
)

// UserError represents user-related errors
type UserError struct {
	Code    UserErrorCode
	Message string
}

func NewUserError(code UserErrorCode, message string) *UserError {
	return &UserError{
		Code:    code,
		Message: message,
	}
}

func (e *UserError) Error() string {
	return fmt.Sprintf("user error [%d]: %s", int(e.Code), e.Message)
}

// Is implements error comparison for Go 1.13+ error handling
func (e *UserError) Is(target error) bool {
	if other, ok := target.(*UserError); ok {
		return e.Code == other.Code
	}
	return false
}

// Package initialization
func init() {
	fmt.Println("[INIT] Models package initialized")
	// Initialize package-level data if needed
}
