//! Tests for template substitution

use codanna::profiles::template::substitute_variables;
use std::collections::HashMap;

#[test]
fn test_substitute_single_variable() {
    let template = "Project: {{project_name}}";
    let mut vars = HashMap::new();
    vars.insert("project_name".to_string(), "MyProject".to_string());

    let result = substitute_variables(template, &vars).unwrap();
    assert_eq!(result, "Project: MyProject");
}

#[test]
fn test_substitute_multiple_variables() {
    let template = "# {{project_name}}\nAuthor: {{author}}\nLicense: {{license}}";
    let mut vars = HashMap::new();
    vars.insert("project_name".to_string(), "MyProject".to_string());
    vars.insert("author".to_string(), "John Doe".to_string());
    vars.insert("license".to_string(), "MIT".to_string());

    let result = substitute_variables(template, &vars).unwrap();
    assert_eq!(result, "# MyProject\nAuthor: John Doe\nLicense: MIT");
}

#[test]
fn test_substitute_same_variable_multiple_times() {
    let template = "{{name}} is {{name}}";
    let mut vars = HashMap::new();
    vars.insert("name".to_string(), "Alice".to_string());

    let result = substitute_variables(template, &vars).unwrap();
    assert_eq!(result, "Alice is Alice");
}

#[test]
fn test_missing_variable_error() {
    let template = "{{missing}}";
    let vars = HashMap::new();

    let result = substitute_variables(template, &vars);
    assert!(result.is_err());

    let err = result.unwrap_err().to_string();
    assert!(err.contains("missing"));
}

#[test]
fn test_no_variables() {
    let template = "Plain text with no variables";
    let vars = HashMap::new();

    let result = substitute_variables(template, &vars).unwrap();
    assert_eq!(result, "Plain text with no variables");
}

#[test]
fn test_empty_template() {
    let template = "";
    let vars = HashMap::new();

    let result = substitute_variables(template, &vars).unwrap();
    assert_eq!(result, "");
}
