//! Provider manifest handling for profile discovery
//!
//! Providers are containers for profiles, similar to how marketplaces contain plugins.
//! A provider manifest lives at `.codanna-profile/provider.json` in provider repositories.

use super::error::{ProfileError, ProfileResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Provider manifest structure
/// Located at .codanna-profile/provider.json in provider repos
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderManifest {
    /// Provider name (e.g., "claude", "codex")
    pub name: String,

    /// Owner information
    pub owner: ProviderOwner,

    /// List of available profiles
    pub profiles: Vec<ProviderProfile>,

    /// Optional metadata block
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ProviderMetadata>,

    /// Optional provider description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional provider URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Provider owner information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderOwner {
    /// Owner name (user or organization)
    pub name: String,

    /// Optional email
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Optional website
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Additional provider metadata
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProviderMetadata {
    /// Provider namespace directory (e.g., ".claude")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    /// Settings file name within namespace
    #[serde(rename = "settingsFile", skip_serializing_if = "Option::is_none")]
    pub settings_file: Option<String>,

    /// Optional profile root used to resolve relative profile sources
    #[serde(rename = "profileRoot", skip_serializing_if = "Option::is_none")]
    pub profile_root: Option<String>,

    /// Optional provider version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Profile entry in provider manifest
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderProfile {
    /// Profile name (must be unique in provider)
    pub name: String,

    /// Where to resolve profile content from (relative path or external source)
    pub source: ProviderProfileSource,

    /// Profile description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Profile version
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Profiles this profile requires (e.g., "claude" requires "codanna")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,

    /// Keywords for categorization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,

    /// Category label
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
}

/// Source descriptor for profiles
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ProviderProfileSource {
    /// Relative path within the provider repository
    Path(String),
    /// Detailed external source descriptor
    Descriptor(ProviderProfileSourceDescriptor),
}

/// Detailed descriptor for git-based profile sources
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProviderProfileSourceDescriptor {
    /// Source type (e.g., "git", "github")
    pub source: String,
    /// GitHub repository (owner/name) when source == "github"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    /// Explicit git URL when source == "git"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Optional path/subdirectory within repository
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Optional subdirectory alias
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subdir: Option<String>,
    /// Optional git ref (branch/tag/sha)
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
}

/// Resolved profile source ready for fetch/extraction
#[derive(Debug, Clone)]
pub enum ResolvedProfileSource {
    /// Subdirectory within the provider repository clone
    ProviderPath { relative: String },
    /// External git repository
    Git {
        url: String,
        git_ref: Option<String>,
        subdir: Option<String>,
    },
}

impl ProviderManifest {
    /// Load provider manifest from JSON file
    pub fn from_file(path: &Path) -> ProfileResult<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
    }

    /// Parse provider manifest from JSON string
    pub fn from_json(json: &str) -> ProfileResult<Self> {
        let manifest: Self = serde_json::from_str(json)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate manifest structure
    pub fn validate(&self) -> ProfileResult<()> {
        if self.name.is_empty() {
            return Err(ProfileError::InvalidManifest {
                reason: "Provider name cannot be empty".to_string(),
            });
        }

        if self.owner.name.is_empty() {
            return Err(ProfileError::InvalidManifest {
                reason: "Provider owner name cannot be empty".to_string(),
            });
        }

        if self.profiles.is_empty() {
            return Err(ProfileError::InvalidManifest {
                reason: "Provider must contain at least one profile".to_string(),
            });
        }

        // Validate each profile
        for profile in &self.profiles {
            if profile.name.is_empty() {
                return Err(ProfileError::InvalidManifest {
                    reason: "Profile name cannot be empty".to_string(),
                });
            }
        }

        Ok(())
    }

    /// Get a profile by name
    pub fn get_profile(&self, name: &str) -> Option<&ProviderProfile> {
        self.profiles.iter().find(|p| p.name == name)
    }
}

impl ProviderProfileSource {
    /// Resolve source to fetch location
    pub fn resolve(&self, provider_root: Option<&str>) -> ResolvedProfileSource {
        match self {
            ProviderProfileSource::Path(path) => {
                let relative = provider_root
                    .map(|root| format!("{root}/{path}"))
                    .unwrap_or_else(|| path.clone());
                ResolvedProfileSource::ProviderPath { relative }
            }
            ProviderProfileSource::Descriptor(desc) => {
                let url = if desc.source == "github" {
                    desc.repo
                        .as_ref()
                        .map(|repo| format!("https://github.com/{repo}.git"))
                        .unwrap_or_default()
                } else {
                    desc.url.clone().unwrap_or_default()
                };

                ResolvedProfileSource::Git {
                    url,
                    git_ref: desc.git_ref.clone(),
                    subdir: desc.subdir.clone().or_else(|| desc.path.clone()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_provider_manifest() {
        let json = r#"{
            "name": "claude",
            "owner": {
                "name": "Codanna Team",
                "email": "team@codanna.dev"
            },
            "metadata": {
                "namespace": ".claude",
                "settingsFile": "settings.local.json",
                "version": "1.0.0"
            },
            "profiles": [
                {
                    "name": "codanna",
                    "source": "./profiles/codanna",
                    "description": "Base codanna configuration",
                    "version": "1.0.0"
                },
                {
                    "name": "claude",
                    "source": "./profiles/claude",
                    "description": "Claude Code provider setup",
                    "version": "1.0.0",
                    "requires": ["codanna"]
                }
            ]
        }"#;

        let manifest = ProviderManifest::from_json(json).unwrap();
        assert_eq!(manifest.name, "claude");
        assert_eq!(manifest.profiles.len(), 2);
        assert_eq!(
            manifest.metadata.as_ref().unwrap().namespace,
            Some(".claude".to_string())
        );
    }

    #[test]
    fn test_validate_requires_name() {
        let json = r#"{
            "name": "",
            "owner": {"name": "Test"},
            "profiles": []
        }"#;
        assert!(ProviderManifest::from_json(json).is_err());
    }

    #[test]
    fn test_validate_requires_profiles() {
        let json = r#"{
            "name": "test",
            "owner": {"name": "Test"},
            "profiles": []
        }"#;
        assert!(ProviderManifest::from_json(json).is_err());
    }

    #[test]
    fn test_get_profile() {
        let json = r#"{
            "name": "claude",
            "owner": {"name": "Test"},
            "profiles": [
                {"name": "codanna", "source": "./profiles/codanna"},
                {"name": "claude", "source": "./profiles/claude"}
            ]
        }"#;

        let manifest = ProviderManifest::from_json(json).unwrap();
        assert!(manifest.get_profile("codanna").is_some());
        assert!(manifest.get_profile("claude").is_some());
        assert!(manifest.get_profile("nonexistent").is_none());
    }

    #[test]
    fn test_resolve_path_source() {
        let source = ProviderProfileSource::Path("./profiles/codanna".to_string());
        match source.resolve(Some("profiles")) {
            ResolvedProfileSource::ProviderPath { relative } => {
                assert_eq!(relative, "profiles/./profiles/codanna");
            }
            _ => panic!("Expected ProviderPath"),
        }
    }

    #[test]
    fn test_resolve_github_source() {
        let source = ProviderProfileSource::Descriptor(ProviderProfileSourceDescriptor {
            source: "github".to_string(),
            repo: Some("codanna/profiles".to_string()),
            url: None,
            path: None,
            subdir: Some("profiles/codanna".to_string()),
            git_ref: Some("main".to_string()),
        });

        match source.resolve(None) {
            ResolvedProfileSource::Git {
                url,
                git_ref,
                subdir,
            } => {
                assert_eq!(url, "https://github.com/codanna/profiles.git");
                assert_eq!(git_ref, Some("main".to_string()));
                assert_eq!(subdir, Some("profiles/codanna".to_string()));
            }
            _ => panic!("Expected Git source"),
        }
    }
}
