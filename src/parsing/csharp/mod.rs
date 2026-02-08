//! C# language support for codanna
//!
//! This module provides complete C# language parsing and analysis capabilities:
//!
//! # Supported Features
//!
//! ## Symbol Extraction
//! - Classes, structs, records, interfaces
//! - Methods, constructors, properties, fields, events
//! - Enums and enum members
//! - Visibility modifiers (public, private, internal, protected)
//! - Method and type signatures
//!
//! ## Relationship Detection
//! - Method calls with proper caller context
//! - Interface implementations
//! - Using directives (imports)
//!
//! ## Code Intelligence
//! - Namespace/module path tracking
//! - Symbol resolution with proper scoping
//! - Import resolution
//!
//! # Architecture
//!
//! - [`parser`] - Tree-sitter AST traversal and symbol extraction
//! - [`behavior`] - Language-specific processing rules
//! - [`resolution`] - Symbol lookup and name resolution
//! - [`definition`] - Language registration and configuration
//!
//! # Example
//!
//! ```no_run
//! use codanna::parsing::csharp::{CSharpParser, CSharpBehavior};
//! use codanna::parsing::LanguageParser;
//!
//! // Create parser and parse C# code
//! let mut parser = CSharpParser::new().expect("Failed to create parser");
//! let behavior = CSharpBehavior::new();
//! // Use parser to extract symbols...
//! ```

pub mod audit;
pub mod behavior;
pub mod definition;
pub mod parser;
pub mod resolution;

pub use behavior::CSharpBehavior;
pub use definition::CSharpLanguage;
pub use parser::CSharpParser;

// Re-export for registry registration
pub(crate) use definition::register;
