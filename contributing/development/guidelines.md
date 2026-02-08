# Code Guidelines

This document provides Rust development guidelines for this project, derived from implementation experience and common patterns. Following these guidelines helps ensure code quality, performance, and maintainability.

## Understanding Architectural Boundaries

### Universal vs Language-Specific Concepts

When determining where code belongs, consider whether the concept is universal or language-specific:

**Universal concepts** (belong in base traits):
- Module paths and qualified names (all languages have some form of namespacing)
- Symbol visibility (public/private/protected)
- Import resolution (all languages import/include code)
- Scope levels (local, module, package, global)

**Language-specific** (belong in language implementations):
- Syntax details (`::` vs `.` vs `/` as separators)
- Resolution order (see language-specific examples below)
- Unique features (Rust lifetimes, Python MRO, TypeScript type space)
- Language-specific keywords (`self`, `super`, `crate`)

### Language Resolution Orders (Examples)

**Rust**: local → imported → module → crate
**Python (LEGB)**: Local → Enclosing → Global → Built-in
**TypeScript**: local → function → module → global (+ type space)
**Go**: local → package → universe → imports
**PHP**: local → class → namespace → global → superglobals

## 1. Function Signatures: Zero-Cost Abstractions

This is a critical principle for performance. The goal is to maximize caller flexibility and eliminate unnecessary memory allocations.

### Parameters
- Use borrowed types for read-only data: `&str` over `String`, `&[T]` over `Vec<T>`
- Use owned types only when you need to store or transform the data
- Prefer `impl Trait` over heap-allocated trait objects (`Box<dyn Trait>`)

### Return Values
- Return iterators (`impl Iterator`) when it avoids allocation AND callers vary in their needs
- Return concrete types (`Vec<T>`) if callers always collect anyway
- Use `Cow<'_, str>` for conditional ownership scenarios

```rust
// ✅ CORRECT: Zero allocation, flexible for caller
fn parse_config(input: &str) -> Result<Config, Error> { ... }
fn find_symbols<'a>(code: &'a str) -> impl Iterator<Item = &'a str> { ... }

// ❌ INCORRECT: Forces allocation or ownership transfer
fn parse_config(input: String) -> Result<Config, Error> { ... }
fn find_symbols(code: &str) -> Vec<String> { ... }

// ✅ CORRECT: Iterator that avoids allocation in the common case
fn extract_words(text: &str) -> impl Iterator<Item = &str> {
    text.split_whitespace()
        .filter(|w| !w.is_empty())
}

// ❌ INCORRECT: Returns iterator when result is always collected
fn get_all_ids(&self) -> impl Iterator<Item = UserId> {
    // Bad: Caller always does .collect(), should return Vec<UserId>
}
```

## 2. Performance: Measure, Don't Guess

Performance optimizations should be justified with measurements. Different rules apply to different code paths.

### Hot Path Rules (>1000 calls/second)
- Avoid memory allocation - use iterators and borrowing
- Avoid `.clone()` unless measured and justified
- Use `&[T]` over `Vec<T>`, `&str` over `String`
- Pre-allocate collections when size is known: `Vec::with_capacity()`

### Setup/Configuration Code
- May allocate memory for clarity
- Prioritize readability over micro-performance
- Still avoid unnecessary allocations

### Performance Targets (Project-Specific)
- Indexing: Target 10,000+ files/second
- Search latency: Target <10ms for semantic search
- Memory: ~100 bytes per symbol
- Vector operations: Target <1μs per vector access

```rust
// ✅ CORRECT: Hot path with zero allocations
fn calculate_similarity(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

// ✅ CORRECT: Setup code prioritizes clarity
fn load_config() -> Config {
    let paths = vec![
        home_dir().join(".config/app.toml"),
        PathBuf::from("/etc/app.toml"),
    ];
    // Allocation is fine here - runs once at startup
}
```

## 3. Type Safety: Avoid Primitive Obsession

Create newtype wrappers for domain-specific concepts. Raw primitives for IDs, scores, or domain values should be wrapped for type safety.

### Recommended Newtypes
- IDs: `UserId(NonZeroU32)`, `ClusterId(NonZeroU32)`, `VectorId(NonZeroU32)`
- Domain values: `Score(f32)`, `Distance(f32)`, `Confidence(f32)`
- File paths with special meaning: `IndexPath(PathBuf)`, `ConfigPath(PathBuf)`

### Type Safety Guidelines
- Use `NonZeroU32` for IDs that cannot be zero
- Validate constraints in newtype constructors
- Make invalid states unrepresentable at compile time when practical

```rust
// ✅ CORRECT: Type-safe, self-documenting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClusterId(NonZeroU32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RelevanceScore(f32);

impl RelevanceScore {
    pub fn new(score: f32) -> Result<Self, ValidationError> {
        if (0.0..=1.0).contains(&score) {
            Ok(Self(score))
        } else {
            Err(ValidationError::ScoreOutOfRange { score })
        }
    }
}

// ❌ INCORRECT: Primitive obsession, error-prone
fn calculate_relevance(query: &str, doc: &str) -> f32 { ... }
fn get_cluster(id: u32) -> Option<Cluster> { ... }
```

## 4. Error Handling: Structured and Actionable

### Library Code Guidelines
- Use `thiserror` for error types
- Include "Suggestion:" in error messages when helpful
- Provide actionable recovery steps
- Avoid `anyhow` in library code (use it in binaries)

### Application Code Guidelines
- Use `anyhow` at the binary level for convenience
- Add context when crossing module boundaries
- Use `Result<T, E>` - avoid `panic!` or `unwrap()`
- Use `expect()` only for impossible states with clear messages

```rust
#[derive(Error, Debug)]
pub enum VectorError {
    #[error("Vector dimension mismatch: expected {expected}, got {actual}\nSuggestion: Ensure all vectors are generated with the same embedding model (dimension {expected})")]
    DimensionMismatch { expected: usize, actual: usize },
    
    #[error("No vectors found in cluster {0}\nSuggestion: Check if clustering has been performed or if minimum cluster size is too high")]
    EmptyCluster(ClusterId),
    
    #[error("Search returned no results for query: {query}\nSuggestion: Try broader search terms or check if the index is properly built")]
    NoResults { query: String },
}
```

## 5. Function Design: Single Responsibility

### Composition Over Size
- Decompose complex operations into focused, composable helper methods
- Extract distinct logical operations into named functions for clarity
- Split functions that handle multiple responsibilities

### Complexity Guidelines
- Avoid more than 2 levels of nesting
- Don't mix different responsibilities (parsing + validation + transformation)
- Extract complex conditions into named predicates

```rust
// ✅ CORRECT: Each function has one clear responsibility
pub fn process_file(path: &Path) -> Result<Symbols, ProcessError> {
    let content = read_file(path)?;          // I/O responsibility
    let tokens = tokenize(&content)?;        // Lexing responsibility
    let ast = parse_tokens(&tokens)?;        // Parsing responsibility
    let symbols = extract_symbols(&ast)?;    // Extraction responsibility
    validate_symbols(&symbols)?;             // Validation responsibility
    Ok(symbols)
}

// ❌ INCORRECT: Mixed responsibilities, too long
pub fn process_file(path: &Path) -> Result<Symbols, Error> {
    let content = std::fs::read_to_string(path)?;
    let mut tokens = Vec::new();
    let mut chars = content.chars();
    while let Some(ch) = chars.next() {
        // 50+ lines of mixed tokenizing, parsing, validating...
    }
}
```

## 6. API Design: Ergonomics

### Builder Pattern
- Use builder pattern for structs with ≥3 constructor parameters
- Make builders infallible until `build()` is called
- Provide sensible defaults via `Default` trait

### Standard Traits
- Derive `Debug` on public types (exception: types containing secrets)
- Implement `Clone` where logical (not for resources like file handles)
- Implement `PartialEq`/`Eq` for types used as keys
- Add `#[must_use]` to validation methods and builder finishers

### Method Naming
- Use `into_*` for methods that consume `self`
- Use `as_*` for methods that borrow `self`
- Use `to_*` for methods that clone/allocate
- Use `with_*` for builder methods

```rust
// ✅ CORRECT: Ergonomic builder with proper traits
#[derive(Debug, Clone)]
pub struct VectorIndex { ... }

#[derive(Debug, Default)]
pub struct VectorIndexBuilder { ... }

impl VectorIndexBuilder {
    pub fn with_dimensions(mut self, dims: usize) -> Self {
        self.dimensions = Some(dims);
        self
    }
    
    #[must_use = "Building the index may fail, check the Result"]
    pub fn build(self) -> Result<VectorIndex, BuildError> {
        // Validation happens here, not in individual setters
    }
}
```

## 7. Code Quality Standards

### Clippy Compliance
- Fix warnings from `cargo clippy -- -W clippy::all`
- Address clippy lints before merging
- Consider enabling additional lints for common mistakes

### Documentation
- Document public APIs with examples
- Include panic conditions in doc comments
- Document performance characteristics for algorithms

### Testing
- Follow @tests/TEST_TEMPLATE.md structure
- Test error conditions, not just happy paths
- Include performance tests for critical paths

## 8. Integration Patterns (Project-Specific)

### Working with DocumentIndex
- Use batch operations when indexing multiple files
- Handle transaction rollback properly
- Warm caches after bulk operations

### Vector Search Integration
- Use `VectorUpdateCoordinator` for incremental updates
- Detect symbol-level changes before re-embedding
- Maintain consistency between text and vector indices

```rust
// ✅ CORRECT: Proper integration with existing systems
impl VectorSearchEngine {
    pub fn index_file(&self, path: &Path) -> Result<(), IndexError> {
        let symbols = self.extract_symbols(path)?;
        let changes = self.change_detector.detect_changes(&symbols)?;
        
        // Only re-embed actually changed symbols
        for changed in changes.modified {
            let embedding = self.generate_embedding(&changed)?;
            self.update_vector(changed.id, embedding)?;
        }
        
        Ok(())
    }
}
```

## 9. Development Workflow

### Progress Tracking
- Use TodoWrite tool for task tracking in complex features
- Update progress before moving to next task
- Break large tasks into trackable subtasks

### Quality Reviews
- Pass quality-reviewer agent checks before integration
- Address critical issues before proceeding
- Explain any guideline violations with clear justification

## Enforcement

These guidelines are enforced through:
1. Automated clippy checks in CI
2. Quality reviewer agent validation
3. Code review requirements
4. Performance benchmarks

Guidelines should be followed, with deviations explained when necessary.
