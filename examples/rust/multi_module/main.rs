//! Multi-module Rust application for testing cross-module imports and resolution
//! 
//! This example demonstrates:
//! - Cross-module imports using absolute and relative paths
//! - Re-exports and visibility modifiers
//! - Nested module structures
//! - Public and private item resolution

// Module declarations
mod models;
mod services;
mod config;
mod utils;

// Import from modules using absolute paths
use crate::models::user::{User, UserRole};
use crate::services::auth::AuthService;
use crate::services::database::DatabaseConnection;
use crate::config::settings::Settings;

// Import with aliases from modules
use crate::utils::helper::{format_output, validate_input};
use crate::utils::helper::DataProcessor as Processor;

// Re-export some items at crate level
pub use models::user::UserRole as PublicUserRole;
pub use services::auth::AuthError;

use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Multi-Module Rust Application ===\n");

    // TEST 1: Cross-module type usage
    println!("1. Cross-module type instantiation:");
    let user = User::new("Alice".to_string(), "alice@example.com".to_string(), UserRole::Admin);
    println!("   Created user: {}", user);

    // TEST 2: Service initialization with cross-module dependencies
    println!("\n2. Service initialization:");
    let settings = Settings::new();
    println!("   Settings loaded: {:?}", settings);
    
    let db = DatabaseConnection::new(settings.db_url());
    db.connect()?;
    println!("   Database connected ✓");

    let auth_service = AuthService::new(db);
    println!("   Auth service initialized ✓");

    // TEST 3: Cross-module function calls
    println!("\n3. Cross-module function calls:");
    if validate_input(&user.email) {
        println!("   Email validation passed ✓");
        
        auth_service.register_user(&user)?;
        let output = format_output(&format!("User {} registered", user.name));
        println!("   {}", output);
    }

    // TEST 4: Generic cross-module types
    println!("\n4. Generic cross-module usage:");
    let mut processor = Processor::new(HashMap::from([
        ("transform".to_string(), "uppercase".to_string()),
    ]));
    let processed = processor.process("hello world");
    println!("   Processed data: {}", processed);

    // TEST 5: Re-exported types
    println!("\n5. Re-exported type usage:");
    let role: PublicUserRole = PublicUserRole::User;
    println!("   Using re-exported UserRole: {:?}", role);

    // TEST 6: Error handling across modules
    println!("\n6. Cross-module error handling:");
    match auth_service.authenticate("alice@example.com", "wrong_password") {
        Ok(token) => println!("   Authentication successful: {}", token),
        Err(e) => println!("   Authentication failed: {} ✓", e),
    }

    println!("\n=== All cross-module tests completed ===");
    Ok(())
}

// Test module with cross-module imports
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::models::user::User;
    use crate::services::auth::{AuthService, AuthError};
    
    #[test]
    fn test_cross_module_integration() {
        let user = User::new(
            "Test User".to_string(),
            "test@example.com".to_string(),
            UserRole::User,
        );
        
        let settings = Settings::new();
        let db = DatabaseConnection::new(settings.db_url());
        let auth = AuthService::new(db);
        
        // Test that we can register a user across modules
        assert!(auth.register_user(&user).is_ok());
    }
    
    #[test]
    fn test_re_exported_types() {
        // Test using re-exported type
        let _role: PublicUserRole = PublicUserRole::Admin;
        
        // Test that error types work across modules
        let error = AuthError::UserNotFound;
        assert_eq!(error.to_string(), "User not found");
    }
}