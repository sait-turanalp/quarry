# Tree-sitter CLI Setup for Local Development

## Purpose

Set up tree-sitter CLI for local grammar exploration and testing without affecting the production build. This enables:
- Testing new language grammars before integration
- Debugging parser issues with official tree-sitter tools
- Exploring AST structures interactively
- Comparing tree-sitter output with our parser implementation

## Quick Start

```bash
# 1. Install tree-sitter CLI (if not already installed)
cargo install tree-sitter-cli --locked

# 2. Install a grammar (one-time per language)
cd contributing/tree-sitter
./scripts/setup.sh typescript

# 3. Parse a file to see AST
tree-sitter parse examples/typescript/comprehensive.ts

# 4. Compare with our parser
.codanna/scripts/compare-nodes.sh typescript
```

## Directory Structure

```
contributing/tree-sitter/
├── README.md             # User documentation
├── grammars/            # Cloned grammar repositories (gitignored)
│   ├── tree-sitter-typescript/
│   ├── tree-sitter-python/
│   └── ...
└── scripts/
    ├── setup.sh         # Install grammars on-demand
    └── explore-ast.sh   # Helper to parse files
    └── compare-nodes.sh    # Compare tree-sitter with our parser
```

## How It Works

1. **Tree-sitter Configuration**: The setup script configures `~/.config/tree-sitter/config.json` to point to our grammars directory
2. **Grammar Naming**: Grammars must be named `tree-sitter-{language}` for tree-sitter to find them
3. **On-Demand Installation**: Install only the languages you need with `./scripts/setup.sh <language>`
4. **Automatic Language Detection**: Tree-sitter determines which grammar to use based on file extension

## Scripts

### setup.sh

Configures tree-sitter and installs grammars on-demand:

```bash
# Show installed grammars
./scripts/setup.sh

# Install specific language
./scripts/setup.sh typescript
./scripts/setup.sh python
./scripts/setup.sh rust
```

Supported languages: typescript, python, rust, go, php, c, cpp

### explore-ast.sh

Simple wrapper for parsing files:

```bash
./scripts/explore-ast.sh examples/typescript/comprehensive.ts
```

### compare-nodes.sh

Compares tree-sitter AST nodes with our parser implementation:

```bash
# From project root
./scripts/compare-nodes.sh typescript
```

This script:
1. Parses the comprehensive example with tree-sitter
2. Runs our parser tests (which regenerates audit reports)
3. Shows differences between the two parsers

## Usage Examples

### Exploring AST Structure

```bash
# Parse any file with tree-sitter
tree-sitter parse examples/typescript/comprehensive.ts

# See specific node types
tree-sitter parse examples/python/main.py | grep "class_definition"
```

### Debugging Parser Differences

```bash
# Compare node recognition
./scripts/compare-nodes.sh typescript

# Output shows:
# - Nodes tree-sitter finds that we don't handle
# - Nodes we report that tree-sitter doesn't find
```

### Testing New Languages

```bash
# Install a new grammar
./contributing/tree-sitter/scripts/setup.sh go

# Test parsing
tree-sitter parse examples/go/main.go
```

## Benefits

1. **Official Reference**: Compare our parser against the official tree-sitter implementation
2. **Node Discovery**: Find exact node names for parser implementation
3. **Grammar Updates**: Test new grammar versions before updating dependencies
4. **Debugging**: Identify parsing differences and missing node handlers
5. **Report Generation**: Running compare-nodes.sh updates audit reports with current timestamps

## Notes

- Grammars are cloned with `--depth 1` for speed
- The grammars directory is gitignored (contains large files)
- Each developer's tree-sitter config points to their local grammar directory
- No environment variables or .env files needed - uses tree-sitter's native configuration
- Running compare-nodes.sh regenerates the audit reports as a side effect

## Integration with Development Workflow

1. When implementing a new language parser:
   - Install the grammar: `./scripts/setup.sh <language>`
   - Parse examples to understand AST structure
   - Use compare-nodes.sh to verify coverage

2. When debugging parsing issues:
   - Parse the problematic file with tree-sitter
   - Compare output with our parser
   - Identify missing or incorrectly handled nodes

3. When updating parser coverage:
   - Run compare-nodes.sh to see gaps
   - Implement missing node handlers
   - Verify with updated audit reports
