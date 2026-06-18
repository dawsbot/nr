#!/bin/sh
# Benchmark `nr` against the Python task launchers it can replace natively:
# poetry, pipenv and pdm. Each of these boots a Python interpreter on every
# `run`; `nr` resolves the project virtualenv and execs the task directly.
#
# We measure a trivial task (so the figure is launcher overhead, the thing nr
# removes) and verify nr produces identical output before timing.
#
# Not `set -e`: these external tools emit warnings and the odd non-zero exit we
# want to tolerate per-tool rather than abort the whole run.
set -u

# uv/pipx install CLI tools here; make sure they're reachable.
export PATH="$HOME/.local/bin:$PATH"

NR="$(cd "$(dirname "$0")/.." && pwd)/target/release/nr"

red()   { printf '\033[0;31m%s\033[0m\n' "$1"; }
green() { printf '\033[0;32m%s\033[0m\n' "$1"; }
dim()   { printf '\033[2m%s\033[0m\n' "$1"; }

# --- preflight: the runners under test must be installed -------------------
echo "Checking for the script runners this benchmark needs..."
missing=0
check() { # name install-hint
  if command -v "$1" >/dev/null 2>&1; then
    green "  ok   $1 ($("$1" --version 2>&1 | head -1))"
  else
    red   "  MISS $1 — install with: $2"
    missing=1
  fi
}
check python3  "your OS package manager"
check hyperfine "brew install hyperfine"
check poetry   "uv tool install poetry  (or: pipx install poetry)"
check pipenv   "uv tool install pipenv  (or: pipx install pipenv)"
check pdm      "uv tool install pdm     (or: pipx install pdm)"
if [ "$missing" -ne 0 ]; then
  red "Install the missing runner(s) above, then re-run this benchmark."
  exit 1
fi

if [ ! -x "$NR" ]; then
  echo "Building nr (release)..."
  (cd "$(dirname "$NR")/.." && cargo build --release >/dev/null)
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
RESULTS="$WORK/results.txt"
: > "$RESULTS"

# Run hyperfine for `<launcher> run bench` vs `nr bench`, record the medians.
bench() { # label  launcher-cmd
  label="$1"; launcher="$2"
  json="$WORK/$label.json"
  hyperfine --warmup 5 -N -i --export-json "$json" \
    -n "$launcher" "$launcher" \
    -n "nr bench" "$NR bench" >/dev/null 2>&1
  if [ ! -f "$json" ]; then
    red "  benchmark for '$launcher' failed; skipping"
    return
  fi
  python3 - "$json" "$label" >> "$RESULTS" <<'PY'
import json, sys
data = json.load(open(sys.argv[1]))["results"]
launcher = data[0]["mean"] * 1000
nr = data[1]["mean"] * 1000
print(f"{sys.argv[2]}\t{launcher:.0f}\t{nr:.1f}\t{launcher/nr:.0f}")
PY
}

# --- pdm -------------------------------------------------------------------
dim "Setting up pdm project..."
P="$WORK/pdm"; mkdir -p "$P"; cd "$P"
python3 -m venv .venv
cat > pyproject.toml <<'EOF'
[project]
name = "bench"
version = "0.1.0"
requires-python = ">=3.9"

[tool.pdm.scripts]
bench = "true"
EOF
pdm use -f .venv/bin/python >/dev/null 2>&1
bench "pdm run" "pdm run bench"

# --- pipenv ----------------------------------------------------------------
dim "Setting up pipenv project..."
P="$WORK/pipenv"; mkdir -p "$P"; cd "$P"
cat > Pipfile <<'EOF'
[[source]]
url = "https://pypi.org/simple"
name = "pypi"

[scripts]
bench = "true"
EOF
export PIPENV_VENV_IN_PROJECT=1
pipenv install >/dev/null 2>&1
bench "pipenv run" "pipenv run bench"
unset PIPENV_VENV_IN_PROJECT

# --- poetry ----------------------------------------------------------------
dim "Setting up poetry project (installs a console script)..."
P="$WORK/poetry"; mkdir -p "$P"; cd "$P"
mkdir -p bench
printf 'def main():\n    pass\n' > bench/__init__.py
cat > pyproject.toml <<'EOF'
[tool.poetry]
name = "bench"
version = "0.1.0"
description = ""
authors = ["x <x@example.com>"]
packages = [{ include = "bench" }]

[tool.poetry.dependencies]
python = ">=3.9"

[tool.poetry.scripts]
bench = "bench:main"

[build-system]
requires = ["poetry-core"]
build-backend = "poetry.core.masonry.api"
EOF
python3 -m venv .venv
poetry env use .venv/bin/python >/dev/null 2>&1
.venv/bin/pip install . >/dev/null 2>&1   # materialize the console-script wrapper
bench "poetry run" "poetry run bench"

# --- report ----------------------------------------------------------------
echo
echo "| Launcher    | Launcher time | nr      | Speedup |"
echo "|-------------|---------------|---------|---------|"
while IFS="$(printf '\t')" read -r label launcher nr speedup; do
  printf "| %-11s | %10s ms | %4s ms | %5sx |\n" "$label" "$launcher" "$nr" "$speedup"
done < "$RESULTS"
echo
dim "Trivial task; figure is launcher startup overhead. poetry runs a Python"
dim "console script, so nr still pays the interpreter startup the script needs."
