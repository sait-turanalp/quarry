//! Common utilities shared across modules.

use chrono::Utc;

/// Get current UTC timestamp in seconds since UNIX_EPOCH.
///
/// Uses chrono for accurate cross-platform timestamp.
pub fn get_utc_timestamp() -> u64 {
    Utc::now().timestamp() as u64
}

/// Split a camelCase, PascalCase, snake_case or kebab-case identifier into words.
///
/// `getHTTPResponse` -> `["get", "HTTP", "Response"]`, `get_user_by_id` -> the four parts.
/// Returns a single-element vector when the name has no internal boundary.
pub fn split_identifier(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current_word = String::new();
    let mut prev_was_upper = false;

    let mut chars = name.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '_' || ch == '-' {
            // snake_case or kebab-case separator
            if !current_word.is_empty() {
                words.push(std::mem::take(&mut current_word));
            }
            prev_was_upper = false;
        } else if ch.is_uppercase() {
            // Boundary either at lower->upper (`getUser`) or at the tail of an
            // acronym run followed by a word (`XMLParser` -> `XML` + `Parser`).
            let starts_word = chars.peek().is_some_and(|next| next.is_lowercase());
            if !current_word.is_empty() && (!prev_was_upper || starts_word) {
                words.push(std::mem::take(&mut current_word));
            }
            current_word.push(ch);
            prev_was_upper = true;
        } else {
            current_word.push(ch);
            prev_was_upper = false;
        }
    }

    if !current_word.is_empty() {
        words.push(current_word);
    }

    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_utc_timestamp() {
        let ts = get_utc_timestamp();
        // Should be a reasonable Unix timestamp (after 2020)
        assert!(ts > 1577836800, "Timestamp should be after 2020-01-01");
    }

    #[test]
    fn test_split_identifier() {
        assert_eq!(
            split_identifier("get_user_by_id"),
            ["get", "user", "by", "id"]
        );
        assert_eq!(
            split_identifier("getHTTPResponse"),
            ["get", "HTTP", "Response"]
        );
        assert_eq!(split_identifier("XMLParser"), ["XML", "Parser"]);
        assert_eq!(split_identifier("simple"), ["simple"]);
    }

    #[test]
    fn test_expand_identifiers_keeps_original_and_adds_parts() {
        let out = expand_identifiers("fn get_user_by_id(conn)");
        assert!(out.starts_with("fn get_user_by_id(conn)"));
        assert!(out.contains("user"));
        assert!(out.contains("id"));
        // single-word tokens must not be duplicated
        assert_eq!(expand_identifiers("plain words here"), "plain words here");
    }
}
