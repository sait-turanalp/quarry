//! Rust language parser implementation

pub mod audit;
pub mod behavior;
pub mod definition;
pub mod parser;
pub mod resolution;

pub use behavior::RustBehavior;
pub use definition::RustLanguage;
pub use parser::RustParser;
pub use resolution::{RustResolutionContext, RustTraitResolver};

// Re-export for registry registration
pub(crate) use definition::register;
