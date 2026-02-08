//! Configurable AI guidance system for multi-hop queries.
//!
//! Provides template-based, user-configurable guidance messages
//! that help AI assistants navigate through systematic exploration.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

pub mod config;
pub mod engine;
pub mod templates;

pub use config::GuidanceConfig;
pub use engine::GuidanceEngine;

/// Variables available for template substitution
#[derive(Debug, Clone, Serialize)]
pub struct TemplateContext {
    /// Name of the tool being executed
    pub tool: String,
    /// Query string if applicable
    pub query: Option<String>,
    /// Number of results returned
    pub result_count: usize,
    /// Whether results were found
    pub has_results: bool,
    /// Custom variables for specific tools
    pub custom: HashMap<String, String>,
}

impl TemplateContext {
    /// Create a new template context
    pub fn new(tool: &str, result_count: usize) -> Self {
        Self {
            tool: tool.to_string(),
            query: None,
            result_count,
            has_results: result_count > 0,
            custom: HashMap::new(),
        }
    }

    /// Set the query string
    pub fn with_query(mut self, query: Option<&str>) -> Self {
        self.query = query.map(String::from);
        self
    }

    /// Add a custom variable
    pub fn with_custom(mut self, key: &str, value: &str) -> Self {
        self.custom.insert(key.to_string(), value.to_string());
        self
    }
}

/// Result of guidance generation
#[derive(Debug, Clone)]
pub struct GuidanceResult {
    /// The generated guidance message
    pub message: String,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f32,
    /// Whether this is a fallback/default message
    pub is_fallback: bool,
}