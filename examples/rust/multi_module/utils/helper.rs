//! Helper utilities for common operations
//! 
//! This module demonstrates:
//! - Utility function implementations
//! - Generic programming patterns
//! - Common data processing patterns

use std::collections::HashMap;
use std::fmt;

// Import from other modules for cross-module functionality
use super::ValidationError;

/// Format a message for output display
pub fn format_output(message: &str) -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    
    format!("[{}] OUTPUT: {}", timestamp, message)
}

/// Validate input data (email-like validation)
pub fn validate_input(data: &str) -> bool {
    !data.is_empty() && data.contains('@') && data.len() >= 5
}

/// Advanced input validation with detailed error reporting
pub fn validate_input_detailed(data: &str) -> Result<(), ValidationError> {
    if data.is_empty() {
        return Err(ValidationError::Empty);
    }
    
    if data.len() < 5 {
        return Err(ValidationError::TooShort(5));
    }
    
    if data.len() > 254 {
        return Err(ValidationError::TooLong(254));
    }
    
    if !data.contains('@') {
        return Err(ValidationError::InvalidFormat("email format".to_string()));
    }
    
    Ok(())
}

/// Generic data processor with configurable behavior
pub struct DataProcessor {
    config: HashMap<String, String>,
    processed_count: usize,
}

impl DataProcessor {
    /// Create a new data processor with configuration
    pub fn new(config: HashMap<String, String>) -> Self {
        Self {
            config,
            processed_count: 0,
        }
    }
    
    /// Process input data according to configuration
    pub fn process(&mut self, data: &str) -> String {
        self.processed_count += 1;
        
        let mode = self.config.get("mode").map(|s| s.as_str()).unwrap_or("standard");
        
        match mode {
            "uppercase" => data.to_uppercase(),
            "lowercase" => data.to_lowercase(),
            "reverse" => data.chars().rev().collect(),
            "trim" => data.trim().to_string(),
            _ => format!("processed({}): {}", self.processed_count, data),
        }
    }
    
    /// Process multiple items in batch
    pub fn process_batch(&mut self, items: Vec<&str>) -> Vec<String> {
        items.into_iter().map(|item| self.process(item)).collect()
    }
    
    /// Get processing statistics
    pub fn stats(&self) -> ProcessingStats {
        ProcessingStats {
            processed_count: self.processed_count,
            config_count: self.config.len(),
        }
    }
    
    /// Update processor configuration
    pub fn set_config(&mut self, key: String, value: String) {
        self.config.insert(key, value);
    }
    
    /// Get configuration value
    pub fn get_config(&self, key: &str) -> Option<&String> {
        self.config.get(key)
    }
    
    /// Reset processor state
    pub fn reset(&mut self) {
        self.processed_count = 0;
    }
    
    /// Check if processor is configured for a specific mode
    pub fn is_mode(&self, mode: &str) -> bool {
        self.config.get("mode").map_or(false, |m| m == mode)
    }
    
    // Private helper method
    fn validate_config(&self) -> bool {
        !self.config.is_empty()
    }
}

impl fmt::Debug for DataProcessor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DataProcessor")
            .field("processed_count", &self.processed_count)
            .field("config_keys", &self.config.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Clone for DataProcessor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            processed_count: 0, // Reset count on clone
        }
    }
}

/// Processing statistics
#[derive(Debug, Clone)]
pub struct ProcessingStats {
    pub processed_count: usize,
    pub config_count: usize,
}

impl ProcessingStats {
    pub fn efficiency(&self) -> f64 {
        if self.config_count == 0 {
            0.0
        } else {
            self.processed_count as f64 / self.config_count as f64
        }
    }
}

impl fmt::Display for ProcessingStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ProcessingStats(processed: {}, config_entries: {}, efficiency: {:.2})",
            self.processed_count,
            self.config_count,
            self.efficiency()
        )
    }
}

// Generic utility functions
pub fn safe_division(a: f64, b: f64) -> Option<f64> {
    if b != 0.0 {
        Some(a / b)
    } else {
        None
    }
}

pub fn retry_with_backoff<T, E, F>(
    mut operation: F,
    max_attempts: usize,
    base_delay_ms: u64,
) -> Result<T, E>
where
    F: FnMut() -> Result<T, E>,
{
    let mut attempts = 0;
    
    loop {
        attempts += 1;
        match operation() {
            Ok(result) => return Ok(result),
            Err(error) => {
                if attempts >= max_attempts {
                    return Err(error);
                }
                
                // Mock delay (in real implementation, would use sleep)
                let delay = base_delay_ms * (2_u64.pow(attempts as u32 - 1));
                println!("Retry attempt {} after {}ms delay", attempts, delay);
            }
        }
    }
}

// String utilities
pub fn truncate_string(s: &str, max_length: usize) -> String {
    if s.len() <= max_length {
        s.to_string()
    } else {
        format!("{}...", &s[..max_length.saturating_sub(3)])
    }
}

pub fn normalize_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn extract_domain_from_email(email: &str) -> Option<String> {
    email.split('@').nth(1).map(|domain| domain.to_lowercase())
}

// Collection utilities
pub fn merge_hashmaps<K, V>(mut map1: HashMap<K, V>, map2: HashMap<K, V>) -> HashMap<K, V>
where
    K: std::hash::Hash + Eq,
{
    for (key, value) in map2 {
        map1.insert(key, value);
    }
    map1
}

pub fn find_duplicates<T>(items: &[T]) -> Vec<&T>
where
    T: Eq + std::hash::Hash,
{
    let mut seen = std::collections::HashSet::new();
    let mut duplicates = Vec::new();
    
    for item in items {
        if !seen.insert(item) {
            duplicates.push(item);
        }
    }
    
    duplicates
}

// Module-level constants
pub const MAX_PROCESSING_BATCH_SIZE: usize = 1000;
pub const DEFAULT_RETRY_ATTEMPTS: usize = 3;
pub const DEFAULT_BACKOFF_MS: u64 = 100;

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_format_output() {
        let output = format_output("test message");
        assert!(output.contains("OUTPUT: test message"));
        assert!(output.starts_with('['));
    }
    
    #[test]
    fn test_validate_input() {
        assert!(validate_input("test@example.com"));
        assert!(!validate_input("invalid"));
        assert!(!validate_input(""));
        assert!(!validate_input("no"));
    }
    
    #[test]
    fn test_validate_input_detailed() {
        assert!(validate_input_detailed("test@example.com").is_ok());
        assert!(matches!(validate_input_detailed(""), Err(ValidationError::Empty)));
        assert!(matches!(validate_input_detailed("test"), Err(ValidationError::TooShort(_))));
        assert!(matches!(validate_input_detailed("test.example.com"), Err(ValidationError::InvalidFormat(_))));
    }
    
    #[test]
    fn test_data_processor() {
        let config = HashMap::from([
            ("mode".to_string(), "uppercase".to_string()),
        ]);
        let mut processor = DataProcessor::new(config);
        
        let result = processor.process("hello");
        assert_eq!(result, "HELLO");
        
        let stats = processor.stats();
        assert_eq!(stats.processed_count, 1);
    }
    
    #[test]
    fn test_data_processor_batch() {
        let config = HashMap::from([
            ("mode".to_string(), "lowercase".to_string()),
        ]);
        let mut processor = DataProcessor::new(config);
        
        let results = processor.process_batch(vec!["HELLO", "WORLD"]);
        assert_eq!(results, vec!["hello", "world"]);
    }
    
    #[test]
    fn test_string_utilities() {
        assert_eq!(truncate_string("hello world", 5), "he...");
        assert_eq!(normalize_whitespace("  hello   world  "), "hello world");
        assert_eq!(extract_domain_from_email("user@example.com"), Some("example.com".to_string()));
    }
    
    #[test]
    fn test_collection_utilities() {
        let map1 = HashMap::from([("a".to_string(), 1)]);
        let map2 = HashMap::from([("b".to_string(), 2)]);
        let merged = merge_hashmaps(map1, map2);
        assert_eq!(merged.len(), 2);
        
        let duplicates = find_duplicates(&[1, 2, 3, 2, 4, 3]);
        assert_eq!(duplicates.len(), 2);
    }
    
    #[test]
    fn test_safe_division() {
        assert_eq!(safe_division(10.0, 2.0), Some(5.0));
        assert_eq!(safe_division(10.0, 0.0), None);
    }
}