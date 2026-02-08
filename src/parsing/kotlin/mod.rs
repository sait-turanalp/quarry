//! Kotlin language parser implementation

pub mod audit;
pub mod behavior;
pub mod definition;
pub mod parser;
pub mod resolution;

pub use audit::KotlinParserAudit;
pub use behavior::KotlinBehavior;
pub use definition::KotlinLanguage;
pub use parser::KotlinParser;
pub use resolution::{KotlinInheritanceResolver, KotlinResolutionContext};

// Re-export for registry registration
pub(crate) use definition::register;
