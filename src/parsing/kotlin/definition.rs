//! Kotlin language definition for the registry
//!
//! Provides the language metadata and glue code used by the language registry
//! to instantiate parsers and behaviors for Kotlin.

use std::sync::Arc;

use super::{KotlinBehavior, KotlinParser};
use crate::parsing::{LanguageBehavior, LanguageDefinition, LanguageId, LanguageParser};
use crate::{IndexError, IndexResult, Settings};

/// Language definition for Kotlin
pub struct KotlinLanguage;

impl KotlinLanguage {
    /// Stable identifier used throughout the registry
    pub const ID: LanguageId = LanguageId::new("kotlin");
}

impl LanguageDefinition for KotlinLanguage {
    fn id(&self) -> LanguageId {
        Self::ID
    }

    fn name(&self) -> &'static str {
        "Kotlin"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["kt", "kts"]
    }

    fn create_parser(&self, _settings: &Settings) -> IndexResult<Box<dyn LanguageParser>> {
        let parser = KotlinParser::new().map_err(IndexError::General)?;
        Ok(Box::new(parser))
    }

    fn create_behavior(&self) -> Box<dyn LanguageBehavior> {
        Box::new(KotlinBehavior::new())
    }

    fn default_enabled(&self) -> bool {
        true // Kotlin support is enabled by default
    }

    fn is_enabled(&self, settings: &Settings) -> bool {
        settings
            .languages
            .get(self.id().as_str())
            .map(|config| config.enabled)
            .unwrap_or(self.default_enabled())
    }
}

/// Register Kotlin language with the global registry
pub(crate) fn register(registry: &mut crate::parsing::LanguageRegistry) {
    registry.register(Arc::new(KotlinLanguage));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_metadata() {
        let lang = KotlinLanguage;

        assert_eq!(lang.id(), LanguageId::new("kotlin"));
        assert_eq!(lang.name(), "Kotlin");
        assert_eq!(lang.extensions(), &["kt", "kts"]);
    }

    #[test]
    fn test_default_enabled_flag() {
        let lang = KotlinLanguage;
        assert!(lang.default_enabled());

        let settings = Settings::default();
        assert_eq!(lang.is_enabled(&settings), lang.default_enabled());
    }

    #[test]
    fn test_parser_creation() {
        let lang = KotlinLanguage;
        let settings = Settings::default();
        let parser = lang.create_parser(&settings);
        assert!(parser.is_ok());
    }
}
