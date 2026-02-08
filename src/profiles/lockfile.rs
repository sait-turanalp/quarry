//! Lockfile management for tracking installed profiles

use super::error::{ProfileError, ProfileResult};
use super::provider_registry::ProviderSource;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Lockfile structure for tracking installed profiles
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ProfileLockfile {
    /// Version of lockfile format
    pub version: String,

    /// Installed profiles
    pub profiles: HashMap<String, ProfileLockEntry>,
}

/// Individual profile entry in lockfile
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProfileLockEntry {
    /// Profile name
    pub name: String,

    /// Installed version
    pub version: String,

    /// Installation timestamp
    pub installed_at: String,

    /// List of installed files
    pub files: Vec<String>,

    /// Integrity checksum (SHA-256 hash of all installed files)
    /// Uses default (empty string) for backwards compatibility with old lockfiles
    #[serde(default)]
    pub integrity: String,

    /// Git commit SHA (if installed from git source)
    /// Uses default (None) for backwards compatibility and local sources
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,

    /// Provider ID this profile came from
    /// Uses default (None) for backwards compatibility with old lockfiles
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,

    /// Source where this profile was installed from
    /// Uses default (None) for backwards compatibility with old lockfiles
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ProviderSource>,
}

impl ProfileLockfile {
    /// Create a new empty lockfile
    pub fn new() -> Self {
        Self {
            version: "1.0.0".to_string(),
            profiles: HashMap::new(),
        }
    }

    /// Load lockfile from disk
    pub fn load(path: &Path) -> ProfileResult<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(path)?;
        let lockfile: Self =
            serde_json::from_str(&content).map_err(|_| ProfileError::InvalidManifest {
                reason: "Lockfile is corrupted".to_string(),
            })?;

        Ok(lockfile)
    }

    /// Save lockfile to disk
    pub fn save(&self, path: &Path) -> ProfileResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Check if a profile is installed
    pub fn is_installed(&self, name: &str) -> bool {
        self.profiles.contains_key(name)
    }

    /// Get entry for a profile
    pub fn get_profile(&self, name: &str) -> Option<&ProfileLockEntry> {
        self.profiles.get(name)
    }

    /// Add or update a profile entry
    pub fn add_profile(&mut self, entry: ProfileLockEntry) {
        self.profiles.insert(entry.name.clone(), entry);
    }

    /// Remove a profile entry
    pub fn remove_profile(&mut self, name: &str) -> Option<ProfileLockEntry> {
        self.profiles.remove(name)
    }

    /// Find which profile owns a file
    pub fn find_file_owner(&self, file_path: &str) -> Option<&str> {
        for (name, entry) in &self.profiles {
            if entry.files.contains(&file_path.to_string()) {
                return Some(name);
            }
        }
        None
    }
}
