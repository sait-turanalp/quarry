#!/bin/bash

# Parse any file to explore its AST structure
# Uses codanna parse by default, with option for tree-sitter

FILE=$1
MODE=${2:-codanna}  # Default to codanna, can specify 'tree-sitter'

if [ -z "$FILE" ]; then
    echo "Usage: $0 <file> [codanna|tree-sitter|both]"
    echo ""
    echo "Parse any file to explore its AST structure."
    echo "Examples:"
    echo "  $0 examples/typescript/comprehensive.ts           # Use codanna (default)"
    echo "  $0 examples/typescript/comprehensive.ts codanna   # Use codanna explicitly"
    echo "  $0 examples/typescript/comprehensive.ts tree-sitter # Use tree-sitter"
    echo "  $0 examples/typescript/comprehensive.ts both      # Compare both parsers"
    echo ""
    echo "Options for codanna:"
    echo "  --all-nodes    Include anonymous nodes (punctuation, keywords)"
    echo "  --max-depth N  Limit traversal depth"
    echo ""
    echo "Note: For detailed comparison, use:"
    echo "  ./contributing/tree-sitter/scripts/compare-nodes.sh<file>"
    exit 1
fi

# If relative path, make it relative to project root
if [[ "$FILE" != /* ]]; then
    PROJECT_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
    FILE="$PROJECT_ROOT/$FILE"
fi

# Check if file exists
if [ ! -f "$FILE" ]; then
    echo "❌ File not found: $FILE"
    exit 1
fi

case "$MODE" in
    codanna)
        echo "=== Parsing with codanna (named nodes only) ==="
        cargo run --quiet -- parse "$FILE" 2>/dev/null | head -50
        echo ""
        echo "Note: Add --all-nodes flag to see all nodes including punctuation"
        ;;
    tree-sitter)
        echo "=== Parsing with tree-sitter ==="
        tree-sitter parse "$FILE"
        ;;
    both)
        echo "=== Parsing with codanna ==="
        CODANNA_COUNT=$(cargo run --quiet -- parse "$FILE" 2>/dev/null | jq -r .node | sort -u | wc -l | tr -d ' ')
        echo "Codanna found $CODANNA_COUNT unique named node types"
        echo ""
        echo "=== Parsing with tree-sitter ==="
        TS_COUNT=$(tree-sitter parse "$FILE" 2>/dev/null | grep -o '([a-z_]*' | grep -v '^$' | sort -u | wc -l | tr -d ' ')
        echo "Tree-sitter found $TS_COUNT unique named node types"
        echo ""
        echo "Match: $([[ $TS_COUNT == $CODANNA_COUNT ]] && echo "✅ Perfect match!" || echo "⚠️ Difference detected")"
        ;;
    *)
        echo "❌ Unknown mode: $MODE"
        echo "Use: codanna, tree-sitter, or both"
        exit 1
        ;;
esac