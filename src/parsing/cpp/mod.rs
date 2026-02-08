//! C++ language parser implementation

pub mod audit;
pub mod behavior;
pub mod definition;
pub mod parser;
pub mod resolution;

pub use audit::CppParserAudit;
pub use behavior::CppBehavior;
pub use definition::CppLanguage;
pub use parser::CppParser;
pub use resolution::{CppInheritanceResolver, CppResolutionContext};

// Re-export for registry registration
pub(crate) use definition::register;
