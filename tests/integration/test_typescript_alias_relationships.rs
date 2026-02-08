use codanna::parsing::typescript::parser::TypeScriptParser;
use codanna::parsing::typescript::resolution::TypeScriptResolutionContext;
use codanna::parsing::{LanguageParser, ResolutionScope, ScopeLevel};
use codanna::{FileId, Range, Symbol, SymbolId, SymbolKind, Visibility};

#[test]
fn test_typescript_alias_resolution_for_relationships() {
    println!("\n=== Testing TypeScript Alias Resolution for Relationships ===");

    // Step 1: Create a Button symbol as it exists in the index
    let button_id = SymbolId::new(42).unwrap();
    let mut button_symbol = Symbol::new(
        button_id,
        "Button",
        SymbolKind::Constant,
        FileId::new(1).unwrap(),
        Range::new(0, 0, 0, 0),
    );
    button_symbol.module_path = Some("examples.typescript.react.src.components.ui.button".into());
    button_symbol.visibility = Visibility::Public;

    println!(
        "Created symbol: name='{}', module_path={:?}",
        button_symbol.name, button_symbol.module_path
    );

    // Step 2: Parse code that imports Button using an alias
    let code = r#"
import { Button } from '@/components/ui/button';

export function MyComponent() {
    return <Button>Click me</Button>;
}
"#;

    let mut parser = TypeScriptParser::new().unwrap();

    // Extract imports
    let file_id = FileId::new(2).unwrap();
    let imports = parser.find_imports(code, file_id);
    println!("\nParser extracted {} import(s):", imports.len());
    for import in &imports {
        println!("  Path: '{}', Alias: {:?}", import.path, import.alias);
    }

    // Verify the import was extracted correctly
    assert_eq!(imports.len(), 1);
    let import = &imports[0];
    assert_eq!(import.path, "@/components/ui/button");
    assert_eq!(import.alias, Some("Button".to_string()));
    assert!(!import.is_glob);

    // Step 3: Build resolution context
    let mut context = TypeScriptResolutionContext::new(FileId::new(2).unwrap());

    // Add the Button symbol by name (current behavior)
    println!("\nAdding symbol to resolution context by name only:");
    println!("  By name: '{}'", button_symbol.name);
    context.add_symbol(
        button_symbol.name.to_string(),
        button_id,
        ScopeLevel::Module,
    );

    // Step 4: Try to resolve the import
    // After enhancement, the import path would be something like './src/components/ui/button'
    // But we need to resolve to the module path: 'examples.typescript.react.src.components.ui.button'

    let enhanced_path = "./src/components/ui/button";
    let expected_module_path = "examples.typescript.react.src.components.ui.button";

    println!("\nTrying to resolve enhanced import path: '{enhanced_path}'");
    let resolved_by_path = context.resolve(enhanced_path);
    println!("Resolution result by enhanced path: {resolved_by_path:?}");

    println!("\nTrying to resolve by module path: '{expected_module_path}'");
    let resolved_by_module = context.resolve(expected_module_path);
    println!("Resolution result by module path: {resolved_by_module:?}");

    // Without the fix, these won't resolve
    assert_eq!(
        resolved_by_path, None,
        "Should not resolve by enhanced path alone"
    );
    assert_eq!(
        resolved_by_module, None,
        "Should not resolve by module path alone"
    );

    // Step 5: THE FIX - Add symbol by its module_path too
    if let Some(module_path) = &button_symbol.module_path {
        println!("\nNow adding by module_path (THE FIX): '{module_path}'");
        context.add_symbol(module_path.to_string(), button_id, ScopeLevel::Global);
    }

    // Step 6: Now resolution should work
    println!("\nRetrying resolution after fix:");
    let resolved_after_fix = context.resolve(expected_module_path);
    println!("Resolution result after fix: {resolved_after_fix:?}");

    assert_eq!(
        resolved_after_fix,
        Some(button_id),
        "Should resolve after adding by module_path"
    );

    println!(
        "\n=== Test Complete: Module path registration is essential for cross-module resolution ==="
    );
}

#[test]
fn test_typescript_import_to_module_path_mapping() {
    println!("\n=== Testing Import Path to Module Path Mapping ===");

    // This test demonstrates the gap between:
    // 1. Enhanced import paths (e.g., "./src/components/ui/button")
    // 2. Module paths in the index (e.g., "examples.typescript.react.src.components.ui.button")

    // The enhanced import path is relative to the importing file
    let enhanced_import = "./src/components/ui/button";

    // But the symbol's module_path includes the full project structure
    let indexed_module_path = "examples.typescript.react.src.components.ui.button";

    println!("Enhanced import path: '{enhanced_import}'");
    println!("Indexed module path: '{indexed_module_path}'");

    // These don't match directly, which is why resolution fails
    assert_ne!(enhanced_import, indexed_module_path);

    // To fix this, we need to either:
    // 1. Add symbols by their module_path in addition to their name
    // 2. Transform the enhanced import path to match the module_path format
    // 3. Use a more sophisticated resolution that can handle both formats

    println!("\nThe resolution context needs to handle this mapping!");
}
