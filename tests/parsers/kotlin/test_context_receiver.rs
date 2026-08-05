//! Kotlin context receiver function extraction tests

use quarry::parsing::LanguageParser;
use quarry::parsing::kotlin::KotlinParser;
use quarry::types::SymbolCounter;
use quarry::{FileId, SymbolKind};

fn parse_kotlin(code: &str) -> Vec<quarry::Symbol> {
    let mut parser = KotlinParser::new().expect("Failed to create Kotlin parser");
    let mut counter = SymbolCounter::new();
    parser.parse(code, FileId(1), &mut counter)
}

#[test]
fn test_context_receiver_function_extracted() {
    // Pattern: context(View, Database) fun save() { }
    // Grammar parses as infix_expression, not function_declaration
    // We should still extract 'save' as a function
    let code = r#"context(View, Database) fun save() { }"#;
    let symbols = parse_kotlin(code);

    eprintln!("Symbols found:");
    for sym in &symbols {
        eprintln!("  {} ({:?})", sym.name, sym.kind);
    }

    let save_fn = symbols.iter().find(|s| s.name.as_ref() == "save");
    if save_fn.is_none() {
        eprintln!("ERROR: 'save' function not found in symbols");
    }
    assert!(save_fn.is_some());
    assert_eq!(save_fn.unwrap().kind, SymbolKind::Function);
}
