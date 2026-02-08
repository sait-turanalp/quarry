//! Test file specifically for verifying import resolution
//! This file tests that imports are correctly resolved to their definitions
//!
//! EXPECTED BEHAVIOR:
//! 1. Standard library imports should resolve to std:: paths
//! 2. Aliased imports should track both original and alias names
//! 3. Internal module imports should resolve relative to this file
//! 4. Nested module imports should resolve through the hierarchy
//! 5. Super imports should resolve to parent modules
//! 6. Crate-level imports should resolve absolutely

// === STANDARD LIBRARY IMPORTS ===
// These should resolve to their std:: paths
use std::collections::HashMap;
use std::sync::Arc;

// === ALIASED IMPORTS ===
// These should track both the original path and the alias
use std::collections::HashSet as Set;
use std::sync::Mutex as Lock;

// === INTERNAL MODULE STRUCTURE ===
// Module for testing internal imports
mod helpers {
    pub fn helper_function() -> String {
        "Helper".to_string()
    }

    pub struct HelperStruct {
        pub value: i32,
    }

    impl HelperStruct {
        pub fn new(value: i32) -> Self {
            HelperStruct { value }
        }
    }

    // Nested module for testing deeper imports
    pub mod nested {
        pub fn nested_function() -> String {
            "Nested".to_string()
        }

        // Test super import (importing from parent module)
        use super::HelperStruct;

        pub fn use_parent_struct() -> HelperStruct {
            HelperStruct::new(100)
        }
    }
}

// === RELATIVE MODULE IMPORTS ===
// Import from internal module - should resolve to helpers:: within this file's context
use helpers::{helper_function, HelperStruct};
use helpers::nested::nested_function;

// === CONFLICTING NAMES TEST ===
// Another module that also has a helper_function (to test resolution picks the right one)
mod other_helpers {
    pub fn helper_function() -> i32 {
        42
    }

    // This import should NOT be confused with helpers::helper_function
    pub fn call_own_helper() -> i32 {
        helper_function()  // Calls other_helpers::helper_function
    }
}

// === CRATE-LEVEL IMPORTS (if this were a multi-file crate) ===
// These demonstrate how imports would work in a real crate
// use crate::helpers::HelperStruct;  // Would work if we needed explicit crate import

// === SELF IMPORTS ===
// Import from self (current module)
use self::other_helpers::helper_function as other_helper;

// === MAIN FUNCTION WITH IMPORT USAGE ===
fn main() {
    println!("=== Testing Import Resolution ===\n");

    // TEST 1: Standard library imports
    println!("1. Standard library imports:");
    let mut map: HashMap<String, i32> = HashMap::new();
    map.insert("key".to_string(), 42);
    println!("   HashMap created and used ✓");

    let data = Arc::new(vec![1, 2, 3]);
    println!("   Arc created and used ✓");

    // TEST 2: Aliased imports
    println!("\n2. Aliased imports:");
    let mut set: Set<i32> = Set::new();  // Using alias 'Set' for HashSet
    set.insert(1);
    println!("   HashSet used as 'Set' ✓");

    let lock = Lock::new(5);  // Using alias 'Lock' for Mutex
    println!("   Mutex used as 'Lock' ✓");

    // TEST 3: Relative module imports
    println!("\n3. Relative module imports:");
    // This should resolve to helpers::helper_function, NOT other_helpers::helper_function
    let result = helper_function();
    println!("   helper_function() returns: '{}' (expected: 'Helper') ✓", result);

    let helper = HelperStruct::new(10);
    println!("   HelperStruct::new() works with value: {} ✓", helper.value);

    // TEST 4: Nested module import
    println!("\n4. Nested module imports:");
    let nested_result = nested_function();
    println!("   nested_function() returns: '{}' (expected: 'Nested') ✓", nested_result);

    // TEST 5: Self import with alias
    println!("\n5. Self imports with alias:");
    let other_result = other_helper();
    println!("   other_helper() returns: {} (expected: 42) ✓", other_result);

    println!("\n=== All import tests completed ===");
}

// === TEST MODULE WITH SUPER IMPORTS ===
#[cfg(test)]
mod tests {
    // Import everything from parent module
    use super::*;

    #[test]
    fn test_standard_imports() {
        // HashMap should be available through super::*
        let _map: HashMap<String, i32> = HashMap::new();
        // Arc should be available
        let _arc: Arc<Vec<i32>> = Arc::new(vec![]);
    }

    #[test]
    fn test_aliased_imports() {
        // Aliases should work through super::*
        let _set: Set<i32> = Set::new();
        let _lock: Lock<i32> = Lock::new(0);
    }

    #[test]
    fn test_relative_imports() {
        // This should resolve to helpers::helper_function due to the import
        let result = helper_function();
        assert_eq!(result, "Helper", "Should call helpers::helper_function");

        // This should resolve to helpers::HelperStruct::new
        let helper = HelperStruct::new(20);
        assert_eq!(helper.value, 20);

        // Nested function should work
        let nested = nested_function();
        assert_eq!(nested, "Nested");
    }

    #[test]
    fn test_self_imports() {
        // Test the aliased import
        let result = other_helper();
        assert_eq!(result, 42, "Should call other_helpers::helper_function via alias");
    }

    // Nested test module to test deeper super imports
    mod nested_tests {
        use super::super::*;  // Import from grandparent

        #[test]
        fn test_grandparent_imports() {
            // Should still have access to helpers through grandparent
            let result = helper_function();
            assert_eq!(result, "Helper");
        }
    }
}

// === RESOLUTION EXPECTATIONS SUMMARY ===
//
// When indexed and analyzed, we expect:
//
// 1. STANDARD LIBRARY:
//    - HashMap resolves to std::collections::HashMap
//    - Arc resolves to std::sync::Arc
//
// 2. ALIASES:
//    - Set resolves to std::collections::HashSet (with alias tracked)
//    - Lock resolves to std::sync::Mutex (with alias tracked)
//
// 3. RELATIVE MODULES:
//    - helpers::helper_function resolves to
//      crate::examples::rust::import_resolution_test::helpers::helper_function
//    - helpers::HelperStruct resolves to
//      crate::examples::rust::import_resolution_test::helpers::HelperStruct
//
// 4. NESTED MODULES:
//    - helpers::nested::nested_function resolves to
//      crate::examples::rust::import_resolution_test::helpers::nested::nested_function
//
// 5. SUPER IMPORTS:
//    - super::HelperStruct (in nested module) resolves to parent's HelperStruct
//    - super::* (in tests) imports everything from parent module
//
// 6. SELF IMPORTS:
//    - self::other_helpers::helper_function resolves within current module
//
// 7. DISAMBIGUATION:
//    - helper_function() in main calls helpers::helper_function (via import)
//    - helper_function() in other_helpers calls other_helpers::helper_function (no import)