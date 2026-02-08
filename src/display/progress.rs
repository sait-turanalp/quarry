//! Progress tracking utilities for long-running operations.

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::time::Duration;

/// Create a styled progress bar for file processing.
pub fn create_progress_bar(total: u64, message: &str) -> ProgressBar {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb.set_message(message.to_string());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

/// Create a spinner for indeterminate progress.
pub fn create_spinner(message: &str) -> ProgressBar {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    spinner.set_message(message.to_string());
    spinner.enable_steady_tick(Duration::from_millis(80));
    spinner
}

/// Multi-threaded progress tracker for parallel operations.
pub struct ProgressTracker {
    multi: MultiProgress,
    main_bar: ProgressBar,
}

impl ProgressTracker {
    /// Create a new progress tracker with a main progress bar.
    pub fn new(total: u64, message: &str) -> Self {
        let multi = MultiProgress::new();
        let main_bar = multi.add(create_progress_bar(total, message));

        Self { multi, main_bar }
    }

    /// Add a sub-progress bar for a worker thread.
    pub fn add_worker(&self, message: &str) -> ProgressBar {
        self.multi.add(create_spinner(message))
    }

    /// Update the main progress bar.
    pub fn inc(&self, delta: u64) {
        self.main_bar.inc(delta);
    }

    /// Set the main progress bar message.
    pub fn set_message(&self, message: &str) {
        self.main_bar.set_message(message.to_string());
    }

    /// Finish the main progress bar with a message.
    pub fn finish_with_message(&self, message: &str) {
        self.main_bar.finish_with_message(message.to_string());
    }

    /// Get the multi-progress instance for custom handling.
    pub fn multi(&self) -> &MultiProgress {
        &self.multi
    }
}

/// Create a progress bar for benchmark iterations.
pub fn create_benchmark_progress(iterations: u64) -> ProgressBar {
    let pb = ProgressBar::new(iterations);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{msg} [{bar:30.yellow/blue}] {pos}/{len}")
            .unwrap()
            .progress_chars("=>-"),
    );
    pb.set_message("Running benchmark");
    pb
}

/// Helper to display a temporary spinner during an operation.
pub fn with_spinner<F, T>(message: &str, operation: F) -> T
where
    F: FnOnce() -> T,
{
    let spinner = create_spinner(message);
    let result = operation();
    spinner.finish_and_clear();
    result
}
