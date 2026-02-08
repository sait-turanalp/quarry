use codanna::parsing::LanguageParser;
use codanna::parsing::kotlin::KotlinParser;

#[test]
fn test_kotlin_constructor_parameter_types() {
    let code = r#"
interface PgClient {
    fun query(sql: String): List<Row>
}

class ReadWritePgClient : PgClient {
    override fun query(sql: String): List<Row> {
        return emptyList()
    }
}

class AuroraCurrencyRepository(
    private val client: PgClient,
    private val readWriteClient: ReadWritePgClient,
) {
    suspend fun updateCurrencyCollections(
        id: UUID,
        collectionId: UUID,
        clearCollections: Boolean = false,
    ): CurrencyModel? {
        return null
    }
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);

    println!("Found {} type uses:", uses.len());
    for (context, used_type, range) in &uses {
        println!(
            "  {} uses {} at line {}",
            context, used_type, range.start_line
        );
    }

    // Test that constructor parameters are tracked
    let use_pairs: Vec<(String, String)> = uses
        .iter()
        .map(|(context, used_type, _)| (context.to_string(), used_type.to_string()))
        .collect();

    assert!(
        use_pairs.contains(&(
            "AuroraCurrencyRepository".to_string(),
            "PgClient".to_string()
        )),
        "Should detect PgClient usage in AuroraCurrencyRepository constructor"
    );
    assert!(
        use_pairs.contains(&(
            "AuroraCurrencyRepository".to_string(),
            "ReadWritePgClient".to_string()
        )),
        "Should detect ReadWritePgClient usage in AuroraCurrencyRepository constructor"
    );
}

#[test]
fn test_kotlin_function_parameter_types() {
    let code = r#"
class UserService {
    fun processUser(user: User, validator: UserValidator): Result {
        return Result()
    }

    fun createUser(name: String, age: Int): User {
        return User()
    }
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);

    println!("Found {} type uses:", uses.len());
    for (context, used_type, range) in &uses {
        println!(
            "  {} uses {} at line {}",
            context, used_type, range.start_line
        );
    }

    let use_pairs: Vec<(String, String)> = uses
        .iter()
        .map(|(context, used_type, _)| (context.to_string(), used_type.to_string()))
        .collect();

    // Check function parameter types (excluding primitives like String and Int)
    assert!(
        use_pairs.contains(&("processUser".to_string(), "User".to_string())),
        "Should detect User parameter type in processUser"
    );
    assert!(
        use_pairs.contains(&("processUser".to_string(), "UserValidator".to_string())),
        "Should detect UserValidator parameter type in processUser"
    );

    // Check return types
    assert!(
        use_pairs.contains(&("processUser".to_string(), "Result".to_string())),
        "Should detect Result return type in processUser"
    );
    assert!(
        use_pairs.contains(&("createUser".to_string(), "User".to_string())),
        "Should detect User return type in createUser"
    );

    // Primitives should be filtered out
    assert!(
        !use_pairs.iter().any(|(_, t)| t == "String" || t == "Int"),
        "Should filter out primitive types String and Int"
    );
}

#[test]
fn test_kotlin_property_types() {
    let code = r#"
class Application {
    private val database: Database
    val cache: CacheManager
    var logger: Logger
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let uses = parser.find_uses(code);

    println!("Found {} type uses:", uses.len());
    for (context, used_type, range) in &uses {
        println!(
            "  {} uses {} at line {}",
            context, used_type, range.start_line
        );
    }

    let use_pairs: Vec<(String, String)> = uses
        .iter()
        .map(|(context, used_type, _)| (context.to_string(), used_type.to_string()))
        .collect();

    assert!(
        use_pairs.contains(&("database".to_string(), "Database".to_string())),
        "Should detect Database property type"
    );
    assert!(
        use_pairs.contains(&("cache".to_string(), "CacheManager".to_string())),
        "Should detect CacheManager property type"
    );
    assert!(
        use_pairs.contains(&("logger".to_string(), "Logger".to_string())),
        "Should detect Logger property type"
    );
}
