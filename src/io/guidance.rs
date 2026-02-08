//! AI assistant guidance generation for multi-hop queries.
//!
//! Generates contextual hints to guide AI assistants through
//! systematic codebase exploration using multiple tool calls.

use crate::symbol::Symbol;
use crate::types::SymbolKind;

/// Generate guidance message based on tool and results.
pub fn generate_guidance(tool: &str, _query: Option<&str>, result_count: usize) -> Option<String> {
    match tool {
        "semantic_search_docs" => {
            if result_count == 0 {
                Some("No results found. Try broader search terms or check if the codebase is indexed.".to_string())
            } else if result_count == 1 {
                Some("Found one match. Consider using 'find_symbol' or 'get_calls' to explore this symbol's relationships.".to_string())
            } else {
                Some(format!(
                    "Found {result_count} matches. Consider using 'find_symbol' on the most relevant result for detailed analysis, or refine your search query."
                ))
            }
        }

        "semantic_search_chunks" => {
            if result_count == 0 {
                Some("No code chunks found. Try broader terms, module names, or known identifiers.".to_string())
            } else {
                Some(format!(
                    "Found {result_count} chunks. Inspect line ranges, then pivot to 'find_symbol' for exact API navigation."
                ))
            }
        }

        "find_symbol" => {
            if result_count == 0 {
                Some("Symbol not found. Use 'search_symbols' with fuzzy matching or 'semantic_search_docs' for broader search.".to_string())
            } else {
                Some("Symbol found with full context. Explore 'get_calls' to see what it calls, 'find_callers' to see usage, or 'analyze_impact' to understand change implications.".to_string())
            }
        }

        "get_calls" => {
            if result_count == 0 {
                Some(
                    "No function calls found. This might be a leaf function or data structure."
                        .to_string(),
                )
            } else {
                Some(format!(
                    "Found {result_count} function calls. Consider using 'find_symbol' on key dependencies or 'analyze_impact' to trace the call chain further."
                ))
            }
        }

        "find_callers" => {
            if result_count == 0 {
                Some("No callers found. This might be an entry point, unused code, or called dynamically.".to_string())
            } else {
                Some(format!(
                    "Found {result_count} callers. Consider 'analyze_impact' for complete dependency graph or investigate specific callers with 'find_symbol'."
                ))
            }
        }

        "analyze_impact" => {
            if result_count < 3 {
                Some("Limited impact radius. This symbol is relatively isolated.".to_string())
            } else {
                Some(format!(
                    "Impact analysis shows {result_count} affected symbols. Focus on critical paths or use 'find_symbol' on key dependencies."
                ))
            }
        }

        "search_symbols" => {
            if result_count == 0 {
                Some("No symbols match your query. Try 'semantic_search_docs' for natural language search or adjust your pattern.".to_string())
            } else {
                Some(format!(
                    "Found {result_count} matching symbols. Use 'find_symbol' on specific results for full context or narrow your search with 'kind' parameter."
                ))
            }
        }

        "semantic_search_with_context" => {
            if result_count == 0 {
                Some("No semantic matches found. Try different phrasing or ensure documentation exists for the concepts you're searching.".to_string())
            } else {
                Some("Rich context provided. Investigate specific relationships using targeted tools like 'get_calls' or 'find_callers'.".to_string())
            }
        }

        _ => None,
    }
}

/// Generate guidance for chained operations.
pub fn suggest_next_action(current_tool: &str, symbols: &[Symbol]) -> Option<String> {
    if symbols.is_empty() {
        return None;
    }

    // Analyze the symbols to suggest intelligent next steps
    let has_trait = symbols.iter().any(|s| s.kind == SymbolKind::Trait);
    let has_function = symbols
        .iter()
        .any(|s| s.kind == SymbolKind::Function || s.kind == SymbolKind::Method);
    let has_struct = symbols
        .iter()
        .any(|s| s.kind == SymbolKind::Struct || s.kind == SymbolKind::Class);

    match current_tool {
        "semantic_search_docs" if has_trait => {
            Some("Trait found. Use 'retrieve implementations' to find implementing types.".to_string())
        }
        "semantic_search_docs" if has_function => {
            Some("Function found. Use 'find_callers' to understand usage patterns or 'get_calls' for dependencies.".to_string())
        }
        "semantic_search_docs" if has_struct => {
            Some("Struct/Class found. Use 'search_symbols' with module filter to find related methods.".to_string())
        }
        _ => None,
    }
}
