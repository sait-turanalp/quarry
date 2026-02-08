#!/bin/bash
# Quick pre-push check - matches GitHub Actions quick-check.yml
# For full test suite, use test-codanna-local.sh
# To auto-fix issues, use auto-fix.sh

set -e

# Match GitHub Actions environment
export CARGO_TERM_COLOR=always
export RUST_BACKTRACE=1

echo "🚀 Quick CI check (matches GitHub Actions quick-check.yml)"
echo "This should complete in ~2-3 minutes"
echo ""

# Ensure we're using the latest stable Rust (matches GitHub Actions)
echo "0️⃣ Ensuring Rust toolchain is up-to-date..."
rustup update stable --no-self-update > /dev/null 2>&1 || true
current_version=$(rustc --version)
echo "   Using: $current_version"
echo ""

# Format check - should be instant
echo "1️⃣ Check formatting (not modifying files)..."
cargo fmt --all -- --check
echo "✓ Formatting check passed"

echo ""
echo "2️⃣ Clippy strict mode (all targets and features)..."
cargo clippy --all-targets --all-features -- -D warnings
echo "✓ Clippy check passed"

echo ""
echo "3️⃣ Compile check (all features)..."
cargo check --all-features
echo "✓ Compile check passed"

echo ""
echo "✅ Quick checks passed!"
echo ""
echo "💡 Tips:"
echo "   - Run './contributing/scripts/auto-fix.sh' to automatically fix formatting and clippy issues"
echo "   - Run './contributing/scripts/full-test.sh' for full test suite before release"