use codanna::parsing::LanguageParser;
use codanna::parsing::kotlin::KotlinParser;

#[test]
fn test_kotlin_class_method_definitions() {
    let code = r#"
class UserService {
    fun createUser(name: String): User {
        return User()
    }

    fun deleteUser(id: Int) {
        // delete logic
    }

    private fun validateUser(user: User): Boolean {
        return true
    }
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let defines = parser.find_defines(code);

    println!("Found {} method definitions:", defines.len());
    for (definer, method, range) in &defines {
        println!(
            "  {} defines {} at line {}",
            definer, method, range.start_line
        );
    }

    let define_pairs: Vec<(String, String)> = defines
        .iter()
        .map(|(definer, method, _)| (definer.to_string(), method.to_string()))
        .collect();

    assert!(
        define_pairs.contains(&("UserService".to_string(), "createUser".to_string())),
        "Should detect createUser method in UserService"
    );
    assert!(
        define_pairs.contains(&("UserService".to_string(), "deleteUser".to_string())),
        "Should detect deleteUser method in UserService"
    );
    assert!(
        define_pairs.contains(&("UserService".to_string(), "validateUser".to_string())),
        "Should detect validateUser method in UserService"
    );
}

#[test]
fn test_kotlin_object_method_definitions() {
    let code = r#"
object DatabaseConfig {
    fun getConnectionString(): String {
        return "jdbc:postgresql://localhost"
    }

    fun getMaxConnections(): Int {
        return 10
    }
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let defines = parser.find_defines(code);

    println!("Found {} method definitions:", defines.len());
    for (definer, method, range) in &defines {
        println!(
            "  {} defines {} at line {}",
            definer, method, range.start_line
        );
    }

    let define_pairs: Vec<(String, String)> = defines
        .iter()
        .map(|(definer, method, _)| (definer.to_string(), method.to_string()))
        .collect();

    assert!(
        define_pairs.contains(&(
            "DatabaseConfig".to_string(),
            "getConnectionString".to_string()
        )),
        "Should detect getConnectionString method in DatabaseConfig object"
    );
    assert!(
        define_pairs.contains(&(
            "DatabaseConfig".to_string(),
            "getMaxConnections".to_string()
        )),
        "Should detect getMaxConnections method in DatabaseConfig object"
    );
}

#[test]
fn test_kotlin_nested_class_method_definitions() {
    let code = r#"
class OuterClass {
    fun outerMethod() {
        println("outer")
    }

    class InnerClass {
        fun innerMethod() {
            println("inner")
        }
    }
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let defines = parser.find_defines(code);

    println!("Found {} method definitions:", defines.len());
    for (definer, method, range) in &defines {
        println!(
            "  {} defines {} at line {}",
            definer, method, range.start_line
        );
    }

    let define_pairs: Vec<(String, String)> = defines
        .iter()
        .map(|(definer, method, _)| (definer.to_string(), method.to_string()))
        .collect();

    assert!(
        define_pairs.contains(&("OuterClass".to_string(), "outerMethod".to_string())),
        "Should detect outerMethod in OuterClass"
    );
    assert!(
        define_pairs.contains(&("InnerClass".to_string(), "innerMethod".to_string())),
        "Should detect innerMethod in InnerClass"
    );
}
