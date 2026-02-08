#[cfg(test)]
mod tests {
    use codanna::parsing::LanguageParser;
    use codanna::parsing::typescript::TypeScriptParser;

    #[test]
    fn test_jsx_component_usage_tracking() {
        let code = r#"
import React from 'react';
import { Button } from './button';

export function MyPage() {
  return (
    <div>
      <Button>Click me</Button>
    </div>
  );
}

export function AnotherComponent() {
  return <Button>Another</Button>;
}
"#;

        let mut parser = TypeScriptParser::new().expect("Failed to create parser");
        let uses = parser.find_uses(code);

        println!("Found {} uses:", uses.len());
        for (from, to, range) in &uses {
            println!("  {} uses {} at line {}", from, to, range.start_line);
        }

        // We expect to find JSX component usage: MyPage uses Button, AnotherComponent uses Button
        assert!(
            !uses.is_empty(),
            "Should find JSX component usage (Button used by MyPage and AnotherComponent)"
        );

        // Check that we found Button usage
        let button_uses: Vec<_> = uses
            .iter()
            .filter(|(_, component, _)| *component == "Button")
            .collect();

        assert!(
            !button_uses.is_empty(),
            "Should find at least one usage of Button component"
        );

        println!("Button is used by {} function(s)", button_uses.len());
    }

    #[test]
    fn test_jsx_ignores_lowercase_elements() {
        let code = r#"
export function Component() {
  return (
    <div>
      <span>Text</span>
    </div>
  );
}
"#;

        let mut parser = TypeScriptParser::new().expect("Failed to create parser");
        let uses = parser.find_uses(code);

        // Should NOT track lowercase HTML elements (div, span)
        let html_uses: Vec<_> = uses
            .iter()
            .filter(|(_, component, _)| component.chars().next().unwrap().is_lowercase())
            .collect();

        assert!(
            html_uses.is_empty(),
            "Should not track lowercase HTML elements, only uppercase React components"
        );
    }

    #[test]
    fn test_jsx_self_closing_components() {
        let code = r#"
export function App() {
  return <CustomComponent />;
}
"#;

        let mut parser = TypeScriptParser::new().expect("Failed to create parser");
        let uses = parser.find_uses(code);

        println!("Found {} uses:", uses.len());
        for (from, to, range) in &uses {
            println!("  {} uses {} at line {}", from, to, range.start_line);
        }

        let custom_uses: Vec<_> = uses
            .iter()
            .filter(|(_, component, _)| *component == "CustomComponent")
            .collect();

        assert_eq!(
            custom_uses.len(),
            1,
            "Should find self-closing component usage"
        );
    }
}
