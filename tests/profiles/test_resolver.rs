//! Tests for profile resolution

use codanna::profiles::resolver::ProfileResolver;

#[test]
fn test_resolve_from_manifest_only() {
    let resolver = ProfileResolver::new();

    // Simulate manifest with profile
    let manifest_profile = Some("claude".to_string());
    let local_profile = None;
    let cli_profile = None;

    let resolved = resolver.resolve_profile_name(cli_profile, local_profile, manifest_profile);
    assert_eq!(resolved, Some("claude".to_string()));
}

#[test]
fn test_resolve_local_overrides_manifest() {
    let resolver = ProfileResolver::new();

    let manifest_profile = Some("claude".to_string());
    let local_profile = Some("my-custom".to_string());
    let cli_profile = None;

    // Local should override manifest
    let resolved = resolver.resolve_profile_name(cli_profile, local_profile, manifest_profile);
    assert_eq!(resolved, Some("my-custom".to_string()));
}

#[test]
fn test_resolve_cli_overrides_all() {
    let resolver = ProfileResolver::new();

    let manifest_profile = Some("claude".to_string());
    let local_profile = Some("my-custom".to_string());
    let cli_profile = Some("override".to_string());

    // CLI should override everything
    let resolved = resolver.resolve_profile_name(cli_profile, local_profile, manifest_profile);
    assert_eq!(resolved, Some("override".to_string()));
}

#[test]
fn test_resolve_none_when_empty() {
    let resolver = ProfileResolver::new();

    let resolved = resolver.resolve_profile_name(None, None, None);
    assert_eq!(resolved, None);
}
