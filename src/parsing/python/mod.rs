//! Python language parser implementation

pub mod audit;
pub mod behavior;
pub mod definition;
pub mod parser;
pub mod resolution;

pub use behavior::PythonBehavior;
pub use definition::PythonLanguage;
pub use parser::PythonParser;
pub use resolution::{PythonInheritanceResolver, PythonResolutionContext};

// Re-export for registry registration
pub(crate) use definition::register;
