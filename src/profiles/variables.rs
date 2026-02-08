//! Variable merging system for profile templates

use std::collections::HashMap;

/// Variable storage with tiered priority
#[derive(Debug, Clone)]
pub struct Variables {
    global: HashMap<String, String>,
    manifest: HashMap<String, String>,
    local: HashMap<String, String>,
    cli: HashMap<String, String>,
}

impl Variables {
    /// Create a new empty variable set
    pub fn new() -> Self {
        Self {
            global: HashMap::new(),
            manifest: HashMap::new(),
            local: HashMap::new(),
            cli: HashMap::new(),
        }
    }

    /// Set a global variable
    pub fn set_global(&mut self, key: &str, value: &str) {
        self.global.insert(key.to_string(), value.to_string());
    }

    /// Set a manifest variable
    pub fn set_manifest(&mut self, key: &str, value: &str) {
        self.manifest.insert(key.to_string(), value.to_string());
    }

    /// Set a local variable
    pub fn set_local(&mut self, key: &str, value: &str) {
        self.local.insert(key.to_string(), value.to_string());
    }

    /// Set a CLI variable
    pub fn set_cli(&mut self, key: &str, value: &str) {
        self.cli.insert(key.to_string(), value.to_string());
    }

    /// Merge variables with priority: CLI > Local > Manifest > Global
    pub fn merge(&self) -> HashMap<String, String> {
        let mut result = HashMap::new();

        // Apply in order of increasing priority
        result.extend(self.global.clone());
        result.extend(self.manifest.clone());
        result.extend(self.local.clone());
        result.extend(self.cli.clone());

        result
    }
}

impl Default for Variables {
    fn default() -> Self {
        Self::new()
    }
}
