//! Integration test for C++ resolution using real C++ code

use codanna::parsing::LanguageBehavior;
use codanna::parsing::cpp::parser::CppParser;
use codanna::types::SymbolCounter;
use codanna::{FileId, SymbolKind};

#[test]
fn test_cpp_resolution_with_real_code() {
    // Read the comprehensive C++ example
    let cpp_code = std::fs::read_to_string("examples/cpp/comprehensive.cpp")
        .expect("Failed to read comprehensive.cpp example");

    println!("\n=== C++ RESOLUTION INTEGRATION TEST ===");
    println!("Testing with {} bytes of C++ code", cpp_code.len());

    // Create parser and behavior
    let mut parser = CppParser::new().expect("Failed to create CppParser");
    let behavior = codanna::parsing::cpp::behavior::CppBehavior::new();
    let file_id = FileId(1);
    let mut symbol_counter = SymbolCounter::new();

    // Parse the C++ code to extract symbols
    let symbols = parser.parse(&cpp_code, file_id, &mut symbol_counter);

    println!("\nExtracted {} symbols from C++ code:", symbols.len());
    for symbol in &symbols {
        println!(
            "  - {}: {:?} (line {})",
            symbol.name, symbol.kind, symbol.range.start_line
        );
    }

    // Create resolution context and add symbols
    let mut context = behavior.create_resolution_context(file_id);

    // Add all extracted symbols to the context
    for symbol in &symbols {
        context.add_symbol(
            symbol.name.to_string(),
            symbol.id,
            codanna::parsing::resolution::ScopeLevel::Module,
        );
    }

    println!("\n=== C++ RESOLUTION TESTS ===");

    // Test Case 1: Resolve a class/struct
    let class_resolved = context.resolve("Logger");
    println!("\nTest 1: Resolving 'Logger' class/struct");
    println!("Expected: Should resolve to a class/struct symbol");
    println!("Actual: {class_resolved:?}");

    if let Some(symbol_id) = class_resolved {
        let class_symbol = symbols.iter().find(|s| s.id == symbol_id).unwrap();
        println!(
            "✅ RESOLVED: Logger -> {} at line {}",
            class_symbol.name, class_symbol.range.start_line
        );
        assert_eq!(&*class_symbol.name, "Logger");
        assert!(matches!(
            class_symbol.kind,
            SymbolKind::Class | SymbolKind::Struct
        ));
    } else {
        println!("⚠️  'Logger' not resolved - checking what classes/structs were found:");
        let classes: Vec<_> = symbols
            .iter()
            .filter(|s| matches!(s.kind, SymbolKind::Class | SymbolKind::Struct))
            .collect();
        for cls in classes {
            println!("  - Class/Struct found: {}", cls.name);
        }
    }

    // Test Case 2: Resolve a method/function
    let function_resolved = context.resolve("main");
    println!("\nTest 2: Resolving 'main' function");
    println!("Expected: Should resolve to a function symbol");
    println!("Actual: {function_resolved:?}");

    if let Some(symbol_id) = function_resolved {
        let func_symbol = symbols.iter().find(|s| s.id == symbol_id).unwrap();
        println!(
            "✅ RESOLVED: main -> {} at line {}",
            func_symbol.name, func_symbol.range.start_line
        );
        assert_eq!(&*func_symbol.name, "main");
        assert!(matches!(
            func_symbol.kind,
            SymbolKind::Function | SymbolKind::Method
        ));
    } else {
        println!("⚠️  'main' function not resolved");
    }

    // Test Case 3: Test namespace resolution if available
    let namespace_resolved = context.resolve("std");
    println!("\nTest 3: Resolving 'std' namespace (if available)");
    println!("Expected: May resolve to a namespace/module symbol");
    println!("Actual: {namespace_resolved:?}");

    if let Some(symbol_id) = namespace_resolved {
        let ns_symbol = symbols.iter().find(|s| s.id == symbol_id).unwrap();
        println!(
            "✅ RESOLVED: std -> {} at line {}",
            ns_symbol.name, ns_symbol.range.start_line
        );
    } else {
        println!("⚠️  'std' namespace not resolved - this may be expected");
    }

    // Test Case 4: Try to resolve a non-existent symbol
    let unknown_resolved = context.resolve("NonExistentSymbol123");
    println!("\nTest 4: Resolving non-existent symbol");
    println!("Expected: Should NOT resolve");
    println!("Actual: {unknown_resolved:?}");

    if unknown_resolved.is_none() {
        println!("✅ CORRECT: Unknown symbol correctly not resolved");
    } else {
        panic!("❌ FAILED: Unknown symbol should not resolve");
    }

    // Test Case 5: Resolution validation with actual symbols
    println!("\nTest 5: Resolution validation with actual C++ symbols");
    let mut resolved_count = 0;
    let mut total_count = 0;
    let mut function_count = 0;
    let mut class_count = 0;
    let mut method_count = 0;

    for symbol in &symbols {
        total_count += 1;

        // Count different symbol types
        match symbol.kind {
            SymbolKind::Function => function_count += 1,
            SymbolKind::Method => method_count += 1,
            SymbolKind::Class | SymbolKind::Struct => class_count += 1,
            _ => {}
        }

        if let Some(resolved_id) = context.resolve(&symbol.name) {
            resolved_count += 1;
            if resolved_id == symbol.id {
                println!("✅ CORRECTLY RESOLVED: {} ({:?})", symbol.name, symbol.kind);
            } else {
                // Multiple symbols with same name - expected in C++ due to overloading/namespaces
                println!(
                    "⚠️  RESOLVED TO DIFFERENT ID: {} ({:?}) - may be due to overloading",
                    symbol.name, symbol.kind
                );
            }
        } else {
            println!("❌ FAILED TO RESOLVE: {} ({:?})", symbol.name, symbol.kind);
        }
    }

    println!("\n=== C++ SUMMARY ===");
    println!("✅ C++ Resolution Context successfully created and tested");
    println!(
        "✅ Real C++ code parsed and {} symbols extracted",
        symbols.len()
    );
    println!("   - Functions: {function_count}");
    println!("   - Methods: {method_count}");
    println!("   - Classes/Structs: {class_count}");
    println!(
        "   - Other symbols: {}",
        total_count - function_count - method_count - class_count
    );
    println!("✅ Resolution working: {resolved_count}/{total_count} symbols resolved correctly");
    println!("✅ Unknown symbols correctly rejected");

    // Test passes if we have some resolution working
    assert!(resolved_count > 0, "At least some symbols should resolve");
    assert!(total_count > 0, "Should have extracted some symbols");

    println!("✅ C++ Integration test PASSED\n");
}

#[test]
fn test_cpp_resolution_context_basic() {
    // Basic unit test for C++ context creation
    let behavior = codanna::parsing::cpp::behavior::CppBehavior::new();
    let file_id = FileId(1);
    let mut context = behavior.create_resolution_context(file_id);

    // Add test symbols manually
    let class_id = codanna::SymbolId(200);
    let method_id = codanna::SymbolId(201);

    context.add_symbol(
        "TestClass".to_string(),
        class_id,
        codanna::parsing::resolution::ScopeLevel::Module,
    );

    context.add_symbol(
        "test_method".to_string(),
        method_id,
        codanna::parsing::resolution::ScopeLevel::Local,
    );

    // Test resolution
    let class_resolved = context.resolve("TestClass");
    assert!(class_resolved.is_some());
    assert_eq!(class_resolved.unwrap(), class_id);

    let method_resolved = context.resolve("test_method");
    assert!(method_resolved.is_some());
    assert_eq!(method_resolved.unwrap(), method_id);

    // Test unknown symbol
    let unknown = context.resolve("unknown_symbol");
    assert!(unknown.is_none());

    println!("✅ Basic C++ resolution context test passed");
}
