//! Profile manifest parsing and validation

use super::error::{ProfileError, ProfileResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Profile manifest structure
/// Located at profiles/{name}/profile.json within provider repositories
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProfileManifest {
    /// Profile name
    pub name: String,

    /// Profile version
    pub version: String,

    /// Provider name (defaults to profile name if not specified)
    #[serde(default)]
    pub provider: Option<String>,

    /// Files to install (relative to profile directory)
    #[serde(default)]
    pub files: Vec<String>,
}

impl ProfileManifest {
    /// Get provider name (defaults to profile name)
    pub fn provider_name(&self) -> &str {
        self.provider.as_deref().unwrap_or(&self.name)
    }
}

impl ProfileManifest {
    /// Load profile manifest from JSON file
    pub fn from_file(path: &Path) -> ProfileResult<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
    }

    /// Parse profile manifest from JSON string
    pub fn from_json(json: &str) -> ProfileResult<Self> {
        let mut manifest: Self = serde_json::from_str(json)?;
        // Filter out empty file paths
        manifest.files.retain(|f| !f.is_empty());
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> ProfileResult<()> {
        if self.name.is_empty() {
            return Err(ProfileError::InvalidManifest {
                reason: "Profile name cannot be empty".to_string(),
            });
        }
        if self.version.is_empty() {
            return Err(ProfileError::InvalidManifest {
                reason: "Profile version cannot be empty".to_string(),
            });
        }
        Ok(())
    }
}
