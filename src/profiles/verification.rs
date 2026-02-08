//! Profile integrity verification
//!
//! Plugin reference: src/plugins/mod.rs:516-574 (verify_entry), 355-407 (verify_plugin, verify_all_plugins)

use super::error::{ProfileError, ProfileResult};
use super::fsops::calculate_integrity;
use super::lockfile::{ProfileLockEntry, ProfileLockfile};
use std::path::Path;

/// Verify integrity of a specific profile
///
/// Checks that all files tracked in the lockfile match their expected integrity hash.
///
/// # Errors
/// - `NotInstalled` if profile is not in lockfile
/// - `IntegrityCheckFailed` if files have been modified or are missing
pub fn verify_profile(workspace: &Path, profile_name: &str, verbose: bool) -> ProfileResult<()> {
    let lockfile_path = workspace.join(".codanna/profiles.lock.json");
    let lockfile = ProfileLockfile::load(&lockfile_path)?;

    let entry = lockfile
        .get_profile(profile_name)
        .ok_or_else(|| ProfileError::NotInstalled {
            name: profile_name.to_string(),
        })?;

    verify_profile_entry(workspace, entry, verbose)
}

/// Verify all installed profiles
///
/// Checks integrity of every profile in the lockfile.
///
/// # Errors
/// - `IntegrityCheckFailed` if any profile fails verification
pub fn verify_all_profiles(workspace: &Path, verbose: bool) -> ProfileResult<()> {
    let lockfile_path = workspace.join(".codanna/profiles.lock.json");
    let lockfile = ProfileLockfile::load(&lockfile_path)?;

    if lockfile.profiles.is_empty() {
        if verbose {
            println!("No profiles installed");
        }
        return Ok(());
    }

    for entry in lockfile.profiles.values() {
        verify_profile_entry(workspace, entry, verbose)?;
    }

    println!("All profiles verified successfully");
    Ok(())
}

/// Internal: Verify a single profile entry
///
/// Plugin reference: src/plugins/mod.rs:516-574
fn verify_profile_entry(
    workspace: &Path,
    entry: &ProfileLockEntry,
    verbose: bool,
) -> ProfileResult<()> {
    if verbose {
        println!("Verifying profile '{}'...", entry.name);
        println!("  Stored integrity: {}", entry.integrity);
    }

    // If integrity is empty (legacy lockfile), skip verification
    if entry.integrity.is_empty() {
        if verbose {
            println!("  WARNING: No integrity hash stored (legacy lockfile)");
            println!("  Skipping verification - reinstall profile to add integrity checking");
        }
        return Ok(());
    }

    // Calculate integrity of installed files
    let absolute_files: Vec<String> = entry
        .files
        .iter()
        .map(|rel| workspace.join(rel).to_string_lossy().to_string())
        .collect();

    let actual = calculate_integrity(&absolute_files)?;

    if actual != entry.integrity {
        return Err(ProfileError::IntegrityCheckFailed {
            profile: entry.name.clone(),
            expected: entry.integrity.clone(),
            actual,
        });
    }

    if verbose {
        println!("  Calculated integrity: {actual}");
        println!("  Integrity OK ({} files)", entry.files.len());
    } else {
        println!(
            "Profile '{}' verified ({} files)",
            entry.name,
            entry.files.len()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiles::lockfile::ProfileLockEntry;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_verify_succeeds_with_matching_integrity() {
        let temp = tempdir().unwrap();
        let workspace = temp.path();

        // Create test files
        fs::write(workspace.join("test1.txt"), "content 1").unwrap();
        fs::write(workspace.join("test2.txt"), "content 2").unwrap();

        let files = vec![
            workspace.join("test1.txt").to_string_lossy().to_string(),
            workspace.join("test2.txt").to_string_lossy().to_string(),
        ];
        let integrity = calculate_integrity(&files).unwrap();

        // Create lockfile with correct integrity
        let lockfile_path = workspace.join(".codanna/profiles.lock.json");
        fs::create_dir_all(lockfile_path.parent().unwrap()).unwrap();

        let entry = ProfileLockEntry {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            installed_at: "2025-01-11".to_string(),
            files: vec!["test1.txt".to_string(), "test2.txt".to_string()],
            integrity,
            commit: None,
            provider_id: None,
            source: None,
        };

        let mut lockfile = ProfileLockfile::new();
        lockfile.add_profile(entry);
        lockfile.save(&lockfile_path).unwrap();

        // Verify should succeed
        let result = verify_profile(workspace, "test", false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_fails_with_modified_files() {
        let temp = tempdir().unwrap();
        let workspace = temp.path();

        // Create test files
        fs::write(workspace.join("test1.txt"), "content 1").unwrap();
        fs::write(workspace.join("test2.txt"), "content 2").unwrap();

        let files = vec![
            workspace.join("test1.txt").to_string_lossy().to_string(),
            workspace.join("test2.txt").to_string_lossy().to_string(),
        ];
        let integrity = calculate_integrity(&files).unwrap();

        // Create lockfile
        let lockfile_path = workspace.join(".codanna/profiles.lock.json");
        fs::create_dir_all(lockfile_path.parent().unwrap()).unwrap();

        let entry = ProfileLockEntry {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            installed_at: "2025-01-11".to_string(),
            files: vec!["test1.txt".to_string(), "test2.txt".to_string()],
            integrity,
            commit: None,
            provider_id: None,
            source: None,
        };

        let mut lockfile = ProfileLockfile::new();
        lockfile.add_profile(entry);
        lockfile.save(&lockfile_path).unwrap();

        // Modify a file
        fs::write(workspace.join("test1.txt"), "MODIFIED").unwrap();

        // Verify should fail
        let result = verify_profile(workspace, "test", false);
        assert!(result.is_err());
        match result {
            Err(ProfileError::IntegrityCheckFailed { profile, .. }) => {
                assert_eq!(profile, "test");
            }
            _ => panic!("Expected IntegrityCheckFailed error"),
        }
    }

    #[test]
    fn test_verify_fails_with_missing_files() {
        let temp = tempdir().unwrap();
        let workspace = temp.path();

        // Create test files
        fs::write(workspace.join("test1.txt"), "content 1").unwrap();
        fs::write(workspace.join("test2.txt"), "content 2").unwrap();

        let files = vec![
            workspace.join("test1.txt").to_string_lossy().to_string(),
            workspace.join("test2.txt").to_string_lossy().to_string(),
        ];
        let integrity = calculate_integrity(&files).unwrap();

        // Create lockfile
        let lockfile_path = workspace.join(".codanna/profiles.lock.json");
        fs::create_dir_all(lockfile_path.parent().unwrap()).unwrap();

        let entry = ProfileLockEntry {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            installed_at: "2025-01-11".to_string(),
            files: vec!["test1.txt".to_string(), "test2.txt".to_string()],
            integrity,
            commit: None,
            provider_id: None,
            source: None,
        };

        let mut lockfile = ProfileLockfile::new();
        lockfile.add_profile(entry);
        lockfile.save(&lockfile_path).unwrap();

        // Delete a file
        fs::remove_file(workspace.join("test2.txt")).unwrap();

        // Verify should fail
        let result = verify_profile(workspace, "test", false);
        assert!(result.is_err());
        match result {
            Err(ProfileError::IntegrityCheckFailed { .. }) => {}
            _ => panic!("Expected IntegrityCheckFailed error"),
        }
    }

    #[test]
    fn test_verify_all_profiles() {
        let temp = tempdir().unwrap();
        let workspace = temp.path();

        // Create files for two profiles
        fs::write(workspace.join("profile1.txt"), "content 1").unwrap();
        fs::write(workspace.join("profile2.txt"), "content 2").unwrap();

        let files1 = vec![workspace.join("profile1.txt").to_string_lossy().to_string()];
        let integrity1 = calculate_integrity(&files1).unwrap();

        let files2 = vec![workspace.join("profile2.txt").to_string_lossy().to_string()];
        let integrity2 = calculate_integrity(&files2).unwrap();

        // Create lockfile with two profiles
        let lockfile_path = workspace.join(".codanna/profiles.lock.json");
        fs::create_dir_all(lockfile_path.parent().unwrap()).unwrap();

        let mut lockfile = ProfileLockfile::new();

        lockfile.add_profile(ProfileLockEntry {
            name: "profile1".to_string(),
            version: "1.0.0".to_string(),
            installed_at: "2025-01-11".to_string(),
            files: vec!["profile1.txt".to_string()],
            integrity: integrity1,
            commit: None,
            provider_id: None,
            source: None,
        });

        lockfile.add_profile(ProfileLockEntry {
            name: "profile2".to_string(),
            version: "1.0.0".to_string(),
            installed_at: "2025-01-11".to_string(),
            files: vec!["profile2.txt".to_string()],
            integrity: integrity2,
            commit: None,
            provider_id: None,
            source: None,
        });

        lockfile.save(&lockfile_path).unwrap();

        // Verify all should succeed
        let result = verify_all_profiles(workspace, false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_skips_legacy_without_integrity() {
        let temp = tempdir().unwrap();
        let workspace = temp.path();

        // Create test file
        fs::write(workspace.join("test.txt"), "content").unwrap();

        // Create lockfile without integrity (legacy)
        let lockfile_path = workspace.join(".codanna/profiles.lock.json");
        fs::create_dir_all(lockfile_path.parent().unwrap()).unwrap();

        let entry = ProfileLockEntry {
            name: "legacy".to_string(),
            version: "1.0.0".to_string(),
            installed_at: "2025-01-11".to_string(),
            files: vec!["test.txt".to_string()],
            integrity: String::new(), // Empty = legacy
            commit: None,
            provider_id: None,
            source: None,
        };

        let mut lockfile = ProfileLockfile::new();
        lockfile.add_profile(entry);
        lockfile.save(&lockfile_path).unwrap();

        // Verify should succeed (skip verification for legacy)
        let result = verify_profile(workspace, "legacy", false);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_not_installed() {
        let temp = tempdir().unwrap();
        let workspace = temp.path();

        // Create empty lockfile
        let lockfile_path = workspace.join(".codanna/profiles.lock.json");
        fs::create_dir_all(lockfile_path.parent().unwrap()).unwrap();
        let lockfile = ProfileLockfile::new();
        lockfile.save(&lockfile_path).unwrap();

        // Verify should fail with NotInstalled
        let result = verify_profile(workspace, "nonexistent", false);
        assert!(result.is_err());
        match result {
            Err(ProfileError::NotInstalled { name }) => {
                assert_eq!(name, "nonexistent");
            }
            _ => panic!("Expected NotInstalled error"),
        }
    }
}
