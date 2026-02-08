//! Consistent color theme and styling for terminal output.

use console::Style;
use owo_colors::OwoColorize;
use std::sync::LazyLock;

/// Global theme instance for consistent styling across the application.
pub static THEME: LazyLock<Theme> = LazyLock::new(Theme::default);

/// Color theme for terminal output.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Success/positive indicators
    pub success: Style,
    /// Error/failure indicators
    pub error: Style,
    /// Warning/caution indicators
    pub warning: Style,
    /// Informational text
    pub info: Style,
    /// Headers and titles
    pub header: Style,
    /// Emphasized text
    pub emphasis: Style,
    /// Dimmed/secondary text
    pub dim: Style,
    /// File paths
    pub path: Style,
    /// Numbers and metrics
    pub number: Style,
    /// Code/symbol names
    pub code: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            success: Style::new().green().bright(),
            error: Style::new().red().bright(),
            warning: Style::new().yellow().bright(),
            info: Style::new().blue().bright(),
            header: Style::new().cyan().bold(),
            emphasis: Style::new().bold(),
            dim: Style::new().dim(),
            path: Style::new().magenta(),
            number: Style::new().cyan(),
            code: Style::new().yellow(),
        }
    }
}

impl Theme {
    /// Format a success message with checkmark.
    pub fn success_with_icon(&self, text: &str) -> String {
        if Self::should_disable_colors() {
            format!("✓ {text}")
        } else {
            format!("{} {}", "✓".green(), self.success.apply_to(text))
        }
    }

    /// Format an error message with X mark.
    pub fn error_with_icon(&self, text: &str) -> String {
        if Self::should_disable_colors() {
            format!("✗ {text}")
        } else {
            format!("{} {}", "✗".red(), self.error.apply_to(text))
        }
    }

    /// Format a warning message with warning sign.
    pub fn warning_with_icon(&self, text: &str) -> String {
        if Self::should_disable_colors() {
            format!("⚠ {text}")
        } else {
            format!("{} {}", "⚠".yellow(), self.warning.apply_to(text))
        }
    }

    /// Check if color output should be disabled.
    pub fn should_disable_colors() -> bool {
        use is_terminal::IsTerminal;
        std::env::var("NO_COLOR").is_ok() || !std::io::stdout().is_terminal()
    }

    /// Apply theme styling conditionally based on terminal support.
    pub fn apply<T: std::fmt::Display>(&self, style: &Style, text: T) -> String {
        if Self::should_disable_colors() {
            text.to_string()
        } else {
            style.apply_to(text).to_string()
        }
    }
}
