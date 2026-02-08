//! Guidance engine that uses configuration from settings.

use crate::config::{GuidanceConfig, GuidanceTemplate};

/// Generate guidance based on configuration
pub fn generate_guidance_from_config(
    config: &GuidanceConfig,
    tool: &str,
    _query: Option<&str>,
    result_count: usize,
) -> Option<String> {
    if !config.enabled {
        return None;
    }

    // Get template for this tool
    let template = config.templates.get(tool)?;

    // Select appropriate template based on result count
    let template_str = select_template(template, result_count)?;

    // Replace variables
    let mut result = template_str.clone();

    // Replace built-in variables
    result = result.replace("{result_count}", &result_count.to_string());
    result = result.replace("{tool}", tool);

    // Replace custom variables from config
    for (key, value) in &config.variables {
        result = result.replace(&format!("{{{key}}}"), value);
    }

    Some(result)
}

/// Select the appropriate template based on result count
fn select_template(template: &GuidanceTemplate, result_count: usize) -> Option<String> {
    // Check custom ranges first
    for range in &template.custom {
        let in_range = result_count >= range.min && range.max.is_none_or(|max| result_count <= max);
        if in_range {
            return Some(range.template.clone());
        }
    }

    // Fall back to standard templates
    match result_count {
        0 => template.no_results.clone(),
        1 => template.single_result.clone(),
        _ => template.multiple_results.clone(),
    }
}
