//! State management for language behaviors
//!
//! This module provides thread-safe state management for language behaviors,
//! allowing them to track imports, file mappings, and other stateful information
//! across indexing operations.

use crate::FileId;
use crate::parsing::Import;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Thread-safe state container for language behaviors
///
/// Each language behavior can maintain its own state for tracking imports,
/// file-to-module mappings, and other language-specific information.
#[derive(Debug, Clone)]
pub struct BehaviorState {
    inner: Arc<RwLock<BehaviorStateInner>>,
}

#[derive(Debug, Default)]
struct BehaviorStateInner {
    /// Import statements by file
    imports_by_file: HashMap<FileId, Vec<Import>>,

    /// Maps file paths to their module paths
    file_to_module: HashMap<PathBuf, String>,

    /// Maps module paths to file paths
    module_to_file: HashMap<String, PathBuf>,

    /// Maps file paths to FileIds
    path_to_file_id: HashMap<PathBuf, FileId>,

    /// Maps FileId to module path for quick lookup
    file_id_to_module: HashMap<FileId, String>,

    /// Cache project roots by FileId for performance
    project_roots: HashMap<FileId, PathBuf>,
}

impl BehaviorState {
    /// Create a new empty state container
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(BehaviorStateInner::default())),
        }
    }

    /// Register a file with its module path
    pub fn register_file(&self, path: PathBuf, file_id: FileId, module_path: String) {
        let mut state = self.inner.write().unwrap();

        // Update all mappings
        state
            .file_to_module
            .insert(path.clone(), module_path.clone());
        state
            .module_to_file
            .insert(module_path.clone(), path.clone());
        state.path_to_file_id.insert(path, file_id);
        state.file_id_to_module.insert(file_id, module_path);
    }

    /// Add an import to a file
    pub fn add_import(&self, import: Import) {
        let mut state = self.inner.write().unwrap();
        state
            .imports_by_file
            .entry(import.file_id)
            .or_default()
            .push(import);
    }

    /// Get all imports for a file
    pub fn get_imports_for_file(&self, file_id: FileId) -> Vec<Import> {
        let state = self.inner.read().unwrap();
        state
            .imports_by_file
            .get(&file_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Get the module path for a file
    pub fn get_module_path(&self, file_id: FileId) -> Option<String> {
        let state = self.inner.read().unwrap();
        state.file_id_to_module.get(&file_id).cloned()
    }

    /// Resolve a module path to a file path
    pub fn resolve_module_to_file(&self, module_path: &str) -> Option<PathBuf> {
        let state = self.inner.read().unwrap();
        state.module_to_file.get(module_path).cloned()
    }

    /// Get the FileId for a path
    pub fn get_file_id(&self, path: &Path) -> Option<FileId> {
        let state = self.inner.read().unwrap();
        state.path_to_file_id.get(path).copied()
    }

    /// Get the file path for a FileId
    pub fn get_file_path(&self, file_id: FileId) -> Option<PathBuf> {
        let state = self.inner.read().unwrap();
        // Look through path_to_file_id to find the path for this file_id
        for (path, &id) in &state.path_to_file_id {
            if id == file_id {
                return Some(path.clone());
            }
        }
        None
    }

    /// Set the project root for a file (for caching)
    pub fn set_project_root(&self, file_id: FileId, project_root: PathBuf) {
        let mut state = self.inner.write().unwrap();
        state.project_roots.insert(file_id, project_root);
    }

    /// Get the cached project root for a file
    pub fn get_project_root(&self, file_id: FileId) -> Option<PathBuf> {
        let state = self.inner.read().unwrap();
        state.project_roots.get(&file_id).cloned()
    }

    /// Clear all state (useful for testing)
    pub fn clear(&self) {
        let mut state = self.inner.write().unwrap();
        state.imports_by_file.clear();
        state.file_to_module.clear();
        state.module_to_file.clear();
        state.path_to_file_id.clear();
        state.file_id_to_module.clear();
        state.project_roots.clear();
    }
}

impl Default for BehaviorState {
    fn default() -> Self {
        Self::new()
    }
}

/// Extension trait for language behaviors with state
///
/// This trait provides default implementations for stateful operations,
/// allowing languages to maintain import and file tracking information.
pub trait StatefulBehavior {
    /// Get the behavior's state container
    fn state(&self) -> &BehaviorState;

    /// Register a file with state tracking
    fn register_file_with_state(&self, path: PathBuf, file_id: FileId, module_path: String) {
        self.state().register_file(path, file_id, module_path);
    }

    /// Add an import with state tracking
    fn add_import_with_state(&self, import: Import) {
        self.state().add_import(import);
    }

    /// Get imports for a file from state
    fn get_imports_from_state(&self, file_id: FileId) -> Vec<Import> {
        self.state().get_imports_for_file(file_id)
    }
}
