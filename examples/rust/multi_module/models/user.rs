//! User model definitions and related functionality
//! 
//! This module demonstrates:
//! - Enum definitions with various variants
//! - Struct definitions with methods
//! - Error types
//! - Visibility modifiers

use std::fmt;

/// User role enumeration
#[derive(Debug, Clone, PartialEq)]
pub enum UserRole {
    Admin,
    User,
    Guest,
}

impl fmt::Display for UserRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UserRole::Admin => write!(f, "admin"),
            UserRole::User => write!(f, "user"),
            UserRole::Guest => write!(f, "guest"),
        }
    }
}

/// User struct representing a system user
#[derive(Debug, Clone)]
pub struct User {
    pub name: String,
    pub email: String,
    pub role: UserRole,
    id: Option<u64>, // Private field
    created_at: std::time::SystemTime,
}

impl User {
    /// Create a new user
    pub fn new(name: String, email: String, role: UserRole) -> Self {
        Self {
            name,
            email,
            role,
            id: None,
            created_at: std::time::SystemTime::now(),
        }
    }
    
    /// Get user ID (if set)
    pub fn id(&self) -> Option<u64> {
        self.id
    }
    
    /// Set user ID (package-private)
    pub(crate) fn set_id(&mut self, id: u64) {
        self.id = Some(id);
    }
    
    /// Check if user is admin
    pub fn is_admin(&self) -> bool {
        matches!(self.role, UserRole::Admin)
    }
    
    /// Get creation timestamp (module-private)
    pub(super) fn created_at(&self) -> std::time::SystemTime {
        self.created_at
    }
    
    /// Validate user data (private method)
    fn validate(&self) -> Result<(), UserError> {
        if self.name.is_empty() {
            return Err(UserError::InvalidName);
        }
        if !self.email.contains('@') {
            return Err(UserError::InvalidEmail);
        }
        Ok(())
    }
    
    /// Create user with validation
    pub fn create_validated(name: String, email: String, role: UserRole) -> Result<Self, UserError> {
        let user = Self::new(name, email, role);
        user.validate()?;
        Ok(user)
    }
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "User(name: {}, email: {}, role: {})", self.name, self.email, self.role)
    }
}

/// User-related errors
#[derive(Debug, Clone)]
pub enum UserError {
    InvalidName,
    InvalidEmail,
    UserNotFound,
    DuplicateEmail,
}

impl fmt::Display for UserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UserError::InvalidName => write!(f, "Invalid user name"),
            UserError::InvalidEmail => write!(f, "Invalid email address"),
            UserError::UserNotFound => write!(f, "User not found"),
            UserError::DuplicateEmail => write!(f, "Email already exists"),
        }
    }
}

impl std::error::Error for UserError {}

// Module-level constants
pub const MAX_NAME_LENGTH: usize = 100;
pub const MAX_EMAIL_LENGTH: usize = 254;

// Module-level helper function (private)
fn normalize_email(email: &str) -> String {
    email.to_lowercase().trim().to_string()
}

// Public utility function
pub fn create_guest_user(name: String) -> User {
    let email = format!("{}@guest.local", name.to_lowercase().replace(' ', "."));
    User::new(name, email, UserRole::Guest)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_user_creation() {
        let user = User::new(
            "John Doe".to_string(),
            "john@example.com".to_string(),
            UserRole::User,
        );
        assert_eq!(user.name, "John Doe");
        assert_eq!(user.role, UserRole::User);
        assert!(user.id().is_none());
    }
    
    #[test]
    fn test_user_validation() {
        let result = User::create_validated(
            "".to_string(),
            "invalid-email".to_string(),
            UserRole::User,
        );
        assert!(result.is_err());
        
        let result = User::create_validated(
            "Valid Name".to_string(),
            "valid@email.com".to_string(),
            UserRole::User,
        );
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_guest_user_creation() {
        let guest = create_guest_user("Jane Smith".to_string());
        assert_eq!(guest.role, UserRole::Guest);
        assert!(guest.email.contains("@guest.local"));
    }
}