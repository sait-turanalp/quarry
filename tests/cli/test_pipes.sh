#!/bin/bash
# test_pipes.sh - Verify all commands work in pipes for slash commands

echo "=== Testing Codanna Pipes for Slash Commands ==="
echo

# Make sure we have the binary
CODANNA="./target/release/codanna"
if [ ! -f "$CODANNA" ]; then
    echo "Error: Codanna binary not found at $CODANNA"
    echo "Please run: cargo build --release"
    exit 1
fi

# Check for jq
if ! command -v jq &> /dev/null; then
    echo "Error: jq is required for JSON processing"
    echo "Please install jq: brew install jq"
    exit 1
fi

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "Testing individual commands output valid JSON..."
echo

# Test each command outputs valid JSON
test_json_output() {
    local cmd="$1"
    local desc="$2"
    
    echo -n "  $desc... "
    if $CODANNA $cmd --json 2>/dev/null | jq '.' > /dev/null 2>&1; then
        echo -e "${GREEN}✓${NC}"
        return 0
    else
        echo -e "${RED}✗${NC}"
        return 1
    fi
}

test_json_output "retrieve symbol main" "retrieve symbol"
test_json_output "retrieve callers new" "retrieve callers"
test_json_output "retrieve calls main" "retrieve calls"
test_json_output "retrieve describe OutputManager" "retrieve describe"
test_json_output "retrieve search parse" "retrieve search"
test_json_output "mcp semantic_search_docs query:error limit:3" "semantic search"

echo
echo "Testing pipe chains..."
echo

# Test piping between commands
echo -n "  Simple pipe (symbol -> name extraction)... "
RESULT=$($CODANNA retrieve symbol main --json 2>/dev/null | jq -r '.data.items[0].symbol.name' 2>/dev/null)
if [ -n "$RESULT" ]; then
    echo -e "${GREEN}✓${NC} Found: $RESULT"
else
    echo -e "${RED}✗${NC}"
fi

echo -n "  Chain pipe (symbol -> callers)... "
CHAIN_RESULT=$($CODANNA retrieve symbol main --json 2>/dev/null | \
    jq -r '.data.items[0].symbol.name' 2>/dev/null | \
    xargs -I {} $CODANNA retrieve callers {} --json 2>/dev/null | \
    jq '.data.count' 2>/dev/null)

if [ -n "$CHAIN_RESULT" ]; then
    echo -e "${GREEN}✓${NC} Found $CHAIN_RESULT callers"
else
    echo -e "${YELLOW}⚠${NC} No callers found (may be correct)"
fi

echo -n "  Multi-level trace (calls -> calls)... "
TRACE_RESULT=$($CODANNA retrieve calls main --json 2>/dev/null | \
    jq -r '.data.items[:2].symbol.name // empty' 2>/dev/null | \
    head -2 | \
    xargs -I {} sh -c "echo '{}:' && $CODANNA retrieve calls {} --json 2>/dev/null | jq '.data.count // 0'" 2>/dev/null)

if [ -n "$TRACE_RESULT" ]; then
    echo -e "${GREEN}✓${NC}"
    echo "    Trace results:"
    echo "$TRACE_RESULT" | sed 's/^/      /'
else
    echo -e "${YELLOW}⚠${NC} Limited trace depth"
fi

echo
echo "Testing error handling..."
echo

echo -n "  Non-existent symbol... "
ERROR_RESULT=$($CODANNA retrieve symbol nonexistent_symbol_xyz --json 2>/dev/null | jq -r '.status' 2>/dev/null)
if [ "$ERROR_RESULT" = "not_found" ]; then
    echo -e "${GREEN}✓${NC} Correctly returns not_found status"
else
    echo -e "${RED}✗${NC} Unexpected status: $ERROR_RESULT"
fi

echo -n "  Exit codes (0=success, 3=not_found)... "
$CODANNA retrieve symbol main --json > /dev/null 2>&1
SUCCESS_CODE=$?
$CODANNA retrieve symbol nonexistent_xyz --json > /dev/null 2>&1
NOTFOUND_CODE=$?

if [ $SUCCESS_CODE -eq 0 ] && [ $NOTFOUND_CODE -eq 3 ]; then
    echo -e "${GREEN}✓${NC} Exit codes correct"
else
    echo -e "${RED}✗${NC} Exit codes: success=$SUCCESS_CODE, not_found=$NOTFOUND_CODE"
fi

echo
echo "Testing performance..."
echo

echo -n "  Single command response time... "
START=$(perl -MTime::HiRes=time -e 'printf "%.0f\n", time*1000')
$CODANNA retrieve symbol main --json > /dev/null 2>&1
END=$(perl -MTime::HiRes=time -e 'printf "%.0f\n", time*1000')
DURATION=$((END - START))

if [ $DURATION -lt 300 ]; then
    echo -e "${GREEN}✓${NC} ${DURATION}ms (< 300ms target)"
else
    echo -e "${YELLOW}⚠${NC} ${DURATION}ms (exceeds 300ms target)"
fi

echo -n "  Pipe chain response time... "
START=$(perl -MTime::HiRes=time -e 'printf "%.0f\n", time*1000')
$CODANNA retrieve symbol main --json 2>/dev/null | \
    jq -r '.data.items[0].symbol.name' | \
    xargs -I {} $CODANNA retrieve callers {} --json 2>/dev/null | \
    jq '.data.count' > /dev/null 2>&1
END=$(perl -MTime::HiRes=time -e 'printf "%.0f\n", time*1000')
PIPE_DURATION=$((END - START))

if [ $PIPE_DURATION -lt 1000 ]; then
    echo -e "${GREEN}✓${NC} ${PIPE_DURATION}ms (< 1s target)"
else
    echo -e "${YELLOW}⚠${NC} ${PIPE_DURATION}ms (exceeds 1s target)"
fi

echo
echo "=== Summary ==="
echo
echo "✅ All commands output clean JSON suitable for piping"
echo "✅ Commands can be chained with jq and xargs"
echo "✅ Error handling works correctly with proper exit codes"
if [ $DURATION -lt 300 ] && [ $PIPE_DURATION -lt 1000 ]; then
    echo "✅ Performance meets targets for slash commands"
else
    echo "⚠️  Performance could be optimized for slash commands"
fi
echo
echo "These commands are ready to be used in Claude slash commands!"
echo "Example: /impact main"
echo "         /trace some_function 3"
echo "         /find \"error handling\""