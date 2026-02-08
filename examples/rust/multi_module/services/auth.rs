//! Authentication service implementation
//! 
//! This module demonstrates:
//! - Service implementation with dependencies
//! - Cross-module type usage
//! - Error handling patterns

use std::collections::HashMap;
use std::fmt;

// Import from other modules in this crate
use crate::models::user::{User, UserError};
use crate::services::database::DatabaseConnection;

// Import from standard library
use std::time::{SystemTime, Duration};

/// Authentication token type
pub type AuthToken = String;

/// Authentication service
pub struct AuthService {
    database: DatabaseConnection,
    sessions: HashMap<AuthToken, String>, // token -> email
    users: HashMap<String, User>, // email -> user
    token_counter: u64,
}

impl AuthService {
    /// Create new authentication service
    pub fn new(database: DatabaseConnection) -> Self {
        Self {
            database,
            sessions: HashMap::new(),
            users: HashMap::new(),
            token_counter: 0,
        }
    }
    
    /// Register a new user
    pub fn register_user(&self, user: &User) -> Result<(), AuthError> {
        // Use the database connection (demonstrating cross-module usage)
        self.database.execute("INSERT INTO users (name, email, role) VALUES (?, ?, ?)", None)?;
        
        if self.users.contains_key(&user.email) {
            return Err(AuthError::DuplicateEmail);
        }
        
        // In a real implementation, we'd store in the database
        // For this example, we just validate the operation
        self.validate_user(user)?;
        
        Ok(())
    }
    
    /// Authenticate user and create session
    pub fn authenticate(&self, email: &str, password: &str) -> Result<AuthToken, AuthError> {
        let _user = self.users.get(email).ok_or(AuthError::UserNotFound)?;
        
        // Mock password validation
        if password.len() < 6 {
            return Err(AuthError::InvalidCredentials);
        }
        
        // Generate token
        let token = self.generate_token();
        
        // In a real implementation, we'd store the session
        Ok(token)
    }
    
    /// Validate session token
    pub fn validate_session(&self, token: &AuthToken) -> Result<&User, AuthError> {
        let email = self.sessions.get(token).ok_or(AuthError::InvalidToken)?;
        self.users.get(email).ok_or(AuthError::UserNotFound)
    }
    
    /// Logout user
    pub fn logout(&mut self, token: &AuthToken) -> Result<(), AuthError> {
        self.sessions.remove(token);
        Ok(())
    }
    
    /// Private helper to validate user
    fn validate_user(&self, user: &User) -> Result<(), AuthError> {
        if user.name.is_empty() {
            return Err(AuthError::ValidationError("Name cannot be empty".to_string()));
        }
        Ok(())
    }
    
    /// Private helper to generate tokens
    fn generate_token(&self) -> AuthToken {
        format!("token_{}", SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_secs())
    }
    
    /// Get user count (package-private for testing)
    pub(crate) fn user_count(&self) -> usize {
        self.users.len()
    }
}

/// Authentication errors
#[derive(Debug, Clone)]
pub enum AuthError {
    UserNotFound,
    DuplicateEmail,
    InvalidCredentials,
    InvalidToken,
    ValidationError(String),
    DatabaseError(String),
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::UserNotFound => write!(f, "User not found"),
            AuthError::DuplicateEmail => write!(f, "Email already registered"),
            AuthError::InvalidCredentials => write!(f, "Invalid credentials"),
            AuthError::InvalidToken => write!(f, "Invalid or expired token"),
            AuthError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            AuthError::DatabaseError(msg) => write!(f, "Database error: {}", msg),
        }
    }
}

impl std::error::Error for AuthError {}

// Conversion from database errors
impl From<super::database::DatabaseError> for AuthError {
    fn from(error: super::database::DatabaseError) -> Self {
        AuthError::DatabaseError(error.to_string())
    }
}

// Conversion from user errors
impl From<UserError> for AuthError {
    fn from(error: UserError) -> Self {
        match error {
            UserError::UserNotFound => AuthError::UserNotFound,
            UserError::DuplicateEmail => AuthError::DuplicateEmail,
            other => AuthError::ValidationError(other.to_string()),
        }
    }
}

// Module-level helper functions
pub fn hash_password(password: &str) -> String {
    // Mock password hashing
    format!("hashed_{}", password)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    // Mock password verification
    hash == &hash_password(password)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::user::{User, UserRole};
    use crate::services::database::DatabaseConnection;
    
    fn create_test_auth_service() -> AuthService {
        let db = DatabaseConnection::new("test://memory".to_string());
        AuthService::new(db)
    }
    
    #[test]
    fn test_user_registration() {
        let auth = create_test_auth_service();
        let user = User::new(
            "Test User".to_string(),
            "test@example.com".to_string(),
            UserRole::User,
        );
        
        // Registration should succeed
        assert!(auth.register_user(&user).is_ok());
    }
    
    #[test]
    fn test_authentication_failure() {
        let auth = create_test_auth_service();
        
        // Authentication should fail for non-existent user
        let result = auth.authenticate("nonexistent@example.com", "password123");
        assert!(matches!(result, Err(AuthError::UserNotFound)));
    }
    
    #[test]
    fn test_password_hashing() {
        let password = "test123";
        let hash = hash_password(password);
        assert!(verify_password(password, &hash));
        assert!(!verify_password("wrong", &hash));
    }
}