//! Lockfile management for tracking installed plugins

use super::error::{PluginError, PluginResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Lockfile structure for tracking installed plugins
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct PluginLockfile {
    /// Version of lockfile format
    pub version: String,

    /// Installed plugins
    pub plugins: HashMap<String, PluginLockEntry>,
}

/// Individual plugin entry in lockfile
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginLockEntry {
    /// Plugin name
    pub name: String,

    /// Installed version
    pub version: String,

    /// Git commit SHA
    pub commit: String,

    /// Marketplace URL
    pub marketplace_url: String,

    /// Installation timestamp
    pub installed_at: String,

    /// Last update timestamp
    pub updated_at: String,

    /// Integrity checksum
    pub integrity: String,

    /// List of installed files
    pub files: Vec<String>,

    /// MCP server keys added
    #[serde(default)]
    pub mcp_keys: Vec<String>,

    /// Resolved plugin source information
    #[serde(default)]
    pub source: Option<LockfilePluginSource>,
}

/// Stored plugin source information
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LockfilePluginSource {
    /// Plugin files came from a path within the marketplace repository
    MarketplacePath { relative: String },
    /// Plugin files were fetched from an external git repository
    Git {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        git_ref: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        subdir: Option<String>,
    },
}

impl PluginLockfile {
    /// Create a new empty lockfile
    pub fn new() -> Self {
        Self {
            version: "1.0.0".to_string(),
            plugins: HashMap::new(),
        }
    }

    /// Load lockfile from disk
    pub fn load(path: &Path) -> PluginResult<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(path)?;
        let lockfile: Self =
            serde_json::from_str(&content).map_err(|_| PluginError::LockfileCorrupted)?;

        Ok(lockfile)
    }

    /// Save lockfile to disk
    pub fn save(&self, path: &Path) -> PluginResult<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Check if a plugin is installed
    pub fn is_installed(&self, name: &str) -> bool {
        self.plugins.contains_key(name)
    }

    /// Get entry for a plugin
    pub fn get_plugin(&self, name: &str) -> Option<&PluginLockEntry> {
        self.plugins.get(name)
    }

    /// Add or update a plugin entry
    pub fn add_plugin(&mut self, entry: PluginLockEntry) {
        self.plugins.insert(entry.name.clone(), entry);
    }

    /// Remove a plugin entry
    pub fn remove_plugin(&mut self, name: &str) -> Option<PluginLockEntry> {
        self.plugins.remove(name)
    }

    /// Find which plugin owns a file
    pub fn find_file_owner(&self, file_path: &str) -> Option<&str> {
        for (name, entry) in &self.plugins {
            if entry.files.contains(&file_path.to_string()) {
                return Some(name);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_lockfile() {
        let lockfile = PluginLockfile::new();
        assert_eq!(lockfile.version, "1.0.0");
        assert!(lockfile.plugins.is_empty());
    }

    #[test]
    fn test_add_plugin() {
        let mut lockfile = PluginLockfile::new();
        let entry = PluginLockEntry {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            commit: "abc123".to_string(),
            marketplace_url: "https://example.com".to_string(),
            installed_at: "2025-01-11".to_string(),
            updated_at: "2025-01-11".to_string(),
            integrity: "sha256:abc".to_string(),
            files: vec![".claude/commands/test.md".to_string()],
            mcp_keys: vec![],
            source: None,
        };

        lockfile.add_plugin(entry);
        assert!(lockfile.is_installed("test-plugin"));
    }

    #[test]
    fn test_find_file_owner() {
        let mut lockfile = PluginLockfile::new();
        let entry = PluginLockEntry {
            name: "test-plugin".to_string(),
            version: "1.0.0".to_string(),
            commit: "abc123".to_string(),
            marketplace_url: "https://example.com".to_string(),
            installed_at: "2025-01-11".to_string(),
            updated_at: "2025-01-11".to_string(),
            integrity: "sha256:abc".to_string(),
            files: vec![".claude/commands/test.md".to_string()],
            mcp_keys: vec![],
            source: None,
        };

        lockfile.add_plugin(entry);
        assert_eq!(
            lockfile.find_file_owner(".claude/commands/test.md"),
            Some("test-plugin")
        );
        assert_eq!(lockfile.find_file_owner(".claude/commands/other.md"), None);
    }
}
