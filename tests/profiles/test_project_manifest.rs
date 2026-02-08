//! Tests for project manifest parsing

use codanna::profiles::project::ProjectManifest;
use tempfile::tempdir;

#[test]
fn test_parse_minimal_project_manifest() {
    let json = r#"{
        "profile": "claude"
    }"#;

    let manifest = ProjectManifest::from_json(json).unwrap();
    assert_eq!(manifest.profile, "claude");
}

#[test]
fn test_reject_empty_profile() {
    let json = r#"{
        "profile": ""
    }"#;

    let result = ProjectManifest::from_json(json);
    assert!(result.is_err());

    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Profile name"));
}

#[test]
fn test_save_and_load() {
    let temp = tempdir().unwrap();
    let manifest_path = temp.path().join("manifest.json");

    let mut manifest = ProjectManifest::new();
    manifest.profile = "claude".to_string();

    // Save
    manifest.save(&manifest_path).unwrap();
    assert!(manifest_path.exists());

    // Load
    let loaded = ProjectManifest::load(&manifest_path).unwrap();
    assert_eq!(loaded.profile, "claude");
}

#[test]
fn test_load_missing_file() {
    let temp = tempdir().unwrap();
    let manifest_path = temp.path().join("missing.json");

    let result = ProjectManifest::load(&manifest_path);
    assert!(result.is_err());
}

#[test]
fn test_load_or_create_existing() {
    let temp = tempdir().unwrap();
    let manifest_path = temp.path().join("manifest.json");

    // Create existing
    let mut manifest = ProjectManifest::new();
    manifest.profile = "existing".to_string();
    manifest.save(&manifest_path).unwrap();

    // Load or create should load existing
    let loaded = ProjectManifest::load_or_create(&manifest_path).unwrap();
    assert_eq!(loaded.profile, "existing");
}

#[test]
fn test_load_or_create_missing() {
    let temp = tempdir().unwrap();
    let manifest_path = temp.path().join("missing.json");

    // Should create new
    let manifest = ProjectManifest::load_or_create(&manifest_path).unwrap();
    assert!(manifest.profile.is_empty());
}
