use codanna::parsing::rust::parser::RustParser;
use codanna::parsing::rust::resolution::RustResolutionContext;
use codanna::parsing::{ResolutionScope, ScopeLevel};
use codanna::{FileId, Range, Symbol, SymbolId, SymbolKind, Visibility};

#[test]
fn test_cross_module_call_resolution_step_by_step() {
    println!("\n=== Testing Cross-Module Call Resolution Step by Step ===");

    // Step 1: Create symbol as it would exist in the index
    let init_global_dirs_id = SymbolId::new(42).unwrap();
    let mut init_global_dirs_symbol = Symbol::new(
        init_global_dirs_id,
        "init_global_dirs",
        SymbolKind::Function,
        FileId::new(1).unwrap(),
        Range::new(0, 0, 0, 0),
    );
    init_global_dirs_symbol.module_path = Some("crate::init::init_global_dirs".into());
    init_global_dirs_symbol.visibility = Visibility::Public;

    println!(
        "Created symbol: name='{}', module_path={:?}",
        init_global_dirs_symbol.name, init_global_dirs_symbol.module_path
    );

    // Step 2: Parse code to extract calls
    let code = r#"
pub fn init_config_file() {
    crate::init::init_global_dirs();
}
"#;

    let mut parser = RustParser::new().unwrap();
    let calls = parser.find_calls(code);

    println!("\nParser extracted {} call(s):", calls.len());
    for (from, to, _) in &calls {
        println!("  '{from}' -> '{to}'");
    }

    // Verify parser extracts the right thing
    assert_eq!(calls.len(), 1);
    let (_from, to, _range) = &calls[0];
    assert_eq!(*to, "crate::init::init_global_dirs");

    // Step 3: Build resolution context and add symbols
    let mut context = RustResolutionContext::new(FileId::new(2).unwrap());

    // This is what build_resolution_context would do - add the symbol by name
    println!("\nAdding symbol to resolution context:");
    println!("  By name: '{}'", init_global_dirs_symbol.name);
    context.add_symbol(
        init_global_dirs_symbol.name.to_string(),
        init_global_dirs_id,
        ScopeLevel::Global,
    );

    // Step 4: Try to resolve WITHOUT the fix first
    let call_target = to;
    println!("\nResolving call target WITHOUT fix: '{call_target}'");
    let resolved_without_fix = context.resolve(call_target);
    println!("Resolution result WITHOUT fix: {resolved_without_fix:?}");

    // This should be None without the fix
    assert_eq!(
        resolved_without_fix, None,
        "Without fix, should not resolve"
    );

    // Step 5: Now add by module_path (THE FIX)
    if let Some(module_path) = &init_global_dirs_symbol.module_path {
        println!("\nNow adding by module_path (THE FIX): '{module_path}'");
        context.add_symbol(
            module_path.to_string(),
            init_global_dirs_id,
            ScopeLevel::Global,
        );
    }

    // Step 6: Try to resolve WITH the fix
    println!("\nResolving call target WITH fix: '{call_target}'");
    let resolved_with_fix = context.resolve(call_target);
    println!("Resolution result WITH fix: {resolved_with_fix:?}");

    // This SHOULD work with our fix
    assert_eq!(
        resolved_with_fix,
        Some(init_global_dirs_id),
        "With fix, should resolve '{call_target}' to SymbolId(42)"
    );

    println!("\n✅ SUCCESS: Cross-module call resolved correctly with the fix!");
}

#[test]
fn test_resolution_shows_the_problem() {
    println!("\n=== Demonstrating the Resolution Problem ===");

    let symbol_id = SymbolId::new(42).unwrap();
    let mut context = RustResolutionContext::new(FileId::new(1).unwrap());

    // Current behavior: only add by name
    context.add_symbol(
        "init_global_dirs".to_string(),
        symbol_id,
        ScopeLevel::Global,
    );

    // Parser extracts this from the call
    let call_target = "crate::init::init_global_dirs";

    // Try to resolve what the parser extracted
    let result = context.resolve(call_target);

    println!("Parser extracts: '{call_target}'");
    println!("Symbol added as: 'init_global_dirs'");
    println!("Resolution result: {result:?}");
    println!("Expected: Some(SymbolId(42))");
    println!("Actual: None");
    println!("\n❌ This is the bug - qualified paths don't resolve!");

    // But the short name works
    let result2 = context.resolve("init_global_dirs");
    println!("\nResolving just 'init_global_dirs': {result2:?} (this works)");
    assert_eq!(result2, Some(symbol_id));
}
