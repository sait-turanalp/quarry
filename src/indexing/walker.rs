//! File system walker for discovering source files to index
//!
//! This module provides efficient directory traversal with support for:
//! - .gitignore rules
//! - Custom ignore patterns from configuration
//! - Language filtering
//! - Hidden file handling

use crate::Settings;
use crate::parsing::get_registry;
use glob::Pattern;
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Walks directories to find source files to index
#[derive(Debug)]
pub struct FileWalker {
    settings: Arc<Settings>,
}

impl FileWalker {
    /// Create a new file walker with the given settings
    pub fn new(settings: Arc<Settings>) -> Self {
        Self { settings }
    }

    /// Walk a directory and return an iterator of files to index
    pub fn walk(&self, root: &Path) -> impl Iterator<Item = PathBuf> {
        let mut builder = WalkBuilder::new(root);
        let include_tests = self.settings.indexing.include_tests;
        let ignore_patterns = compile_ignore_patterns(&self.settings.indexing.ignore_patterns);
        let root = root.to_path_buf();

        // Configure the walker
        builder
            .hidden(false) // Don't traverse hidden directories by default
            .git_ignore(true) // Respect .gitignore files
            .git_global(true) // Respect global gitignore
            .git_exclude(true) // Respect .git/info/exclude
            .follow_links(false) // Don't follow symlinks by default
            .max_depth(None) // No depth limit
            .require_git(false); // Allow gitignore to work in non-git directories

        // Always support .quarryignore files for custom ignore patterns (follows .gitignore pattern)
        builder.add_custom_ignore_filename(".quarryignore");

        // The ignore crate's override feature is for INCLUDING files, not excluding them.
        // To add custom ignore patterns, we need to use a different approach.
        // For now, we'll rely on .gitignore and .quarryignore files.

        // TODO: Add support for custom ignore patterns from settings
        // One approach would be to create a temporary .quarry-ignore file
        // or use the glob filtering in the iterator below

        // Get enabled extensions from the registry
        let enabled_extensions = self.get_enabled_extensions();

        // Build and filter the walker
        builder
            .build()
            .filter_map(Result::ok) // Skip files we can't access
            .filter(|entry| entry.file_type().is_some_and(|ft| ft.is_file()))
            .filter_map(move |entry| {
                let path = entry.path();

                // Skip hidden files (files starting with .)
                if let Some(file_name) = path.file_name() {
                    if let Some(name_str) = file_name.to_str() {
                        if name_str.starts_with('.') {
                            return None;
                        }
                    }
                }

                if !include_tests && is_test_path(path) {
                    return None;
                }

                if path_matches_ignore_patterns(path, &root, &ignore_patterns) {
                    return None;
                }

                // Check if this file extension is enabled
                if let Some(extension) = path.extension() {
                    if let Some(ext_str) = extension.to_str() {
                        if enabled_extensions.iter().any(|ext| ext == ext_str) {
                            return Some(path.to_path_buf());
                        }
                    }
                }

                None
            })
    }

    /// Get list of enabled file extensions from the registry
    fn get_enabled_extensions(&self) -> Vec<String> {
        let registry = get_registry();
        if let Ok(registry) = registry.lock() {
            registry
                .enabled_extensions(&self.settings)
                .map(|ext| ext.to_string())
                .collect()
        } else {
            // Fallback to empty if registry lock fails
            Vec::new()
        }
    }

    /// Count files that would be indexed (useful for dry runs)
    pub fn count_files(&self, root: &Path) -> usize {
        self.walk(root).count()
    }
}

fn is_test_path(path: &Path) -> bool {
    for component in path.components() {
        let comp = component.as_os_str().to_string_lossy();
        if comp == "tests" || comp == "test" || comp == "__tests__" {
            return true;
        }
    }

    let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };

    if file_name.starts_with("test_") {
        return true;
    }

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    stem.ends_with("_test") || stem.ends_with(".test") || stem.ends_with(".spec")
}

fn compile_ignore_patterns(raw_patterns: &[String]) -> Vec<Pattern> {
    let mut compiled = Vec::new();
    for raw in raw_patterns {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut candidates = vec![trimmed.to_string()];
        if trimmed.ends_with("/**") && !trimmed.starts_with("**/") && !trimmed.starts_with('/') {
            candidates.push(format!("**/{trimmed}"));
        }
        for candidate in candidates {
            if let Ok(pattern) = Pattern::new(&candidate) {
                compiled.push(pattern);
            }
        }
    }
    compiled
}

fn normalize_glob_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn path_matches_ignore_patterns(path: &Path, root: &Path, patterns: &[Pattern]) -> bool {
    if patterns.is_empty() {
        return false;
    }
    let abs = normalize_glob_path(path);
    let rel = path
        .strip_prefix(root)
        .ok()
        .map(normalize_glob_path)
        .unwrap_or_else(|| abs.clone());
    let rel_dot = if rel.starts_with("./") {
        rel.clone()
    } else {
        format!("./{rel}")
    };

    patterns
        .iter()
        .any(|pattern| pattern.matches(&rel) || pattern.matches(&rel_dot) || pattern.matches(&abs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_settings() -> Arc<Settings> {
        let mut settings = Settings::default();
        // Disable Python and PHP for testing (only Rust enabled)
        settings.languages.get_mut("python").unwrap().enabled = false;
        settings.languages.get_mut("php").unwrap().enabled = false;
        // Rust remains enabled by default
        Arc::new(settings)
    }

    #[test]
    fn test_walk_directory() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create some test files
        fs::write(root.join("main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("lib.rs"), "pub fn lib() {}").unwrap();
        fs::write(root.join("test.py"), "def test(): pass").unwrap();
        fs::write(root.join("README.md"), "# Test").unwrap();

        let settings = create_test_settings();
        let walker = FileWalker::new(settings);

        let files: Vec<_> = walker.walk(root).collect();

        // Should find only Rust files (Python and PHP disabled in test settings)
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|p| p.ends_with("main.rs")));
        assert!(files.iter().any(|p| p.ends_with("lib.rs")));
    }

    #[test]
    fn test_ignore_hidden_files() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create hidden file and visible file
        fs::write(root.join(".hidden.rs"), "fn hidden() {}").unwrap();
        fs::write(root.join("visible.rs"), "fn visible() {}").unwrap();

        let settings = create_test_settings();
        let walker = FileWalker::new(settings);

        let files: Vec<_> = walker.walk(root).collect();

        // Should only find the visible file (hidden files are filtered out)
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("visible.rs"));
    }

    #[test]
    fn test_gitignore_respected() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create .gitignore (should work without git init due to require_git(false))
        fs::write(root.join(".gitignore"), "ignored.rs\n").unwrap();

        // Create files
        fs::write(root.join("ignored.rs"), "fn ignored() {}").unwrap();
        fs::write(root.join("included.rs"), "fn included() {}").unwrap();

        let settings = create_test_settings();
        let walker = FileWalker::new(settings);

        let files: Vec<_> = walker.walk(root).collect();

        // Should only find the included file
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("included.rs"));
    }

    #[test]
    fn test_settings_ignore_patterns_respected() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let skip_dir = root.join("nested").join("target");
        fs::create_dir_all(&skip_dir).unwrap();
        fs::write(skip_dir.join("skip.rs"), "fn skip() {}").unwrap();
        fs::write(root.join("keep.rs"), "fn keep() {}").unwrap();

        let mut settings = Settings::default();
        settings.languages.get_mut("python").unwrap().enabled = false;
        settings.languages.get_mut("php").unwrap().enabled = false;
        settings.indexing.ignore_patterns = vec!["target/**".to_string()];
        let walker = FileWalker::new(Arc::new(settings));

        let files: Vec<_> = walker.walk(root).collect();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("keep.rs"));
    }
}
