use codanna::parsing::LanguageParser;
use codanna::parsing::csharp::CSharpParser;
use codanna::types::{FileId, SymbolCounter};

#[test]
fn test_csharp_parser_basic() {
    let code = r#"
namespace TestNamespace
{
    /// <summary>
    /// A test class for demonstrating C# parsing
    /// </summary>
    public class TestClass
    {
        /// <summary>
        /// Prints a greeting message to the console
        /// </summary>
        public void TestMethod()
        {
            Console.WriteLine("Hello");
        }
    }
}
"#;

    let mut parser = CSharpParser::new().expect("Failed to create C# parser");
    let file_id = FileId::new(1).unwrap();
    let mut counter = SymbolCounter::new();

    let symbols = parser.parse(code, file_id, &mut counter);

    assert!(!symbols.is_empty(), "Should extract symbols from C# code");

    let class_symbol = symbols.iter().find(|s| &*s.name == "TestClass");
    assert!(class_symbol.is_some(), "Should find TestClass");

    let class = class_symbol.unwrap();
    assert!(
        class.doc_comment.is_some(),
        "Should extract XML documentation from TestClass"
    );
    let doc = class.doc_comment.as_ref().unwrap();
    assert!(
        doc.contains("<summary>"),
        "Documentation should contain summary tag, got: {doc}"
    );
    assert!(
        doc.contains("A test class for demonstrating C# parsing"),
        "Documentation should contain full multi-line text, got: {doc}"
    );
    assert!(
        doc.contains("</summary>"),
        "Documentation should contain closing summary tag, got: {doc}"
    );

    let method_symbol = symbols.iter().find(|s| &*s.name == "TestMethod");
    assert!(method_symbol.is_some(), "Should find TestMethod");

    let method = method_symbol.unwrap();
    assert!(
        method.doc_comment.is_some(),
        "Should extract XML documentation from TestMethod"
    );
    let method_doc = method.doc_comment.as_ref().unwrap();
    assert!(
        method_doc.contains("Prints a greeting message to the console"),
        "Method documentation should contain full text, got: {method_doc}"
    );
    assert!(
        method_doc.contains("<summary>"),
        "Documentation should at least contain summary tag"
    );
}

#[test]
fn test_csharp_namespace_tracking() {
    let code = r#"
namespace MyApp.Services
{
    /// <summary>
    /// Service for retrieving data
    /// </summary>
    public class DataService
    {
        /// <summary>
        /// Gets the data value
        /// </summary>
        /// <returns>The data as an integer</returns>
        public int GetData() { return 42; }
    }
}
"#;

    let mut parser = CSharpParser::new().expect("Failed to create parser");
    let file_id = FileId::new(1).unwrap();
    let mut counter = SymbolCounter::new();

    let symbols = parser.parse(code, file_id, &mut counter);

    let class = symbols.iter().find(|s| &*s.name == "DataService").unwrap();
    assert_eq!(class.module_path.as_deref(), Some("MyApp.Services"));
    assert!(class.doc_comment.is_some(), "Should have documentation");
}

#[test]
fn test_csharp_method_calls() {
    let code = r#"
/// <summary>
/// Performs mathematical calculations
/// </summary>
public class Calculator
{
    /// <summary>
    /// Adds two numbers together
    /// </summary>
    private int Add(int a, int b) { return a + b; }

    /// <summary>
    /// Calculates a result using addition
    /// </summary>
    public int Calculate()
    {
        return Add(1, 2);
    }
}
"#;

    let mut parser = CSharpParser::new().expect("Failed to create parser");
    let file_id = FileId::new(1).unwrap();
    let mut counter = SymbolCounter::new();

    let symbols = parser.parse(code, file_id, &mut counter);

    let calls = parser.find_calls(code);

    // Check that Calculate -> Add call is properly tracked with caller context
    let calculate_to_add = calls
        .iter()
        .find(|(from, to, _)| *from == "Calculate" && *to == "Add");
    assert!(
        calculate_to_add.is_some(),
        "Should detect 'Calculate -> Add' call with proper caller context. Found calls: {:?}",
        calls
            .iter()
            .map(|(f, t, _)| format!("{f} -> {t}"))
            .collect::<Vec<_>>()
    );

    let calc_method = symbols.iter().find(|s| &*s.name == "Calculate").unwrap();
    if let Some(doc) = &calc_method.doc_comment {
        assert!(doc.contains("<summary>"));
        assert!(doc.contains("Calculates a result using addition"));
    }
}
