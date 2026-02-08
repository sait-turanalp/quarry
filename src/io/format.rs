//! Format definitions for CLI input/output.
//!
//! Provides structured format types for consistent JSON responses
//! compatible with tool integration and future JSON-RPC support.

use crate::error::IndexError;
use crate::io::exit_code::ExitCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};

/// Output format for CLI commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Human-readable text (default)
    Text,
    /// JSON for tool integration
    Json,
    // Future: Yaml, Xml, etc.
}

impl OutputFormat {
    /// Create format from JSON flag.
    #[must_use]
    pub fn from_json_flag(json: bool) -> Self {
        if json { Self::Json } else { Self::Text }
    }

    /// Check if format is JSON.
    #[must_use]
    pub fn is_json(&self) -> bool {
        matches!(self, Self::Json)
    }
}

/// Standard JSON response format.
///
/// Compatible with JSON-RPC 2.0 structure for future tool integration.
/// Provides consistent structure for both success and error responses.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonResponse<T = serde_json::Value>
where
    T: Serialize,
{
    /// Status: "success" or "error"
    pub status: String,

    /// Result code (e.g., "OK", "NOT_FOUND", "PARSE_ERROR")
    pub code: String,

    /// Human-readable message
    pub message: String,

    /// System guidance for AI assistants (suggests next action)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_message: Option<String>,

    /// Actual data payload (only for success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,

    /// Error details and suggestions (only for errors)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorDetails>,

    /// Exit code for shell scripts
    pub exit_code: u8,

    /// Metadata (execution time, version, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<ResponseMeta>,
}

/// Error details for JSON responses.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorDetails {
    /// Recovery suggestions
    pub suggestions: Vec<String>,
    /// Additional error context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Response metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct ResponseMeta {
    /// Version of the tool
    pub version: String,
    /// Timestamp of the response
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    /// Execution time in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<u64>,
}

impl<T> JsonResponse<T>
where
    T: Serialize,
{
    /// Create a success response with data.
    pub fn success(data: T) -> Self {
        Self {
            status: "success".to_string(),
            code: "OK".to_string(),
            message: "Operation completed successfully".to_string(),
            system_message: None,
            data: Some(data),
            error: None,
            exit_code: ExitCode::Success as u8,
            meta: None,
        }
    }

    /// Add metadata to the response.
    pub fn with_meta(mut self, meta: ResponseMeta) -> Self {
        self.meta = Some(meta);
        self
    }

    /// Add system guidance message for AI assistants.
    pub fn with_system_message(mut self, message: &str) -> Self {
        self.system_message = Some(message.to_string());
        self
    }
}

impl JsonResponse<serde_json::Value> {
    /// Create a not found response.
    pub fn not_found(entity: &str, name: &str) -> Self {
        Self {
            status: "error".to_string(),
            code: "NOT_FOUND".to_string(),
            message: format!("{entity} '{name}' not found"),
            system_message: None,
            data: None,
            error: Some(ErrorDetails {
                suggestions: vec![
                    "Check the spelling".to_string(),
                    "Ensure the index is up to date".to_string(),
                ],
                context: None,
            }),
            exit_code: ExitCode::NotFound as u8,
            meta: None,
        }
    }

    /// Create a generic error response.
    pub fn error(code: ExitCode, message: &str, suggestions: Vec<&str>) -> Self {
        Self {
            status: "error".to_string(),
            code: format!("{code:?}").to_uppercase(),
            message: message.to_string(),
            system_message: None,
            data: None,
            error: Some(ErrorDetails {
                suggestions: suggestions.iter().map(|s| s.to_string()).collect(),
                context: None,
            }),
            exit_code: code as u8,
            meta: None,
        }
    }

    /// Create an error response from IndexError.
    pub fn from_error(error: &IndexError) -> Self {
        Self {
            status: "error".to_string(),
            code: error.status_code(),
            message: error.to_string(),
            system_message: None,
            data: None,
            error: Some(ErrorDetails {
                suggestions: error
                    .recovery_suggestions()
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                context: None,
            }),
            exit_code: ExitCode::from_error(error) as u8,
            meta: None,
        }
    }
}

/// Format current time as UTC timestamp string.
///
/// Returns a string in the format "YYYY-MM-DD HH:MM:SS UTC".
/// This is used for report generation and audit timestamps.
///
/// # Example
/// ```
/// use codanna::io::format::format_utc_timestamp;
///
/// let timestamp = format_utc_timestamp();
/// // Returns something like "2025-09-28 15:30:45 UTC"
/// ```
pub fn format_utc_timestamp() -> String {
    // Use chrono for accurate cross-platform date/time formatting
    let now = Utc::now();
    now.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_format_from_flag() {
        assert_eq!(OutputFormat::from_json_flag(true), OutputFormat::Json);
        assert_eq!(OutputFormat::from_json_flag(false), OutputFormat::Text);
    }

    #[test]
    fn test_json_response_success() {
        #[derive(Serialize)]
        struct TestData {
            name: String,
            value: i32,
        }

        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };

        let response = JsonResponse::success(data);
        assert_eq!(response.status, "success");
        assert_eq!(response.code, "OK");
        assert_eq!(response.exit_code, 0);
        assert!(response.data.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_json_response_not_found() {
        let response = JsonResponse::not_found("Symbol", "main");
        assert_eq!(response.status, "error");
        assert_eq!(response.code, "NOT_FOUND");
        assert_eq!(response.exit_code, 3);
        assert!(response.data.is_none());
        assert!(response.error.is_some());
    }
}
