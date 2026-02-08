# Local and Remote CI Parity Guide

## Overview

Local CI scripts match GitHub Actions workflows exactly. All scripts automatically ensure your Rust toolchain matches the remote environment before running checks.

## How It Works

GitHub Actions uses the latest stable Rust toolchain (via `dtolnay/rust-toolchain@stable`). Local scripts update to the same version before running checks.

All local CI scripts run `rustup update stable` before executing checks:

1. Local Rust version matches GitHub Actions
2. Clippy lints are identical
3. Consistent behavior between local and remote

## Available Scripts

1. **`contributing/scripts/quick-check.sh`** - Fast pre-push validation (2-3 minutes)
2. **`contributing/scripts/full-test.sh`** - Complete test suite (5-10 minutes)
3. **`contributing/scripts/auto-fix.sh`** - Auto-fix formatting and clippy issues

All scripts include automatic Rust toolchain synchronization:

```bash
# Ensure we're using the latest stable Rust (matches GitHub Actions)
rustup update stable --no-self-update > /dev/null 2>&1 || true
current_version=$(rustc --version)
echo "Using: $current_version"
```

## Workflow

### Before Committing

```bash
./contributing/scripts/quick-check.sh
```

This updates Rust and runs all checks in ~2-3 minutes.

### Before Releasing

```bash
./contributing/scripts/full-test.sh
```

Runs comprehensive test suite matching GitHub Actions exactly.

### Fixing Issues

```bash
./contributing/scripts/auto-fix.sh
```

Auto-formats code, fixes clippy warnings, and verifies with quick-check.

## Verification

Check your local environment matches remote:

```bash
./contributing/scripts/quick-check.sh
rustc --version  # Should show latest stable
```

## Implementation Details

The `|| true` in `rustup update` allows scripts to continue if:
- Network is unavailable
- Rust is already up-to-date
- Update fails for any reason

Scripts report the Rust version being used and continue with checks.

## Maintenance

When GitHub Actions updates:
1. Check `.github/workflows/full-test.yml`
2. Update `contributing/scripts/full-test.sh` to match
3. Update `contributing/scripts/quick-check.sh` if needed

Keep scripts and workflows in sync.
