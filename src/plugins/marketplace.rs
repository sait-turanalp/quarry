//! Marketplace manifest handling for plugin discovery

use super::{
    error::{PluginError, PluginResult},
    plugin::{HookSpec, McpServerSpec, PathSpec, PluginAuthor, PluginManifest},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Marketplace manifest structure
/// Located at .claude-plugin/marketplace.json in marketplace repos
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MarketplaceManifest {
    /// Marketplace name
    pub name: String,

    /// Owner information
    pub owner: MarketplaceOwner,

    /// List of available plugins
    pub plugins: Vec<MarketplacePlugin>,

    /// Optional metadata block
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<MarketplaceMetadata>,

    /// Optional marketplace description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional marketplace URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Marketplace owner information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MarketplaceOwner {
    /// Owner name (user or organization)
    pub name: String,

    /// Optional email
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Optional website
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Additional marketplace metadata
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MarketplaceMetadata {
    /// Optional plugin root used to resolve relative plugin sources
    #[serde(rename = "pluginRoot", skip_serializing_if = "Option::is_none")]
    pub plugin_root: Option<String>,

    /// Optional description override
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Optional marketplace version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Plugin entry in marketplace
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MarketplacePlugin {
    /// Plugin name (must be unique in marketplace)
    pub name: String,

    /// Where to resolve plugin content from
    pub source: MarketplacePluginSource,

    /// Plugin description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Plugin version
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Plugin author
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<PluginAuthor>,

    /// Optional homepage
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,

    /// Repository URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    /// License identifier
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// Keywords for categorization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,

    /// Category label
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,

    /// Tags for search/discovery
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Optional command paths
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<PathSpec>,

    /// Optional agent paths
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agents: Option<PathSpec>,

    /// Optional hook configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<HookSpec>,

    /// Optional script paths
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scripts: Option<PathSpec>,

    /// Optional MCP server configuration
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "mcpServers")]
    pub mcp_servers: Option<McpServerSpec>,

    /// Whether plugin.json is required (defaults to true)
    #[serde(default = "MarketplacePlugin::default_strict")]
    pub strict: bool,
}

/// Source descriptor for plugins that live outside the marketplace repo
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum MarketplacePluginSource {
    /// Relative path within the marketplace repository
    Path(String),
    /// Detailed external source descriptor
    Descriptor(MarketplacePluginSourceDescriptor),
}

/// Detailed descriptor for git-based sources
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MarketplacePluginSourceDescriptor {
    /// Source type (e.g., git, github)
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

/// Resolved plugin source ready for fetch/extraction
#[derive(Debug, Clone)]
pub enum ResolvedPluginSource {
    /// Subdirectory within the marketplace repository clone
    MarketplacePath { relative: String },
    /// External git repository
    Git {
        url: String,
        git_ref: Option<String>,
        subdir: Option<String>,
    },
}

impl MarketplaceManifest {
    /// Load marketplace manifest from JSON file
    pub fn from_file(path: &Path) -> PluginResult<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
    }

    /// Parse marketplace manifest from JSON string
    pub fn from_json(json: &str) -> PluginResult<Self> {
        let manifest: Self = serde_json::from_str(json)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate manifest structure
    pub fn validate(&self) -> PluginResult<()> {
        if self.name.is_empty() {
            return Err(PluginError::InvalidMarketplaceManifest {
                reason: "Marketplace name cannot be empty".to_string(),
            });
        }

        if self.owner.name.is_empty() {
            return Err(PluginError::InvalidMarketplaceManifest {
                reason: "Owner name cannot be empty".to_string(),
            });
        }

        if self.plugins.is_empty() {
            return Err(PluginError::InvalidMarketplaceManifest {
                reason: "Marketplace must contain at least one plugin".to_string(),
            });
        }

        // Check for duplicate plugin names
        let mut seen = std::collections::HashSet::new();
        for plugin in &self.plugins {
            if !seen.insert(&plugin.name) {
                return Err(PluginError::InvalidMarketplaceManifest {
                    reason: format!("Duplicate plugin name: {}", plugin.name),
                });
            }

            // Validate plugin entry
            plugin.validate()?;
        }

        Ok(())
    }

    /// Find a plugin by name
    pub fn find_plugin(&self, name: &str) -> Option<&MarketplacePlugin> {
        self.plugins.iter().find(|p| p.name == name)
    }
}

impl MarketplacePlugin {
    fn default_strict() -> bool {
        true
    }

    /// Validate plugin entry
    pub fn validate(&self) -> PluginResult<()> {
        if self.name.is_empty() {
            return Err(PluginError::InvalidMarketplaceManifest {
                reason: "Plugin name cannot be empty".to_string(),
            });
        }

        self.source.validate(&self.name)?;

        Ok(())
    }

    /// Resolve plugin source relative to marketplace metadata
    pub fn resolve_source(
        &self,
        metadata: Option<&MarketplaceMetadata>,
    ) -> PluginResult<ResolvedPluginSource> {
        match &self.source {
            MarketplacePluginSource::Path(path) => {
                let combined =
                    combine_paths(metadata.and_then(|m| m.plugin_root.as_deref()), path)?;
                Ok(ResolvedPluginSource::MarketplacePath { relative: combined })
            }
            MarketplacePluginSource::Descriptor(desc) => desc.to_resolved_source(),
        }
    }

    /// Build a PluginManifest from marketplace data when strict = false
    pub fn to_plugin_manifest(&self) -> PluginResult<PluginManifest> {
        let description = self
            .description
            .clone()
            .unwrap_or_else(|| format!("Plugin '{}' provided by marketplace", self.name));
        if description.is_empty() {
            return Err(PluginError::InvalidPluginManifest {
                reason: format!(
                    "Plugin '{}' requires a description when strict mode is disabled",
                    self.name
                ),
            });
        }

        let version = self.version.clone().unwrap_or_else(|| "0.0.0".to_string());

        let author = self.author.clone().unwrap_or(PluginAuthor {
            name: "Marketplace".to_string(),
            email: None,
            url: None,
        });

        let manifest = PluginManifest {
            name: self.name.clone(),
            version,
            description,
            author,
            repository: self.repository.clone(),
            license: self.license.clone(),
            keywords: self.keywords.clone(),
            commands: self.commands.clone(),
            agents: self.agents.clone(),
            hooks: self.hooks.clone(),
            scripts: self.scripts.clone(),
            mcp_servers: self.mcp_servers.clone(),
        };

        manifest.validate()?;
        Ok(manifest)
    }
}

impl MarketplacePluginSource {
    fn validate(&self, plugin_name: &str) -> PluginResult<()> {
        match self {
            MarketplacePluginSource::Path(path) => validate_relative_path(plugin_name, path),
            MarketplacePluginSource::Descriptor(desc) => desc.validate(plugin_name),
        }
    }
}

impl MarketplacePluginSourceDescriptor {
    fn validate(&self, plugin_name: &str) -> PluginResult<()> {
        match self.source.as_str() {
            "git" => {
                if self
                    .url
                    .as_ref()
                    .map(|s| s.trim())
                    .unwrap_or_default()
                    .is_empty()
                {
                    return Err(PluginError::InvalidMarketplaceManifest {
                        reason: format!(
                            "Plugin '{plugin_name}' git source requires a non-empty 'url' field"
                        ),
                    });
                }
            }
            "github" => {
                if self
                    .repo
                    .as_ref()
                    .map(|s| s.trim())
                    .unwrap_or_default()
                    .is_empty()
                {
                    return Err(PluginError::InvalidMarketplaceManifest {
                        reason: format!(
                            "Plugin '{plugin_name}' github source requires a 'repo' field"
                        ),
                    });
                }
            }
            other => {
                return Err(PluginError::InvalidMarketplaceManifest {
                    reason: format!("Plugin '{plugin_name}' has unsupported source type '{other}'"),
                });
            }
        }

        if let Some(path) = self.path.as_ref().or(self.subdir.as_ref()) {
            validate_relative_path(plugin_name, path)?;
        }

        Ok(())
    }

    fn to_resolved_source(&self) -> PluginResult<ResolvedPluginSource> {
        let subdir = self
            .subdir
            .clone()
            .or_else(|| self.path.clone())
            .map(|p| sanitize(&p));

        match self.source.as_str() {
            "git" => {
                let url = self
                    .url
                    .as_ref()
                    .ok_or_else(|| PluginError::InvalidMarketplaceManifest {
                        reason: "Git source requires 'url'".to_string(),
                    })?
                    .clone();
                Ok(ResolvedPluginSource::Git {
                    url,
                    git_ref: self.git_ref.clone(),
                    subdir,
                })
            }
            "github" => {
                let repo = self
                    .repo
                    .as_ref()
                    .ok_or_else(|| PluginError::InvalidMarketplaceManifest {
                        reason: "GitHub source requires 'repo'".to_string(),
                    })?
                    .clone();
                let url = format!("https://github.com/{repo}.git");
                Ok(ResolvedPluginSource::Git {
                    url,
                    git_ref: self.git_ref.clone(),
                    subdir,
                })
            }
            other => Err(PluginError::InvalidMarketplaceManifest {
                reason: format!("Unsupported plugin source type '{other}'"),
            }),
        }
    }
}

fn combine_paths(base: Option<&str>, child: &str) -> PluginResult<String> {
    let mut path = PathBuf::new();
    if let Some(base) = base {
        let sanitized = sanitize(base);
        if !sanitized.is_empty() && sanitized != "." {
            path.push(sanitized);
        }
    }
    let child_sanitized = sanitize(child);
    if child_sanitized.is_empty() || child_sanitized == "." {
        return Ok(path.to_string_lossy().replace('\\', "/"));
    }
    path.push(child_sanitized);
    Ok(path.to_string_lossy().replace('\\', "/"))
}

fn sanitize(path: &str) -> String {
    let trimmed = path.trim();
    let without_prefix = trimmed.trim_start_matches("./");
    let without_slash = without_prefix.trim_start_matches('/');
    if without_slash.is_empty() {
        ".".to_string()
    } else {
        without_slash.to_string()
    }
}

fn validate_relative_path(plugin_name: &str, path: &str) -> PluginResult<()> {
    if path.contains("..") {
        return Err(PluginError::InvalidMarketplaceManifest {
            reason: format!("Plugin '{plugin_name}' source path cannot contain '..'"),
        });
    }

    if Path::new(path).is_absolute() {
        return Err(PluginError::InvalidMarketplaceManifest {
            reason: format!("Plugin '{plugin_name}' source path must be relative"),
        });
    }

    if path.trim().is_empty() {
        return Err(PluginError::InvalidMarketplaceManifest {
            reason: format!("Plugin '{plugin_name}' source path cannot be empty"),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid_marketplace() {
        let json = r#"{
            "name": "test-marketplace",
            "owner": {
                "name": "Test User"
            },
            "plugins": [
                {
                    "name": "test-plugin",
                    "source": "./plugins/test",
                    "description": "A test plugin"
                }
            ]
        }"#;

        let manifest = MarketplaceManifest::from_json(json).unwrap();
        assert_eq!(manifest.name, "test-marketplace");
        assert_eq!(manifest.plugins.len(), 1);
    }

    #[test]
    fn test_reject_empty_marketplace() {
        let json = r#"{
            "name": "test-marketplace",
            "owner": {
                "name": "Test User"
            },
            "plugins": []
        }"#;

        let result = MarketplaceManifest::from_json(json);
        assert!(matches!(
            result,
            Err(PluginError::InvalidMarketplaceManifest { .. })
        ));
    }

    #[test]
    fn test_reject_duplicate_plugins() {
        let json = r#"{
            "name": "test-marketplace",
            "owner": {
                "name": "Test User"
            },
            "plugins": [
                {
                    "name": "duplicate",
                    "source": "./plugin1",
                    "description": "Plugin 1"
                },
                {
                    "name": "duplicate",
                    "source": "./plugin2",
                    "description": "Plugin 2"
                }
            ]
        }"#;

        let result = MarketplaceManifest::from_json(json);
        assert!(matches!(
            result,
            Err(PluginError::InvalidMarketplaceManifest { .. })
        ));
    }

    #[test]
    fn test_find_plugin() {
        let json = r#"{
            "name": "test-marketplace",
            "owner": {
                "name": "Test User"
            },
            "plugins": [
                {
                    "name": "plugin-a",
                    "source": "./a",
                    "description": "Plugin A"
                },
                {
                    "name": "plugin-b",
                    "source": "./b",
                    "description": "Plugin B"
                }
            ]
        }"#;

        let manifest = MarketplaceManifest::from_json(json).unwrap();
        assert!(manifest.find_plugin("plugin-a").is_some());
        assert!(manifest.find_plugin("plugin-b").is_some());
        assert!(manifest.find_plugin("plugin-c").is_none());
    }
}
