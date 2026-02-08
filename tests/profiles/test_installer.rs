//! Tests for profile file installation

use codanna::profiles::installer::ProfileInstaller;
use codanna::profiles::lockfile::{ProfileLockEntry, ProfileLockfile};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_install_single_file() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("source");
    let dest_dir = temp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    // Create source file
    let source_file = source_dir.join("CLAUDE.md");
    fs::write(&source_file, "# Claude Profile").unwrap();

    // Create empty lockfile
    let lockfile = ProfileLockfile::new();

    let installer = ProfileInstaller::new();
    let files = vec!["CLAUDE.md".to_string()];

    let (installed, sidecars) = installer
        .install_files(
            &source_dir,
            &dest_dir,
            &files,
            "test-profile",
            "test",
            &lockfile,
            false,
        )
        .unwrap();

    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0], "CLAUDE.md");
    assert_eq!(sidecars.len(), 0); // No conflicts

    // Verify file exists
    let dest_file = dest_dir.join("CLAUDE.md");
    assert!(dest_file.exists());
    assert_eq!(fs::read_to_string(&dest_file).unwrap(), "# Claude Profile");
}

#[test]
fn test_install_multiple_files() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("source");
    let dest_dir = temp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    // Create source files
    fs::write(source_dir.join("file1.txt"), "content1").unwrap();
    fs::write(source_dir.join("file2.txt"), "content2").unwrap();

    let lockfile = ProfileLockfile::new();
    let installer = ProfileInstaller::new();
    let files = vec!["file1.txt".to_string(), "file2.txt".to_string()];

    let (installed, sidecars) = installer
        .install_files(
            &source_dir,
            &dest_dir,
            &files,
            "test-profile",
            "test",
            &lockfile,
            false,
        )
        .unwrap();

    assert_eq!(installed.len(), 2);
    assert_eq!(sidecars.len(), 0);
    assert!(dest_dir.join("file1.txt").exists());
    assert!(dest_dir.join("file2.txt").exists());
}

#[test]
fn test_install_with_subdirectory() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("source");
    let dest_dir = temp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    // Create nested file
    let nested_dir = source_dir.join("subdir");
    fs::create_dir_all(&nested_dir).unwrap();
    fs::write(nested_dir.join("nested.txt"), "nested content").unwrap();

    let lockfile = ProfileLockfile::new();
    let installer = ProfileInstaller::new();
    let files = vec!["subdir/nested.txt".to_string()];

    let (installed, sidecars) = installer
        .install_files(
            &source_dir,
            &dest_dir,
            &files,
            "test-profile",
            "test",
            &lockfile,
            false,
        )
        .unwrap();

    assert_eq!(installed.len(), 1);
    assert_eq!(sidecars.len(), 0);

    // Verify nested structure created
    let dest_file = dest_dir.join("subdir/nested.txt");
    assert!(dest_file.exists());
    assert_eq!(fs::read_to_string(&dest_file).unwrap(), "nested content");
}

#[test]
fn test_skip_nonexistent_files() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("source");
    let dest_dir = temp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    // Only create one file
    fs::write(source_dir.join("exists.txt"), "content").unwrap();

    let lockfile = ProfileLockfile::new();
    let installer = ProfileInstaller::new();
    let files = vec!["exists.txt".to_string(), "missing.txt".to_string()];

    let (installed, sidecars) = installer
        .install_files(
            &source_dir,
            &dest_dir,
            &files,
            "test-profile",
            "test",
            &lockfile,
            false,
        )
        .unwrap();

    // Should only install the file that exists
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0], "exists.txt");
    assert_eq!(sidecars.len(), 0);
}

// Sidecar conflict resolution tests

#[test]
fn test_sidecar_for_unknown_owner() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("source");
    let dest_dir = temp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    // Create source file
    fs::write(source_dir.join("CLAUDE.md"), "# Profile Version").unwrap();

    // Create existing file (unknown owner)
    fs::write(dest_dir.join("CLAUDE.md"), "# User's Existing File").unwrap();

    // Empty lockfile (file not tracked)
    let lockfile = ProfileLockfile::new();
    let installer = ProfileInstaller::new();
    let files = vec!["CLAUDE.md".to_string()];

    let (installed, sidecars) = installer
        .install_files(
            &source_dir,
            &dest_dir,
            &files,
            "test-profile",
            "codanna",
            &lockfile,
            true,
        )
        .unwrap();

    // Should create sidecar
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0], "CLAUDE.codanna.md");
    assert_eq!(sidecars.len(), 1);
    assert_eq!(sidecars[0].0, "CLAUDE.md"); // intended
    assert_eq!(sidecars[0].1, "CLAUDE.codanna.md"); // actual

    // Original file preserved
    let original_content = fs::read_to_string(dest_dir.join("CLAUDE.md")).unwrap();
    assert_eq!(original_content, "# User's Existing File");

    // Sidecar file created
    let sidecar_content = fs::read_to_string(dest_dir.join("CLAUDE.codanna.md")).unwrap();
    assert_eq!(sidecar_content, "# Profile Version");
}

#[test]
fn test_sidecar_for_different_profile() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("source");
    let dest_dir = temp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(source_dir.join("CLAUDE.md"), "# Profile B").unwrap();
    fs::write(dest_dir.join("CLAUDE.md"), "# Profile A").unwrap();

    // Lockfile shows profile-a owns the file
    let mut lockfile = ProfileLockfile::new();
    lockfile.add_profile(ProfileLockEntry {
        name: "profile-a".to_string(),
        version: "1.0.0".to_string(),
        installed_at: "2025-01-11".to_string(),
        files: vec!["CLAUDE.md".to_string()],
        integrity: "abc123".to_string(),
        commit: None,
        provider_id: None,
        source: None,
    });

    let installer = ProfileInstaller::new();
    let files = vec!["CLAUDE.md".to_string()];

    let (installed, sidecars) = installer
        .install_files(
            &source_dir,
            &dest_dir,
            &files,
            "profile-b",
            "codanna",
            &lockfile,
            true,
        )
        .unwrap();

    // Should create sidecar
    assert_eq!(sidecars.len(), 1);
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0], "CLAUDE.codanna.md"); // Installed list contains sidecar path
    assert!(dest_dir.join("CLAUDE.codanna.md").exists());

    // Profile A's file preserved
    let original_content = fs::read_to_string(dest_dir.join("CLAUDE.md")).unwrap();
    assert_eq!(original_content, "# Profile A");
}

#[test]
fn test_same_profile_updates_without_sidecar() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("source");
    let dest_dir = temp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    fs::write(source_dir.join("CLAUDE.md"), "# Updated Version").unwrap();
    fs::write(dest_dir.join("CLAUDE.md"), "# Original Version").unwrap();

    // Lockfile shows test-profile owns the file
    let mut lockfile = ProfileLockfile::new();
    lockfile.add_profile(ProfileLockEntry {
        name: "test-profile".to_string(),
        version: "1.0.0".to_string(),
        installed_at: "2025-01-11".to_string(),
        files: vec!["CLAUDE.md".to_string()],
        integrity: "abc123".to_string(),
        commit: None,
        provider_id: None,
        source: None,
    });

    let installer = ProfileInstaller::new();
    let files = vec!["CLAUDE.md".to_string()];

    let (installed, sidecars) = installer
        .install_files(
            &source_dir,
            &dest_dir,
            &files,
            "test-profile",
            "codanna",
            &lockfile,
            true,
        )
        .unwrap();

    // Should overwrite (not create sidecar) - we own it
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0], "CLAUDE.md");
    assert_eq!(sidecars.len(), 0); // No sidecar!

    // File should be updated
    let content = fs::read_to_string(dest_dir.join("CLAUDE.md")).unwrap();
    assert_eq!(content, "# Updated Version");
}

#[test]
fn test_sidecar_naming_patterns() {
    let temp = tempdir().unwrap();
    let source_dir = temp.path().join("source");
    let dest_dir = temp.path().join("dest");
    fs::create_dir_all(&source_dir).unwrap();
    fs::create_dir_all(&dest_dir).unwrap();

    // Test various file naming patterns
    let test_cases = vec![
        ("CLAUDE.md", "CLAUDE.codanna.md"),
        (".gitignore", ".gitignore.codanna"),
        ("docker-compose.yml", "docker-compose.codanna.yml"),
        ("file.tar.gz", "file.codanna.tar.gz"),
    ];

    let lockfile = ProfileLockfile::new();

    for (original, expected_sidecar) in test_cases {
        // Create source and existing dest
        fs::write(source_dir.join(original), "source").unwrap();
        fs::write(dest_dir.join(original), "existing").unwrap();

        let installer = ProfileInstaller::new();
        let files = vec![original.to_string()];

        let (_, sidecars) = installer
            .install_files(
                &source_dir,
                &dest_dir,
                &files,
                "test",
                "codanna",
                &lockfile,
                true,
            )
            .unwrap();

        assert_eq!(sidecars.len(), 1);
        assert_eq!(sidecars[0].1, expected_sidecar);
        assert!(dest_dir.join(expected_sidecar).exists());

        // Cleanup for next iteration
        fs::remove_file(dest_dir.join(original)).unwrap();
        fs::remove_file(dest_dir.join(expected_sidecar)).unwrap();
    }
}
