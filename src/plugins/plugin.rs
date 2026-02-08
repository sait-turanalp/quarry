//! Plugin manifest handling and validation

use super::error::{PluginError, PluginResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// Plugin manifest structure (plugin.json)
/// Located at .claude-plugin/plugin.json in plugin directories
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginManifest {
    /// Plugin name
    pub name: String,

    /// Plugin version (semantic versioning recommended)
    pub version: String,

    /// Plugin description
    pub description: String,

    /// Author information
    pub author: PluginAuthor,

    /// Repository URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    /// License identifier (e.g., "MIT", "Apache-2.0")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// Keywords for categorization
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,

    /// Additional command paths (beyond default commands/ directory)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<PathSpec>,

    /// Agent paths (beyond default agents/ directory)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<PathSpec>,

    /// Hook configuration (inline or path to hooks.json)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<HookSpec>,

    /// Script paths (beyond default scripts/ directory)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scripts: Option<PathSpec>,

    /// MCP server configuration (inline or path to .mcp.json)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "mcpServers")]
    pub mcp_servers: Option<McpServerSpec>,
}

/// Author information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginAuthor {
    /// Author name
    pub name: String,

    /// Optional email
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Optional website
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Path specification - can be string or array of strings
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum PathSpec {
    /// Single path
    Single(String),
    /// Multiple paths
    Multiple(Vec<String>),
}

/// Hook specification - can be string (path) or inline object
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum HookSpec {
    /// Path to hooks.json file
    Path(String),
    /// Inline hook configuration
    Inline(Value),
}

/// MCP server specification - can be string (path) or inline object
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum McpServerSpec {
    /// Path to .mcp.json file
    Path(String),
    /// Inline MCP server configuration
    Inline(Value),
}

impl PluginManifest {
    /// Load plugin manifest from JSON file
    pub fn from_file(path: &Path) -> PluginResult<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_json(&content)
    }

    /// Parse plugin manifest from JSON string
    pub fn from_json(json: &str) -> PluginResult<Self> {
        let manifest: Self = serde_json::from_str(json)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate manifest structure
    pub fn validate(&self) -> PluginResult<()> {
        if self.name.is_empty() {
            return Err(PluginError::InvalidPluginManifest {
                reason: "Plugin name cannot be empty".to_string(),
            });
        }

        if self.version.is_empty() {
            return Err(PluginError::InvalidPluginManifest {
                reason: "Plugin version cannot be empty".to_string(),
            });
        }

        if self.description.is_empty() {
            return Err(PluginError::InvalidPluginManifest {
                reason: "Plugin description cannot be empty".to_string(),
            });
        }

        if self.author.name.is_empty() {
            return Err(PluginError::InvalidPluginManifest {
                reason: "Author name cannot be empty".to_string(),
            });
        }

        // Validate path specifications
        if let Some(ref commands) = self.commands {
            Self::validate_path_spec(commands, "commands")?;
        }

        if let Some(ref agents) = self.agents {
            Self::validate_path_spec(agents, "agents")?;
        }

        if let Some(ref scripts) = self.scripts {
            Self::validate_path_spec(scripts, "scripts")?;
        }

        if let Some(HookSpec::Path(ref path)) = self.hooks {
            Self::validate_relative_path(path, "hooks")?;
        }

        if let Some(McpServerSpec::Path(ref path)) = self.mcp_servers {
            Self::validate_relative_path(path, "mcpServers")?;
        }

        Ok(())
    }

    /// Validate a path specification
    fn validate_path_spec(spec: &PathSpec, field: &str) -> PluginResult<()> {
        match spec {
            PathSpec::Single(path) => Self::validate_relative_path(path, field)?,
            PathSpec::Multiple(paths) => {
                if paths.is_empty() {
                    return Err(PluginError::InvalidPluginManifest {
                        reason: format!("{field} paths cannot be empty"),
                    });
                }
                for path in paths {
                    Self::validate_relative_path(path, field)?;
                }
            }
        }
        Ok(())
    }

    fn validate_relative_path(path: &str, field: &str) -> PluginResult<()> {
        if path.is_empty() {
            return Err(PluginError::InvalidPluginManifest {
                reason: format!("{field} path cannot be empty"),
            });
        }

        if std::path::Path::new(path).is_absolute() {
            return Err(PluginError::InvalidPluginManifest {
                reason: format!("{field} path must be relative to plugin root"),
            });
        }

        if !path.starts_with("./") {
            return Err(PluginError::InvalidPluginManifest {
                reason: format!("{field} paths must start with './'"),
            });
        }

        if path.contains("..") {
            return Err(PluginError::InvalidPluginManifest {
                reason: format!("{field} path cannot contain '..'"),
            });
        }

        Ok(())
    }

    /// Get all command paths (default + specified)
    pub fn get_command_paths(&self) -> Vec<String> {
        let mut paths = vec!["commands".to_string()];
        if let Some(ref spec) = self.commands {
            match spec {
                PathSpec::Single(path) => paths.push(path.clone()),
                PathSpec::Multiple(multi) => paths.extend(multi.clone()),
            }
        }
        paths
    }

    /// Get all agent paths (default + specified)
    pub fn get_agent_paths(&self) -> Vec<String> {
        let mut paths = vec!["agents".to_string()];
        if let Some(ref spec) = self.agents {
            match spec {
                PathSpec::Single(path) => paths.push(path.clone()),
                PathSpec::Multiple(multi) => paths.extend(multi.clone()),
            }
        }
        paths
    }
}

impl PathSpec {
    /// Convert to vector of paths
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            PathSpec::Single(path) => vec![path.clone()],
            PathSpec::Multiple(paths) => paths.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_plugin() {
        let json = r#"{
            "name": "test-plugin",
            "version": "1.0.0",
            "description": "A test plugin",
            "author": {
                "name": "Test Author"
            }
        }"#;

        let manifest = PluginManifest::from_json(json).unwrap();
        assert_eq!(manifest.name, "test-plugin");
        assert_eq!(manifest.version, "1.0.0");
    }

    #[test]
    fn test_parse_plugin_with_paths() {
        let json = r#"{
            "name": "test-plugin",
            "version": "1.0.0",
            "description": "A test plugin",
            "author": {
                "name": "Test Author"
            },
            "commands": "./custom-commands",
            "agents": ["./agent1", "./agent2"]
        }"#;

        let manifest = PluginManifest::from_json(json).unwrap();
        assert!(matches!(manifest.commands, Some(PathSpec::Single(_))));
        assert!(matches!(manifest.agents, Some(PathSpec::Multiple(_))));
    }

    #[test]
    fn test_get_command_paths() {
        let json = r#"{
            "name": "test-plugin",
            "version": "1.0.0",
            "description": "A test plugin",
            "author": {
                "name": "Test Author"
            },
            "commands": ["./extra-commands", "./more-commands"]
        }"#;

        let manifest = PluginManifest::from_json(json).unwrap();
        let paths = manifest.get_command_paths();
        assert_eq!(paths.len(), 3);
        assert!(paths.contains(&"commands".to_string()));
        assert!(paths.contains(&"./extra-commands".to_string()));
        assert!(paths.contains(&"./more-commands".to_string()));
    }

    #[test]
    fn test_reject_empty_name() {
        let json = r#"{
            "name": "",
            "version": "1.0.0",
            "description": "A test plugin",
            "author": {
                "name": "Test Author"
            }
        }"#;

        let result = PluginManifest::from_json(json);
        assert!(matches!(
            result,
            Err(PluginError::InvalidPluginManifest { .. })
        ));
    }

    #[test]
    fn test_reject_path_traversal() {
        let json = r#"{
            "name": "test-plugin",
            "version": "1.0.0",
            "description": "A test plugin",
            "author": {
                "name": "Test Author"
            },
            "commands": "./../escape"
        }"#;

        let result = PluginManifest::from_json(json);
        assert!(matches!(
            result,
            Err(PluginError::InvalidPluginManifest { .. })
        ));
    }

    #[test]
    fn test_reject_paths_without_prefix() {
        let json = r#"{
            "name": "test-plugin",
            "version": "1.0.0",
            "description": "A test plugin",
            "author": {
                "name": "Test Author"
            },
            "commands": "custom-commands"
        }"#;

        let result = PluginManifest::from_json(json);
        assert!(matches!(
            result,
            Err(PluginError::InvalidPluginManifest { .. })
        ));
    }
}
