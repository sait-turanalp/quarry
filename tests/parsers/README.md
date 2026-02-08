# Parser Tests

This directory contains language-specific parser and resolution tests.

## Structure

```
parsers/
├── typescript/     # TypeScript parser and resolution tests
├── c/             # C parser and resolution tests
├── cpp/           # C++ parser and resolution tests
├── python/        # Python parser and resolution tests (future)
├── go/            # Go parser and resolution tests (future)
├── php/           # PHP parser and resolution tests (future)
└── rust/          # Rust parser and resolution tests (future)
```

## Adding New Tests

1. Create your test file in the appropriate language subdirectory
2. Add the module declaration to `tests/parsers_tests.rs`

Example:
```rust
// In tests/parsers_tests.rs
#[path = "parsers/python/test_resolution.rs"]
mod test_python_resolution;
```

## Test Categories

- **Resolution Tests**: Test language-specific resolution (tsconfig, include paths, modules, etc.)
- **Parser Tests**: Test language parsing and symbol extraction
- **Behavior Tests**: Test language-specific behaviors and quirks

## Current Tests

### TypeScript
- `test_resolution_pipeline.rs` - Tests tsconfig.json path alias resolution with extends chains

### C
- `test_resolution.rs` - Tests C include path resolution and system headers

### C++
- `test_resolution.rs` - Tests C++ include path resolution, namespaces, and templates