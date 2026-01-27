#!/usr/bin/env bash
set -e

# Benchmark script for nr
# Updates README.md with fresh benchmark results
# Works on macOS, Linux, and Windows (via Git Bash/WSL)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
README="$ROOT_DIR/README.md"
ITERATIONS=50

# Detect OS and version
detect_system() {
    case "$(uname -s)" in
        Darwin)
            OS="macOS"
            VERSION=$(sw_vers -productVersion 2>/dev/null || echo "unknown")
            CHIP=$(uname -m)
            if [[ "$CHIP" == "arm64" ]]; then
                CHIP="Apple Silicon"
            else
                CHIP="Intel"
            fi
            echo "$OS $VERSION ($CHIP)"
            ;;
        Linux)
            OS="Linux"
            if [ -f /etc/os-release ]; then
                . /etc/os-release
                VERSION="$NAME $VERSION_ID"
            else
                VERSION=$(uname -r)
            fi
            echo "$OS - $VERSION"
            ;;
        MINGW*|MSYS*|CYGWIN*)
            OS="Windows"
            VERSION=$(cmd.exe /c ver 2>/dev/null | tr -d '\r' | tail -1 || echo "unknown")
            echo "$OS $VERSION"
            ;;
        *)
            echo "Unknown OS ($(uname -s))"
            ;;
    esac
}

# Benchmark a command, return average time in ms
benchmark() {
    local cmd="$1"
    local total=0

    for ((i=1; i<=ITERATIONS; i++)); do
        # Time the command, capture only the real time in ms
        local start=$(perl -MTime::HiRes=time -e 'printf "%.0f", time * 1000')
        eval "$cmd" > /dev/null 2>&1 || true
        local end=$(perl -MTime::HiRes=time -e 'printf "%.0f", time * 1000')
        local elapsed=$((end - start))
        total=$((total + elapsed))
    done

    echo $((total / ITERATIONS))
}

# Check if command exists
has_cmd() {
    command -v "$1" > /dev/null 2>&1
}

echo "Benchmarking nr..."
echo "System: $(detect_system)"
echo "Iterations per runner: $ITERATIONS"
echo ""

# Ensure nr is built
if [ ! -f "$ROOT_DIR/target/release/nr" ]; then
    echo "Building nr..."
    cargo build --release --manifest-path="$ROOT_DIR/Cargo.toml"
fi

# Run benchmarks
declare -A times
declare -a runners=()

# Always benchmark nr
echo -n "  nr: "
times[nr]=$(benchmark "$ROOT_DIR/target/release/nr test")
echo "${times[nr]}ms"
runners+=("nr")

# Benchmark available runners
if has_cmd npm; then
    echo -n "  npm: "
    times[npm]=$(benchmark "npm run test --silent")
    echo "${times[npm]}ms"
    runners+=("npm")
fi

if has_cmd bun; then
    echo -n "  bun: "
    times[bun]=$(benchmark "bun run --silent test")
    echo "${times[bun]}ms"
    runners+=("bun")
fi

if has_cmd yarn; then
    echo -n "  yarn: "
    times[yarn]=$(benchmark "yarn run --silent test")
    echo "${times[yarn]}ms"
    runners+=("yarn")
fi

if has_cmd pnpm; then
    echo -n "  pnpm: "
    times[pnpm]=$(benchmark "pnpm run --silent test")
    echo "${times[pnpm]}ms"
    runners+=("pnpm")
fi

echo ""

# Find the slowest (baseline)
baseline=${times[npm]:-${times[nr]}}
for runner in "${runners[@]}"; do
    if (( times[$runner] > baseline )); then
        baseline=${times[$runner]}
    fi
done

# Generate markdown table sorted by speed
generate_table() {
    echo "| Runner | Time | Speedup |"
    echo "|--------|------|---------|"

    # Sort runners by time
    for runner in $(for r in "${runners[@]}"; do echo "${times[$r]} $r"; done | sort -n | awk '{print $2}'); do
        local time=${times[$runner]}
        local speedup
        if (( time > 0 )); then
            speedup=$(awk "BEGIN {printf \"%.1f\", $baseline / $time}")
        else
            speedup="∞"
        fi

        # Bold the speedup for the fastest
        if [[ "$runner" == "nr" ]]; then
            echo "| $runner | ${time}ms | **${speedup}x** |"
        else
            echo "| $runner | ${time}ms | ${speedup}x |"
        fi
    done
}

SYSTEM_INFO=$(detect_system)
TABLE=$(generate_table)

# Create the new benchmark section
BENCHMARK_CONTENT="| Runner | Time | Speedup |
|--------|------|---------|
$(generate_table | tail -n +3)

*Measured running \`echo test\` on $SYSTEM_INFO. Your mileage may vary.*"

# Update README between markers
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS sed
    sed -i '' '/<!-- BENCHMARK_START -->/,/<!-- BENCHMARK_END -->/c\
<!-- BENCHMARK_START -->\
'"$(echo "$BENCHMARK_CONTENT" | sed 's/$/\\/' | sed '$ s/\\$//')"'\
<!-- BENCHMARK_END -->' "$README"
else
    # GNU sed (Linux, Git Bash)
    sed -i '/<!-- BENCHMARK_START -->/,/<!-- BENCHMARK_END -->/c\<!-- BENCHMARK_START -->\n'"$(echo "$BENCHMARK_CONTENT" | sed ':a;N;$!ba;s/\n/\\n/g')"'\n<!-- BENCHMARK_END -->' "$README"
fi

echo "Updated $README with benchmark results"
