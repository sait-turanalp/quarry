//! Rich terminal display utilities for enhanced CLI output.
//!
//! Provides styled tables, progress bars, and formatted output
//! for a professional command-line experience.

pub mod help;
pub mod progress;
pub mod tables;
pub mod theme;

pub use help::{create_help_text, format_command_description, format_help_section};
pub use progress::{ProgressTracker, create_progress_bar, create_spinner};
pub use tables::{TableBuilder, create_benchmark_table, create_summary_table};
pub use theme::{THEME, Theme};
