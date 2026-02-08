//! Tests for profile manifest parsing

use codanna::profiles::manifest::ProfileManifest;

#[test]
fn test_parse_minimal_profile_manifest() {
    let json = r#"{
        "name": "codanna",
        "version": "1.0.0",
        "files": []
    }"#;

    let manifest = ProfileManifest::from_json(json).unwrap();
    assert_eq!(manifest.name, "codanna");
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(manifest.files.len(), 0);
}

#[test]
fn test_reject_empty_name() {
    let json = r#"{
        "name": "",
        "version": "1.0.0",
        "files": []
    }"#;

    let result = ProfileManifest::from_json(json);
    assert!(result.is_err());

    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("name"));
}

#[test]
fn test_reject_empty_version() {
    let json = r#"{
        "name": "codanna",
        "version": "",
        "files": []
    }"#;

    let result = ProfileManifest::from_json(json);
    assert!(result.is_err());

    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("version"));
}

#[test]
fn test_skip_empty_file_paths() {
    let json = r#"{
        "name": "codanna",
        "version": "1.0.0",
        "files": ["CLAUDE.md", "", "README.md", ""]
    }"#;

    let manifest = ProfileManifest::from_json(json).unwrap();
    assert_eq!(manifest.files.len(), 2);
    assert_eq!(manifest.files[0], "CLAUDE.md");
    assert_eq!(manifest.files[1], "README.md");
}
