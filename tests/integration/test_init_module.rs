//! Test: Init module creates global directories
//!
//! This test verifies ONE thing:
//! - init_global_dirs() creates the models directory

use codanna::init;
use std::env;
use tempfile::TempDir;

#[test]
fn test_init_creates_models_directory() {
    // Create a temporary directory for this test to avoid touching production files
    let temp_dir = TempDir::new().expect("Failed to create temp dir");

    // Override HOME to point to our temp directory during this test
    let original_home = env::var("HOME").ok();
    unsafe {
        env::set_var("HOME", temp_dir.path());
    }

    println!("Test: init_global_dirs() creates models directory");

    // Call the function we're testing
    let result = init::init_global_dirs();

    // Verify it succeeded
    assert!(result.is_ok(), "Should create directories without error");

    // Verify the models directory exists (using the actual function)
    let models_dir = init::models_dir();
    println!("Expected: {} exists", models_dir.display());
    println!(
        "Got:      {}",
        if models_dir.exists() {
            "exists"
        } else {
            "missing"
        }
    );

    assert!(models_dir.exists(), "Models directory should exist");
    assert!(models_dir.is_dir(), "Should be a directory, not a file");

    println!("Result: Models directory created successfully");

    // Restore original HOME
    unsafe {
        if let Some(home) = original_home {
            env::set_var("HOME", home);
        } else {
            env::remove_var("HOME");
        }
    }

    // TempDir will clean up automatically when dropped
}
