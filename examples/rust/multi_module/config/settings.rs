//! Application settings and configuration management
//! 
//! This module demonstrates:
//! - Configuration structure design
//! - Environment variable integration
//! - Validation and defaults

use std::env;
use std::fmt;
use std::time::Duration;

/// Main application settings
#[derive(Debug, Clone)]
pub struct Settings {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub logging: LoggingConfig,
    pub features: FeatureFlags,
}

impl Settings {
    /// Create new settings with defaults
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Load settings from environment variables
    pub fn load_from_env_vars(&mut self) -> Result<(), ConfigError> {
        // Server configuration
        if let Ok(host) = env::var("APP_SERVER_HOST") {
            self.server.host = host;
        }
        
        if let Ok(port_str) = env::var("APP_SERVER_PORT") {
            self.server.port = port_str.parse()
                .map_err(|_| ConfigError::InvalidValue("APP_SERVER_PORT".to_string()))?;
        }
        
        // Database configuration
        if let Ok(url) = env::var("APP_DATABASE_URL") {
            self.database.url = url;
        }
        
        if let Ok(max_conn_str) = env::var("APP_DATABASE_MAX_CONNECTIONS") {
            self.database.max_connections = max_conn_str.parse()
                .map_err(|_| ConfigError::InvalidValue("APP_DATABASE_MAX_CONNECTIONS".to_string()))?;
        }
        
        // Logging configuration
        if let Ok(level) = env::var("APP_LOG_LEVEL") {
            self.logging.level = match level.to_lowercase().as_str() {
                "debug" => LogLevel::Debug,
                "info" => LogLevel::Info,
                "warn" => LogLevel::Warn,
                "error" => LogLevel::Error,
                _ => return Err(ConfigError::InvalidValue("APP_LOG_LEVEL".to_string())),
            };
        }
        
        // Feature flags
        if let Ok(value) = env::var("APP_FEATURE_METRICS") {
            self.features.enable_metrics = value.to_lowercase() == "true";
        }
        
        Ok(())
    }
    
    /// Validate all settings
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.server.validate()?;
        self.database.validate()?;
        self.logging.validate()?;
        Ok(())
    }
    
    /// Get database URL (convenience method)
    pub fn db_url(&self) -> &str {
        &self.database.url
    }
    
    /// Get server address (convenience method)
    pub fn server_address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
            logging: LoggingConfig::default(),
            features: FeatureFlags::default(),
        }
    }
}

/// Server configuration section
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
    pub max_connections: usize,
}

impl ServerConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.host.is_empty() {
            return Err(ConfigError::MissingRequired("server.host".to_string()));
        }
        if self.port == 0 {
            return Err(ConfigError::InvalidValue("server.port cannot be 0".to_string()));
        }
        Ok(())
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8080,
            timeout: Duration::from_secs(30),
            max_connections: 100,
        }
    }
}

/// Database configuration section
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub connection_timeout: Duration,
    pub query_timeout: Duration,
}

impl DatabaseConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.url.is_empty() {
            return Err(ConfigError::MissingRequired("database.url".to_string()));
        }
        if !self.url.contains("://") {
            return Err(ConfigError::InvalidValue("database.url must be a valid URL".to_string()));
        }
        Ok(())
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://app.db".to_string(),
            max_connections: 10,
            connection_timeout: Duration::from_secs(5),
            query_timeout: Duration::from_secs(30),
        }
    }
}

/// Logging configuration section
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub level: LogLevel,
    pub file_path: Option<String>,
    pub console: bool,
}

impl LoggingConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        // All logging configurations are valid by default
        Ok(())
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: LogLevel::Info,
            file_path: None,
            console: true,
        }
    }
}

/// Log level enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "DEBUG"),
            LogLevel::Info => write!(f, "INFO"),
            LogLevel::Warn => write!(f, "WARN"),
            LogLevel::Error => write!(f, "ERROR"),
        }
    }
}

/// Feature flags configuration
#[derive(Debug, Clone)]
pub struct FeatureFlags {
    pub enable_metrics: bool,
    pub enable_tracing: bool,
    pub experimental_features: bool,
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self {
            enable_metrics: false,
            enable_tracing: false,
            experimental_features: false,
        }
    }
}

/// Configuration errors
#[derive(Debug, Clone)]
pub enum ConfigError {
    MissingRequired(String),
    InvalidValue(String),
    FileNotFound(String),
    ParseError(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::MissingRequired(field) => write!(f, "Missing required configuration: {}", field),
            ConfigError::InvalidValue(msg) => write!(f, "Invalid configuration value: {}", msg),
            ConfigError::FileNotFound(path) => write!(f, "Configuration file not found: {}", path),
            ConfigError::ParseError(msg) => write!(f, "Configuration parse error: {}", msg),
        }
    }
}

impl std::error::Error for ConfigError {}

// Module-level helper functions
pub fn get_env_with_default(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_string())
}

pub fn parse_duration_from_str(duration_str: &str) -> Result<Duration, ConfigError> {
    match duration_str.parse::<u64>() {
        Ok(secs) => Ok(Duration::from_secs(secs)),
        Err(_) => Err(ConfigError::InvalidValue(format!("Invalid duration: {}", duration_str))),
    }
}

// Module-level constants for configuration keys
pub const SERVER_HOST_KEY: &str = "APP_SERVER_HOST";
pub const SERVER_PORT_KEY: &str = "APP_SERVER_PORT";
pub const DATABASE_URL_KEY: &str = "APP_DATABASE_URL";
pub const LOG_LEVEL_KEY: &str = "APP_LOG_LEVEL";

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    
    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.server.host, "localhost");
        assert_eq!(settings.server.port, 8080);
        assert_eq!(settings.database.url, "sqlite://app.db");
    }
    
    #[test]
    fn test_settings_validation() {
        let settings = Settings::default();
        assert!(settings.validate().is_ok());
        
        let mut invalid_settings = settings.clone();
        invalid_settings.server.host = String::new();
        assert!(invalid_settings.validate().is_err());
    }
    
    #[test]
    fn test_env_loading() {
        env::set_var("APP_SERVER_HOST", "example.com");
        env::set_var("APP_SERVER_PORT", "9000");
        
        let mut settings = Settings::default();
        assert!(settings.load_from_env_vars().is_ok());
        
        assert_eq!(settings.server.host, "example.com");
        assert_eq!(settings.server.port, 9000);
        
        // Cleanup
        env::remove_var("APP_SERVER_HOST");
        env::remove_var("APP_SERVER_PORT");
    }
    
    #[test]
    fn test_duration_parsing() {
        let duration = parse_duration_from_str("30").unwrap();
        assert_eq!(duration, Duration::from_secs(30));
        
        let result = parse_duration_from_str("invalid");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_log_level_display() {
        assert_eq!(LogLevel::Debug.to_string(), "DEBUG");
        assert_eq!(LogLevel::Info.to_string(), "INFO");
        assert_eq!(LogLevel::Warn.to_string(), "WARN");
        assert_eq!(LogLevel::Error.to_string(), "ERROR");
    }
}