//! Configuration for the guidance system.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Configuration for AI guidance generation
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GuidanceConfig {
    /// Enable/disable guidance system globally
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Default confidence level for generated guidance
    #[serde(default = "default_confidence")]
    pub default_confidence: f32,

    /// Tool-specific guidance templates
    #[serde(default)]
    pub tools: HashMap<String, ToolGuidance>,

    /// Global template variables
    #[serde(default)]
    pub global_vars: HashMap<String, String>,
}

/// Guidance configuration for a specific tool
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolGuidance {
    /// Template for when no results are found
    pub no_results: Option<String>,

    /// Template for a single result
    pub single_result: Option<String>,

    /// Template for multiple results
    pub multiple_results: Option<String>,

    /// Custom templates based on result count ranges
    #[serde(default)]
    pub ranges: Vec<RangeTemplate>,

    /// Tool-specific variables
    #[serde(default)]
    pub variables: HashMap<String, String>,
}

/// Template for a specific result count range
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RangeTemplate {
    /// Minimum result count (inclusive)
    pub min: usize,
    /// Maximum result count (inclusive, None = unbounded)
    pub max: Option<usize>,
    /// Template to use for this range
    pub template: String,
}

impl Default for GuidanceConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_confidence: 0.8,
            tools: default_tool_templates(),
            global_vars: default_global_vars(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_confidence() -> f32 {
    0.8
}

fn default_global_vars() -> HashMap<String, String> {
    let mut vars = HashMap::new();
    vars.insert("project_name".to_string(), "codanna".to_string());
    vars
}

fn default_tool_templates() -> HashMap<String, ToolGuidance> {
    let mut tools = HashMap::new();

    // Semantic search defaults
    tools.insert("semantic_search_docs".to_string(), ToolGuidance {
        no_results: Some(
            "No results found. Try broader search terms or check if the codebase is indexed.".to_string()
        ),
        single_result: Some(
            "Found one match. Consider using 'find_symbol' or 'get_calls' to explore this symbol's relationships.".to_string()
        ),
        multiple_results: Some(
            "Found {result_count} matches. Consider using 'find_symbol' on the most relevant result for detailed analysis, or refine your search query.".to_string()
        ),
        ranges: vec![
            RangeTemplate {
                min: 10,
                max: None,
                template: "Found {result_count} matches. Consider refining your search with more specific terms or exploring the top results with 'find_symbol'.".to_string(),
            }
        ],
        variables: HashMap::new(),
    });

    // Find symbol defaults
    tools.insert("find_symbol".to_string(), ToolGuidance {
        no_results: Some(
            "Symbol not found. Use 'search_symbols' with fuzzy matching or 'semantic_search_docs' for broader search.".to_string()
        ),
        single_result: Some(
            "Symbol found with full context. Explore 'get_calls' to see what it calls, 'find_callers' to see usage, or 'analyze_impact' to understand change implications.".to_string()
        ),
        multiple_results: Some(
            "Found {result_count} symbols with that name. Review each to find the one you're looking for.".to_string()
        ),
        ranges: vec![],
        variables: HashMap::new(),
    });

    // Get calls defaults
    tools.insert("get_calls".to_string(), ToolGuidance {
        no_results: Some(
            "No function calls found. This might be a leaf function or data structure.".to_string()
        ),
        single_result: Some(
            "Found 1 function call. Use 'find_symbol' to explore this dependency.".to_string()
        ),
        multiple_results: Some(
            "Found {result_count} function calls. Consider using 'find_symbol' on key dependencies or 'analyze_impact' to trace the call chain further.".to_string()
        ),
        ranges: vec![],
        variables: HashMap::new(),
    });

    // Find callers defaults
    tools.insert("find_callers".to_string(), ToolGuidance {
        no_results: Some(
            "No callers found. This might be an entry point, unused code, or called dynamically.".to_string()
        ),
        single_result: Some(
            "Found 1 caller. Use 'find_symbol' to explore where this function is used.".to_string()
        ),
        multiple_results: Some(
            "Found {result_count} callers. Consider 'analyze_impact' for complete dependency graph or investigate specific callers with 'find_symbol'.".to_string()
        ),
        ranges: vec![],
        variables: HashMap::new(),
    });

    // Analyze impact defaults
    tools.insert("analyze_impact".to_string(), ToolGuidance {
        no_results: Some(
            "No impact detected. This symbol appears to be isolated.".to_string()
        ),
        single_result: Some(
            "Minimal impact radius. This symbol has limited dependencies.".to_string()
        ),
        multiple_results: Some(
            "Impact analysis shows {result_count} affected symbols. Focus on critical paths or use 'find_symbol' on key dependencies.".to_string()
        ),
        ranges: vec![
            RangeTemplate {
                min: 2,
                max: Some(5),
                template: "Limited impact radius with {result_count} affected symbols. This change is relatively contained.".to_string(),
            },
            RangeTemplate {
                min: 20,
                max: None,
                template: "Significant impact with {result_count} affected symbols. Consider breaking this change into smaller parts.".to_string(),
            }
        ],
        variables: HashMap::new(),
    });

    // Search symbols defaults
    tools.insert("search_symbols".to_string(), ToolGuidance {
        no_results: Some(
            "No symbols match your query. Try 'semantic_search_docs' for natural language search or adjust your pattern.".to_string()
        ),
        single_result: Some(
            "Found exactly one match. Use 'find_symbol' to get full details about this symbol.".to_string()
        ),
        multiple_results: Some(
            "Found {result_count} matching symbols. Use 'find_symbol' on specific results for full context or narrow your search with 'kind' parameter.".to_string()
        ),
        ranges: vec![],
        variables: HashMap::new(),
    });

    // Semantic search with context defaults
    tools.insert("semantic_search_with_context".to_string(), ToolGuidance {
        no_results: Some(
            "No semantic matches found. Try different phrasing or ensure documentation exists for the concepts you're searching.".to_string()
        ),
        single_result: Some(
            "Found one match with full context. Review the relationships to understand how this fits into the codebase.".to_string()
        ),
        multiple_results: Some(
            "Rich context provided for {result_count} matches. Investigate specific relationships using targeted tools like 'get_calls' or 'find_callers'.".to_string()
        ),
        ranges: vec![],
        variables: HashMap::new(),
    });

    tools
}

impl ToolGuidance {
    /// Get the appropriate template based on result count
    pub fn get_template(&self, result_count: usize) -> Option<&str> {
        // Check custom ranges first
        for range in &self.ranges {
            let in_range = result_count >= range.min &&
                range.max.map_or(true, |max| result_count <= max);
            if in_range {
                return Some(&range.template);
            }
        }

        // Fall back to standard templates
        match result_count {
            0 => self.no_results.as_deref(),
            1 => self.single_result.as_deref(),
            _ => self.multiple_results.as_deref(),
        }
    }
}