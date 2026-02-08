//! C language parser implementation

pub mod audit;
pub mod behavior;
pub mod definition;
pub mod parser;
pub mod resolution;

pub use audit::CParserAudit;
pub use behavior::CBehavior;
pub use definition::CLanguage;
pub use parser::CParser;
pub use resolution::{CInheritanceResolver, CResolutionContext};

// Re-export for registry registration
pub(crate) use definition::register;
