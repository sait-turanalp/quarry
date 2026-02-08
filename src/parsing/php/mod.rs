//! PHP language parser implementation

pub mod audit;
pub mod behavior;
pub mod definition;
pub mod parser;
pub mod resolution;

pub use behavior::PhpBehavior;
pub use definition::PhpLanguage;
pub use parser::PhpParser;
pub use resolution::{PhpInheritanceResolver, PhpResolutionContext};

// Re-export for registry registration
pub(crate) use definition::register;
