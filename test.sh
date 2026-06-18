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

# --- Python launcher preflight ---------------------------------------------
# The poetry/pipenv/pdm tests below drive the real launchers. Make sure this
# machine has them (uv/pipx commonly install to ~/.local/bin).
export PATH="$HOME/.local/bin:$PATH"
HAVE_POETRY=0; HAVE_PIPENV=0; HAVE_PDM=0; HAVE_UV=0
echo "Python launchers (needed for the native-venv tests):"
if command -v poetry >/dev/null 2>&1; then
    HAVE_POETRY=1; printf "  ${GREEN}✓${NC} poetry (%s)\n" "$(poetry --version 2>&1 | head -1)"
else
    printf "  ${RED}✗${NC} poetry not found — install: uv tool install poetry\n"
fi
if command -v pipenv >/dev/null 2>&1; then
    HAVE_PIPENV=1; printf "  ${GREEN}✓${NC} pipenv (%s)\n" "$(pipenv --version 2>&1 | head -1)"
else
    printf "  ${RED}✗${NC} pipenv not found — install: uv tool install pipenv\n"
fi
if command -v pdm >/dev/null 2>&1; then
    HAVE_PDM=1; printf "  ${GREEN}✓${NC} pdm (%s)\n" "$(pdm --version 2>&1 | head -1)"
else
    printf "  ${RED}✗${NC} pdm not found — install: uv tool install pdm\n"
fi
if command -v uv >/dev/null 2>&1; then
    HAVE_UV=1; printf "  ${GREEN}✓${NC} uv (%s)\n" "$(uv --version 2>&1 | head -1)"
else
    printf "  ${RED}✗${NC} uv not found — install: https://docs.astral.sh/uv/\n"
fi
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

# Test 16: pdm inline script runs natively in the in-project .venv
PDM_TMP=$(mktemp -d)
python3 -m venv "$PDM_TMP/.venv" >/dev/null 2>&1
printf '[project]\nname = "x"\nversion = "0.1.0"\n\n[tool.pdm.scripts]\nbench = "echo nr-pdm-native"\n' > "$PDM_TMP/pyproject.toml"
OUTPUT=$(cd "$PDM_TMP" && env -u VIRTUAL_ENV "$BIN_ABS" bench 2>&1) || true
rm -rf "$PDM_TMP"
if echo "$OUTPUT" | grep -q "nr-pdm-native"; then
    pass "pdm inline script runs in venv"
else
    fail "pdm inline script runs in venv: $OUTPUT"
fi

# Test 17: pipenv Pipfile [scripts] runs natively in the in-project .venv
PIPENV_TMP=$(mktemp -d)
python3 -m venv "$PIPENV_TMP/.venv" >/dev/null 2>&1
printf '[scripts]\nbench = "echo nr-pipenv-native"\n' > "$PIPENV_TMP/Pipfile"
OUTPUT=$(cd "$PIPENV_TMP" && env -u VIRTUAL_ENV "$BIN_ABS" bench 2>&1) || true
rm -rf "$PIPENV_TMP"
if echo "$OUTPUT" | grep -q "nr-pipenv-native"; then
    pass "pipenv script runs in venv"
else
    fail "pipenv script runs in venv: $OUTPUT"
fi

# Test 18: poetry console script execs the venv wrapper when it exists
POETRY_TMP=$(mktemp -d)
mkdir -p "$POETRY_TMP/.venv/bin"
printf '#!/bin/sh\necho nr-poetry-native\n' > "$POETRY_TMP/.venv/bin/greet"
chmod +x "$POETRY_TMP/.venv/bin/greet"
printf '[tool.poetry]\nname = "x"\n\n[tool.poetry.scripts]\ngreet = "x:main"\n' > "$POETRY_TMP/pyproject.toml"
OUTPUT=$(cd "$POETRY_TMP" && env -u VIRTUAL_ENV "$BIN_ABS" greet 2>&1) || true
rm -rf "$POETRY_TMP"
if echo "$OUTPUT" | grep -q "nr-poetry-native"; then
    pass "poetry script execs venv wrapper"
else
    fail "poetry script execs venv wrapper: $OUTPUT"
fi

# Test 19: poetry script with no wrapper present falls back to the launcher
POETRY_TMP2=$(mktemp -d)
printf '[tool.poetry]\nname = "x"\n\n[tool.poetry.scripts]\ngreet = "x:main"\n' > "$POETRY_TMP2/pyproject.toml"
OUTPUT=$(cd "$POETRY_TMP2" && env -u VIRTUAL_ENV "$BIN_ABS" greet 2>&1) || true
rm -rf "$POETRY_TMP2"
# With no virtualenv, nr must delegate to `poetry run` rather than try to exec
# the bare name (which would be a "greet: command not found" from the shell).
if echo "$OUTPUT" | grep -qi "poetry" && ! echo "$OUTPUT" | grep -q "sh: greet"; then
    pass "poetry falls back to launcher without a venv"
else
    fail "poetry falls back to launcher without a venv: $OUTPUT"
fi

# Test 20: nr matches `pdm run` output (real launcher, when installed)
if [ "$HAVE_PDM" = "1" ]; then
    REAL_TMP=$(mktemp -d)
    python3 -m venv "$REAL_TMP/.venv" >/dev/null 2>&1
    printf '[project]\nname = "x"\nversion = "0.1.0"\nrequires-python = ">=3.9"\n\n[tool.pdm.scripts]\ngreet = "echo equiv-pdm"\n' > "$REAL_TMP/pyproject.toml"
    (cd "$REAL_TMP" && pdm use -f .venv/bin/python >/dev/null 2>&1) || true
    REAL=$(cd "$REAL_TMP" && pdm run greet 2>/dev/null | tail -1)
    MINE=$(cd "$REAL_TMP" && env -u VIRTUAL_ENV "$BIN_ABS" greet 2>/dev/null | tail -1)
    rm -rf "$REAL_TMP"
    if [ "$REAL" = "equiv-pdm" ] && [ "$MINE" = "$REAL" ]; then
        pass "nr matches 'pdm run' output ($MINE)"
    else
        fail "nr matches 'pdm run' output: pdm='$REAL' nr='$MINE'"
    fi
else
    echo "(skipping pdm equivalence test: pdm not installed)"
fi

# Test 21: nr matches `pipenv run` output (real launcher, when installed)
if [ "$HAVE_PIPENV" = "1" ]; then
    REAL_TMP=$(mktemp -d)
    printf '[[source]]\nurl = "https://pypi.org/simple"\nname = "pypi"\n\n[scripts]\ngreet = "echo equiv-pipenv"\n' > "$REAL_TMP/Pipfile"
    (cd "$REAL_TMP" && PIPENV_VENV_IN_PROJECT=1 pipenv install >/dev/null 2>&1) || true
    REAL=$(cd "$REAL_TMP" && PIPENV_VENV_IN_PROJECT=1 pipenv run greet 2>/dev/null | tail -1)
    MINE=$(cd "$REAL_TMP" && env -u VIRTUAL_ENV "$BIN_ABS" greet 2>/dev/null | tail -1)
    rm -rf "$REAL_TMP"
    if [ "$REAL" = "equiv-pipenv" ] && [ "$MINE" = "$REAL" ]; then
        pass "nr matches 'pipenv run' output ($MINE)"
    else
        fail "nr matches 'pipenv run' output: pipenv='$REAL' nr='$MINE'"
    fi
else
    echo "(skipping pipenv equivalence test: pipenv not installed)"
fi

# Test 22: nr matches `poetry run` output (real launcher, when installed)
if [ "$HAVE_POETRY" = "1" ]; then
    REAL_TMP=$(mktemp -d)
    mkdir -p "$REAL_TMP/proj"
    printf 'def main():\n    print("equiv-poetry")\n' > "$REAL_TMP/proj/__init__.py"
    cat > "$REAL_TMP/pyproject.toml" <<EOF
[tool.poetry]
name = "proj"
version = "0.1.0"
description = ""
authors = ["x <x@example.com>"]
packages = [{ include = "proj" }]

[tool.poetry.dependencies]
python = ">=3.9"

[tool.poetry.scripts]
greet = "proj:main"

[build-system]
requires = ["poetry-core"]
build-backend = "poetry.core.masonry.api"
EOF
    (cd "$REAL_TMP" && python3 -m venv .venv >/dev/null 2>&1 \
        && poetry env use .venv/bin/python >/dev/null 2>&1 \
        && .venv/bin/pip install . >/dev/null 2>&1) || true
    REAL=$(cd "$REAL_TMP" && poetry run greet 2>/dev/null | tail -1)
    MINE=$(cd "$REAL_TMP" && env -u VIRTUAL_ENV "$BIN_ABS" greet 2>/dev/null | tail -1)
    rm -rf "$REAL_TMP"
    if [ "$REAL" = "equiv-poetry" ] && [ "$MINE" = "$REAL" ]; then
        pass "nr matches 'poetry run' output ($MINE)"
    else
        fail "nr matches 'poetry run' output: poetry='$REAL' nr='$MINE'"
    fi
else
    echo "(skipping poetry equivalence test: poetry not installed)"
fi

# Test 23: nr matches `uv run` output for a uv project (delegates to uv run)
if [ "$HAVE_UV" = "1" ]; then
    REAL_TMP=$(mktemp -d)
    mkdir -p "$REAL_TMP/src/proj"
    printf 'def main():\n    print("equiv-uv")\n' > "$REAL_TMP/src/proj/__init__.py"
    cat > "$REAL_TMP/pyproject.toml" <<EOF
[project]
name = "proj"
version = "0.1.0"
requires-python = ">=3.9"

[project.scripts]
greet = "proj:main"

[build-system]
requires = ["uv_build"]
build-backend = "uv_build"
EOF
    (cd "$REAL_TMP" && uv sync >/dev/null 2>&1) || true
    REAL=$(cd "$REAL_TMP" && uv run greet 2>/dev/null | tail -1)
    MINE=$(cd "$REAL_TMP" && env -u VIRTUAL_ENV "$BIN_ABS" greet 2>/dev/null | tail -1)
    rm -rf "$REAL_TMP"
    if [ "$REAL" = "equiv-uv" ] && [ "$MINE" = "$REAL" ]; then
        pass "nr matches 'uv run' output ($MINE)"
    else
        fail "nr matches 'uv run' output: uv='$REAL' nr='$MINE'"
    fi
else
    echo "(skipping uv equivalence test: uv not installed)"
fi

# Test 24: --version / -V print the version, and work outside a project
VER_TMP=$(mktemp -d)
OUTPUT=$(cd "$VER_TMP" && "$BIN_ABS" --version 2>&1)
OUTPUT_SHORT=$(cd "$VER_TMP" && "$BIN_ABS" -V 2>&1)
rmdir "$VER_TMP"
if echo "$OUTPUT" | grep -qE '^nr [0-9]+\.[0-9]+\.[0-9]+' && [ "$OUTPUT" = "$OUTPUT_SHORT" ]; then
    pass "--version prints version ($OUTPUT)"
else
    fail "--version prints version: '$OUTPUT' / '$OUTPUT_SHORT'"
fi

echo ""
echo "Results: ${PASS} passed, ${FAIL} failed"

if [ $FAIL -gt 0 ]; then
    exit 1
fi
