use codanna::parsing::LanguageParser;
use codanna::parsing::kotlin::KotlinParser;

#[test]
fn test_nested_class_context() {
    let code = r#"
class Outer {
    fun outerMethod() {
        println("Outer method")
    }

    class Middle {
        fun middleMethod() {
            println("Middle method")
        }

        class Inner {
            fun innerMethod() {
                println("Inner method")
            }
        }
    }

    fun anotherOuterMethod() {
        println("Another outer method")
    }
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let defines = parser.find_defines(code);

    println!("Found {} definitions:", defines.len());
    for (definer, defined, range) in &defines {
        println!(
            "  {} defines {} at line {}",
            definer, defined, range.start_line
        );
    }

    let define_pairs: Vec<(String, String)> = defines
        .iter()
        .map(|(definer, defined, _)| (definer.to_string(), defined.to_string()))
        .collect();

    // Verify outer class methods
    assert!(
        define_pairs.contains(&("Outer".to_string(), "outerMethod".to_string())),
        "Should detect outerMethod in Outer class"
    );
    assert!(
        define_pairs.contains(&("Outer".to_string(), "anotherOuterMethod".to_string())),
        "Should detect anotherOuterMethod in Outer class after nested class - verifies context restoration"
    );

    // Verify middle class and its method
    assert!(
        define_pairs.contains(&("Middle".to_string(), "middleMethod".to_string())),
        "Should detect middleMethod in Middle class"
    );

    // Verify inner class and its method
    assert!(
        define_pairs.contains(&("Inner".to_string(), "innerMethod".to_string())),
        "Should detect innerMethod in Inner class"
    );
}

#[test]
fn test_companion_object_context() {
    let code = r#"
class MyClass {
    fun instanceMethod() {
        println("Instance method")
    }

    companion object {
        fun staticMethod() {
            println("Static-like method")
        }

        fun anotherStaticMethod() {
            println("Another static-like method")
        }
    }

    fun anotherInstanceMethod() {
        println("Another instance method")
    }
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let defines = parser.find_defines(code);

    println!("Found {} definitions:", defines.len());
    for (definer, defined, range) in &defines {
        println!(
            "  {} defines {} at line {}",
            definer, defined, range.start_line
        );
    }

    let define_pairs: Vec<(String, String)> = defines
        .iter()
        .map(|(definer, defined, _)| (definer.to_string(), defined.to_string()))
        .collect();

    // Verify instance methods
    assert!(
        define_pairs.contains(&("MyClass".to_string(), "instanceMethod".to_string())),
        "Should detect instanceMethod in MyClass"
    );
    assert!(
        define_pairs.contains(&("MyClass".to_string(), "anotherInstanceMethod".to_string())),
        "Should detect anotherInstanceMethod in MyClass after companion object - verifies context restoration"
    );

    // The critical test: verify that methods after companion object are still tracked with correct class context
    // This verifies that the save/restore pattern works - if context wasn't restored,
    // anotherInstanceMethod would be lost or have wrong parent
    assert_eq!(
        defines.len(),
        2,
        "Should find exactly 2 instance methods (companion object methods tracked separately)"
    );
}

#[test]
fn test_nested_function_context() {
    let code = r#"
class Container {
    fun outerFunction() {
        fun localFunction() {
            println("Local function inside method")
        }

        localFunction()
        println("Outer function")
    }

    class NestedClass {
        fun nestedMethod() {
            fun anotherLocalFunction() {
                println("Local function in nested class method")
            }

            anotherLocalFunction()
        }
    }

    fun afterNestedClass() {
        println("Method after nested class")
    }
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let defines = parser.find_defines(code);

    println!("Found {} definitions:", defines.len());
    for (definer, defined, range) in &defines {
        println!(
            "  {} defines {} at line {}",
            definer, defined, range.start_line
        );
    }

    let define_pairs: Vec<(String, String)> = defines
        .iter()
        .map(|(definer, defined, _)| (definer.to_string(), defined.to_string()))
        .collect();

    // Verify outer function
    assert!(
        define_pairs.contains(&("Container".to_string(), "outerFunction".to_string())),
        "Should detect outerFunction in Container class"
    );

    // Verify nested class and its method
    assert!(
        define_pairs.contains(&("NestedClass".to_string(), "nestedMethod".to_string())),
        "Should detect nestedMethod in NestedClass"
    );

    // Verify context is restored after nested class
    assert!(
        define_pairs.contains(&("Container".to_string(), "afterNestedClass".to_string())),
        "Should detect afterNestedClass in Container class after nested class - verifies context restoration"
    );

    // The critical test: verify context is restored correctly after nested class
    // If the save/restore pattern is working, afterNestedClass should be tracked with Container as parent
    // If it's broken, afterNestedClass might be lost or have NestedClass as parent
    assert_eq!(
        defines.len(),
        3,
        "Should find exactly 3 methods: outerFunction, nestedMethod, and afterNestedClass"
    );
}
