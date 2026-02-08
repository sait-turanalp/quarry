use codanna::parsing::LanguageParser;
use codanna::parsing::python::PythonParser;
use codanna::types::SymbolKind;
use codanna::types::{FileId, SymbolCounter};

#[test]
fn test_module_level_class_instantiation_detection() {
    let python_code = r#"
# Module-level class instantiation - should be detected
client = DatabaseClient()
manager = ConfigManager()

def process_data():
    """Function with class instantiation"""
    # Function-level instantiation - should be detected
    processor = DataProcessor()
    validator = InputValidator()
    return processor.process()

# Another module-level instantiation
logger = Logger()

class Application:
    def __init__(self):
        # Method-level instantiation - should be detected
        self.db = DatabaseConnection()
        self.cache = CacheManager()
"#;

    let mut parser = PythonParser::new().expect("Failed to create parser");
    let file_id = FileId::new(1).unwrap();
    let mut counter = SymbolCounter::new();

    // Parse the code
    let symbols = parser.parse(python_code, file_id, &mut counter);

    // Verify a module symbol is created for this file in the parser stage
    let module_symbol = symbols.iter().find(|s| s.kind == SymbolKind::Module);
    assert!(
        module_symbol.is_some(),
        "Module symbol should be created by the parser"
    );
    assert_eq!(module_symbol.unwrap().name.as_ref(), "<module>");

    // Get calls - this is where the bug manifests
    let calls = parser.find_calls(python_code);

    println!("Detected calls:");
    for (caller, callee, _range) in &calls {
        println!("  {caller} -> {callee}");
    }

    // Test expectations
    let call_pairs: Vec<(String, String)> = calls
        .iter()
        .map(|(caller, callee, _)| (caller.to_string(), callee.to_string()))
        .collect();

    // Function-level calls (these should work with current implementation)
    assert!(
        call_pairs.contains(&("process_data".to_string(), "DataProcessor".to_string())),
        "Should detect DataProcessor instantiation in process_data function"
    );
    assert!(
        call_pairs.contains(&("process_data".to_string(), "InputValidator".to_string())),
        "Should detect InputValidator instantiation in process_data function"
    );

    // Method-level calls (these should work with current implementation)
    assert!(
        call_pairs.contains(&("__init__".to_string(), "DatabaseConnection".to_string())),
        "Should detect DatabaseConnection instantiation in __init__ method"
    );
    assert!(
        call_pairs.contains(&("__init__".to_string(), "CacheManager".to_string())),
        "Should detect CacheManager instantiation in __init__ method"
    );

    // Module-level calls (these should now work with the fix!)
    assert!(
        call_pairs.contains(&("<module>".to_string(), "DatabaseClient".to_string())),
        "Should detect DatabaseClient instantiation at module level"
    );
    assert!(
        call_pairs.contains(&("<module>".to_string(), "ConfigManager".to_string())),
        "Should detect ConfigManager instantiation at module level"
    );
    assert!(
        call_pairs.contains(&("<module>".to_string(), "Logger".to_string())),
        "Should detect Logger instantiation at module level"
    );

    println!("\n✅ SUCCESS: Module-level class instantiations are now detected!");
    println!("Module-level detections:");
    for (caller, callee, _) in &calls {
        if caller == &"<module>" {
            println!("  <module> -> {callee}");
        }
    }
}
