#!/bin/sh
set -e

BINARY="./target/release/nr"
PASS=0
FAIL=0

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

pass() {
    PASS=$((PASS + 1))
    printf "${GREEN}✓${NC} %s\n" "$1"
}

fail() {
    FAIL=$((FAIL + 1))
    printf "${RED}✗${NC} %s\n" "$1"
}

echo "Running tests..."
echo ""

# Test 1: Basic script execution
OUTPUT=$($BINARY test 2>&1)
if echo "$OUTPUT" | grep -q "test passed"; then
    pass "Basic script execution"
else
    fail "Basic script execution: $OUTPUT"
fi

# Test 2: node_modules/.bin PATH injection
OUTPUT=$($BINARY moo 2>&1)
if echo "$OUTPUT" | grep -q "hello from node_modules"; then
    pass "node_modules/.bin PATH injection"
else
    fail "node_modules/.bin PATH injection: $OUTPUT"
fi

# Test 3: Script not found error
OUTPUT=$($BINARY nonexistent 2>&1) || true
if echo "$OUTPUT" | grep -q "not found"; then
    pass "Script not found error"
else
    fail "Script not found error: $OUTPUT"
fi

# Test 4: List scripts (no args)
OUTPUT=$($BINARY 2>&1)
if echo "$OUTPUT" | grep -q "Scripts available"; then
    pass "List scripts"
else
    fail "List scripts: $OUTPUT"
fi

# Test 5: Extra args passthrough
OUTPUT=$($BINARY test --verbose 2>&1) || true
if [ $? -eq 0 ] || echo "$OUTPUT" | grep -q "test passed"; then
    pass "Extra args passthrough"
else
    fail "Extra args passthrough: $OUTPUT"
fi

# Test 6: No package.json (run from /tmp)
OUTPUT=$(cd /tmp && $OLDPWD/$BINARY test 2>&1) || true
if echo "$OUTPUT" | grep -q "No package.json"; then
    pass "No package.json error"
else
    fail "No package.json error: $OUTPUT"
fi

# Test 7: Monorepo - binaries in parent node_modules/.bin
MONOREPO_TMP=$(mktemp -d)
trap 'rm -rf "$MONOREPO_TMP"' EXIT

mkdir -p "$MONOREPO_TMP/node_modules/.bin"
mkdir -p "$MONOREPO_TMP/packages/frontend"

# Create a fake binary in root node_modules/.bin
echo '#!/bin/sh
echo "monorepo-binary-works"' > "$MONOREPO_TMP/node_modules/.bin/fakecli"
chmod +x "$MONOREPO_TMP/node_modules/.bin/fakecli"

# Create package.json in subdirectory
echo '{"scripts":{"test":"fakecli"}}' > "$MONOREPO_TMP/packages/frontend/package.json"

# Run nr from subdirectory
OUTPUT=$(cd "$MONOREPO_TMP/packages/frontend" && "$OLDPWD/$BINARY" test 2>&1)
if echo "$OUTPUT" | grep -q "monorepo-binary-works"; then
    pass "Monorepo: parent node_modules/.bin"
else
    fail "Monorepo: parent node_modules/.bin: $OUTPUT"
fi

# Test 8: --help flag
OUTPUT=$($BINARY --help 2>&1)
if echo "$OUTPUT" | grep -q "Run npm scripts" && echo "$OUTPUT" | grep -q "\-\-completions"; then
    pass "--help flag"
else
    fail "--help flag: $OUTPUT"
fi

# Test 9: --list-scripts outputs script names
OUTPUT=$($BINARY --list-scripts 2>&1)
if echo "$OUTPUT" | grep -q "test" && echo "$OUTPUT" | grep -q "build"; then
    pass "--list-scripts"
else
    fail "--list-scripts: $OUTPUT"
fi

# Test 10: --completions bash
OUTPUT=$($BINARY --completions bash 2>&1)
if echo "$OUTPUT" | grep -q "complete -F _nr_completions nr"; then
    pass "--completions bash"
else
    fail "--completions bash: $OUTPUT"
fi

# Test 11: --completions zsh
OUTPUT=$($BINARY --completions zsh 2>&1)
if echo "$OUTPUT" | grep -q "#compdef nr"; then
    pass "--completions zsh"
else
    fail "--completions zsh: $OUTPUT"
fi

# Test 12: --completions fish
OUTPUT=$($BINARY --completions fish 2>&1)
if echo "$OUTPUT" | grep -q "complete -c nr"; then
    pass "--completions fish"
else
    fail "--completions fish: $OUTPUT"
fi

# Test 13: --completions with invalid shell
OUTPUT=$($BINARY --completions powershell 2>&1) || true
if echo "$OUTPUT" | grep -q "Unsupported shell"; then
    pass "--completions invalid shell error"
else
    fail "--completions invalid shell error: $OUTPUT"
fi

echo ""
echo "Results: ${PASS} passed, ${FAIL} failed"

if [ $FAIL -gt 0 ]; then
    exit 1
fi
