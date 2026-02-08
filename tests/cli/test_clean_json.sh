#!/bin/bash
# Test that all retrieve commands output clean JSON without debug output

echo "=== Testing Clean JSON Output ==="
echo

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Test function
test_json() {
    local cmd="$1"
    local desc="$2"
    
    echo -n "Testing $desc... "
    
    # Run command and capture both stdout and stderr
    OUTPUT=$(./target/release/codanna $cmd --json 2>&1)
    
    # Check if stderr contains any output (debug messages would go here)
    STDERR=$(./target/release/codanna $cmd --json 2>&1 1>/dev/null)
    
    # Check if output is valid JSON
    if echo "$OUTPUT" | jq '.' > /dev/null 2>&1; then
        # Check if there's any stderr output
        if [ -z "$STDERR" ]; then
            echo -e "${GREEN}✓${NC} Clean JSON, no debug output"
        else
            echo -e "${RED}✗${NC} Has stderr output: $STDERR"
        fi
    else
        echo -e "${RED}✗${NC} Invalid JSON or has debug output"
        echo "  Output: $OUTPUT"
    fi
}

# Test each retrieve command
test_json "retrieve symbol main" "symbol (exists)"
test_json "retrieve symbol nonexistent" "symbol (not found)"
test_json "retrieve callers new" "callers"
test_json "retrieve calls main" "calls"
test_json "retrieve describe OutputManager" "describe"
test_json "retrieve implementations Parser" "implementations"
test_json "retrieve search parse" "search"

echo
echo "=== Testing Piping ==="
echo

# Test piping between commands
echo -n "Testing pipe chain... "
PIPE_RESULT=$(./target/release/codanna retrieve symbol main --json 2>/dev/null | \
    jq -r '.data.items[0].symbol.name' 2>/dev/null | \
    xargs -I {} ./target/release/codanna retrieve callers {} --json 2>/dev/null | \
    jq '.data.count' 2>/dev/null)

if [ -n "$PIPE_RESULT" ]; then
    echo -e "${GREEN}✓${NC} Pipe chain works (found $PIPE_RESULT callers)"
else
    echo -e "${RED}✗${NC} Pipe chain failed"
fi

echo
echo "=== Summary ==="
echo "All commands should output clean JSON to stdout only."
echo "Errors should go to stderr and not interfere with JSON parsing."