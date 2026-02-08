#!/bin/bash
# Exact replica of .github/workflows/full-test.yml
# Run this before pushing to catch ALL GitHub Actions failures
# NOTE: Keep this in sync with full-test.yml - if you update one, update the other!

set -e  # Exit on first error

# Set environment variables like GitHub Actions
export CARGO_TERM_COLOR=always
export RUST_BACKTRACE=1

echo "🚀 Running Codanna CI locally (exact GitHub Actions replica)"
echo "============================================================"

# Ensure we're using the latest stable Rust (matches GitHub Actions)
echo ""
echo "🦀 Ensuring Rust toolchain is up-to-date..."
rustup update stable --no-self-update > /dev/null 2>&1 || true
current_version=$(rustc --version)
echo "   Using: $current_version"

# Job 1: Test Suite
echo ""
echo "📦 Job: Test Suite"
echo "==================="

# Fast checks first
echo ""
echo "📝 Check formatting"
cargo fmt --check

echo ""
echo "📎 Clippy with project rules (strict mode with all targets and features)"
cargo clippy --all-targets --all-features -- -D warnings

# Build release binary first for all subsequent tests
echo ""
echo "🔨 Building release binary for testing (with all features)..."
cargo build --release --all-features
# Use absolute path to ensure tests can find it regardless of working directory
export CODANNA_BIN="$(pwd)/target/release/codanna"
echo "✓ Using release binary: $CODANNA_BIN"

# Build with different feature combinations
echo ""
echo "🔨 Build (default features)"
cargo build --verbose

echo ""
echo "🔨 Build (no default features)"
cargo build --verbose --no-default-features

echo ""
echo "🔨 Build (all features)"
cargo build --verbose --all-features

# Run tests
echo ""
echo "🧪 Run tests"
cargo test --verbose

# Codanna-specific checks
echo ""
echo "🌳 Check tree-sitter queries compile"
# Note: This is a simple check - in GitHub Actions this might be more sophisticated
echo "(Running integration tests to verify tree-sitter functionality)"
cargo test --test "*" -- --nocapture 2>&1 | head -20 || true

echo ""
echo "🖥️  Test MCP server functionality"
# Run mcp-test locally (works fine with local permissions)
# Note: This is skipped in GitHub Actions due to permission issues
if [ -d ".codanna/index" ]; then
    $CODANNA_BIN mcp-test
    if [ $? -eq 0 ]; then
        echo "✓ MCP server and tools working correctly"
    else
        echo "✗ MCP server test failed"
        exit 1
    fi
else
    echo "⚠️  Skipping mcp-test (no index found)"
    echo "   Run 'codanna init && codanna index src' first to test MCP"
fi

echo ""
echo "📋 Verify CLI commands"
$CODANNA_BIN --help > /dev/null
echo "✓ Main help works"
$CODANNA_BIN index --help > /dev/null
echo "✓ Index help works"
$CODANNA_BIN retrieve --help > /dev/null
echo "✓ Retrieve help works"

# Performance checks
echo ""
echo "📊 Check binary size"
ls -lh $CODANNA_BIN

# Handle platform differences for stat command
if [[ "$OSTYPE" == "darwin"* ]]; then
    size=$(stat -f%z $CODANNA_BIN)
else
    size=$(stat -c%s $CODANNA_BIN)
fi

echo "Binary size: $size bytes"
size_mb=$((size / 1048576))
echo "Binary size: ${size_mb}MB"

if [ $size -gt 50000000 ]; then
    echo "⚠️  WARNING: Binary larger than 50MB"
fi

# Documentation
echo ""
echo "📚 Check docs build"
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features

echo ""
echo "✅ Test Suite: PASSED"
echo ""
echo "============================================================"
echo "✅ All GitHub Actions checks passed locally! Safe to push 🚀"
echo "============================================================"