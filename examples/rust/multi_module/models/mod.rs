//! Models module - contains all data structures and business objects
//! 
//! This module demonstrates:
//! - Module organization and re-exports
//! - Public/private visibility within modules
//! - Cross-module type definitions

// Declare submodules
pub mod user;

// Re-export commonly used types at module level
pub use user::{User, UserRole, UserError};

// Module-level constants
pub const MODULE_VERSION: &str = "1.0.0";

// Module-level type alias
pub type UserId = u64;

// Private helper function (not re-exported)
fn validate_module_invariants() -> bool {
    true
}

// Public helper function
pub fn get_module_info() -> String {
    format!("Models module v{}", MODULE_VERSION)
}