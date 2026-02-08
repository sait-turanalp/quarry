use codanna::parsing::kotlin::parser::KotlinParser;
use codanna::parsing::parser::LanguageParser;
use codanna::types::{FileId, SymbolCounter, SymbolKind};

#[test]
fn test_interface_declaration() {
    let code = r#"
package com.example

/**
 * Repository interface
 */
interface Repository<T> {
    fun save(item: T): Boolean
    fun findById(id: Long): T?
    fun findAll(): List<T>
}

interface Named {
    val name: String
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let mut counter = SymbolCounter::new();
    let file_id = FileId::new(1).unwrap();

    let symbols = parser.parse(code, file_id, &mut counter);

    // Should find Repository and Named interfaces
    let interfaces: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Interface)
        .collect();

    assert_eq!(
        interfaces.len(),
        2,
        "Should find 2 interfaces, found {}",
        interfaces.len()
    );

    let repository = interfaces.iter().find(|s| s.name.as_ref() == "Repository");
    assert!(repository.is_some(), "Should find Repository interface");
    let repository = repository.unwrap();
    assert!(
        repository.doc_comment.is_some(),
        "Repository should have doc comment"
    );

    // Should find interface methods
    let methods: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Method)
        .collect();

    assert!(
        methods.len() >= 3,
        "Should find at least 3 methods in Repository interface"
    );

    let save_method = methods.iter().find(|s| s.name.as_ref() == "save");
    assert!(save_method.is_some(), "Should find save method");

    let find_all_method = methods.iter().find(|s| s.name.as_ref() == "findAll");
    assert!(find_all_method.is_some(), "Should find findAll method");
}

#[test]
fn test_enum_class_declaration() {
    let code = r#"
package com.example

/**
 * Status enum
 */
enum class Status {
    ACTIVE,
    INACTIVE,
    PENDING,
    ARCHIVED
}

/**
 * Priority enum with properties
 */
enum class Priority(val level: Int, val label: String) {
    LOW(1, "Low Priority"),
    MEDIUM(2, "Medium Priority"),
    HIGH(3, "High Priority"),
    CRITICAL(4, "Critical Priority");

    fun isHighPriority(): Boolean = this == HIGH || this == CRITICAL
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let mut counter = SymbolCounter::new();
    let file_id = FileId::new(1).unwrap();

    let symbols = parser.parse(code, file_id, &mut counter);

    // Should find Status and Priority enums
    let enums: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Enum)
        .collect();

    assert_eq!(enums.len(), 2, "Should find 2 enums, found {}", enums.len());

    let status = enums.iter().find(|s| s.name.as_ref() == "Status");
    assert!(status.is_some(), "Should find Status enum");
    let status = status.unwrap();
    assert!(
        status.doc_comment.is_some(),
        "Status should have doc comment"
    );

    let priority = enums.iter().find(|s| s.name.as_ref() == "Priority");
    assert!(priority.is_some(), "Should find Priority enum");

    // Should find enum entries (constants)
    let constants: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Constant)
        .collect();

    assert!(
        constants.len() >= 8,
        "Should find at least 8 enum entries (4 for Status + 4 for Priority)"
    );

    // Check Status entries
    let active = constants.iter().find(|s| s.name.as_ref() == "ACTIVE");
    assert!(active.is_some(), "Should find ACTIVE entry");

    let inactive = constants.iter().find(|s| s.name.as_ref() == "INACTIVE");
    assert!(inactive.is_some(), "Should find INACTIVE entry");

    // Check Priority entries
    let low = constants.iter().find(|s| s.name.as_ref() == "LOW");
    assert!(low.is_some(), "Should find LOW entry");

    let critical = constants.iter().find(|s| s.name.as_ref() == "CRITICAL");
    assert!(critical.is_some(), "Should find CRITICAL entry");

    // Should find enum method
    let methods: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Method)
        .collect();

    assert!(
        !methods.is_empty(),
        "Should find at least 1 method in Priority enum"
    );

    let is_high_priority = methods.iter().find(|s| s.name.as_ref() == "isHighPriority");
    assert!(
        is_high_priority.is_some(),
        "Should find isHighPriority method"
    );
}

#[test]
fn test_interface_implementation() {
    let code = r#"
interface Printer {
    fun print(message: String)
}

class ConsolePrinter : Printer {
    override fun print(message: String) {
        println(message)
    }
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let mut counter = SymbolCounter::new();
    let file_id = FileId::new(1).unwrap();

    let symbols = parser.parse(code, file_id, &mut counter);

    // Should find the interface
    let interfaces: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Interface)
        .collect();

    assert_eq!(interfaces.len(), 1, "Should find 1 interface");

    // Should find the implementing class
    let classes: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Class)
        .collect();

    assert_eq!(classes.len(), 1, "Should find 1 class");

    let console_printer = classes.iter().find(|s| s.name.as_ref() == "ConsolePrinter");
    assert!(
        console_printer.is_some(),
        "Should find ConsolePrinter class"
    );
}

#[test]
fn test_multiple_interfaces() {
    let code = r#"
interface Named {
    val name: String
}

interface Auditable {
    fun audit(): String
}

class UserRepository : Repository<User>, Named, Auditable {
    override val name: String = "UserRepository"

    override fun audit(): String = "Audited"
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let mut counter = SymbolCounter::new();
    let file_id = FileId::new(1).unwrap();

    let symbols = parser.parse(code, file_id, &mut counter);

    // Should find the interfaces
    let interfaces: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Interface)
        .collect();

    assert_eq!(interfaces.len(), 2, "Should find 2 interfaces");

    // Should find Named interface
    let named = interfaces.iter().find(|s| s.name.as_ref() == "Named");
    assert!(named.is_some(), "Should find Named interface");

    // Should find Auditable interface
    let auditable = interfaces.iter().find(|s| s.name.as_ref() == "Auditable");
    assert!(auditable.is_some(), "Should find Auditable interface");

    // Should find the implementing class
    let classes: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Class)
        .collect();

    assert_eq!(classes.len(), 1, "Should find 1 class");
}

#[test]
fn test_enum_with_companion_object() {
    let code = r#"
enum class Priority(val level: Int) {
    LOW(1),
    HIGH(3);

    companion object {
        fun fromLevel(level: Int): Priority? {
            return values().find { it.level == level }
        }
    }
}
"#;

    let mut parser = KotlinParser::new().expect("Failed to create parser");
    let mut counter = SymbolCounter::new();
    let file_id = FileId::new(1).unwrap();

    let symbols = parser.parse(code, file_id, &mut counter);

    // Should find the enum
    let enums: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Enum)
        .collect();

    assert_eq!(enums.len(), 1, "Should find 1 enum");

    // Should find enum entries
    let constants: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Constant)
        .collect();

    assert_eq!(constants.len(), 2, "Should find 2 enum entries");
}
