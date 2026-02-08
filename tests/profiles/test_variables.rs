//! Tests for variable merging

use codanna::profiles::variables::Variables;

#[test]
fn test_merge_empty() {
    let vars = Variables::new();
    let merged = vars.merge();

    assert!(merged.is_empty());
}

#[test]
fn test_merge_global_only() {
    let mut vars = Variables::new();
    vars.set_global("author", "Global Author");
    vars.set_global("license", "MIT");

    let merged = vars.merge();
    assert_eq!(merged.get("author"), Some(&"Global Author".to_string()));
    assert_eq!(merged.get("license"), Some(&"MIT".to_string()));
}

#[test]
fn test_merge_manifest_overrides_global() {
    let mut vars = Variables::new();
    vars.set_global("author", "Global Author");
    vars.set_global("license", "MIT");
    vars.set_manifest("author", "Manifest Author");

    let merged = vars.merge();
    // Manifest should override global
    assert_eq!(merged.get("author"), Some(&"Manifest Author".to_string()));
    // Global still present
    assert_eq!(merged.get("license"), Some(&"MIT".to_string()));
}

#[test]
fn test_merge_local_overrides_all() {
    let mut vars = Variables::new();
    vars.set_global("author", "Global Author");
    vars.set_manifest("author", "Manifest Author");
    vars.set_local("author", "Local Author");

    let merged = vars.merge();
    // Local should win
    assert_eq!(merged.get("author"), Some(&"Local Author".to_string()));
}

#[test]
fn test_merge_cli_overrides_all() {
    let mut vars = Variables::new();
    vars.set_global("author", "Global Author");
    vars.set_manifest("author", "Manifest Author");
    vars.set_local("author", "Local Author");
    vars.set_cli("author", "CLI Author");

    let merged = vars.merge();
    // CLI should win
    assert_eq!(merged.get("author"), Some(&"CLI Author".to_string()));
}

#[test]
fn test_merge_respects_priority() {
    let mut vars = Variables::new();
    vars.set_global("a", "global-a");
    vars.set_global("b", "global-b");
    vars.set_manifest("b", "manifest-b");
    vars.set_manifest("c", "manifest-c");
    vars.set_local("c", "local-c");
    vars.set_local("d", "local-d");
    vars.set_cli("d", "cli-d");
    vars.set_cli("e", "cli-e");

    let merged = vars.merge();
    assert_eq!(merged.get("a"), Some(&"global-a".to_string()));
    assert_eq!(merged.get("b"), Some(&"manifest-b".to_string()));
    assert_eq!(merged.get("c"), Some(&"local-c".to_string()));
    assert_eq!(merged.get("d"), Some(&"cli-d".to_string()));
    assert_eq!(merged.get("e"), Some(&"cli-e".to_string()));
}
