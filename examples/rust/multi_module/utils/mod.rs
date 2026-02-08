//! Utilities module - common helper functions and utilities
//! 
//! This module demonstrates:
//! - Utility function organization
//! - Common helper patterns
//! - Module-level re-exports

pub mod helper;

// Re-export commonly used utilities
pub use helper::{format_output, validate_input, DataProcessor};

// Module-level type aliases
pub type ProcessorConfig = std::collections::HashMap<String, String>;
pub type ValidationResult = Result<(), ValidationError>;

// Module-level constants
pub const MODULE_NAME: &str = "utils";
pub const VERSION: &str = "1.0.0";

/// Validation error type
#[derive(Debug, Clone)]
pub enum ValidationError {
    Empty,
    TooShort(usize),
    TooLong(usize),
    InvalidFormat(String),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::Empty => write!(f, "Value cannot be empty"),
            ValidationError::TooShort(min) => write!(f, "Value too short (minimum: {})", min),
            ValidationError::TooLong(max) => write!(f, "Value too long (maximum: {})", max),
            ValidationError::InvalidFormat(expected) => write!(f, "Invalid format (expected: {})", expected),
        }
    }
}

impl std::error::Error for ValidationError {}

// Module-level utility functions
pub fn get_module_info() -> String {
    format!("{} v{}", MODULE_NAME, VERSION)
}

pub fn create_default_processor() -> DataProcessor {
    let config = ProcessorConfig::from([
        ("mode".to_string(), "standard".to_string()),
        ("encoding".to_string(), "utf8".to_string()),
    ]);
    DataProcessor::new(config)
}