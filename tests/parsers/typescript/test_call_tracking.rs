#[cfg(test)]
mod tests {
    use codanna::parsing::LanguageParser;
    use codanna::parsing::typescript::TypeScriptParser;

    #[test]
    fn test_typescript_call_tracking() {
        let code = r#"
function test() {
    console.log('hello');
    otherFunction();
}

const arrow = () => {
    console.log('arrow');
    helperFunction();
};

class MyClass {
    method() {
        console.log('method');
        this.otherMethod();
    }
}
"#;

        let mut parser = TypeScriptParser::new().expect("Failed to create parser");
        let calls = parser.find_calls(code);

        println!("Found {} calls:", calls.len());
        for (caller, called, range) in &calls {
            println!("  {} -> {} at line {}", caller, called, range.start_line);
        }

        // We expect to find console.log and function calls
        assert!(
            !calls.is_empty(),
            "Should find at least some function calls"
        );
    }
}
