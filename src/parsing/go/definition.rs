//! Go language definition and registration
//!
//! This module defines the Go language support for Codanna, providing
//! Tree-sitter-based parsing and symbol extraction for Go codebases.
//!
//! ## AST Node Types and Symbol Mappings
//!
//! The Go parser uses Tree-sitter-go v0.23.4 and handles the following
//! primary node types and their corresponding symbol classifications:
//!
//! ### Type Declarations (`type_declaration`)
//! - **Struct types** (`struct_type`) → `SymbolKind::Struct`
//! - **Interface types** (`interface_type`) → `SymbolKind::Interface`
//! - **Type aliases** (`type_alias`) → `SymbolKind::TypeAlias`
//!
//! ### Function and Method Declarations
//! - **Functions** (`function_declaration`) → `SymbolKind::Function`
//! - **Methods** (`method_declaration` with receiver) → `SymbolKind::Method`
//!
//! ### Variable and Constant Declarations
//! - **Variables** (`var_declaration`) → `SymbolKind::Variable`
//! - **Constants** (`const_declaration`) → `SymbolKind::Constant`
//!
//! ### Field Declarations
//! - **Struct fields** (`field_declaration`) → `SymbolKind::Field`
//! - **Interface methods** (`method_elem`) → `SymbolKind::Method`
//!
//! ### Import and Package System
//! - **Package clause** (`package_clause`) - Handled for module resolution
//! - **Import declarations** (`import_declaration`) - Processed for symbol resolution
//!
//! ## Go-Specific Language Features
//!
//! The Go parser handles unique Go constructs including:
//! - Method receivers (pointer and value receivers)
//! - Embedded structs and interfaces
//! - Generic types and type constraints (Go 1.18+)
//! - Package-level visibility via capitalization
//! - Channel operations and goroutines
//! - Interface implementation detection (implicit)
//!
//! For complete node type mappings and detailed examples, see:
//! `contributing/parsers/go/NODE_MAPPING.md`

use crate::parsing::{
    LanguageBehavior, LanguageDefinition, LanguageId, LanguageParser, LanguageRegistry,
};
use crate::{IndexError, IndexResult, Settings};
use std::sync::Arc;

use super::{GoBehavior, GoParser};

/// Go language definition
///
/// Provides factory methods for creating Go parsers and behaviors,
/// and defines language metadata like file extensions and identification.
pub struct GoLanguage;

impl LanguageDefinition for GoLanguage {
    fn id(&self) -> LanguageId {
        LanguageId::new("go")
    }

    fn name(&self) -> &'static str {
        "Go"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["go"]
    }

    fn create_parser(&self, _settings: &Settings) -> IndexResult<Box<dyn LanguageParser>> {
        let parser = GoParser::new().map_err(|e| IndexError::General(e.to_string()))?;
        Ok(Box::new(parser))
    }

    fn create_behavior(&self) -> Box<dyn LanguageBehavior> {
        Box::new(GoBehavior::new())
    }

    fn default_enabled(&self) -> bool {
        true // Enable Go by default
    }

    fn is_enabled(&self, settings: &Settings) -> bool {
        settings
            .languages
            .get("Go")
            .map(|config| config.enabled)
            .unwrap_or(self.default_enabled())
    }
}

/// Register Go language with the registry
pub(crate) fn register(registry: &mut LanguageRegistry) {
    registry.register(Arc::new(GoLanguage));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_go_language_id() {
        let go_lang = GoLanguage;
        assert_eq!(go_lang.id(), LanguageId::new("go"));
    }

    #[test]
    fn test_go_language_name() {
        let go_lang = GoLanguage;
        assert_eq!(go_lang.name(), "Go");
    }

    #[test]
    fn test_go_file_extensions() {
        let go_lang = GoLanguage;
        assert_eq!(go_lang.extensions(), &["go"]);
    }

    #[test]
    fn test_go_enabled_by_default() {
        let go_lang = GoLanguage;
        assert!(go_lang.default_enabled());
    }

    #[test]
    fn test_go_enabled_with_default_settings() {
        let go_lang = GoLanguage;
        let settings = Settings::default();
        assert!(go_lang.is_enabled(&settings));
    }

    #[test]
    fn test_go_parser_creation() {
        let go_lang = GoLanguage;
        let settings = Settings::default();

        let parser_result = go_lang.create_parser(&settings);
        assert!(parser_result.is_ok(), "Go parser creation should succeed");

        // Verify we get a valid parser
        let parser = parser_result.unwrap();
        assert_eq!(parser.language(), crate::parsing::Language::Go);
    }

    #[test]
    fn test_go_behavior_creation() {
        let go_lang = GoLanguage;
        let behavior = go_lang.create_behavior();

        // Verify the behavior has correct Go-specific properties
        assert_eq!(behavior.module_separator(), "/");
        assert!(behavior.supports_inherent_methods());
        assert!(!behavior.supports_traits()); // Go has interfaces, not traits
    }

    #[test]
    fn test_go_language_registry_registration() {
        use crate::parsing::LanguageRegistry;

        let mut registry = LanguageRegistry::new();
        register(&mut registry);

        // Verify Go language is registered
        let go_id = LanguageId::new("go");
        assert!(registry.get(go_id).is_some());
    }

    #[test]
    fn test_go_file_extension_recognition() {
        use crate::parsing::LanguageRegistry;

        let mut registry = LanguageRegistry::new();
        register(&mut registry);

        // Test that .go files are recognized as Go
        let detected = registry.get_by_extension("go");
        assert!(detected.is_some());
        assert_eq!(detected.unwrap().id(), LanguageId::new("go"));
    }

    #[test]
    fn test_go_factory_methods_consistency() {
        let go_lang = GoLanguage;
        let settings = Settings::default();

        // Create parser and behavior
        let parser = go_lang.create_parser(&settings).unwrap();
        let _behavior = go_lang.create_behavior();

        // Verify they're consistent with the language definition
        assert_eq!(parser.language(), crate::parsing::Language::Go);
        assert_eq!(parser.language().to_language_id(), go_lang.id());

        // Both should handle Go constructs appropriately
        let go_id = go_lang.id();
        assert_eq!(go_id.as_str(), "go");
    }
}
