//! Type-safe counter for generating unique symbol IDs.
//!
//! This module provides a type-safe wrapper around symbol ID generation,
//! following the project's strict type safety guidelines to prevent
//! primitive obsession and ensure correct usage.

use std::num::NonZeroU32;

/// Type-safe counter for generating unique symbol IDs.
///
/// This type ensures that:
/// - Symbol IDs start at 1 (never 0)
/// - IDs are generated sequentially
/// - The counter cannot be misused as a regular integer
/// - Thread safety is not needed (parsers run single-threaded per file)
#[derive(Debug, Clone)]
pub struct SymbolCounter {
    next_id: NonZeroU32,
}

impl SymbolCounter {
    /// Creates a new counter starting at 1.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_id: NonZeroU32::new(1).expect("1 is non-zero"),
        }
    }

    /// Generates the next symbol ID and increments the counter.
    ///
    /// # Panics
    /// Panics if the counter would overflow (after 4 billion symbols).
    /// This is a theoretical limit that won't be reached in practice.
    pub fn next_id(&mut self) -> super::SymbolId {
        let current = self.next_id;

        // Increment for next call
        // Safe: we start at 1 and won't realistically overflow
        self.next_id = NonZeroU32::new(
            current
                .get()
                .checked_add(1)
                .expect("Symbol counter overflow - file has more than 4 billion symbols"),
        )
        .expect("Incremented value is non-zero");

        super::SymbolId(current.get())
    }

    /// Returns the current count of symbols generated.
    ///
    /// This is useful for statistics and progress reporting.
    #[must_use]
    pub fn current_count(&self) -> u32 {
        self.next_id.get() - 1
    }

    /// Resets the counter back to 1.
    ///
    /// Useful when starting to parse a new file.
    pub fn reset(&mut self) {
        self.next_id = NonZeroU32::new(1).expect("1 is non-zero");
    }

    /// Creates a counter starting from a specific value.
    ///
    /// # Panics
    /// Panics if `start_from` is 0.
    pub fn from_value(start_from: u32) -> Self {
        Self {
            next_id: NonZeroU32::new(start_from).expect("Counter value must be non-zero"),
        }
    }
}

impl Default for SymbolCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_counter_starts_at_one() {
        let mut counter = SymbolCounter::new();
        let first_id = counter.next_id();
        assert_eq!(first_id.0, 1);
    }

    #[test]
    fn test_symbol_counter_increments() {
        let mut counter = SymbolCounter::new();
        let id1 = counter.next_id();
        let id2 = counter.next_id();
        let id3 = counter.next_id();

        assert_eq!(id1.0, 1);
        assert_eq!(id2.0, 2);
        assert_eq!(id3.0, 3);
    }

    #[test]
    fn test_current_count() {
        let mut counter = SymbolCounter::new();
        assert_eq!(counter.current_count(), 0);

        counter.next_id();
        assert_eq!(counter.current_count(), 1);

        counter.next_id();
        counter.next_id();
        assert_eq!(counter.current_count(), 3);
    }

    #[test]
    fn test_reset() {
        let mut counter = SymbolCounter::new();
        counter.next_id();
        counter.next_id();
        counter.next_id();
        assert_eq!(counter.current_count(), 3);

        counter.reset();
        assert_eq!(counter.current_count(), 0);

        let first_after_reset = counter.next_id();
        assert_eq!(first_after_reset.0, 1);
    }

    #[test]
    fn test_default_impl() {
        let counter = SymbolCounter::default();
        assert_eq!(counter.current_count(), 0);
    }
}
