//! TypeScript language parser implementation

pub mod audit;
pub mod behavior;
pub mod definition;
pub mod parser;
pub mod resolution;
pub mod tsconfig;

pub use behavior::TypeScriptBehavior;
pub use definition::TypeScriptLanguage;
pub use parser::TypeScriptParser;
pub use resolution::{TypeScriptInheritanceResolver, TypeScriptResolutionContext};
pub use tsconfig::{
    CompilerOptions, PathAliasResolver, PathRule, TsConfig, parse_jsonc_tsconfig, read_tsconfig,
    resolve_extends_chain,
};

// Re-export for registry registration
pub(crate) use definition::register;
