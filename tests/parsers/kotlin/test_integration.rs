use codanna::parsing::LanguageParser;
use codanna::parsing::kotlin::KotlinParser;

#[test]
fn test_kotlin_readwritepgclient_example() {
    // This is the real-world example from the user that wasn't being tracked
    let code = r#"
interface PgClient {
    fun query(sql: String): List<Row>
}

class ReadWritePgClient : PgClient {
    override fun query(sql: String): List<Row> {
        return emptyList()
    }

    fun execute(sql: String, params: Tuple, preference: QueryPreference): List<Row> {
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
        return readWriteClient.execute(
            UPDATE_CURRENCY_COLLECTIONS,
            id,
            collectionId,
            clearCollections,
        )
    }
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");

    // Test type usage tracking
    let uses = parser.find_uses(code);

    println!("\n=== Type Usage ===");
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

    // This is the key fix - constructor parameter types should now be tracked
    assert!(
        use_pairs.contains(&(
            "AuroraCurrencyRepository".to_string(),
            "PgClient".to_string()
        )),
        "Should detect PgClient usage in AuroraCurrencyRepository constructor - THIS WAS THE BUG!"
    );
    assert!(
        use_pairs.contains(&(
            "AuroraCurrencyRepository".to_string(),
            "ReadWritePgClient".to_string()
        )),
        "Should detect ReadWritePgClient usage in AuroraCurrencyRepository constructor - THIS WAS THE BUG!"
    );

    // Test method definitions
    let defines = parser.find_defines(code);

    println!("\n=== Method Definitions ===");
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
        define_pairs.contains(&("ReadWritePgClient".to_string(), "query".to_string())),
        "Should detect query method in ReadWritePgClient"
    );
    assert!(
        define_pairs.contains(&("ReadWritePgClient".to_string(), "execute".to_string())),
        "Should detect execute method in ReadWritePgClient"
    );
    assert!(
        define_pairs.contains(&(
            "AuroraCurrencyRepository".to_string(),
            "updateCurrencyCollections".to_string()
        )),
        "Should detect updateCurrencyCollections method in AuroraCurrencyRepository"
    );

    // Test extends tracking
    let extends = parser.find_extends(code);

    println!("\n=== Inheritance ===");
    for (derived, base, range) in &extends {
        println!(
            "  {} extends {} at line {}",
            derived, base, range.start_line
        );
    }

    let extend_pairs: Vec<(String, String)> = extends
        .iter()
        .map(|(derived, base, _)| (derived.to_string(), base.to_string()))
        .collect();

    assert!(
        extend_pairs.contains(&("ReadWritePgClient".to_string(), "PgClient".to_string())),
        "Should detect ReadWritePgClient extends PgClient"
    );
}

#[test]
fn test_kotlin_data_class_with_types() {
    let code = r#"
data class UserProfile(
    val user: User,
    val settings: UserSettings,
    val permissions: PermissionSet,
)

class User
class UserSettings
class PermissionSet
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

    // Data class constructor parameters should be tracked
    assert!(
        use_pairs.contains(&("UserProfile".to_string(), "User".to_string())),
        "Should detect User usage in UserProfile data class"
    );
    assert!(
        use_pairs.contains(&("UserProfile".to_string(), "UserSettings".to_string())),
        "Should detect UserSettings usage in UserProfile data class"
    );
    assert!(
        use_pairs.contains(&("UserProfile".to_string(), "PermissionSet".to_string())),
        "Should detect PermissionSet usage in UserProfile data class"
    );
}
