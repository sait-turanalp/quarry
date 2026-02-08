//! Tests for profile lockfile

use codanna::profiles::lockfile::{ProfileLockEntry, ProfileLockfile};

#[test]
fn test_new_lockfile() {
    let lockfile = ProfileLockfile::new();
    assert_eq!(lockfile.version, "1.0.0");
    assert!(lockfile.profiles.is_empty());
}

#[test]
fn test_add_profile() {
    let mut lockfile = ProfileLockfile::new();
    let entry = ProfileLockEntry {
        name: "claude".to_string(),
        version: "1.0.0".to_string(),
        installed_at: "2025-01-11".to_string(),
        files: vec!["CLAUDE.md".to_string(), ".clauderc".to_string()],
        integrity: "abc123".to_string(),
        commit: None,
        provider_id: None,
        source: None,
    };

    lockfile.add_profile(entry);
    assert!(lockfile.is_installed("claude"));
    assert_eq!(lockfile.get_profile("claude").unwrap().files.len(), 2);
    assert_eq!(lockfile.get_profile("claude").unwrap().integrity, "abc123");
}

#[test]
fn test_find_file_owner() {
    let mut lockfile = ProfileLockfile::new();
    let entry = ProfileLockEntry {
        name: "claude".to_string(),
        version: "1.0.0".to_string(),
        installed_at: "2025-01-11".to_string(),
        files: vec!["CLAUDE.md".to_string()],
        integrity: "def456".to_string(),
        commit: None,
        provider_id: None,
        source: None,
    };

    lockfile.add_profile(entry);
    assert_eq!(lockfile.find_file_owner("CLAUDE.md"), Some("claude"));
    assert_eq!(lockfile.find_file_owner("OTHER.md"), None);
}

#[test]
fn test_serialize_with_integrity() {
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let lockfile_path = temp.path().join("lockfile.json");

    let mut lockfile = ProfileLockfile::new();
    let entry = ProfileLockEntry {
        name: "test".to_string(),
        version: "1.0.0".to_string(),
        installed_at: "2025-01-11T00:00:00Z".to_string(),
        files: vec!["test.txt".to_string()],
        integrity: "sha256hash".to_string(),
        commit: None,
        provider_id: None,
        source: None,
    };

    lockfile.add_profile(entry);
    lockfile.save(&lockfile_path).unwrap();

    // Read back and verify integrity is saved
    let loaded = ProfileLockfile::load(&lockfile_path).unwrap();
    let loaded_entry = loaded.get_profile("test").unwrap();
    assert_eq!(loaded_entry.integrity, "sha256hash");
}

#[test]
fn test_deserialize_legacy_without_integrity() {
    use tempfile::tempdir;

    let temp = tempdir().unwrap();
    let lockfile_path = temp.path().join("lockfile.json");

    // Write a legacy lockfile without integrity field
    let legacy_json = r#"{
        "version": "1.0.0",
        "profiles": {
            "legacy": {
                "name": "legacy",
                "version": "1.0.0",
                "installed_at": "2025-01-11",
                "files": ["test.txt"]
            }
        }
    }"#;

    std::fs::write(&lockfile_path, legacy_json).unwrap();

    // Should load successfully with empty integrity
    let loaded = ProfileLockfile::load(&lockfile_path).unwrap();
    let loaded_entry = loaded.get_profile("legacy").unwrap();
    assert_eq!(loaded_entry.integrity, ""); // Default value
    assert_eq!(loaded_entry.files.len(), 1);
}
