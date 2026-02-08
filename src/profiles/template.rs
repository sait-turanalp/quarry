//! Template variable substitution

use super::error::{ProfileError, ProfileResult};
use std::collections::HashMap;

/// Substitute variables in template string
///
/// Replaces {{variable}} patterns with values from the map
/// Returns error if a variable is referenced but not found
pub fn substitute_variables(
    template: &str,
    variables: &HashMap<String, String>,
) -> ProfileResult<String> {
    let mut result = template.to_string();

    // Find all {{variable}} patterns
    let pattern = regex::Regex::new(r"\{\{(\w+)\}\}").expect("Invalid regex");

    for capture in pattern.captures_iter(template) {
        let full_match = &capture[0];
        let var_name = &capture[1];

        // Look up variable value
        let value = variables
            .get(var_name)
            .ok_or_else(|| ProfileError::InvalidManifest {
                reason: format!("Variable '{var_name}' not found in context"),
            })?;

        // Replace all occurrences
        result = result.replace(full_match, value);
    }

    Ok(result)
}
