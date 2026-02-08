//! C# language definition and registration

use crate::parsing::{
    LanguageBehavior, LanguageDefinition, LanguageId, LanguageParser, LanguageRegistry,
};
use crate::{IndexError, IndexResult, Settings};
use std::sync::Arc;

use super::{CSharpBehavior, CSharpParser};

/// C# language definition
pub struct CSharpLanguage;

impl LanguageDefinition for CSharpLanguage {
    fn id(&self) -> LanguageId {
        LanguageId::new("csharp")
    }

    fn name(&self) -> &'static str {
        "C#"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["cs", "csx", "cshtml"]
    }

    fn create_parser(&self, _settings: &Settings) -> IndexResult<Box<dyn LanguageParser>> {
        let parser = CSharpParser::new().map_err(|e| IndexError::General(e.to_string()))?;
        Ok(Box::new(parser))
    }

    fn create_behavior(&self) -> Box<dyn LanguageBehavior> {
        Box::new(CSharpBehavior::new())
    }

    fn default_enabled(&self) -> bool {
        true // Enable C# by default
    }

    fn is_enabled(&self, settings: &Settings) -> bool {
        settings
            .languages
            .get("csharp")
            .map(|config| config.enabled)
            .unwrap_or(self.default_enabled())
    }
}

/// Register C# language with the registry
pub(crate) fn register(registry: &mut LanguageRegistry) {
    registry.register(Arc::new(CSharpLanguage));
}
