//! Core provider trait for project configuration resolution

use std::path::PathBuf;

use super::{ResolutionResult, Sha256Hash};
use crate::config::Settings;

/// Core trait implemented by language-specific project configuration providers.
///
/// Each language (TypeScript, Python, Go, etc.) implements this trait to provide
/// resolution logic for its project configuration files (tsconfig.json, pyproject.toml, go.mod).
pub trait ProjectResolutionProvider: Send + Sync {
    /// Language identifier (e.g., "typescript", "python", "go")
    fn language_id(&self) -> &'static str;

    /// Check if this provider is enabled in the current settings
    fn is_enabled(&self, settings: &Settings) -> bool;

    /// Get the configuration file paths this provider manages
    fn config_paths(&self, settings: &Settings) -> Vec<PathBuf>;

    /// Compute SHA-256 hashes for the configuration files
    fn compute_shas(
        &self,
        configs: &[PathBuf],
    ) -> ResolutionResult<std::collections::HashMap<PathBuf, Sha256Hash>>;

    /// Rebuild the provider's cache from settings
    fn rebuild_cache(&self, settings: &Settings) -> ResolutionResult<()>;

    /// Select files affected by configuration changes
    fn select_affected_files(&self, settings: &Settings) -> Vec<PathBuf>;
}
