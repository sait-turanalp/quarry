//! Configuration module - handles application settings and environment
//! 
//! This module demonstrates:
//! - Configuration management patterns
//! - Environment variable handling
//! - Default value strategies

pub mod settings;

// Re-export main configuration types
pub use settings::{Settings, DatabaseConfig, ServerConfig, ConfigError};

// Module-level constants
pub const DEFAULT_CONFIG_FILE: &str = "app.toml";
pub const ENV_PREFIX: &str = "APP_";

// Type aliases for configuration
pub type ConfigResult<T> = Result<T, ConfigError>;

// Module-level helper functions
pub fn load_from_file(path: &str) -> ConfigResult<Settings> {
    println!("Loading configuration from: {}", path);
    // In a real implementation, this would parse a config file
    Ok(Settings::default())
}

pub fn load_from_env() -> ConfigResult<Settings> {
    println!("Loading configuration from environment variables");
    let mut settings = Settings::default();
    settings.load_from_env_vars()?;
    Ok(settings)
}