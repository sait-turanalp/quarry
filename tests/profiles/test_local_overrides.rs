//! Tests for local overrides parsing

use codanna::profiles::local::LocalOverrides;

#[test]
fn test_parse_local_overrides() {
    let json = r#"{
        "profile": "my-custom-profile"
    }"#;

    let overrides = LocalOverrides::from_json(json).unwrap();
    assert_eq!(overrides.profile, Some("my-custom-profile".to_string()));
}

#[test]
fn test_parse_empty_overrides() {
    let json = r#"{}"#;

    let overrides = LocalOverrides::from_json(json).unwrap();
    assert_eq!(overrides.profile, None);
}
