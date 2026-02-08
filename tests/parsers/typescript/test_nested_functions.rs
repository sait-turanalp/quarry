#[cfg(test)]
mod tests {
    use codanna::parsing::LanguageParser;
    use codanna::parsing::typescript::TypeScriptParser;
    use codanna::types::{FileId, SymbolCounter};

    #[test]
    fn test_nested_function_extraction() {
        // Test that nested functions are properly extracted as symbols
        // This was the critical fix in Sprint 3 for React component support
        let code = r#"
// React component pattern with nested functions
const Component = () => {
    const handleClick = () => {
        console.log('clicked');
        toggleTheme();
    };
    
    const toggleTheme = () => {
        console.log('theme');
    };
    
    return { handleClick, toggleTheme };
};

// Regular function with nested function
function outer() {
    function inner() {
        console.log('inner');
    }
    inner();
}
"#;

        let mut parser = TypeScriptParser::new().expect("Failed to create parser");

        // Check symbol extraction
        let file_id = FileId::new(1).unwrap();
        let mut counter = SymbolCounter::new();
        let symbols = parser.parse(code, file_id, &mut counter);

        // Verify nested functions are extracted
        let symbol_names: Vec<&str> = symbols.iter().map(|s| s.name.as_ref()).collect();

        assert_eq!(
            symbols.len(),
            5,
            "Expected 5 symbols: Component, handleClick, toggleTheme, outer, inner"
        );
        assert!(symbol_names.contains(&"Component"), "Missing Component");
        assert!(
            symbol_names.contains(&"handleClick"),
            "Missing nested handleClick"
        );
        assert!(
            symbol_names.contains(&"toggleTheme"),
            "Missing nested toggleTheme"
        );
        assert!(symbol_names.contains(&"outer"), "Missing outer");
        assert!(symbol_names.contains(&"inner"), "Missing nested inner");
    }

    #[test]
    fn test_nested_function_relationships() {
        // Test that relationships between nested functions are tracked
        let code = r#"
const App = () => {
    const doWork = () => {
        helperFunction();
    };
    
    const helperFunction = () => {
        console.log('helping');
    };
    
    doWork();
};
"#;

        let mut parser = TypeScriptParser::new().expect("Failed to create parser");

        // Check call tracking
        let calls = parser.find_calls(code);

        // Find the doWork -> helperFunction call
        let has_nested_call = calls
            .iter()
            .any(|(caller, callee, _)| *caller == "doWork" && *callee == "helperFunction");

        assert!(
            has_nested_call,
            "Should track doWork -> helperFunction call"
        );

        // Also check App calls doWork
        let has_parent_call = calls
            .iter()
            .any(|(caller, callee, _)| *caller == "App" && *callee == "doWork");

        assert!(has_parent_call, "Should track App -> doWork call");
    }
}
