//! Provider registry for managing project configuration resolvers

use super::provider::ProjectResolutionProvider;
use std::sync::Arc;

/// Trait for accessing registered project configuration providers
pub trait ResolutionProviderRegistry {
    fn providers(&self) -> &[Arc<dyn ProjectResolutionProvider>];
}

/// Simple registry implementation for project configuration providers
pub struct SimpleProviderRegistry {
    providers: Vec<Arc<dyn ProjectResolutionProvider>>,
}

impl Default for SimpleProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SimpleProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    pub fn add(&mut self, provider: Arc<dyn ProjectResolutionProvider>) {
        self.providers.push(provider);
    }

    /// Get only the providers that are currently active based on settings
    pub fn active_providers(
        &self,
        settings: &crate::config::Settings,
    ) -> Vec<Arc<dyn ProjectResolutionProvider>> {
        self.providers
            .iter()
            .filter(|p| p.is_enabled(settings))
            .cloned()
            .collect()
    }
}

impl ResolutionProviderRegistry for SimpleProviderRegistry {
    fn providers(&self) -> &[Arc<dyn ProjectResolutionProvider>] {
        &self.providers
    }
}
