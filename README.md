# nr

Run npm scripts. <!-- FASTEST_SPEEDUP_START -->30<!-- FASTEST_SPEEDUP_END -->x faster.

A zero-overhead npm script runner written in Rust. No Node.js startup, no npm overhead—just your script.

## Benchmarks

<!-- BENCHMARK_START -->
| Runner | Time | Speedup |
|--------|------|---------|
| nr | 9ms | **29.6x** |
| bun | 10ms | 26.6x |
| npm | 132ms | 2.0x |
| yarn | 141ms | 1.9x |
| pnpm | 266ms | 1.0x |

*Measured running `echo test` on macOS 26.2 (Apple Silicon). Your mileage may vary.*
<!-- BENCHMARK_END -->

## Install

### Homebrew (macOS/Linux)

```bash
brew install dawsbot/tap/nr
```

### Cargo (cross-platform)

```bash
cargo install nr-run
```

### Shell (macOS/Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/dawsbot/nr/main/install.sh | bash
```

### Manual

Download the latest binary from [Releases](https://github.com/dawsbot/nr/releases), extract, and add to your PATH.

## Usage

```bash
# List available scripts
nr

# Run a script
nr build

# Pass arguments to the script
nr test -- --watch
```

## Why is it faster?

**No Node.js startup.** npm, yarn, and pnpm all bootstrap Node.js before doing anything. That's 50-100ms before your script even starts. `nr` is a native binary—it starts instantly.

**Direct exec.** On Unix systems, `nr` uses the `exec()` syscall to replace itself with your command. No fork, no wait, no overhead. Your script runs in the same process slot.

**Minimal parsing.** We only read the `scripts` field from package.json. Not dependencies, not lockfiles, not node_modules. Just scripts.

**Tiny binary.** 377KB. No runtime, no GC, no framework. Just machine code.

## What it doesn't do

`nr` is intentionally minimal:

- No `pre`/`post` lifecycle scripts (use `nr pretest && nr test` if needed)
- No `node_modules/.bin` PATH injection (your script should reference binaries explicitly or use npx)
- No workspaces support
- No colorized npm-style banners

If you need these features, use npm. If you want speed, use `nr`.

## Build from source

```bash
git clone https://github.com/dawsbot/nr
cd nr
cargo build --release
cp target/release/nr /usr/local/bin/
```

## Status

Experimental. Works on my machine. Report issues at [GitHub](https://github.com/dawsbot/nr/issues).

## License

MIT
