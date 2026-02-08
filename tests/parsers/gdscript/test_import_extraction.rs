use codanna::FileId;
use codanna::parsing::LanguageParser;
use codanna::parsing::gdscript::GdscriptParser;

#[test]
fn test_gdscript_extends_import_extraction() {
    let code = r#"
extends Node2D

class_name Player

func _ready():
    pass
"#;

    let mut parser = GdscriptParser::new().expect("Failed to create parser");
    let file_id = FileId::new(1).unwrap();

    let imports = parser.find_imports(code, file_id);

    println!("Found {} imports:", imports.len());
    for import in &imports {
        println!(
            "  Path: '{}', Alias: {:?}, Glob: {}",
            import.path, import.alias, import.is_glob
        );
    }

    // Should find 2 imports: extends Node2D and class_name Player
    assert_eq!(imports.len(), 2, "Should extract extends and class_name");

    // Check extends import
    let extends_import = imports.iter().find(|i| i.path == "Node2D");
    assert!(extends_import.is_some(), "Should find extends Node2D");
    assert!(!extends_import.unwrap().is_glob);

    // Check class_name import
    let class_name_import = imports.iter().find(|i| i.path == "Player");
    assert!(class_name_import.is_some(), "Should find class_name Player");
    assert!(
        class_name_import.unwrap().is_glob,
        "class_name should be marked as glob (globally visible)"
    );
}

#[test]
fn test_gdscript_preload_import_extraction() {
    let code = r#"
const Enemy = preload("res://scripts/enemy.gd")
const Weapon = preload("res://items/weapon.gd")

func spawn_enemy():
    var enemy_instance = Enemy.instance()
    add_child(enemy_instance)
"#;

    let mut parser = GdscriptParser::new().expect("Failed to create parser");
    let file_id = FileId::new(1).unwrap();

    let imports = parser.find_imports(code, file_id);

    println!("Found {} imports:", imports.len());
    for import in &imports {
        println!("  Path: '{}'", import.path);
    }

    // Should find 2 preload imports
    assert_eq!(imports.len(), 2, "Should extract both preload statements");

    // Check enemy preload
    let enemy_import = imports.iter().find(|i| i.path == "res://scripts/enemy.gd");
    assert!(
        enemy_import.is_some(),
        "Should find enemy.gd preload import"
    );

    // Check weapon preload
    let weapon_import = imports.iter().find(|i| i.path == "res://items/weapon.gd");
    assert!(
        weapon_import.is_some(),
        "Should find weapon.gd preload import"
    );
}

#[test]
fn test_gdscript_mixed_imports() {
    let code = r#"
extends CharacterBody2D

class_name Enemy

const Bullet = preload("res://projectiles/bullet.gd")

func _ready():
    pass
"#;

    let mut parser = GdscriptParser::new().expect("Failed to create parser");
    let file_id = FileId::new(1).unwrap();

    let imports = parser.find_imports(code, file_id);

    println!("Found {} imports:", imports.len());
    for import in &imports {
        println!(
            "  Path: '{}', Glob: {}, TypeOnly: {}",
            import.path, import.is_glob, import.is_type_only
        );
    }

    // Should find 3 imports: extends, class_name, and preload
    assert_eq!(
        imports.len(),
        3,
        "Should extract extends, class_name, and preload"
    );

    // Verify each import type
    assert!(
        imports.iter().any(|i| i.path == "CharacterBody2D"),
        "Should find extends CharacterBody2D"
    );
    assert!(
        imports.iter().any(|i| i.path == "Enemy" && i.is_glob),
        "Should find class_name Enemy (as glob)"
    );
    assert!(
        imports
            .iter()
            .any(|i| i.path == "res://projectiles/bullet.gd"),
        "Should find preload bullet.gd"
    );
}
