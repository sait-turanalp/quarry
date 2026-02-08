//! Global provider registry management
//!
//! Manages registered providers in ~/.codanna/providers.json
//! Similar to how plugin marketplaces are managed.

use super::error::ProfileResult;
use super::provider::ProviderManifest;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Global provider registry stored at ~/.codanna/providers.json
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderRegistry {
    /// Registry format version
    pub version: u32,

    /// Registered providers
    #[serde(default)]
    pub providers: HashMap<String, RegisteredProvider>,
}

/// Information about a registered provider
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegisteredProvider {
    /// Provider name
    pub name: String,

    /// Source information
    pub source: ProviderSource,

    /// Provider namespace (e.g., ".claude")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    /// Available profiles from this provider
    #[serde(default)]
    pub profiles: HashMap<String, ProfileInfo>,

    /// When this provider was last updated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

/// Provider source descriptor
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProviderSource {
    /// GitHub repository (owner/repo shorthand)
    Github { repo: String },
    /// Git URL (any git server: GitLab, Bitbucket, self-hosted, etc.)
    Url { url: String },
    /// Local directory path
    Local { path: String },
}

/// Profile information cached from provider
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProfileInfo {
    /// Profile version
    pub version: String,

    /// Profile description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ProviderRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            version: 1,
            providers: HashMap::new(),
        }
    }

    /// Load registry from file, or create new if missing
    pub fn load(path: &Path) -> ProfileResult<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(path)?;
        let registry: Self = serde_json::from_str(&content)?;
        Ok(registry)
    }

    /// Save registry to file
    pub fn save(&self, path: &Path) -> ProfileResult<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Add a provider to the registry
    pub fn add_provider(
        &mut self,
        provider_id: String,
        manifest: &ProviderManifest,
        source: ProviderSource,
    ) {
        let profiles = manifest
            .profiles
            .iter()
            .map(|p| {
                (
                    p.name.clone(),
                    ProfileInfo {
                        version: p.version.clone().unwrap_or_else(|| "unknown".to_string()),
                        description: p.description.clone(),
                    },
                )
            })
            .collect();

        let registered = RegisteredProvider {
            name: manifest.name.clone(),
            source,
            namespace: manifest.metadata.as_ref().and_then(|m| m.namespace.clone()),
            profiles,
            last_updated: Some(current_timestamp()),
        };

        self.providers.insert(provider_id, registered);
    }

    /// Remove a provider from the registry
    pub fn remove_provider(&mut self, provider_id: &str) -> bool {
        self.providers.remove(provider_id).is_some()
    }

    /// Get a registered provider by ID
    pub fn get_provider(&self, provider_id: &str) -> Option<&RegisteredProvider> {
        self.providers.get(provider_id)
    }

    /// Find which provider contains a given profile
    pub fn find_provider_for_profile(&self, profile_name: &str) -> Option<&RegisteredProvider> {
        self.providers
            .values()
            .find(|p| p.profiles.contains_key(profile_name))
    }

    /// Find provider ID and provider for a given profile
    pub fn find_provider_with_id(&self, profile_name: &str) -> Option<(&str, &RegisteredProvider)> {
        self.providers
            .iter()
            .find(|(_, p)| p.profiles.contains_key(profile_name))
            .map(|(id, p)| (id.as_str(), p))
    }

    /// List all available profiles across all providers
    pub fn list_all_profiles(&self) -> Vec<(String, String, &ProfileInfo)> {
        let mut profiles = Vec::new();
        for (provider_id, provider) in &self.providers {
            for (profile_name, profile_info) in &provider.profiles {
                profiles.push((provider_id.clone(), profile_name.clone(), profile_info));
            }
        }
        profiles
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl RegisteredProvider {
    /// Get the git URL for this provider source
    pub fn git_url(&self) -> Option<String> {
        match &self.source {
            ProviderSource::Github { repo } => Some(format!("https://github.com/{repo}.git")),
            ProviderSource::Url { url } => Some(url.clone()),
            ProviderSource::Local { .. } => None,
        }
    }

    /// Check if this is a local provider
    pub fn is_local(&self) -> bool {
        matches!(self.source, ProviderSource::Local { .. })
    }

    /// Get the local path if this is a local provider
    pub fn local_path(&self) -> Option<&str> {
        match &self.source {
            ProviderSource::Local { path } => Some(path),
            _ => None,
        }
    }
}

impl ProviderSource {
    /// Create a GitHub source from owner/repo shorthand
    pub fn from_github_shorthand(repo: &str) -> Self {
        Self::Github {
            repo: repo.to_string(),
        }
    }

    /// Create a Url source from git URL
    pub fn from_git_url(url: &str) -> Self {
        Self::Url {
            url: url.to_string(),
        }
    }

    /// Create a Local source from path
    pub fn from_local_path(path: &str) -> Self {
        Self::Local {
            path: path.to_string(),
        }
    }

    /// Parse a source string into ProviderSource
    /// Detects GitHub shorthand (owner/repo), git URLs, or local paths
    pub fn parse(source: &str) -> Self {
        if source.starts_with("http://")
            || source.starts_with("https://")
            || source.starts_with("git@")
        {
            Self::from_git_url(source)
        } else if source.contains('/') && !source.starts_with('.') && !source.starts_with('/') {
            // Looks like GitHub shorthand (owner/repo)
            Self::from_github_shorthand(source)
        } else {
            // Treat as local path
            Self::from_local_path(source)
        }
    }
}

/// Get current timestamp in ISO 8601 format
fn current_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");

    let secs = duration.as_secs();
    let days = secs / 86400;
    let years = 1970 + days / 365;
    let day_of_year = days % 365;
    let month = (day_of_year / 30) + 1;
    let day = (day_of_year % 30) + 1;

    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    format!("{years:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_new_registry() {
        let registry = ProviderRegistry::new();
        assert_eq!(registry.version, 1);
        assert_eq!(registry.providers.len(), 0);
    }

    #[test]
    fn test_save_and_load() {
        let temp = tempdir().unwrap();
        let registry_path = temp.path().join("providers.json");

        let mut registry = ProviderRegistry::new();
        let manifest = ProviderManifest {
            name: "claude".to_string(),
            owner: super::super::provider::ProviderOwner {
                name: "Test".to_string(),
                email: None,
                url: None,
            },
            profiles: vec![],
            metadata: None,
            description: None,
            url: None,
        };

        registry.add_provider(
            "test-provider".to_string(),
            &manifest,
            ProviderSource::from_github_shorthand("codanna/claude-provider"),
        );

        registry.save(&registry_path).unwrap();
        assert!(registry_path.exists());

        let loaded = ProviderRegistry::load(&registry_path).unwrap();
        assert_eq!(loaded.providers.len(), 1);
        assert!(loaded.providers.contains_key("test-provider"));
    }

    #[test]
    fn test_add_and_remove_provider() {
        let mut registry = ProviderRegistry::new();
        let manifest = ProviderManifest {
            name: "claude".to_string(),
            owner: super::super::provider::ProviderOwner {
                name: "Test".to_string(),
                email: None,
                url: None,
            },
            profiles: vec![super::super::provider::ProviderProfile {
                name: "codanna".to_string(),
                source: super::super::provider::ProviderProfileSource::Path(
                    "./profiles/codanna".to_string(),
                ),
                description: Some("Test profile".to_string()),
                version: Some("1.0.0".to_string()),
                requires: vec![],
                keywords: vec![],
                category: None,
            }],
            metadata: None,
            description: None,
            url: None,
        };

        registry.add_provider(
            "test".to_string(),
            &manifest,
            ProviderSource::from_github_shorthand("codanna/test"),
        );
        assert_eq!(registry.providers.len(), 1);

        let removed = registry.remove_provider("test");
        assert!(removed);
        assert_eq!(registry.providers.len(), 0);
    }

    #[test]
    fn test_find_provider_for_profile() {
        let mut registry = ProviderRegistry::new();
        let manifest = ProviderManifest {
            name: "claude".to_string(),
            owner: super::super::provider::ProviderOwner {
                name: "Test".to_string(),
                email: None,
                url: None,
            },
            profiles: vec![super::super::provider::ProviderProfile {
                name: "codanna".to_string(),
                source: super::super::provider::ProviderProfileSource::Path(
                    "./profiles/codanna".to_string(),
                ),
                description: None,
                version: Some("1.0.0".to_string()),
                requires: vec![],
                keywords: vec![],
                category: None,
            }],
            metadata: None,
            description: None,
            url: None,
        };

        registry.add_provider(
            "test".to_string(),
            &manifest,
            ProviderSource::from_github_shorthand("codanna/test"),
        );

        let provider = registry.find_provider_for_profile("codanna");
        assert!(provider.is_some());
        assert_eq!(provider.unwrap().name, "claude");

        let missing = registry.find_provider_for_profile("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_parse_github_shorthand() {
        let source = ProviderSource::parse("codanna/claude-provider");
        match source {
            ProviderSource::Github { repo } => {
                assert_eq!(repo, "codanna/claude-provider");
            }
            _ => panic!("Expected Github source"),
        }
    }

    #[test]
    fn test_parse_git_url() {
        let source = ProviderSource::parse("https://github.com/codanna/claude-provider.git");
        match source {
            ProviderSource::Url { url } => {
                assert_eq!(url, "https://github.com/codanna/claude-provider.git");
            }
            _ => panic!("Expected Url source"),
        }
    }

    #[test]
    fn test_parse_local_path() {
        let source = ProviderSource::parse("./my-provider");
        match source {
            ProviderSource::Local { path } => {
                assert_eq!(path, "./my-provider");
            }
            _ => panic!("Expected Local source"),
        }
    }

    #[test]
    fn test_git_url_conversion() {
        let source = ProviderSource::from_github_shorthand("codanna/test");
        let provider = RegisteredProvider {
            name: "test".to_string(),
            source,
            namespace: None,
            profiles: HashMap::new(),
            last_updated: None,
        };

        let url = provider.git_url();
        assert_eq!(url, Some("https://github.com/codanna/test.git".to_string()));
    }
}
