//! Services module - contains business logic and external integrations
//! 
//! This module demonstrates:
//! - Service layer organization
//! - Cross-module dependencies
//! - Public API surface management

// Declare submodules
pub mod auth;
pub mod database;

// Re-export service types
pub use auth::{AuthService, AuthError, AuthToken};
pub use database::{DatabaseConnection, DatabaseError, QueryResult};

// Import from other modules in this crate
use crate::models::{User, UserRole};

// Module-level type aliases
pub type ServiceResult<T> = Result<T, ServiceError>;

// Combined service error type
#[derive(Debug)]
pub enum ServiceError {
    Auth(AuthError),
    Database(DatabaseError),
    Config(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::Auth(e) => write!(f, "Authentication error: {}", e),
            ServiceError::Database(e) => write!(f, "Database error: {}", e),
            ServiceError::Config(msg) => write!(f, "Configuration error: {}", msg),
        }
    }
}

impl std::error::Error for ServiceError {}

impl From<AuthError> for ServiceError {
    fn from(error: AuthError) -> Self {
        ServiceError::Auth(error)
    }
}

impl From<DatabaseError> for ServiceError {
    fn from(error: DatabaseError) -> Self {
        ServiceError::Database(error)
    }
}

// Module-level service coordinator
pub struct ServiceCoordinator {
    auth: AuthService,
    _database: DatabaseConnection,
}

impl ServiceCoordinator {
    pub fn new(auth: AuthService, database: DatabaseConnection) -> Self {
        Self {
            auth,
            _database: database,
        }
    }
    
    pub fn create_admin_user(&self, name: String, email: String) -> ServiceResult<()> {
        let admin = User::new(name, email, UserRole::Admin);
        self.auth.register_user(&admin)?;
        Ok(())
    }
}