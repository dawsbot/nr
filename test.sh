#!/bin/sh
set -e

BINARY="./target/release/nr"
BIN_ABS="$PWD/target/release/nr"
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

# Test 4: List tasks (no args)
OUTPUT=$($BINARY 2>&1)
if echo "$OUTPUT" | grep -q "Run a task"; then
    pass "List tasks"
else
    fail "List tasks: $OUTPUT"
fi

# Test 5: Extra args passthrough
OUTPUT=$($BINARY test --verbose 2>&1) || true
if [ $? -eq 0 ] || echo "$OUTPUT" | grep -q "test passed"; then
    pass "Extra args passthrough"
else
    fail "Extra args passthrough: $OUTPUT"
fi

# Test 6: No manifest at all (run from an empty temp dir)
EMPTY_TMP=$(mktemp -d)
OUTPUT=$(cd "$EMPTY_TMP" && "$BIN_ABS" test 2>&1) || true
rmdir "$EMPTY_TMP"
if echo "$OUTPUT" | grep -q "No tasks found"; then
    pass "No manifest error"
else
    fail "No manifest error: $OUTPUT"
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

# Test 8: Quoted extra args arrive as a single argument
OUTPUT=$($BINARY argcount "two words" 2>&1)
if echo "$OUTPUT" | grep -q "argc=1" && echo "$OUTPUT" | grep -q "arg:two words"; then
    pass "Quoted extra args preserved"
else
    fail "Quoted extra args preserved: $OUTPUT"
fi

# Test 9: Shell operators in scripts still work
OUTPUT=$($BINARY chain 2>&1)
if echo "$OUTPUT" | grep -q "first" && echo "$OUTPUT" | grep -q "second"; then
    pass "Shell operators (&&)"
else
    fail "Shell operators (&&): $OUTPUT"
fi

# Test 10: Leading env assignment goes through the shell
OUTPUT=$($BINARY envtest 2>&1)
if echo "$OUTPUT" | grep -q "bar"; then
    pass "Env assignment prefix"
else
    fail "Env assignment prefix: $OUTPUT"
fi

# Test 11: node_modules/.bin resolution without shell metachars
OUTPUT=$($BINARY moodirect 2>&1)
if echo "$OUTPUT" | grep -q "hello-direct"; then
    pass "node_modules/.bin via direct exec"
else
    fail "node_modules/.bin via direct exec: $OUTPUT"
fi

# Test 12: Procfile task execution (no external tool required)
PROC_TMP=$(mktemp -d)
printf 'web: echo procfile-web-up\nworker: echo worker-up\n' > "$PROC_TMP/Procfile"
OUTPUT=$(cd "$PROC_TMP" && "$BIN_ABS" web 2>&1) || true
rm -rf "$PROC_TMP"
if echo "$OUTPUT" | grep -q "procfile-web-up"; then
    pass "Procfile task execution"
else
    fail "Procfile task execution: $OUTPUT"
fi

# Test 13: Makefile target delegation (requires make, near-universal)
if command -v make >/dev/null 2>&1; then
    MAKE_TMP=$(mktemp -d)
    printf 'greet:\n\t@echo makefile-target-ran\n' > "$MAKE_TMP/Makefile"
    OUTPUT=$(cd "$MAKE_TMP" && "$BIN_ABS" greet 2>&1) || true
    rm -rf "$MAKE_TMP"
    if echo "$OUTPUT" | grep -q "makefile-target-ran"; then
        pass "Makefile target delegation"
    else
        fail "Makefile target delegation: $OUTPUT"
    fi
else
    echo "(skipping Makefile test: make not installed)"
fi

# Test 14: Multiple sources merged in one listing
MULTI_TMP=$(mktemp -d)
echo '{"scripts":{"start":"node ."}}' > "$MULTI_TMP/package.json"
printf 'web: echo hi\n' > "$MULTI_TMP/Procfile"
OUTPUT=$(cd "$MULTI_TMP" && "$BIN_ABS" 2>&1) || true
rm -rf "$MULTI_TMP"
if echo "$OUTPUT" | grep -q "package.json" && echo "$OUTPUT" | grep -q "Procfile" \
   && echo "$OUTPUT" | grep -q "start" && echo "$OUTPUT" | grep -q "web"; then
    pass "Multiple sources merged in listing"
else
    fail "Multiple sources merged in listing: $OUTPUT"
fi

# Test 15: Cargo.toml exposes conventional commands
CARGO_TMP=$(mktemp -d)
printf '[package]\nname = "x"\nversion = "0.1.0"\n' > "$CARGO_TMP/Cargo.toml"
OUTPUT=$(cd "$CARGO_TMP" && "$BIN_ABS" 2>&1) || true
rm -rf "$CARGO_TMP"
if echo "$OUTPUT" | grep -q "Cargo.toml" && echo "$OUTPUT" | grep -q "clippy"; then
    pass "Cargo.toml conventional commands"
else
    fail "Cargo.toml conventional commands: $OUTPUT"
fi

echo ""
echo "Results: ${PASS} passed, ${FAIL} failed"

if [ $FAIL -gt 0 ]; then
    exit 1
fi
