//! TypeScript language definition and registration

use crate::parsing::{
    LanguageBehavior, LanguageDefinition, LanguageId, LanguageParser, LanguageRegistry,
};
use crate::{IndexError, IndexResult, Settings};
use std::sync::Arc;

use super::{TypeScriptBehavior, TypeScriptParser};

/// TypeScript language definition
pub struct TypeScriptLanguage;

impl LanguageDefinition for TypeScriptLanguage {
    fn id(&self) -> LanguageId {
        LanguageId::new("typescript")
    }

    fn name(&self) -> &'static str {
        "TypeScript"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["ts", "tsx", "mts", "cts"]
    }

    fn create_parser(&self, _settings: &Settings) -> IndexResult<Box<dyn LanguageParser>> {
        let parser = TypeScriptParser::new().map_err(|e| IndexError::General(e.to_string()))?;
        Ok(Box::new(parser))
    }

    fn create_behavior(&self) -> Box<dyn LanguageBehavior> {
        Box::new(TypeScriptBehavior::new())
    }

    fn default_enabled(&self) -> bool {
        true // Enable TypeScript by default
    }

    fn is_enabled(&self, settings: &Settings) -> bool {
        settings
            .languages
            .get("typescript")
            .map(|config| config.enabled)
            .unwrap_or(self.default_enabled())
    }
}

/// Register TypeScript language with the registry
pub(crate) fn register(registry: &mut LanguageRegistry) {
    registry.register(Arc::new(TypeScriptLanguage));
}
