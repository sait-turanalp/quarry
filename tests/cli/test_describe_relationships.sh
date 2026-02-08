#!/bin/bash
# Test that retrieve describe properly shows relationships for different symbol types

echo "=== Testing Retrieve Describe Relationships ==="
echo

CODANNA="./target/release/codanna"

# Test 1: Struct should show its methods
echo "Test 1: Struct (OutputManager) should show methods..."
METHODS=$($CODANNA retrieve describe OutputManager --json | jq '.item.relationships.defines | length')
if [ "$METHODS" != "null" ] && [ "$METHODS" -gt 0 ]; then
    echo "✓ Found $METHODS methods"
    $CODANNA retrieve describe OutputManager --json | jq '.item.relationships.defines[:3] | map(.name)'
else
    echo "✗ No methods found (expected: new, unified, etc.)"
fi
echo

# Test 2: Function should show callers and calls
echo "Test 2: Function (index_file) should show callers/calls..."
CALLERS=$($CODANNA retrieve describe index_file --json | jq '.item.relationships.called_by | length')
CALLS=$($CODANNA retrieve describe index_file --json | jq '.item.relationships.calls | length')
echo "  Callers: $CALLERS"
echo "  Calls: $CALLS"
if [ "$CALLS" != "null" ] && [ "$CALLS" != "0" ]; then
    echo "✓ Has calls relationships"
else
    echo "✗ No calls found"
fi
echo

# Test 3: Method should show what it calls
echo "Test 3: Method (new) should show relationships..."
$CODANNA retrieve describe new --json | jq '{
    called_by: (.item.relationships.called_by | length),
    calls: (.item.relationships.calls | length)
}'
echo

# Test 4: Trait should show implementations
echo "Test 4: Trait should show implementations..."
# Search for a known trait (LanguageBehavior)
TRAIT=$($CODANNA retrieve search "LanguageBehavior" --limit 5 --json | jq -r '.items | map(select(.symbol.kind == "Trait")) | .[0].symbol.name')
if [ -n "$TRAIT" ] && [ "$TRAIT" != "null" ]; then
    echo "  Testing trait: $TRAIT"
    IMPLS=$($CODANNA retrieve describe "$TRAIT" --json | jq '.item.relationships.implemented_by | length')
    echo "  Implementations: $IMPLS"
else
    echo "  No trait found in search results"
fi
echo

echo "=== Summary ==="
echo "Expected behavior:"
echo "- Structs show their methods in 'defines'"
echo "- Functions/Methods show 'called_by' and 'calls'"
echo "- Traits show 'implemented_by'"
echo "- All relationships should be populated when applicable"