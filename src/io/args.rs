//! Argument parsing utilities for Unix-style key:value format

use std::collections::HashMap;

/// Parse positional arguments into optional first positional and key:value pairs
///
/// Supports both formats:
/// - `["query text", "limit:10", "kind:function"]`
/// - `["query:\"search text\"", "limit:10"]`
///
/// Handles quoted values that may be split by shell:
/// - `["query:\"error", "handling\""]` gets reconstructed to `query:"error handling"`
///
/// # Returns
/// - First positional argument (if not a key:value pair)
/// - HashMap of key:value pairs
///
/// # Example
/// ```
/// use codanna::io::args::parse_positional_args;
///
/// let args = vec!["unified output".to_string(), "limit:3".to_string()];
/// let (query, params) = parse_positional_args(&args);
/// assert_eq!(query, Some("unified output".to_string()));
/// assert_eq!(params.get("limit"), Some(&"3".to_string()));
/// ```
pub fn parse_positional_args(args: &[String]) -> (Option<String>, HashMap<String, String>) {
    if args.is_empty() {
        return (None, HashMap::new());
    }

    let mut params = HashMap::new();
    let mut first_positional = None;
    let mut i = 0;

    while i < args.len() {
        let arg = &args[i];

        if let Some((key, value)) = arg.split_once(':') {
            // This is a key:value pair
            let final_value = if value.starts_with('"') && !value.ends_with('"') {
                // Opening quote but no closing quote - value was split by shell
                // Reconstruct the full value
                let mut full_value = value.to_string();
                i += 1;

                while i < args.len() {
                    let next_part = &args[i];
                    full_value.push(' ');
                    full_value.push_str(next_part);

                    if next_part.ends_with('"') {
                        // Found the closing quote
                        break;
                    }
                    i += 1;
                }

                // Remove surrounding quotes if present
                if full_value.starts_with('"') && full_value.ends_with('"') {
                    full_value[1..full_value.len() - 1].to_string()
                } else {
                    full_value
                }
            } else if value.starts_with('"') && value.ends_with('"') && value.len() > 1 {
                // Complete quoted value - remove quotes
                value[1..value.len() - 1].to_string()
            } else {
                // No quotes or complete value
                value.to_string()
            };

            params.insert(key.to_string(), final_value);
        } else if first_positional.is_none() {
            // First non-key:value argument becomes the positional argument
            first_positional = Some(arg.clone());
        } else {
            // Additional positional arguments - could warn here
            eprintln!("Warning: Ignoring extra positional argument: {arg}");
        }

        i += 1;
    }

    (first_positional, params)
}

/// Extract a required string parameter or use positional argument
pub fn get_required_string(
    positional: Option<String>,
    params: &HashMap<String, String>,
    key: &str,
    error_msg: &str,
) -> Result<String, String> {
    positional
        .or_else(|| params.get(key).cloned())
        .ok_or_else(|| error_msg.to_string())
}

/// Extract an optional usize parameter with default
pub fn get_usize_param(params: &HashMap<String, String>, key: &str, default: usize) -> usize {
    params
        .get(key)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(default)
}

/// Extract an optional string parameter
pub fn get_string_param(params: &HashMap<String, String>, key: &str) -> Option<String> {
    params.get(key).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_positional() {
        let args = vec!["search_term".to_string()];
        let (pos, params) = parse_positional_args(&args);
        assert_eq!(pos, Some("search_term".to_string()));
        assert!(params.is_empty());
    }

    #[test]
    fn test_parse_key_value_pairs() {
        let args = vec!["limit:10".to_string(), "kind:function".to_string()];
        let (pos, params) = parse_positional_args(&args);
        assert_eq!(pos, None);
        assert_eq!(params.get("limit"), Some(&"10".to_string()));
        assert_eq!(params.get("kind"), Some(&"function".to_string()));
    }

    #[test]
    fn test_parse_mixed() {
        let args = vec!["main".to_string(), "limit:5".to_string()];
        let (pos, params) = parse_positional_args(&args);
        assert_eq!(pos, Some("main".to_string()));
        assert_eq!(params.get("limit"), Some(&"5".to_string()));
    }

    #[test]
    fn test_parse_quoted_value() {
        let args = vec!["query:\"test value\"".to_string()];
        let (pos, params) = parse_positional_args(&args);
        assert_eq!(pos, None);
        assert_eq!(params.get("query"), Some(&"test value".to_string()));
    }

    #[test]
    fn test_parse_split_quoted_value() {
        let args = vec![
            "query:\"error".to_string(),
            "handling".to_string(),
            "in".to_string(),
            "parser\"".to_string(),
            "limit:3".to_string(),
        ];
        let (pos, params) = parse_positional_args(&args);
        assert_eq!(pos, None);
        assert_eq!(
            params.get("query"),
            Some(&"error handling in parser".to_string())
        );
        assert_eq!(params.get("limit"), Some(&"3".to_string()));
    }
}
